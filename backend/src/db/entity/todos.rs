use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "todos")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub title: String,
    pub prompt: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub deleted_at: Option<String>,
    pub executor: Option<String>,
    pub scheduler_enabled: Option<bool>,
    pub scheduler_config: Option<String>,
    pub scheduler_timezone: Option<String>,
    pub task_id: Option<String>,
    pub workspace: Option<String>,
    pub worktree_enabled: Option<bool>,
    /// Inline hook definitions stored as a JSON array of `TodoHookItem`.
    /// Each item binds a trigger to a target todo that should run when the
    /// parent todo's lifecycle event matches.
    pub hooks: Option<String>,
    pub acceptance_criteria: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::execution_records::Entity")]
    ExecutionRecords,
}

impl Related<super::execution_records::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ExecutionRecords.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
