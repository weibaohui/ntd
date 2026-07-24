use axum::Router;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use serde::Deserialize;

use crate::adapters::parse_executor_type;
use crate::executor_service::{
    run_todo_execution, run_todo_execution_with_params, ExecutionResult, RunTodoExecutionRequest,
};
use crate::handlers::{workspace_guard, ApiJson, AppError, AppState};
use crate::models::{
    ApiResponse, DashboardStats, ExecuteRequest, ExecutionLogsPage, ExecutionRecordsPage,
    ExecutionStatus, ExecutionSummary, SmartCreateRequest, TodoIdQuery,
};

/// 统一启动一条 Todo 执行，供手动执行、消息路由等入口复用。
pub async fn start_todo_execution(
    request: RunTodoExecutionRequest,
) -> Result<ExecutionResult, AppError> {
    let result = if request.params.is_some() {
        run_todo_execution_with_params(request).await
    } else {
        run_todo_execution(request).await
    };

    if result.record_id.is_none() {
        return Err(AppError::Internal("Failed to start execution".to_string()));
    }

    Ok(result)
}

#[allow(dead_code)]
pub async fn get_execution_records(
    State(state): State<AppState>,
    Query(query): Query<TodoIdQuery>,
) -> Result<ApiResponse<ExecutionRecordsPage>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(10).clamp(1, 100);
    let offset = (page - 1) * limit;
    let status = match query.status.as_deref() {
        Some("all") | None => None,
        Some(s) => Some(
            s.parse::<ExecutionStatus>()
                .map(|v| v.as_str())
                .map_err(AppError::BadRequest)?
        ),
    };
    let (records, total) = state
        .db
        .get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: query.todo_id,
            step_id: query.step_id,
            limit,
            offset,
            status,
            hours: query.hours,
        })
        .await?;

    // 当提供了 todo_id 或 step_id 时，workspace_id 是隐含的（由 todo 决定）。
    // 仅在 todo_id/step_id 都为空且 workspace_id 有值时过滤——目前没有调用方
    // 走这个分支，但保持接口一致性，传了就能用。
    let records = if let Some(wid) = query.workspace_id {
        if query.todo_id.is_none() && query.step_id.is_none() {
            // ? 传播 DbErr：旧实现 unwrap_or_default() 会把 DB 读失败静默当成空 Vec，
            // 下游 filter 把所有 record 过滤掉、返回 200+0 条，调用方无法区分真空还是 DB 挂了。
            let ws_todos = state.db.get_todos_by_workspace_id(Some(wid)).await?;
            let ws_todo_ids: std::collections::HashSet<i64> = ws_todos.iter().map(|t| t.id).collect();
            records.into_iter().filter(|r| ws_todo_ids.contains(&r.todo_id)).collect()
        } else {
            records
        }
    } else {
        records
    };

    Ok(ApiResponse::ok(ExecutionRecordsPage {
        records,
        total,
        page,
        limit,
    }))
}

pub async fn get_execution_record(
    State(state): State<AppState>,
    Path((ws_id, id)): Path<(i64, i64)>,
) -> Result<ApiResponse<crate::models::ExecutionRecord>, AppError> {
    // V1 隔离：record 经 todo 间接关联 workspace，校验归属防止跨 ws 读他人执行记录
    workspace_guard::verify_execution_belongs_to_ws(&state.db, id, ws_id).await?;
    let record = state
        .db
        .get_execution_record(id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(record))
}

/// 更新执行记录评分。评分仅针对已结束的记录（success/failed）；
/// running 记录不允许评分。`rating: null` 表示清除评分。
#[derive(Debug, Deserialize)]
pub struct RateExecutionRequest {
    /// 评分值（0-100）。传 null 表示清除当前评分。
    pub rating: Option<i32>,
}

pub async fn rate_execution_handler(
    State(state): State<AppState>,
    Path((ws_id, id)): Path<(i64, i64)>,
    ApiJson(req): ApiJson<RateExecutionRequest>,
) -> Result<ApiResponse<crate::models::ExecutionRecord>, AppError> {
    // V1 隔离：评分前校验 record 归属 workspace
    workspace_guard::verify_execution_belongs_to_ws(&state.db, id, ws_id).await?;
    if let Some(r) = req.rating {
        if !(0..=100).contains(&r) {
            return Err(AppError::BadRequest(format!(
                "rating must be in 0..=100, got {}",
                r
            )));
        }
    }

    let record = state
        .db
        .get_execution_record(id)
        .await?
        .ok_or(AppError::NotFound)?;

    if record.status == ExecutionStatus::Running {
        return Err(AppError::BadRequest(
            "Cannot rate a running execution".to_string(),
        ));
    }

    state
        .db
        .update_execution_record_rating(id, req.rating)
        .await?;

    let updated = state
        .db
        .get_execution_record(id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(updated))
}

#[derive(Debug, Deserialize)]
pub struct ExecutionLogsQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

