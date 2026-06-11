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
    /// 验收标准（自动评审时作为评审 prompt 的一部分）。
    pub acceptance_criteria: Option<String>,
    /// 0=普通 todo, 1=评审师模板（系统自动维护的专用 todo）,
    /// 2=自动评审任务（由 spawn 出来的、跑在用户视角下可见的评审实例）.
    /// 普通 todo 不需要这个字段; 但为了避免 schema 变更，所有行都有值.
    pub todo_type: Option<i32>,
    /// 当 todo_type=2 (review instance) 时, 关联到被评审的原 todo.
    pub parent_todo_id: Option<i64>,
    /// 是否在执行完成后自动派生一个评审 todo (默认 true, 只对 normal 类型有效).
    pub auto_review_enabled: Option<bool>,
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
