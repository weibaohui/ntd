use axum::extract::{Path, Query, State};
use cron::Schedule;
use serde::Deserialize;
use std::str::FromStr;

use crate::db::TodoUpdate;
use crate::handlers::{ApiJson, AppError, AppState};
// todo hook 已整块移除（plan `purring-forging-petal`），HookContext 不再导入。
use crate::models::{
    utc_timestamp, ApiResponse, BatchCopyTodoWorkspaceRequest,
    BatchUpdateTodoExecutorRequest, BatchUpdateTodoResult,
    BatchUpdateTodoSchedulerRequest, BatchUpdateTodoWorkspaceRequest, BatchWorkspaceResult,
    ComputedBucket, CreateTodoRequest, RecentCompletedTodo, Todo, TodoCenterItem,
    UpdateTagsRequest, UpdateTodoRequest, UpdateWebhookRequest,
};
// 批量恢复调度需要在 handler 中构造 ServiceContext 调用 scheduler.upsert_task，
// 与单个 update_scheduler handler (handlers/scheduler.rs) 的处理路径保持一致。
use crate::service_context::ServiceContext;

/// Validate cron expression, return helpful error for invalid ones
fn validate_cron_expression(expr: &str) -> Result<(), String> {
    Schedule::from_str(expr)
        .map(|_| ())
        .map_err(|_| {
            format!(
                "Invalid cron expression: '{}'. AI must convert natural language to valid cron format. \
                Expected format with 6 fields (seconds + 5 standard): '0 */12 * * * *' (every 12 min), \
                '0 0 * * * *' (every minute), '0 0 9 * * *' (daily at 9am). See https://crontab.guru/",
                expr
            )
        })
}

#[derive(Debug, serde::Deserialize)]
pub struct TodoListQuery {
    /// 按工作空间 ID 过滤 Todo（对应 todos.workspace_id 字段）
    #[serde(default)]
    pub workspace_id: Option<i64>,
    /// 按最近 N 小时过滤（对 updated_at 生效，completed/failed 状态的 todo
    /// 按 finished_at 过滤）；不传或 0 表示不过滤。
    #[serde(default)]
    pub hours: Option<u32>,
}

pub async fn get_todos(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<TodoListQuery>,
) -> Result<ApiResponse<Vec<Todo>>, AppError> {
    let todos = state.db.get_todos_by_workspace_id(params.workspace_id).await?;
    // 按 hours 过滤：只保留在最近 N 小时内更新过的 todo
    let todos = if let Some(h) = params.hours.filter(|&h| h > 0) {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(h as i64);
        let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%S").to_string();
        todos.into_iter().filter(|t| {
            // 按 updated_at 过滤（finished_at 字段未在模型上实现，暂统一用 updated_at）
            t.updated_at >= cutoff_str
        }).collect()
    } else {
        todos
    };
    Ok(ApiResponse::ok(todos))
}

/// 事项中心查询参数。
/// `bucket` 为空或非法时返回全部分类（前端可自行按 computed_bucket 分组）。
/// `search` 为标题/prompt 子串过滤（设计文档 API 示例带 search 参数）。
#[derive(Debug, serde::Deserialize)]
pub struct TodoCenterQuery {
    #[serde(default)]
    pub workspace_id: Option<i64>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
}

/// `GET /api/todos/center`：事项中心五类驱动视图。
///
/// 服务端按事实字段推导 computed_bucket 并（可选）按 bucket 过滤，
/// 批量补算 loop 引用计数与最近执行记录，避免前端 N+1。
pub async fn get_todo_center(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<TodoCenterQuery>,
) -> Result<ApiResponse<Vec<TodoCenterItem>>, AppError> {
    // 解析 bucket 串为枚举；空/非法 → None = 不过滤
    let bucket = params.bucket.as_deref().and_then(ComputedBucket::parse_query);
    let search = params.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let items = state.db.get_todo_center(params.workspace_id, bucket, search).await?;
    Ok(ApiResponse::ok(items))
}

