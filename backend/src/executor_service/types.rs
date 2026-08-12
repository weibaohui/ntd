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
    pub feishu_receive_id_type: Option<String>,
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
    pub feishu_receive_id_type: Option<String>,
    /// 工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
    pub workspace_id: Option<i64>,
    /// 专家索引（需求 092 P2）：discussion_auto 接力回写时需要据此解析管家结论里的 @。
    /// None 表示该执行路径无专家索引（与 RunTodoExecutionRequest.expert_manager 同源）。
    pub expert_manager: Option<Arc<crate::expert::ExpertIndexManager>>,
}

impl SpawnContext {
    /// 提取执行链路共享依赖五元组（096-W2-PR3）——
    /// 五元组是 SpawnContext 字段的真子集，逐字段 Arc::clone 廉价（原子计数自增）。
    pub(crate) fn execution_deps(&self) -> ExecutionDeps {
        ExecutionDeps {
            db: Arc::clone(&self.db),
            executor_registry: Arc::clone(&self.executor_registry),
            tx: self.tx.clone(),
            task_manager: Arc::clone(&self.task_manager),
            config: Arc::clone(&self.config),
        }
    }
}

/// select! 三种终态枚举，避免在三个分支里各重复「杀进程 + drain + finalize」
/// 清理模板。child 仍由调用方持有，可继续调 kill_process_tree。
pub(crate) enum RunOutcome {
    Cancelled,
    TimedOut,
    Completed(std::io::Result<std::process::ExitStatus>),
}

/// 093-B3：执行终态结果三元组。
/// success / exit_code / result_str 在 finalize_normal_completion、
/// emit_completion_events 等函数间总是结伴出现（Data Clump 坏味道），
/// 聚合成一个对象后 19 参/15 参签名才能塌缩。
pub(crate) struct CompletionOutcome {
    /// 终态成败：来自 check_success（exit_code + step_finish 事件回授的综合判定），
    /// 决定 record 状态写 success 还是 failed
    pub success: bool,
    /// 原始退出码：与 success 分开保留——非零退出码也可能判成功（NTD-012 语义），
    /// 事件载荷与日志需要原值而非折算后的布尔
    pub exit_code: i32,
    /// 最终输出文本：finalize 阶段从日志链提取，emit_completion_events 直接消费；
    /// String 而非 &str：outcome 跨 await 点存活，借用会拉长调用栈生命周期
    pub result_str: String,
}

/// 093-B3：select! 终态后 kill + drain 所需的进程句柄簇。
/// 这 5 个句柄在三个终态分支间原样传递（child 引用 + 4 个 owned handle），
/// 聚合成一个对象后 run_cancellation_path / run_timeout_path 才能从 17 参塌缩到 2 参。
/// 生命周期注解：child 是可变借用（kill_process_tree 需要 &mut），其余 handle 随结构 move。
pub(crate) struct ProcessTeardown<'a> {
    /// 子进程组句柄：终态分支要调 kill_process_tree，必须是可变借用；
    /// 借用而非 move：kill 后调用方可能还需读 exit 状态
    pub child: &'a mut command_group::AsyncGroupChild,
    /// stdout 读取任务：终态时需 drain（await 其结束）防日志截断；
    /// Option 包容「stdout 未开启」的执行器形态
    pub stdout_task: Option<tokio::task::JoinHandle<()>>,
    /// stderr 读取任务：同 stdout 的 drain 语义
    pub stderr_task: Option<tokio::task::JoinHandle<()>>,
    /// 日志冲刷器：drain 后调 finalize 把残余 buffer 一次性入库
    /// （issue #653：之后不得再重复插全量日志）
    pub log_flusher: Arc<crate::log_flusher::LogFlusher>,
    /// 周期冲刷定时器：终态必须 abort，否则持有 flusher 引用泄漏到进程结束后
    pub flush_timer: tokio::task::JoinHandle<()>,
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

/// 执行链路共享依赖五元组（096-W2-PR3：Introduce Parameter Object）。
///
/// `(db, executor_registry, tx, task_manager, config)` 这组依赖在 completion 黑板
/// 三函数、`maybe_run_auto_review`、auto_review 三函数共 7 个函数签名中原样排列出现
/// （Data Clump 坏味道，各挂 `#[allow(too_many_arguments)]` 苟活）。聚合成对象后：
/// - 各函数签名塌缩为 `deps: ExecutionDeps`（owned 现场）或 `&ExecutionDeps`（借用现场）；
/// - 新增共享依赖只动本结构，不再波及 7 处签名与全部调用点。
///
/// 全字段 Arc 包装，clone 廉价；`Clone` derive 供 tokio::spawn move 闭包整体搬入。
pub(crate) struct ExecutionDeps {
    pub db: Arc<Database>,
    pub executor_registry: Arc<ExecutorRegistry>,
    pub tx: broadcast::Sender<ExecEvent>,
    pub task_manager: Arc<crate::task_manager::TaskManager>,
    pub config: Arc<std::sync::RwLock<crate::config::Config>>,
}

impl Clone for ExecutionDeps {
    fn clone(&self) -> Self {
        // 逐字段 Arc::clone：只增引用计数，不复制底层数据
        Self {
            db: Arc::clone(&self.db),
            executor_registry: Arc::clone(&self.executor_registry),
            tx: self.tx.clone(),
            task_manager: Arc::clone(&self.task_manager),
            config: Arc::clone(&self.config),
        }
    }
}