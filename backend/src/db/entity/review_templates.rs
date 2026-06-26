//! `review_templates` 表的 SeaORM 实体。
//!
//! 历史：评审模板曾以 `todos.todo_type=1`（标题"评审任务"）兼任；V15 迁移把
//! 这部分数据搬到独立的 `review_templates` 表。该实体是新的"一等公民"，
//! 与 `todo_templates`（独立功能：可导入的 todo 模板库）**不混用**。
//!
//! 字段语义：
//! - `name`：在 loop 编辑器下拉里展示用，业务层保证唯一（DAO 在
//!   create/update 时校验；不在 schema 上加 UNIQUE 约束以兼容历史脏数据）。
//! - `description`：下拉副标题，nullable 允许"裸名"模板存在。
//! - `prompt`：评审师提示词原文，**含占位符**（`{original_prompt}`、
//!   `{acceptance_criteria}` 等），由调用方在运行时替换。
//! - `id`：INTEGER PRIMARY KEY（**无 AUTOINCREMENT**），允许重用已删除 id，
//!   让遗留 type=1 todo 升级时能迁入到原 id，保留 loops.review_template_id FK。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "review_templates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub prompt: String,
    /// 所属工作空间（目录路径）。None = 全局模板（不限制工作空间）。
    pub workspace: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// 关系预留：当前 review_templates 没有外键到其他表（loops.review_template_id
/// 是逻辑引用，不是 DB 级 FK；见 plan 文档）。后续如果需要 ORM 级 relation，
/// 在这里加 DeriveRelation 的 enum 变体。
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}