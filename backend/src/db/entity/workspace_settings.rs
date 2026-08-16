use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 工作空间设置表：存储每个工作空间的独立配置
///
/// 聊天直连配置（108 修订）：未命中斜杠命令的消息进聊天直连——
/// 单聊与「对话执行器」纯直聊；群聊由群聊管家处理（专家人设 + 执行器）。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_settings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 工作空间 ID（唯一）
    pub workspace_id: i64,
    /// 群聊管家的专家名（对应 ExpertIndexManager 中的专家 name，不是 id）。
    /// 仅群聊消费（butler_chat 注入）；单聊直聊（dm_chat）不读此字段。
    /// None 表示未配置：群聊管家退化为纯执行器聊天（无专家 prompt 注入）；
    /// 空串 "" 表示显式清空（语义同 None，保留写入侧「清空」与「不动」的区分）。
    pub butler_expert_name: Option<String>,
    /// 对话执行器类型（如 claudecode / pi）：单聊直聊与群聊管家共用的执行进程。
    /// None 或空串 "" 都表示未配置管家（前端清空选择时提交空串）：
    /// 未命中斜杠命令的消息收到配置引导提示，不执行任何东西。
    /// 下游读取方统一按「空=未配置」过滤（resolve_butler_executor / workspace_butler_executor）。
    pub butler_executor: Option<String>,
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
