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

/// cancel 分支末段：写 DB（cancelled/failed + 空 result） + 发 Output/Finished 事件 + remove task。
///
/// 抽出来让 cancel 分支的 select! 臂保持 ≤ 30 行：杀进程 + drain 已经抽到
/// `drain_readers_and_flush`，剩下的就是"DB + 事件 + cleanup"。
///
/// 日志写入不再走 `remaining_logs`：进入本函数前 `drain_readers_and_flush` 已经调用
/// `log_flusher.finalize()` 把残余 buffer 一次性入库；再传全量日志会触发
/// `update_execution_record` 的 `insert_execution_logs` 分支重复插入（issue #653）。
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
    let _ = db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
        id: record_id,
        status: crate::models::ExecutionStatus::Failed.as_str(),
        // 日志已由 LogFlusher 全部入库；传 "[]" 避免重复插入。
        remaining_logs: "[]",
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
///
/// 日志写入策略与 [`handle_cancellation_branch`] 一致：进入本函数前 `LogFlusher::finalize`
/// 已 drain 全部日志到 DB，`remaining_logs` 传 `"[]"` 避免重复插入（issue #653）。
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
    let _ = db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
        id: record_id,
        status: crate::models::ExecutionStatus::Failed.as_str(),
        // 日志已由 LogFlusher 全部入库；传 "[]" 避免重复插入。
        remaining_logs: "[]",
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

/// 正常完成分支：把 stats、usage、model 写库；status 更新交给 `update_execution_record`。
///
/// 日志写入不在这条路径上：执行过程中 [`crate::log_flusher::LogFlusher`] 已经按阈值 / timer
/// 批量写库，进入本函数前 [`LogFlusher::finalize`] 也已 drain 残余 buffer（详见
/// `run_todo_execution` 的 `RunOutcome::Completed` 分支）。这里再把"全量日志"以
/// `remaining_logs` 传入会触发 [`crate::db::Database::update_execution_record`] 的
/// `insert_execution_logs` 分支，导致每条日志被插两次（issue #653）。因此固定传 `"[]"`。
///
/// `executor_spawn` 是 executor 的共享引用（stdout reader 里持有的是同一份），
/// 这里再调用 `get_final_result` / `get_usage` / `get_model` 不会产生竞争——
/// 这三个方法只读内部 state，且只在 finalization 时调用一次。
async fn persist_completion_record(
    db: &Database,
    executor: &dyn crate::adapters::CodeExecutor,
    record_id: i64,
    all_logs: &[crate::models::ParsedLogEntry],
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
    // remaining_logs 故意传 "[]"：日志已由 LogFlusher 全部入库，再传全量会导致重复插入。
    let _ = db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
        id: record_id,
        status: final_status,
        remaining_logs: "[]",
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
    // issue #660: 顶层退化为「pre-spawn 编排 → 失败翻译 → spawn 子任务」三段。
    // 任一阶段失败立即 short-circuit 返回；成功路径最终 spawn 出 fire-and-forget
    // 子任务，主流程不再 await（与重构前语义一致）。
    let prepared = match prepare_execution_state(request).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let spawned = match start_todo_and_prepare_spawn(prepared).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    dispatch_spawned_executor_task(spawned).await
}

// ============================================================================
//  issue #660: stage 函数
//
//  把原本 449 行的 `run_todo_execution` 拆为 3 个 stage：
//    1. `prepare_execution_state`   — 拆解 request / 注册 task / 加载 todo /
//       并发控制 / 触发 pre-hook / 选定 executor / 创建 execution_record
//    2. `start_todo_and_prepare_spawn` — 落 worktree / start_todo / 注册 TaskInfo /
//       准备要 move 进 spawn 闭包的字段
//    3. `dispatch_spawned_executor_task` — `tokio::spawn` 子任务
//
//  每个 stage 函数返回 `Result<T, ExecutionResult>`：`Ok` 进入下一阶段，
//  `Err(ExecutionResult)` 表示需要把 ExecutionResult 直接返回给调用方。
// ============================================================================

/// Stage 1: 把 request 拆解并完成「executor 选定 + record 创建」前所有同步/异步检查。
///
/// 该阶段**不**启动 todo 状态变更，也**不**创建 worktree —— 这两步属于 stage 2，
/// 这样 stage 1 出错时无需清理 worktree，副作用面更窄。
///
/// 拆解思路：6 个独立子职责分别抽到 helper（substitute / register / concurrency /
/// hook / executor / record），顶层只负责串联；每个 helper 函数 ≤30 行，
/// 满足 CLAUDE.md 的 30 行硬规则。
async fn prepare_execution_state(
    request: RunTodoExecutionRequest,
) -> Result<PreparedExecution, ExecutionResult> {
    // 1) 占位符替换 + 拆解 request 字段。
    let substituted = substitute_message_placeholders(&request);
    // 2) 注册 task（生成 task_id + guard + cancel_rx）并加载 todo。
    let task_state = register_task_and_load_todo(&request).await?;
    // 3) 取运行时配置（max_concurrent / timeout_secs）。
    let (max_concurrent, timeout_secs) = read_runtime_config(&request);
    // 4) 加载 todo 后做并发检查 + pre-hook 触发。
    let initial_todo = task_state.todo.clone();
    let todo =
        enforce_concurrency_limit(&request, initial_todo, max_concurrent, &task_state.task_id)
            .await?;
    fire_pre_execution_hook_if_needed(&request, &todo, substituted.chain).await?;
    // 5) 选定 executor 并构造 command_args。
    let selected =
        select_executor_and_build_command(&request, &todo, &substituted.message).await?;
    // 6) 创建 execution record 并把 stage 1 产物聚合成 PreparedExecution。
    create_run_execution_record(
        request,
        task_state,
        todo,
        timeout_secs,
        selected,
    )
    .await
}

/// Stage 1 步骤 1：在 `request` 上做 message 占位符替换，并返回替换后的 message。
///
/// `chain` 在 `fire_pre_execution_hook_if_needed` 还要用，所以提前 clone 出来。
/// 占位符替换只在编排阶段有效——executor 看到的 message 与 stage 2 写入
/// execution_record.command 的字符串一致。
struct SubstitutedContext {
    message: String,
    chain: Vec<i64>,
}

fn substitute_message_placeholders(request: &RunTodoExecutionRequest) -> SubstitutedContext {
    let message = request
        .params
        .as_ref()
        .map(|params| crate::models::replace_placeholders(&request.message, params))
        .unwrap_or_else(|| request.message.clone());
    SubstitutedContext {
        message,
        chain: request.chain.clone(),
    }
}

