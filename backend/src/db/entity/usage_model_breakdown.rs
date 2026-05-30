use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Model breakdown stored separately for detailed per-model statistics
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "usage_model_breakdowns")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Reference to usage_daily_stats id
    pub daily_stat_id: i64,
    /// Model name
    pub model_name: String,
    /// Input tokens for this model
    pub input_tokens: i64,
    /// Output tokens for this model
    pub output_tokens: i64,
    /// Cache creation tokens for this model
    pub cache_creation_tokens: i64,
    /// Cache read tokens for this model
    pub cache_read_tokens: i64,
    /// Extra total tokens for this model
    pub extra_total_tokens: i64,
    /// Cost for this model
    pub cost: f64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::usage_stats::Entity",
        from = "Column::DailyStatId",
        to = "super::usage_stats::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    UsageDailyStats,
}

impl Related<super::usage_stats::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UsageDailyStats.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
