use std::sync::Arc;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::Instrument;
use uuid::Uuid;

use command_group::AsyncCommandGroup;

use crate::adapters::{parse_executor_type, CodeExecutor, ExecutorRegistry};
use crate::db::{Database, NewExecutionRecord};
use crate::handlers::ExecEvent;
use crate::hooks::HookService;
use crate::log_flusher::LogFlusher;
use crate::models::{ExecutorType, ExecutionUsage, ParsedLogEntry, Todo};
use crate::services::worktree::WorktreeService;
use crate::task_manager::TaskManager;

fn send_event(tx: &broadcast::Sender<ExecEvent>, event: ExecEvent) {
    let _ = tx.send(event);
}

// ============================================================================
//  Git Worktree 集成 (issue #643)
//
//  这里只放 3 个细粒度辅助函数，遵循 issue #635/640 倡导的"run_todo_execution 不再
//  持有逻辑，只负责编排"原则。每个函数 ≤ 20 行，职责单一。
// ============================================================================

/// issue #643: 单次执行使用的 worktree 上下文。
///
/// - `effective_workspace`: 子进程的 cwd。None=继续用 todo.workspace；
///   Some(p)=worktree 目录被 ntd 接管，子进程在 worktree 内运行。
/// - `record_path`: 回写到 execution_records.worktree_path 的值（None=无需记录）。
/// - `auto_cleanup`: 终态时是否需要调用 WorktreeService::cleanup_worktree。
#[derive(Debug, Clone, Default)]
struct WorktreeContext {
    effective_workspace: Option<String>,
    record_path: Option<String>,
    auto_cleanup: bool,
}

/// 根据 todo.workspace 找到对应的 project_directory，决定是否开 worktree。
///
/// 不在 `WorktreeContext` 内持有数据库句柄——这是个**纯异步查询**函数，方便在
/// run_todo_execution 主路径上独立调用并把结果 move 进 spawn 闭包。
async fn resolve_worktree_context(
    db: &Database,
    todo: &Option<Todo>,
) -> WorktreeContext {
    // 没有 todo（被 hook 删除）/ 没有 workspace 关联项目目录——不启用 worktree
    let Some(t) = todo.as_ref() else {
        return WorktreeContext::default();
    };
    let Some(ws) = t.workspace.as_deref() else {
        return WorktreeContext::default();
    };
    // 目录在 project_directories 表里没登记——同样不启用（避免给任意 workspace 路径做 worktree）
    let Ok(Some(dir)) = db.get_project_directory_by_path(ws).await else {
        return WorktreeContext::default();
    };
    if !dir.git_worktree_enabled {
        return WorktreeContext::default();
    }

    // 走到这里说明用户在该目录下开启了 worktree 自动管理。
    // 创建失败时不阻塞执行——回退到原始 workspace，子进程仍然能跑通。
    let svc = WorktreeService::new();
    match svc.create_worktree(ws, t.id) {
        Ok(wt_path) => WorktreeContext {
            effective_workspace: Some(wt_path.clone()),
            record_path: Some(wt_path),
            auto_cleanup: dir.auto_cleanup,
        },
        Err(e) => {
            tracing::warn!(
                workspace = %ws,
                todo_id = t.id,
                error = %e,
                "failed to create git worktree, falling back to original workspace"
            );
            WorktreeContext::default()
        }
    }
}

/// 把 worktree_path 持久化到 execution_records。
///
/// 这一步不在 `resolve_worktree_context` 内做，因为该函数不持有 record_id；
/// 调用方在拿到 `create_execution_record` 返回的 id 之后再回填。
async fn record_worktree_path(db: &Database, record_id: i64, path: Option<&str>) {
    if let Some(p) = path {
        if let Err(e) = db.update_execution_record_worktree_path(record_id, p).await {
            tracing::warn!(record_id, error = ?e, "failed to persist worktree_path");
        }
    }
}

/// 执行结束后清理 worktree（如果启用了 auto_cleanup）。
///
/// `WorktreeError` 不会出现：本服务把失败映射成 warn，不再向上抛。
fn cleanup_worktree_if_needed(ctx: &WorktreeContext) {
    if !ctx.auto_cleanup {
        return;
    }
    let Some(path) = ctx.record_path.as_deref() else {
        return;
    };
    let svc = WorktreeService::new();
    if let Err(e) = svc.cleanup_worktree(path) {
        tracing::warn!(worktree = %path, error = %e, "worktree cleanup failed");
    }
}

/// 使用 command-group 安全地杀死进程树
/// command-group 会自动创建进程组，kill() 时会杀死整个进程组
async fn kill_process_tree(child: &mut command_group::AsyncGroupChild) {
    if let Err(e) = child.kill().await {
        tracing::warn!("Failed to kill process group: {}", e);
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutionResult {
    pub task_id: String,
    pub record_id: Option<i64>,
}

pub struct RunTodoExecutionRequest {
    pub db: Arc<Database>,
    pub executor_registry: Arc<ExecutorRegistry>,
    pub tx: broadcast::Sender<ExecEvent>,
    pub task_manager: Arc<TaskManager>,
    pub config: Arc<std::sync::RwLock<crate::config::Config>>,
    /// 共享的 hook 触发器（来自 AppState 单例）。
    ///
    /// 之所以放在 request 里而不是在 `run_todo_execution` 内重新 `Arc::new(HookService::new(...))`，
    /// 是因为：
    /// 1. `HookService` 本身持有 `ServiceContext`（5 个 Arc + tokio::RwLock），每次执行末
    ///    段 fire 钩子时重新 clone 5 个 Arc 是无意义的开销。
    /// 2. AppState 里已经维护了一个长生命周期的 `Arc<HookService>`（handlers/mod.rs 中创建），
    ///    handler 路径（handlers/todo.rs::update_todo 状态变更）已经在复用它，executor
    ///    路径保持一致能避免出现两套 `HookService` 实例可能造成的不一致。
    /// 3. `HookService::fire_for_todo` 内部会 `tokio::spawn` 子任务发新执行记录，
    ///    复用同一个 service 能让所有 hook 触发的执行共享同一份内存状态。
    pub hook_service: Arc<HookService>,
    pub todo_id: i64,
    pub message: String,
    pub req_executor: Option<String>,
    pub trigger_type: String,
    pub params: Option<std::collections::HashMap<String, String>>,
    pub resume_session_id: Option<String>,
    pub resume_message: Option<String>,
    /// Todo ids already visited on the dispatch path, used to break cycles
    /// when a hook triggers a todo that would re-fire the source.
    pub chain: Vec<i64>,
    /// Hook trigger provenance. `None` for manual/cron/webhook/feishu
    /// triggers; populated by `execute_target_todo` for hook firings.
    pub source_todo_id: Option<i64>,
    pub source_todo_title: Option<String>,
    pub source_hook_id: Option<i64>,
    /// Feishu bot to send result directly to binding chat.
    pub feishu_bot_id: Option<i64>,
    /// Feishu receive_id (open_id for p2p, chat_id for group).
    pub feishu_receive_id: Option<String>,
}

// ============================================================================
//  `run_todo_execution` 拆出的子函数（issue #606）
//
//  设计意图：
//  - 把 881 行巨型函数按阶段拆为职责单一的 helper，便于阅读与单测。
//  - pre-spawn 阶段的纯函数（`resolve_executor_type` / `apply_worktree_flag` /
//    `build_completion_record_update`）不依赖任何运行时状态，可直接 unit-test。
//  - 涉及 IO / DB / 异步 spawn 的 helper（`count_active_running_for_todo` /
//    `spawn_stdout_reader` / `spawn_stderr_reader`）通过明确的输入输出
//    与顶级函数解耦，行为契约靠 doc comment 钉死。
//  - 顶层 `run_todo_execution` 退化为"拼阶段 + 翻译 ExecutionResult"的骨架。
// ============================================================================

/// 选择执行器类型。优先级：调用方显式 `req_executor` > todo 存储的 `todo_executor` > 默认值。
///
/// - 输入字符串无法解析为已知 executor 时记 warn 并退到下一优先级；
/// - 所有输入都解析失败时返回 `ExecutorType::default()`。
///
/// 不返回 `Result`，因为解析失败是预期内的"软"行为，调用方直接走默认分支即可。
/// 把 warn 日志集中在这里，避免顶级函数里出现散落的 `tracing::warn!` 解释为什么降级。
fn resolve_executor_type(req_executor: Option<&str>, todo_executor: Option<&str>) -> ExecutorType {
    // 显式请求 > 存储值 > 默认。三层 or_else 形成优先级链。
    req_executor
        .and_then(|exec| {
            parse_executor_type(exec).or_else(|| {
                tracing::warn!("Unknown explicit executor '{}', trying todo executor", exec);
                None
            })
        })
        .or_else(|| {
            todo_executor.and_then(|exec| {
                parse_executor_type(exec).or_else(|| {
                    tracing::warn!("Unknown todo executor '{}', falling back to default", exec);
                    None
                })
            })
        })
        .unwrap_or_default()
}

/// 给 `command_args` 插入 `--worktree` 开关。
///
/// - 仅当 `worktree_enabled == true` 且 executor 是 `Claudecode` 或 `Hermes` 时生效；
///   其他 executor 即使 todo 开启了 worktree，也会被静默忽略。
/// - 位置约束：必须放在 `--session-id` / `--resume` 之前，否则 Claude Code / Hermes
///   在 resume session 时不会触发 worktree 初始化。
///   没找到这些开关时 append 到末尾，依然能让 Claude Code 自动管理 worktree。
fn apply_worktree_flag(command_args: &mut Vec<String>, exec_type: ExecutorType, worktree_enabled: bool) {
    if !worktree_enabled {
        return;
    }
    match exec_type {
        ExecutorType::Claudecode | ExecutorType::Hermes => {
            // 找 `--session-id` 或 `--resume` 的位置；找不到就 append 到末尾。
            let insert_pos = command_args
                .iter()
                .position(|s| s == "--session-id" || s == "--resume")
                .unwrap_or(command_args.len());
            command_args.insert(insert_pos, "--worktree".to_string());
        }
        // 其他 executor 不支持 worktree flag，todo 配置的 `worktree_enabled`
        // 对它们而言无意义；显式忽略避免误把 flag 透传给不识别的二进制。
        _ => {}
    }
}

/// 统计 `todo_id` 下"真正在跑"的执行记录数，自动过滤掉僵尸记录。
///
/// 僵尸 = 数据库标记 running，但 `task_manager` 里查不到对应 task（多半是上一次
/// daemon 重启 / 异常退出遗留的脏数据）。这种记录不算"占用并发配额"，否则 daemon
/// 重启后所有 todo 都会被并发上限挡死。
///
/// 返回 `Result<usize, ()>`：
/// - `Ok(n)` —— 真正的活跃数；调用方据此判断是否已超 `max_concurrent`。
/// - `Err(())` —— DB 查询失败（已记 error 日志）；调用方应直接 abort 当次执行，
///   而不是猜测并发是否够用。
async fn count_active_running_for_todo(
    task_manager: &TaskManager,
    db: &Database,
    todo_id: i64,
) -> Result<usize, ()> {
    // 一次性拿到 task_manager 的活跃 task 列表，后面用它来过滤 DB 的 running 记录。
    let running_tasks = task_manager.get_all_task_infos().await;
    let running_records = db
        .get_running_records_by_todo_id(todo_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get running execution records: {}", e);
        })?;
    Ok(running_records
        .iter()
        .filter(|r| {
            // 没有 task_id 的记录也算僵尸（早期 schema 不写这个字段），
            // 一律不计入并发占用。
            r.task_id
                .as_ref()
                .map(|task_id| running_tasks.iter().any(|t| t.task_id == *task_id))
                .unwrap_or(false)
        })
        .count())
}

/// 启动一个 stderr reader 任务：逐行读 stderr -> 经 executor 解析 -> 推入 LogFlusher。
///
/// 返回 `None` 表示 executor 子进程根本没暴露 stderr（少见，比如某些 mock executor）。
fn spawn_stderr_reader<R>(
    stderr_handle: Option<R>,
    executor: Arc<dyn CodeExecutor>,
    log_flusher: Arc<LogFlusher>,
    tx: broadcast::Sender<ExecEvent>,
    task_id: String,
) -> Option<JoinHandle<()>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    stderr_handle.map(|stderr_reader| {
        tokio::spawn(async move {
            // BufReader::lines 在读到 EOF 时返回 Ok(None)，循环自然退出。
            let mut reader = BufReader::new(stderr_reader).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                // 优先让 executor 自定义解析；解析不到就当 raw stderr 行（log_type="stderr"）。
                let entry = executor
                    .parse_stderr_line(&line)
                    .unwrap_or_else(|| ParsedLogEntry::stderr(line.clone()));
                log_flusher.push(entry.clone()).await;
                send_event(
                    &tx,
                    ExecEvent::Output {
                        task_id: task_id.clone(),
                        entry,
                    },
                );
            }
        })
    })
}

