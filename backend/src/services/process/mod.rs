//! 工艺管理（Process Management）服务模块。
//!
//! 提供工艺模板解析、实例化（安装为 Loop）等能力。
//! M1 聚焦「模板 → Loop」的静态转换；运行时阶段驱动、产物捕获、门禁评价
//! 在后续迭代中逐步补齐。

use serde::{Deserialize, Serialize};

pub mod artifact_capture;
pub mod audit;
pub mod delivery_state;
pub mod gate_evaluator;
pub mod gates;
pub mod installer;
pub mod phase_driver;
pub mod recommender;
pub mod repair_log;
pub mod rework_tracker;
pub mod transition_resolver;
pub mod user_dir;

/// 工艺模板完整定义（从 `process_templates.definition` YAML 解析）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProcessDefinition {
    /// 工艺元信息
    pub process: ProcessMeta,
    /// 全局执行限制
    #[serde(default)]
    pub limits: ProcessLimits,
    /// 异常处理配置
    #[serde(default)]
    pub abnormal_handler: Option<AbnormalHandlerConfig>,
    /// 阶段列表
    #[serde(default)]
    pub phases: Vec<PhaseDefinition>,
    /// 环节原型内联定义（可选，若未引用外部 step_template 表）
    #[serde(default)]
    pub step_templates: Vec<StepTemplateDefinition>,
}

/// 工艺元信息。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProcessMeta {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_complexity")]
    pub complexity: String,
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_category() -> String {
    "general".to_string()
}

fn default_complexity() -> String {
    "standard".to_string()
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// 全局限制配置。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProcessLimits {
    #[serde(default)]
    pub max_step_executions: Option<i32>,
    #[serde(default)]
    pub max_total_tokens: Option<i64>,
}

/// 异常处理配置。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AbnormalHandlerConfig {
    pub todo_template: Option<String>,
    #[serde(default)]
    pub trigger_on: Vec<String>,
}

/// 阶段定义。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PhaseDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub spec: String,
    /// 引用外部阶段规范文件（如 bundled://processes/conventions/requirement-phase-spec.md）。
    /// 优先级高于 inline `spec`：若 `spec_ref` 存在且文件可读，覆盖 `spec`。
    #[serde(default)]
    pub spec_ref: Option<String>,
    #[serde(default)]
    pub acceptance_criteria: String,
    /// 引用外部验收标准文件，优先级高于 inline `acceptance_criteria`。
    #[serde(default)]
    pub acceptance_criteria_ref: Option<String>,
    #[serde(default)]
    pub links: Vec<LinkDefinition>,
}

/// spec 模板文件引用（name + path），执行时注入 AI 上下文供其重点阅读。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepTemplateRef {
    pub name: String,
    pub path: String,
}

/// 环节（Link）定义。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinkDefinition {
    pub id: String,
    pub name: String,
    /// 引用的 spec 模板文件列表（执行时注入 AI 上下文供其重点阅读）。
    /// 设计上仅作 spec 引用；执行配置一律以内联字段（prompt/executor/expert/model/acceptance_criteria）为准。
    pub step_template: Option<Vec<StepTemplateRef>>,
    #[serde(default)]
    pub prompt: String,
    pub executor: Option<String>,
    pub expert: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    pub model: Option<String>,
    /// 评审类型：ai / human，默认 ai
    #[serde(default = "default_review_type")]
    pub review_type: String,
    #[serde(default)]
    pub expected_artifacts: Vec<ExpectedArtifact>,
    #[serde(default)]
    pub gates: Vec<GateDefinition>,
    #[serde(default = "default_on_success")]
    pub on_success: String,
    #[serde(default = "default_on_gate_fail")]
    pub on_gate_fail: String,
    /// 兼容旧模板的评分失败策略；未提供 on_gate_fail 时作为 fallback
    #[serde(default = "default_on_rating_fail")]
    pub on_rating_fail: String,
    #[serde(default = "default_max_rework")]
    pub max_rework: i32,
    /// 环节级验收标准（内联；早期由原型表提供，现已随 step_template 解耦迁入 link）。
    #[serde(default)]
    pub acceptance_criteria: String,
}

