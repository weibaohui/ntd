use axum::extract::{Path, Query, State};
use cron::Schedule;
use serde::Deserialize;
use std::str::FromStr;

use crate::db::TodoUpdate;
use crate::handlers::{ApiJson, AppError, AppState};
use crate::hooks::models::HookContext;
use crate::models::{
    utc_timestamp, ApiResponse, BatchUpdateTodoExecutorRequest, BatchUpdateTodoResult,
    CreateTodoRequest, RecentCompletedTodo, StepDto, Todo, UpdateTagsRequest, UpdateTodoRequest,
};

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

pub async fn get_todos(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<TodoListQuery>,
) -> Result<ApiResponse<Vec<Todo>>, AppError> {
    // kind=item / kind=step / kind=all(默认) 三种过滤。
    // 保持向后兼容：未传参时返回所有 todo。
    let todos = match params.kind.as_deref() {
        Some("item") => state.db.list_todos_by_kind("item").await?,
        Some("step") => state.db.list_todos_by_kind("step").await?,
        _ => state.db.get_todos().await?,
    };
    Ok(ApiResponse::ok(todos))
}

#[derive(Debug, serde::Deserialize)]
pub struct TodoListQuery {
    #[serde(default)]
    pub kind: Option<String>,
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
    let executor = req
        .executor
        .clone()
        .unwrap_or_else(|| "claudecode".to_string());

    let id = state.db.create_todo_with_extras(
        title,
        &prompt,
        Some(&executor),
        req.acceptance_criteria.as_deref(),
    ).await?;

    // Update executor if specified
    if let Some(ref exec) = req.executor {
        if let Err(e) = state.db.update_todo_executor(id, exec).await {
            tracing::warn!("Failed to update executor for todo {}: {}", id, e);
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
    let system_default_tz = state.config.read().unwrap().scheduler_default_timezone.clone();
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

    // Persist inline hooks if the caller supplied them. Skipping the call when
    // `None` means we keep the default empty list, which matches what was
    // there before this field was added.
    if let Some(ref hooks) = req.hooks {
        if let Err(e) = state.db.update_todo_hooks(id, hooks).await {
            tracing::warn!("Failed to set initial hooks for todo {}: {}", id, e);
        }
    }

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
        executor: Some(executor),
        scheduler_enabled,
        scheduler_config: scheduler_config.clone(),
        scheduler_timezone: scheduler_timezone.clone(),
        scheduler_next_run_at: None,
        task_id: None,
        workspace: None,
        worktree_enabled: false,
        hooks: req.hooks.clone().unwrap_or_default(),
        acceptance_criteria: req.acceptance_criteria.clone(),
        todo_type: 0,
        parent_todo_id: None,
        auto_review_enabled: req.auto_review_enabled.unwrap_or(true),
        // 新建 todo 默认 kind='item'；如需环节，由 promote 接口或新建时显式指定。
        kind: "item".to_string(),
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
    let workspace = req.workspace.or(current.workspace);
    let worktree_enabled = req.worktree_enabled.unwrap_or(current.worktree_enabled);

    // Check if status is actually changing
    let status_changed = req.status.is_some() && req.status.unwrap() != current.status;

    let scheduler_config = req
        .scheduler_config
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned();

    // Get timezone: req > existing > system default
    let system_default_tz = state.config.read().unwrap().scheduler_default_timezone.clone();
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
            workspace: workspace.as_deref(),
            worktree_enabled: Some(worktree_enabled),
            acceptance_criteria: req.acceptance_criteria.as_deref(),
            auto_review_enabled: req.auto_review_enabled,
        })
        .await
        .map_err(AppError::from)?;

    if let Some(ref hooks) = req.hooks {
        state
            .db
            .update_todo_hooks(id, hooks)
            .await
            .map_err(AppError::from)?;
    }

    // Fire state-change hooks (asynchronously)
    if status_changed {
        let old_status = current.status;
        if let Some(ctx) = HookContext::for_state_change(
            id,
            title.clone(),
            old_status,
            new_status,
            executor.clone(),
            workspace.clone(),
            vec![id],
        ) {
            state.hook_service.clone().fire_for_todo(id, ctx);
        }
    }

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
        let current = state.require_todo(id).await?;
        let old_status = current.status;

        state
            .db
            .force_update_todo_status(id, new_status)
            .await
            .map_err(AppError::from)?;

        // Fire state-change hooks (asynchronously)
        if old_status != new_status {
            if let Some(ctx) = HookContext::for_state_change(
                id,
                current.title.clone(),
                old_status,
                new_status,
                current.executor.clone(),
                current.workspace.clone(),
                vec![id],
            ) {
                state.hook_service.clone().fire_for_todo(id, ctx);
            }
        }
    }
    let todo = state.require_todo(id).await?;
    Ok(ApiResponse::ok(todo))
}

#[derive(Deserialize)]
pub struct RecentCompletedParams {
    #[serde(default = "default_recent_hours")]
    pub hours: u32,
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
        state.db.get_recent_completed_todos(hours).await?,
    ))
}

// ====== todo → step 提升 ======
// 注意：环节 CRUD 已在 step_.rs 中，todo.rs 只保留 promote。

/// POST /api/todos/:id/promote — 事项提升为环节（复制数据到 steps 表，原 todo 保留）
pub async fn promote_todo_to_step(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<StepDto>, AppError> {
    let todo = state
        .db
        .get_todo(id)
        .await?
        .ok_or(AppError::NotFound)?;
    let step = state
        .db
        .create_step(
            &todo.title,
            &todo.prompt,
            todo.executor.as_deref(),
            todo.acceptance_criteria.as_deref(),
            Some(todo.id),
            None,
        )
        .await?;
    Ok(ApiResponse::ok(StepDto::from(step)))
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
