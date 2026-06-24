//! Executor Service —— 顶层 orchestrator 模块。
//!
//! 顶层 `run_todo_execution` 只做「pre-spawn 编排 → 失败翻译 → spawn 子任务」三段，
//! 各阶段实际工作下沉到子模块：
//!
//! - [`worktree`] —— Git Worktree 创建/清理/参数注入（issue #643）
//! - [`log_capture`] —— stdout/stderr reader + LogFlusher pipeline + stats 提取
//! - [`pre_spawn`] —— pre-spawn 失败短路、并发上限、executor 选择
//! - [`completion`] —— 终态分支（正常/取消/超时）、自动评审、completion hook、emit event
//! - [`stages`] —— 三阶段 stage 函数 + spawn 闭包 + dispatch 收口
//! - [`auto_review`] —— 同步自动评审（基于独立 runtime + std::thread 跑评审实例）
//! - [`types`] —— 跨模块共享的 stage 产物聚合类型
//!
//! 各子模块可独立单测；本文件只在「公共 API + 编排骨架」级别保留代码。

pub(crate) mod auto_review;
pub(crate) mod completion;
pub(crate) mod log_capture;
pub(crate) mod pre_spawn;
pub(crate) mod spawn_lifecycle;
pub(crate) mod stages;
pub(crate) mod types;
pub(crate) mod worktree;

use std::sync::Arc;
use tokio::sync::broadcast;

use crate::adapters::ExecutorRegistry;
use crate::db::Database;
use crate::handlers::ExecEvent;
use crate::task_manager::TaskManager;

/// 执行结束返回给调用方的最小契约。
///
/// `record_id == None` 表示这次执行未成功创建 `execution_records` 行
/// （例如并发上限拒接、executor 不可用）；调用方可以据此判定是否需要进一步排查。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutionResult {
    pub task_id: String,
    pub record_id: Option<i64>,
}

/// `run_todo_execution` 的入参聚合体。
///
/// 把 14+ 字段打包成一个 struct 而不是平铺签名，避免 Long Parameter List 坏味道；
/// 新增字段时只动 1 处而不是 5 个 stage 函数签名。
pub struct RunTodoExecutionRequest {
    pub db: Arc<Database>,
    pub executor_registry: Arc<ExecutorRegistry>,
    pub tx: broadcast::Sender<ExecEvent>,
    pub task_manager: Arc<TaskManager>,
    pub config: Arc<std::sync::RwLock<crate::config::Config>>,
    pub todo_id: i64,
    pub message: String,
    pub req_executor: Option<String>,
    pub trigger_type: String,
    pub params: Option<std::collections::HashMap<String, String>>,
    pub resume_session_id: Option<String>,
    pub resume_message: Option<String>,
    /// 触发这次执行的源 todo id（如 loop 评审任务、cron 任务）。
    /// 持久化到 `execution_records.source_todo_id`，供前端 step 面板
    /// 「这条执行由谁触发」展示用。
    pub source_todo_id: Option<i64>,
    /// 触发源的展示标题（loop 写 step 标题，auto_review 写原 todo 标题）。
    /// 持久化到 `execution_records.source_todo_title`。
    pub source_todo_title: Option<String>,
    /// Feishu bot to send result directly to binding chat.
    pub feishu_bot_id: Option<i64>,
    /// Feishu receive_id (open_id for p2p, chat_id for group).
    pub feishu_receive_id: Option<String>,
    /// 当本次执行是 loop 环节的一部分时，指向 loop_step_executions 表的 id。
    pub loop_step_execution_id: Option<i64>,
    /// 环节 id（steps 表），环节独立执行时设置
    pub step_id: Option<i64>,
    /// 显式工作空间路径（用于 loop 场景：loop 有自己的 workspace，
    /// 但 executor service 内部通过 todo 加载获取 workspace。当 todo 不存在
    /// 或 todo_id=0 时，使用此字段作为 workspace 用于 worktree 创建和 cwd 回退）。
    pub workspace: Option<String>,
}

/// Run a todo execution. Priority: explicit executor > todo stored executor > default.
///
/// 整条执行路径放进一个 `todo_execution` span，附 todo_id / trigger_type / req_executor
/// 三个字段：issue #513 的诉求是「执行器调用追踪」，而 spawn 子任务、stdout/stderr
/// 读取、log flush、database update、hook fire 这一长串环节现在会被一个统一的 span 包住，
/// 配合 request_id 中间件，上游 HTTP 入口的 trace_id 可以贯穿到执行末段，便于定位
/// 「某个 todo 整体耗时多少、哪一段最慢」。
#[tracing::instrument(
    name = "todo_execution",
    level = "info",
    skip_all,
    fields(
        todo_id = request.todo_id,
        trigger_type = %request.trigger_type,
        req_executor = %request.req_executor.as_deref().unwrap_or(""),
    )
)]
pub async fn run_todo_execution(request: RunTodoExecutionRequest) -> ExecutionResult {
    // 三阶段 stage 调用：每个阶段返回 Result<T, ExecutionResult>；
    // 任一阶段失败立即 short-circuit 返回 ExecutionResult 给上游。
    let prepared = match stages::prepare_execution_state(request).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let spawned = match stages::start_todo_and_prepare_spawn(prepared).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    stages::dispatch_spawned_executor_task(spawned).await
}

/// Run a todo execution with parameter substitution.
/// Replaces placeholders `{{key}}` in the message with corresponding values from params before execution.
pub async fn run_todo_execution_with_params(
    mut request: RunTodoExecutionRequest,
) -> ExecutionResult {
    // 顶层做一遍占位符替换，避免改动 stage 1 内部逻辑；
    // params 被 take 走，确保 stage 1 不会再用旧的 HashMap。
    if let Some(params) = request.params.take() {
        request.message = crate::models::replace_placeholders(&request.message, &params);
    }
    run_todo_execution(request).await
}