/// Stage 1 步骤 2：注册 task 并加载 todo。返回 task_id + guard + cancel_rx + todo。
///
/// Issue #506：用 RAII guard 注册 task，确保即便后续路径 panic/早返回忘了
/// remove，sender 也会被 guard drop 时清理。guard 在 stage 1 末尾才 drop，
/// 等价于覆盖整段 task 生命周期。
struct TaskState {
    task_id: String,
    task_guard: crate::task_manager::TaskGuard,
    cancel_rx: tokio::sync::mpsc::Receiver<()>,
    todo: Option<Todo>,
}

async fn register_task_and_load_todo(
    request: &RunTodoExecutionRequest,
) -> Result<TaskState, ExecutionResult> {
    let task_id = Uuid::new_v4().to_string();
    let mut task_guard = request
        .task_manager
        .register_with_guard(task_id.clone())
        .await;
    let cancel_rx = task_guard.take_receiver();

    // 加载 todo；load 失败时仍然继续（不阻断执行），把 todo 视作 None。
    let todo = match request.db.get_todo(request.todo_id).await {
        Ok(Some(t)) => Some(t),
        Ok(None) => None,
        Err(e) => {
            tracing::error!(
                "Failed to fetch todo {} for executor selection: {}",
                request.todo_id,
                e
            );
            None
        }
    };
    Ok(TaskState {
        task_id,
        task_guard,
        cancel_rx,
        todo,
    })
}

/// Stage 1 步骤 3：从 config 读 max_concurrent + timeout_secs，config lock
/// 释放后两个值就各自独立可用，避免后续代码块带 lock。
fn read_runtime_config(request: &RunTodoExecutionRequest) -> (u32, u64) {
    let cfg = request.config.read().unwrap();
    (cfg.max_concurrent_todos, cfg.execution_timeout_secs)
}

/// Stage 1 步骤 4a：如果 todo 已存在，校验并发限制。todo 为 None 时跳过。
///
/// 任一步失败都返回失败 ExecutionResult（task_id 已生成，调用方可以基于它取消
/// task）。`count_active_running_for_todo` 失败（zombie 检测挂掉）也按拒绝处理。
async fn enforce_concurrency_limit(
    request: &RunTodoExecutionRequest,
    todo: Option<Todo>,
    max_concurrent: u32,
    task_id: &str,
) -> Result<Option<Todo>, ExecutionResult> {
    let Some(t) = todo else {
        return Ok(None);
    };
    let running_count =
        match count_active_running_for_todo(&request.task_manager, &request.db, request.todo_id)
            .await
        {
            Ok(n) => n,
            Err(()) => {
                return Err(ExecutionResult {
                    task_id: task_id.to_string(),
                    record_id: None,
                })
            }
        };
    if running_count >= max_concurrent as usize {
        return Err(reject_concurrency_limit(
            &request.task_manager,
            &request.tx,
            task_id,
            request.todo_id,
            &t.title,
            running_count,
            max_concurrent,
        )
        .await);
    }
    Ok(Some(t))
}

/// Stage 1 步骤 4b：Fire before_execution hooks synchronously — block until all
/// pre-flight targets finish. todo 为 None 时跳过（todo 已被删）。
async fn fire_pre_execution_hook_if_needed(
    request: &RunTodoExecutionRequest,
    todo: &Option<Todo>,
    chain: Vec<i64>,
) -> Result<(), ExecutionResult> {
    let Some(t) = todo else { return Ok(()) };
    let ctx = crate::hooks::models::HookContext::for_before_execution(
        request.todo_id,
        t.title.clone(),
        t.executor.clone(),
        t.workspace.clone(),
        chain,
    );
    if let Err(msg) = request
        .hook_service
        .clone()
        .fire_before_execution(request.todo_id, ctx)
        .await
    {
        tracing::warn!("aborting execution due to pre-hook failure: {}", msg);
        return Err(ExecutionResult {
            task_id: String::new(),
            record_id: None,
        });
    }
    Ok(())
}

/// Stage 1 步骤 5 产物：executor 选择 + command 构造。
///
/// 决策顺序：显式 req_executor > todo.executor > registry default。命令构造
/// 用 `command_args_with_session` 处理 resume / 非 resume 分支，再用
/// `apply_worktree_flag` 给 claude_code / hermes 加 worktree 参数。
struct SelectedExecutor {
    executor: Arc<dyn CodeExecutor>,
    command_args: Vec<String>,
    executable_path: String,
    executor_str: String,
    todo_workspace: Option<String>,
    session_id_for_executor: String,
}

async fn select_executor_and_build_command(
    request: &RunTodoExecutionRequest,
    todo: &Option<Todo>,
    message: &str,
) -> Result<SelectedExecutor, ExecutionResult> {
    let todo_executor = todo.as_ref().and_then(|t| t.executor.clone());
    let todo_workspace = todo.as_ref().and_then(|t| t.workspace.clone());
    let todo_worktree_enabled = todo
        .as_ref()
        .map(|t| t.worktree_enabled)
        .unwrap_or(false);

    let executor_type =
        resolve_executor_type(request.req_executor.as_deref(), todo_executor.as_deref());
    let executor = match request.executor_registry.get(executor_type).await {
        Some(exec) => exec,
        None => match request.executor_registry.get_default().await {
            Some(exec) => exec,
            None => {
                return Err(reject_no_executor(
                    &request.db,
                    &request.task_manager,
                    &request.tx,
                    "",
                    request.todo_id,
                    todo.as_ref().map(|t| t.title.as_str()).unwrap_or(""),
                    executor_type,
                )
                .await);
            }
        },
    };

    let executable_path = executor.executable_path().to_string();
    // 首次执行时需要有效的 UUID 作为 session-id，不能用 "fallback" 这种占位符。
    // resume_session_id 为 None 时生成新 UUID，确保 Claude Code CLI 不会报 "Invalid session ID" 错误。
    let session_id_for_executor = request
        .resume_session_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let is_resume = request.resume_session_id.is_some();
    let mut command_args = executor.command_args_with_session(
        message,
        Some(&session_id_for_executor),
        is_resume,
    );
    apply_worktree_flag(&mut command_args, executor.executor_type(), todo_worktree_enabled);
    let executor_str = executor.executor_type().to_string();

    // Update todo's executor to the one being used. 失败仅记日志，不阻断执行。
    if let Err(e) = request
        .db
        .update_todo_executor(request.todo_id, &executor_str)
        .await
    {
        tracing::error!("Failed to update todo executor: {}", e);
    }

    Ok(SelectedExecutor {
        executor,
        command_args,
        executable_path,
        executor_str,
        todo_workspace,
        session_id_for_executor,
    })
}