/// 启动一个 stdout reader 任务：逐行读 stdout -> 提取 session_id / todo_progress / 统计 ->
/// 推入 LogFlusher + emit Output event。
///
/// 返回 `None` 表示 executor 子进程没暴露 stdout。
///
/// 与 stderr reader 不同，stdout 还要承担三件事：
///   1. 第一次出现 `executor.extract_session_id` 时把 session_id 回写到 execution_records；
///   2. 解析 `todo_progress` 时除写库外还要发 `TodoProgress` 事件（前端实时进度条）；
///   3. 每 10 行 或 工具调用时扫一遍 buffer 计算 stats，emit `ExecutionStats`。
/// 这三件事没法再下沉到 LogFlusher（一个是 DB 写，一个是 progress 事件），所以留在这里。
fn spawn_stdout_reader<R>(
    stdout_handle: Option<R>,
    executor: Arc<dyn CodeExecutor>,
    db: Arc<Database>,
    log_flusher: Arc<LogFlusher>,
    tx: broadcast::Sender<ExecEvent>,
    task_id: String,
    record_id: i64,
) -> Option<JoinHandle<()>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let stdout_reader = stdout_handle?;
    let tx_clone = tx.clone();
    let executor_clone = executor.clone();
    let db_for_todo = db.clone();
    let log_flusher_for_stdout = log_flusher.clone();
    let tid = task_id;
    let rid = record_id;

    Some(tokio::spawn(async move {
        let mut reader = BufReader::new(stdout_reader).lines();
        let mut log_count = 0u64;
        // session_id 只更新一次：避免每次重复出现 session_id 时反复触发 DB UPDATE。
        let mut session_id_updated = false;
        while let Ok(Some(line)) = reader.next_line().await {
            // 第一次出现 session_id 时回写 DB。后续再出现就不再更新——
            // session_id 在同一次执行里是稳定的，写多次意义不大。
            if !session_id_updated {
                if let Some(sid) = executor_clone.extract_session_id(&line) {
                    let _ = db_for_todo
                        .update_execution_record_session_id(rid, &sid)
                        .await;
                    session_id_updated = true;
                }
            }
            // executor 解析失败（不是 JSONL 格式的行）就跳过；不强制 stderr 兜底。
            let Some(parsed) = executor_clone.parse_output_line(&line) else {
                continue;
            };
            // todo_progress：写库 + 发事件，让前端能实时显示进度。
            if let Some(progress) = crate::todo_progress::try_extract_todo_progress(&parsed) {
                if let Ok(progress_json) = serde_json::to_string(&progress) {
                    let _ = db_for_todo
                        .update_execution_record_todo_progress(rid, &progress_json)
                        .await;
                }
                send_event(
                    &tx_clone,
                    ExecEvent::TodoProgress {
                        task_id: tid.clone(),
                        progress,
                    },
                );
            }

            // 统计：工具调用必发；普通日志每 10 条发一次。
            // 用 with_logs 持锁只读扫描，避免与 push 路径冲突。
            let is_tool_call = parsed.log_type == "tool_use"
                || parsed.log_type == "tool_call"
                || parsed.log_type == "tool";
            log_count += 1;
            if is_tool_call || log_count.is_multiple_of(10) {
                let stats = log_flusher_for_stdout
                    .with_logs(|current_logs| {
                        let tool_calls = current_logs
                            .iter()
                            .filter(|l| {
                                l.log_type == "tool_use"
                                    || l.log_type == "tool_call"
                                    || l.log_type == "tool"
                            })
                            .count() as u64;
                        let conversation_turns = current_logs
                            .iter()
                            .filter(|l| {
                                l.log_type == "assistant"
                                    || l.log_type == "result"
                                    || l.log_type == "text"
                            })
                            .count() as u64;
                        let thinking_count = current_logs
                            .iter()
                            .filter(|l| l.log_type == "thinking")
                            .count() as u64;
                        crate::models::ExecutionStats {
                            tool_calls,
                            conversation_turns,
                            thinking_count,
                        }
                    })
                    .await;
                send_event(
                    &tx_clone,
                    ExecEvent::ExecutionStats {
                        task_id: tid.clone(),
                        stats,
                    },
                );
            }

            // 推入 flusher：内部 CAS 触发后台 flush。
            log_flusher_for_stdout.push(parsed.clone()).await;
            send_event(
                &tx_clone,
                ExecEvent::Output {
                    task_id: tid.clone(),
                    entry: parsed,
                },
            );
        }
    }))
}

/// 把 executor 报回的 `usage.duration_ms` 统一覆盖成 wall-clock 实际耗时。
///
/// 设计意图（issue #513 之后）：
/// - 不同 executor 自己报的 duration 可能与"spawn 到 child.wait 返回"的实际耗时不一致；
/// - UI / 日志需要的是真实墙钟时间，而不是 executor 内部估算。
/// - usage 为 `None` 时构造一个全 0 + wall-clock duration 的占位，保证 DB 列一定有值。
///
/// 单独抽出来是因为这段"覆盖 vs 构造占位"的逻辑有 3 个分支，容易在重构里漂移；
/// 单元测试可以只盯这个 helper，无需 mock DB。
fn apply_wall_clock_duration(
    usage: Option<ExecutionUsage>,
    execution_start: std::time::Instant,
) -> Option<ExecutionUsage> {
    let wall_clock_duration_ms = execution_start.elapsed().as_millis() as u64;
    match usage {
        Some(mut u) => {
            u.duration_ms = Some(wall_clock_duration_ms);
            Some(u)
        }
        None => Some(ExecutionUsage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            total_cost_usd: None,
            duration_ms: Some(wall_clock_duration_ms),
        }),
    }
}

// ─── Pre-spawn 早返回 helper（issue #606 拆分） ───────────────────────
//
// 这四个 helper 把"任务失败时的 cleanup + Finished event + 返回 ExecutionResult"
// 集中到一处。调用方从原来 25-30 行的 inline match arm 缩到 ~5 行：
//   `return reject_xxx(...).await;`
// 同时让"finish_todo_execution / task_manager.remove / Finished event"的顺序
// 不再散落在四个分支里 —— 任何修改（比如新加 feishu 字段）只改一处。

/// 并发上限拒接：仅发 Finished 事件 + 移除 task。
/// 不调 `finish_todo_execution` —— todo 状态没变过，DB 里还是上一态。
async fn reject_concurrency_limit(
    task_manager: &TaskManager,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    running_count: usize,
    max_concurrent: u32,
) -> ExecutionResult {
    tracing::warn!(
        "Todo {} has {} execution(s) still running (limit: {}), rejecting",
        todo_id, running_count, max_concurrent
    );
    task_manager.remove(task_id).await;
    send_event(
        tx,
        ExecEvent::Finished {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: "".to_string(),
            success: false,
            result: Some(format!(
                "Todo {} has {} execution(s) still running (limit: {}). Please stop them first.",
                todo_id, running_count, max_concurrent
            )),
            feishu_bot_id: None,
            feishu_receive_id: None,
        },
    );
    ExecutionResult {
        task_id: task_id.to_string(),
        record_id: None,
    }
}

/// 没有可用 executor：发 Finished + finish_todo_execution（DB 回滚 todo 到非 running）。
/// 不发 Output 事件 —— 这里没什么可观测的执行细节可报。
async fn reject_no_executor(
    db: &Database,
    task_manager: &TaskManager,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor_type: ExecutorType,
) -> ExecutionResult {
    tracing::error!(
        "No executor available for type {:?} and no default registered",
        executor_type
    );
    let _ = db.finish_todo_execution(todo_id, false).await;
    send_event(
        tx,
        ExecEvent::Finished {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: executor_type.to_string(),
            success: false,
            result: Some("No executor available".to_string()),
            feishu_bot_id: None,
            feishu_receive_id: None,
        },
    );
    task_manager.remove(task_id).await;
    ExecutionResult {
        task_id: task_id.to_string(),
        record_id: None,
    }
}

