use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 每个阶段在一次 loop execution 中的执行记录。
///
/// execution_record_id 指向 execution_records，关联到具体的 executor run。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loop_step_executions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub loop_execution_id: i64,
    pub step_id: i64,
    pub todo_id: i64,
    pub execution_record_id: Option<i64>,
    /// pending | running | success | failed | skipped
    #[sea_orm(default_value = "pending")]
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
    /// 执行时的评分阈值（快照，不随 loop 配置变化）
    pub min_rating: Option<i32>,
    /// 执行时的未达标策略（快照，不随 loop 配置变化）
    pub unrated_policy: Option<String>,
    /// 执行时的评审评分（快照，不随 execution_record 变化）
    pub rating: Option<i32>,
    /// 本次 loop_execution 中的全局执行序号（1, 2, 3...）
    #[sea_orm(default_value = "0")]
    pub sequence_index: i32,
    /// 本次步执行的核心结论摘要
    pub conclusion: Option<String>,
    /// 人工审批状态: NULL | "pending" | "approved"（非人工审批环节为 NULL）
    pub approval_status: Option<String>,
    /// 审批人的备注/意见
    pub approval_comment: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::loop_executions::Entity",
        from = "Column::LoopExecutionId",
        to = "super::loop_executions::Column::Id"
    )]
    BelongsToLoopExecution,
    #[sea_orm(
        belongs_to = "super::loop_steps::Entity",
        from = "Column::StepId",
        to = "super::loop_steps::Column::Id"
    )]
    BelongsToStep,
    #[sea_orm(
        belongs_to = "super::todos::Entity",
        from = "Column::TodoId",
        to = "super::todos::Column::Id"
    )]
    BelongsToTodo,
    #[sea_orm(
        belongs_to = "super::execution_records::Entity",
        from = "Column::ExecutionRecordId",
        to = "super::execution_records::Column::Id"
    )]
    BelongsToExecutionRecord,
}

impl Related<super::loop_executions::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoopExecution.def() }
}

impl Related<super::loop_steps::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToStep.def() }
}

impl Related<super::todos::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToTodo.def() }
}

impl Related<super::execution_records::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToExecutionRecord.def() }
}

impl ActiveModelBehavior for ActiveModel {}