fn default_page() -> i64 { 1 }
fn default_per_page() -> i64 { 200 }

pub async fn get_execution_logs_handler(
    State(state): State<AppState>,
    Path((ws_id, id)): Path<(i64, i64)>,
    Query(query): Query<ExecutionLogsQuery>,
) -> Result<ApiResponse<ExecutionLogsPage>, AppError> {
    // V1 隔离：取日志前校验 record 归属 workspace
    workspace_guard::verify_execution_belongs_to_ws(&state.db, id, ws_id).await?;
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(10, 1000);
    let (logs, total) = state
        .db
        .get_execution_logs(id, page, per_page)
        .await?;
    Ok(ApiResponse::ok(ExecutionLogsPage {
        logs,
        total,
        page,
        per_page,
    }))
}

pub async fn get_execution_records_by_session(
    State(state): State<AppState>,
    Path((ws_id, session_id)): Path<(i64, String)>,
) -> Result<ApiResponse<Vec<crate::models::ExecutionRecord>>, AppError> {
    let records = state
        .db
        .get_execution_records_by_session(&session_id)
        .await?;
    // V1 隔离：同一 session 可能含跨 ws 的记录，按 workspace 过滤只保留本 ws 的
    let ws_todos = state.db.get_todos_by_workspace_id(Some(ws_id)).await?;
    let ws_todo_ids: std::collections::HashSet<i64> =
        ws_todos.iter().map(|t| t.id).collect();
    let records: Vec<_> = records
        .into_iter()
        .filter(|r| ws_todo_ids.contains(&r.todo_id))
        .collect();
    Ok(ApiResponse::ok(records))
}

pub async fn execute_handler(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    ApiJson(req): ApiJson<ExecuteRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    // V1 隔离：启动执行前校验目标 todo 属于路径 workspace
    workspace_guard::verify_todo_belongs_to_ws(&state.db, req.todo_id, ws_id).await?;
    // Get the todo to use its prompt as fallback when message is not provided
    let todo = state
        .db
        .get_todo(req.todo_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("Todo {} not found", req.todo_id)))?;

    // 检查该 todo 下正在执行的记录数量是否已达并发上限
    // 需要过滤掉孤儿记录：状态为 running 但 task_manager 中没有对应 task
    // 中毒时用 into_inner 取旧值继续：默认 unwind 下 axum handler panic 不会重启进程，
    // 若 .unwrap() 会让所有 config 路由级联 500。
    let max_concurrent = state.config.read().unwrap_or_else(|e| e.into_inner()).max_concurrent_todos;
    let running_tasks = state.task_manager.get_all_task_infos().await;
    let running_records = state.db.get_running_records_by_todo_id(req.todo_id).await?;
    let running_count_for_todo = running_records
        .iter()
        .filter(|r| {
            // 排除僵尸记录：状态为 running 但 task_manager 中没有对应 task
            if let Some(task_id) = &r.task_id {
                running_tasks.iter().any(|t| t.task_id == *task_id)
            } else {
                false
            }
        })
        .count();
    if running_count_for_todo >= max_concurrent as usize {
        return Err(AppError::BadRequest(format!(
            "Todo {} has {} execution(s) still running (limit: {}). Please stop them first.",
            req.todo_id, running_count_for_todo, max_concurrent
        )));
    }

    // Build params: --message injects into {{message}} placeholder (only if not already set)
    let mut params = req.params.clone().unwrap_or_default();
    if let Some(ref msg) = req.message {
        let trimmed = msg.trim();
        if !trimmed.is_empty() && !params.contains_key("message") {
            params.insert("message".to_string(), trimmed.to_string());
        }
    }

    // Replace placeholders in todo.prompt using params
    let message = crate::models::replace_placeholders(&todo.prompt, &params);

    let result = start_todo_execution(RunTodoExecutionRequest {
        db: state.db.clone(),
        executor_registry: state.executor_registry.clone(),
        tx: state.tx.clone(),
        task_manager: state.task_manager.clone(),
        config: state.config.clone(),
        todo_id: req.todo_id,
        message,
        req_executor: req.executor,
        req_model: req.model,
        trigger_type: "manual".to_string(),
        params: req.params,
        resume_session_id: None,
        resume_message: None,
        source_todo_id: None,
        source_todo_title: None,
        loop_step_execution_id: None,
        step_id: None,
        feishu_bot_id: None,
        feishu_receive_id: None,
            feishu_receive_id_type: None,
        workspace_path: None,
        // 从 todo 中提取 workspace_id，用于 FeishuPushService 按 workspace 隔离推送
        workspace_id: todo.workspace_id,
        // 手动执行路径：注入专家上下文，让关联了 expert_name 的 todo 加载专家 prompt
        expert_manager: Some(state.expert_manager.clone()),
    })
    .await;
    let result = result?;
    let record_id = result.record_id
        .ok_or_else(|| AppError::Internal("执行启动失败：未获取到执行记录 ID".to_string()))?;

    Ok(ApiResponse::ok(
        serde_json::json!({ "task_id": result.task_id, "record_id": record_id }),
    ))
}

