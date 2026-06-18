//! Log capture pipeline —— stdout/stderr reader + LogFlusher + 统计提取。
//!
//! 模块职责：
//!   1. [`spawn_stdout_reader`] / [`spawn_stderr_reader`] —— 异步读子进程 stdout/stderr
//!      推入 [`crate::log_flusher::LogFlusher`]，同时 emit `Output` event
//!   2. [`setup_log_capture_pipeline`] —— 一次性装配 flusher + readers + flush timer
//!   3. [`drain_readers_and_flush`] —— cancel/timeout 分支共用的清理（杀进程 → drain → finalize）
//!   4. [`flush_and_extract_result`] —— 正常完成分支：从 DB 取日志快照 + 提取 result_str
//!   5. [`extract_execution_stats`] —— 统计 tool_calls / conversation_turns / thinking_count
//!
//! 每个函数 ≤ 30 行，编排层（completion / stages）只调用入口函数。

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::adapters::CodeExecutor;
use crate::db::Database;
use crate::handlers::ExecEvent;
use crate::log_flusher::LogFlusher;
use crate::models::ParsedLogEntry;

/// 触发 `Output` 事件的 helper：忽略 send 失败（无订阅者 = 没人想看，不算错误）。
pub(crate) fn send_event(tx: &broadcast::Sender<ExecEvent>, event: ExecEvent) {
    let _ = tx.send(event);
}

