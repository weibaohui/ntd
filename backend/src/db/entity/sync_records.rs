//! Sync records entity
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "sync_records")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_name = "direction")]
    pub direction: String,
    #[sea_orm(column_name = "conflict_mode")]
    pub conflict_mode: String,
    #[sea_orm(column_name = "status")]
    pub status: String,
    #[sea_orm(column_name = "data_type")]
    pub data_type: String,
    #[sea_orm(column_name = "details")]
    pub details: Option<String>,
    #[sea_orm(column_name = "error_message")]
    pub error_message: Option<String>,
    #[sea_orm(column_name = "created_at")]
    pub created_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
