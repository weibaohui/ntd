use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 一次 Loop 执行中每个阶段的运行记录。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loop_phase_executions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属 loop_execution ID。
    pub loop_execution_id: i64,
    /// 关联的 loop_phase ID。
    pub phase_id: i64,
    /// 状态：`pending` | `running` | `success` | `failed` | `skipped`。
    #[sea_orm(default_value = "pending")]
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
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
        belongs_to = "super::loop_phases::Entity",
        from = "Column::PhaseId",
        to = "super::loop_phases::Column::Id"
    )]
    BelongsToLoopPhase,
}

impl Related<super::loop_executions::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoopExecution.def() }
}

impl Related<super::loop_phases::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoopPhase.def() }
}

impl ActiveModelBehavior for ActiveModel {}
