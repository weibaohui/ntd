use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Loop 与 Tag 的关联表（环路标签）。
///
/// 设计与 todo_tags 保持一致：联合主键 (loop_id, tag_id)，
/// 级联删除确保删除 loop 时自动清理关联记录。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loop_tags")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub loop_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub tag_id: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
