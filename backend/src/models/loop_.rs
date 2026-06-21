//! Loop Studio 数据模型（API DTO）。
//!
//! 与 `db::entity::loops` 等实体不同：
//! - 实体是 SeaORM 自动派生的,直接对应数据库行
//! - 这里定义的是面向 API 层的 DTO,经过 snake_case / camelCase 转换、字段精简、
//!   嵌套结构组装,直接给前端消费
use serde::{Deserialize, Serialize};

use crate::db::entity::{
    loop_executions, loop_step_executions, loop_steps, loop_triggers, loops,
};
use crate::db::loop_::{LoopFullView, LoopListRow};
use crate::models::TodoStatus;

/// Loop 列表行(左栏一行)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopListItem {
    #[serde(flatten)]
    pub loop_: LoopDto,
    pub trigger_count: i32,
    pub step_count: i32,
    pub last_execution_status: String,
    pub last_execution_at: Option<String>,
}

impl From<LoopListRow> for LoopListItem {
    fn from(row: LoopListRow) -> Self {
        Self {
            loop_: row.loop_.into(),
            trigger_count: row.trigger_count,
            step_count: row.step_count,
            last_execution_status: row.last_execution_status,
            last_execution_at: row.last_execution_at,
        }
    }
}

/// Loop 详情(基本+子项完整数据),LoopStudio 详情页一次拿到。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDetail {
    #[serde(flatten)]
    pub loop_: LoopDto,
    pub triggers: Vec<LoopTriggerDto>,
    pub steps: Vec<LoopStepDto>,
    /// todo_id -> TodoDto,前端展示 step 关联的 todo 信息时直接 lookup
    pub todo_map: std::collections::HashMap<i64, TodoSummary>,
}

impl From<LoopFullView> for LoopDetail {
    fn from(view: LoopFullView) -> Self {
        let todo_map = view
            .todo_map
            .into_iter()
            .map(|(id, t)| {
                (
                    id,
                    TodoSummary {
                        id: t.id,
                        title: t.title,
                        status: t.status.unwrap_or_default(),
                        executor: t.executor.unwrap_or_default(),
                    },
                )
            })
            .collect();
        let steps = view
            .steps_meta
            .into_iter()
            .map(|(s, todo_title, todo_executor, todo_status): (loop_steps::Model, String, String, String)| LoopStepDto {
                step: s.into(),
                todo_title,
                todo_executor,
                todo_status,
            })
            .collect();
        Self {
            loop_: view.loop_.into(),
            triggers: view.triggers.into_iter().map(Into::into).collect(),
            steps,
            todo_map,
        }
    }
}

