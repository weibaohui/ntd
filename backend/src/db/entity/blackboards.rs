use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 黑板（Blackboard）实体：每个工作空间维护一个黑板，由 LLM 自动维护。
///
/// 每个 workspace 最多一条记录（workspace_id 为 UNIQUE），
/// content 字段存储 Markdown 格式的黑板内容。
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
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