pub async fn stop_execution_handler(
    State(state): State<AppState>,
    Path((ws_id, record_id)): Path<(i64, i64)>,
) -> Result<ApiResponse<()>, AppError> {
    // V1 隔离：停止前校验 record 归属 workspace（路径 {id} 即 record_id）
    workspace_guard::verify_execution_belongs_to_ws(&state.db, record_id, ws_id).await?;
    tracing::info!("Stopping execution record: {}", record_id);

    let record =
        state
            .db
            .get_execution_record(record_id)
            .await?
            .ok_or(AppError::BadRequest(
                "Execution record not found".to_string(),
            ))?;

    if record.status != ExecutionStatus::Running {
        return Err(AppError::BadRequest(
            "Execution record is not running".to_string(),
        ));
    }

    if let Some(task_id) = &record.task_id {
        tracing::info!(
            "Stopping execution record {} with task_id: {}",
            record_id,
            task_id
        );
        let cancelled = state.task_manager.cancel(task_id).await;
        if !cancelled {
            // 任务不在 task_manager 中，可能是已完成清理或已崩溃。
            // 重新查询 DB 确认当前状态，避免与正常完成的任务产生竞态
            let current_record = state
                .db
                .get_execution_record(record_id)
                .await?
                .ok_or(AppError::BadRequest(
                    "Execution record not found".to_string(),
                ))?;
            if current_record.status == ExecutionStatus::Running {
                tracing::warn!(
                    "Task {} was not found in task manager and record is still Running (crashed), forcing DB update to failed",
                    task_id
                );
                state
                    .db
                    .force_fail_execution_record(record_id)
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?;
            } else {
                tracing::warn!(
                    "Task {} was not found in task manager but record status is {:?} (already completed), skipping",
                    task_id, current_record.status
                );
            }
            return Ok(ApiResponse::ok(()));
        }
        // 取消成功时，由任务内部的 cancel 分支处理 DB 更新，
        // 避免与 stop handler 同时写入造成竞态条件
        tracing::info!("Successfully stopped execution record {}", record_id);
        Ok(ApiResponse::ok(()))
    } else {
        Err(AppError::BadRequest(
            "No task_id found for this execution record".to_string(),
        ))
    }
}

pub async fn get_running_execution_records_handler(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
) -> Result<ApiResponse<Vec<crate::models::ExecutionRecord>>, AppError> {
    // V1 隔离：按 workspace 过滤正在运行的执行记录（经 todo 间接关联）
    let records = state.db.get_running_execution_records().await?;
    let ws_todos = state.db.get_todos_by_workspace_id(Some(ws_id)).await?;
    let ws_todo_ids: std::collections::HashSet<i64> = ws_todos.iter().map(|t| t.id).collect();
    let records: Vec<_> = records
        .into_iter()
        .filter(|r| ws_todo_ids.contains(&r.todo_id))
        .collect();
    Ok(ApiResponse::ok(records))
}

#[derive(Debug, Deserialize)]
pub struct RunningBoardQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    /// 按工作空间 ID 过滤；不传返回全部。
    #[serde(default)]
    #[allow(dead_code)]
    pub workspace_id: Option<i64>,
    /// 按最近 N 小时过滤（对 execution records 生效）；不传或 0 表示不过滤。
    #[serde(default)]
    pub hours: Option<u32>,
}

#[allow(dead_code)]
pub async fn get_running_board(
    State(state): State<AppState>,
    Query(query): Query<RunningBoardQuery>,
) -> Result<ApiResponse<crate::models::RunningBoardResponse>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * limit;

    let (records, total) = state
        .db
        .get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: None,
            step_id: None,
            limit,
            offset,
            status: None,
            hours: query.hours,
        })
        .await?;

    // 按 workspace_id 过滤 execution records（表本身无 workspace_id，
    // 需通过 todos 表关联过滤）。一次查出该 workspace 下所有 todo_ids，
    // 构建 Hashset 后在内存中过滤。
    let records = if let Some(wid) = query.workspace_id {
        // 用 ? 传播 DbErr：旧实现 unwrap_or_default() 会把 DB 读失败（SQLite locked/IO）
        // 静默当成「空工作空间」，下游 filter 把所有 record 过滤掉、返回 200+0 条，
        // 调用方无法区分真空还是 DB 挂了。改为传播错误让请求返回 5xx。
        let ws_todos = state.db.get_todos_by_workspace_id(Some(wid)).await?;
        let ws_todo_ids: std::collections::HashSet<i64> = ws_todos.iter().map(|t| t.id).collect();
        records.into_iter().filter(|r| ws_todo_ids.contains(&r.todo_id)).collect()
    } else {
        records
    };

    let scheduled_todos = state.db.get_scheduler_todos(query.workspace_id).await?;

    Ok(ApiResponse::ok(crate::models::RunningBoardResponse {
        records,
        scheduled_todos,
        total,
        page,
        limit,
    }))
}

