use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 飞书聊天 ↔ 项目目录绑定关系表
///
/// 设计意图：将飞书会话（私聊/群聊）绑定到特定项目目录，支持多轮对话的会话恢复。
/// - Web UI 创建时 chat_id = "__pending__"，等待飞书侧 /bind 补齐真实 chat_id
/// - 飞书 /bind 命令可直接创建完整绑定或补齐 pending 绑定的 chat_id
/// - 绑定后每条飞书消息都通过 Claude Code 在该项目目录下执行
/// - 首次创建新 session，后续消息 resume 同一 session 保持上下文
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "feishu_project_bindings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,

    /// 关联的飞书 Bot ID — 逻辑外键，删除 Bot 时业务层负责清理
    pub bot_id: i64,

    /// 飞书聊天 ID，与 bot_id 组成 UNIQUE 约束，一个聊天只能绑定一个项目
    /// 特殊值 "__pending__" 表示 Web UI 创建的待绑定状态
    pub chat_id: String,

    /// 聊天类型："p2p"（私聊）或 "group"（群聊）
    pub chat_type: String,

    /// 关联的项目目录 ID → project_directories.id
    /// Claude Code 在此目录下运行 --worktree 模式
    pub project_dir_id: i64,

    /// 该项目对应的 Todo ID，所有对话历史关联到该 Todo
    /// 自动创建时 title="飞书-<项目名>"
    pub todo_id: i64,

    /// 当前 Claude Code session_id（用于 resume）
    /// - None：尚未执行过任何任务
    /// - Some：首次执行时由 update_feishu_project_binding_session 填充
    ///   后续 resume 执行保持同一 session_id
    pub session_id: Option<String>,

    /// 最近一次 execution_record.id（用于检查执行状态）
    /// - None：尚未执行
    /// - Some：每次执行后更新（包括 resume），指向最新一条记录
    pub latest_record_id: Option<i64>,

    /// 绑定状态："idle"（空闲）或 "running"（执行中）
    /// 转换：idle → running（start execution）→ idle（execution finished/cleanup）
    /// 对 running 状态可发起 resume 执行（新增 execution_record 但不改 session_id）
    pub status: String,

    /// 创建时间（ISO 8601 UTC）
    pub created_at: String,

    /// 最后更新时间
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
