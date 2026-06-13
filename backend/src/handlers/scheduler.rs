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
            // PR #544 review HIGH #1 修复: 旧实现无差别 `warn!`,导致 500
            // 类错误(Database / Internal)被 `RUST_LOG=error` 过滤器漏掉。
            // 现在按 variant 分级: 用户输入错 → warn(预期,运维不需告警),
            // 内部错 → error(需进入 Sentry / Loki error 告警)。
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
                use crate::scheduler::SchedulerError;
                // PR #544 review HIGH #1 修复: 旧实现无差别 `warn!`,导致 500
                // 类错误(Database / Internal)被 `RUST_LOG=error` 过滤器漏掉。
                // 现在按 variant 分级: 用户输入错 → warn(预期,运维不需告警),
                // 内部错 → error(需进入 Sentry / Loki error 告警)。
                //
                // 用 match 调对应 macro 而不是 `tracing::event!(level, ...)` —
                // `event!` 要求 level 是 const token 而非 runtime value(E0435)。
                match &e {
                    SchedulerError::InvalidCron(_)
                    | SchedulerError::InvalidTimezone(_) => {
                        tracing::warn!(
                            "Failed to upsert scheduled task for todo {} (cron='{}', tz={:?}): {}",
                            id, config, scheduler_timezone, e
                        );
                    }
                    SchedulerError::Database(_)
                    | SchedulerError::Internal(_) => {
                        tracing::error!(
                            "Failed to upsert scheduled task for todo {} (cron='{}', tz={:?}): {}",
                            id, config, scheduler_timezone, e
                        );
                    }
                }
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