pub async fn force_fail_execution_handler(
    State(state): State<AppState>,
    Path((ws_id, record_id)): Path<(i64, i64)>,
) -> Result<ApiResponse<()>, AppError> {
    // V1 隔离：强制失败前校验 record 归属 workspace
    workspace_guard::verify_execution_belongs_to_ws(&state.db, record_id, ws_id).await?;
    let record =
        state
            .db
            .get_execution_record(record_id)
            .await?
            .ok_or(AppError::BadRequest(
                "Execution record not found".to_string(),
            ))?;

    if record.status != ExecutionStatus::Running {
        return Err(AppError::BadRequest(
            "Execution record is not running".to_string(),
        ));
    }

    // Try to cancel in-memory task if it exists
    if let Some(task_id) = &record.task_id {
        state.task_manager.cancel(task_id).await;
    }

    state
        .db
        .force_fail_execution_record(record_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(()))
}

#[derive(Debug, Deserialize)]
pub struct ResumeExecutionRequest {
    pub message: Option<String>,
}

pub async fn resume_execution_handler(
    State(state): State<AppState>,
    Path((ws_id, id)): Path<(i64, i64)>,
    ApiJson(req): ApiJson<ResumeExecutionRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    // V1 隔离：恢复执行前校验 record 归属 workspace
    workspace_guard::verify_execution_belongs_to_ws(&state.db, id, ws_id).await?;
    let record = state
        .db
        .get_execution_record(id)
        .await?
        .ok_or(AppError::NotFound)?;

    if record.status == ExecutionStatus::Running {
        return Err(AppError::BadRequest(
            "Cannot resume a running execution".to_string(),
        ));
    }

    let executor_type = record
        .executor
        .as_deref()
        .and_then(parse_executor_type)
        .ok_or_else(|| AppError::BadRequest("Unknown executor type".to_string()))?;

    let executor = state
        .executor_registry
        .get(executor_type).await
        .ok_or_else(|| AppError::Internal("Executor not found in registry".to_string()))?;

    if !executor.supports_resume() {
        return Err(AppError::BadRequest(
            "This executor does not support resuming conversations".to_string(),
        ));
    }

    let todo_id = record.todo_id;
    let todo = state
        .db
        .get_todo(todo_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // 检查该 todo 下正在执行的记录数量是否已达并发上限
    // 需要过滤掉孤儿记录：状态为 running 但 task_manager 中没有对应 task
    // RwLock 中毒恢复：PoisonError 时取内部 guard 继续运行，与 backup.rs 保持一致
    let max_concurrent = state.config.read().unwrap_or_else(|e| e.into_inner()).max_concurrent_todos;
    let running_tasks = state.task_manager.get_all_task_infos().await;
    let running_records = state.db.get_running_records_by_todo_id(todo_id).await?;
    let running_count_for_todo = running_records
        .iter()
        .filter(|r| {
            // 排除僵尸记录：状态为 running 但 task_manager 中没有对应 task
            if let Some(task_id) = &r.task_id {
                running_tasks.iter().any(|t| t.task_id == *task_id)
            } else {
                false
            }
        })
        .count();
    if running_count_for_todo >= max_concurrent as usize {
        return Err(AppError::BadRequest(format!(
            "Todo {} has {} execution(s) still running (limit: {}). Cannot resume.",
            todo_id, running_count_for_todo, max_concurrent
        )));
    }

    let message = req
        .message
        .as_ref()
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .map(|m| m.to_string())
        .unwrap_or_else(|| todo.prompt.clone());

    let resume_message = req
        .message
        .as_ref()
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .map(|m| m.to_string());

    // 只能从 session_id resume。task_id 是执行启动时生成的随机 UUID，
    // 不是 Claude Code 的真实 session ID，不能作为 resume 的凭据。
    // session_id 由 executor 在解析 stdout 的 system 事件时异步回写 DB；
    // 若执行异常退出或 extractor 未实现，DB 里的 session_id 仍为 NULL，
    // 此时调用 resume 需等待或重试。
    let resume_session_id = resolve_resume_session_id(&record)?;

    let result = start_todo_execution(RunTodoExecutionRequest {
        db: state.db.clone(),
        executor_registry: state.executor_registry.clone(),
        tx: state.tx.clone(),
        task_manager: state.task_manager.clone(),
        config: state.config.clone(),
        todo_id,
        message,
        req_executor: record.executor.clone(),
        req_model: None,
        trigger_type: "manual".to_string(),
        params: None,
        resume_session_id: Some(resume_session_id),
        resume_message,
        source_todo_id: None,
        source_todo_title: None,
        loop_step_execution_id: None,
        step_id: None,
        feishu_bot_id: None,
        feishu_receive_id: None,
            feishu_receive_id_type: None,
        workspace_path: None,
        // 从 todo 中提取 workspace_id，用于 FeishuPushService 按 workspace 隔离推送
        workspace_id: todo.workspace_id,
        // resume 路径同样需要专家上下文：用户可能基于上一次专家输出继续追问
        expert_manager: Some(state.expert_manager.clone()),
    })
    .await?;
    let record_id = result.record_id
        .ok_or_else(|| AppError::Internal("执行启动失败：未获取到执行记录 ID".to_string()))?;

    Ok(ApiResponse::ok(
        serde_json::json!({ "task_id": result.task_id, "record_id": record_id }),
    ))
}

pub async fn get_execution_summary(
    State(state): State<AppState>,
    Path((ws_id, id)): Path<(i64, i64)>,
) -> Result<ApiResponse<ExecutionSummary>, AppError> {
    // V1 隔离：db.get_execution_summary 按 todo_id 聚合，故校验 todo 归属 workspace。
    // 此 handler 由 todo 域 /todos/{id}/summary 引用（execution 域无 summary 端点）。
    workspace_guard::verify_todo_belongs_to_ws(&state.db, id, ws_id).await?;
    Ok(ApiResponse::ok(state.db.get_execution_summary(id).await?))
}

/// Dashboard stats cache: 30-second TTL, supports multiple time ranges.
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant as StdInstant};
use tokio::sync::RwLock;

