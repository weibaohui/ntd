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
    pub color: String,
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
            color: m.color,
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
    pub color: Option<String>,
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