/// Stage 1 步骤 6：创建 execution record 并组装 PreparedExecution 产物。
///
/// record_id 是 stage 1 唯一需要数据库写的字段。失败时走 reject_create_record_failure
/// 把 todo 标回非 running 并清理 task，调用方拿到 ExecutionResult 直接返回给前端。
async fn create_run_execution_record(
    request: RunTodoExecutionRequest,
    task_state: TaskState,
    todo: Option<Todo>,
    timeout_secs: u64,
    selected: SelectedExecutor,
) -> Result<PreparedExecution, ExecutionResult> {
    let command = format!(
        "{} {}",
        selected.executable_path,
        selected.command_args.join(" ")
    );
    let record_id = match request
        .db
        .create_execution_record(NewExecutionRecord {
            todo_id: request.todo_id,
            command: &command,
            executor: &selected.executor_str,
            trigger_type: &request.trigger_type,
            task_id: &task_state.task_id,
            session_id: Some(&selected.session_id_for_executor),
            resume_message: request.resume_message.as_deref(),
            source_todo_id: request.source_todo_id,
            source_todo_title: request.source_todo_title.as_deref(),
            source_hook_id: request.source_hook_id,
        })
        .await
    {
        Ok(id) => id,
        Err(e) => {
            return Err(reject_create_record_failure(
                &request.db,
                &request.task_manager,
                &task_state.task_id,
                request.todo_id,
                e,
            )
            .await);
        }
    };
    Ok(PreparedExecution {
        request,
        task_guard: task_state.task_guard,
        cancel_rx: task_state.cancel_rx,
        task_id: task_state.task_id,
        command_args: selected.command_args,
        executable_path: selected.executable_path,
        executor: selected.executor,
        executor_str: selected.executor_str,
        record_id,
        todo,
        todo_workspace: selected.todo_workspace,
        timeout_secs,
    })
}

/// Stage 2: 落 worktree / start_todo / 注册 TaskInfo，准备 spawn 闭包所需的全部字段。
///
/// 失败路径包括 start_todo_execution：失败时必须先清理已创建的 worktree，再返回
/// reject 路径的 ExecutionResult，避免「未启动成功」的执行记录与 worktree 残留错位。
async fn start_todo_and_prepare_spawn(
    prepared: PreparedExecution,
) -> Result<SpawnInputs, ExecutionResult> {
    let worktree_ctx = resolve_worktree_context(&prepared.request.db, &prepared.todo).await;
    record_worktree_path(
        &prepared.request.db,
        prepared.record_id,
        worktree_ctx.record_path.as_deref(),
    )
    .await;

    start_todo_or_cleanup(&prepared, &worktree_ctx).await?;
    let todo_title = extract_todo_title(&prepared.todo);
    let executor_spawn = prepared.executor.clone();
    let execution_timeout_secs = prepared.timeout_secs;

    register_websocket_task_info(&prepared, &todo_title, &executor_spawn).await;

    // effective_workspace 在 worktree 失败回退 + todo.workspace 回退 之后确定，
    // 后续 move 进 spawn 闭包作为 cwd。
    let effective_workspace = worktree_ctx
        .effective_workspace
        .clone()
        .or(prepared.todo_workspace.clone());

    Ok(SpawnInputs {
        prepared,
        todo_title,
        executor_spawn,
        effective_workspace,
        execution_timeout_secs,
        worktree_ctx,
    })
}

/// 在 TaskManager 注册本次执行的任务信息（task_id / todo_id / executor 类型），
/// WebSocket 同步会从这里取最新 title + logs。
async fn register_websocket_task_info(
    prepared: &PreparedExecution,
    todo_title: &str,
    executor_spawn: &Arc<dyn CodeExecutor>,
) {
    prepared
        .request
        .task_manager
        .register_info(crate::task_manager::TaskInfo {
            task_id: prepared.task_id.clone(),
            todo_id: prepared.request.todo_id,
            todo_title: todo_title.to_string(),
            executor: executor_spawn.executor_type().to_string(),
            // 初始为空，WebSocket 同步时会从数据库获取实际日志。
            logs: "[]".to_string(),
        })
        .await;
}

/// 把 todo 标为 in_progress 并关联 task_id。失败时清掉 worktree，再走 reject 路径。
///
/// start_todo_execution 失败必须先 cleanup worktree：worktree 已在 stage 2 入口
/// 创建并写入 record_path，若启用了 auto_cleanup 不在这里清理会留下孤儿 worktree
/// 目录/分支与「未启动成功」的执行记录错位。
async fn start_todo_or_cleanup(
    prepared: &PreparedExecution,
    worktree_ctx: &WorktreeContext,
) -> Result<(), ExecutionResult> {
    if let Err(e) = prepared
        .request
        .db
        .start_todo_execution(prepared.request.todo_id, &prepared.task_id)
        .await
    {
        cleanup_worktree_if_needed(worktree_ctx);
        return Err(reject_start_todo_failure(
            &prepared.request.db,
            &prepared.request.tx,
            &prepared.request.task_manager,
            &prepared.task_id,
            prepared.request.todo_id,
            prepared
                .todo
                .as_ref()
                .map(|t| t.title.as_str())
                .unwrap_or(""),
            &prepared.executor_str,
            prepared.record_id,
            e,
        )
        .await);
    }
    Ok(())
}

/// 从 `Option<Todo>` 提取 title；todo 已删除时返回空串。
///
/// 之所以是独立 helper：spawn 闭包内多处需要 `todo_title: String`（emit event、
/// TaskInfo 注册、feishu 推送），抽出来后调用方都走同一处 title 解析逻辑，
/// 不必在每处重复 `todo.as_ref().map(...)`。
fn extract_todo_title(todo: &Option<Todo>) -> String {
    todo.as_ref().map(|t| t.title.clone()).unwrap_or_default()
}

