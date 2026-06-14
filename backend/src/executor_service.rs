use std::sync::Arc;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;
use tracing::Instrument;
use uuid::Uuid;

use command_group::AsyncCommandGroup;

use crate::adapters::{parse_executor_type, ExecutorRegistry};
use crate::db::{Database, NewExecutionRecord};
use crate::handlers::ExecEvent;
use crate::hooks::HookService;
use crate::models::{ExecutorType, ParsedLogEntry};
use crate::task_manager::TaskManager;

fn send_event(tx: &broadcast::Sender<ExecEvent>, event: ExecEvent) {
    let _ = tx.send(event);
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
        chain,
        source_todo_id,
        source_todo_title,
        source_hook_id,
        feishu_bot_id,
        feishu_receive_id,
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

    // Get todo to read stored executor and check concurrency
    let todo = match db.get_todo(todo_id).await {
        Ok(Some(t)) => {
            // 检查该 todo 下正在执行的记录数量是否已达并发上限
            // 需要过滤掉孤儿记录：状态为 running 但 task_manager 中没有对应 task
            let running_tasks = task_manager.get_all_task_infos().await;
            let running_records = match db.get_running_records_by_todo_id(todo_id).await {
                Ok(records) => records,
                Err(e) => {
                    tracing::error!("Failed to get running execution records: {}", e);
                    return ExecutionResult {
                        task_id,
                        record_id: None,
                    };
                }
            };
            let running_count_for_todo = running_records
                .iter()
                .filter(|r| {
                    // 排除僵尸记录：状态为 running 但 task_manager 中没有对应 task
                    if let Some(task_id) = &r.task_id {
                        running_tasks.iter().any(|t| t.task_id == *task_id)
                    } else {
                        false
                    }
                })
                .count();
            if running_count_for_todo >= max_concurrent as usize {
                tracing::warn!(
                    "Todo {} has {} execution(s) still running (limit: {}), rejecting",
                    todo_id, running_count_for_todo, max_concurrent
                );
                task_manager.remove(&task_id).await;
                send_event(
                    &tx,
                    ExecEvent::Finished {
                        task_id: task_id.clone(),
                        todo_id,
                        todo_title: t.title.clone(),
                        executor: "".to_string(),
                        success: false,
                        result: Some(format!(
                            "Todo {} has {} execution(s) still running (limit: {}). Please stop them first.",
                            todo_id, running_count_for_todo, max_concurrent
                        )),
                        feishu_bot_id: None,
                        feishu_receive_id: None,
                    },
                );
                return ExecutionResult {
                    task_id,
                    record_id: None,
                };
            }

            Some(t)
        }
        Ok(None) => None,
        Err(e) => {
            tracing::error!(
                "Failed to fetch todo {} for executor selection: {}",
                todo_id,
                e
            );
            None
        }
    };
    let todo_executor = todo.as_ref().and_then(|t| t.executor.clone());
    let todo_workspace = todo.as_ref().and_then(|t| t.workspace.clone());
    let todo_worktree_enabled = todo.as_ref().map(|t| t.worktree_enabled).unwrap_or(false);

    // Determine which executor to use: explicit > todo stored > default
    let executor_type = req_executor
        .as_deref()
        .and_then(|exec| {
            parse_executor_type(exec).or_else(|| {
                tracing::warn!("Unknown explicit executor '{}', trying todo executor", exec);
                None
            })
        })
        .or_else(|| {
            todo_executor.as_deref().and_then(|exec| {
                parse_executor_type(exec).or_else(|| {
                    tracing::warn!("Unknown todo executor '{}', falling back to default", exec);
                    None
                })
            })
        })
        .unwrap_or_default();

    let executor = match executor_registry
        .get(executor_type).await
    {
        Some(exec) => exec,
        None => match executor_registry.get_default().await {
            Some(exec) => exec,
            None => {
                tracing::error!(
                    "No executor available for type {:?} and no default registered",
                    executor_type
                );
                let _ = db.finish_todo_execution(todo_id, false).await;
                send_event(
                    &tx,
                    ExecEvent::Finished {
                        task_id: task_id.clone(),
                        todo_id,
                        todo_title: todo.as_ref().map(|t| t.title.clone()).unwrap_or_default(),
                        executor: executor_type.to_string(),
                        success: false,
                        result: Some("No executor available".to_string()),
                        feishu_bot_id: None,
                        feishu_receive_id: None,
                    },
                );
                task_manager.remove(&task_id).await;
                return ExecutionResult {
                    task_id,
                    record_id: None,
                };
            },
        },
    };

    let executable_path = executor.executable_path().to_string();
    let session_id_for_executor = resume_session_id.as_deref().unwrap_or(&task_id);
    let is_resume = resume_session_id.is_some();
    let mut command_args =
        executor.command_args_with_session(&message, Some(session_id_for_executor), is_resume);

    // Add worktree flag for claude_code and hermes executors
    // claude_code: --worktree (no path argument, Claude Code auto-manages worktree cleanup)
    // hermes: --worktree (no path argument, uses current directory)
    // Must be placed before --session-id or --resume flag
    let exec_type = executor.executor_type();
    if todo_worktree_enabled {
        match exec_type {
            ExecutorType::Claudecode | ExecutorType::Hermes => {
                // Find position of --session-id or --resume and insert before it
                let insert_pos = command_args.iter()
                    .position(|s| s == "--session-id" || s == "--resume")
                    .unwrap_or(command_args.len());
                command_args.insert(insert_pos, "--worktree".to_string());
            }
            _ => {}
        }
    }

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
            tracing::error!("Failed to create execution record: {}", e);
            let _ = db.finish_todo_execution(todo_id, false).await;
            task_manager.remove(&task_id).await;
            return ExecutionResult {
                task_id,
                record_id: None,
            };
        }
    };

    // State-change hooks for "进入执行中" fire from the update_todo handler when
    // the user transitions the todo into in_progress. The executor no longer
    // gates execution on a hook — it just runs.

    // Update todo status to running and associate with task
    if let Err(e) = db.start_todo_execution(todo_id, &task_id).await {
        tracing::error!("Failed to start todo execution: {}", e);
        let entry = ParsedLogEntry::error(format!("Failed to start todo execution: {}", e));
        send_event(
            &tx,
            ExecEvent::Output {
                task_id: task_id.clone(),
                entry,
            },
        );
        send_event(
            &tx,
            ExecEvent::Finished {
                task_id: task_id.clone(),
                todo_id,
                todo_title: todo.as_ref().map(|t| t.title.clone()).unwrap_or_default(),
                executor: executor_str.clone(),
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
                result: &format!("Failed to start todo execution: {}", e),
                usage: None,
                model: None,
                review_meta: None,
            })
            .await;
        task_manager.remove(&task_id).await;
        return ExecutionResult {
            task_id,
            record_id: Some(record_id),
        };
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

    // 将 task_guard 移入 tokio::spawn 闭包，使其存活到执行结束。
    // 若不这么做，外层 run_todo_execution 返回时 guard 即 drop，
    // 导致 manager.remove 被调用、Sender 被丢弃、cancel_rx 的 channel 关闭，
    // select! 中 cancel_rx.recv() 立即返回 None，误触发取消流程。
    let _task_guard = task_guard;

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
        // 将 _task_guard 移入异步闭包，使其存活到整个执行周期。
        // 若不绑定，外层 drop 时会误删 Sender 导致 cancel_rx.recv() 返回 None。
        let _task_guard = _task_guard;
        let execution_start = std::time::Instant::now();

        send_event(
            &tx_clone,
            ExecEvent::Started {
                task_id: task_id.clone(),
                todo_id,
                todo_title: todo_title.clone(),
                executor: executor_spawn.executor_type().to_string(),
            },
        );

        let entry = ParsedLogEntry::info(format!("Starting {}", executor_spawn.executor_type()));
        send_event(
            &tx_clone,
            ExecEvent::Output {
                task_id: task_id.clone(),
                entry,
            },
        );

        // 使用 command-group 创建进程组，自动管理进程树
        let mut cmd = tokio::process::Command::new(&executable_path);

        tracing::debug!(
            executable = %executable_path,
            arg_count = command_args.len(),
            "Spawning executor"
        );

        cmd.args(&command_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped());

        // 设置工作目录（如果指定了 workspace）
        if let Some(ws) = todo_workspace.as_ref() {
            cmd.current_dir(ws);
        }

        // 使用 command-group 的 group_spawn 创建进程组
        let mut child = match cmd.group_spawn() {
            Ok(c) => c,
            Err(e) => {
                let error_msg = format!("Failed to spawn executor: {}", e);
                let entry = ParsedLogEntry::error(error_msg.clone());
                send_event(
                    &tx_clone,
                    ExecEvent::Output {
                        task_id: task_id.clone(),
                        entry,
                    },
                );
                send_event(
                    &tx_clone,
                    ExecEvent::Finished {
                        task_id: task_id.clone(),
                        todo_id,
                        todo_title: todo_title.clone(),
                        executor: executor_spawn.executor_type().to_string(),
                        success: false,
                        result: Some(error_msg),
                        feishu_bot_id,
                        feishu_receive_id,
                    },
                );
                let _ = db_clone.finish_todo_execution(todo_id, false).await;
                task_manager_spawn.remove(&task_id).await;
                return;
            }
        };

        let child_id = child.id().unwrap_or(0);

        // Close stdin immediately so child processes get EOF when they try to read it.
        // Without this, processes that read stdin after finishing work will hang forever.
        drop(child.inner().stdin.take());

        // 保存 pid 到 execution_records 表 (使用进程组 leader 的 pid)
        if child_id > 0 {
            let _ = db_clone
                .update_execution_record_pid(record_id, Some(child_id as i32))
                .await;
        }

        let stdout_handle = child.inner().stdout.take();
        let stderr_handle = child.inner().stderr.take();

        // 统一管理 stdout/stderr 推入的日志 buffer 与后台 flush。
        // 详见 `crate::log_flusher` 文档（issue #496）：用 CAS + pending 标志替代旧的
        // fetch_add+swap+store(0) 三步非原子组合；用 oneshot-style 标记驱动 shutdown，
        // 不再有"4s sleep 在 select! 里永远胜出不了 interval.tick"的死代码。
        let log_flusher = Arc::new(crate::log_flusher::LogFlusher::new(
            Box::new(crate::log_flusher::DatabaseLogSink::new(db_clone.clone())),
            crate::log_flusher::LogFlusherConfig::for_record(record_id),
        ));

        let executor_for_parse = executor_spawn.clone();

        // Process stdout
        let stdout_task = if let Some(stdout_reader) = stdout_handle {
            let tx_clone = tx.clone();
            let tid = task_id.clone();
            let executor_clone = executor_for_parse.clone();
            let db_for_todo = db_clone.clone();
            let rid = record_id;
            let log_flusher_for_stdout = log_flusher.clone();

            Some(tokio::spawn(async move {
                let mut reader = BufReader::new(stdout_reader).lines();
                let mut log_count = 0u64;
                let mut session_id_updated = false;
                while let Ok(Some(line)) = reader.next_line().await {
                    // Extract and update session_id if present
                    if !session_id_updated {
                        if let Some(sid) = executor_clone.extract_session_id(&line) {
                            let _ = db_for_todo
                                .update_execution_record_session_id(rid, &sid)
                                .await;
                            session_id_updated = true;
                        }
                    }
                    if let Some(parsed) = executor_clone.parse_output_line(&line) {
                        // Detect todo progress updates
                        if let Some(progress) =
                            crate::todo_progress::try_extract_todo_progress(&parsed)
                        {
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

                        // Send stats update after tool calls or every 10 log entries
                        // 走 LogFlusher::with_logs 在持锁状态下扫描 buffer，比旧实现里
                        // 直接 lock logs_for_db 更明确——锁只覆盖只读扫描，不会跨越 push。
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
                                        .count()
                                        as u64;
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

                        // 把日志推入 flusher：内部 CAS 触发后台 flush，
                        // 替代了原 fetch_add+swap+store(0) 的非原子组合。
                        log_flusher_for_stdout.push(parsed.clone()).await;
                        send_event(
                            &tx_clone,
                            ExecEvent::Output {
                                task_id: tid.clone(),
                                entry: parsed,
                            },
                        );
                    }
                }
            }))
        } else {
            None
        };

        // Capture stderr
        let stderr_tx = tx.clone();
        let stderr_tid = task_id.clone();
        let executor_for_stderr = executor_spawn.clone();
        let log_flusher_for_stderr = log_flusher.clone();
        let stderr_task = stderr_handle.map(|stderr_reader| {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr_reader).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let entry = if let Some(parsed) = executor_for_stderr.parse_stderr_line(&line) {
                        parsed
                    } else {
                        ParsedLogEntry::stderr(line.clone())
                    };
                    // 推入 LogFlusher 走与 stdout 完全相同的 CAS 路径；
                    // 不再有各自一份的 fetch_add/swap/store(0) 复制代码。
                    log_flusher_for_stderr.push(entry.clone()).await;
                    send_event(
                        &stderr_tx,
                        ExecEvent::Output {
                            task_id: stderr_tid.clone(),
                            entry,
                        },
                    );
                }
            })
        });

        // 定时兜底 flush：每 3 秒检查未刷新条目，有则写库。
        // 直接走 LogFlusher::run_timer：内部已包含"pending 标志、shutdown 退出"逻辑，
        // 这里不再重复 select! 的 4s sleep 死代码（issue #496 第三点）。
        let flush_timer = {
            let log_flusher_for_timer = log_flusher.clone();
            tokio::spawn(async move { log_flusher_for_timer.run_timer().await })
        };

        // execution_timeout_secs is captured by value here — config changes after this
        // task starts have no effect. To pick up a new timeout, wait for the current
        // execution to finish (or force-fail it via the UI).
        let timeout_enabled = execution_timeout_secs > 0;
        // Non-zero values are guaranteed >= 60 by normalize_paths clamp, so from_secs is safe.
        let timeout_duration = std::time::Duration::from_secs(execution_timeout_secs);
        let timeout_str = format_timeout_secs(execution_timeout_secs);
        let timeout_sleep = tokio::time::sleep(timeout_duration);
        tokio::pin!(timeout_sleep);

        let status = tokio::select! {
            biased;
            _ = cancel_rx.recv() => {
                // Cancelled (or channel closed): 使用 command-group 安全杀死整个进程组
                kill_process_tree(&mut child).await;

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
                // ——替代旧的"set flag → 等 timer → drain flush_handles → serialize buffer"四步分散逻辑。
                log_flusher.finalize().await;
                let _ = flush_timer.await;

                let _ = db_clone.update_todo_status(todo_id, crate::models::TodoStatus::Cancelled).await;
                let _ = db_clone.update_todo_task_id(todo_id, None).await;

                // 此时 buffer 已被 finalize drain 到 DB；从 DB 读全量日志做 remaining_logs 字段
                let remaining_logs = db_clone
                    .get_all_execution_logs(record_id)
                    .await
                    .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "[]".to_string()))
                    .unwrap_or_else(|_| "[]".to_string());
                let _ = db_clone.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
                    id: record_id,
                    status: crate::models::ExecutionStatus::Failed.as_str(),
                    remaining_logs: &remaining_logs,
                    result: "任务已被手动停止",
                    usage: None,
                    model: None,
                    review_meta: None,
                }).await;

                let entry = ParsedLogEntry::error("Execution cancelled by user");
                send_event(&tx_clone, ExecEvent::Output { task_id: task_id.clone(), entry });
                send_event(&tx_clone, ExecEvent::Finished {
                    task_id: task_id.clone(),
                    todo_id,
                    todo_title: todo_title.clone(),
                    executor: executor_spawn.executor_type().to_string(),
                    success: false,
                    result: Some("Task was cancelled by user".to_string()),
                    feishu_bot_id,
                    feishu_receive_id,
                });
                task_manager_spawn.remove(&task_id).await;
                return;
            }
            _ = &mut timeout_sleep, if timeout_enabled => {
                // Timeout: 自动终止执行时间过长的进程，释放资源
                tracing::warn!(
                    "Execution timeout, terminating process: timeout={}s, todo_id={}, task_id={}",
                    execution_timeout_secs, todo_id, task_id
                );
                kill_process_tree(&mut child).await;

                let _status = child.wait().await;

                if let Some(handle) = stdout_task {
                    let _ = handle.await;
                }
                if let Some(handle) = stderr_task {
                    let _ = handle.await;
                }

                log_flusher.finalize().await;
                if let Err(e) = flush_timer.await {
                    tracing::error!("flush_timer panicked: {}", e);
                }

                let remaining_logs = db_clone
                    .get_all_execution_logs(record_id)
                    .await
                    .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "[]".to_string()))
                    .unwrap_or_else(|_| "[]".to_string());
                let _ = db_clone.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
                    id: record_id,
                    status: crate::models::ExecutionStatus::Failed.as_str(),
                    remaining_logs: &remaining_logs,
                    result: "Execution timeout",
                    usage: None,
                    model: None,
                    review_meta: None,
                }).await;

                let entry = ParsedLogEntry::error("Execution timeout, process terminated by system");
                send_event(&tx_clone, ExecEvent::Output { task_id: task_id.clone(), entry });
                send_event(&tx_clone, ExecEvent::Finished {
                    task_id: task_id.clone(),
                    todo_id,
                    todo_title: todo_title.clone(),
                    executor: executor_spawn.executor_type().to_string(),
                    success: false,
                    result: Some(format!("Execution timeout, exceeded {}", timeout_str)),
                    feishu_bot_id,
                    feishu_receive_id,
                });
                task_manager_spawn.remove(&task_id).await;
                return;
            }
            status = child.wait() => {
                // 子进程已自然退出，command-group 的进程组已自动清理
                if let Some(handle) = stdout_task {
                    let _ = handle.await;
                }
                if let Some(handle) = stderr_task {
                    let _ = handle.await;
                }

                status
            }
        };

        let exit_code = status
            .as_ref()
            .map(|s| s.code().unwrap_or(-1))
            .unwrap_or(-1);
        let success = executor_spawn.check_success(exit_code);

        // Try post-execution todo progress extraction (for executors like hermes that don't expose tool calls in stdout)
        if let Some(progress) = executor_spawn.post_execution_todo_progress() {
            if let Ok(progress_json) = serde_json::to_string(&progress) {
                let _ = db_clone
                    .update_execution_record_todo_progress(record_id, &progress_json)
                    .await;
                send_event(
                    &tx_clone,
                    ExecEvent::TodoProgress {
                        task_id: task_id.clone(),
                        progress,
                    },
                );
            }
        }

        // 正常退出：与 cancel/timeout 一样走 finalize 把残余刷到 DB
        log_flusher.finalize().await;
        let _ = flush_timer.await;

        // 从 execution_logs 表读取已刷入的日志（finalize 已把 buffer 全量写入）
        let flushed_logs = db_clone
            .get_all_execution_logs(record_id)
            .await
            .unwrap_or_default();
        let all_logs_snapshot = flushed_logs;
        let all_logs_json = serde_json::to_string(&all_logs_snapshot).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize all logs: {}", e);
            "[]".to_string()
        });
        let result_str = executor_spawn
            .get_final_result(&all_logs_snapshot)
            .unwrap_or_default();

        // Extract execution stats from logs in single pass
        let execution_stats = extract_execution_stats(&all_logs_snapshot, executor_spawn.get_tool_calls_count());
        if let Ok(stats_json) = serde_json::to_string(&execution_stats) {
            let _ = db_clone
                .update_execution_record_stats(record_id, &stats_json)
                .await;
        }

        let final_status = if success {
            crate::models::ExecutionStatus::Success.as_str()
        } else {
            crate::models::ExecutionStatus::Failed.as_str()
        };
        let mut usage = executor_spawn.get_usage(&all_logs_snapshot);
        let model = executor_spawn.get_model();

        // Always use wall-clock duration (start to end of execution)
        // This ensures duration is always available, regardless of executor support
        let wall_clock_duration_ms = execution_start.elapsed().as_millis() as u64;
        match usage.as_mut() {
            Some(u) => {
                // Override executor-reported duration with actual wall-clock time
                u.duration_ms = Some(wall_clock_duration_ms);
            }
            None => {
                usage = Some(crate::models::ExecutionUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    total_cost_usd: None,
                    duration_ms: Some(wall_clock_duration_ms),
                });
            }
        }

        let _ = db_clone
            .update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
                id: record_id,
                status: final_status,
                remaining_logs: &all_logs_json,
                result: &result_str,
                usage: usage.as_ref(),
                model: model.as_deref(),
                review_meta: None,
            })
            .await;

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
                db_clone.clone(),
                executor_registry_spawn.clone(),
                tx_clone.clone(),
                task_manager_spawn.clone(),
                config_spawn.clone(),
                hook_service_spawn.clone(),
                todo_id,
                record_id,
            )
            .await;
        }

        let _ = db_clone.finish_todo_execution(todo_id, success).await;

        // Fire the state-change hook for "进入已完成/失败". The executor
        // bypasses the update_todo handler, so this is the only place the
        // transition to a terminal state is observed.
        if let Some(t) = todo.as_ref() {
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
                chain.clone(),
            ) {
                // 复用 AppState 里共享的 HookService，而不是再 Arc::new 一份。
                //
                // 之前这里每次执行末段都重新构造 HookService + 重新 clone 5 个
                // ServiceContext 字段 (db/executor_registry/tx/task_manager/config)，
                // 造成 (1) 重复的 Arc 引用计数抖动，(2) 出现两份 HookService 实例
                // 各自管自己内部状态的不一致 (例如后续想加共享缓存会立刻踩坑)。
                // 现在与 handlers/todo.rs 走的是同一个 Arc<HookService> 单例。
                //
                // fire_for_todo 内部是 tokio::spawn 子任务 + fire-and-forget，
                // 这里只追加一行 debug 日志方便排查"是否走到了 fire 路径"。
                tracing::debug!(
                    "firing state-change hook for todo #{} -> {:?}",
                    todo_id,
                    new_status
                );
                hook_service_spawn.clone().fire_for_todo(todo_id, ctx);
            }
        }

        let entry = ParsedLogEntry::new(
            if success { "info" } else { "error" },
            format!(
                "Executor finished with exit_code: {}, result: {}",
                exit_code, result_str
            ),
        );
        send_event(
            &tx_clone,
            ExecEvent::Output {
                task_id: task_id.clone(),
                entry,
            },
        );

        send_event(
            &tx_clone,
            ExecEvent::Finished {
                task_id: task_id.clone(),
                todo_id,
                todo_title: todo_title.clone(),
                executor: executor_spawn.executor_type().to_string(),
                success,
                result: Some(result_str),
                feishu_bot_id,
                feishu_receive_id,
            },
        );
        task_manager_spawn.remove(&task_id).await;
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

    // 2) 评审师模板
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

    // 4) 复用评审师模板 todo，直接执行（不 clone 新实例）
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
}
