use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "hook_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub hook_id: Option<i64>,
    pub hook_name: Option<String>,
    pub trigger: String,
    pub todo_id: Option<i64>,
    pub args_sent: Option<String>,
    pub env_sent: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: Option<i64>,
    pub success: Option<bool>,
    pub error_msg: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