/// `POST /api/todos/{id}/archive`：归档事项（仅隐藏，不删数据/不解引用）。
/// 返回重新计算后的 TodoCenterItem（computed_bucket=archived）。
pub async fn archive_todo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<TodoCenterItem>, AppError> {
    // rows_affected=0 说明 todo 不存在或已软删，统一回 404
    if !state.db.archive_todo(id).await? {
        return Err(AppError::NotFound);
    }
    let item = load_center_item_or_404(&state, id).await?;
    Ok(ApiResponse::ok(item))
}

/// `POST /api/todos/{id}/restore`：恢复事项（清空 archived_at，分类按真实关系重算）。
pub async fn restore_todo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<TodoCenterItem>, AppError> {
    if !state.db.restore_todo(id).await? {
        return Err(AppError::NotFound);
    }
    let item = load_center_item_or_404(&state, id).await?;
    Ok(ApiResponse::ok(item))
}

/// `PUT /api/todos/{id}/webhook`：开启/关闭事件驱动。
/// 与 `PUT /api/todos/{id}/scheduler` 对称的扁平具名路由。
pub async fn update_webhook(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateWebhookRequest>,
) -> Result<ApiResponse<TodoCenterItem>, AppError> {
    if !state.db.update_todo_webhook(id, req.webhook_enabled).await? {
        return Err(AppError::NotFound);
    }
    let item = load_center_item_or_404(&state, id).await?;
    Ok(ApiResponse::ok(item))
}

