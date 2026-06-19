use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
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
    /// Only meaningful on terminal records (success/failed).
    pub rating: Option<i32>,
    /// 精确指向"由哪条执行记录发起的自动评审"——自动评审只回填到这条记录上,
    /// 不会污染同一 todo 后续 re-run 的执行记录.
    /// NULL = 这条记录不是被自动评审的产物.
    pub source_execution_record_id: Option<i64>,
    /// 自动评审的状态:
    ///   - NULL        : 还未评审过 (初始)
    ///   - 'pending'   : 已 spawn 评审 todo, 但还没跑完
    ///   - 'success'   : 评审完成, 已写入 rating
    ///   - 'failed'    : 评审 todo 失败
    ///   - 'interrupted' : 评审 todo 中断
    ///   - 'skipped'   : 评审被显式跳过 (例如 todo_type=review 自身不触发)
    pub last_review_status: Option<String>,
    /// 最近一次评审 spawn 的 UTC 时间戳.
    pub last_reviewed_at: Option<String>,
    /// issue #643: 本次执行实际使用的 git worktree 目录路径。
    /// NULL = 未启用 worktree 或尚未确定目录。`auto_cleanup = true` 时，执行结束后
    /// 该目录会被 WorktreeService 删除，但 `worktree_path` 字段会保留在记录里便于排查。
    pub worktree_path: Option<String>,
    /// 当本次执行是 loop 环节的一部分时，指向 loop_step_executions 表的 id。
    pub loop_step_execution_id: Option<i64>,
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