/// 创建 execution_record 失败：finish_todo_execution + 移除 task。
/// 不发 Finished 事件 —— 没记录 id，前端无从关联；只清理内存 task。
async fn reject_create_record_failure(
    db: &Database,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    error: impl std::fmt::Display,
) -> ExecutionResult {
    tracing::error!("Failed to create execution record: {}", error);
    let _ = db.finish_todo_execution(todo_id, false).await;
    task_manager.remove(task_id).await;
    ExecutionResult {
        task_id: task_id.to_string(),
        record_id: None,
    }
}

/// `start_todo_execution` 失败：发 Output/Finished 事件 + 写 record 为 Failed 状态 +
/// finish_todo_execution + 移除 task。返回的 `record_id` 仍是 `Some`，
/// 让调用方可以基于 record_id 后续追查失败记录。
async fn reject_start_todo_failure(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor_str: &str,
    record_id: i64,
    error: impl std::fmt::Display,
) -> ExecutionResult {
    tracing::error!("Failed to start todo execution: {}", error);
    let entry = ParsedLogEntry::error(format!("Failed to start todo execution: {}", error));
    send_event(
        tx,
        ExecEvent::Output {
            task_id: task_id.to_string(),
            entry,
        },
    );
    send_event(
        tx,
        ExecEvent::Finished {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: executor_str.to_string(),
            success: false,
            result: Some("Failed to start execution".to_string()),
            feishu_bot_id: None,
            feishu_receive_id: None,
        },
    );
    let _ = db.finish_todo_execution(todo_id, false).await;
    let _ = db
        .update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: record_id,
            status: crate::models::ExecutionStatus::Failed.as_str(),
            remaining_logs: "[]",
            result: &format!("Failed to start todo execution: {}", error),
            usage: None,
            model: None,
            review_meta: None,
        })
        .await;
    task_manager.remove(task_id).await;
    ExecutionResult {
        task_id: task_id.to_string(),
        record_id: Some(record_id),
    }
}

/// Fire "进入 Completed / Failed" 的 state-change 钩子。
///
/// 之所以单独抽出来：
/// - executor 不经过 update_todo handler（status 是 db 层直接改的），
///   这里是唯一能 observe "Running -> terminal" 转换的地方；
/// - 抽出来之后 hook fire 和 `db.finish_todo_execution` 顺序可以独立测试
///   （虽然实际仍需端到端验证，但至少逻辑分支不再深嵌在 spawn 闭包中）。
///
/// `chain` 必须 clone 一份传进来，否则 spawn 闭包 move 走的 chain 会让这里
/// 借用检查失败。
fn fire_completion_hooks(
    todo: Option<&Todo>,
    todo_id: i64,
    success: bool,
    chain: Vec<i64>,
    hook_service: Arc<HookService>,
) {
    let Some(t) = todo else {
        // todo 已被删除或加载失败时跳过 hook fire —— 没有 todo 上下文就没法构造 HookContext。
        return;
    };
    let new_status = if success {
        crate::models::TodoStatus::Completed
    } else {
        crate::models::TodoStatus::Failed
    };
    if let Some(ctx) = crate::hooks::models::HookContext::for_state_change(
        todo_id,
        t.title.clone(),
        crate::models::TodoStatus::Running,
        new_status,
        t.executor.clone(),
        t.workspace.clone(),
        chain,
    ) {
        // fire_for_todo 内部 tokio::spawn，是 fire-and-forget。
        tracing::debug!(
            "firing state-change hook for todo #{} -> {:?}",
            todo_id,
            new_status
        );
        hook_service.fire_for_todo(todo_id, ctx);
    }
}

// ============================================================================
//  spawn 闭包阶段的 helper（issue #635 拆分）
//
//  `run_todo_execution` 内嵌的 `tokio::spawn` 闭包原本约 400 行，
//  按职责拆成以下 7 个 helper：
//    - emit_started_event            // Started 事件 + 首条 info 日志
//    - build_executor_command        // 构造 tokio::process::Command
//    - handle_spawn_failure          // group_spawn 失败时的清理 + 事件
//    - save_child_pid_and_close_stdin // 关 stdin + 写 PID
//    - setup_log_capture_pipeline    // flusher + stdout/stderr reader + flush timer
//    - drain_readers_and_flush       // cancel/timeout 通用清理（drain readers + finalize flusher）
//    - handle_cancellation_branch    // cancel 分支专属：写 DB + 事件
//    - handle_timeout_branch         // timeout 分支专属：写 DB + 事件
//    - persist_completion_record     // 正常完成：stats + usage + DB 更新
//    - emit_completion_event_and_cleanup // 末段 Output/Finished 事件 + remove task
//
//  每个 helper ≤ 30 行、嵌套 ≤ 2 层；spawn 闭包本体退化为"拼阶段"。
// ============================================================================

/// 发送 Started 事件 + 首条 info 日志。
///
/// 这两条信息是前端"执行已开始"的视觉信号：Started 用来切 tab / 滚动日志区，
/// info 日志用来让用户的"日志空"状态立刻出现一行，避免疑惑是否卡住。
fn emit_started_event(
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn crate::adapters::CodeExecutor,
) {
    send_event(
        tx,
        ExecEvent::Started {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: executor.executor_type().to_string(),
        },
    );
    let entry = ParsedLogEntry::info(format!("Starting {}", executor.executor_type()));
    send_event(tx, ExecEvent::Output {
        task_id: task_id.to_string(),
        entry,
    });
}

/// 构造 executor 子进程命令，统一设置 stdout/stderr/stdin 为 piped。
///
/// workspace 设置为 `cmd.current_dir`，但仅在 todo 指定 workspace 时生效——
/// 没设 workspace 的 todo 让 executor 用 daemon 当前目录即可。
fn build_executor_command(
    executable_path: &str,
    command_args: &[String],
    workspace: Option<&str>,
) -> tokio::process::Command {
    // command-group 的 group_spawn 在调用方使用，这里只构造裸 Command。
    let mut cmd = tokio::process::Command::new(executable_path);
    cmd.args(command_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::piped());
    if let Some(ws) = workspace {
        cmd.current_dir(ws);
    }
    cmd
}

/// `group_spawn` 失败时的清理：发 Output/Finished 事件 + finish_todo_execution + remove task。
///
/// 返回 bool 仅用于"是否真的需要继续后续逻辑"，调用方一般是直接 return。
async fn handle_spawn_failure(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn crate::adapters::CodeExecutor,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
    error: std::io::Error,
) {
    let error_msg = format!("Failed to spawn executor: {}", error);
    let entry = ParsedLogEntry::error(error_msg.clone());
    send_event(tx, ExecEvent::Output { task_id: task_id.to_string(), entry });
    send_event(tx, ExecEvent::Finished {
        task_id: task_id.to_string(),
        todo_id,
        todo_title: todo_title.to_string(),
        executor: executor.executor_type().to_string(),
        success: false,
        result: Some(error_msg),
        feishu_bot_id,
        feishu_receive_id,
    });
    let _ = db.finish_todo_execution(todo_id, false).await;
    task_manager.remove(task_id).await;
}

/// 关掉子进程 stdin 并把进程组 leader PID 写库。
///
/// 关 stdin 是必须的：不少 executor 在执行完后会再读一次 stdin，没有 EOF 就会 hang。
/// PID 写库是为了后续 cancel / status 查询能定位进程；child.id() == None 表示
/// 进程已退出（race），跳过写库即可。
async fn save_child_pid_and_close_stdin(
    child: &mut command_group::AsyncGroupChild,
    db: &Database,
    record_id: i64,
) {
    // Close stdin immediately so child processes get EOF when they try to read it.
    // Without this, processes that read stdin after finishing work will hang forever.
    drop(child.inner().stdin.take());
    let child_id = child.id().unwrap_or(0);
    if child_id > 0 {
        let _ = db
            .update_execution_record_pid(record_id, Some(child_id as i32))
            .await;
    }
}

/// LogFlusher 配置 + stdout/stderr reader + flush timer 一起初始化。
///
/// 拆出来是因为这三件事之间没有逻辑耦合，但都需要从 child 拿 stdout/stderr handle；
/// 把它们打包成一个函数能减少 spawn 闭包里的"先建 flusher → spawn stdout → spawn stderr →
/// spawn timer"线性铺陈，并显式表达"`LogFlusher::for_record` 才是单实例"的语义。
///
/// `SO`/`SE` 两个泛型分别对应 stdout/stderr handle 的真实类型（tokio 的 ChildStdout
/// 和 ChildStderr 是两个不同类型，没法合并成一个 `R`），其余参数都是共享引用，
/// 由调用方在闭包里 clone 进来。
async fn setup_log_capture_pipeline<SO, SE>(
    stdout_handle: Option<SO>,
    stderr_handle: Option<SE>,
    executor: Arc<dyn crate::adapters::CodeExecutor>,
    db: Arc<Database>,
    tx: broadcast::Sender<ExecEvent>,
    task_id: String,
    record_id: i64,
) -> (
    Arc<LogFlusher>,
    Option<JoinHandle<()>>,
    Option<JoinHandle<()>>,
    JoinHandle<()>,
)
where
    SO: AsyncRead + Unpin + Send + 'static,
    SE: AsyncRead + Unpin + Send + 'static,
{
    // 统一管理 stdout/stderr 推入的日志 buffer 与后台 flush。
    // 详见 `crate::log_flusher` 文档（issue #496）：用 CAS + pending 标志替代旧的
    // fetch_add+swap+store(0) 三步非原子组合；用 oneshot-style 标记驱动 shutdown。
    let log_flusher = Arc::new(crate::log_flusher::LogFlusher::new(
        Box::new(crate::log_flusher::DatabaseLogSink::new(db.clone())),
        crate::log_flusher::LogFlusherConfig::for_record(record_id),
    ));
    let stdout_task = spawn_stdout_reader(
        stdout_handle,
        executor.clone(),
        db.clone(),
        log_flusher.clone(),
        tx.clone(),
        task_id.clone(),
        record_id,
    );
    let stderr_task = spawn_stderr_reader(
        stderr_handle,
        executor.clone(),
        log_flusher.clone(),
        tx.clone(),
        task_id.clone(),
    );
    // 定时兜底 flush：每 3 秒检查未刷新条目，有则写库。
    let flush_timer = {
        let log_flusher_for_timer = log_flusher.clone();
        tokio::spawn(async move { log_flusher_for_timer.run_timer().await })
    };
    (log_flusher, stdout_task, stderr_task, flush_timer)
}

