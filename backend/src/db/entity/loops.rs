use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Loop 主体（一个自动化场景）。
///
/// 编排多个 todo 顺序执行、附带多种触发器。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loops")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    #[sea_orm(default_value = "")]
    pub description: String,
    pub workspace: Option<String>,
    /// enabled | paused
    #[sea_orm(default_value = "paused")]
    pub status: String,
    #[sea_orm(default_value = "#722ed1")]
    pub color: String,
    #[sea_orm(default_value = "loop")]
    pub icon: String,
    pub review_template_id: Option<i64>,
    /// JSON 全局限制配置: {"max_step_executions": 20, "max_total_tokens": null}
    #[sea_orm(default_value = "{}")]
    pub limits_config: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::loop_triggers::Entity")]
    LoopTriggers,
    #[sea_orm(has_many = "super::loop_steps::Entity")]
    LoopSteps,
    #[sea_orm(has_many = "super::loop_executions::Entity")]
    LoopExecutions,
}

impl Related<super::loop_triggers::Entity> for Entity {
    fn to() -> RelationDef { Relation::LoopTriggers.def() }
}

impl Related<super::loop_steps::Entity> for Entity {
    fn to() -> RelationDef { Relation::LoopSteps.def() }
}

impl Related<super::loop_executions::Entity> for Entity {
    fn to() -> RelationDef { Relation::LoopExecutions.def() }
}

impl ActiveModelBehavior for ActiveModel {}
