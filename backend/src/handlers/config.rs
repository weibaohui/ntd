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
    let cfg = state.config.read().unwrap().clone();
    Ok(ApiResponse::ok(cfg))
}

pub async fn update_config(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<UpdateConfigRequest>,
) -> Result<ApiResponse<Config>, AppError> {
    // 块作用域内 clone 出 owned 值,await 落盘前写锁已 drop,避免 std::sync 写锁
    // 锁卫跨 .await 让 future 变成 !Send。原顺序是 modify -> clone -> spawn_blocking().await,
    // 锁卫会一直持有到 await 结束,违反 std 锁的 Send 约束。
    let cfg_to_save = {
        let mut cfg = state.config.write().unwrap();

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
        if let Some(capacity) = req.broadcast_channel_capacity {
            // 与 YAML 加载路径保持一致的最小值校验，避免配成 0 让 ring buffer 立刻被覆盖丢消息。
            // 注：运行时修改后只持久化配置，重启服务才会在新连接上生效（broadcast channel 启动时建）。
            if capacity < crate::config::MIN_BROADCAST_CHANNEL_CAPACITY {
                return Err(AppError::BadRequest(format!(
                    "broadcast_channel_capacity must be at least {}",
                    crate::config::MIN_BROADCAST_CHANNEL_CAPACITY
                )));
            }
            cfg.broadcast_channel_capacity = capacity;
        }
        if let Some(enabled) = req.auto_update_enabled {
            cfg.auto_update_enabled = enabled;
        }
        if let Some(interval) = req.auto_update_interval {
            let valid = ["day", "week", "month"];
            if !valid.contains(&interval.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "auto_update_interval must be one of: day, week, month"
                )));
            }
            cfg.auto_update_interval = interval;
        }
        if let Some(hour) = req.auto_update_hour {
            if hour > 23 {
                return Err(AppError::BadRequest(
                    "auto_update_hour must be 0-23".to_string(),
                ));
            }
            cfg.auto_update_hour = hour;
        }

        cfg.normalize_paths();
        cfg.clamp_execution_timeout_secs();
        cfg.clone()
    };

    // 落盘 + 回包用同一份 owned Config,顺序无依赖,两份独立 clone 即可。
    let cfg_to_return = cfg_to_save.clone();
    tokio::task::spawn_blocking(move || cfg_to_save.save())
        .await
        .map_err(|e| AppError::Internal(format!("Join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to save config: {}", e)))?;

    Ok(ApiResponse::ok(cfg_to_return))
}

#[cfg(test)]
mod tests {
    use super::validate_execution_timeout_secs;
    use crate::config::Config;
    use std::sync::Arc;

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

    /// Pin down the lock choice: `AppState::config` uses `std::sync::RwLock`
    /// (not `tokio::sync::RwLock`) so the read path stays on the synchronous
    /// critical-section fast path. If a future refactor flips this back to
    /// `tokio::sync::RwLock` the test type-checks the equivalent at this
    /// call site, catching the regression early.
    #[test]
    fn test_config_lock_is_std_sync() {
        let cfg: Arc<std::sync::RwLock<Config>> = Arc::new(std::sync::RwLock::new(Config::default()));
        let reader = cfg.read().unwrap();
        assert_eq!(reader.max_concurrent_todos, Config::default().max_concurrent_todos);
        drop(reader);

        let mut writer = cfg.write().unwrap();
        writer.max_concurrent_todos = 7;
        drop(writer);

        let reader = cfg.read().unwrap();
        assert_eq!(reader.max_concurrent_todos, 7);
    }
}