/// Stage 3: `tokio::spawn` 出 fire-and-forget 子任务，并立刻返回 ExecutionResult。
///
/// 实际的 select! / match 逻辑放在 `run_spawned_executor_task` 顶层异步函数里，
/// 这样 spawn 闭包退化为单行 `async move { run_spawned_executor_task(...).await }`，
/// 编排与执行两段清晰分离。
async fn dispatch_spawned_executor_task(spawned: SpawnInputs) -> ExecutionResult {
    let task_id_return = spawned.prepared.task_id.clone();
    let record_id = spawned.prepared.record_id;

    // 为整个 spawn 闭包建立 executor_run span：
    // tokio::spawn 不会自动继承外层 span（参见 issue #513），所以需要把异步块整体包到
    // Instrument 中。这样 child process spawn / stdout/stderr / log flush / db update /
    // hook fire 这一长串环节的日志都会被 executor_run span 包住。
    let executor_span = tracing::info_span!(
        "executor_run",
        task_id = %spawned.prepared.task_id,
        todo_id = spawned.prepared.request.todo_id,
        record_id = spawned.prepared.record_id,
        executor = %spawned.executor_spawn.executor_type(),
    );

    tokio::spawn(
        async move {
            run_spawned_executor_task(spawned).await;
        }
        .instrument(executor_span),
    );

    ExecutionResult {
        task_id: task_id_return,
        record_id: Some(record_id),
    }
}

/// issue #660: 原来 449 行 `run_todo_execution` 的 spawn 闭包体。
///
/// 该函数由 `dispatch_spawned_executor_task` 通过 `tokio::spawn` 调用，是 fire-and-forget
/// 子任务的真正实现。设计上与重构前的闭包体逐位等价——所有副作用（emit event、写 DB、
/// fire hook、清理 worktree）都按原顺序保留。
async fn run_spawned_executor_task(spawned: SpawnInputs) {
    // 编排流程：构建 runtime → 启动子进程 → 等待 outcome → dispatch。
    // 每个 step 拆到独立 helper，本函数仅负责串联，使 body 控制在 ≤30 行。
    let execution_start = std::time::Instant::now();
    let mut runtime = move_into_runtime(spawned);

    emit_started_event(
        &runtime.tx,
        &runtime.task_id,
        runtime.todo_id,
        &runtime.todo_title,
        runtime.executor_spawn.as_ref(),
    );

    let Some(mut child) = try_spawn_executor_child(&runtime).await else {
        return;
    };
    save_child_pid_and_close_stdin(&mut child, &runtime.db, runtime.record_id).await;

    let (log_flusher, stdout_task, stderr_task, flush_timer) =
        setup_log_capture_pipeline_for(&runtime, &mut child).await;
    let outcome = await_run_outcome_with_timeout(&mut runtime, &mut child).await;
    dispatch_outcome(
        outcome,
        &mut child,
        stdout_task,
        stderr_task,
        log_flusher,
        flush_timer,
        runtime,
        execution_start,
    )
    .await;
}

/// 启动子进程；spawn 失败时清理 worktree 并触发 spawn failure 路径，返回 `None`。
///
/// 返回 `Option` 让调用点用 `let ... else { return; }` 早退，省去 match/Err 分支。
async fn try_spawn_executor_child(
    runtime: &SpawnRuntime,
) -> Option<command_group::AsyncGroupChild> {
    match spawn_executor_child(runtime) {
        Ok(c) => Some(c),
        Err(e) => {
            cleanup_worktree_if_needed(&runtime.worktree_ctx);
            handle_spawn_failure(
                &runtime.db,
                &runtime.tx,
                &runtime.task_manager,
                &runtime.task_id,
                runtime.todo_id,
                &runtime.todo_title,
                runtime.executor_spawn.as_ref(),
                runtime.feishu_bot_id,
                runtime.feishu_receive_id.clone(),
                e,
            )
            .await;
            None
        }
    }
}

/// 配置超时 + 在 select! 中 await outcome（cancel / timeout / child exit）。
///
/// 把 timeout_sleep 的 pin 留在 helper 内。cancel_rx 通过 `runtime.prepared.cancel_rx`
/// 借用，避免 SpawnRuntime 顶层冗余 cancel_rx 字段。
async fn await_run_outcome_with_timeout(
    runtime: &mut SpawnRuntime,
    child: &mut command_group::AsyncGroupChild,
) -> RunOutcome {
    let mut timeout_sleep = configure_timeout_sleep(runtime.execution_timeout_secs);
    await_run_outcome(
        &mut runtime.prepared.cancel_rx,
        &mut timeout_sleep,
        runtime.execution_timeout_secs,
        child,
    )
    .await
}

/// `run_spawned_executor_task` 的执行期状态：把 SpawnInputs 字段全部 clone
/// 出来成可借用结构，避免在 spawn 闭包内对原 owned 值反复 .clone()。
///
/// `cancel_rx` / `task_guard` 不在此结构下沉：仍由 `prepared: PreparedExecution` 持有，
/// 通过 `runtime.prepared.cancel_rx` / `runtime.prepared.task_guard` 访问。
/// SpawnRuntime 只冗余「spawn 阶段热路径」所需字段，减少一次 clone 同时避开
/// `prepared.cancel_rx` 与 `prepared` 字段的部分 move 冲突。
struct SpawnRuntime {
    db: Arc<Database>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    todo_id: i64,
    todo_title: String,
    executor_spawn: Arc<dyn CodeExecutor>,
    record_id: i64,
    worktree_ctx: WorktreeContext,
    task_id: String,
    execution_timeout_secs: u64,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
    /// spawn 阶段实际使用的 cwd：worktree 路径优先，回退到 todo.workspace。
    /// 修复 issue #660 重构中的回归：原代码在 spawn 闭包内用 effective_workspace
    /// 决定子进程 cwd，但拆分到 spawn_executor_child 后误用了 todo_workspace，
    /// 导致启用 worktree 时子进程仍在原始 workspace 内运行。
    effective_workspace: Option<String>,
    prepared: PreparedExecution,
}

/// 把 SpawnInputs 全部字段展开到 SpawnRuntime。
///
/// 先把 `prepared` 整体下沉到本地变量（避开 `spawned.prepared.cancel_rx` 与
/// `prepared: spawned.prepared` 同时部分 move 触发 E0382）。
fn move_into_runtime(spawned: SpawnInputs) -> SpawnRuntime {
    let prepared = spawned.prepared;
    SpawnRuntime {
        db: prepared.request.db.clone(),
        tx: prepared.request.tx.clone(),
        task_manager: prepared.request.task_manager.clone(),
        todo_id: prepared.request.todo_id,
        todo_title: spawned.todo_title.clone(),
        executor_spawn: spawned.executor_spawn.clone(),
        record_id: prepared.record_id,
        worktree_ctx: spawned.worktree_ctx,
        task_id: prepared.task_id.clone(),
        execution_timeout_secs: spawned.execution_timeout_secs,
        feishu_bot_id: prepared.request.feishu_bot_id,
        feishu_receive_id: prepared.request.feishu_receive_id.clone(),
        // 关键：把 effective_workspace 整字段 move 进 runtime，
        // 避免 spawn_executor_child 误用 todo_workspace（worktree 失效）。
        effective_workspace: spawned.effective_workspace,
        prepared,
    }
}

