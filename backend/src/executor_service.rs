use std::sync::Arc;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use command_group::AsyncCommandGroup;

use crate::adapters::{parse_executor_type, ExecutorRegistry};
use crate::db::{Database, NewExecutionRecord};
use crate::handlers::ExecEvent;
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
    pub config: Arc<tokio::sync::RwLock<crate::config::Config>>,
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
pub async fn run_todo_execution(request: RunTodoExecutionRequest) -> ExecutionResult {
    let RunTodoExecutionRequest {
        db,
        executor_registry,
        tx,
        task_manager,
        config,
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
    let mut cancel_rx = task_manager.register(task_id.clone()).await;

    // Read runtime settings from config
    let (max_concurrent, timeout_secs) = {
        let cfg = config.read().await;
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

    tokio::spawn(async move {
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

        let logs = Arc::new(Mutex::new(Vec::<ParsedLogEntry>::new()));
        let logs_for_db = logs.clone();
        let logs_for_result = logs.clone();
        let flush_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let unflushed_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let flush_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> =
            Arc::new(Mutex::new(Vec::new()));
        const FLUSH_COUNT_THRESHOLD: u64 = 5;
        // 全局 flush 互斥锁，防止并发 flush 任务在 append_execution_record_logs 中产生读-改-写竞态
        let flush_mutex: Arc<tokio::sync::Mutex<()>> = Arc::new(tokio::sync::Mutex::new(()));
        // Graceful shutdown flag for flush timer - avoids aborting in-flight flush tasks
        let flush_shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let executor_for_parse = executor_spawn.clone();

        // Process stdout
        let stdout_task = if let Some(stdout_reader) = stdout_handle {
            let tx_clone = tx.clone();
            let tid = task_id.clone();
            let executor_clone = executor_for_parse.clone();
            let logs_for_db = logs_for_db.clone();
            let db_for_todo = db_clone.clone();
            let rid = record_id;
            let flush_pending_for_stdout = flush_pending.clone();
            let unflushed_for_stdout = unflushed_count.clone();
            let flush_handles_stdout = flush_handles.clone();
            let flush_mutex_stdout = flush_mutex.clone();

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
                        let is_tool_call = parsed.log_type == "tool_use"
                            || parsed.log_type == "tool_call"
                            || parsed.log_type == "tool";
                        log_count += 1;
                        if is_tool_call || log_count.is_multiple_of(10) {
                            let current_logs = logs_for_db.lock().await;
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
                            let stats = crate::models::ExecutionStats {
                                tool_calls,
                                conversation_turns,
                                thinking_count,
                            };
                            send_event(
                                &tx_clone,
                                ExecEvent::ExecutionStats {
                                    task_id: tid.clone(),
                                    stats,
                                },
                            );
                        }

                        logs_for_db.lock().await.push(parsed.clone());
                        let prev =
                            unflushed_for_stdout.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if prev + 1 >= FLUSH_COUNT_THRESHOLD
                            && !flush_pending_for_stdout
                                .swap(true, std::sync::atomic::Ordering::Relaxed)
                        {
                            unflushed_for_stdout.store(0, std::sync::atomic::Ordering::Relaxed);
                            let snapshot = std::mem::take(&mut *logs_for_db.lock().await);
                            let snapshot_len = snapshot.len() as u64;
                            let db_flush = db_for_todo.clone();
                            let rid_flush = rid;
                            let fp = flush_pending_for_stdout.clone();
                            let fm = flush_mutex_stdout.clone();
                            let uc_restore = unflushed_for_stdout.clone();
                            let logs_restore = logs_for_db.clone();
                            let h = tokio::spawn(async move {
                                let _guard = fm.lock().await;
                                let success = match serde_json::to_string(&snapshot) {
                                    Ok(json) => {
                                        db_flush.append_execution_record_logs(rid_flush, &json).await.is_ok()
                                    }
                                    Err(_) => false,
                                };
                                drop(_guard); // Release mutex before potential restore
                                if !success {
                                    // On failure, merge snapshot back and restore count
                                    let mut logs = logs_restore.lock().await;
                                    logs.extend(snapshot);
                                    uc_restore.fetch_add(snapshot_len, std::sync::atomic::Ordering::Relaxed);
                                }
                                fp.store(false, std::sync::atomic::Ordering::Relaxed);
                            });
                            flush_handles_stdout.lock().await.push(h);
                        }
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
        let logs_for_stderr = logs.clone();
        let executor_for_stderr = executor_spawn.clone();
        let db_for_stderr = db_clone.clone();
        let rid_for_stderr = record_id;
        let flush_for_stderr = flush_pending.clone();
        let unflushed_for_stderr = unflushed_count.clone();
        let flush_handles_stderr = flush_handles.clone();
        let flush_mutex_stderr = flush_mutex.clone();
        let stderr_task = stderr_handle.map(|stderr_reader| {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr_reader).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let entry = if let Some(parsed) = executor_for_stderr.parse_stderr_line(&line) {
                        parsed
                    } else {
                        ParsedLogEntry::stderr(line.clone())
                    };
                    logs_for_stderr.lock().await.push(entry.clone());
                    let prev =
                        unflushed_for_stderr.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if prev + 1 >= FLUSH_COUNT_THRESHOLD
                        && !flush_for_stderr.swap(true, std::sync::atomic::Ordering::Relaxed)
                    {
                        unflushed_for_stderr.store(0, std::sync::atomic::Ordering::Relaxed);
                        let snapshot = std::mem::take(&mut *logs_for_stderr.lock().await);
                        let snapshot_len = snapshot.len() as u64;
                        let db_flush = db_for_stderr.clone();
                        let rid_flush = rid_for_stderr;
                        let fp = flush_for_stderr.clone();
                        let fm = flush_mutex_stderr.clone();
                        let uc_restore = unflushed_for_stderr.clone();
                        let logs_restore = logs_for_stderr.clone();
                        let h = tokio::spawn(async move {
                            let _guard = fm.lock().await;
                            let success = match serde_json::to_string(&snapshot) {
                                Ok(json) => {
                                    db_flush.append_execution_record_logs(rid_flush, &json).await.is_ok()
                                }
                                Err(_) => false,
                            };
                            drop(_guard); // Release mutex before potential restore
                            if !success {
                                // On failure, merge snapshot back and restore count
                                let mut logs = logs_restore.lock().await;
                                logs.extend(snapshot);
                                uc_restore.fetch_add(snapshot_len, std::sync::atomic::Ordering::Relaxed);
                            }
                            fp.store(false, std::sync::atomic::Ordering::Relaxed);
                        });
                        flush_handles_stderr.lock().await.push(h);
                    }
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

        // 定时兜底 flush：每 3 秒检查未刷新条目，有则写库
        let timer_db = db_clone.clone();
        let timer_logs = logs.clone();
        let timer_fp = flush_pending.clone();
        let timer_uc = unflushed_count.clone();
        let timer_handles = flush_handles.clone();
        let timer_shutdown = flush_shutdown.clone();
        let flush_timer = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if timer_shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                            // Graceful shutdown: do one final flush if needed, then exit
                            let n = timer_uc.swap(0, std::sync::atomic::Ordering::Relaxed);
                            if n > 0 {
                                let snapshot = std::mem::take(&mut *timer_logs.lock().await);
                                let db_f = timer_db.clone();
                                let rid_f = record_id;
                                let fp = timer_fp.clone();
                                let h = tokio::spawn(async move {
                                    if let Ok(json) = serde_json::to_string(&snapshot) {
                                        let _ = db_f.append_execution_record_logs(rid_f, &json).await;
                                    }
                                    fp.store(false, std::sync::atomic::Ordering::Relaxed);
                                });
                                timer_handles.lock().await.push(h);
                            }
                            break;
                        }
                        if timer_fp.load(std::sync::atomic::Ordering::Relaxed) {
                            continue;
                        }
                        let n = timer_uc.swap(0, std::sync::atomic::Ordering::Relaxed);
                        if n > 0 && !timer_fp.swap(true, std::sync::atomic::Ordering::Relaxed) {
                            let snapshot = std::mem::take(&mut *timer_logs.lock().await);
                            let snapshot_len = snapshot.len() as u64;
                            let db_f = timer_db.clone();
                            let rid_f = record_id;
                            let fp = timer_fp.clone();
                            let uc_restore = timer_uc.clone();
                            let logs_restore = timer_logs.clone();
                            let h = tokio::spawn(async move {
                                let success = match serde_json::to_string(&snapshot) {
                                    Ok(json) => {
                                        db_f.append_execution_record_logs(rid_f, &json).await.is_ok()
                                    }
                                    Err(_) => false,
                                };
                                if !success {
                                    // On failure, merge snapshot back and restore count
                                    let mut logs = logs_restore.lock().await;
                                    logs.extend(snapshot);
                                    uc_restore.fetch_add(snapshot_len, std::sync::atomic::Ordering::Relaxed);
                                }
                                fp.store(false, std::sync::atomic::Ordering::Relaxed);
                            });
                            timer_handles.lock().await.push(h);
                        } else if n > 0 {
                            timer_uc.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(4)) => {
                        // Fallback timeout to prevent hanging (timer should exit via shutdown flag)
                        if timer_shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }
        });

        // execution_timeout_secs is captured by value here — config changes after this
        // task starts have no effect. To pick up a new timeout, wait for the current
        // execution to finish (or force-fail it via the UI).
        let timeout_enabled = execution_timeout_secs > 0;
        // Non-zero values are guaranteed >= 60 by normalize_paths clamp, so from_secs is safe.
        let timeout_duration = std::time::Duration::from_secs(execution_timeout_secs);
        // Human-readable timeout string for display messages (always English).
        // Uses hours for >=60 min, days for >=24 h to keep the output readable.
        let timeout_str = {
            let total_min = execution_timeout_secs / 60;
            let secs = execution_timeout_secs % 60;
            if secs == 0 {
                if total_min >= 1440 {
                    format!("{} day(s)", total_min / 1440)
                } else if total_min >= 60 {
                    format!("{} hour(s)", total_min / 60)
                } else {
                    format!("{} min", total_min)
                }
            } else {
                format!("{} min {} sec", total_min, secs)
            }
        };
        let timeout_sleep = tokio::time::sleep(timeout_duration);
        tokio::pin!(timeout_sleep);

        let status = tokio::select! {
            biased;
            _ = cancel_rx.recv() => {
                // Cancelled (or channel closed): 使用 command-group 安全杀死整个进程组
                kill_process_tree(&mut child).await;
                // Graceful shutdown: signal timer to finish its pending flush
                flush_shutdown.store(true, std::sync::atomic::Ordering::Relaxed);

                // 收割僵尸进程
                let _status = child.wait().await;

                if let Some(handle) = stdout_task {
                    let _ = handle.await;
                }
                if let Some(handle) = stderr_task {
                    let _ = handle.await;
                }

                // Graceful shutdown: wait for timer to finish its final flush cycle
                let _ = flush_timer.await;
                // 等待所有进行中的 flush 任务完成，防止旧快照覆盖
                for h in flush_handles.lock().await.drain(..) {
                    let _ = h.await;
                }

                let _ = db_clone.update_todo_status(todo_id, crate::models::TodoStatus::Cancelled).await;
                let _ = db_clone.update_todo_task_id(todo_id, None).await;

                // 更新 execution_records 状态为 failed
                let logs_json = serde_json::to_string(&*logs.lock().await)
                    .unwrap_or_else(|e| { tracing::error!("Failed to serialize logs: {}", e); "[]".to_string() });
                let _ = db_clone.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
                    id: record_id,
                    status: crate::models::ExecutionStatus::Failed.as_str(),
                    remaining_logs: &logs_json,
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
                // Graceful shutdown: signal timer to finish its pending flush before abort
                flush_shutdown.store(true, std::sync::atomic::Ordering::Relaxed);

                let _status = child.wait().await;

                if let Some(handle) = stdout_task {
                    let _ = handle.await;
                }
                if let Some(handle) = stderr_task {
                    let _ = handle.await;
                }

                // Wait for timer to finish its final flush cycle
                if let Err(e) = flush_timer.await {
                    tracing::error!("flush_timer panicked: {}", e);
                }

                for h in flush_handles.lock().await.drain(..) {
                    let _ = h.await;
                }

                let logs_json = serde_json::to_string(&*logs.lock().await)
                    .unwrap_or_else(|e| { tracing::error!("Failed to serialize logs: {}", e); "[]".to_string() });
                let _ = db_clone.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
                    id: record_id,
                    status: crate::models::ExecutionStatus::Failed.as_str(),
                    remaining_logs: &logs_json,
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
                // Graceful shutdown: signal timer to finish its pending flush
                flush_shutdown.store(true, std::sync::atomic::Ordering::Relaxed);

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

        // Graceful shutdown: wait for timer to finish its final flush cycle
        let _ = flush_timer.await;
        // 等待所有进行中的 flush 任务完成，防止旧快照覆盖最终写入
        for h in flush_handles.lock().await.drain(..) {
            let _ = h.await;
        }

        // 从 execution_logs 表读取已刷入的日志，与内存中剩余的日志合并形成完整快照
        let remaining = std::mem::take(&mut *logs_for_result.lock().await);
        let flushed_logs = db_clone
            .get_all_execution_logs(record_id)
            .await
            .unwrap_or_default();
        let mut all_logs_snapshot = flushed_logs;
        all_logs_snapshot.extend(remaining);
        let all_logs_json = serde_json::to_string(&all_logs_snapshot).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize all logs: {}", e);
            "[]".to_string()
        });
        let result_str = executor_spawn
            .get_final_result(&all_logs_snapshot)
            .unwrap_or_default();

        // Extract execution stats from logs in single pass
        let (tool_calls, conversation_turns, thinking_count) = {
            let mut tc = 0u64;
            let mut ct = 0u64;
            let mut th = 0u64;
            for l in &all_logs_snapshot {
                match l.log_type.as_str() {
                    "tool_use" | "tool_call" | "tool" => tc += 1,
                    "assistant" | "result" | "text" => ct += 1,
                    "thinking" => th += 1,
                    _ => {}
                }
            }
            (tc, ct, th)
        };
        // Override tool_calls if executor provides its own count
        let tool_calls = executor_spawn.get_tool_calls_count().unwrap_or(tool_calls);
        let execution_stats = crate::models::ExecutionStats {
            tool_calls,
            conversation_turns,
            thinking_count,
        };
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
                let svc_ctx = crate::service_context::ServiceContext {
                    db: db_clone.clone(),
                    executor_registry: executor_registry_spawn.clone(),
                    tx: tx_clone.clone(),
                    task_manager: task_manager_spawn.clone(),
                    config: config_spawn.clone(),
                };
                let svc = std::sync::Arc::new(crate::hooks::HookService::new(svc_ctx));
                svc.fire_for_todo(todo_id, ctx);
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
    });

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
/// 参数: (db, todo, record_id, executor_registry, tx, task_manager, config).
/// 任何错误都只记 warn 日志，不影响原 todo 的完成响应。
///
/// 实现: 由于 run_auto_review_inner 内部需要 await run_todo_execution (后者会
/// 进一步 spawn) —— 整个 future 不是 Send —— 必须在独立 runtime 上 block_on.
pub async fn run_auto_review(
    db: Arc<crate::db::Database>,
    executor_registry: Arc<crate::adapters::ExecutorRegistry>,
    tx: tokio::sync::broadcast::Sender<crate::handlers::ExecEvent>,
    task_manager: Arc<crate::task_manager::TaskManager>,
    config: Arc<tokio::sync::RwLock<crate::config::Config>>,
    todo_id: i64,
    record_id: i64,
) {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let db_c = db.clone();
    let er_c = executor_registry.clone();
    let tx_c = tx.clone();
    let tm_c = task_manager.clone();
    let cfg_c = config.clone();
    let runtime = review_runtime();
    std::thread::spawn(move || {
        let result = runtime.block_on(run_auto_review_inner(
            db_c, er_c, tx_c, tm_c, cfg_c, todo_id, record_id,
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
        }
        Err(_) => {
            tracing::warn!("auto-review thread dropped reply for todo #{} record #{}", todo_id, record_id);
            let _ = db
                .set_record_last_review_status(record_id, "failed")
                .await;
        }
    }
}

async fn run_auto_review_inner(
    db: Arc<crate::db::Database>,
    executor_registry: Arc<crate::adapters::ExecutorRegistry>,
    tx: tokio::sync::broadcast::Sender<crate::handlers::ExecEvent>,
    task_manager: Arc<crate::task_manager::TaskManager>,
    config: Arc<tokio::sync::RwLock<crate::config::Config>>,
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
        return Ok(());
    }
    let record = db.get_execution_record(record_id).await
        .map_err(|e| format!("load record: {}", e))?
        .ok_or_else(|| format!("record #{} not found", record_id))?;
    use crate::models::ExecutionStatus;
    if !matches!(record.status, ExecutionStatus::Success | ExecutionStatus::Failed) {
        let _ = db.set_record_last_review_status(record_id, "skipped").await;
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

    // 4) clone 评审实例 todo
    let review_todo_id = db.clone_todo_for_review(template_id, original.id).await
        .map_err(|e| format!("clone review instance: {}", e))?;

    // 5) 标记 pending
    let _ = db.set_record_last_review_status(record_id, "pending").await;
    let _ = db.set_record_last_reviewed_at(record_id).await;

    // 6) 同步执行评审实例
    let request = RunTodoExecutionRequest {
        db: db.clone(),
        executor_registry: executor_registry.clone(),
        tx: tx.clone(),
        task_manager: task_manager.clone(),
        config: config.clone(),
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
    // 评审实例自身 todo 也转完成
    let _ = db.finish_todo_execution(
        review_todo_id,
        matches!(review_status_str, "success"),
    ).await;

    tracing::info!(
        "auto-review done: original_todo=#{} record=#{} review_todo=#{} review_record=#{} status={} rating={:?}",
        todo_id, record_id, review_todo_id, review_record_id, review_status_str, rating
    );
    Ok(())
}

