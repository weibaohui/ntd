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
    let system_default_tz = state.config.read().unwrap().scheduler_default_timezone.clone();

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
            // upsert_task 返回 `SchedulerError`,通过
            // `From<SchedulerError> for AppError` 自动映射:
            // InvalidCron / InvalidTimezone → 400,其它 → 500。
            //
            // 之前实现是把任何 error 都强行塞成 500("Failed to upsert..."),
            // 用户的 cron 写错了也变成服务器错误,前端无法做差异化提示。
            // 这里保留 warn 日志方便排查,但 HTTP 状态码交给 From 决定。
            if let Err(e) = state
                .scheduler
                .upsert_task(
                    &ctx,
                    id,
                    config.clone(),
                    scheduler_timezone.clone(),
                )
                .await
            {
                tracing::warn!(
                    "Failed to upsert scheduled task for todo {}: {}",
                    id, e
                );
                return Err(AppError::from(e));
            }
            state
                .db
                .update_todo_scheduler(crate::db::SchedulerUpdate { id, enabled: req.scheduler_enabled, config: req.scheduler_config.as_deref(), timezone: scheduler_timezone.as_deref() })
                .await?;
        } else {
            state.scheduler.remove_task_for_todo(id).await;
            state
                .db
                .update_todo_scheduler(crate::db::SchedulerUpdate { id, enabled: req.scheduler_enabled, config: req.scheduler_config.as_deref(), timezone: scheduler_timezone.as_deref() })
                .await?;
        }
    } else {
        state.scheduler.remove_task_for_todo(id).await;
        state
            .db
            .update_todo_scheduler(crate::db::SchedulerUpdate { id, enabled: req.scheduler_enabled, config: req.scheduler_config.as_deref(), timezone: scheduler_timezone.as_deref() })
            .await?;
    }

    let todo = state.require_todo(id).await?;
    Ok(ApiResponse::ok(todo))
}

pub async fn get_scheduler_todos(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<Todo>>, AppError> {
    Ok(ApiResponse::ok(state.db.get_scheduler_todos().await?))
}
