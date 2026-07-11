use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "project_directories")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub path: String,
    pub name: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// issue #643: 项目目录级开关，开启后 ntd 在该目录下执行 Todo 时自动创建 git worktree。
    /// 默认 false，旧库通过迁移加列后保持 false，对存量行为零影响。
    #[sea_orm(default)]
    pub git_worktree_enabled: bool,
    /// issue #643: 执行结束（成功/失败/取消）后是否自动清理 worktree。
    /// 仅在 `git_worktree_enabled = true` 时才有意义；前端会在 UI 上把"自动清理"
    /// 开关禁用到 `git_worktree_enabled` 之外，确保不会出现"开了清理但没开 worktree"的废组合。
    #[sea_orm(default)]
    pub auto_cleanup: bool,
    /// 私聊默认响应执行器的 session_id 映射，JSON 对象格式：
    /// `{ "claudecode": "ses_xxx", "zhanlu": "ses_yyy" }`
    /// 用于在私聊场景下继续同一会话（resume），实现多轮对话体验。
    pub executor_sessions: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}