//! 执行器 Profile 管理
//!
//! # 架构
//!
//! - 数据存储在 `~/.ntd/profiles.yaml`（dev 模式为 `~/.ntd/profiles.dev.yaml`）
//! - 每个 Profile 包含一组执行器配置（key 为执行器 name，如 `claudecode`、`pi`）
//! - 通用字段（api_key、model、base_url）显式建模，专有字段通过 `extra` HashMap 兜底
//! - `switch_profile` 将选中的 Profile 通过 ConfigGenerator 写入各执行器的原生配置文件
//!
//! # 安全
//!
//! API Key 明文存储，依赖文件系统权限保护。后续可引入 AES 加密。
//! apply 前自动备份原配置文件到 `~/.ntd/profile_backups/`。

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ============================================================================
// 数据结构
// ============================================================================

/// 顶层 Profile 配置（profiles.yaml 根结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesConfig {
    /// 当前激活的 profile 名称
    #[serde(default = "default_current_profile")]
    pub current_profile: String,
    /// 所有 profile 的映射表
    #[serde(default)]
    pub profiles: HashMap<String, ExecutorProfile>,
}

fn default_current_profile() -> String {
    "default".to_string()
}

impl Default for ProfilesConfig {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        profiles.insert("default".to_string(), ExecutorProfile {
            name: "默认配置".to_string(),
            description: Some("日常开发使用的默认配置".to_string()),
            ..Default::default()
        });
        Self {
            current_profile: "default".to_string(),
            profiles,
        }
    }
}

/// 单个 Executor Profile：按执行器名称分组，每个执行器一组设置。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutorProfile {
    /// Profile 显示名称（如"日常开发"、"工作配置"）
    pub name: String,
    /// 可选描述
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 各执行器配置（key 为 executor name，如 "claudecode"、"pi"、"atomcode"、"kilo"）
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub executors: HashMap<String, ExecutorSettings>,
}

/// 单个执行器的一组设置。
///
/// 通用字段（api_key、model、base_url）显式建模，各执行器的专有字段
/// 通过 `#[serde(flatten)]` 的 `extra` HashMap 兜底——例如 PI 的
/// `openai_api_key`、`google_api_key` 等不会因字段未定义而丢失。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutorSettings {
    /// API Key（对应各执行器的 api_key / apiKey / token 等字段）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// API 基础 URL（如 https://api.anthropic.com）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// 默认模型（如 claude-sonnet-4-20250514）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 专有字段兜底（如 pi 的 openai_api_key, google_api_key 等）
    #[serde(default, flatten)]
    pub extra: HashMap<String, String>,
}

impl ExecutorSettings {
    /// 将所有字段合并为一个 HashMap，方便配置生成器遍历。
    pub fn to_map(&self) -> HashMap<String, String> {
        let mut m = self.extra.clone();
        if let Some(v) = &self.api_key {
            m.insert("api_key".to_string(), v.clone());
        }
        if let Some(v) = &self.base_url {
            m.insert("base_url".to_string(), v.clone());
        }
        if let Some(v) = &self.model {
            m.insert("model".to_string(), v.clone());
        }
        m
    }

    /// 从 HashMap 反向构造（用于 API 接收扩展字段）。
    pub fn from_map(mut map: HashMap<String, String>) -> Self {
        Self {
            api_key: map.remove("api_key"),
            base_url: map.remove("base_url"),
            model: map.remove("model"),
            extra: map,
        }
    }
}

// ============================================================================
// API DTOs
// ============================================================================

/// 返回给前端的 Profile 摘要（不含执行器配置详情，仅用于列表展示）。
#[derive(Debug, Clone, Serialize)]
pub struct ProfileSummary {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    /// 该 profile 配置了多少种执行器
    pub executor_count: usize,
    /// 是否为当前激活的 profile
    pub is_current: bool,
}

/// 创建 Profile 的请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct CreateProfileRequest {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// 初始执行器配置（可选，为空则创建空 profile）
    #[serde(default)]
    pub executors: HashMap<String, ExecutorSettings>,
}

/// 更新 Profile 的请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateProfileRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// 完整替换某个执行器的配置；None = 不修改该执行器
    #[serde(default)]
    pub executors: HashMap<String, Option<ExecutorSettings>>,
}

/// 切换 Profile 的响应。
#[derive(Debug, Clone, Serialize)]
pub struct ApplyProfileResult {
    pub profile_name: String,
    pub profile_display_name: String,
    pub applied_executors: Vec<String>,
    pub skipped_executors: Vec<String>,
    pub errors: Vec<String>,
}

// ============================================================================
// 加载/保存
// ============================================================================

/// `ProfilesConfig` 加载/保存路径。
impl ProfilesConfig {
    /// 获取 profiles.yaml 文件路径。
    /// dev 模式使用 profiles.dev.yaml，与 config.rs 的约定一致。
    pub fn config_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let base = home.join(".ntd");
        if crate::config::Config::is_dev_mode() {
            base.join("profiles.dev.yaml")
        } else {
            base.join("profiles.yaml")
        }
    }

    /// 从磁盘加载 profiles.yaml。
    /// 不存在时返回默认值（含一个 `default` 空 profile）。
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            let cfg = ProfilesConfig::default();
            // 首次启动自动写盘默认值
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            if let Ok(yaml) = serde_yaml::to_string(&cfg) {
                let _ = std::fs::write(&path, yaml);
            }
            return cfg;
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                serde_yaml::from_str::<ProfilesConfig>(&content).unwrap_or_else(|e| {
                    tracing::warn!(
                        error = %e,
                        path = %path.display(),
                        "failed to parse profiles.yaml, using defaults"
                    );
                    ProfilesConfig::default()
                })
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "failed to read profiles.yaml, using defaults"
                );
                ProfilesConfig::default()
            }
        }
    }

    /// 原子写 profiles.yaml（临时文件 + rename）。
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create profiles dir: {}", e))?;
        }
        let yaml = serde_yaml::to_string(self).map_err(|e| format!("Failed to serialize profiles: {}", e))?;
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, yaml).map_err(|e| format!("Failed to write temp profiles: {}", e))?;
        std::fs::rename(&tmp_path, &path).map_err(|e| format!("Failed to rename profiles: {}", e))?;
        Ok(())
    }
}

