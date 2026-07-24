use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 环节门禁评价记录：对产物按验收标准进行客观评价。
///
/// 支持 `artifact_present`、`ai_criteria_review`、`human_approval`、`script_check` 四种类型。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loop_step_execution_gates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属 loop_step_execution ID。
    pub loop_step_execution_id: i64,
    /// 门禁类型：`artifact_present` | `ai_criteria_review` | `human_approval` | `script_check`。
    pub gate_type: String,
    /// 门禁名称。
    pub gate_name: String,
    /// 门禁配置（JSON）。
    #[sea_orm(column_type = "Text")]
    pub config: String,
    /// 状态：`pending` | `passed` | `failed`。
    #[sea_orm(default_value = "pending")]
    pub status: String,
    /// 评价结果（JSON），如评分、脚本输出。
    #[sea_orm(column_type = "Text")]
    pub result: Option<String>,
    /// 评价时间（UTC ISO8601）。
    pub evaluated_at: Option<String>,
    /// 评价者：`ai`、用户 ID 或脚本名。
    pub evaluated_by: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::loop_step_executions::Entity",
        from = "Column::LoopStepExecutionId",
        to = "super::loop_step_executions::Column::Id"
    )]
    BelongsToLoopStepExecution,
}

impl Related<super::loop_step_executions::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoopStepExecution.def() }
}

impl ActiveModelBehavior for ActiveModel {}
