use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 工作空间设置表：存储每个工作空间的独立配置
///
/// 存储默认响应配置，支持三种类型：todo、loop、executor。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_settings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 工作空间 ID（唯一）
    pub workspace_id: i64,
    /// 默认响应类型：'todo' | 'loop' | 'executor'
    pub default_response_type: String,
    /// 默认响应 Todo ID（type='todo' 时使用）
    pub default_response_todo_id: Option<i64>,
    /// 默认响应 Loop ID（type='loop' 时使用）
    pub default_response_loop_id: Option<i64>,
    /// 默认响应执行器类型（type='executor' 时使用）
    pub default_response_executor: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
