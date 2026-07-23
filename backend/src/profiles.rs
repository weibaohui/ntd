//! Provider 池 + Profile 管理。
//!
//! # 架构
//!
//! 两层结构：
//!
//! 1. **Provider Pool（供应商池）** — 集中管理 API Key、Base URL、协议格式、模型列表。
//!    按供应商/平台维度组织，一个 provider 对应一个 API 端点。
//!
//! 2. **Profile** — 针对每个执行器，引用 Provider 池中的某个供应商和模型。
//!    Apply 时自动从 Provider 查出 key/url，按执行器格式生成配置文件。
//!
//! # 存储
//!
//! 统一存储在 `~/.ntd/profiles.yaml`。

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ============================================================================
// Provider 池
// ============================================================================

/// API 协议格式。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// OpenAI 兼容协议（大多数执行器使用，如 AtomCode、Kilo、Opencode 等）
    #[default]
    Openai,
    /// Anthropic 原生协议（Claude Code 使用）
    Anthropic,
}

/// 单个模型条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModel {
    /// 模型标识符（如 "deepseek-v4-flash"）
    pub name: String,
    /// 显示名称（可选，如 "DeepSeek v4 Flash"）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 是否支持 1M 上下文（某些模型有 1M context window，某些没有）
    #[serde(default)]
    pub supports_1m_context: bool,
}

/// 单个供应商（API 服务商）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    /// 显示名称（如 "DeepSeek (Anthropic 协议)"）
    pub name: String,
    /// API Key
    pub api_key: String,
    /// API 基础 URL
    pub base_url: String,
    /// 协议格式
    #[serde(default)]
    pub protocol: Protocol,
    /// 可用模型列表
    #[serde(default)]
    pub models: Vec<ProviderModel>,
}

// ============================================================================
// Profile
// ============================================================================

/// 单个 Profile 中，对某个执行器的配置——引用 Provider 池中的供应商和模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorRef {
    /// 引用的 provider name（在 Provider Pool 中查找 api_key/base_url/protocol）
    pub provider: String,
    /// 引用的模型名（在该 provider 的 models 列表中）
    pub model: String,
}

/// 单个 Profile。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorProfile {
    /// Profile 显示名称
    pub name: String,
    /// 可选描述
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 各执行器配置（key 为 executor name，如 "claudecode"，value 引用 provider+model）
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub executors: HashMap<String, ExecutorRef>,
}

// ============================================================================
// 顶层配置
// ============================================================================

/// 完整的 profiles.yaml 结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesConfig {
    /// 供应商池
    #[serde(default)]
    pub providers: HashMap<String, Provider>,
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
            description: Some("日常开发使用的默认配置，可在此选择供应商和模型".to_string()),
            executors: HashMap::new(),
        });
        Self {
            providers: HashMap::new(),
            current_profile: "default".to_string(),
            profiles,
        }
    }
}

// ============================================================================
// API DTOs
// ============================================================================

/// 创建 Provider 的请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,
    pub display_name: String,
    pub api_key: String,
    pub base_url: String,
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(default)]
    pub models: Vec<ProviderModel>,
}

/// Provider 摘要（返回给前端列表，不包含 api_key）。
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSummary {
    pub name: String,
    pub display_name: String,
    pub base_url: String,
    pub protocol: Protocol,
    pub model_count: usize,
}

/// Provider 详情（含 api_key，用于编辑弹窗回填）。
#[derive(Debug, Clone, Serialize)]
pub struct ProviderDetail {
    pub name: String,
    pub display_name: String,
    pub api_key: String,
    pub base_url: String,
    pub protocol: Protocol,
    pub models: Vec<ProviderModel>,
}

/// 更新 Provider 的请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateProviderRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub protocol: Option<Protocol>,
    /// None = 不修改，Some(None) = 清空模型，Some(Some(list)) = 替换
    #[serde(default)]
    pub models: Option<Vec<ProviderModel>>,
}

/// Profile 摘要（列表展示，不含执行器详情）。
#[derive(Debug, Clone, Serialize)]
pub struct ProfileSummary {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub executor_count: usize,
    pub is_current: bool,
}

/// 创建 Profile 的请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct CreateProfileRequest {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub executors: HashMap<String, ExecutorRef>,
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
    pub executors: HashMap<String, Option<ExecutorRef>>,
}

/// 应用 Profile 的响应。
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

impl ProfilesConfig {
    /// 获取 profiles.yaml 文件路径。
    pub fn config_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let base = home.join(".ntd");
        if crate::config::Config::is_dev_mode() {
            base.join("profiles.dev.yaml")
        } else {
            base.join("profiles.yaml")
        }
    }

    /// 从磁盘加载。不存在时返回默认值。
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            let cfg = ProfilesConfig::default();
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
                    tracing::warn!(error = %e, path = %path.display(), "failed to parse profiles.yaml, using defaults");
                    ProfilesConfig::default()
                })
            }
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "failed to read profiles.yaml, using defaults");
                ProfilesConfig::default()
            }
        }
    }

    /// 原子写。
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

/// 内联 generators 模块。
pub mod generators;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark, clippy::match_same_arms, clippy::from_over_into, clippy::unwrap_or_default)]
mod tests {
    use super::*;

    #[test]
    fn test_default_has_default_profile() {
        let cfg = ProfilesConfig::default();
        assert_eq!(cfg.current_profile, "default");
        assert!(cfg.profiles.contains_key("default"));
        assert!(cfg.providers.is_empty(), "默认不应有 provider");
    }

    #[test]
    fn test_provider_round_trip() {
        let provider = Provider {
            name: "测试".to_string(),
            api_key: "sk-xxx".to_string(),
            base_url: "https://api.test.com".to_string(),
            protocol: Protocol::Openai,
            models: vec![
                ProviderModel { name: "gpt-4".to_string(), display_name: Some("GPT-4".to_string()), supports_1m_context: false },
                ProviderModel { name: "gpt-4o".to_string(), display_name: None, supports_1m_context: false },
            ],
        };
        let json = serde_json::to_string(&provider).unwrap();
        let restored: Provider = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.api_key, "sk-xxx");
        assert_eq!(restored.models.len(), 2);
        assert_eq!(restored.protocol, Protocol::Openai);
    }

    #[test]
    fn test_executor_ref_resolve() {
        let mut providers = HashMap::new();
        providers.insert("test-prov".to_string(), Provider {
            name: "测试".to_string(),
            api_key: "sk-xxx".to_string(),
            base_url: "https://api.test.com".to_string(),
            protocol: Protocol::Anthropic,
            models: vec![
                ProviderModel { name: "claude-3".to_string(), display_name: None, supports_1m_context: false },
            ],
        });

        let cfg = ProfilesConfig {
            providers,
            current_profile: "default".to_string(),
            profiles: HashMap::new(),
        };

        // 验证能从 provider 查到信息
        let p = cfg.providers.get("test-prov").unwrap();
        assert_eq!(p.api_key, "sk-xxx");
        assert_eq!(p.protocol, Protocol::Anthropic);
        assert_eq!(p.models[0].name, "claude-3");
    }
}