/// cancel / timeout 分支共享的清理：杀进程 → 等子进程退出 → drain readers → finalize flusher → 等 timer。
///
/// `child.inner().wait()` 与 `child.wait()` 等价；这里保留 `child.wait()` 是为了
/// 复用 Rust 的 Drop 语义（命令组在 child drop 时清理进程组）。
///
/// `log_flusher` 接 `Arc<LogFlusher>` 而不是 `&LogFlusher`，因为 `finalize` 的签名
/// 是 `async fn finalize(self: Arc<Self>)`，需要拿所有权才能走完 drain 流程。
async fn drain_readers_and_flush(
    child: &mut command_group::AsyncGroupChild,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<LogFlusher>,
    flush_timer: JoinHandle<()>,
) {
    // 收割僵尸进程
    let _status = child.wait().await;
    if let Some(handle) = stdout_task {
        let _ = handle.await;
    }
    if let Some(handle) = stderr_task {
        let _ = handle.await;
    }
    // LogFlusher::finalize 一并处理:
    //   1) 标记 shutdown 让 timer 退出
    //   2) 等所有 in-flight flush task 完成
    //   3) drain buffer 残余一次性 append
    //   4) 等 timer + 所有 spawned flush task 退出
    log_flusher.finalize().await;
    let _ = flush_timer.await;
}

/// 读全部 execution_logs 序列化为 JSON 字符串。
///
/// 失败一律回落到 `"[]"`，避免外部 IO 错误把 cancel / timeout 流程带崩。
async fn fetch_remaining_logs_json(db: &Database, record_id: i64) -> String {
    db.get_all_execution_logs(record_id)
        .await
        .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "[]".to_string()))
        .unwrap_or_else(|_| "[]".to_string())
}

/// cancel 分支末段：写 DB（cancelled/failed + 空 result） + 发 Output/Finished 事件 + remove task。
///
/// 抽出来让 cancel 分支的 select! 臂保持 ≤ 30 行：杀进程 + drain 已经抽到
/// `drain_readers_and_flush`，剩下的就是"DB + 事件 + cleanup"。
async fn handle_cancellation_branch(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn crate::adapters::CodeExecutor,
    record_id: i64,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
) {
    let _ = db.update_todo_status(todo_id, crate::models::TodoStatus::Cancelled).await;
    let _ = db.update_todo_task_id(todo_id, None).await;
    let remaining_logs = fetch_remaining_logs_json(db, record_id).await;
    let _ = db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
        id: record_id,
        status: crate::models::ExecutionStatus::Failed.as_str(),
        remaining_logs: &remaining_logs,
        result: "任务已被手动停止",
        usage: None,
        model: None,
        review_meta: None,
    }).await;
    let entry = ParsedLogEntry::error("Execution cancelled by user");
    send_event(tx, ExecEvent::Output { task_id: task_id.to_string(), entry });
    send_event(tx, ExecEvent::Finished {
        task_id: task_id.to_string(),
        todo_id,
        todo_title: todo_title.to_string(),
        executor: executor.executor_type().to_string(),
        success: false,
        result: Some("Task was cancelled by user".to_string()),
        feishu_bot_id,
        feishu_receive_id,
    });
    task_manager.remove(task_id).await;
}

/// timeout 分支末段：写 DB（failed + 包含超时常量文案） + 发 Output/Finished 事件 + remove task。
async fn handle_timeout_branch(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn crate::adapters::CodeExecutor,
    record_id: i64,
    execution_timeout_secs: u64,
    timeout_str: String,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
) {
    tracing::warn!(
        "Execution timeout, terminating process: timeout={}s, todo_id={}, task_id={}",
        execution_timeout_secs, todo_id, task_id
    );
    let remaining_logs = fetch_remaining_logs_json(db, record_id).await;
    let _ = db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
        id: record_id,
        status: crate::models::ExecutionStatus::Failed.as_str(),
        remaining_logs: &remaining_logs,
        result: "Execution timeout",
        usage: None,
        model: None,
        review_meta: None,
    }).await;
    let entry = ParsedLogEntry::error("Execution timeout, process terminated by system");
    send_event(tx, ExecEvent::Output { task_id: task_id.to_string(), entry });
    send_event(tx, ExecEvent::Finished {
        task_id: task_id.to_string(),
        todo_id,
        todo_title: todo_title.to_string(),
        executor: executor.executor_type().to_string(),
        success: false,
        result: Some(format!("Execution timeout, exceeded {}", timeout_str)),
        feishu_bot_id,
        feishu_receive_id,
    });
    task_manager.remove(task_id).await;
}

/// 正常完成分支：把全量日志、stats、usage、model 写库。
///
/// `executor_spawn` 是 executor 的共享引用（stdout reader 里持有的是同一份），
/// 这里再调用 `get_final_result` / `get_usage` / `get_model` 不会产生竞争——
/// 这三个方法只读内部 state，且只在 finalization 时调用一次。
async fn persist_completion_record(
    db: &Database,
    executor: &dyn crate::adapters::CodeExecutor,
    record_id: i64,
    all_logs: &[crate::models::ParsedLogEntry],
    all_logs_json: &str,
    success: bool,
    execution_start: std::time::Instant,
) {
    let result_str = executor.get_final_result(all_logs).unwrap_or_default();
    let stats = extract_execution_stats(all_logs, executor.get_tool_calls_count());
    if let Ok(stats_json) = serde_json::to_string(&stats) {
        let _ = db
            .update_execution_record_stats(record_id, &stats_json)
            .await;
    }
    let final_status = if success {
        crate::models::ExecutionStatus::Success.as_str()
    } else {
        crate::models::ExecutionStatus::Failed.as_str()
    };
    // wall-clock duration 覆盖交给 helper 集中处理，避免三个终态分支各自维护。
    let usage = apply_wall_clock_duration(executor.get_usage(all_logs), execution_start);
    let model = executor.get_model();
    let _ = db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
        id: record_id,
        status: final_status,
        remaining_logs: all_logs_json,
        result: &result_str,
        usage: usage.as_ref(),
        model: model.as_deref(),
        review_meta: None,
    }).await;
}

/// executor 后置 todo_progress 钩子：把 executor 内部 state 推出的进度写库 + 发事件。
///
/// 部分 executor（如 hermes）不在 stdout 中暴露 tool call，但内部已经累积了
/// todo_progress —— 这里给它们一个补 push 的口子。
async fn emit_post_execution_todo_progress(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    executor: &dyn crate::adapters::CodeExecutor,
    task_id: &str,
    record_id: i64,
) {
    if let Some(progress) = executor.post_execution_todo_progress() {
        if let Ok(progress_json) = serde_json::to_string(&progress) {
            let _ = db
                .update_execution_record_todo_progress(record_id, &progress_json)
                .await;
            send_event(tx, ExecEvent::TodoProgress {
                task_id: task_id.to_string(),
                progress,
            });
        }
    }
}

/// 正常完成末段：auto-review + finish_todo_execution + completion hook + 末段事件 + remove task。
///
/// auto-review 仅在 `trigger_type != "auto_review"` 时启动（防止评审实例自身再触发评审）；
/// 钩子 fire 在 finish 之后调用，符合"rating gate 要求评审完成后再触发"的语义。
async fn finalize_normal_completion(
    db: Arc<Database>,
    executor_registry: Arc<crate::adapters::ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
    hook_service: Arc<HookService>,
    executor: Arc<dyn crate::adapters::CodeExecutor>,
    task_id: String,
    todo_id: i64,
    todo_title: String,
    todo: Option<Todo>,
    chain: Vec<i64>,
    record_id: i64,
    success: bool,
    exit_code: i32,
    result_str: String,
    trigger_type: String,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
) {
    // ===== 自动评审 (auto-review) =====
    // 仅在以下条件同时满足时启动:
    //   - trigger_type != "auto_review" 避免评审实例本身反向触发评审
    //   - 正常执行 (success/failed), 不是被中断
    //
    // 同步语义: Hook fire 在这之后才进行, rating gate 要求评审完成后再触发,
    // 所以评审要同步跑完. run_auto_review 内部 std::thread::spawn + block_on,
    // 与本 spawned task 完全隔离, 不存在 Send 问题.
    if trigger_type != "auto_review" {
        run_auto_review(
            db.clone(),
            executor_registry.clone(),
            tx.clone(),
            task_manager.clone(),
            config.clone(),
            hook_service.clone(),
            todo_id,
            record_id,
        )
        .await;
    }
    let _ = db.finish_todo_execution(todo_id, success).await;
    // 拆到 `fire_completion_hooks`：构造 HookContext 的样板代码集中维护，
    // todo 缺失 / HookContext 构造失败的早返回都在 helper 里处理。
    fire_completion_hooks(todo.as_ref(), todo_id, success, chain, hook_service.clone());
    let entry = ParsedLogEntry::new(
        if success { "info" } else { "error" },
        format!(
            "Executor finished with exit_code: {}, result: {}",
            exit_code, result_str
        ),
    );
    send_event(&tx, ExecEvent::Output { task_id: task_id.clone(), entry });
    send_event(&tx, ExecEvent::Finished {
        task_id: task_id.clone(),
        todo_id,
        todo_title: todo_title.clone(),
        executor: executor.executor_type().to_string(),
        success,
        result: Some(result_str),
        feishu_bot_id,
        feishu_receive_id,
    });
    task_manager.remove(&task_id).await;
}

