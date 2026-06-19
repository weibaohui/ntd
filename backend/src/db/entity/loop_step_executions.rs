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
