use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 环路触发器：决定 loop 何时被启动。
///
/// trigger_type 决定 config JSON 的 schema（见 migrations.rs v2 注释）。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loop_triggers")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub loop_id: i64,
    /// manual | cron | webhook | feishu_message | feishu_command |
    /// todo_completed | todo_state_changed | tag_added
    pub trigger_type: String,
    #[sea_orm(default_value = "{}")]
    pub config: String,
    #[sea_orm(default_value = "1")]
    pub enabled: i32,
    /// 同一 loop 多个触发器同时命中时,priority 大的优先触发。0 为默认。
    #[sea_orm(default_value = "0")]
    pub priority: i32,
    pub created_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::loops::Entity",
        from = "Column::LoopId",
        to = "super::loops::Column::Id"
    )]
    BelongsToLoop,
}

impl Related<super::loops::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoop.def() }
}

impl ActiveModelBehavior for ActiveModel {}