/// Run a todo execution. Priority: explicit executor > todo stored executor > default.
///
/// 整条执行路径放进一个 `todo_execution` span，附 todo_id / trigger_type / req_executor
/// 三个字段：issue #513 的诉求是「执行器调用追踪」，而 spawn 子任务、stdout/stderr
/// 读取、log flush、database update、hook fire 这一长串环节（参见原 issue 333-1013）
/// 现在会被一个统一的 span 包住，配合 request_id 中间件，上游 HTTP 入口的 trace_id
/// 可以贯穿到执行末段，便于定位「某个 todo 整体耗时多少、哪一段最慢」。
///
/// 注意：request_id **没有**显式作为 `todo_execution` span 的字段，
/// 因为 request 来源不一定都来自 HTTP（如 cron / webhook / 飞书）——具体来源由
/// 各自的调用方在自己的 span（如 `http_request`）里记录，避免在 `todo_execution` span
/// 里误导排查。如果未来需要跨源关联（HTTP 请求触发的 todo 与 daemon 重启后的 cron 触发的
/// todo 关联），需要扩展 `RunTodoExecutionRequest` 增加 `request_id: Option<String>`
/// 字段并在此 span 上声明。
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
    // Extract `chain` before the rest so it stays available for cloning in
    // the pre-hook section and in `fire_completion_hooks` at the end.
    let chain = request.chain;
    let RunTodoExecutionRequest {
        db,
        executor_registry,
        tx,
        task_manager,
        config,
        hook_service,
        todo_id,
        message,
        req_executor,
        trigger_type,
        params,
        resume_session_id,
        resume_message,
        source_todo_id,
        source_todo_title,
        source_hook_id,
        feishu_bot_id,
        feishu_receive_id,
        ..
    } = request;
    let message = params
        .as_ref()
        .map(|params| crate::models::replace_placeholders(&message, params))
        .unwrap_or(message);
    let task_id = Uuid::new_v4().to_string();
    // Issue #506：用 RAII guard 注册 task，确保即便后续路径 panic/早返回忘了 remove，
    // sender 也会被 guard drop 时清理。guard 在此函数尾部才 drop，等价于覆盖整段 task 生命周期。
    let mut task_guard = task_manager.register_with_guard(task_id.clone()).await;
    let mut cancel_rx = task_guard.take_receiver();

    // Read runtime settings from config
    let (max_concurrent, timeout_secs) = {
        let cfg = config.read().unwrap();
        (cfg.max_concurrent_todos, cfg.execution_timeout_secs)
    };

    // 加载 todo + 并发检查。Load todo metadata for executor selection, then run the
    // zombie-aware concurrency check via `count_active_running_for_todo`.
    // 任一步失败都返回失败 ExecutionResult（task_id 已生成，调用方可以基于它取消 task）。
    let todo = match db.get_todo(todo_id).await {
        Ok(Some(t)) => {
            let running_count_for_todo =
                match count_active_running_for_todo(&task_manager, &db, todo_id).await {
                    Ok(n) => n,
                    Err(()) => return ExecutionResult { task_id, record_id: None },
                };
            if running_count_for_todo >= max_concurrent as usize {
                return reject_concurrency_limit(
                    &task_manager, &tx, &task_id, todo_id, &t.title,
                    running_count_for_todo, max_concurrent,
                ).await;
            }
            Some(t)
        }
        Ok(None) => None,
        Err(e) => {
            tracing::error!("Failed to fetch todo {} for executor selection: {}", todo_id, e);
            None
        }
    };
    let todo_executor = todo.as_ref().and_then(|t| t.executor.clone());
    let todo_workspace = todo.as_ref().and_then(|t| t.workspace.clone());
    let todo_worktree_enabled = todo.as_ref().map(|t| t.worktree_enabled).unwrap_or(false);

    // Fire before_execution hooks synchronously — block until all pre-flight targets finish.
    // If the hook fails and the user didn't set skip_if_missing, we abort the main execution.
    // We skip firing altogher when `todo` is None (todo was deleted between scheduling and now).
    if let Some(ref t) = todo {
        let ctx = crate::hooks::models::HookContext::for_before_execution(
            todo_id,
            t.title.clone(),
            t.executor.clone(),
            t.workspace.clone(),
            chain.clone(),
        );
        match hook_service.clone().fire_before_execution(todo_id, ctx).await {
            Ok(()) => {}
            Err(msg) => {
                // Pre-hook failed — abort this execution without creating a record.
                tracing::warn!("aborting execution due to pre-hook failure: {}", msg);
                return ExecutionResult { task_id, record_id: None };
            }
        }
    }

    // Determine which executor to use: explicit > todo stored > default.
    // 抽到 `resolve_executor_type` 让 warn 日志集中，并支持单测。
    let executor_type =
        resolve_executor_type(req_executor.as_deref(), todo_executor.as_deref());

    let executor = match executor_registry.get(executor_type).await {
        Some(exec) => exec,
        None => match executor_registry.get_default().await {
            Some(exec) => exec,
            None => {
                return reject_no_executor(
                    &db, &task_manager, &tx, &task_id, todo_id,
                    todo.as_ref().map(|t| t.title.as_str()).unwrap_or(""),
                    executor_type,
                ).await;
            }
        },
    };

    let executable_path = executor.executable_path().to_string();
    let session_id_for_executor = resume_session_id.as_deref().unwrap_or(&task_id);
    let is_resume = resume_session_id.is_some();
    let mut command_args =
        executor.command_args_with_session(&message, Some(session_id_for_executor), is_resume);

    // 抽到 `apply_worktree_flag`：claude_code / hermes 之外不插，避免污染其它 executor 的 argv。
    apply_worktree_flag(
        &mut command_args,
        executor.executor_type(),
        todo_worktree_enabled,
    );

    // Update todo's executor to the one being used
    let executor_str = executor.executor_type().to_string();
    if let Err(e) = db.update_todo_executor(todo_id, &executor_str).await {
        tracing::error!("Failed to update todo executor: {}", e);
    }

    // Create execution record
    let command = format!("{} {}", executable_path, command_args.join(" "));
    let record_id = match db
        .create_execution_record(NewExecutionRecord {
            todo_id,
            command: &command,
            executor: &executor_str,
            trigger_type: &trigger_type,
            task_id: &task_id,
            session_id: Some(session_id_for_executor),
            resume_message: resume_message.as_deref(),
            source_todo_id,
            source_todo_title: source_todo_title.as_deref(),
            source_hook_id,
        })
        .await
    {
        Ok(id) => id,
        Err(e) => {
            return reject_create_record_failure(&db, &task_manager, &task_id, todo_id, e).await;
        }
    };

    // issue #643: 如果 todo 绑定的项目目录开启了 worktree 自动管理,
    // 在这里创建 worktree 并把路径写回 execution_record. 失败时回退到原 workspace,
    // 不阻塞执行. effective_workspace 决定子进程 cwd, 同时被 move 进 spawn 闭包用于 cleanup.
    let worktree_ctx = resolve_worktree_context(&db, &todo).await;
    record_worktree_path(&db, record_id, worktree_ctx.record_path.as_deref()).await;
    let effective_workspace = worktree_ctx
        .effective_workspace
        .clone()
        .or(todo_workspace.clone());

    // State-change hooks for "进入执行中" fire from the update_todo handler when
    // the user transitions the todo into in_progress. The executor no longer
    // gates execution on a hook — it just runs.

    // Update todo status to running and associate with task
    if let Err(e) = db.start_todo_execution(todo_id, &task_id).await {
        // worktree 已在此之前创建并写入 record_path；失败路径下若启用了 auto_cleanup
        // 必须立刻清理，避免遗留 worktree 目录/分支与「未启动成功」的执行记录错位。
        cleanup_worktree_if_needed(&worktree_ctx);
        return reject_start_todo_failure(
            &db, &tx, &task_manager, &task_id, todo_id,
            todo.as_ref().map(|t| t.title.as_str()).unwrap_or(""),
            &executor_str, record_id, e,
        ).await;
    }

    let task_id_return = task_id.clone();
    let db_clone = db.clone();
    let tx_clone = tx.clone();
    let executor_spawn = executor.clone();
    let task_manager_spawn = task_manager.clone();
    let executor_registry_spawn = executor_registry.clone();
    let config_spawn = config.clone();
    // 共享 AppState 的 hook_service：避免在执行末段 fire 钩子时再 Arc::new 一份 HookService
    // （参见 RunTodoExecutionRequest::hook_service 字段注释）。
    let hook_service_spawn = hook_service.clone();

    let todo_title = todo.as_ref().map(|t| t.title.clone()).unwrap_or_default();
    let execution_timeout_secs = timeout_secs;

    // 注册任务信息，用于 WebSocket 同步
    task_manager
        .register_info(crate::task_manager::TaskInfo {
            task_id: task_id.clone(),
            todo_id,
            todo_title: todo_title.clone(),
            executor: executor_spawn.executor_type().to_string(),
            logs: "[]".to_string(), // 初始为空，WebSocket 同步时会从数据库获取实际日志
        })
        .await;

    // 为整个 spawn 闭包建立 executor_run span：
    // tokio::spawn 不会自动继承外层 span（参见 issue #513），所以需要把异步块整体包到
    // Instrument 中。这样 child process spawn / stdout/stderr / log flush / db update /
    // hook fire 这一长串环节的日志都会被 executor_run span 包住。
    //
    // 关于 span hierarchy 的注意：`todo_execution` span 由 `#[tracing::instrument]` 在
    // `run_todo_execution` 进入时建立、函数返回时退出；而下面的 `tokio::spawn` 是
    // fire-and-forget，闭包实际开始执行时 `run_todo_execution` 已经返回，
    // `todo_execution` span 也已经关闭。所以在运行时 `executor_run` 不会作为
    // `todo_execution` 的活动子 span 出现——这里 `Span::current()` 拿到的仅是
    // 退出态的 parent 引用。span 树查看工具会显示孤儿 `executor_run` 事件，
    // 这是预期的；真正的两层实时嵌套需要把 spawn 改成 join 模式（详见 issue #513）。
    let executor_span = tracing::info_span!(
        "executor_run",
        task_id = %task_id,
        todo_id = todo_id,
        record_id = record_id,
        executor = %executor_spawn.executor_type(),
    );

    tokio::spawn(
        async move {
        // 将 task_guard 移入异步闭包，使其存活到整个执行周期。
        // 若不绑定，外层 drop 时会误删 Sender 导致 cancel_rx.recv() 返回 None。
        let _task_guard = task_guard;
        // wall-clock 起点在 spawned 闭包最早位置取，避免后续 finalization 时把
        // setup / DB 写入耗时也算进 duration。
        let execution_start = std::time::Instant::now();

        emit_started_event(
            &tx_clone,
            &task_id,
            todo_id,
            &todo_title,
            executor_spawn.as_ref(),
        );

        tracing::debug!(
            executable = %executable_path,
            arg_count = command_args.len(),
            "Spawning executor"
        );
        let mut cmd = build_executor_command(
            &executable_path,
            &command_args,
            effective_workspace.as_deref(),
        );

        // 使用 command-group 的 group_spawn 创建进程组
        let mut child = match cmd.group_spawn() {
            Ok(c) => c,
            Err(e) => {
                // spawn 失败：worktree 不会留下孤儿文件，但 record_path 已经被回写到 DB，
                // 如果 auto_cleanup=true 必须在这里显式清理，否则下次「同 todo_id 同秒」
                // 重试时会被前面的 exists 守卫拦下，导致 worktree 永远不清理。
                cleanup_worktree_if_needed(&worktree_ctx);
                handle_spawn_failure(
                    &db_clone,
                    &tx_clone,
                    &task_manager_spawn,
                    &task_id,
                    todo_id,
                    &todo_title,
                    executor_spawn.as_ref(),
                    feishu_bot_id,
                    feishu_receive_id,
                    e,
                )
                .await;
                return;
            }
        };
        save_child_pid_and_close_stdin(&mut child, &db_clone, record_id).await;

        let stdout_handle = child.inner().stdout.take();
        let stderr_handle = child.inner().stderr.take();
        let (log_flusher, stdout_task, stderr_task, flush_timer) = setup_log_capture_pipeline(
            stdout_handle,
            stderr_handle,
            executor_spawn.clone(),
            db_clone.clone(),
            tx.clone(),
            task_id.clone(),
            record_id,
        )
        .await;

        // execution_timeout_secs is captured by value here — config changes after this
        // task starts have no effect. To pick up a new timeout, wait for the current
        // execution to finish (or force-fail it via the UI).
        let timeout_enabled = execution_timeout_secs > 0;
        // Non-zero values are guaranteed >= 60 by normalize_paths clamp, so from_secs is safe.
        let timeout_duration = std::time::Duration::from_secs(execution_timeout_secs);
        let timeout_str = format_timeout_secs(execution_timeout_secs);
        let timeout_sleep = tokio::time::sleep(timeout_duration);
        tokio::pin!(timeout_sleep);

        // 用 enum 携带 select! 结果，避免在三个分支里各重复"杀进程 + drain + finalize"的
        // 清理模板。每个分支走完之后 child 仍然在手，能继续调 kill_process_tree。
        enum RunOutcome {
            Cancelled,
            TimedOut,
            Completed(std::io::Result<std::process::ExitStatus>),
        }
        let outcome = tokio::select! {
            biased;
            _ = cancel_rx.recv() => RunOutcome::Cancelled,
            _ = &mut timeout_sleep, if timeout_enabled => RunOutcome::TimedOut,
            status = child.wait() => RunOutcome::Completed(status),
        };

        match outcome {
            RunOutcome::Cancelled => {
                // Cancelled (or channel closed): 使用 command-group 安全杀死整个进程组
                kill_process_tree(&mut child).await;
                drain_readers_and_flush(
                    &mut child,
                    stdout_task,
                    stderr_task,
                    log_flusher.clone(),
                    flush_timer,
                )
                .await;
                handle_cancellation_branch(
                    &db_clone,
                    &tx_clone,
                    &task_manager_spawn,
                    &task_id,
                    todo_id,
                    &todo_title,
                    executor_spawn.as_ref(),
                    record_id,
                    feishu_bot_id,
                    feishu_receive_id,
                )
                .await;
                // issue #643: 取消路径也要按 auto_cleanup 清理 worktree
                cleanup_worktree_if_needed(&worktree_ctx);
            }
            RunOutcome::TimedOut => {
                kill_process_tree(&mut child).await;
                drain_readers_and_flush(
                    &mut child,
                    stdout_task,
                    stderr_task,
                    log_flusher.clone(),
                    flush_timer,
                )
                .await;
                handle_timeout_branch(
                    &db_clone,
                    &tx_clone,
                    &task_manager_spawn,
                    &task_id,
                    todo_id,
                    &todo_title,
                    executor_spawn.as_ref(),
                    record_id,
                    execution_timeout_secs,
                    timeout_str,
                    feishu_bot_id,
                    feishu_receive_id,
                )
                .await;
                // issue #643: 超时路径也要按 auto_cleanup 清理 worktree
                cleanup_worktree_if_needed(&worktree_ctx);
            }
            RunOutcome::Completed(status) => {
                // 子进程已自然退出，stdout/stderr 管道已关闭；先 await reader 让它们
                // 把 buffer 里残余的行都解析完，再 finalize flusher 一次性写库。
                if let Some(handle) = stdout_task {
                    let _ = handle.await;
                }
                if let Some(handle) = stderr_task {
                    let _ = handle.await;
                }
                let exit_code = status
                    .as_ref()
                    .map(|s| s.code().unwrap_or(-1))
                    .unwrap_or(-1);
                let success = executor_spawn.check_success(exit_code);

                emit_post_execution_todo_progress(
                    &db_clone,
                    &tx_clone,
                    executor_spawn.as_ref(),
                    &task_id,
                    record_id,
                )
                .await;

                // 正常退出：与 cancel/timeout 一样走 finalize 把残余刷到 DB
                log_flusher.finalize().await;
                let _ = flush_timer.await;

                let all_logs_snapshot = db_clone
                    .get_all_execution_logs(record_id)
                    .await
                    .unwrap_or_default();
                let all_logs_json =
                    serde_json::to_string(&all_logs_snapshot).unwrap_or_else(|e| {
                        tracing::error!("Failed to serialize all logs: {}", e);
                        "[]".to_string()
                    });
                let result_str = executor_spawn
                    .get_final_result(&all_logs_snapshot)
                    .unwrap_or_default();

                persist_completion_record(
                    &db_clone,
                    executor_spawn.as_ref(),
                    record_id,
                    &all_logs_snapshot,
                    &all_logs_json,
                    success,
                    execution_start,
                )
                .await;

                finalize_normal_completion(
                    db_clone.clone(),
                    executor_registry_spawn.clone(),
                    tx_clone.clone(),
                    task_manager_spawn.clone(),
                    config_spawn.clone(),
                    hook_service_spawn.clone(),
                    executor_spawn.clone(),
                    task_id.clone(),
                    todo_id,
                    todo_title.clone(),
                    todo.clone(),
                    chain.clone(),
                    record_id,
                    success,
                    exit_code,
                    result_str,
                    trigger_type.clone(),
                    feishu_bot_id,
                    feishu_receive_id,
                )
                .await;
                // issue #643: 正常完成路径按 auto_cleanup 清理 worktree
                cleanup_worktree_if_needed(&worktree_ctx);
            }
        }
    }
    .instrument(executor_span));

    ExecutionResult {
        task_id: task_id_return,
        record_id: Some(record_id),
    }
}