struct DashboardCacheEntry {
    stats: DashboardStats,
    expires_at: StdInstant,
}

static DASHBOARD_CACHE: LazyLock<RwLock<HashMap<(i64, u32), DashboardCacheEntry>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub async fn get_dashboard_stats(
    State(state): State<AppState>,
    // Dashboard 为全局运营视图，路由注册在 /api/v1/stats/dashboard（无 workspace 路径参数），
    // 因此 handler 不能声明 Path(ws_id)，否则 Axum 提取器会因路径参数缺失而直接返回 500。
    Query(params): Query<DashboardStatsParams>,
) -> Result<ApiResponse<DashboardStats>, AppError> {
    let hours_key = params.hours.unwrap_or(24 * 7); // default: 7 days
    // Dashboard 为全局运营视图，缓存键仅按时间窗口区分
    let cache_key = (0i64, hours_key);

    {
        let cache = DASHBOARD_CACHE.read().await;
        if let Some(entry) = cache.get(&cache_key) {
            if entry.expires_at > StdInstant::now() {
                return Ok(ApiResponse::ok(entry.stats.clone()));
            }
        }
    }

    // Dashboard 聚合全库数据，作为全局运营视图；不再按 workspace 隔离。
    let stats = state.db.get_dashboard_stats(params.hours).await?;

    {
        let mut cache = DASHBOARD_CACHE.write().await;
        cache.insert(cache_key, DashboardCacheEntry {
            stats: stats.clone(),
            expires_at: StdInstant::now() + Duration::from_secs(30),
        });
    }

    Ok(ApiResponse::ok(stats))
}

#[derive(Deserialize)]
pub struct DashboardStatsParams {
    #[serde(default)]
    pub hours: Option<u32>,
}

pub async fn get_running_todos(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
) -> Result<ApiResponse<Vec<crate::models::Todo>>, AppError> {
    // V1 隔离：按 workspace 过滤正在运行的 todo（db 层无 ws 参数，内存过滤）
    let running_todos = state.db.get_running_todos().await?;
    let filtered: Vec<_> = running_todos
        .into_iter()
        .filter(|t| t.workspace_id == Some(ws_id))
        .collect();
    Ok(ApiResponse::ok(filtered))
}

