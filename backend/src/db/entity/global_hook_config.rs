use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "global_hook_config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub enabled: Option<bool>,
    pub default_timeout_secs: Option<i64>,
    pub max_concurrency: Option<i64>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
