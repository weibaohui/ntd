//! 门禁引擎 —— Gate 接口与共享类型。
//!
//! 每种门禁类型实现一个 async `evaluate` 函数，由 `GateEvaluator` 统一编排。

use crate::services::process::GateDefinition;

/// 单条门禁的评价结果。
#[derive(Debug, Clone)]
pub struct GateResult {
    pub gate_name: String,
    pub gate_type: String,
    pub passed: bool,
    /// 评价细节（JSON 字符串）：评分、原因、脚本输出等。
    pub detail: Option<String>,
}

/// 门禁评价上下文。
#[derive(Debug, Clone)]
pub struct GateContext<'a> {
    /// 本次环节执行 ID（loop_step_executions.id）。
    pub step_execution_id: i64,
    /// 环节配置中的门禁定义。
    pub config: GateDefinition,
    /// 环节使用的 skill 名称列表。
    pub skill_names: &'a [String],
    /// 已捕获的产物（名称 → 文本内容）。
    pub artifacts: &'a [crate::db::entity::loop_step_artifacts::Model],
    /// 执行记录 result 文本（用于 AI 评审）。
    pub execution_result: Option<&'a str>,
    /// 环节的 acceptance_criteria。
    pub acceptance_criteria: Option<&'a str>,
    /// 工作空间路径（用于 script_check）。
    pub workspace_path: &'a str,
}

/// 门禁评价错误。
#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("DB 错误: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("脚本执行错误: {0}")]
    ScriptExecution(String),
    #[error("配置解析错误: {0}")]
    ConfigParse(String),
}

pub mod ai_criteria_review;
pub mod human_approval;
