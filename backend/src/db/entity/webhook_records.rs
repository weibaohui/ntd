use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "webhook_records")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub webhook_id: Option<i64>,
    pub method: String,
    pub path: String,
    pub query_params: Option<String>,
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub triggered_todo_id: Option<i64>,
    pub status_code: Option<i32>,
    pub response_body: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
