//! Unified configuration management.
//!
//! Config file location: `~/.ntd/config.yaml` or `~/.ntd/config.dev.yaml` (when NTD_MODE=dev)
//!
//! All components (server, CLI, executors) read their settings from this module.
//! No direct environment variable reads — route everything through Config.

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Default port.
pub const DEFAULT_PORT: u16 = 8088;
/// Dev mode port.
pub const DEFAULT_DEV_PORT: u16 = 18088;
/// Default host.
pub const DEFAULT_HOST: &str = "0.0.0.0";
/// Default executor paths (binary names).
pub const DEFAULT_EXECUTOR_PATH: &str = ""; // use binary name directly
/// 默认执行超时时间（秒）。
pub const DEFAULT_EXECUTION_TIMEOUT_SECS: u64 = 3600;
/// 执行超时上限（秒）：7 天。YAML 加载和 HTTP update 均受此约束。
pub const MAX_EXECUTION_TIMEOUT_SECS: u64 = 604800;
/// WebSocket broadcast channel 默认容量。
///
/// 原先硬编码为 100，但 AI 执行器（如 claude_code）的 `Output` 事件频率可达每秒数十条，
/// 100 的容量在 1~2 秒延迟下就会因 ring buffer 覆盖而丢消息，导致前端日志断断续续、
/// `Finished` 等关键事件丢失。10000 大约能覆盖 200s@50msg/s 或 100s@100msg/s 的 burst，
/// 对绝大多数使用场景足够；同时 broadcast 是 ring buffer，内存开销仅 O(capacity)。
pub const DEFAULT_BROADCAST_CHANNEL_CAPACITY: usize = 10000;
/// WebSocket broadcast channel 最小容量。低于此值视为配置错误，自动抬升以避免退化到丢消息。
pub const MIN_BROADCAST_CHANNEL_CAPACITY: usize = 100;
/// WebSocket broadcast channel 软上限。超过此值时记录 warn 但不强制截断，
/// 保留运维人员根据实际负载调大的自主权。约 1M events × <1KB ≈ 1GB ceiling。
pub const SOFT_MAX_BROADCAST_CHANNEL_CAPACITY: usize = 1_000_000;

/// Top-level configuration, persisted to `~/.ntd/config.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Server port (default: 8088)
    pub port: u16,
    /// Server host (default: 0.0.0.0)
    pub host: String,
    /// Database file path (default: ~/.ntd/data.db)
    pub db_path: String,
    /// Log level (default: INFO)
    pub log_level: String,
    /// Executor binary paths (kept for backward-compat migration, not serialized to config.yaml)
    #[serde(skip_serializing)]
    pub executors: ExecutorPaths,
    /// 是否开启自动数据库备份
    pub auto_backup_enabled: bool,
    /// 自动备份 cron 表达式（6 字段，含秒）
    pub auto_backup_cron: String,
    /// 自动备份最大保留文件数
    pub auto_backup_max_files: usize,
    /// 是否开启 Todo 自动备份
    pub auto_todo_backup_enabled: bool,
    /// Todo 自动备份 cron 表达式（6 字段，含秒）
    pub auto_todo_backup_cron: String,
    /// Todo 自动备份最大保留文件数
    pub auto_todo_backup_max_files: usize,
    /// 是否开启自定义模板自动同步
    pub auto_sync_custom_templates_enabled: bool,
    /// 自定义模板自动同步 cron 表达式（6 字段，含秒）
    pub auto_sync_custom_templates_cron: String,
    /// 全局斜杠命令规则
    pub slash_command_rules: Vec<SlashCommandRule>,
    /// 默认响应：当没有匹配到斜杠命令时执行的 Todo ID
    pub default_response_todo_id: Option<i64>,
    /// 历史消息最大处理年龄（秒），超过此时间的历史消息拉取后标记跳过不处理（默认 600 = 10 分钟）
    pub history_message_max_age_secs: u64,
    /// 最大并发执行数（默认 3）
    pub max_concurrent_todos: u32,
    /// 执行超时时间（秒，默认 3600 = 60 分钟）；设置为 0 表示不限制执行时长；上限 604800（7 天）
    pub execution_timeout_secs: u64,
    /// 日志清理保留天数（None 表示不清理）
    pub auto_cleanup_logs_days: Option<usize>,
    /// 是否开启 Skill 自动备份
    pub auto_skill_backup_enabled: bool,
    /// Skill 自动备份 cron 表达式（6 字段，含秒）
    pub auto_skill_backup_cron: String,
    /// Skill 自动备份最大保留文件数
    pub auto_skill_backup_max_files: usize,
    /// 定时任务默认时区（用于 cron 表达式转换）
    pub scheduler_default_timezone: Option<String>,
    /// 是否开启 AI 使用统计自动归档
    pub auto_usage_stats_enabled: bool,
    /// AI 使用统计自动归档 cron 表达式（6 字段，含秒），默认每天凌晨 1 点执行
    pub auto_usage_stats_cron: String,
    /// WebSocket broadcast channel 容量（ring buffer 大小）。
    ///
    /// 影响所有 `/events` WebSocket 订阅者能缓存的最大事件数。当慢消费者
    /// （如刚连上的客户端、暂停的标签页）落后超过此容量时，最旧事件将被覆盖丢弃。
    /// 仅在启动时生效，运行时调整需要重启服务。最小值见 `MIN_BROADCAST_CHANNEL_CAPACITY`。
    pub broadcast_channel_capacity: usize,
    /// 云端同步配置
    pub cloud_sync: CloudSyncConfig,
    /// CORS 允许的来源域名白名单（仅生产模式生效）。
    /// 空列表 = 仅同源请求（不发送 Access-Control-Allow-Origin header）。
    /// 可配置多个域名，如 ["https://app.example.com", "https://admin.example.com"]。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cors_allowed_origins: Vec<String>,
}

