//! 跨模块共享的 stage 产物聚合类型。
//!
//! 模块职责：定义 stage 1/2/3 之间传递的"数据载体"，让各 stage 函数签名只接
//! 一个 struct 而不是 14+ 个参数。新增字段时只动 1 处而不是 N 个函数签名。
//!
//! 所有类型仅在本 crate 内可见；外部 API 仍由 [`super::ExecutionResult`] 与
//! [`super::RunTodoExecutionRequest`] 负责。

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::adapters::{CodeExecutor, ExecutorRegistry};
use crate::db::Database;
use crate::executor_service::ExecEvent;

use super::worktree::WorktreeContext;
use super::RunTodoExecutionRequest;

/// Stage 1 产物：完成 executor 选择 + record 创建，并持有 task_guard / cancel_rx。
///
/// 这一阶段不动 todo 状态、不创建 worktree，所以 fail-fast 路径无需清理副作用。
///
/// 设计取舍：把 `RunTodoExecutionRequest` 整段嵌入 `request` 字段而不是平铺。
/// 平铺需要 14 个字段二次声明；嵌入只需 1 个字段，添加 request 字段时只动 1 处。
pub(crate) struct PreparedExecution {
    /// 入参 request。Stage 2 / Stage 3 仍会读到 todo_id / trigger_type 等。
    pub request: RunTodoExecutionRequest,
    /// RAII guard for task registry；必须 move 进 spawn 子任务，否则 drop 时会误删 sender。
    // 该字段靠 move/drop 起作用（持有期间保持注册项存活），自身从不被按名读取，
    // 故 allow(dead_code)——这是 RAII 模式的正常形态，删除会破坏存活期语义。
    #[allow(dead_code)]
    pub task_guard: crate::task_manager::TaskGuard,
    /// 与 task_manager 的 cancel channel；spawn 子任务在 select! 中 recv 它。
    pub cancel_rx: tokio::sync::mpsc::Receiver<()>,
    pub task_id: String,
    /// 已做 placeholder 替换的 command argv，spawn 阶段原样转发给 executor。
    pub command_args: Vec<String>,
    pub executable_path: String,
    /// 选定的 executor Arc，spawn 阶段用作 `executor_spawn`。
    pub executor: Arc<dyn CodeExecutor>,
    pub executor_str: String,
    pub record_id: i64,
    /// todo 在并发控制 / pre-hook / executor 选择中都用到，必须保留；load_todo 失败时为 None。
    pub todo: Option<crate::models::Todo>,
    /// 仅 spawn 阶段用于 effective_workspace_path 回退。
    pub todo_workspace_path: Option<String>,
    pub timeout_secs: u64,
}

/// Stage 2 产物：worktree 已创建 + todo 已启动 + TaskInfo 已注册，
/// 准备 move 进 spawn 子任务的全部数据。
///
/// 嵌入 `prepared` 而不是平铺 stage 1 的 14 个字段；新加 stage 1 字段时只动 1 处。
pub(crate) struct SpawnInputs {
    pub prepared: PreparedExecution,
    pub todo_title: String,
    pub executor_spawn: Arc<dyn CodeExecutor>,
    /// spawn 阶段实际使用的 cwd：worktree 路径优先，回退到 todo.workspace_path。
    pub effective_workspace_path: Option<String>,
    pub execution_timeout_secs: u64,
    pub worktree_ctx: WorktreeContext,
}

/// `run_spawned_executor_task` 的执行期状态：把 SpawnInputs 字段全部 clone
/// 出来成可借用结构，避免在 spawn 闭包内对原 owned 值反复 .clone()。
///
/// `cancel_rx` / `task_guard` 不在此结构下沉：仍由 `prepared: PreparedExecution` 持有，
/// 通过 `runtime.prepared.cancel_rx` / `runtime.prepared.task_guard` 访问。
pub(crate) struct SpawnRuntime {
    pub db: Arc<Database>,
    pub tx: broadcast::Sender<ExecEvent>,
    pub task_manager: Arc<crate::task_manager::TaskManager>,
    pub todo_id: i64,
    pub todo_title: String,
    pub executor_spawn: Arc<dyn CodeExecutor>,
    pub record_id: i64,
    pub worktree_ctx: WorktreeContext,
    pub task_id: String,
    pub execution_timeout_secs: u64,
    pub feishu_bot_id: Option<i64>,
    pub feishu_receive_id: Option<String>,
    /// spawn 阶段实际使用的 cwd：worktree 路径优先，回退到 todo.workspace_path。
    /// 修复 issue #660 重构中的回归：原代码在 spawn 闭包内用 effective_workspace_path
    /// 决定子进程 cwd，但拆分到 spawn_executor_child 后误用了 todo_workspace_path，
    /// 导致启用 worktree 时子进程仍在原始 workspace_path 内运行。
    pub effective_workspace_path: Option<String>,
    pub prepared: PreparedExecution,
}

/// `handle_completed_branch` 的入参聚合。
///
/// 之前 23 个位置参数 + `#[allow(clippy::too_many_arguments)]` 是 Long Parameter
/// List 坏味道的复发。改成结构体传参后调用方写 SpawnContext { ... } 字面量 22
/// 行，但 handle_completed_branch 函数体能缩到 < 30 行真正符合 CLAUDE.md。
pub(crate) struct SpawnContext {
    pub db: Arc<Database>,
    pub tx: broadcast::Sender<ExecEvent>,
    pub task_manager: Arc<crate::task_manager::TaskManager>,
    pub executor_registry: Arc<ExecutorRegistry>,
    pub config: Arc<std::sync::RwLock<crate::config::Config>>,
    pub executor: Arc<dyn CodeExecutor>,
    pub task_id: String,
    pub todo_id: i64,
    pub todo_title: String,
    pub record_id: i64,
    pub execution_start: std::time::Instant,
    pub worktree_ctx: WorktreeContext,
    pub trigger_type: String,
    pub feishu_bot_id: Option<i64>,
    pub feishu_receive_id: Option<String>,
    /// 工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
    pub workspace_id: Option<i64>,
}

/// select! 三种终态枚举，避免在三个分支里各重复「杀进程 + drain + finalize」
/// 清理模板。child 仍由调用方持有，可继续调 kill_process_tree。
pub(crate) enum RunOutcome {
    Cancelled,
    TimedOut,
    Completed(std::io::Result<std::process::ExitStatus>),
}

/// Stage 1 步骤 1：在 `request` 上做 message 占位符替换，并返回替换后的 message。
pub(crate) struct SubstitutedContext {
    pub message: String,
}

/// Stage 1 步骤 2：注册 task 并加载 todo。返回 task_id + guard + cancel_rx + todo。
///
/// Issue #506：用 RAII guard 注册 task，确保即便后续路径 panic/早返回忘了
/// remove，sender 也会被 guard drop 时清理。
pub(crate) struct TaskState {
    pub task_id: String,
    pub task_guard: crate::task_manager::TaskGuard,
    pub cancel_rx: tokio::sync::mpsc::Receiver<()>,
    pub todo: Option<crate::models::Todo>,
}