/// 独立 runtime. 用于 run_auto_review 在原 todo 的 spawned task 内部同步运行
/// 自动评审逻辑, 避免与外层 spawned task 产生 Send / 嵌套 spawn 问题.
fn review_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("auto-review-runtime")
            .build()
            .expect("failed to build auto-review runtime")
    })
}

/// Run a todo execution with parameter substitution.
/// Replaces placeholders `{{key}}` in the message with corresponding values from params before execution.
pub async fn run_todo_execution_with_params(
    mut request: RunTodoExecutionRequest,
) -> ExecutionResult {
    if let Some(params) = request.params.take() {
        request.message = crate::models::replace_placeholders(&request.message, &params);
    }
    run_todo_execution(request).await
}

// ============================================================================
//  自动评审 (auto-review) —— 同步派生一个评审 todo，给刚完成的那条执行记录打分
// ============================================================================
//
// 调用点: run_todo_execution 在 update_execution_record (写终态) 之后.
// 仅当:
//   - 源 todo 是 normal 类型 (todo_type=0)
//   - auto_review_enabled=true
//   - 源 record 进入了 success 或 failed 终态
//   - source_execution_record_id 尚未被设置 (避免重复评审同一条记录)
// 才启动评审。
//
// 为避免与 run_todo_execution 的内部逻辑产生循环引用，这里用一个简化的
// 同步路径：等 run_todo_execution 启动后创建的 record 进入终态，再解析 rating 回填。
// 不需要 tokio::spawn —— 我们让原执行路径在 auto_review 上同步等待。

/// 同步运行自动评审。在原 todo 执行完成、update_execution_record 写入 success/failed 后调用。
///
/// 参数: (db, todo, record_id, executor_registry, tx, task_manager, config, hook_service).
/// 任何错误都只记 warn 日志，不影响原 todo 的完成响应。
///
/// 实现: 由于 run_auto_review_inner 内部需要 await run_todo_execution (后者会
/// 进一步 spawn) —— 整个 future 不是 Send —— 必须在独立 runtime 上 block_on.
pub async fn run_auto_review(
    db: Arc<crate::db::Database>,
    executor_registry: Arc<crate::adapters::ExecutorRegistry>,
    tx: tokio::sync::broadcast::Sender<crate::handlers::ExecEvent>,
    task_manager: Arc<crate::task_manager::TaskManager>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
    hook_service: Arc<HookService>,
    todo_id: i64,
    record_id: i64,
) {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let db_c = db.clone();
    let er_c = executor_registry.clone();
    let tx_c = tx.clone();
    let tx_outer = tx.clone();
    let tm_c = task_manager.clone();
    let cfg_c = config.clone();
    let hs_c = hook_service.clone();
    let runtime = review_runtime();
    std::thread::spawn(move || {
        let result = runtime.block_on(run_auto_review_inner(
            db_c, er_c, tx_c, tm_c, cfg_c, hs_c, todo_id, record_id,
        ));
        let _ = reply_tx.send(result);
    });
    match reply_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!(
                "auto-review for todo #{} record #{} failed: {}",
                todo_id, record_id, e
            );
            let _ = db
                .set_record_last_review_status(record_id, "failed")
                .await;
            let _ = tx_outer.send(crate::handlers::ExecEvent::ReviewStatusChanged {
                record_id,
                todo_id,
                review_status: "failed".to_string(),
            });
        }
        Err(_) => {
            tracing::warn!("auto-review thread dropped reply for todo #{} record #{}", todo_id, record_id);
            let _ = db
                .set_record_last_review_status(record_id, "failed")
                .await;
            let _ = tx_outer.send(crate::handlers::ExecEvent::ReviewStatusChanged {
                record_id,
                todo_id,
                review_status: "failed".to_string(),
            });
        }
    }
}