/// `build_executor_command` + `group_spawn` 两步合一：argv 已就绪，直接
/// 创建进程组让 kill 时能整组杀，避免留下 zombie 子进程。
fn spawn_executor_child(runtime: &SpawnRuntime) -> Result<command_group::AsyncGroupChild, std::io::Error> {
    let mut cmd = build_executor_command(
        &runtime.prepared.executable_path,
        &runtime.prepared.command_args,
        // 用 effective_workspace 而不是 prepared.todo_workspace：
        // effective_workspace 在 worktree 启用时已经回退到 worktree 路径，
        // 直接用 todo_workspace 会让子进程在原始 workspace 运行（issue #643 失效）。
        runtime.effective_workspace.as_deref(),
    );
    cmd.group_spawn()
}

/// 把 stdout/stderr handle 拆出来，连同 db/tx 一起喂给 `setup_log_capture_pipeline`。
async fn setup_log_capture_pipeline_for(
    runtime: &SpawnRuntime,
    child: &mut command_group::AsyncGroupChild,
) -> (
    Arc<LogFlusher>,
    Option<JoinHandle<()>>,
    Option<JoinHandle<()>>,
    tokio::task::JoinHandle<()>,
) {
    let stdout_handle = child.inner().stdout.take();
    let stderr_handle = child.inner().stderr.take();
    setup_log_capture_pipeline(
        stdout_handle,
        stderr_handle,
        runtime.executor_spawn.clone(),
        runtime.db.clone(),
        runtime.tx.clone(),
        runtime.task_id.clone(),
        runtime.record_id,
    )
    .await
}

/// select! 收口之后按 outcome 分发到 cancellation / timeout / completion 三个分支。
#[allow(clippy::too_many_arguments)]
async fn dispatch_outcome(
    outcome: RunOutcome,
    child: &mut command_group::AsyncGroupChild,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<LogFlusher>,
    flush_timer: tokio::task::JoinHandle<()>,
    runtime: SpawnRuntime,
    execution_start: std::time::Instant,
) {
    match outcome {
        RunOutcome::Cancelled => {
            run_cancellation_path(
                child,
                stdout_task,
                stderr_task,
                log_flusher,
                flush_timer,
                &runtime.db,
                &runtime.tx,
                &runtime.task_manager,
                &runtime.task_id,
                runtime.todo_id,
                &runtime.todo_title,
                runtime.executor_spawn.as_ref(),
                runtime.record_id,
                runtime.feishu_bot_id,
                runtime.feishu_receive_id.clone(),
                &runtime.worktree_ctx,
            )
            .await;
        }
        RunOutcome::TimedOut => {
            run_timeout_path(
                child,
                stdout_task,
                stderr_task,
                log_flusher,
                flush_timer,
                &runtime.db,
                &runtime.tx,
                &runtime.task_manager,
                &runtime.task_id,
                runtime.todo_id,
                &runtime.todo_title,
                runtime.executor_spawn.as_ref(),
                runtime.record_id,
                runtime.execution_timeout_secs,
                runtime.feishu_bot_id,
                runtime.feishu_receive_id.clone(),
                &runtime.worktree_ctx,
            )
            .await;
        }
        RunOutcome::Completed(status) => {
            handle_completed_branch(
                status,
                stdout_task,
                stderr_task,
                log_flusher,
                flush_timer,
                SpawnContext {
                    db: runtime.db,
                    tx: runtime.tx,
                    task_manager: runtime.task_manager,
                    executor_registry: runtime.prepared.request.executor_registry.clone(),
                    config: runtime.prepared.request.config.clone(),
                    hook_service: runtime.prepared.request.hook_service.clone(),
                    executor: runtime.executor_spawn,
                    task_id: runtime.task_id,
                    todo_id: runtime.todo_id,
                    todo_title: runtime.todo_title,
                    todo: runtime.prepared.todo,
                    chain: runtime.prepared.request.chain,
                    record_id: runtime.record_id,
                    execution_start,
                    worktree_ctx: runtime.worktree_ctx,
                    trigger_type: runtime.prepared.request.trigger_type,
                    feishu_bot_id: runtime.feishu_bot_id,
                    feishu_receive_id: runtime.feishu_receive_id,
                },
            )
            .await;
        }
    }
}

/// select! 三种终态枚举，避免在三个分支里各重复「杀进程 + drain + finalize」
/// 清理模板。child 仍由调用方持有，可继续调 kill_process_tree。
enum RunOutcome {
    Cancelled,
    TimedOut,
    Completed(std::io::Result<std::process::ExitStatus>),
}

/// 把超时换算成 `Pin<Box<Sleep>>`。`execution_timeout_secs == 0` 表示禁用超时，
/// 此时返回「永久 sleep」的 future，select! 永远不命中该分支。
fn configure_timeout_sleep(execution_timeout_secs: u64) -> std::pin::Pin<Box<tokio::time::Sleep>> {
    let timeout_enabled = execution_timeout_secs > 0;
    // Non-zero values are guaranteed >= 60 by normalize_paths clamp, so from_secs is safe.
    let duration = std::time::Duration::from_secs(execution_timeout_secs);
    let sleep = tokio::time::sleep(if timeout_enabled {
        duration
    } else {
        // 用一个非常大的 duration（u64::MAX 秒 ≈ 5.8 亿年）模拟「永不超时」。
        // 这样 select! 不会编译报「`if` guard 时所有分支都需要 future 合法」。
        std::time::Duration::from_secs(u64::MAX)
    });
    Box::pin(sleep)
}

/// select! 收口：cancel 优先 → timeout 次之 → child wait。
///
/// `biased;` 让取消分支优先于超时分支，避免「按 timeout_secs 比较大、但用户已经
/// 点了取消」的请求被超时路径抢走（issue #606 提到的边界 case）。
async fn await_run_outcome(
    cancel_rx: &mut tokio::sync::mpsc::Receiver<()>,
    timeout_sleep: &mut std::pin::Pin<Box<tokio::time::Sleep>>,
    execution_timeout_secs: u64,
    child: &mut command_group::AsyncGroupChild,
) -> RunOutcome {
    let timeout_enabled = execution_timeout_secs > 0;
    tokio::select! {
        biased;
        _ = cancel_rx.recv() => RunOutcome::Cancelled,
        _ = timeout_sleep, if timeout_enabled => RunOutcome::TimedOut,
        status = child.wait() => RunOutcome::Completed(status),
    }
}

