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
    /// 执行超时时间（秒，默认 3600 = 60 分钟），超过此时间将自动终止进程
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
    /// 云端同步配置
    pub cloud_sync: CloudSyncConfig,
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
    /// 认证 token
    pub token: Option<String>,
    /// 设备 ID
    pub device_id: Option<i64>,
    /// 最后同步时间
    pub last_sync_at: Option<String>,
    /// 默认冲突解决模式
    pub default_conflict_mode: String,
}

impl Default for CloudSyncConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            token: None,
            device_id: None,
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
            execution_timeout_secs: 3600,
            auto_cleanup_logs_days: Some(30),
            auto_skill_backup_enabled: false,
            auto_skill_backup_cron: "0 0 5 * * *".to_string(),
            auto_skill_backup_max_files: 30,
            scheduler_default_timezone: None,
            auto_usage_stats_enabled: false,
            auto_usage_stats_cron: "0 0 1 * * *".to_string(),
            cloud_sync: CloudSyncConfig::default(),
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
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut cfg_for_save = cfg.clone();
            cfg_for_save.normalize_paths();
            if let Ok(yaml) = serde_yaml::to_string(&cfg_for_save) {
                if let Err(e) = std::fs::write(&path, yaml) {
                    eprintln!("Warning: failed to write config file ({}), using in-memory defaults", e);
                }
            }
            cfg.normalize_paths();
            return cfg;
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let mut cfg = serde_yaml::from_str::<Config>(&content).unwrap_or_else(|e| {
                    eprintln!("Warning: failed to parse config file ({}), using defaults", e);
                    Config::default()
                });
                cfg.normalize_paths();
                cfg
            }
            Err(e) => {
                eprintln!("Warning: failed to read config file ({}), using defaults", e);
                let mut cfg = Config::default();
                cfg.normalize_paths();
                cfg
            }
        }
    }

    /// Normalize paths: convert ~ and relative paths to absolute paths.
    pub fn normalize_paths(&mut self) {
        self.db_path = Self::normalize_single_path(&self.db_path);
        // Normalize executor paths
        let normalized: HashMap<String, String> = self.executors.paths.iter()
            .map(|(k, v)| (k.clone(), Self::normalize_single_path(v)))
            .collect();
        self.executors.paths = normalized;
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
        let home = dirs::home_dir().expect("need home dir for test");
        let result = Config::normalize_single_path("~/bin/joinai");
        let expected = home.join("bin").join("joinai").to_string_lossy().to_string();
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
        let result = Config::normalize_single_path("opencode");
        assert_eq!(result, "opencode", "bare command name should be left untouched for PATH lookup");

        let result = Config::normalize_single_path("joinai");
        assert_eq!(result, "joinai", "bare command name should be left untouched for PATH lookup");
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
}