async fn run_auto_review_inner(
    db: Arc<crate::db::Database>,
    executor_registry: Arc<crate::adapters::ExecutorRegistry>,
    tx: tokio::sync::broadcast::Sender<crate::handlers::ExecEvent>,
    task_manager: Arc<crate::task_manager::TaskManager>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
    hook_service: Arc<HookService>,
    todo_id: i64,
    record_id: i64,
) -> Result<(), String> {
    use crate::services::auto_review::{ensure_reviewer_template, parse_rating_from_result, DEFAULT_REVIEWER_PROMPT, MAX_OUTPUT_CHARS, REVIEWER_TEMPLATE_TITLE};

    // 1) 加载原 todo & 检查前置条件
    let original = db.get_todo(todo_id).await
        .map_err(|e| format!("load original todo: {}", e))?
        .ok_or_else(|| format!("original todo #{} not found", todo_id))?;
    if original.todo_type != 0 || !original.auto_review_enabled {
        let _ = db.set_record_last_review_status(record_id, "skipped").await;
        let _ = tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
            record_id, todo_id, review_status: "skipped".to_string(),
        });
        return Ok(());
    }
    let record = db.get_execution_record(record_id).await
        .map_err(|e| format!("load record: {}", e))?
        .ok_or_else(|| format!("record #{} not found", record_id))?;
    use crate::models::ExecutionStatus;
    if !matches!(record.status, ExecutionStatus::Success | ExecutionStatus::Failed) {
        let _ = db.set_record_last_review_status(record_id, "skipped").await;
        let _ = tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
            record_id, todo_id, review_status: "skipped".to_string(),
        });
        return Ok(());
    }
    // 避免重复评审
    if record.last_review_status.as_deref() == Some("success") {
        return Ok(());
    }

    // 2) 评审任务
    let template_id = ensure_reviewer_template(&db, REVIEWER_TEMPLATE_TITLE, DEFAULT_REVIEWER_PROMPT).await?;
    let template = db.get_todo(template_id).await
        .map_err(|e| format!("reload template: {}", e))?
        .ok_or_else(|| "reviewer template vanished".to_string())?;

    // 3) 截断输出 + 合并 prompt
    let original_output = record.result.clone().unwrap_or_default();
    let truncated: String = if original_output.chars().count() > MAX_OUTPUT_CHARS {
        let mut s: String = original_output.chars().take(MAX_OUTPUT_CHARS).collect();
        s.push_str("\n\n[...以下被截断...]");
        s
    } else {
        original_output
    };
    let acceptance_criteria = original
        .acceptance_criteria
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("(无验收标准 —— 由评审师自行判断输出质量)");
    let composed_prompt = template
        .prompt
        .replace("{original_prompt}", &original.prompt)
        .replace("{max_output_chars}", &MAX_OUTPUT_CHARS.to_string())
        .replace("{original_output}", &truncated)
        .replace("{acceptance_criteria}", acceptance_criteria);

    // 4) 复用评审任务 todo，直接执行（不 clone 新实例）
    let review_todo_id = template_id;

    // 5) 标记 pending
    let _ = db.set_record_last_review_status(record_id, "pending").await;
    let _ = db.set_record_last_reviewed_at(record_id).await;
    let _ = tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
        record_id,
        todo_id,
        review_status: "pending".to_string(),
    });

    // 6) 同步执行评审实例
    let request = RunTodoExecutionRequest {
        db: db.clone(),
        executor_registry: executor_registry.clone(),
        tx: tx.clone(),
        task_manager: task_manager.clone(),
        config: config.clone(),
        // 评审实例同样要复用同一个 HookService，确保评审触发的钩子与其他执行路径
        // 共享状态；如果评审触发链上又产生新的执行，hook_service 不会再被重复构造。
        hook_service: hook_service.clone(),
        todo_id: review_todo_id,
        message: composed_prompt,
        req_executor: template.executor.clone(),
        trigger_type: "auto_review".to_string(),
        params: None,
        resume_session_id: None,
        resume_message: None,
        chain: vec![],
        source_todo_id: Some(original.id),
        source_todo_title: Some(original.title.clone()),
        source_hook_id: None,
        feishu_bot_id: None,
        feishu_receive_id: None,
    };
    let exec_result = run_todo_execution(request).await;
    let review_record_id = match exec_result.record_id {
        Some(id) => id,
        None => {
            let _ = db.set_record_last_review_status(record_id, "failed").await;
            let _ = tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
                record_id, todo_id, review_status: "failed".to_string(),
            });
            return Err("review execution produced no record (rejected?)".to_string());
        }
    };

    // 7) 轮询评审实例 record 的终态
    let max_wait = std::time::Duration::from_secs(300);
    let poll = std::time::Duration::from_millis(500);
    let start = std::time::Instant::now();
    let final_review = loop {
        if start.elapsed() > max_wait {
            return Err("review record timeout".to_string());
        }
        if let Some(rec) = db.get_execution_record(review_record_id).await
            .map_err(|e| format!("poll: {}", e))?
        {
            if !matches!(rec.status, ExecutionStatus::Running) {
                break rec;
            }
        }
        tokio::time::sleep(poll).await;
    };

    // 8) 解析 + 回填
    let review_status_str = match final_review.status {
        ExecutionStatus::Success => "success",
        ExecutionStatus::Failed => "failed",
        _ => "interrupted",
    };
    let rating = parse_rating_from_result(final_review.result.as_deref());
    if let Some(r) = rating {
        let _ = db.update_execution_record_rating(record_id, Some(r)).await;
    }
    let _ = db.link_review_to_source(review_record_id, record_id, review_status_str).await;
    let _ = db.set_record_last_review_status(record_id, review_status_str).await;
    let _ = tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
        record_id,
        todo_id,
        review_status: review_status_str.to_string(),
    });

    tracing::info!(
        "auto-review done: original_todo=#{} record=#{} review_todo=#{} review_record=#{} status={} rating={:?}",
        todo_id, record_id, review_todo_id, review_record_id, review_status_str, rating
    );
    Ok(())
}


#[cfg(test)]
mod run_todo_execution_request_tests {
    //! 验证 `RunTodoExecutionRequest::hook_service` 字段被正确暴露，
    //! 防止后续重构无意中把它移除/改名、导致 executor_service 末段又
    //! 回退到 `Arc::new(HookService::new(...))` 重复构造 (issue #509)。
    use super::*;

    /// 编译期断言：把 `RunTodoExecutionRequest` 的 `hook_service` 字段
    /// 投影成 `&Arc<HookService>`，相当于把字段的类型和名字"钉死"。
    /// 如果字段被删除/改名/换类型，下面这一行会直接编译失败，提示
    /// 重构者违反了 #509 的设计意图（所有 fire 钩子入口必须共享一个
    /// HookService 单例）。
    fn _hook_service_field_is_arc_hook_service(r: &RunTodoExecutionRequest) -> &Arc<HookService> {
        &r.hook_service
    }
}

/// 格式化超时秒数为人类可读字符串。
///
/// 使用 hours for >=60 min, days for >=24 h to keep the output readable.
/// 精度取舍：只精确到分钟级别（秒数只在 <60s 时显示），后端 timeout 精度
/// 为秒级，分钟以上的秒数误差在 UI 上无感知差异。
///
/// 边界情况：
/// - 0 秒 → "0 min"（表示无超时限制）
/// - 60-3599 秒 → "X min Y sec" 格式
/// - 3600+ 秒 → "X hour(s)" 或 "X day(s)"
fn format_timeout_secs(secs: u64) -> String {
    let total_min = secs / 60;
    let remaining_secs = secs % 60;
    if remaining_secs == 0 {
        if total_min >= 1440 {
            format!("{} day(s)", total_min / 1440)
        } else if total_min >= 60 {
            format!("{} hour(s)", total_min / 60)
        } else {
            format!("{} min", total_min)
        }
    } else {
        format!("{} min {} sec", total_min, remaining_secs)
    }
}

