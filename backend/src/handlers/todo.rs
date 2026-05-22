use axum::extract::{Path, Query, State};
use cron::Schedule;
use serde::Deserialize;
use std::str::FromStr;

use crate::db::TodoUpdate;
use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{
    utc_timestamp, ApiResponse, CreateTodoRequest, RecentCompletedTodo, Todo, UpdateTagsRequest,
    UpdateTodoRequest,
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

pub async fn get_todos(State(state): State<AppState>) -> Result<ApiResponse<Vec<Todo>>, AppError> {
    Ok(ApiResponse::ok(state.db.get_todos().await?))
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
    let id = state.db.create_todo(title, &prompt).await?;

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
    let system_default_tz = state.config.read().await.scheduler_default_timezone.clone();
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
    if let Err(e) = state.db.update_todo_scheduler(id, scheduler_enabled, scheduler_config.as_deref(), scheduler_timezone.as_deref()).await {
        tracing::warn!("Failed to update scheduler for todo {}: {}", id, e);
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
    let status = req.status.unwrap_or(current.status);
    let executor = req.executor.or(current.executor);
    let workspace = req.workspace.or(current.workspace);
    let worktree_enabled = req.worktree_enabled.unwrap_or(current.worktree_enabled);

    let scheduler_config = req
        .scheduler_config
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned();

    // Get timezone from request, or fall back to system default
    let system_default_tz = state.config.read().await.scheduler_default_timezone.clone();
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
    state
        .db
        .update_todo_full(TodoUpdate {
            id,
            title: &title,
            prompt: &prompt,
            status,
            executor: executor.as_deref(),
            scheduler_enabled: req.scheduler_enabled,
            scheduler_config: scheduler_config.as_deref(),
            scheduler_timezone: scheduler_timezone.as_deref(),
            workspace: workspace.as_deref(),
            worktree_enabled: Some(worktree_enabled),
        })
        .await
        .map_err(AppError::from)?;

    let todo = state.require_todo(id).await?;
    Ok(ApiResponse::ok(todo))
}

pub async fn update_todo_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateTagsRequest>,
) -> Result<ApiResponse<()>, AppError> {
    state.db.set_todo_tags(id, &req.tag_ids).await?;
    Ok(ApiResponse::ok(()))
}

pub async fn delete_todo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<()>, AppError> {
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
    if let Some(status) = req.status {
        state
            .db
            .force_update_todo_status(id, status)
            .await
            .map_err(AppError::from)?;
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
