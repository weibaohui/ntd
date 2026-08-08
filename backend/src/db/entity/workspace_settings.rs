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
    /// 工作空间级共识 prompt（需求 022）。
    /// 该 workspace 下任意 todo 执行时，适配层把这段 prompt 拼到 message 最前面，
    /// 内容包括产物目录、认证信息、基本文件路径等共识信息。
    /// None 表示未配置（读取时跳过拼接）；空串 "" 表示显式清空。
    pub system_prompt: Option<String>,
    /// 工作空间级「委派接力轮数上限」默认（需求 092 护栏配置化）。
    /// NULL=未配置 → 回退终极兜底常量 MAX_DELEGATE_ROUNDS；任务级 delegate_max_rounds 可再覆盖之。
    pub delegate_max_rounds: Option<i64>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
