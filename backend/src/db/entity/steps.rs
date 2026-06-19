use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 环节：从 todo 提升而来的独立实体。
///
/// 仅保留标题、提示词、执行器、验收标准，不含 hook/定时/门禁等 todo 属性。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "steps")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub title: String,
    #[sea_orm(default_value = "")]
    pub prompt: String,
    pub executor: Option<String>,
    pub acceptance_criteria: Option<String>,
    /// 来源 todo id（仅记录，不影响 todo）
    pub source_todo_id: Option<i64>,
    #[sea_orm(default_value = "#722ed1")]
    pub color: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
