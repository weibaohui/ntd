use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Loop 每次运行的顶层记录。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loop_executions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub loop_id: i64,
    /// manual 触发时为 NULL
    pub trigger_id: Option<i64>,
    /// manual | cron | webhook | feishu_message | feishu_command |
    /// todo_completed | todo_state_changed | tag_added
    pub trigger_type: String,
    #[sea_orm(default_value = "{}")]
    pub trigger_meta: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// running | success | failed | partial | cancelled
    #[sea_orm(default_value = "running")]
    pub status: String,
    #[sea_orm(default_value = "0")]
    pub total_steps: i32,
    #[sea_orm(default_value = "0")]
    pub completed_steps: i32,
    #[sea_orm(default_value = "0")]
    pub failed_steps: i32,
    /// 累计执行过的 step 次数（含循环重走）
    #[sea_orm(default_value = "0")]
    pub total_executed_steps: i32,
    /// 执行失败时的错误说明（如工作空间不一致）。仅在 status=failed 时有值。
    pub error_message: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::loops::Entity",
        from = "Column::LoopId",
        to = "super::loops::Column::Id"
    )]
    BelongsToLoop,
    #[sea_orm(
        belongs_to = "super::loop_triggers::Entity",
        from = "Column::TriggerId",
        to = "super::loop_triggers::Column::Id"
    )]
    BelongsToTrigger,
    #[sea_orm(has_many = "super::loop_step_executions::Entity")]
    LoopStepExecutions,
}

impl Related<super::loops::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoop.def() }
}

impl Related<super::loop_triggers::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToTrigger.def() }
}

impl Related<super::loop_step_executions::Entity> for Entity {
    fn to() -> RelationDef { Relation::LoopStepExecutions.def() }
}

impl ActiveModelBehavior for ActiveModel {}
