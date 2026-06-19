//! Loop Studio 数据模型（API DTO）。
//!
//! 与 `db::entity::loops` 等实体不同：
//! - 实体是 SeaORM 自动派生的,直接对应数据库行
//! - 这里定义的是面向 API 层的 DTO,经过 snake_case / camelCase 转换、字段精简、
//!   嵌套结构组装,直接给前端消费
use serde::{Deserialize, Serialize};

use crate::db::entity::{
    loop_executions, loop_stage_executions, loop_stages, loop_triggers, loops,
};
use crate::db::loop_::{LoopFullView, LoopListRow};
use crate::models::TodoStatus;

/// Loop 列表行(左栏一行)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopListItem {
    #[serde(flatten)]
    pub loop_: LoopDto,
    pub trigger_count: i32,
    pub stage_count: i32,
    pub last_execution_status: String,
    pub last_execution_at: Option<String>,
}

impl From<LoopListRow> for LoopListItem {
    fn from(row: LoopListRow) -> Self {
        Self {
            loop_: row.loop_.into(),
            trigger_count: row.trigger_count,
            stage_count: row.stage_count,
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
    pub stages: Vec<LoopStageDto>,
    /// todo_id -> TodoDto,前端展示 stage 关联的 todo 信息时直接 lookup
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
        let stages = view
            .stages_meta
            .into_iter()
            .map(|(s, todo_title, todo_executor, todo_status): (loop_stages::Model, String, String, String)| LoopStageDto {
                stage: s.into(),
                todo_title,
                todo_executor,
                todo_status,
            })
            .collect();
        Self {
            loop_: view.loop_.into(),
            triggers: view.triggers.into_iter().map(Into::into).collect(),
            stages,
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
pub struct LoopStageDto {
    #[serde(flatten)]
    pub stage: LoopStageRawDto,
    /// 冗余字段,JOIN 时一并查出来,避免前端再请求 todo 详情
    pub todo_title: String,
    pub todo_executor: String,
    pub todo_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStageRawDto {
    pub id: i64,
    pub loop_id: i64,
    pub name: String,
    pub description: String,
    pub order_index: i32,
    pub todo_id: i64,
    pub run_mode: String,
    pub skip_on_source_failed: bool,
    pub min_rating: Option<i32>,
    pub unrated_policy: String,
    pub enabled: bool,
    pub created_at: Option<String>,
}

impl From<loop_stages::Model> for LoopStageRawDto {
    fn from(m: loop_stages::Model) -> Self {
        Self {
            id: m.id,
            loop_id: m.loop_id,
            name: m.name,
            description: m.description,
            order_index: m.order_index,
            todo_id: m.todo_id,
            run_mode: m.run_mode,
            skip_on_source_failed: m.skip_on_source_failed != 0,
            min_rating: m.min_rating,
            unrated_policy: m.unrated_policy,
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
    pub total_stages: i32,
    pub completed_stages: i32,
    pub failed_stages: i32,
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
            total_stages: m.total_stages,
            completed_stages: m.completed_stages,
            failed_stages: m.failed_stages,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStageExecutionDto {
    pub id: i64,
    pub loop_execution_id: i64,
    pub stage_id: i64,
    pub todo_id: i64,
    pub execution_record_id: Option<i64>,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
}

impl From<loop_stage_executions::Model> for LoopStageExecutionDto {
    fn from(m: loop_stage_executions::Model) -> Self {
        Self {
            id: m.id,
            loop_execution_id: m.loop_execution_id,
            stage_id: m.stage_id,
            todo_id: m.todo_id,
            execution_record_id: m.execution_record_id,
            status: m.status,
            started_at: m.started_at,
            finished_at: m.finished_at,
            error_message: m.error_message,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopExecutionDetail {
    #[serde(flatten)]
    pub execution: LoopExecutionDto,
    pub stage_executions: Vec<LoopStageExecutionDto>,
    pub loop_name: String,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLoopStatusRequest {
    /// draft | enabled | paused
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
pub struct CreateStageRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub todo_id: i64,
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
}

fn default_run_mode() -> String { "sequential".to_string() }
fn default_unrated_policy() -> String { "skip".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateStageRequest {
    pub name: String,
    pub description: String,
    pub todo_id: i64,
    pub run_mode: String,
    pub skip_on_source_failed: bool,
    pub min_rating: Option<i32>,
    pub unrated_policy: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReorderStagesRequest {
    /// 新顺序的 stage id 列表
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
        "draft" | "enabled" | "paused" => Ok(()),
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
        "draft" => "#8c8c8c",
        _ => "#bfbfbf",
    }
}

pub fn stage_execution_color(status: &str) -> &'static str {
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
