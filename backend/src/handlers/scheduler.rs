use axum::{
    extract::{Path, State},
};

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, Todo, UpdateSchedulerRequest};
use crate::service_context::ServiceContext;

#[axum::debug_handler]
pub async fn update_scheduler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateSchedulerRequest>,
) -> Result<ApiResponse<Todo>, AppError> {
    // Get existing todo to preserve its timezone if not provided
    let existing_todo = state.db.get_todo(id).await?;
    let existing_tz = existing_todo.as_ref().and_then(|t| t.scheduler_timezone.clone());

    // Get system default timezone
    let system_default_tz = state.config.read().await.scheduler_default_timezone.clone();

    // Determine final timezone: req > existing > system default
    let scheduler_timezone = req
        .scheduler_timezone
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned()
        .or_else(|| existing_tz.filter(|s| !s.is_empty()))
        .or_else(|| system_default_tz.filter(|s| !s.is_empty()));

    if req.scheduler_enabled {
        if let Some(ref config) = req.scheduler_config {
            let ctx = ServiceContext {
                db: state.db.clone(),
                executor_registry: state.executor_registry.clone(),
                tx: state.tx.clone(),
                task_manager: state.task_manager.clone(),
                config: state.config.clone(),
            };
            match state
                .scheduler
                .upsert_task(
                    &ctx,
                    id,
                    config.clone(),
                    scheduler_timezone.clone(),
                )
                .await
            {
                Ok(_) => {
                    state
                        .db
                        .update_todo_scheduler(crate::db::SchedulerUpdate { id, enabled: req.scheduler_enabled, config: req.scheduler_config.as_deref(), timezone: scheduler_timezone.as_deref() })
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
                .update_todo_scheduler(crate::db::SchedulerUpdate { id, enabled: req.scheduler_enabled, config: req.scheduler_config.as_deref(), timezone: scheduler_timezone.as_deref() })
                .await
                .map_err(AppError::from)?;
        }
    } else {
        state.scheduler.remove_task_for_todo(id).await;
        state
            .db
            .update_todo_scheduler(crate::db::SchedulerUpdate { id, enabled: req.scheduler_enabled, config: req.scheduler_config.as_deref(), timezone: scheduler_timezone.as_deref() })
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
