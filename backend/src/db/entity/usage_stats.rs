use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "usage_daily_stats")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Date in YYYY-MM-DD format
    pub date: String,
    /// Project path (optional, from session)
    pub project_path: Option<String>,
    /// Session ID (optional)
    pub session_id: Option<String>,
    /// Input token count
    pub input_tokens: i64,
    /// Output token count
    pub output_tokens: i64,
    /// Cache creation token count
    pub cache_creation_tokens: i64,
    /// Cache read token count
    pub cache_read_tokens: i64,
    /// Extra total tokens (e.g., reasoning tokens)
    pub extra_total_tokens: i64,
    /// Total cost in USD
    pub total_cost: f64,
    /// Credits used (optional)
    pub credits: Option<f64>,
    /// Message count
    pub message_count: Option<i64>,
    /// Models used (JSON array of strings)
    pub models_used: String,
    /// Project name (optional)
    pub project: Option<String>,
    /// Versions (JSON array of strings, optional)
    pub versions: Option<String>,
    /// Last activity timestamp
    pub last_activity: Option<String>,
    /// Statistics type: daily, weekly, monthly
    pub stats_type: String,
    /// Created at
    pub created_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