/// 取单个 todo 的 TodoCenterItem；不存在则 404。
/// 抽出来让 archive/restore/webhook 三个 handler 共用同一条回传路径。
async fn load_center_item_or_404(
    state: &AppState,
    id: i64,
) -> Result<TodoCenterItem, AppError> {
    state
        .db
        .get_todo_center_item(id)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn get_todo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<Todo>, AppError> {
    let todo = state.require_todo(id).await?;
    Ok(ApiResponse::ok(todo))
}

pub async fn create_todo(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateTodoRequest>,
) -> Result<ApiResponse<Todo>, AppError> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("Title is required".to_string()));
    }
    let now = utc_timestamp();
    let prompt = if req.prompt.trim().is_empty() {
        title.to_string()
    } else {
        req.prompt.trim().to_string()
    };
    // 在 handler 层解析一次最终执行器名：有显式非空值用它，否则取数据库默认执行器。
    // 这样既作为 create_todo_with_extras 的入参（传入确定值后 DAO 内部不再重复解析、
    // 不再重复查库），又直接复用为返回给前端的 executor 字段——避免「解析两次 + 逻辑重复」。
    let executor_name = match req.executor.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => state
            .db
            .get_default_executor_name()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?,
    };

    // 工作空间 id 必填且必须存在：handler 按 id 解析出 path 后再下传，
    // DAO 一次写入 workspace_id + workspace_path 两列保证双字段同步。
    let dir = state
        .db
        .get_project_directory_by_id(req.workspace_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id)))?;
    let id = state.db.create_todo_with_extras(
        title,
        &prompt,
        // executor_name 已是非空确定值，DAO 不会再触发默认执行器回退逻辑
        Some(&executor_name),
        req.acceptance_criteria.as_deref(),
        req.webhook_enabled.unwrap_or(false),
        req.workspace_id,
        &dir.path,
    ).await?;

    // Update executor if specified
    if let Some(ref exec) = req.executor {
        if let Err(e) = state.db.update_todo_executor(id, exec).await {
            tracing::warn!("Failed to update executor for todo {}: {}", id, e);
        }
    }

    // 落库 action_type/action_key（create_todo_with_extras 不支持这两个字段）
    if req.action_type.is_some() || req.action_key.is_some() {
        if let Err(e) = state.db.update_todo_full(crate::db::TodoUpdate {
            id,
            title,
            prompt: &prompt,
            status: crate::models::TodoStatus::Pending,
            executor: None,
            scheduler_enabled: None,
            scheduler_config: None,
            scheduler_timezone: None,
            workspace_id: None,
            webhook_enabled: None,
            acceptance_criteria: None,
            auto_review_enabled: None,
            action_type: req.action_type.as_deref(),
            action_key: req.action_key.as_deref(),
        }).await {
            tracing::warn!("Failed to set action_type/action_key for todo {}: {}", id, e);
        }
    }

    for tag_id in &req.tag_ids {
        state.db.add_todo_tag(id, *tag_id).await?;
    }

    // Handle scheduler settings
    let scheduler_enabled = req.scheduler_enabled.unwrap_or(false);
    let scheduler_config = req
        .scheduler_config
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned();

    // Get timezone from request, or fall back to system default
    // RwLock 中毒 = 曾有线程持锁 panic，继续执行无意义
    #[allow(clippy::unwrap_used)]
    let system_default_tz = state.config.read().unwrap_or_else(|e| e.into_inner()).scheduler_default_timezone.clone();
    let scheduler_timezone = req
        .scheduler_timezone
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned()
        .or(system_default_tz.filter(|s| !s.is_empty()));

    // Validate cron expression if scheduler config is provided
    if let Some(ref config) = scheduler_config {
        validate_cron_expression(config)?;
    }

    // Update scheduler - always call to ensure consistent state
    // When scheduler_enabled is false or config is empty, scheduler will be disabled
    if let Err(e) = state.db.update_todo_scheduler(crate::db::SchedulerUpdate {
        id,
        enabled: scheduler_enabled,
        config: scheduler_config.as_deref(),
        timezone: scheduler_timezone.as_deref(),
    })
    .await {
        tracing::warn!("Failed to update scheduler for todo {}: {}", id, e);
    }

    // todo hook 已整块移除（plan `purring-forging-petal`）：不再处理 inline hooks 列表。
    // 原来的 `update_todo_hooks(id, hooks)` 也已删除（hooks 列随 V23 迁移一起 drop）。

    // auto_review_enabled: 请求中显式指定为 Some(false) 时关闭, 其它情况保持默认 true
    if let Some(false) = req.auto_review_enabled {
        if let Err(e) = state.db.update_todo_auto_review_enabled(id, false).await {
            tracing::warn!("Failed to disable auto_review for todo {}: {}", id, e);
        }
    }

    Ok(ApiResponse::ok(Todo {
        id,
        title: title.to_string(),
        prompt,
        status: crate::models::TodoStatus::Pending,
        created_at: now.clone(),
        updated_at: now,
        tag_ids: req.tag_ids.clone(),
        // executor_name 在创建前已解析（见函数前段），这里直接复用，
        // 与写入 DB 的值是同一个，确保返回值与落库值一致。
        executor: Some(executor_name),
        scheduler_enabled,
        scheduler_config: scheduler_config.clone(),
        scheduler_timezone: scheduler_timezone.clone(),
        scheduler_next_run_at: None,
        task_id: None,
        // cwd 字段保留为 None：handler 已通过 create_todo_with_extras 把 path 同步写入 DB，
        // 这里只对外回传 workspace_id；前端不再消费 workspace_path。
        workspace_path: None,
        workspace_id: Some(req.workspace_id),
        webhook_enabled: req.webhook_enabled.unwrap_or(false),
        acceptance_criteria: req.acceptance_criteria.clone(),
        todo_type: 0,
        parent_todo_id: None,
        review_template_id: None,
        auto_review_enabled: req.auto_review_enabled.unwrap_or(false),
        action_type: req.action_type.clone(),
        action_key: req.action_key.clone(),
        // 新建事项未归档
        archived_at: None,
    }))
}

