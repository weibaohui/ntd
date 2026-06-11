use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "execution_records")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub todo_id: Option<i64>,
    pub status: Option<String>,
    pub command: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub result: Option<String>,
    pub usage: Option<String>,
    pub executor: Option<String>,
    pub model: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub trigger_type: Option<String>,
    pub pid: Option<i32>,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub todo_progress: Option<String>,
    pub execution_stats: Option<String>,
    pub resume_message: Option<String>,
    /// When `trigger_type` is `hook:*`, the id of the source todo whose hook
    /// fired this execution. NULL for manual / cron / webhook / feishu
    /// triggers.
    pub source_todo_id: Option<i64>,
    /// Snapshot of the source todo's title at trigger time, so the UI can
    /// render "triggered by todo #X 'Title'" without joining `todos` (the
    /// source may be deleted or renamed later).
    pub source_todo_title: Option<String>,
    /// The `TodoHookItem.id` that fired. Combined with `source_todo_id` this
    /// points at the exact hook entry that triggered this execution.
    pub source_hook_id: Option<i64>,
    /// User-provided score for this execution's result (0-100, optional).
    /// Only meaningful on terminal records (success/failed); running records
    /// never carry a score.
    pub rating: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::todos::Entity",
        from = "Column::TodoId",
        to = "super::todos::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Todos,
}

impl Related<super::todos::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Todos.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