fn default_review_type() -> String {
    "ai".to_string()
}

fn default_on_success() -> String {
    "next".to_string()
}

fn default_on_gate_fail() -> String {
    "break".to_string()
}

fn default_on_rating_fail() -> String {
    "break".to_string()
}

fn default_max_rework() -> i32 {
    3
}

/// 期望产物定义。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExpectedArtifact {
    pub name: String,
    #[serde(rename = "type")]
    pub artifact_type: String,
    pub path: Option<String>,
    pub locator: Option<String>,
}

/// 门禁定义。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GateDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub gate_type: String,
    pub artifact: Option<String>,
    pub criteria_ref: Option<String>,
    pub min_score: Option<i32>,
    pub script: Option<String>,
}

/// 内联环节原型定义。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepTemplateDefinition {
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub prompt: String,
    pub executor: Option<String>,
    pub expert_name: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub acceptance_criteria: String,
}

/// 工艺安装结果。
#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub loop_id: i64,
    pub loop_name: String,
    pub phase_count: usize,
    pub step_count: usize,
}

/// 安装错误。
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("工艺模板未找到: {0}")]
    TemplateNotFound(String),
    #[error("工作空间未找到: {0}")]
    WorkspaceNotFound(i64),
    #[error("YAML 解析失败: {0}")]
    ParseError(String),
    #[error("数据库错误: {0}")]
    DbError(#[from] sea_orm::DbErr),
    #[error("goto 目标未找到: {0}")]
    GotoTargetNotFound(String),
}

/// 工艺运行时错误（门禁、捕获、阶段驱动等）。
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("解析错误: {0}")]
    ParseError(String),
    #[error("数据库错误: {0}")]
    Db(Box<sea_orm::DbErr>),
    #[error("门禁错误: {0}")]
    GateError(#[from] gates::GateError),
    #[error("产物捕获错误: {0}")]
    ArtifactCaptureError(#[from] artifact_capture::ArtifactCaptureError),
    #[error("运行错误: {0}")]
    Runtime(String),
}

impl From<sea_orm::DbErr> for ProcessError {
    fn from(e: sea_orm::DbErr) -> Self {
        ProcessError::Db(Box::new(e))
    }
}

impl From<serde_yaml::Error> for InstallError {
    fn from(e: serde_yaml::Error) -> Self {
        InstallError::ParseError(e.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod process_definition_tests {
    use super::*;

    #[test]
    fn parse_bundled_process_templates() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("bundled-repo/processes");
        if !base.exists() {
            return;
        }

        let mut parsed = 0;
        for entry in std::fs::read_dir(&base).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if !path.is_dir() || path.file_name().unwrap() == "step-templates" {
                continue;
            }
            for file in std::fs::read_dir(&path).unwrap() {
                let file = file.unwrap().path();
                if file.extension().and_then(|e| e.to_str()) != Some("yaml") {
                    continue;
                }
                let content = std::fs::read_to_string(&file).unwrap();
                let def: ProcessDefinition = serde_yaml::from_str(&content)
                    .unwrap_or_else(|e| panic!("解析 {} 失败: {}", file.display(), e));
                assert!(!def.process.name.is_empty());
                parsed += 1;
            }
        }
        assert!(parsed >= 3, "应至少解析 3 个内置工艺模板");
    }

    #[test]
    fn parse_bundled_step_templates() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("bundled-repo/processes/step-templates");
        if !dir.exists() {
            return;
        }

        let mut parsed = 0;
        for file in std::fs::read_dir(&dir).unwrap() {
            let file = file.unwrap().path();
            if file.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            let content = std::fs::read_to_string(&file).unwrap();
            let wrapper: serde_yaml::Value = serde_yaml::from_str(&content)
                .unwrap_or_else(|e| panic!("解析 {} 失败: {}", file.display(), e));
            assert!(
                wrapper.get("step_template").is_some(),
                "{} 必须包含 step_template 根键",
                file.display()
            );
            parsed += 1;
        }
        assert!(parsed >= 1, "应至少解析 1 个环节原型");
    }
}