pub async fn update_todo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateTodoRequest>,
) -> Result<ApiResponse<Todo>, AppError> {
    // 获取当前值用于填充
    let current = state.require_todo(id).await?;

    let title = req.title.unwrap_or_else(|| current.title.clone());
    // None: keep current prompt; Some(empty): fallback to title; Some(value): use value
    let prompt = match req.prompt {
        Some(p) => {
            let p = p.trim();
            if p.is_empty() {
                title.clone()
            } else {
                p.to_string()
            }
        }
        None => current.prompt.clone(),
    };
    let new_status = req.status.unwrap_or(current.status);
    let executor = req.executor.or(current.executor);
    // 工作空间切换是可选的；只有显式传 workspace_id 才把 id + 解析得到的 path 一并下传。
    // 不传则保持原工作空间不变——DAO 端 workspace_id=None 时不动这一列。
    let new_workspace_id = req.workspace_id;
    let new_workspace_path: Option<String> = if let Some(wid) = new_workspace_id {
        let dir = state
            .db
            .get_project_directory_by_id(wid)
            .await?
            .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", wid)))?;
        Some(dir.path)
    } else {
        None
    };

    let scheduler_config = req
        .scheduler_config
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned();

    // Get timezone: req > existing > system default
    // RwLock 中毒 = 曾有线程持锁 panic，继续执行无意义
    #[allow(clippy::unwrap_used)]
    let system_default_tz = state.config.read().unwrap_or_else(|e| e.into_inner()).scheduler_default_timezone.clone();
    let existing_tz = current.scheduler_timezone.clone();
    let scheduler_timezone = req
        .scheduler_timezone
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned()
        .or_else(|| existing_tz.filter(|s| !s.is_empty()))
        .or_else(|| system_default_tz.filter(|s| !s.is_empty()));

    // Validate cron expression if scheduler config is provided
    if let Some(ref config) = scheduler_config {
        validate_cron_expression(config)?;
    }
    state
        .db
        .update_todo_full(TodoUpdate {
            id,
            title: &title,
            prompt: &prompt,
            status: new_status,
            executor: executor.as_deref(),
            scheduler_enabled: req.scheduler_enabled,
            scheduler_config: scheduler_config.as_deref(),
            scheduler_timezone: scheduler_timezone.as_deref(),
            workspace_id: new_workspace_id,
            webhook_enabled: req.webhook_enabled,
            acceptance_criteria: req.acceptance_criteria.as_deref(),
            auto_review_enabled: req.auto_review_enabled,
            action_type: req.action_type.as_deref(),
            action_key: req.action_key.as_deref(),
        })
        .await
        .map_err(AppError::from)?;

    // 工作空间双字段同步：TodoUpdate 只写了 workspace_id，cwd path 由 update_todo_workspace 单独补齐
    // （handler 已经按 id 查到 path，避免 DAO 再做反查）。
    if let (Some(wid), Some(wpath)) = (new_workspace_id, new_workspace_path.as_deref()) {
        state
            .db
            .update_todo_workspace(id, Some(wid), Some(wpath))
            .await
            .map_err(AppError::from)?;
    }

    // todo hook 已整块移除（plan `purring-forging-petal`）：todo 不再持有
    // inline hooks 列表，也不会在状态变化时 fire 任何 hook。原先的两步
    // 「更新 hooks 列 / 异步 fire state-change 钩子」已经全部移除。

    let todo = state.require_todo(id).await?;
    Ok(ApiResponse::ok(todo))
}

pub async fn update_todo_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateTagsRequest>,
) -> Result<ApiResponse<()>, AppError> {
    // 先查询之前关联的 tag（用于计算新增的 tag）
    let old_tag_ids: std::collections::HashSet<i64> = state.db.get_todo_tag_ids(id).await.unwrap_or_default().into_iter().collect();
    state.db.set_todo_tags(id, &req.tag_ids).await?;
    // Loop Studio: 对每个新增的 tag 派发 tag_added 触发器
    if let Some(dispatcher) = state.loop_trigger_dispatcher.as_ref() {
        for &tag_id in &req.tag_ids {
            if !old_tag_ids.contains(&tag_id) {
                // 只派发新增的 tag（已存在的 tag 不重复触发）
                let _ = dispatcher.dispatch_tag_added(tag_id, id).await;
            }
        }
    }
    Ok(ApiResponse::ok(()))
}

pub async fn delete_todo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<()>, AppError> {
    // Get todo info before deletion for hooks
    state.db.get_todo(id).await?;

    // 引用校验：被 loop_steps 引用的 todo 不允许直接删除（不区分 enabled）。
    // 关注数据完整性：禁用环节也算引用，否则删后该 step 被重新启用会指向已删除事项
    // （设计文档风险三：「loop_steps.todo_id 引用」校验，无 enabled 限定）。
    let loop_ref_count = state.db.count_loop_steps_by_todo(id).await?;
    if loop_ref_count > 0 {
        return Err(AppError::BadRequest(format!(
            "该事项被 {loop_ref_count} 个 Loop 环节引用，请先到 Loop 编辑页移除引用后再删除"
        )));
    }

    // 先清理调度器任务（如果有）
    state.scheduler.remove_task_for_todo(id).await;

    // 如果 todo 正在执行，尝试取消
    if let Ok(Some(todo)) = state.db.get_todo(id).await {
        if let Some(task_id) = todo.task_id {
            state.task_manager.cancel(&task_id).await;
        }
    }

    state.db.delete_todo(id).await.map_err(AppError::from)?;

    Ok(ApiResponse::ok(()))
}

