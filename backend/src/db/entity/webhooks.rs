use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Webhook 类型：区分是绑定 todo 还是绑定 loop。
/// - "todo": 通过 /webhook/trigger/{id}/todo 触发，执行对应 todo
/// - "loop": 通过 /webhook/trigger/{id}/loop 触发，触发对应 loop 的 webhook 触发器
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebhookType {
    #[serde(rename = "todo")]
    Todo,
    #[serde(rename = "loop")]
    Loop,
}

impl Default for WebhookType {
    fn default() -> Self { WebhookType::Todo }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "webhooks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    /// 绑定的默认 todo（仅 webhook_type = "todo" 时有效）
    pub default_todo_id: Option<i64>,
    /// 绑定的 loop（仅 webhook_type = "loop" 时有效）
    pub loop_id: Option<i64>,
    /// 类型：todo | loop
    #[sea_orm(default_value = "todo")]
    pub webhook_type: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
