//! blackboard_pages 实体：黑板 Wiki 化后的多页面存储。
//!
//! 每个工作空间维护一组页面（index / topic / log），
//! 取代旧版 blackboards.content 单文件模式。
//! slug 在同一 workspace 内唯一，由 LLM 生成（如 "auth-module"）。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 黑板页面实体：一个 workspace 下的一篇 Wiki 页面。
///
/// page_type 区分页面角色：
/// - index：目录页（后端自动生成，列出所有 topic 页摘要）
/// - topic：主题页（LLM 产出，按领域归类的执行结论）
/// - log：日志页（后端自动生成，按时间记录每次摄入操作）
/// - analysis：预留，后期提问功能产出的分析页
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "blackboard_pages")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 工作空间 ID，关联 project_directories(id)
    pub workspace_id: i64,
    /// 页面类型：index / topic / log（analysis 预留）
    #[sea_orm(column_type = "Text")]
    pub page_type: String,
    /// 页面唯一标识，同一 workspace 内唯一，LLM 生成（如 "auth-module"）
    #[sea_orm(column_type = "Text")]
    pub slug: String,
    /// 显示标题（如 "认证模块"）
    #[sea_orm(column_type = "Text")]
    pub title: String,
    /// 一句话摘要，用于 index 页面和目录树展示
    #[sea_orm(column_type = "Text")]
    pub summary: String,
    /// Markdown 内容
    #[sea_orm(column_type = "Text")]
    pub content: String,
    /// 来源 execution_record_id 列表（JSON 数组），记录本页面整合了哪些执行结论
    #[sea_orm(column_type = "Text")]
    pub source_refs: String,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
