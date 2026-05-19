use axum::extract::{Path, Query, State};
use serde::Deserialize;

use crate::adapters::parse_executor_type;
use crate::executor_service::{
    run_todo_execution, run_todo_execution_with_params, ExecutionResult, RunTodoExecutionRequest,
};
use crate::handlers::{ApiJson, AppError, AppState};
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
        .get_execution_records(query.todo_id, limit, offset, status)
        .await?;
    Ok(ApiResponse::ok(ExecutionRecordsPage {
        records,
        total,
        page,
        limit,
    }))
}

pub async fn get_execution_record(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<crate::models::ExecutionRecord>, AppError> {
    let record = state
        .db
        .get_execution_record(id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(record))
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
    Path(id): Path<i64>,
    Query(query): Query<ExecutionLogsQuery>,
) -> Result<ApiResponse<ExecutionLogsPage>, AppError> {
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
    Path(session_id): Path<String>,
) -> Result<ApiResponse<Vec<crate::models::ExecutionRecord>>, AppError> {
    let records = state
        .db
        .get_execution_records_by_session(&session_id)
        .await?;
    Ok(ApiResponse::ok(records))
}

pub async fn execute_handler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<ExecuteRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    // Get the todo to use its prompt as fallback when message is not provided
    let todo = state
        .db
        .get_todo(req.todo_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("Todo {} not found", req.todo_id)))?;

    // 检查该 todo 下正在执行的记录数量是否已达并发上限
    // 需要过滤掉孤儿记录：状态为 running 但 task_manager 中没有对应 task
    let max_concurrent = state.config.read().await.max_concurrent_todos;
    let running_tasks = state.task_manager.get_all_task_infos().await;
    let running_records = state.db.get_running_execution_records().await?;
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
        .filter(|r| r.todo_id == req.todo_id)
        .count();
    if running_count_for_todo >= max_concurrent as usize {
        return Err(AppError::BadRequest(format!(
            "Todo {} has {} execution(s) still running (limit: {}). Please stop them first.",
            req.todo_id, running_count_for_todo, max_concurrent
        )));
    }

    // Fall back to todo.prompt if message is None or whitespace-only
    let message = req
        .message
        .as_ref()
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .map(|m| m.to_string())
        .unwrap_or_else(|| todo.prompt.clone());

    let result = start_todo_execution(RunTodoExecutionRequest {
        db: state.db.clone(),
        executor_registry: state.executor_registry.clone(),
        tx: state.tx.clone(),
        task_manager: state.task_manager.clone(),
        config: state.config.clone(),
        todo_id: req.todo_id,
        message,
        req_executor: req.executor,
        trigger_type: "manual".to_string(),
        params: None,
        resume_session_id: None,
        resume_message: None,
    })
    .await;
    let result = result?;
    let record_id = result.record_id
        .ok_or_else(|| AppError::Internal("执行启动失败：未获取到执行记录 ID".to_string()))?;

    Ok(ApiResponse::ok(
        serde_json::json!({ "task_id": result.task_id, "record_id": record_id }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct StopExecutionRequest {
    pub record_id: i64,
}

pub async fn stop_execution_handler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<StopExecutionRequest>,
) -> Result<ApiResponse<()>, AppError> {
    tracing::info!("Stopping execution record: {}", req.record_id);

    let record =
        state
            .db
            .get_execution_record(req.record_id)
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
            req.record_id,
            task_id
        );
        let cancelled = state.task_manager.cancel(task_id).await;
        if !cancelled {
            // 任务不在 task_manager 中，可能是已完成清理或已崩溃。
            // 重新查询 DB 确认当前状态，避免与正常完成的任务产生竞态
            let current_record = state
                .db
                .get_execution_record(req.record_id)
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
                    .force_fail_execution_record(req.record_id)
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
        tracing::info!("Successfully stopped execution record {}", req.record_id);
        Ok(ApiResponse::ok(()))
    } else {
        Err(AppError::BadRequest(
            "No task_id found for this execution record".to_string(),
        ))
    }
}

pub async fn get_running_execution_records_handler(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<crate::models::ExecutionRecord>>, AppError> {
    let records = state.db.get_running_execution_records().await?;
    Ok(ApiResponse::ok(records))
}

#[derive(Debug, Deserialize)]
pub struct ForceFailRequest {
    pub record_id: i64,
}

pub async fn force_fail_execution_handler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<ForceFailRequest>,
) -> Result<ApiResponse<()>, AppError> {
    let record =
        state
            .db
            .get_execution_record(req.record_id)
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
        .force_fail_execution_record(req.record_id)
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
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<ResumeExecutionRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
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
    let max_concurrent = state.config.read().await.max_concurrent_todos;
    let running_tasks = state.task_manager.get_all_task_infos().await;
    let running_records = state.db.get_running_execution_records().await?;
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
        .filter(|r| r.todo_id == todo_id)
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

    let resume_session_id = record.session_id.or(record.task_id).ok_or_else(|| {
        AppError::BadRequest("No session_id found for this execution record".to_string())
    })?;

    let result = start_todo_execution(RunTodoExecutionRequest {
        db: state.db.clone(),
        executor_registry: state.executor_registry.clone(),
        tx: state.tx.clone(),
        task_manager: state.task_manager.clone(),
        config: state.config.clone(),
        todo_id,
        message,
        req_executor: record.executor.clone(),
        trigger_type: "manual".to_string(),
        params: None,
        resume_session_id: Some(resume_session_id),
        resume_message,
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
    Path(id): Path<i64>,
) -> Result<ApiResponse<ExecutionSummary>, AppError> {
    Ok(ApiResponse::ok(state.db.get_execution_summary(id).await?))
}

pub async fn get_dashboard_stats(
    State(state): State<AppState>,
    Query(params): Query<DashboardStatsParams>,
) -> Result<ApiResponse<DashboardStats>, AppError> {
    Ok(ApiResponse::ok(state.db.get_dashboard_stats(params.hours).await?))
}

#[derive(Deserialize)]
pub struct DashboardStatsParams {
    #[serde(default)]
    pub hours: Option<u32>,
}

pub async fn get_running_todos(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<crate::models::Todo>>, AppError> {
    let running_todos = state.db.get_running_todos().await?;
    Ok(ApiResponse::ok(running_todos))
}

/// 智能新建：用户提交自然语言描述，通过默认响应 Todo 自动执行
pub async fn smart_create_handler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<SmartCreateRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let content = req.content.trim();
    if content.is_empty() {
        return Err(AppError::BadRequest("内容不能为空".to_string()));
    }

    // 读取默认响应 Todo ID
    let default_todo_id = {
        let cfg = state.config.read().await;
        cfg.default_response_todo_id
    };

    let todo_id = default_todo_id
        .ok_or_else(|| AppError::BadRequest("尚未配置默认响应 Todo，请先在设置中配置".to_string()))?;

    // 验证 Todo 存在
    let todo = state
        .db
        .get_todo(todo_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("默认响应 Todo #{} 不存在", todo_id)))?;

    // 检查并发限制
    let max_concurrent = state.config.read().await.max_concurrent_todos;
    let running_tasks = state.task_manager.get_all_task_infos().await;
    let running_records = state.db.get_running_execution_records().await?;
    let running_count_for_todo = running_records
        .iter()
        .filter(|r| {
            if let Some(task_id) = &r.task_id {
                running_tasks.iter().any(|t| t.task_id == *task_id)
            } else {
                false
            }
        })
        .filter(|r| r.todo_id == todo_id)
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
        trigger_type: "smart_create".to_string(),
        params: Some(params),
        resume_session_id: None,
        resume_message: None,
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
