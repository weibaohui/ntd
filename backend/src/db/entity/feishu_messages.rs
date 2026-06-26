use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "feishu_messages")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub bot_id: i64,
    pub message_id: String,
    pub chat_id: String,
    pub chat_type: String,
    pub sender_open_id: String,
    pub sender_nickname: Option<String>,
    pub sender_type: Option<String>,
    pub content: Option<String>,
    pub msg_type: String,
    pub is_mention: Option<bool>,
    pub processed: Option<bool>,
    pub processed_todo_id: Option<i64>,
    pub execution_record_id: Option<i64>,
    pub is_history: Option<bool>,
    pub fetch_time: Option<String>,
    pub created_at: Option<String>,
    /// 消息接收时，智能体所属的工作空间 ID
    pub workspace_id: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