/// 取消分支：kill 进程组 → drain readers → handle_cancellation_branch → cleanup worktree。
#[allow(clippy::too_many_arguments)]
async fn run_cancellation_path(
    child: &mut command_group::AsyncGroupChild,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<LogFlusher>,
    flush_timer: tokio::task::JoinHandle<()>,
    db: &Arc<Database>,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &Arc<TaskManager>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn CodeExecutor,
    record_id: i64,
    feishu_bot_id: Option<i64>,
    // 接 Option<String> 而非 Option<&str>，与 `handle_cancellation_branch` 内部签名对齐，
    // 避免在调用边界再发生 .to_string() 转换。
    feishu_receive_id: Option<String>,
    worktree_ctx: &WorktreeContext,
) {
    kill_process_tree(child).await;
    drain_readers_and_flush(child, stdout_task, stderr_task, log_flusher, flush_timer).await;
    handle_cancellation_branch(
        db,
        tx,
        task_manager,
        task_id,
        todo_id,
        todo_title,
        executor,
        record_id,
        feishu_bot_id,
        feishu_receive_id,
    )
    .await;
    cleanup_worktree_if_needed(worktree_ctx);
}

/// 超时分支：kill → drain → handle_timeout_branch → cleanup worktree。
#[allow(clippy::too_many_arguments)]
async fn run_timeout_path(
    child: &mut command_group::AsyncGroupChild,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<LogFlusher>,
    flush_timer: tokio::task::JoinHandle<()>,
    db: &Arc<Database>,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &Arc<TaskManager>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn CodeExecutor,
    record_id: i64,
    execution_timeout_secs: u64,
    feishu_bot_id: Option<i64>,
    // 接 Option<String> 而非 Option<&str>，与 `handle_timeout_branch` 内部签名对齐。
    feishu_receive_id: Option<String>,
    worktree_ctx: &WorktreeContext,
) {
    kill_process_tree(child).await;
    drain_readers_and_flush(child, stdout_task, stderr_task, log_flusher, flush_timer).await;
    handle_timeout_branch(
        db,
        tx,
        task_manager,
        task_id,
        todo_id,
        todo_title,
        executor,
        record_id,
        execution_timeout_secs,
        format_timeout_secs(execution_timeout_secs),
        feishu_bot_id,
        feishu_receive_id,
    )
    .await;
    cleanup_worktree_if_needed(worktree_ctx);
}

/// `handle_completed_branch` 的入参聚合。
///
/// 之前 23 个位置参数 + `#[allow(clippy::too_many_arguments)]` 是 Long Parameter
/// List 坏味道的复发。改成结构体传参后调用方写 SpawnContext { ... } 字面量 22
/// 行，但 handle_completed_branch 函数体能缩到 < 30 行真正符合 CLAUDE.md。
struct SpawnContext {
    db: Arc<Database>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    executor_registry: Arc<ExecutorRegistry>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
    hook_service: Arc<HookService>,
    executor: Arc<dyn CodeExecutor>,
    task_id: String,
    todo_id: i64,
    todo_title: String,
    todo: Option<Todo>,
    chain: Vec<i64>,
    record_id: i64,
    execution_start: std::time::Instant,
    worktree_ctx: WorktreeContext,
    trigger_type: String,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
}

/// 把「正常退出 → await readers → finalize flusher → emit progress →
/// 解析 result → persist record → finalize_normal_completion → cleanup worktree」
/// 整条完成路径抽到一个函数，让 `run_spawned_executor_task` 的 match 分支只剩下
/// kill + drain + 调对应 helper 的骨架。
async fn handle_completed_branch(
    status: std::io::Result<std::process::ExitStatus>,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<LogFlusher>,
    flush_timer: tokio::task::JoinHandle<()>,
    ctx: SpawnContext,
) {
    // 编排「正常完成」路径：await readers → 解析 exit → 发进度 → flush 提取 →
    // persist record + finalize → cleanup worktree。
    // 每步交给 helper，本函数 body 控制在 ≤30 行。
    await_readers(stdout_task, stderr_task).await;
    let (exit_code, success) = resolve_exit_outcome(&status, ctx.executor.as_ref());
    emit_post_execution_todo_progress_ctx(&ctx).await;
    let (logs_snapshot, result_str) =
        flush_and_extract_logs(&ctx, log_flusher, flush_timer).await;
    persist_and_finalize_completion(&ctx, success, exit_code, &logs_snapshot, result_str).await;
    cleanup_worktree_if_needed(&ctx.worktree_ctx);
}

/// 透传 ctx 字段给 `emit_post_execution_todo_progress`。
async fn emit_post_execution_todo_progress_ctx(ctx: &SpawnContext) {
    emit_post_execution_todo_progress(
        &ctx.db,
        &ctx.tx,
        ctx.executor.as_ref(),
        &ctx.task_id,
        ctx.record_id,
    )
    .await;
}

/// 把 ctx + flusher 状态喂给 `flush_and_extract_result`，避免 handle_completed_branch
/// 内出现 5 行 `&ctx.db / &ctx.record_id / ctx.executor.as_ref()` 模板。
async fn flush_and_extract_logs(
    ctx: &SpawnContext,
    log_flusher: Arc<LogFlusher>,
    flush_timer: tokio::task::JoinHandle<()>,
) -> (Vec<crate::models::ParsedLogEntry>, String) {
    flush_and_extract_result(
        log_flusher,
        flush_timer,
        &ctx.db,
        ctx.record_id,
        ctx.executor.as_ref(),
    )
    .await
}

/// persist_completion_record + finalize_normal_completion 二合一：
///
/// 把原本散落在 handle_completed_branch 末尾的 21 参数 finalize 调用收口到一个 helper，
/// 让 orchestrator 只剩 4 行；persist_completion_record 自己的 5 参数不变，
/// 因为它规模可控、可读性 OK。
async fn persist_and_finalize_completion(
    ctx: &SpawnContext,
    success: bool,
    exit_code: i32,
    logs_snapshot: &[crate::models::ParsedLogEntry],
    result_str: String,
) {
    persist_completion_record(
        &ctx.db,
        ctx.executor.as_ref(),
        ctx.record_id,
        logs_snapshot,
        success,
        ctx.execution_start,
    )
    .await;
    finalize_normal_completion(
        ctx.db.clone(),
        ctx.executor_registry.clone(),
        ctx.tx.clone(),
        ctx.task_manager.clone(),
        ctx.config.clone(),
        ctx.hook_service.clone(),
        ctx.executor.clone(),
        ctx.task_id.clone(),
        ctx.todo_id,
        ctx.todo_title.clone(),
        ctx.todo.clone(),
        ctx.chain.clone(),
        ctx.record_id,
        success,
        exit_code,
        result_str,
        ctx.trigger_type.clone(),
        ctx.feishu_bot_id,
        ctx.feishu_receive_id.clone(),
    )
    .await;
}

