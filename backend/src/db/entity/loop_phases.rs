use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Loop 内的阶段（Phase），聚合若干 loop_steps。
///
/// 阶段有独立的规范、验收标准，并在执行时生成 loop_phase_executions。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loop_phases")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属 Loop ID。
    pub loop_id: i64,
    /// 阶段标识名称，如 `requirement`、`design`。
    pub name: String,
    /// 阶段描述。
    #[sea_orm(column_type = "Text")]
    pub description: String,
    /// 阶段在 Loop 内的排序。
    #[sea_orm(default_value = "0")]
    pub order_index: i32,
    /// 阶段规范说明（Markdown）。
    #[sea_orm(column_type = "Text")]
    pub spec: String,
    /// 阶段验收标准（Markdown）。
    #[sea_orm(column_type = "Text")]
    pub acceptance_criteria: String,
    /// 是否启用：1 启用，0 禁用。
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
}

impl Related<super::loops::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoop.def() }
}

impl ActiveModelBehavior for ActiveModel {}