/// 极简的 todo 摘要(嵌在 loop detail 里),不暴露 prompt 等敏感字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoSummary {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub executor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDto {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub workspace: Option<String>,
    pub status: String,
    pub color: String,
    pub icon: String,
    pub review_template_id: Option<i64>,
    pub limits_config: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl From<loops::Model> for LoopDto {
    fn from(m: loops::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            description: m.description,
            workspace: m.workspace,
            status: m.status,
            color: m.color,
            icon: m.icon,
            review_template_id: m.review_template_id,
            limits_config: m.limits_config,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopTriggerDto {
    pub id: i64,
    pub loop_id: i64,
    pub trigger_type: String,
    pub config: String,
    pub enabled: bool,
    pub priority: i32,
    pub created_at: Option<String>,
}

impl From<loop_triggers::Model> for LoopTriggerDto {
    fn from(m: loop_triggers::Model) -> Self {
        Self {
            id: m.id,
            loop_id: m.loop_id,
            trigger_type: m.trigger_type,
            config: m.config,
            enabled: m.enabled != 0,
            priority: m.priority,
            created_at: m.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStepDto {
    #[serde(flatten)]
    pub step: LoopStepRawDto,
    /// 冗余字段,JOIN 时一并查出来,避免前端再请求 todo 详情
    pub todo_title: String,
    pub todo_executor: String,
    pub todo_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStepRawDto {
    pub id: i64,
    pub loop_id: i64,
    pub name: String,
    pub description: String,
    pub order_index: i32,
    /// 关联的 step id（对应 steps 表）
    pub step_id: i64,
    pub run_mode: String,
    pub skip_on_source_failed: bool,
    pub min_rating: Option<i32>,
    pub unrated_policy: String,
    pub on_success: String,
    pub success_goto_step_id: Option<i64>,
    pub on_rating_fail: String,
    pub fail_goto_step_id: Option<i64>,
    pub enabled: bool,
    pub created_at: Option<String>,
}

impl From<loop_steps::Model> for LoopStepRawDto {
    fn from(m: loop_steps::Model) -> Self {
        Self {
            id: m.id,
            loop_id: m.loop_id,
            name: m.name,
            description: m.description,
            order_index: m.order_index,
            step_id: m.step_id,
            run_mode: m.run_mode,
            skip_on_source_failed: m.skip_on_source_failed != 0,
            min_rating: m.min_rating,
            unrated_policy: m.unrated_policy,
            on_success: m.on_success,
            success_goto_step_id: m.success_goto_step_id,
            on_rating_fail: m.on_rating_fail,
            fail_goto_step_id: m.fail_goto_step_id,
            enabled: m.enabled != 0,
            created_at: m.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopExecutionDto {
    pub id: i64,
    pub loop_id: i64,
    pub trigger_id: Option<i64>,
    pub trigger_type: String,
    pub trigger_meta: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub total_steps: i32,
    pub completed_steps: i32,
    pub failed_steps: i32,
}

impl From<loop_executions::Model> for LoopExecutionDto {
    fn from(m: loop_executions::Model) -> Self {
        Self {
            id: m.id,
            loop_id: m.loop_id,
            trigger_id: m.trigger_id,
            trigger_type: m.trigger_type,
            trigger_meta: m.trigger_meta,
            started_at: m.started_at,
            finished_at: m.finished_at,
            status: m.status,
            total_steps: m.total_steps,
            completed_steps: m.completed_steps,
            failed_steps: m.failed_steps,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStepExecutionDto {
    pub id: i64,
    pub loop_execution_id: i64,
    pub step_id: i64,
    pub todo_id: i64,
    pub execution_record_id: Option<i64>,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
    /// 自动评审评分（0-100），来自关联的 execution_record
    pub rating: Option<i32>,
    /// 评分未达阈值时的策略（skip / pass）
    pub unrated_policy: Option<String>,
    /// 评分阈值
    pub min_rating: Option<i32>,
    /// 环节名称，来自 loop_steps 表
    pub step_name: Option<String>,
    /// 全局执行序号（黑板用）
    pub sequence_index: i32,
    /// 本次步执行的结论摘要
    pub conclusion: Option<String>,
    /// 本次环节执行消耗的 token（从 execution_record.usage JSON 解析）
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub cache_creation_input_tokens: Option<i64>,
    pub total_cost_usd: Option<f64>,
}

impl From<loop_step_executions::Model> for LoopStepExecutionDto {
    fn from(m: loop_step_executions::Model) -> Self {
        Self {
            id: m.id,
            loop_execution_id: m.loop_execution_id,
            step_id: m.step_id,
            todo_id: m.todo_id,
            execution_record_id: m.execution_record_id,
            status: m.status,
            started_at: m.started_at,
            finished_at: m.finished_at,
            error_message: m.error_message,
            rating: m.rating,
            unrated_policy: m.unrated_policy,
            min_rating: m.min_rating,
            step_name: None,
            sequence_index: m.sequence_index,
            conclusion: m.conclusion,
            input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            total_cost_usd: None,
        }
    }
}

impl LoopStepExecutionDto {
    pub fn with_review(mut self, rating: Option<i32>, policy: Option<String>, threshold: Option<i32>, name: Option<String>) -> Self {
        self.rating = rating;
        self.unrated_policy = policy;
        self.min_rating = threshold;
        self.step_name = name;
        self
    }
}

/// Loop Execution 附加的 token 汇总统计,
/// 由后端在 get_execution 时从 execution_records.usage JSON 聚合计算。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoopExecutionTokenSummary {
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cache_read_input_tokens: i64,
    pub total_cache_creation_input_tokens: i64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopExecutionDetail {
    #[serde(flatten)]
    pub execution: LoopExecutionDto,
    pub step_executions: Vec<LoopStepExecutionDto>,
    pub loop_name: String,
    /// 本次 loop execution 的 token 汇总统计
    pub token_summary: LoopExecutionTokenSummary,
}

// ====== 请求体（创建/更新）======

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLoopRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    pub review_template_id: Option<i64>,
}

fn default_color() -> String { "#722ed1".to_string() }
fn default_icon() -> String { "loop".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLoopRequest {
    pub name: String,
    pub description: String,
    pub workspace: Option<String>,
    pub color: String,
    pub icon: String,
    pub review_template_id: Option<i64>,
    #[serde(default)]
    pub limits_config: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLoopStatusRequest {
    /// enabled | paused
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTriggerRequest {
    pub trigger_type: String,
    #[serde(default = "default_trigger_config")]
    pub config: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
}

fn default_trigger_config() -> String { "{}".to_string() }
fn default_true() -> bool { true }

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTriggerRequest {
    pub trigger_type: String,
    pub config: String,
    pub enabled: bool,
    pub priority: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLoopStepRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub step_id: i64,
    #[serde(default = "default_run_mode")]
    pub run_mode: String,
    #[serde(default)]
    pub skip_on_source_failed: bool,
    #[serde(default)]
    pub min_rating: Option<i32>,
    #[serde(default = "default_unrated_policy")]
    pub unrated_policy: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_on_success")]
    pub on_success: String,
    #[serde(default)]
    pub success_goto_step_id: Option<i64>,
    #[serde(default = "default_on_rating_fail")]
    pub on_rating_fail: String,
    #[serde(default)]
    pub fail_goto_step_id: Option<i64>,
}

fn default_run_mode() -> String { "sequential".to_string() }
fn default_unrated_policy() -> String { "skip".to_string() }
fn default_on_success() -> String { "next".to_string() }
fn default_on_rating_fail() -> String { "break".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLoopStepRequest {
    pub name: String,
    pub description: String,
    pub step_id: i64,
    pub run_mode: String,
    pub skip_on_source_failed: bool,
    pub min_rating: Option<i32>,
    pub unrated_policy: String,
    pub enabled: bool,
    pub on_success: String,
    pub success_goto_step_id: Option<i64>,
    pub on_rating_fail: String,
    pub fail_goto_step_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReorderLoopStepsRequest {
    /// 新顺序的 step id 列表
    pub ordered_ids: Vec<i64>,
}

/// 触发器类型的辅助校验。
pub fn validate_trigger_type(t: &str) -> Result<(), String> {
    match t {
        "manual" | "cron" | "webhook" | "feishu_message" | "feishu_command"
        | "todo_completed" | "todo_state_changed" | "tag_added" => Ok(()),
        _ => Err(format!("未知的 trigger_type: {}", t)),
    }
}

pub fn validate_loop_status(s: &str) -> Result<(), String> {
    match s {
        "enabled" | "paused" => Ok(()),
        _ => Err(format!("未知的 loop status: {}", s)),
    }
}

/// 把 loop_execution.status 归类为前端展示用的颜色。
pub fn loop_execution_color(status: &str) -> &'static str {
    match status {
        "running" => "#1890ff",
        "success" => "#52c41a",
        "failed" => "#f5222d",
        "partial" => "#fa8c16",
        "cancelled" => "#8c8c8c",
        _ => "#bfbfbf",
    }
}

pub fn loop_status_color(status: &str) -> &'static str {
    match status {
        "enabled" => "#52c41a",
        "paused" => "#fa8c16",
        _ => "#bfbfbf",
    }
}

pub fn step_execution_color(status: &str) -> &'static str {
    match status {
        "pending" => "#bfbfbf",
        "running" => "#1890ff",
        "success" => "#52c41a",
        "failed" => "#f5222d",
        "skipped" => "#fa8c16",
        _ => "#bfbfbf",
    }
}

// 触发器类型 → 图标提示(给前端用,避免把映射表塞到前端)
pub fn trigger_type_icon(t: &str) -> &'static str {
    match t {
        "manual" => "play",
        "cron" => "clock",
        "webhook" => "api",
        "feishu_message" => "message",
        "feishu_command" => "command",
        "todo_completed" => "check",
        "todo_state_changed" => "sync",
        "tag_added" => "tag",
        _ => "trigger",
    }
}

// 让 `TodoStatus` 在本模块可直接使用
#[allow(dead_code)]
fn _ensure_todo_status_in_scope(_: TodoStatus) {}