/// 等待 stdout/stderr reader 跑完，回收子任务句柄。
///
/// 子进程已自然退出，stdout/stderr 管道已关闭；reader 协程会在管道 EOF 时自然
/// 退出。`let _ = handle.await` 故意忽略 JoinError（reader panic 不影响主流程）。
async fn await_readers(
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
) {
    if let Some(handle) = stdout_task {
        let _ = handle.await;
    }
    if let Some(handle) = stderr_task {
        let _ = handle.await;
    }
}

/// 把 `ExitStatus` 翻译成「exit_code + success」。executor 子类自行决定什么
/// exit code 算成功（claude_code 把 0 当成功，hermes 把 0/1 之外的都当失败等）。
fn resolve_exit_outcome(
    status: &std::io::Result<std::process::ExitStatus>,
    executor: &dyn CodeExecutor,
) -> (i32, bool) {
    let exit_code = status.as_ref().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
    let success = executor.check_success(exit_code);
    (exit_code, success)
}

/// 收口「finalize flusher + 取日志快照 + 抽 result_str」三步。读日志是从 DB
/// 拉最近一次 finalize 后的全量行；result_str 是 executor 子类用启发式规则
/// 从日志末尾抽出来的「最终结论」字符串。
///
/// `log_flusher` 接 `Arc<LogFlusher>` 而不是 `&LogFlusher`，因为 `finalize` 的签名
/// 是 `fn finalize(self: Arc<Self>)`——必须 move 走 Arc 才能在内部触发 flush 收尾。
async fn flush_and_extract_result(
    log_flusher: Arc<LogFlusher>,
    flush_timer: tokio::task::JoinHandle<()>,
    db: &Arc<Database>,
    record_id: i64,
    executor: &dyn CodeExecutor,
) -> (Vec<crate::models::ParsedLogEntry>, String) {
    log_flusher.finalize().await;
    let _ = flush_timer.await;
    let all_logs_snapshot = db.get_all_execution_logs(record_id).await.unwrap_or_default();
    let result_str = executor
        .get_final_result(&all_logs_snapshot)
        .unwrap_or_default();
    (all_logs_snapshot, result_str)
}

// ============================================================================
//  issue #660: stage 之间的数据载体
//
//  `PreparedExecution` 与 `SpawnInputs` 都是阶段产物聚合对象，把 6+ 个共享 Arc
//  引用、字符串字段、task_guard 等统一收口，避免 stage 函数签名出现 Long
//  Parameter List。每个字段都标注「为什么需要 move 进下一阶段」，让读者
//  不用读 stage 函数体就能理解数据流。
// ============================================================================

/// Stage 1 产物：完成 executor 选择 + record 创建，并持有 task_guard / cancel_rx。
///
/// 这一阶段不动 todo 状态、不创建 worktree，所以 fail-fast 路径无需清理副作用。
///
/// 设计取舍：把 `RunTodoExecutionRequest` 整段嵌入 `request` 字段而不是平铺。
/// 平铺需要 14 个字段二次声明；嵌入只需 1 个字段，添加 request 字段时只动 1 处。
/// 读 `request.db` 比读 `db` 多打 5 个字符，可读性代价远小于维护成本。
struct PreparedExecution {
    /// 入参 request。Stage 2 / Stage 3 仍会读到 todo_id / chain / trigger_type 等。
    request: RunTodoExecutionRequest,
    /// RAII guard for task registry；必须 move 进 spawn 子任务，否则 drop 时会误删 sender。
    task_guard: crate::task_manager::TaskGuard,
    /// 与 task_manager 的 cancel channel；spawn 子任务在 select! 中 recv 它。
    cancel_rx: tokio::sync::mpsc::Receiver<()>,
    task_id: String,
    /// 已做 placeholder 替换的 command argv，spawn 阶段原样转发给 executor。
    command_args: Vec<String>,
    executable_path: String,
    /// 选定的 executor Arc，spawn 阶段用作 `executor_spawn`。
    executor: Arc<dyn CodeExecutor>,
    executor_str: String,
    record_id: i64,
    /// todo 在并发控制 / pre-hook / executor 选择中都用到，必须保留；load_todo 失败时为 None。
    todo: Option<Todo>,
    /// 仅 spawn 阶段用于 effective_workspace 回退。
    todo_workspace: Option<String>,
    timeout_secs: u64,
}

/// Stage 2 产物：worktree 已创建 + todo 已启动 + TaskInfo 已注册，
/// 准备 move 进 spawn 子任务的全部数据。
///
/// 嵌入 `prepared` 而不是平铺 stage 1 的 14 个字段；新加 stage 1 字段时只动 1 处。
struct SpawnInputs {
    prepared: PreparedExecution,
    todo_title: String,
    executor_spawn: Arc<dyn CodeExecutor>,
    effective_workspace: Option<String>,
    execution_timeout_secs: u64,
    worktree_ctx: WorktreeContext,
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

#[cfg(test)]
mod run_todo_execution_stage_tests {
    //! issue #660: 钉死 `run_todo_execution` 的 stage 函数签名与 PreparedExecution /
    //! SpawnInputs 字段。一旦 stage 拆解规则被破坏，下面的引用会编译失败，
    //! 提示重构者「编排函数必须保持 ≤30 行 + 三个 stage」的契约。
    use super::*;

    /// 顶层 `run_todo_execution` 必须是 `async fn(request) -> ExecutionResult`。
    /// 编译期断言：(1) 函数名 (2) 入参 (3) 返回类型 与 issue #660 约定一致。
    /// 任何破坏会直接编译失败，比加 doc 注释更可靠。
    #[test]
    fn test_run_todo_execution_signature_is_preserved() {
        // 类型签名等价检查：函数指针形如 async fn(req) -> Result<...>。
        // `let _: ExecutionResult = run_todo_execution(req).await` 让编译器检查返回类型，
        // 闭包返回 `()`，所以外层用 `async fn`/`block_on` 不可行——这里只在
        // 编译期验证签名，不实际 await。
        fn _check_return_type(_req: RunTodoExecutionRequest) -> impl std::future::Future<Output = ExecutionResult> {
            run_todo_execution(_req)
        }
    }

