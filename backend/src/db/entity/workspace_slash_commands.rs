use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 工作空间斜杠命令表：workspace_id + slash_command → todo_id / loop_id
///
/// 替代原有的 Config.slash_command_rules，实现按工作空间的斜杠命令隔离。
/// command_type 决定是触发 todo 还是 loop。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_slash_commands")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属工作空间 ID
    pub workspace_id: i64,
    /// 斜杠命令名称，如 "/todo"
    pub slash_command: String,
    /// 命令类型：'todo' 或 'loop'
    pub command_type: String,
    /// 绑定的 Todo ID（command_type='todo' 时使用）
    pub todo_id: i64,
    /// 绑定的 Loop ID（command_type='loop' 时使用）
    pub loop_id: Option<i64>,
    /// 是否启用
    pub enabled: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
