use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Bot 模型：每个 bot 必须归属于一个工作空间。
///
/// workspace_id 字段标识 bot 所属的工作空间，实现 bot 与 workspace 的一对一绑定。
/// 创建 bot 时必须指定 workspace_id（不可为 NULL）。
/// 变更 bot 的 workspace_id 时，其全部聊天绑定会失效。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "agent_bots")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub bot_type: String,
    pub bot_name: String,
    pub app_id: String,
    pub app_secret: String,
    pub bot_open_id: Option<String>,
    /// 所有者（创建者/默认接收人）的 open_id：推送目标的权威来源。
    /// 扫码创建时写入；非扫码创建的 bot 由首次私聊兜底写入（仅当为空）。
    /// 与 bot_open_id 的区别：后者是历史字段，扫码时被错位写入了扫码人 open_id，
    /// 本次重构不改动它，新增本字段做语义正名。
    pub owner_open_id: Option<String>,
    pub domain: Option<String>,
    /// Bot 所属的工作空间 ID（不可为 NULL）
    pub workspace_id: i64,
    pub enabled: Option<bool>,
    pub config: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