pub async fn force_update_todo_status(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateTodoRequest>,
) -> Result<ApiResponse<Todo>, AppError> {
    if let Some(new_status) = req.status {
        state
            .db
            .force_update_todo_status(id, new_status)
            .await
            .map_err(AppError::from)?;

        // todo hook 已整块移除：不再在状态变化时 fire state-change 钩子。
    }
    let todo = state.require_todo(id).await?;
    Ok(ApiResponse::ok(todo))
}

#[derive(Deserialize)]
pub struct RecentCompletedParams {
    #[serde(default = "default_recent_hours")]
    pub hours: u32,
    /// 按工作空间 ID 过滤；不传则返回全部工作空间的已完成 todo。
    #[serde(default)]
    pub workspace_id: Option<i64>,
}

fn default_recent_hours() -> u32 {
    24
}

pub async fn get_recent_completed_todos(
    State(state): State<AppState>,
    Query(params): Query<RecentCompletedParams>,
) -> Result<ApiResponse<Vec<RecentCompletedTodo>>, AppError> {
    let hours = params.hours.clamp(1, 720);
    Ok(ApiResponse::ok(
        state.db.get_recent_completed_todos(hours, params.workspace_id).await?,
    ))
}

/// PUT /api/todos/batch-executor — 批量更新事项执行器
pub async fn batch_update_todos_executor(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<BatchUpdateTodoExecutorRequest>,
) -> Result<ApiResponse<BatchUpdateTodoResult>, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    if req.executor.trim().is_empty() {
        return Err(AppError::BadRequest("executor 不能为空".to_string()));
    }
    let rows_affected = state
        .db
        .batch_update_todos_executor(&req.ids, req.executor.trim())
        .await?;
    Ok(ApiResponse::ok(BatchUpdateTodoResult {
        updated_count: rows_affected as i64,
        total: req.ids.len() as i64,
    }))
}

/// PUT /api/todos/batch-workspace — 批量移动事项到其他工作空间
pub async fn batch_move_todos_workspace(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<BatchUpdateTodoWorkspaceRequest>,
) -> Result<ApiResponse<BatchWorkspaceResult>, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    // handler 把 id 解析为 path 后下传 DAO；DAO 一次写入 workspace_id + workspace_path。
    let dir = state
        .db
        .get_project_directory_by_id(req.workspace_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id)))?;
    let rows_affected = state
        .db
        .batch_update_todos_workspace(&req.ids, req.workspace_id, &dir.path)
        .await?;
    Ok(ApiResponse::ok(BatchWorkspaceResult {
        updated_count: rows_affected as i64,
        total: req.ids.len() as i64,
    }))
}

/// POST /api/todos/batch-copy-workspace — 批量复制事项到其他工作空间
pub async fn batch_copy_todos_workspace(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<BatchCopyTodoWorkspaceRequest>,
) -> Result<ApiResponse<BatchWorkspaceResult>, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    let dir = state
        .db
        .get_project_directory_by_id(req.workspace_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id)))?;
    let created_ids = state
        .db
        .batch_copy_todos_to_workspace(&req.ids, req.workspace_id, &dir.path)
        .await?;
    Ok(ApiResponse::ok(BatchWorkspaceResult {
        updated_count: created_ids.len() as i64,
        total: req.ids.len() as i64,
    }))
}

