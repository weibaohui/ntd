use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 环路阶段：loop 的一个有序步骤，绑定一个 todo。
///
/// 首版仅支持 sequential 执行；run_mode 字段预留扩展。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loop_steps")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub loop_id: i64,
    pub name: String,
    #[sea_orm(default_value = "")]
    pub description: String,
    #[sea_orm(default_value = "0")]
    pub order_index: i32,
    /// 关联的 todo id
    pub todo_id: i64,
    /// sequential (reserved for parallel)
    #[sea_orm(default_value = "sequential")]
    pub run_mode: String,
    /// 上游阶段失败时是否跳过本阶段
    #[sea_orm(default_value = "0")]
    pub skip_on_source_failed: i32,
    /// 0-100 评分闸门；NULL 表示无闸门
    pub min_rating: Option<i32>,
    /// skip | pass
    #[sea_orm(default_value = "skip")]
    pub unrated_policy: String,
    /// 成功时策略: "next" | "goto" | "end"
    #[sea_orm(default_value = "next")]
    pub on_success: String,
    /// on_success="goto" 时的目标 step_id
    pub success_goto_step_id: Option<i64>,
    /// 评分不通过时策略: "break" | "skip" | "goto" | "end"
    #[sea_orm(default_value = "break")]
    pub on_rating_fail: String,
    /// on_rating_fail="goto" 时的目标 step_id
    pub fail_goto_step_id: Option<i64>,
    /// 评审类型: "ai" = AI 自动评审, "human" = 人工审批（默认 "ai"）
    #[sea_orm(default_value = "ai")]
    pub review_type: String,
    #[sea_orm(default_value = "1")]
    pub enabled: i32,
    pub created_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::loops::Entity",
        from = "Column::LoopId",
        to = "super::loops::Column::Id"
    )]
    BelongsToLoop,
    #[sea_orm(
        belongs_to = "super::todos::Entity",
        from = "Column::TodoId",
        to = "super::todos::Column::Id"
    )]
    BelongsToTodo,
}

impl Related<super::loops::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoop.def() }
}

impl Related<super::todos::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToTodo.def() }
}

impl ActiveModelBehavior for ActiveModel {}
