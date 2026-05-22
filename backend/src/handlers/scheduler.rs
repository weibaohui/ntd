use axum::{
    extract::{Path, State},
};

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, Todo, UpdateSchedulerRequest};

#[axum::debug_handler]
pub async fn update_scheduler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateSchedulerRequest>,
) -> Result<ApiResponse<Todo>, AppError> {
    // Get timezone from request, or fall back to system default
    let system_default_tz = state.config.read().await.scheduler_default_timezone.clone();
    let scheduler_timezone = req
        .scheduler_timezone
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned()
        .or(system_default_tz.filter(|s| !s.is_empty()));

    if req.scheduler_enabled {
        if let Some(ref config) = req.scheduler_config {
            match state
                .scheduler
                .upsert_task(
                    state.db.clone(),
                    state.executor_registry.clone(),
                    state.tx.clone(),
                    id,
                    config.clone(),
                    scheduler_timezone.clone(),
                    state.task_manager.clone(),
                    state.config.clone(),
                )
                .await
            {
                Ok(_) => {
                    state
                        .db
                        .update_todo_scheduler(id, req.scheduler_enabled, req.scheduler_config.as_deref(), scheduler_timezone.as_deref())
                        .await
                        .map_err(AppError::from)?;
                }
                Err(e) => {
                    tracing::error!("Failed to upsert scheduled task for todo {}: {}", id, e);
                    return Err(AppError::Internal(format!("Failed to upsert scheduled task: {}", e)));
                }
            }
        } else {
            state.scheduler.remove_task_for_todo(id).await;
            state
                .db
                .update_todo_scheduler(id, req.scheduler_enabled, req.scheduler_config.as_deref(), scheduler_timezone.as_deref())
                .await
                .map_err(AppError::from)?;
        }
    } else {
        state.scheduler.remove_task_for_todo(id).await;
        state
            .db
            .update_todo_scheduler(id, req.scheduler_enabled, req.scheduler_config.as_deref(), scheduler_timezone.as_deref())
            .await
            .map_err(AppError::from)?;
    }

    let todo = state.require_todo(id).await?;
    Ok(ApiResponse::ok(todo))
}

pub async fn get_scheduler_todos(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<Todo>>, AppError> {
    Ok(ApiResponse::ok(state.db.get_scheduler_todos().await?))
}
