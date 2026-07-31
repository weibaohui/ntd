use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 环路阶段：loop 的一个有序步骤，关联一个 todo。
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
    /// 环节级评审模板正文（完整模板，含占位符）；NULL = 未设置，评审时回退到环路级/默认。
    pub review_prompt: Option<String>,
    /// 所属阶段 ID，NULL 表示未分组（兼容旧 Loop）。
    pub phase_id: Option<i64>,
    /// 期望产物配置（JSON 数组）。
    #[sea_orm(default_value = "[]")]
    pub expected_artifacts: String,
    /// 门禁配置（JSON 数组）。
    #[sea_orm(default_value = "[]")]
    pub gate_config: String,
    /// 最大返工次数，默认 3。
    #[sea_orm(default_value = "3")]
    pub max_rework: i32,
    /// 本环节使用的 skill 名称列表（JSON 数组）。
    #[sea_orm(default_value = "[]")]
    pub skill_names: String,
    /// 专家/团队名称。
    pub expert_name: Option<String>,
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
    #[sea_orm(
        belongs_to = "super::loop_phases::Entity",
        from = "Column::PhaseId",
        to = "super::loop_phases::Column::Id"
    )]
    BelongsToPhase,
}

impl Related<super::loops::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoop.def() }
}

impl Related<super::todos::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToTodo.def() }
}

impl Related<super::loop_phases::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToPhase.def() }
}

impl ActiveModelBehavior for ActiveModel {}