// ============================================================================
// 配置生成器
// ============================================================================

/// ProfileGenerator trait：将统一的 ExecutorSettings 转换为各执行器原生配置格式。
///
/// 每个执行器实现一个生成器，覆写 `generate` 方法：
/// 1. 根据 `settings` 构造目标格式的数据
/// 2. 序列化为执行器期望的格式（JSON / YAML / TOML）
/// 3. 写入执行器配置文件的正确路径
///
/// # 备份安全
/// 生成器在覆写前通过 `backup_existing_config` 自动备份原文件到 `~/.ntd/profile_backups/`。
pub trait ProfileGenerator: Send + Sync {
    /// 执行器名称（与 ExecutorType.as_str() 一致）
    fn executor_name(&self) -> &str;

    /// 目标配置文件的路径
    fn config_path(&self, session_dir: &str) -> PathBuf {
        // 默认实现：session_dir 下拼接默认配置文件名
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let expanded = if session_dir.starts_with('~') {
            let relative = session_dir.trim_start_matches('~').trim_start_matches(std::path::MAIN_SEPARATOR);
            home.join(relative)
        } else {
            PathBuf::from(session_dir)
        };
        expanded.join(self.default_filename())
    }

    /// 默认配置文件名（如 "settings.json"、"config.yaml"）。
    fn default_filename(&self) -> &str;

    /// 根据 ExecutorSettings 生成配置内容并写入目标文件。
    fn generate(&self, settings: &ExecutorSettings, session_dir: &str) -> Result<(), String>;
}

/// 内置的 ProfileGenerator 注册表。
pub struct ProfileGeneratorRegistry {
    generators: HashMap<String, Box<dyn ProfileGenerator>>,
}

impl ProfileGeneratorRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            generators: HashMap::new(),
        };
        reg.register(Box::new(generators::ClaudeCodeGenerator));
        reg.register(Box::new(generators::PiGenerator));
        reg.register(Box::new(generators::AtomCodeGenerator));
        reg.register(Box::new(generators::KiloGenerator));
        reg
    }

    pub fn register(&mut self, gen: Box<dyn ProfileGenerator>) {
        let name = gen.executor_name().to_string();
        self.generators.insert(name, gen);
    }

    pub fn get(&self, name: &str) -> Option<&dyn ProfileGenerator> {
        self.generators.get(name).map(|b| b.as_ref())
    }

    pub fn list(&self) -> Vec<&str> {
        self.generators.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ProfileGeneratorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 内联 `generators` 模块。
pub mod generators;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark, clippy::match_same_arms, clippy::from_over_into, clippy::unwrap_or_default)]
mod tests {
    use std::collections::HashMap;
    use super::*;

    #[test]
    fn test_profiles_config_default_has_default_profile() {
        let cfg = ProfilesConfig::default();
        assert_eq!(cfg.current_profile, "default");
        assert!(cfg.profiles.contains_key("default"));
        assert_eq!(cfg.profiles["default"].name, "默认配置");
    }

    #[test]
    fn test_executor_settings_to_map_includes_all_fields() {
        let settings = ExecutorSettings {
            api_key: Some("sk-xxx".to_string()),
            base_url: Some("https://api.example.com".to_string()),
            model: Some("claude-3".to_string()),
            extra: [("custom_field".to_string(), "value".to_string())].into(),
        };
        let map = settings.to_map();
        assert_eq!(map.get("api_key"), Some(&"sk-xxx".to_string()));
        assert_eq!(map.get("base_url"), Some(&"https://api.example.com".to_string()));
        assert_eq!(map.get("model"), Some(&"claude-3".to_string()));
        assert_eq!(map.get("custom_field"), Some(&"value".to_string()));
    }

    #[test]
    fn test_executor_settings_from_map_round_trip() {
        let mut map = HashMap::new();
        map.insert("api_key".to_string(), "sk-xxx".to_string());
        map.insert("model".to_string(), "gpt-4".to_string());
        map.insert("extra_flag".to_string(), "true".to_string());

        let settings = ExecutorSettings::from_map(map);
        assert_eq!(settings.api_key, Some("sk-xxx".to_string()));
        assert_eq!(settings.model, Some("gpt-4".to_string()));
        assert_eq!(settings.base_url, None);
        assert_eq!(settings.extra.get("extra_flag"), Some(&"true".to_string()));
    }

    #[test]
    fn test_executor_settings_to_map_empty_extra() {
        let settings = ExecutorSettings {
            api_key: None,
            base_url: None,
            model: None,
            extra: HashMap::new(),
        };
        let map = settings.to_map();
        assert!(map.is_empty());
    }

    #[test]
    fn test_profile_summary_executor_count() {
        let mut executors = HashMap::new();
        executors.insert("claudecode".to_string(), ExecutorSettings::default());
        executors.insert("pi".to_string(), ExecutorSettings::default());
        let profile = ExecutorProfile {
            name: "test".to_string(),
            description: None,
            executors,
        };
        assert_eq!(profile.executors.len(), 2);
    }
}