/// 智能新建：用户提交自然语言描述，通过默认响应 Todo 自动执行
#[allow(dead_code)]
pub async fn smart_create_handler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<SmartCreateRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let content = req.content.trim();
    if content.is_empty() {
        return Err(AppError::BadRequest("内容不能为空".to_string()));
    }

    // 从 workspace 设置读取默认响应 Todo ID
    // auto-deref 会自动将 Arc<Database> 解引用为 &Database，无需手动 *
    let workspace_settings = crate::db::workspace_setting::get_workspace_settings(&state.db, req.workspace_id).await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 #{} 不存在", req.workspace_id)))?;

    let todo_id = workspace_settings.default_response_todo_id
        .ok_or_else(|| AppError::BadRequest("尚未配置默认响应 Todo，请先在工作空间设置中配置".to_string()))?;

    // 验证 Todo 存在
    let todo = state
        .db
        .get_todo(todo_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("默认响应 Todo #{} 不存在", todo_id)))?;

    // 检查并发限制
    // RwLock 中毒恢复：PoisonError 时取内部 guard 继续运行，与 backup.rs 保持一致
    let max_concurrent = state.config.read().unwrap_or_else(|e| e.into_inner()).max_concurrent_todos;
    let running_tasks = state.task_manager.get_all_task_infos().await;
    let running_records = state.db.get_running_records_by_todo_id(todo_id).await?;
    let running_count_for_todo = running_records
        .iter()
        .filter(|r| {
            if let Some(task_id) = &r.task_id {
                running_tasks.iter().any(|t| t.task_id == *task_id)
            } else {
                false
            }
        })
        .count();
    if running_count_for_todo >= max_concurrent as usize {
        return Err(AppError::BadRequest(format!(
            "默认响应 Todo #{} 已有 {} 个执行在运行中（上限 {}），请稍后再试",
            todo_id, running_count_for_todo, max_concurrent
        )));
    }

    // 构建模板参数
    let mut params = std::collections::HashMap::new();
    params.insert("content".to_string(), content.to_string());
    params.insert("message".to_string(), content.to_string());
    params.insert("raw_message".to_string(), content.to_string());

    let mut message = todo.prompt.clone();

    // 如果 prompt 模板中没有任何占位符，将用户内容追加到 message 末尾，
    // 确保用户提交的内容一定能传递到执行器
    let has_placeholder = params.keys().any(|key| message.contains(&format!("{{{{{}}}}}", key)));
    if !has_placeholder {
        if message.is_empty() {
            message = content.to_string();
        } else {
            message = format!("{}\n\n{}", message, content);
        }
    }

    let result = start_todo_execution(RunTodoExecutionRequest {
        db: state.db.clone(),
        executor_registry: state.executor_registry.clone(),
        tx: state.tx.clone(),
        task_manager: state.task_manager.clone(),
        config: state.config.clone(),
        todo_id,
        message,
        req_executor: None,
        req_model: None,
        trigger_type: "smart_create".to_string(),
        params: Some(params),
        resume_session_id: None,
        resume_message: None,
        source_todo_id: None,
        source_todo_title: None,
        loop_step_execution_id: None,
        step_id: None,
        feishu_bot_id: None,
        feishu_receive_id: None,
            feishu_receive_id_type: None,
        workspace_path: None,
        // 从 todo 中提取 workspace_id，用于 FeishuPushService 按 workspace 隔离推送
        workspace_id: todo.workspace_id,
        // smart_create 路径：同样注入专家上下文，新建 todo 可能带 expert_name
        expert_manager: Some(state.expert_manager.clone()),
    })
    .await?;

    let record_id = result.record_id
        .ok_or_else(|| AppError::Internal("执行启动失败：未获取到执行记录 ID".to_string()))?;

    Ok(ApiResponse::ok(serde_json::json!({
        "task_id": result.task_id,
        "record_id": record_id,
        "todo_id": todo_id,
        "todo_title": todo.title,
    })))
}

/// 从执行记录中解析出可用的 resume session_id。
///
/// 严格化策略：只接受 DB 里的 `session_id`（由 executor 在解析 stdout 的
/// system 事件时异步回写），不再回退到 `task_id`。
/// `task_id` 是后端生成的随机 UUID，不能作为 Claude Code / Kimi 等
/// 执行器的真实会话凭据，传给 `--resume` / `-S` 会导致 CLI 报错。
///
/// 该函数独立于 AppState / DB，方便单测覆盖核心拒绝逻辑。
fn resolve_resume_session_id(
    record: &crate::models::ExecutionRecord,
) -> Result<String, AppError> {
    record.session_id.clone().ok_or_else(|| {
        AppError::BadRequest("No session_id available for resume".to_string())
    })
}

// =========================================================================
// v1 API 路由及处理器变体（K8s 风格路径）
// -------------------------------------------------------------------------
// 所有 v1 路由通过 `v1_routes()` 暴露，在 `mod.rs` 中嵌套到
// /api/v1/workspaces/{ws}/executions 下，因此 v1_routes 中的路径都是相对路径。
// workspace_id 从 URL Path 中提取，不再依赖 Query 参数或请求体。
// =========================================================================

/// v1 变体：从 Path 获取 workspace_id，始终按工作空间过滤执行记录。
pub async fn v1_get_execution_records(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    Query(query): Query<TodoIdQuery>,
) -> Result<ApiResponse<ExecutionRecordsPage>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(10).clamp(1, 100);
    let offset = (page - 1) * limit;
    let status = match query.status.as_deref() {
        Some("all") | None => None,
        Some(s) => Some(
            s.parse::<ExecutionStatus>()
                .map(|v| v.as_str())
                .map_err(AppError::BadRequest)?
        ),
    };
    let (records, total) = state
        .db
        .get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: query.todo_id,
            step_id: query.step_id,
            limit,
            offset,
            status,
            hours: query.hours,
        })
        .await?;

    // V1 隔离：workspace_id 来自 URL Path。即使带 todo_id 也要校验其归属本 ws，
    // 否则 ?todo_id=<他人 todo> 可越权读他人执行记录。返回结果一律按 ws 过滤。
    if let Some(todo_id) = query.todo_id {
        workspace_guard::verify_todo_belongs_to_ws(&state.db, todo_id, ws_id).await?;
    }
    let ws_todos = state.db.get_todos_by_workspace_id(Some(ws_id)).await?;
    let ws_todo_ids: std::collections::HashSet<i64> = ws_todos.iter().map(|t| t.id).collect();
    let records: Vec<_> = records
        .into_iter()
        .filter(|r| ws_todo_ids.contains(&r.todo_id))
        .collect();

    Ok(ApiResponse::ok(ExecutionRecordsPage {
        records,
        total,
        page,
        limit,
    }))
}

