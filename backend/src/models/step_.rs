use serde::{Deserialize, Serialize};

use crate::db::entity::steps;

/// 环节 DTO：独立的实体，不再寄生在 Todo 上。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDto {
    pub id: i64,
    pub title: String,
    pub prompt: String,
    pub executor: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub source_todo_id: Option<i64>,
    /// 被多少个 loop step 引用
    pub used_by_loop_step_count: i64,
    /// 标签 ID 列表（单选，复用 Todo 的标签体系）
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl From<steps::Model> for StepDto {
    fn from(m: steps::Model) -> Self {
        Self {
            id: m.id,
            title: m.title,
            prompt: m.prompt,
            executor: m.executor,
            acceptance_criteria: m.acceptance_criteria,
            source_todo_id: m.source_todo_id,
            used_by_loop_step_count: 0,
            tag_ids: vec![],
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

impl StepDto {
    pub fn with_usage(mut self, count: i64) -> Self {
        self.used_by_loop_step_count = count;
        self
    }

    /// 将 handler 从 step_tags 关联表查询到的标签注入 DTO，避免 `From<steps::Model>` 依赖额外查询。
    /// 标签信息由 handler 在 DB 事务边界外统一获取后调用此方法装配，保持转换层的纯净职责。
    pub fn with_tags(mut self, tag_ids: Vec<i64>) -> Self {
        self.tag_ids = tag_ids;
        self
    }
}

/// 更新环节请求体。
///
/// 所有字段均可选，实现"部分更新"语义 —— 只传需要变更的字段，
/// 未传的字段从数据库保持原值。批量更换执行器时只需传 `executor`。
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateStepRequest {
    pub title: Option<String>,
    pub prompt: Option<String>,
    pub executor: Option<String>,
    pub acceptance_criteria: Option<String>,
    /// 可选更新的标签 ID（单选）；传空数组或无字段表示不更新标签
    #[serde(default)]
    pub tag_ids: Option<Vec<i64>>,
}

/// 直接创建环节请求体。
///
/// 历史上"新建环节"由前端走 createTodo + promoteTodoToStep 间接完成，
/// 这导致：(1) 每次新建环节都额外插一条 todo（孤儿），(2) promote 后
/// step id 与 todo id 不一致，前端选错 id 触发 404。
/// 现在 todo 和 step 已经彻底拆开，前端应直接 POST /api/steps。
/// `title` 必填，其余字段可空。
#[derive(Debug, Clone, Deserialize)]
pub struct CreateStepRequest {
    pub title: String,
    pub prompt: Option<String>,
    pub executor: Option<String>,
    pub acceptance_criteria: Option<String>,
}

/// 批量更新环节执行器请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchUpdateStepExecutorRequest {
    pub ids: Vec<i64>,
    pub executor: String,
}

/// 批量更新环节执行器返回结果。
#[derive(Debug, Clone, Serialize)]
pub struct BatchUpdateStepResult {
    pub updated_count: i64,
    pub total: i64,
}