/// Paths for each supported executor binary.
/// Key is the executor name (e.g., "claudecode"), value is the binary path.
#[derive(Debug, Clone, Serialize)]
#[serde(default)]
pub struct ExecutorPaths {
    pub paths: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for ExecutorPaths {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawExecutorPaths {
            New { paths: HashMap<String, String> },
            Legacy(HashMap<String, String>),
        }

        let raw = RawExecutorPaths::deserialize(deserializer)?;
        let paths = match raw {
            RawExecutorPaths::New { paths } => paths,
            RawExecutorPaths::Legacy(legacy) => legacy,
        };
        Ok(ExecutorPaths { paths })
    }
}

impl Default for ExecutorPaths {
    fn default() -> Self {
        Self {
            paths: HashMap::new(),
        }
    }
}

/// 全局斜杠命令规则。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlashCommandRule {
    pub slash_command: String,
    pub todo_id: i64,
    pub enabled: bool,
}

impl Default for SlashCommandRule {
    fn default() -> Self {
        Self {
            slash_command: "/todo".to_string(),
            todo_id: 0,
            enabled: true,
        }
    }
}

/// 云端同步配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudSyncConfig {
    /// 云端服务器地址
    pub server_url: String,
    /// 同步 Token (ntd_xxx 格式)
    pub sync_token: Option<String>,
    /// 最后同步时间
    pub last_sync_at: Option<String>,
    /// 默认冲突解决模式
    pub default_conflict_mode: String,
}

impl Default for CloudSyncConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            sync_token: None,
            last_sync_at: None,
            default_conflict_mode: "overwrite".to_string(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            host: DEFAULT_HOST.to_string(),
            db_path: "~/.ntd/data.db".to_string(),
            log_level: "INFO".to_string(),
            executors: ExecutorPaths::default(),
            auto_backup_enabled: false,
            auto_backup_cron: "0 0 3 * * *".to_string(),
            auto_backup_max_files: 30,
            auto_todo_backup_enabled: false,
            auto_todo_backup_cron: "0 0 4 * * *".to_string(),
            auto_todo_backup_max_files: 30,
            auto_sync_custom_templates_enabled: false,
            auto_sync_custom_templates_cron: "0 0 4 * * *".to_string(),
            slash_command_rules: Vec::new(),
            default_response_todo_id: None,
            history_message_max_age_secs: 600,
            max_concurrent_todos: 3,
            execution_timeout_secs: DEFAULT_EXECUTION_TIMEOUT_SECS,
            auto_cleanup_logs_days: Some(30),
            auto_skill_backup_enabled: false,
            auto_skill_backup_cron: "0 0 5 * * *".to_string(),
            auto_skill_backup_max_files: 30,
            scheduler_default_timezone: None,
            auto_usage_stats_enabled: false,
            auto_usage_stats_cron: "0 0 1 * * *".to_string(),
            broadcast_channel_capacity: DEFAULT_BROADCAST_CHANNEL_CAPACITY,
            cloud_sync: CloudSyncConfig::default(),
            cors_allowed_origins: Vec::new(),
        }
    }
}

