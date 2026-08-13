//! Executor 域数据模型（096-W4-2：从 models/mod.rs 按域拆分，逐字搬迁零改动）。
//!
//! 含：ExecutorType / ExecutorConfig / 检测与测试结果 DTO 族。
//! 经 `models::mod` 的 `pub use executor::*` 聚合，外部引用路径不变。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub enabled: bool,
    pub display_name: String,
    pub session_dir: String,
    /// 是否为系统默认执行器
    pub is_default: bool,
    /// 执行器级默认模型。None = 未指定，执行时不传 --model，由执行器配置文件决定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// 是否支持动态列模型（computed，不落库）。前端据此决定 Select(有选项)/Input(手填)。
    #[serde(default)]
    pub supports_models: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateExecutorRequest {
    pub path: Option<String>,
    pub enabled: Option<bool>,
    pub display_name: Option<String>,
    pub session_dir: Option<String>,
    /// 执行器默认模型。空串 = 清除默认模型；None = 不修改。
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Serialize)]
pub struct ExecutorDetectResult {
    pub binary_found: bool,
    pub path_resolved: Option<String>,
}

/// resolve 操作用的结果：包含检测结果 + 是否触发了数据库更新
#[derive(Serialize)]
pub struct ExecutorPathResolveResult {
    pub binary_found: bool,
    pub path_resolved: Option<String>,
    /// 数据库路径是否被更新（仅在 binary_found=true 且路径与原值不同时为 true）
    pub path_updated: bool,
    pub old_path: Option<String>,
    pub new_path: Option<String>,
}

#[derive(Serialize)]
pub struct ExecutorTestResult {
    pub test_passed: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct ExecutorBatchDetectResult {
    pub results: Vec<ExecutorDetectInfo>,
    pub total: usize,
    pub found_count: usize,
}

#[derive(Serialize)]
pub struct ExecutorDetectInfo {
    pub name: String,
    pub display_name: String,
    pub binary_found: bool,
    pub path_resolved: Option<String>,
    pub enabled: bool,
}

// Executor types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ExecutorType {
    Mobilecoder,
    #[default]
    Claudecode,
    Codebuddy,
    Opencode,
    Atomcode,
    Hermes,
    Kimi,
    Codex,
    Codewhale,
    Pi,
    Mimo,
    Zhanlu,
    Kilo,
}

impl ExecutorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutorType::Mobilecoder => "mobilecoder",
            ExecutorType::Claudecode => "claudecode",
            ExecutorType::Codebuddy => "codebuddy",
            ExecutorType::Opencode => "opencode",
            ExecutorType::Atomcode => "atomcode",
            ExecutorType::Hermes => "hermes",
            ExecutorType::Kimi => "kimi",
            ExecutorType::Codex => "codex",
            ExecutorType::Codewhale => "codewhale",
            ExecutorType::Pi => "pi",
            ExecutorType::Mimo => "mimo",
            ExecutorType::Zhanlu => "zhanlu",
            ExecutorType::Kilo => "kilo",
        }
    }
}

impl std::fmt::Display for ExecutorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