/// v1 变体：从 Path 获取 workspace_id，始终按工作空间返回 running board。
pub async fn v1_get_running_board(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    Query(query): Query<RunningBoardQuery>,
) -> Result<ApiResponse<crate::models::RunningBoardResponse>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * limit;

    let (records, total) = state
        .db
        .get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: None,
            step_id: None,
            limit,
            offset,
            status: None,
            hours: query.hours,
        })
        .await?;

    // v1 路径下 workspace_id 始终来自 Path，强制按工作空间过滤
    let ws_todos = state.db.get_todos_by_workspace_id(Some(ws_id)).await?;
    let ws_todo_ids: std::collections::HashSet<i64> = ws_todos.iter().map(|t| t.id).collect();
    let records = records.into_iter().filter(|r| ws_todo_ids.contains(&r.todo_id)).collect();

    // v1 路径下使用 Path 中的 workspace_id 查询定时任务
    let scheduled_todos = state.db.get_scheduler_todos(Some(ws_id)).await?;

    Ok(ApiResponse::ok(crate::models::RunningBoardResponse {
        records,
        scheduled_todos,
        total,
        page,
        limit,
    }))
}

/// v1 变体：从 Path 获取 workspace_id，不再依赖请求体中的 workspace_id 字段。
pub async fn v1_smart_create_handler(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    ApiJson(req): ApiJson<SmartCreateRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let content = req.content.trim();
    if content.is_empty() {
        return Err(AppError::BadRequest("内容不能为空".to_string()));
    }

    // v1 使用 Path 中的 workspace_id 而非请求体内的 workspace_id 字段
    let workspace_settings = crate::db::workspace_setting::get_workspace_settings(&state.db, ws_id).await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 #{} 不存在", ws_id)))?;

    let todo_id = workspace_settings.default_response_todo_id
        .ok_or_else(|| AppError::BadRequest("尚未配置默认响应 Todo，请先在工作空间设置中配置".to_string()))?;

    // 验证 Todo 存在且属于当前 workspace：settings 表本身没有外键约束保证
    // default_response_todo_id 与 workspace_id 一致，必须显式校验防止跨空间执行。
    workspace_guard::verify_todo_belongs_to_ws(&state.db, todo_id, ws_id).await?;
    let todo = state
        .db
        .get_todo(todo_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("默认响应 Todo #{} 不存在", todo_id)))?;

    // 检查并发限制
    // RwLock 中毒恢复：PoisonError 时取内部 guard 继续运行，与 backup.rs 保持一致
    let max_concurrent = state.config.read().unwrap_or_else(|e| e.into_inner()).max_concurrent_todos;
    let running_tasks = state.task_manager.get_all_task_infos().await;
    let running_records = state.db.get_running_records_by_todo_id(todo_id).await?;
    let running_count_for_todo = running_records
        .iter()
        .filter(|r| {
            if let Some(task_id) = &r.task_id {
                running_tasks.iter().any(|t| t.task_id == *task_id)
            } else {
                false
            }
        })
        .count();
    if running_count_for_todo >= max_concurrent as usize {
        return Err(AppError::BadRequest(format!(
            "默认响应 Todo #{} 已有 {} 个执行在运行中（上限 {}），请稍后再试",
            todo_id, running_count_for_todo, max_concurrent
        )));
    }

    // 构建模板参数
    let mut params = std::collections::HashMap::new();
    params.insert("content".to_string(), content.to_string());
    params.insert("message".to_string(), content.to_string());
    params.insert("raw_message".to_string(), content.to_string());

    let mut message = todo.prompt.clone();

    // 如果 prompt 模板中没有任何占位符，将用户内容追加到 message 末尾，
    // 确保用户提交的内容一定能传递到执行器
    let has_placeholder = params.keys().any(|key| message.contains(&format!("{{{{{}}}}}", key)));
    if !has_placeholder {
        if message.is_empty() {
            message = content.to_string();
        } else {
            message = format!("{}\n\n{}", message, content);
        }
    }

    let result = start_todo_execution(RunTodoExecutionRequest {
        db: state.db.clone(),
        executor_registry: state.executor_registry.clone(),
        tx: state.tx.clone(),
        task_manager: state.task_manager.clone(),
        config: state.config.clone(),
        todo_id,
        message,
        req_executor: None,
        req_model: None,
        trigger_type: "smart_create".to_string(),
        params: Some(params),
        resume_session_id: None,
        resume_message: None,
        source_todo_id: None,
        source_todo_title: None,
        loop_step_execution_id: None,
        step_id: None,
        feishu_bot_id: None,
        feishu_receive_id: None,
            feishu_receive_id_type: None,
        workspace_path: None,
        // 从 todo 中提取 workspace_id，用于 FeishuPushService 按 workspace 隔离推送
        workspace_id: todo.workspace_id,
        // smart_create 路径：同样注入专家上下文，新建 todo 可能带 expert_name
        expert_manager: Some(state.expert_manager.clone()),
    })
    .await?;

    let record_id = result.record_id
        .ok_or_else(|| AppError::Internal("执行启动失败：未获取到执行记录 ID".to_string()))?;

    Ok(ApiResponse::ok(serde_json::json!({
        "task_id": result.task_id,
        "record_id": record_id,
        "todo_id": todo_id,
        "todo_title": todo.title,
    })))
}

