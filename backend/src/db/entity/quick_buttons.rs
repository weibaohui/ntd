use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 快捷话术按钮表：button_name（全局唯一）+ prompt_text。
///
/// 全局共享（无 workspace 维度）。用于帖子流回复框上方的自定义快捷按钮：
/// 点击把 prompt_text 填入回复输入框，用户确认后走 resume 继续会话。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "quick_buttons")]
pub struct Model {
    #[sea_orm(primary_key)]
    /// 自增主键
    pub id: i64,
    /// 按钮显示名称，全局唯一（DB UNIQUE 约束 + handler 预检双重保证）
    pub button_name: String,
    /// 点击按钮后填入回复输入框的预设话术
    pub prompt_text: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
