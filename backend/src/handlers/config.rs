use axum::extract::State;

use crate::config::Config;
use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, UpdateConfigRequest};

/// 校验执行超时配置，允许 0 表示不限制执行时长，其余值至少为 60 秒，最多 7 天。
fn validate_execution_timeout_secs(execution_timeout_secs: u64) -> Result<(), AppError> {
    if execution_timeout_secs != 0 && execution_timeout_secs < 60 {
        return Err(AppError::BadRequest(
            "execution_timeout_secs must be 0 or at least 60".to_string(),
        ));
    }
    if execution_timeout_secs > crate::config::MAX_EXECUTION_TIMEOUT_SECS {
        return Err(AppError::BadRequest(
            format!("execution_timeout_secs must be at most {}", crate::config::MAX_EXECUTION_TIMEOUT_SECS),
        ));
    }
    Ok(())
}

pub async fn get_config(State(state): State<AppState>) -> Result<ApiResponse<Config>, AppError> {
    let cfg = state.config.read().await.clone();
    Ok(ApiResponse::ok(cfg))
}

pub async fn update_config(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<UpdateConfigRequest>,
) -> Result<ApiResponse<Config>, AppError> {
    let mut cfg = state.config.write().await;

    if let Some(port) = req.port {
        cfg.port = port;
    }
    if let Some(host) = req.host {
        cfg.host = host;
    }
    if let Some(db_path) = req.db_path {
        cfg.db_path = db_path;
    }
    if let Some(log_level) = req.log_level {
        cfg.log_level = log_level;
    }
    if let Some(slash_command_rules) = req.slash_command_rules {
        cfg.slash_command_rules = slash_command_rules;
    }
    if let Some(default_response_todo_id) = req.default_response_todo_id {
        cfg.default_response_todo_id = Some(default_response_todo_id);
    }
    if let Some(history_message_max_age_secs) = req.history_message_max_age_secs {
        cfg.history_message_max_age_secs = history_message_max_age_secs;
    }
    if let Some(max_concurrent_todos) = req.max_concurrent_todos {
        if max_concurrent_todos == 0 {
            return Err(AppError::BadRequest("max_concurrent_todos must be at least 1".to_string()));
        }
        cfg.max_concurrent_todos = max_concurrent_todos;
    }
    if let Some(execution_timeout_secs) = req.execution_timeout_secs {
        validate_execution_timeout_secs(execution_timeout_secs)?;
        cfg.execution_timeout_secs = execution_timeout_secs;
    }
    if let Some(scheduler_default_timezone) = req.scheduler_default_timezone {
        let tz = scheduler_default_timezone.trim();
        cfg.scheduler_default_timezone = if tz.is_empty() {
            None
        } else {
            Some(tz.to_string())
        };
    }

    cfg.normalize_paths();
    cfg.clamp_execution_timeout_secs();

    let cfg_clone = cfg.clone();
    tokio::task::spawn_blocking(move || cfg_clone.save())
        .await
        .map_err(|e| AppError::Internal(format!("Join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to save config: {}", e)))?;

    Ok(ApiResponse::ok(cfg.clone()))
}

#[cfg(test)]
mod tests {
    use super::validate_execution_timeout_secs;

    #[test]
    fn test_validate_execution_timeout_accepts_zero() {
        assert!(validate_execution_timeout_secs(0).is_ok());
    }

    #[test]
    fn test_validate_execution_timeout_rejects_small_positive_value() {
        assert!(validate_execution_timeout_secs(59).is_err());
    }

    #[test]
    fn test_validate_execution_timeout_accepts_minimum_positive_value() {
        assert!(validate_execution_timeout_secs(60).is_ok());
    }

    #[test]
    fn test_validate_execution_timeout_accepts_maximum_value() {
        assert!(validate_execution_timeout_secs(crate::config::MAX_EXECUTION_TIMEOUT_SECS).is_ok());
    }

    #[test]
    fn test_validate_execution_timeout_rejects_exceeds_maximum() {
        assert!(validate_execution_timeout_secs(crate::config::MAX_EXECUTION_TIMEOUT_SECS + 1).is_err());
    }
}