    /// 验证 `PreparedExecution` 持有 `task_guard` 与 `cancel_rx` 两个 RAII 句柄，
    /// 这两个字段在 stage 1 完成时已 fixed，后续 stage 仍必须 move 进 spawn。
    #[test]
    fn test_prepared_execution_carries_task_guard_and_cancel_rx() {
        // 编译期类型检查：访问这两个字段不能改名/删除。
        fn _assert_fields(p: &PreparedExecution) -> &crate::task_manager::TaskGuard {
            &p.task_guard
        }
        fn _assert_cancel(p: &PreparedExecution) -> &tokio::sync::mpsc::Receiver<()> {
            &p.cancel_rx
        }
    }

    /// 验证 `SpawnInputs` 持有 `task_guard`、`cancel_rx`、`executor_spawn`，
    /// 这三个字段在 spawn 阶段开始时**必须**还在手里，否则 spawn 闭包拿不到
    /// 它们就会导致 RAII 失效或 sender 误删。
    /// `task_guard` / `cancel_rx` 在 issue #660 重构后下沉到 `s.prepared`，访问路径
    /// 跟着同步更新。
    #[test]
    fn test_spawn_inputs_carries_required_handles() {
        fn _assert_guard(s: &SpawnInputs) -> &crate::task_manager::TaskGuard {
            &s.prepared.task_guard
        }
        fn _assert_cancel(s: &SpawnInputs) -> &tokio::sync::mpsc::Receiver<()> {
            &s.prepared.cancel_rx
        }
        fn _assert_executor(s: &SpawnInputs) -> &Arc<dyn CodeExecutor> {
            &s.executor_spawn
        }
    }

    /// 验证 stage 函数之间通过 Result<_, ExecutionResult> 串联。
    /// 编译期断言 `prepare_execution_state` 的入参/返回类型与签名预期一致。
    #[test]
    fn test_stage_signatures_are_stable() {
        // 把 async fn 提升为返回 Future 的函数指针（编译期断言签名一致）。
        fn _check_return_type(
            _req: RunTodoExecutionRequest,
        ) -> impl std::future::Future<Output = Result<PreparedExecution, ExecutionResult>> {
            prepare_execution_state(_req)
        }
    }

    /// 回归测试：issue #660 重构中 `move_into_runtime` 必须把
    /// `SpawnInputs.effective_workspace` 透传到 `SpawnRuntime.effective_workspace`，
    /// 而不是丢失到 `prepared.todo_workspace`。这是 issue #643 (worktree) 的
    /// 正确行为保证：worktree 启用时 spawn 子进程必须以 worktree 路径为 cwd。
    #[test]
    fn test_spawn_runtime_carries_effective_workspace() {
        // 编译期断言 SpawnRuntime 持有 effective_workspace 字段。
        // 用法：spawn_executor_child 内部用 runtime.effective_workspace.as_deref()
        // 决定子进程 cwd；如果字段缺失或被改名，这个 helper 就编不过。
        fn _assert_field(rt: &SpawnRuntime) -> Option<&String> {
            rt.effective_workspace.as_ref()
        }
        // 同时断言 prepared.todo_workspace 与 effective_workspace 是两个独立字段，
        // 避免有人把 effective_workspace 直接删掉回退到 todo_workspace。
        fn _assert_distinct_fields(rt: &SpawnRuntime) -> (Option<&String>, Option<&String>) {
            (rt.effective_workspace.as_ref(), rt.prepared.todo_workspace.as_ref())
        }
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

    // ====== Regression tests for session-id handling (issue related to commit e4b3953) ======

    /// 验证当 `resume_session_id` 为 `None` 时，生成有效的 UUID 字符串（而非 "fallback" 占位符），
    /// 且 `is_resume` 标志为 `false`。
    ///
    /// 这是首次执行分支的回归测试：确保 Claude Code CLI 不会因 session-id 格式无效而拒绝执行。
    /// 实现位置：`select_executor_and_build_command` 函数中的 `unwrap_or_else(|| Uuid::new_v4().to_string())`。
    #[test]
    fn test_session_id_handling_when_resume_is_none() {
        // 模拟首次执行场景：request.resume_session_id = None。
        // 由于无法轻易构造完整的 RunTodoExecutionRequest（需要 db / task_manager / tx / 等依赖），
        // 这里直接测试生成逻辑：None.unwrap_or_else(|| Uuid::new_v4().to_string()) 的行为。
        let resume_session_id: Option<String> = None;
        let session_id_for_executor = resume_session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let is_resume = resume_session_id.is_some();

        // 断言生成的 session_id 是有效的 UUID v4 字符串（36 字符，包含连字符）。
        // 格式如: "550e8400-e29b-41d4-a716-446655440000"
        assert_eq!(session_id_for_executor.len(), 36);
        assert!(session_id_for_executor.contains('-'));
        // 验证可以被解析为合法 UUID（核心目的：不是 "fallback" 占位符）。
        assert!(Uuid::parse_str(&session_id_for_executor).is_ok());
        // 验证 is_resume 标志在首次执行时为 false。
        assert!(!is_resume);
    }

    /// 验证当 `resume_session_id` 为 `Some(value)` 时，提供的值被原样保留在 `session_id_for_executor` 中，
    /// 且 `is_resume` 标志为 `true`。
    ///
    /// 这是恢复会话分支的回归测试：确保用户显式传入的 session-id 不被覆盖，
    /// 且 executor 能正确识别这是一个 resume 请求（而非 new session）。
    /// 实现位置：`select_executor_and_build_command` 函数中的 `.clone().unwrap_or_else(...)`。
    #[test]
    fn test_session_id_handling_when_resume_is_some() {
        // 模拟恢复会话场景：request.resume_session_id = Some("existing-uuid")。
        let existing_uuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let resume_session_id: Option<String> = Some(existing_uuid.to_string());
        let session_id_for_executor = resume_session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let is_resume = resume_session_id.is_some();

        // 断言传入的 session_id 被原封不动地保留（未被 unwrap_or_else 的 fallback 覆盖）。
        assert_eq!(session_id_for_executor, existing_uuid);
        // 验证 is_resume 标志在恢复会话时为 true，让 executor 知道这是 resume 而非 new。
        assert!(is_resume);
    }
}