/// PUT /api/todos/batch-scheduler — 批量暂停/恢复事项的周期执行
///
/// 修复：原实现仅更新 DB 的 `scheduler_enabled` 字段，未同步内存中的 cron 任务，
/// 导致批量暂停后 cron 仍会触发、批量恢复后 cron 不触发的状态不一致。
/// 此处在 DB 写入前后同步调用 scheduler，与单个 `update_scheduler` handler
/// (handlers/scheduler.rs) 的行为保持一致：
/// - 暂停：先逐个移除 cron，再写 DB（避免撤 cron 前的触发窗口）
/// - 恢复：先写 DB，再从 DB 读出 config/timezone 重新注册 cron
pub async fn batch_update_todos_scheduler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<BatchUpdateTodoSchedulerRequest>,
) -> Result<ApiResponse<BatchWorkspaceResult>, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    // 根据目标状态分流到对应辅助函数，保持 handler 单一职责、控制函数长度。
    // 两个分支各自用 `?` 解包 Result，统一得到 rows_affected: u64。
    let rows_affected = if req.scheduler_enabled {
        resume_batch_schedulers(&state, &req.ids).await?
    } else {
        pause_batch_schedulers(&state, &req.ids).await?
    };
    Ok(ApiResponse::ok(BatchWorkspaceResult {
        updated_count: rows_affected as i64,
        total: req.ids.len() as i64,
    }))
}

/// 批量暂停：先逐个移除 cron 任务，再一次写 DB（scheduler_enabled=false）。
/// 顺序很重要——先撤 cron 可避免"DB 已标记暂停但 cron 仍触发"的短窗口。
/// 即使某个 id 在 job_map 中不存在，`remove_task_for_todo` 也是 no-op，安全。
async fn pause_batch_schedulers(state: &AppState, ids: &[i64]) -> Result<u64, AppError> {
    for id in ids {
        state.scheduler.remove_task_for_todo(*id).await;
    }
    let rows = state.db.batch_update_todos_scheduler(ids, false).await?;
    Ok(rows)
}

/// 批量恢复：先写 DB（scheduler_enabled=true），再逐个从 DB 读出 todo 的
/// scheduler_config 与 scheduler_timezone 重新注册 cron。
/// 单个 todo 注册失败只 warn 不中断，避免一个无效 cron 导致整批回滚。
async fn resume_batch_schedulers(state: &AppState, ids: &[i64]) -> Result<u64, AppError> {
    let rows = state.db.batch_update_todos_scheduler(ids, true).await?;
    // DB 写返回 0 行说明 ids 全无效：跳过 cron 重新注册循环，
    // 避免对不存在的 todo 触发 N 次 get_todo 失败 + warn 噪音。
    if rows == 0 {
        return Ok(rows);
    }
    let ctx = ServiceContext {
        db: state.db.clone(),
        executor_registry: state.executor_registry.clone(),
        tx: state.tx.clone(),
        task_manager: state.task_manager.clone(),
        config: state.config.clone(),
    };
    for id in ids {
        // 单个 todo 注册失败只 warn 不中断整批：DB 已置 enabled=true，
        // 进程重启时 load_from_db 会基于 DB 字段再次尝试注册（自愈路径）。
        // AppError 未实现 Display，用 Debug 格式化保留错误上下文。
        if let Err(e) = try_resume_one_scheduler(state, &ctx, *id).await {
            tracing::warn!("批量恢复调度时 todo {} 注册失败: {:?}", id, e);
        }
    }
    Ok(rows)
}

/// 单个 todo 的恢复注册：从 DB 读出 scheduler_config 与 scheduler_timezone，
/// 调用 scheduler.upsert_task。config 为空时仅移除残留 cron（与单个 handler
/// 中 "enabled 但无 config" 分支一致），避免留下无表达式但标记 enabled 的脏状态。
async fn try_resume_one_scheduler(
    state: &AppState,
    ctx: &ServiceContext,
    id: i64,
) -> Result<(), AppError> {
    let todo = state
        .db
        .get_todo(id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("todo {} 不存在", id)))?;
    let config = match todo
        .scheduler_config
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        Some(c) => c.to_string(),
        None => {
            // 没有 cron 表达式无法注册，确保内存中无残留 cron 后返回。
            state.scheduler.remove_task_for_todo(id).await;
            return Ok(());
        }
    };
    state
        .scheduler
        .upsert_task(ctx, id, config, todo.scheduler_timezone.clone())
        .await?;
    Ok(())
}