impl Config {
    /// Load config from `~/.ntd/config.yaml` or `~/.ntd/config.dev.yaml` (when NTD_MODE=dev).
    /// Creates the file with defaults if it doesn't exist.
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            let mut cfg = if Self::is_dev_mode() {
                // Dev mode defaults: different port and database
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                Config {
                    port: DEFAULT_DEV_PORT,
                    db_path: home.join(".ntd").join("data.dev.db").to_string_lossy().to_string(),
                    ..Default::default()
                }
            } else {
                Config::default()
            };
            cfg.normalize_paths();
            cfg.clamp_execution_timeout_secs();
            cfg.clamp_broadcast_channel_capacity();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            // Clone the already-normalized and clamped cfg for serialization to disk
            let cfg_for_save = cfg.clone();
            if let Ok(yaml) = serde_yaml::to_string(&cfg_for_save) {
                if let Err(e) = std::fs::write(&path, yaml) {
                    // 配置写盘失败属于「可降级运行」的警告：进程仍可基于内存中已 clamp
                    // 的 Config 启动，下次再尝试持久化即可。用 tracing::warn! 让运维通过
                    // 日志聚合系统（如 journald / Loki）看到这条事件。
                    tracing::warn!(
                        error = %e,
                        path = %path.display(),
                        "failed to write config file, using in-memory defaults"
                    );
                }
            }
            return cfg;
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let mut cfg = serde_yaml::from_str::<Config>(&content).unwrap_or_else(|e| {
                    // 配置解析失败：fallback 到默认值而不是启动失败。
                    // 选用 warn 而非 error 是因为该路径可以优雅降级。
                    tracing::warn!(
                        error = %e,
                        path = %path.display(),
                        "failed to parse config file, using defaults"
                    );
                    Config::default()
                });
                cfg.normalize_paths();
                cfg.clamp_execution_timeout_secs();
                cfg.clamp_broadcast_channel_capacity();
                cfg
            }
            Err(e) => {
                // 配置读取失败（权限不足 / 文件被锁等）：同样降级到默认值。
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "failed to read config file, using defaults"
                );
                let mut cfg = Config::default();
                cfg.normalize_paths();
                cfg.clamp_execution_timeout_secs();
                cfg.clamp_broadcast_channel_capacity();
                cfg
            }
        }
    }

    /// Normalize paths: expand ~ and relative paths to absolute paths.
    pub fn normalize_paths(&mut self) {
        self.db_path = Self::normalize_single_path(&self.db_path);
        // Normalize executor paths
        let normalized: HashMap<String, String> = self.executors.paths.iter()
            .map(|(k, v)| (k.clone(), Self::normalize_single_path(v)))
            .collect();
        self.executors.paths = normalized;
    }

    /// Clamp execution_timeout_secs to the valid range [60, MAX_EXECUTION_TIMEOUT_SECS].
    ///
    /// **Why this is needed**: HTTP `update_config` validates the timeout via
    /// `validate_execution_timeout_secs`, but users can bypass that by editing
    /// `~/.ntd/config.yaml` directly.  `normalize_paths` / `normalize` is called after
    /// both fresh YAML load and HTTP updates, so this is the single enforcement point
    /// for YAML-level edits.
    ///
    /// **Boundary rationale**:
    /// - `0`: allowed — means "no timeout" (matches `timeout_enabled = v > 0` in executor_service)
    /// - `1-59`: raised to 60 — sub-minute timeouts are impractical (process startup
    ///   overhead alone can exceed 30s) and risk killing normal long-running tasks
    /// - `>MAX`: truncated — values in the hundreds of days indicate config errors;
    ///   capping prevents long-running tasks from indefinitely occupying task slots
    pub fn clamp_execution_timeout_secs(&mut self) {
        if self.execution_timeout_secs != 0 && self.execution_timeout_secs < 60 {
            self.execution_timeout_secs = 60;
        } else if self.execution_timeout_secs > MAX_EXECUTION_TIMEOUT_SECS {
            self.execution_timeout_secs = MAX_EXECUTION_TIMEOUT_SECS;
        }
    }

    /// 把 broadcast_channel_capacity 抬升到最小值 `MIN_BROADCAST_CHANNEL_CAPACITY`；
    /// 超过 `SOFT_MAX_BROADCAST_CHANNEL_CAPACITY` 时发出 warn 但不截断。
    ///
    /// **为什么需要**：YAML 直接编辑可绕过 HTTP 校验把 capacity 写成 0/1/50 等无意义值，
    /// 这种配置等同于「关闭事件流」，会让慢消费者立刻丢失 Finished 等关键事件。
    /// 这里强制把任意小于阈值的值抬升到 `MIN_BROADCAST_CHANNEL_CAPACITY`，保持行为可观察。
    ///
    /// **为什么是软上限**：broadcast 是 ring buffer，capacity 与每条事件大小相关，
    /// 每条 ExecEvent 序列化后通常 < 1KB，100000 条也只占约 100MB。
    /// `SOFT_MAX_BROADCAST_CHANNEL_CAPACITY`(1_000_000) 约对应 1-4GB 内存，
    /// 超过此值记录 warn 提示，但不强制截断，保留运维人员的自主权。
    ///
    /// **调用场景**：YAML/默认值加载路径（真 clamp）和 HTTP `update_config` 路径
    /// （HTTP 路径已显式 reject < MIN，此处为冗余防御）。
    pub fn clamp_broadcast_channel_capacity(&mut self) {
        if self.broadcast_channel_capacity < MIN_BROADCAST_CHANNEL_CAPACITY {
            tracing::warn!(
                "broadcast_channel_capacity {} below minimum {}, raising to minimum",
                self.broadcast_channel_capacity,
                MIN_BROADCAST_CHANNEL_CAPACITY
            );
            self.broadcast_channel_capacity = MIN_BROADCAST_CHANNEL_CAPACITY;
        }
        if self.broadcast_channel_capacity > SOFT_MAX_BROADCAST_CHANNEL_CAPACITY {
            tracing::warn!(
                "broadcast_channel_capacity {} exceeds soft limit {}; this may allocate significant memory",
                self.broadcast_channel_capacity,
                SOFT_MAX_BROADCAST_CHANNEL_CAPACITY
            );
        }
    }

    fn normalize_single_path(path: &str) -> String {
        if path.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                let relative = path.trim_start_matches('~').trim_start_matches(std::path::MAIN_SEPARATOR);
                return home.join(relative).to_string_lossy().to_string();
            }
        } else if !path.is_empty()
            && !PathBuf::from(path).is_absolute()
            && path.contains(std::path::MAIN_SEPARATOR)
        {
            if let Some(home) = dirs::home_dir() {
                let stripped = path.trim_start_matches("./");
                return home.join(stripped).to_string_lossy().to_string();
            }
        }
        path.to_string()
    }

    /// Get the server URL string, e.g. "http://localhost:8088".
    pub fn server_url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    /// Save config to `~/.ntd/config.yaml`.
    /// Uses atomic write (temp file + rename) to avoid corruption on crash.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
        }
        let yaml = serde_yaml::to_string(self).map_err(|e| format!("Failed to serialize config: {}", e))?;

        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, yaml).map_err(|e| format!("Failed to write temp config: {}", e))?;
        std::fs::rename(&tmp_path, &path).map_err(|e| format!("Failed to rename config: {}", e))?;
        Ok(())
    }

    /// Path to the config file.
    /// When NTD_MODE=dev, loads ~/.ntd/config.dev.yaml instead.
    fn config_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let base = home.join(".ntd");
        if std::env::var("NTD_MODE").as_deref() == Ok("dev") {
            base.join("config.dev.yaml")
        } else {
            base.join("config.yaml")
        }
    }

    /// Check if running in dev mode (NTD_MODE=dev).
    pub fn is_dev_mode() -> bool {
        std::env::var("NTD_MODE").as_deref() == Ok("dev")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_port() {
        let cfg = Config::default();
        assert_eq!(cfg.port, 8088);
    }

    #[test]
    fn test_server_url() {
        let cfg = Config { port: 9090, ..Default::default() };
        assert_eq!(cfg.server_url(), "http://localhost:9090");
    }

    #[test]
    fn test_default_execution_timeout_round_trip() {
        let cfg = Config::default();
        // 验证序列化/反序列化后默认值仍为 3600，而非恒真断言
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let restored: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(restored.execution_timeout_secs, DEFAULT_EXECUTION_TIMEOUT_SECS);
    }

    #[test]
    fn test_round_trip() {
        let cfg = Config { port: 1234, ..Default::default() };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let restored: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(restored.port, 1234);
        assert!(restored.db_path.contains(".ntd/data.db"));
        assert!(restored.slash_command_rules.is_empty());
    }

    #[test]
    fn test_normalize_single_path_tilde_expansion() {
        // 测试路径归一化能正确展开 ~ 为用户 home 目录，
        // 验证 path_concat 在处理带 ~ 前缀路径时的正确性。
        let home = dirs::home_dir().expect("need home dir for test");
        let result = Config::normalize_single_path("~/bin/mobile");
        // 归一化后的路径应将 ~ 展开为 home_join("bin").join("mobile")
        let expected = home.join("bin").join("mobile").to_string_lossy().to_string();
        assert_eq!(result, expected, "~ should expand to home directory");
    }

    #[test]
    fn test_normalize_single_path_relative() {
        let home = dirs::home_dir().expect("need home dir for test");
        let result = Config::normalize_single_path("./local/claude");
        assert!(
            result.starts_with(&format!("{}", home.display())),
            "relative path should be resolved to absolute under home"
        );
        assert_ne!(result, "./local/claude", "relative path should be changed");
    }

    #[test]
    fn test_normalize_single_path_bare_command() {
        // 裸命令名（无路径斜杠）应原样保留，让系统在 $PATH 中查找
        let result = Config::normalize_single_path("opencode");
        assert_eq!(result, "opencode", "bare command name should be left untouched for PATH lookup");

        // 同上，验证 mobilecoder 作为裸命令名时也能正确透传
        let result = Config::normalize_single_path("mobilecoder");
        assert_eq!(result, "mobilecoder", "bare command name should be left untouched for PATH lookup");
    }

    #[test]
    fn test_normalize_single_path_empty() {
        let result = Config::normalize_single_path("");
        assert_eq!(result, "", "empty path should remain empty");
    }

    #[test]
    fn test_normalize_single_path_already_absolute() {
        let result = Config::normalize_single_path("/usr/bin/claude");
        assert_eq!(result, "/usr/bin/claude", "absolute path should not be modified");
    }

    #[test]
    fn test_slash_command_rules_round_trip() {
        let cfg = Config {
            slash_command_rules: vec![SlashCommandRule {
                slash_command: "/joke".to_string(),
                todo_id: 8,
                enabled: true,
            }],
            ..Default::default()
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let restored: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(restored.slash_command_rules, cfg.slash_command_rules);
    }

    // ---------------------------------------------------------------------------
    // clamp_execution_timeout_secs tests (YAML bypass safety net)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_clamp_preserves_zero() {
        // 0 = "disabled", must pass through unchanged
        let mut cfg = Config { execution_timeout_secs: 0, ..Default::default() };
        cfg.clamp_execution_timeout_secs();
        assert_eq!(cfg.execution_timeout_secs, 0);
    }

    #[test]
    fn test_clamp_raises_sub_minute_to_60() {
        // 1-59s is invalid; normalize should raise to 60
        let mut cfg = Config { execution_timeout_secs: 30, ..Default::default() };
        cfg.clamp_execution_timeout_secs();
        assert_eq!(cfg.execution_timeout_secs, 60);
    }

    #[test]
    fn test_clamp_truncates_above_max() {
        // >MAX should be truncated to MAX
        let mut cfg = Config { execution_timeout_secs: MAX_EXECUTION_TIMEOUT_SECS + 1, ..Default::default() };
        cfg.clamp_execution_timeout_secs();
        assert_eq!(cfg.execution_timeout_secs, MAX_EXECUTION_TIMEOUT_SECS);
    }

    #[test]
    fn test_clamp_preserves_in_range_value() {
        // A valid in-range value must not be modified
        let mut cfg = Config { execution_timeout_secs: 3600, ..Default::default() };
        cfg.clamp_execution_timeout_secs();
        assert_eq!(cfg.execution_timeout_secs, 3600);
    }

    #[test]
    fn test_clamp_at_minimum_boundary() {
        // Exactly 60s (1 min) is valid
        let mut cfg = Config { execution_timeout_secs: 60, ..Default::default() };
        cfg.clamp_execution_timeout_secs();
        assert_eq!(cfg.execution_timeout_secs, 60);
    }

    #[test]
    fn test_clamp_at_maximum_boundary() {
        // Exactly MAX is valid
        let mut cfg = Config { execution_timeout_secs: MAX_EXECUTION_TIMEOUT_SECS, ..Default::default() };
        cfg.clamp_execution_timeout_secs();
        assert_eq!(cfg.execution_timeout_secs, MAX_EXECUTION_TIMEOUT_SECS);
    }

    // ---------------------------------------------------------------------------
    // broadcast_channel_capacity tests (YAML bypass safety net + round-trip)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_default_broadcast_channel_capacity_is_10000() {
        // 默认值是 10000，比硬编码 100 大两个数量级，避免高频输出场景下 ring buffer 覆盖
        let cfg = Config::default();
        assert_eq!(cfg.broadcast_channel_capacity, DEFAULT_BROADCAST_CHANNEL_CAPACITY);
        assert_eq!(cfg.broadcast_channel_capacity, 10000);
    }

    #[test]
    fn test_broadcast_channel_capacity_round_trip() {
        // 自定义值能在 YAML 序列化/反序列化后保留
        let cfg = Config {
            broadcast_channel_capacity: 50000,
            ..Default::default()
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let restored: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(restored.broadcast_channel_capacity, 50000);
    }

    #[test]
    fn test_clamp_broadcast_channel_raises_zero_to_min() {
        // 0 = 等同于关闭事件流，必须抬升到最小值
        let mut cfg = Config {
            broadcast_channel_capacity: 0,
            ..Default::default()
        };
        cfg.clamp_broadcast_channel_capacity();
        assert_eq!(cfg.broadcast_channel_capacity, MIN_BROADCAST_CHANNEL_CAPACITY);
    }

    #[test]
    fn test_clamp_broadcast_channel_raises_small_value() {
        // 任何小于 MIN 的值都被抬升
        let mut cfg = Config {
            broadcast_channel_capacity: 50,
            ..Default::default()
        };
        cfg.clamp_broadcast_channel_capacity();
        assert_eq!(cfg.broadcast_channel_capacity, MIN_BROADCAST_CHANNEL_CAPACITY);
    }

    #[test]
    fn test_clamp_broadcast_channel_preserves_in_range_value() {
        // 在 [MIN, ∞) 范围内的值不应被修改
        let mut cfg = Config {
            broadcast_channel_capacity: 5000,
            ..Default::default()
        };
        cfg.clamp_broadcast_channel_capacity();
        assert_eq!(cfg.broadcast_channel_capacity, 5000);
    }

    #[test]
    fn test_clamp_broadcast_channel_preserves_minimum_boundary() {
        // 恰好等于 MIN 的值不应被修改
        let mut cfg = Config {
            broadcast_channel_capacity: MIN_BROADCAST_CHANNEL_CAPACITY,
            ..Default::default()
        };
        cfg.clamp_broadcast_channel_capacity();
        assert_eq!(cfg.broadcast_channel_capacity, MIN_BROADCAST_CHANNEL_CAPACITY);
    }
}