/// 从日志中提取执行统计信息。
///
/// 单次遍历日志，计算 tool_calls、conversation_turns、thinking_count。
/// 如果 executor 提供了自己的 tool_calls_count，则使用 executor 的值。
///
/// 设计意图：
/// - 不同 executor 的 tool_use 事件格式各异，部分 executor（如 hermes）
///   有自己的工具调用计数器，比日志解析更准确。
/// - conversation_turns 通过文本/结果/助手事件计数，估算 AI 交互轮次。
/// - thinking_count 用于展示 AI 思考过程的复杂度。
///
/// 统计逻辑：
/// - tool_use/tool_call/tool → 工具调用（使用 executor 提供的值覆盖，见后文）
/// - assistant/result/text → 对话轮次（每个文本输出算一轮）
/// - thinking → 思考次数
/// - 其他 log_type 跳过
fn extract_execution_stats(
    logs: &[crate::models::ParsedLogEntry],
    executor_tool_calls: Option<u64>,
) -> crate::models::ExecutionStats {
    let mut tool_calls = 0u64;
    let mut conversation_turns = 0u64;
    let mut thinking_count = 0u64;

    for l in logs {
        match l.log_type.as_str() {
            "tool_use" | "tool_call" | "tool" => tool_calls += 1,
            "assistant" | "result" | "text" => conversation_turns += 1,
            "thinking" => thinking_count += 1,
            _ => {}
        }
    }

    // Override tool_calls if executor provides its own count
    let tool_calls = executor_tool_calls.unwrap_or(tool_calls);

    crate::models::ExecutionStats {
        tool_calls,
        conversation_turns,
        thinking_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timeout_secs() {
        assert_eq!(format_timeout_secs(0), "0 min");
        assert_eq!(format_timeout_secs(60), "1 min");
        assert_eq!(format_timeout_secs(90), "1 min 30 sec");
        assert_eq!(format_timeout_secs(3600), "1 hour(s)");
        assert_eq!(format_timeout_secs(86400), "1 day(s)");
        assert_eq!(format_timeout_secs(7200), "2 hour(s)");
    }

    #[test]
    fn test_extract_execution_stats() {
        let logs = vec![
            crate::models::ParsedLogEntry {
                log_type: "tool_use".to_string(),
                ..Default::default()
            },
            crate::models::ParsedLogEntry {
                log_type: "assistant".to_string(),
                ..Default::default()
            },
            crate::models::ParsedLogEntry {
                log_type: "thinking".to_string(),
                ..Default::default()
            },
        ];

        let stats = extract_execution_stats(&logs, None);
        assert_eq!(stats.tool_calls, 1);
        assert_eq!(stats.conversation_turns, 1);
        assert_eq!(stats.thinking_count, 1);
    }

    #[test]
    fn test_extract_execution_stats_with_executor_override() {
        let logs = vec![
            crate::models::ParsedLogEntry {
                log_type: "tool_use".to_string(),
                ..Default::default()
            },
        ];

        let stats = extract_execution_stats(&logs, Some(5));
        assert_eq!(stats.tool_calls, 5); // overridden
        assert_eq!(stats.conversation_turns, 0);
        assert_eq!(stats.thinking_count, 0);
    }

    // ─── issue #606 拆出的 helper 单测 ────────────────────────────────

    /// `resolve_executor_type` 在显式 > 存储 > 默认 三层优先级上行为正确。
    /// 同时验证无法解析的字符串会被降级到下一优先级，而不是直接吞掉返回 None。
    #[test]
    fn test_resolve_executor_type_priority_chain() {
        // 显式有效值优先于 todo 存储值。
        assert_eq!(
            resolve_executor_type(Some("codex"), Some("claudecode")),
            ExecutorType::Codex
        );
        // 显式无效时降级到 todo 存储值。
        assert_eq!(
            resolve_executor_type(Some("bogus"), Some("hermes")),
            ExecutorType::Hermes
        );
        // todo 存储值也无效时回到默认。
        assert_eq!(
            resolve_executor_type(Some("bogus"), Some("also_bogus")),
            ExecutorType::default()
        );
        // 两者都缺失时回到默认。
        assert_eq!(resolve_executor_type(None, None), ExecutorType::default());
        // 只有 todo 存储值有效时使用它。
        assert_eq!(
            resolve_executor_type(None, Some("pi")),
            ExecutorType::Pi
        );
        // 只有显式值有效时使用它。
        assert_eq!(
            resolve_executor_type(Some("kimi"), None),
            ExecutorType::Kimi
        );
    }

    /// `apply_worktree_flag` 只对 Claudecode / Hermes 起作用，且插在 --session-id / --resume 之前。
    /// 其他 executor 即使 todo 开了 worktree 也不会被污染 argv。
    #[test]
    fn test_apply_worktree_flag_inserts_before_session_id() {
        // Claude Code：插入到 --session-id 之前。
        let mut args = vec!["--print".to_string(), "--session-id".to_string(), "abc".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Claudecode, true);
        assert_eq!(args, vec!["--print", "--worktree", "--session-id", "abc"]);

        // Hermes：插入到 --resume 之前。
        let mut args = vec!["-p".to_string(), "--resume".to_string(), "xyz".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Hermes, true);
        assert_eq!(args, vec!["-p", "--worktree", "--resume", "xyz"]);

        // 找不到 --session-id / --resume 时 append 到末尾。
        let mut args = vec!["-p".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Claudecode, true);
        assert_eq!(args, vec!["-p", "--worktree"]);

        // worktree_enabled = false 时不插入。
        let mut args = vec!["--print".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Claudecode, false);
        assert_eq!(args, vec!["--print"]);

        // 其他 executor 即使 worktree_enabled = true 也不插入。
        let mut args = vec!["--print".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Codex, true);
        assert_eq!(args, vec!["--print"]);
        apply_worktree_flag(&mut args, ExecutorType::Pi, true);
        assert_eq!(args, vec!["--print"]);
    }

    /// `apply_wall_clock_duration` 在 `usage=None` / `usage=Some(...)` 两种情况下都填 wall-clock。
    /// 这是 cancel / timeout / 自然退出三处共享的核心逻辑，必须保证不漂移。
    #[test]
    fn test_apply_wall_clock_duration_overrides_executor_report() {
        // Some 情况：override duration_ms，其他字段保留。
        let usage = ExecutionUsage {
            input_tokens: 10,
            output_tokens: 20,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            total_cost_usd: None,
            duration_ms: Some(999),
        };
        let start = std::time::Instant::now();
        // sleep 1ms 确保 wall_clock > 0（避免极端情况下 elapsed=0 被实现忽略）
        std::thread::sleep(std::time::Duration::from_millis(1));
        let updated = apply_wall_clock_duration(Some(usage.clone()), start).unwrap();
        assert_eq!(updated.input_tokens, 10);
        assert_eq!(updated.output_tokens, 20);
        // 关键断言：duration 一定是 wall-clock，不是 executor 报的 999。
        let wall = updated.duration_ms.unwrap();
        assert!(wall < 999, "wall-clock should override executor-reported 999");
        assert!(wall >= 1);

        // None 情况：构造全 0 + wall-clock 的占位 usage。
        let start2 = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let placeholder = apply_wall_clock_duration(None, start2).unwrap();
        assert_eq!(placeholder.input_tokens, 0);
        assert_eq!(placeholder.output_tokens, 0);
        assert!(placeholder.duration_ms.unwrap() >= 1);
    }

    // ─── issue #635 拆出的 helper 单测 ────────────────────────────────

    /// `build_executor_command` 必须：把 executable 作为 argv[0]、追加 args、设置 piped stdio。
    /// 工作目录仅在显式传 workspace 时设置；workspace=None 时 executor 沿用 daemon cwd。
    #[test]
    fn test_build_executor_command_basic_args() {
        let args = vec!["-p".to_string(), "hello".to_string()];
        let cmd = build_executor_command("/usr/bin/claude", &args, None);
        // std::process::Command 的 as_std() 可以拿到 std 命令，argv 在 std::process::Command 里是私有字段。
        // 这里用 tokio::process::Command 暴露的 std() 拿底层 std::process::Command。
        let std_cmd: &std::process::Command = cmd.as_std();
        let program = std_cmd.get_program();
        assert_eq!(program, "/usr/bin/claude");
        let std_args: Vec<&std::ffi::OsStr> = std_cmd.get_args().collect();
        assert_eq!(std_args.len(), 2);
        assert_eq!(std_args[0], "-p");
        assert_eq!(std_args[1], "hello");
    }

    /// `build_executor_command` 在传入 workspace 时应设置 current_dir，不传则保持 None。
    #[test]
    fn test_build_executor_command_workspace_sets_current_dir() {
        let args = vec!["-p".to_string()];
        let with_ws = build_executor_command("/bin/echo", &args, Some("/tmp/work"));
        assert_eq!(with_ws.as_std().get_current_dir().unwrap(), std::path::Path::new("/tmp/work"));

        let no_ws = build_executor_command("/bin/echo", &args, None);
        assert!(no_ws.as_std().get_current_dir().is_none());
    }

    /// `build_executor_command` 一定能构造出一个 Command 且 std 命令的 program/args 正确。
    /// stdio 是否 piped 没法直接断言（std::process::Command 没暴露 getter），只能通过
    /// `group_spawn()` 实际执行观察；但 program + args 的 getter 已经足以验证核心逻辑。
    #[test]
    fn test_build_executor_command_constructs_cleanly() {
        let args = vec!["-p".to_string(), "build me a web app".to_string()];
        let cmd = build_executor_command("/usr/local/bin/codex", &args, None);
        let std_cmd = cmd.as_std();
        assert_eq!(std_cmd.get_program(), "/usr/local/bin/codex");
        let std_args: Vec<&std::ffi::OsStr> = std_cmd.get_args().collect();
        assert_eq!(std_args.len(), 2);
        assert_eq!(std_args[1], "build me a web app");
    }

    /// `format_timeout_secs` 在 60 秒以下的边界值要正确。
    /// 0 表示无超时，1 秒显示 "0 min 1 sec"，59 秒同理。
    #[test]
    fn test_format_timeout_secs_edges_under_minute() {
        assert_eq!(format_timeout_secs(0), "0 min");
        assert_eq!(format_timeout_secs(1), "0 min 1 sec");
        assert_eq!(format_timeout_secs(59), "0 min 59 sec");
    }

    /// `format_timeout_secs` 在"恰好为 60 的整数倍"上不应带 "sec" 后缀。
    /// 例如 60 秒 = "1 min"，120 秒 = "2 min"，这是因为 remaining_secs == 0 时
    /// 直接走 hours/days 分支。
    #[test]
    fn test_format_timeout_secs_exact_minutes() {
        assert_eq!(format_timeout_secs(60), "1 min");
        assert_eq!(format_timeout_secs(120), "2 min");
        assert_eq!(format_timeout_secs(3540), "59 min");
        // 60 min 是 3600 秒，进 hour 分支。
        assert_eq!(format_timeout_secs(3600), "1 hour(s)");
    }

    // ====== issue #643: WorktreeContext 辅助函数 ======

    /// `cleanup_worktree_if_needed` 在 `auto_cleanup = false` 时不做任何事。
    /// 用 record_path 指向一个不存在的路径验证：即使路径无效也不会调用 svc，
    /// 也就不会触发任何 warn 日志或失败。
    #[test]
    fn test_cleanup_worktree_if_needed_disabled() {
        let ctx = WorktreeContext {
            effective_workspace: None,
            record_path: Some("/tmp/ntd-643-disabled".into()),
            auto_cleanup: false,
        };
        // 不应 panic，不应触发任何 git 调用
        cleanup_worktree_if_needed(&ctx);
    }

    /// `cleanup_worktree_if_needed` 在 `auto_cleanup = true` 但 record_path 为 None 时
    /// 同样直接返回。这种情况理论上不会出现（auto_cleanup 开启一定会有 record_path），
    /// 但作为防御性测试可以确认不会空跑。
    #[test]
    fn test_cleanup_worktree_if_needed_no_path() {
        let ctx = WorktreeContext {
            effective_workspace: None,
            record_path: None,
            auto_cleanup: true,
        };
        cleanup_worktree_if_needed(&ctx);
    }

    /// `WorktreeContext::default()` 三个字段都是 "未启用 worktree" 的初始值。
    /// 验证整个 resolve 链上的"早退"分支共用同一个默认值。
    #[test]
    fn test_worktree_context_default_is_disabled() {
        let ctx = WorktreeContext::default();
        assert!(ctx.effective_workspace.is_none());
        assert!(ctx.record_path.is_none());
        assert!(!ctx.auto_cleanup);
    }
}