/// v1 路由集合：所有路径相对于 /api/v1/workspaces/{ws}/executions。
/// 在 `mod.rs` 中通过 `.nest("/api/v1/workspaces/{ws}/executions", execution::v1_routes())`
/// 挂载，路径中的 {ws} 自动传递给每个处理器。
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        // GET / — 列出工作空间下的执行记录
        .route("/", get(v1_get_execution_records))
        // POST / — 启动一个 todo 执行
        .route("/", post(execute_handler))
        // GET /{id} — 获取单条执行记录详情
        .route("/{id}", get(get_execution_record))
        // PUT /{id}/rating — 评分子执行记录
        .route("/{id}/rating", put(rate_execution_handler))
        // GET /{id}/logs — 获取执行日志
        .route("/{id}/logs", get(get_execution_logs_handler))
        // POST /{id}/resume — 恢复已完成的执行
        .route("/{id}/resume", post(resume_execution_handler))
        // POST /{id}/stop — 停止执行（路径指定 record_id，已做 ws 归属校验）
        .route("/{id}/stop", post(stop_execution_handler))
        // POST /{id}/force-fail — 强制标记执行为失败
        .route("/{id}/force-fail", post(force_fail_execution_handler))
        // GET /running — 本工作空间正在运行的执行记录
        .route("/running", get(get_running_execution_records_handler))
        // GET /running-board — 工作空间运行看板
        .route("/running-board", get(v1_get_running_board))
        // GET /running-todos — 本工作空间正在运行的事项
        .route("/running-todos", get(get_running_todos))
        // GET /session/{session_id} — 按会话 ID 查执行记录
        .route("/session/{session_id}", get(get_execution_records_by_session))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod resume_session_id_tests {
    use super::*;
    use crate::models::{ExecutionRecord, ExecutionStatus};

    /// 构造一个最小可用的测试 ExecutionRecord，固定关键字段。
    fn make_record(session_id: Option<&str>, task_id: Option<&str>) -> ExecutionRecord {
        ExecutionRecord {
            id: 1,
            todo_id: 1,
            status: ExecutionStatus::Success,
            command: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            result: None,
            started_at: String::new(),
            finished_at: None,
            usage: None,
            executor: None,
            model: None,
            trigger_type: "manual".to_string(),
            pid: None,
            task_id: task_id.map(|s| s.to_string()),
            session_id: session_id.map(|s| s.to_string()),
            todo_progress: None,
            agent_runs: None,
            execution_stats: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,
            step_id: None,
            rating: None,
            source_execution_record_id: None,
            last_review_status: None,
            last_reviewed_at: None,
            worktree_path: None,
        }
    }

    #[test]
    fn test_resolve_resume_session_id_none_returns_err() {
        // session_id 为 None：必须返回 400，绝不能放行
        let record = make_record(None, Some("task-uuid-123"));
        let err = resolve_resume_session_id(&record).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        if let AppError::BadRequest(msg) = err {
            assert!(msg.contains("No session_id available for resume"));
        }
    }

    #[test]
    fn test_resolve_resume_session_id_both_none_returns_err() {
        // session_id 与 task_id 都为 None：仍然返回 400
        let record = make_record(None, None);
        let err = resolve_resume_session_id(&record).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn test_resolve_resume_session_id_with_sid_returns_sid() {
        // session_id 有值：直接返回 session_id
        let record = make_record(Some("real-claude-sid-abc"), Some("task-uuid-123"));
        let sid = resolve_resume_session_id(&record).unwrap();
        assert_eq!(sid, "real-claude-sid-abc");
    }

    #[test]
    fn test_resolve_resume_session_id_ignores_task_id() {
        // 即便 task_id 有值、session_id 为 None，也必须拒绝（不能 fallback）
        let record = make_record(None, Some("task-uuid-123"));
        assert!(resolve_resume_session_id(&record).is_err());
    }

    #[test]
    fn test_resolve_resume_session_id_sid_takes_precedence() {
        // 两者都有时返回 session_id，绝不会把 task_id 当作 sid
        let record = make_record(Some("real-sid"), Some("random-task-uuid"));
        let sid = resolve_resume_session_id(&record).unwrap();
        assert_eq!(sid, "real-sid");
        assert_ne!(sid, "random-task-uuid");
    }
}
