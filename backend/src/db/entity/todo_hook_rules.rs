use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "todo_hook_rules")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub todo_hook_id: i64,
    pub hook_id: Option<i64>,
    pub inline_hook: Option<String>,
    pub priority: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
