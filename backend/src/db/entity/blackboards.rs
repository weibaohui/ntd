use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 黑板（Blackboard）实体：每个工作空间维护一个黑板，由 LLM 自动维护。
///
/// 每个 workspace 最多一条记录（workspace_id 为 UNIQUE），
/// content 字段存储 Markdown 格式的黑板内容。
/// pending_record_ids 暂存待处理的 execution_record_id，防抖批次处理时使用。
/// 防抖阈值与 Wiki 提示词模板均为 per-workspace 配置，存储在本表。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "blackboards")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 工作空间 ID（唯一），关联 project_directories(id)
    pub workspace_id: i64,
    /// 黑板 Markdown 内容
    #[sea_orm(column_type = "Text")]
    pub content: String,
    /// 待处理的 execution_record_id 队列（JSON 数组），防抖周期到点后统一处理
    #[sea_orm(column_type = "Text")]
    pub pending_record_ids: String,
    /// 黑板更新防抖周期（秒），达到该时间后统一处理 pending 队列
    pub blackboard_debounce_secs: i64,
    /// 黑板更新防抖条数阈值，达到该条数后立即触发，无需等待周期
    pub blackboard_debounce_count: i64,
    /// Wiki 单阶段维护提示词模板（占位符 {{workspace_id}}、{{pending_record_ids}}）。
    /// LLM 直接编辑 wiki 目录下的 Markdown 文件，空字符串表示使用内置默认模板。
    #[sea_orm(column_type = "Text")]
    pub wiki_prompt: String,
    /// Wiki 对话使用的执行器名称（如 "claudecode"、"codex" 等）。
    /// None 或空字符串表示使用默认值 "claudecode"。
    /// 该字段为 per-workspace 独立配置，与 workspace_settings.default_response_executor 互不影响。
    pub wiki_chat_executor: Option<String>,
    /// Wiki 对话各执行器的 session ID（JSON 对象，key 为执行器名称，value 为 session_id 或 null）。
    /// 例如：{"claudecode": "uuid-session-1", "hermes": "uuid-session-2", "opencode": null}
    /// 切换执行器时不会丢失 session，支持连续对话。
    #[sea_orm(column_type = "Text")]
    pub wiki_chat_sessions: Option<String>,
    /// Wiki 执行超时（秒）。控制 `update_blackboard_wiki` 等待 Finished 事件、
    /// 以及 Wiki 对话子进程的最长存活时间。默认 300（5 分钟），
    /// 可在黑板设置界面按工作空间调整，避免慢模型被强制超时。
    pub wiki_timeout_secs: i64,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