/// 启动一个 stderr reader 任务：逐行读 stderr -> 经 executor 解析 -> 推入 LogFlusher。
///
/// 返回 `None` 表示 executor 子进程根本没暴露 stderr（少见，比如某些 mock executor）。
pub fn spawn_stderr_reader<R>(
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

/// 启动 stdout reader 任务：逐行读 stdout -> 提取 session_id / todo_progress / 统计 ->
/// 推入 LogFlusher + emit Output event。
///
/// 与 stderr reader 不同，stdout 还要承担三件事：
///   1. 第一次出现 `executor.extract_session_id` 时把 session_id 回写到 execution_records；
///   2. 解析 `todo_progress` 时除写库外还要发 `TodoProgress` 事件（前端实时进度条）；
///   3. 每 10 行 或 工具调用时扫一遍 buffer 计算 stats，emit `ExecutionStats`。
/// 这三件事没法再下沉到 LogFlusher（一个是 DB 写，一个是 progress 事件），所以留在这里。
pub fn spawn_stdout_reader<R>(
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
            update_session_id_once(
                &executor_clone,
                &db_for_todo,
                rid,
                &line,
                &mut session_id_updated,
            )
            .await;
            // executor 解析失败（不是 JSONL 格式的行）就跳过；不强制 stderr 兜底。
            let Some(parsed) = executor_clone.parse_output_line(&line) else {
                continue;
            };
            // todo_progress：写库 + 发事件，让前端能实时显示进度。
            emit_todo_progress_if_present(&db_for_todo, &tx_clone, &tid, rid, &parsed).await;

            // 统计：工具调用必发；普通日志每 10 条发一次。
            // 用 with_logs 持锁只读扫描，避免与 push 路径冲突。
            log_count += 1;
            maybe_emit_execution_stats(
                &log_flusher_for_stdout,
                &tx_clone,
                &tid,
                &parsed,
                log_count,
            )
            .await;

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

/// 第一次出现 session_id 时回写 DB，后续重复出现跳过。
async fn update_session_id_once(
    executor: &Arc<dyn CodeExecutor>,
    db: &Arc<Database>,
    record_id: i64,
    line: &str,
    updated: &mut bool,
) {
    if *updated {
        return;
    }
    if let Some(sid) = executor.extract_session_id(line) {
        let _ = db.update_execution_record_session_id(record_id, &sid).await;
        *updated = true;
    }
}

/// 若 parsed 含 todo_progress 则写库 + 发事件。
async fn emit_todo_progress_if_present(
    db: &Arc<Database>,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    record_id: i64,
    parsed: &ParsedLogEntry,
) {
    let Some(progress) = crate::todo_progress::try_extract_todo_progress(parsed) else {
        return;
    };
    if let Ok(progress_json) = serde_json::to_string(&progress) {
        let _ = db
            .update_execution_record_todo_progress(record_id, &progress_json)
            .await;
    }
    send_event(
        tx,
        ExecEvent::TodoProgress {
            task_id: task_id.to_string(),
            progress,
        },
    );
}

/// 工具调用必发 stats；普通日志每 10 条发一次。
async fn maybe_emit_execution_stats(
    log_flusher: &Arc<LogFlusher>,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    parsed: &ParsedLogEntry,
    log_count: u64,
) {
    let is_tool_call = is_tool_call_log_type(&parsed.log_type);
    if !is_tool_call && !log_count.is_multiple_of(10) {
        return;
    }
    let stats = compute_execution_stats(log_flusher).await;
    send_event(
        tx,
        ExecEvent::ExecutionStats {
            task_id: task_id.to_string(),
            stats,
        },
    );
}

/// 判定一行 stdout 是否是工具调用类型（tool_use / tool_call / tool 都算）。
fn is_tool_call_log_type(log_type: &str) -> bool {
    log_type == "tool_use" || log_type == "tool_call" || log_type == "tool"
}

/// 在 LogFlusher 当前 buffer 上扫一遍，统计 tool_calls / conversation_turns / thinking_count。
async fn compute_execution_stats(
    log_flusher: &Arc<LogFlusher>,
) -> crate::models::ExecutionStats {
    log_flusher
        .with_logs(|current_logs| {
            let tool_calls = current_logs
                .iter()
                .filter(|l| is_tool_call_log_type(&l.log_type))
                .count() as u64;
            let conversation_turns = current_logs
                .iter()
                .filter(|l| {
                    l.log_type == "assistant" || l.log_type == "result" || l.log_type == "text"
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
        .await
}

/// LogFlusher 配置 + stdout/stderr reader + flush timer 一起初始化。
///
/// `SO`/`SE` 两个泛型分别对应 stdout/stderr handle 的真实类型（tokio 的 ChildStdout
/// 和 ChildStderr 是两个不同类型，没法合并成一个 `R`）。
pub async fn setup_log_capture_pipeline<SO, SE>(
    stdout_handle: Option<SO>,
    stderr_handle: Option<SE>,
    executor: Arc<dyn CodeExecutor>,
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
/// `log_flusher` 接 `Arc<LogFlusher>` 而不是 `&LogFlusher`，因为 `finalize` 的签名
/// 是 `async fn finalize(self: Arc<Self>)`，需要拿所有权才能走完 drain 流程。
pub async fn drain_readers_and_flush(
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

/// 收口「finalize flusher + 取日志快照 + 抽 result_str」三步。
///
/// 读日志是从 DB 拉最近一次 finalize 后的全量行；result_str 是 executor 子类用启发式规则
/// 从日志末尾抽出来的「最终结论」字符串。
///
/// `log_flusher` 接 `Arc<LogFlusher>` 而不是 `&LogFlusher`，因为 `finalize` 的签名
/// 是 `fn finalize(self: Arc<Self>)`——必须 move 走 Arc 才能在内部触发 flush 收尾。
pub async fn flush_and_extract_result(
    log_flusher: Arc<LogFlusher>,
    flush_timer: JoinHandle<()>,
    db: &Arc<Database>,
    record_id: i64,
    executor: &dyn CodeExecutor,
) -> (Vec<ParsedLogEntry>, String) {
    log_flusher.finalize().await;
    let _ = flush_timer.await;
    let all_logs_snapshot = db.get_all_execution_logs(record_id).await.unwrap_or_default();
    let result_str = executor
        .get_final_result(&all_logs_snapshot)
        .unwrap_or_default();
    (all_logs_snapshot, result_str)
}

/// 从日志中提取执行统计信息。
///
/// 单次遍历日志，计算 tool_calls、conversation_turns、thinking_count。
/// 如果 executor 提供了自己的 tool_calls_count，则使用 executor 的值（更准确）。
pub fn extract_execution_stats(
    logs: &[ParsedLogEntry],
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

/// 等待 stdout/stderr reader 跑完，回收子任务句柄。
///
/// 子进程已自然退出，stdout/stderr 管道已关闭；reader 协程会在管道 EOF 时自然
/// 退出。`let _ = handle.await` 故意忽略 JoinError（reader panic 不影响主流程）。
pub async fn await_readers(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tool_call_log_type() {
        assert!(is_tool_call_log_type("tool_use"));
        assert!(is_tool_call_log_type("tool_call"));
        assert!(is_tool_call_log_type("tool"));
        assert!(!is_tool_call_log_type("assistant"));
        assert!(!is_tool_call_log_type("thinking"));
    }

    #[test]
    fn test_extract_execution_stats() {
        let logs = vec![
            ParsedLogEntry {
                log_type: "tool_use".to_string(),
                ..Default::default()
            },
            ParsedLogEntry {
                log_type: "assistant".to_string(),
                ..Default::default()
            },
            ParsedLogEntry {
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
        let logs = vec![ParsedLogEntry {
            log_type: "tool_use".to_string(),
            ..Default::default()
        }];

        let stats = extract_execution_stats(&logs, Some(5));
        assert_eq!(stats.tool_calls, 5); // overridden
        assert_eq!(stats.conversation_turns, 0);
        assert_eq!(stats.thinking_count, 0);
    }
}