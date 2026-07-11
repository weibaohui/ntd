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
use crate::execution_events::{
    AtomcodeExtractor, ClaudeCodeExtractor, CodebuddyExtractor, CodewhaleExtractor,
    CodexExtractor, DbLogEntry, EventPipeline, ExecutionEvent,
    HermesExtractor, KiloExtractor, KimiExtractor, MimoExtractor, MobilecoderExtractor,
    OpencodeExtractor, PiExtractor, ZhanluExtractor,
};
use crate::executor_service::ExecEvent;
use crate::log_flusher::LogFlusher;
use crate::models::{ExecutorType, ParsedLogEntry};

/// 触发 `Output` 事件的 helper：忽略 send 失败（无订阅者 = 没人想看，不算错误）。
pub(crate) fn send_event(tx: &broadcast::Sender<ExecEvent>, event: ExecEvent) {
    let _ = tx.send(event);
}

/// 根据执行器类型创建对应的 EventPipeline（含专用提取器）
///
/// pub(crate) 是为了让 message_debounce.rs 中的 executor 默认响应路径也能复用
/// 同一套 pipeline 创建逻辑，避免每个调用方各自硬编码 executor 类型 → 提取器映射。
pub(crate) fn create_pipeline_for_executor(executor: &dyn CodeExecutor) -> Option<EventPipeline> {
    let executor_type = executor.executor_type();

    // 根据执行器类型选择合适的提取器
    let pipeline = match executor_type {
        ExecutorType::Claudecode => {
            EventPipeline::with_extractor(ClaudeCodeExtractor::new())
        }
        ExecutorType::Kilo => {
            EventPipeline::with_extractor(KiloExtractor::new())
        }
        ExecutorType::Opencode => {
            EventPipeline::with_extractor(OpencodeExtractor::new())
        }
        ExecutorType::Codex => {
            EventPipeline::with_extractor(CodexExtractor::new())
        }
        ExecutorType::Hermes => {
            EventPipeline::with_extractor(HermesExtractor::new())
        }
        ExecutorType::Kimi => {
            EventPipeline::with_extractor(KimiExtractor::new())
        }
        ExecutorType::Pi => {
            EventPipeline::with_extractor(PiExtractor::new())
        }
        ExecutorType::Mobilecoder => {
            EventPipeline::with_extractor(MobilecoderExtractor::new())
        }
        ExecutorType::Atomcode => {
            EventPipeline::with_extractor(AtomcodeExtractor::new())
        }
        ExecutorType::Zhanlu => {
            EventPipeline::with_extractor(ZhanluExtractor::new())
        }
        ExecutorType::Codewhale => {
            EventPipeline::with_extractor(CodewhaleExtractor::new())
        }
        ExecutorType::Mimo => {
            EventPipeline::with_extractor(MimoExtractor::new())
        }
        ExecutorType::Codebuddy => {
            EventPipeline::with_extractor(CodebuddyExtractor::new())
        }
    };

    Some(pipeline)
}

/// 将 ExecutionEvent 转换为 ParsedLogEntry 并发送 Output 事件
///
/// 返回转换后的 ParsedLogEntry，供 LogFlusher 使用。
/// workspace_id：执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标，
/// 必须贯穿到每个事件发送路径，否则推送服务无法匹配到对应的推送目标。
///
/// pub(crate) 是为了让 message_debounce.rs 中的 executor 默认响应路径在 finalize
/// pipeline 时也能复用同一套事件发送逻辑。
pub(crate) fn emit_execution_event(
    event: &ExecutionEvent,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    workspace_id: Option<i64>,
) -> ParsedLogEntry {
    let parsed = DbLogEntry::from_event_to_parsed_log_entry(event);
    send_event(
        tx,
        ExecEvent::Output {
            task_id: task_id.to_string(),
            entry: parsed.clone(),
            workspace_id,
        },
    );
    parsed
}

/// 尝试用 EventPipeline 解析一行，返回所有新事件的 ParsedLogEntry 列表
///
/// 如果 EventPipeline 没有产生有效事件，返回空 Vec。
/// workspace_id：执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标，
/// 必须贯穿到每个事件发送路径，否则推送服务无法匹配到对应的推送目标。
///
/// pub(crate) 是为了让 message_debounce.rs 中的 executor 默认响应路径复用同一套解析逻辑，
/// 确保 executor 直连执行与 todo 执行产生完全相同格式的事件。
pub(crate) fn try_parse_with_pipeline(
    pipeline: &mut EventPipeline,
    line: &str,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    workspace_id: Option<i64>,
) -> Vec<ParsedLogEntry> {
    let line_trimmed = line.trim();
    if line_trimmed.is_empty() {
        return Vec::new();
    }

    // 记录 feed 前的事件数，便于取出本次新增的所有事件
    let len_before = pipeline.len();

    // 用 pipeline 处理
    pipeline.feed(line_trimmed);

    // 取出本次新增的所有事件
    let new_events: Vec<&ExecutionEvent> = pipeline.events()[len_before..].iter().collect();
    if new_events.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();
    for event in &new_events {
        match event {
            ExecutionEvent::Info { message } => {
                // 空的或纯 JSON 行作为 info，不转发
                if message.starts_with('{') || message.is_empty() {
                    continue;
                }
                // 非 JSON 的普通 info 也转发
                let parsed = emit_execution_event(event, tx, task_id, workspace_id);
                results.push(parsed);
            }
            ExecutionEvent::Error { .. }
            | ExecutionEvent::Thinking { .. }
            | ExecutionEvent::ToolCall { .. }
            | ExecutionEvent::ToolResult { .. }
            | ExecutionEvent::Assistant { .. }
            | ExecutionEvent::Result { .. }
            | ExecutionEvent::SessionStart { .. }
            | ExecutionEvent::Tokens { .. }
            | ExecutionEvent::Cost { .. }
            | ExecutionEvent::Duration { .. }
            | ExecutionEvent::StepStart { .. }
            | ExecutionEvent::StepFinish { .. } => {
                let parsed = emit_execution_event(event, tx, task_id, workspace_id);
                results.push(parsed);
            }
            // SessionEnd 由 pipeline.finalize() 生产，避免重复转发
            ExecutionEvent::SessionEnd { .. }
            | ExecutionEvent::Progress { .. } => {}
            // ModelSwitch 需转发到 DB，否则 completion 阶段 get_model_from_logs 找不到模型
            ExecutionEvent::ModelSwitch { .. } => {
                let parsed = emit_execution_event(event, tx, task_id, workspace_id);
                results.push(parsed);
            }
            // 其他类型不转发
            ExecutionEvent::User { .. }
            | ExecutionEvent::System { .. } => {}
        }
    }
    results
}

/// 尝试用 EventPipeline 解析 stderr 行，返回所有新事件的 ParsedLogEntry 列表
/// workspace_id：执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标，
/// 必须贯穿到每个事件发送路径，否则推送服务无法匹配到对应的推送目标。
fn try_parse_stderr_with_pipeline(
    pipeline: &mut EventPipeline,
    line: &str,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    workspace_id: Option<i64>,
) -> Vec<ParsedLogEntry> {
    let line_trimmed = line.trim();
    if line_trimmed.is_empty() {
        return Vec::new();
    }

    let len_before = pipeline.len();
    // atomcode 的 stderr 与 stdout 格式相同（[xx] 结构化行 + 纯文本混合），
    // 使用 extract() 而非 extract_stderr()，确保 flush_text 的多事件不丢失
    if pipeline.metadata().executor == "atomcode" {
        pipeline.feed(line_trimmed);
    } else {
        pipeline.feed_stderr(line_trimmed);
    }

    pipeline.events()[len_before..]
        .iter()
        .map(|event| emit_execution_event(event, tx, task_id, workspace_id))
        .collect()
}

/// 启动一个 stderr reader 任务：逐行读 stderr -> 经 executor 解析 -> 推入 LogFlusher。
///
/// 返回 `None` 表示 executor 子进程根本没暴露 stderr（少见，比如某些 mock executor）。
/// workspace_id：执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标，
/// 必须贯穿到每个事件发送路径，否则推送服务无法匹配到对应的推送目标。
pub(crate) fn spawn_stderr_reader<R>(
    stderr_handle: Option<R>,
    executor: Arc<dyn CodeExecutor>,
    log_flusher: Arc<LogFlusher>,
    tx: broadcast::Sender<ExecEvent>,
    task_id: String,
    workspace_id: Option<i64>,
) -> Option<JoinHandle<()>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    stderr_handle.map(|stderr_reader| {
        tokio::spawn(async move {
            // 创建 EventPipeline 用于结构化解析
            let mut pipeline = create_pipeline_for_executor(executor.as_ref())
                .unwrap_or_else(|| EventPipeline::new(executor.executor_type().as_str()));

            // BufReader::lines 在读到 EOF 时返回 Ok(None)，循环自然退出。
            let mut reader = BufReader::new(stderr_reader).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                // 跳过 atomcode 流式/中间过程输出行
                let t = line.trim_start();
                if t.starts_with("[tool-streaming") || t.starts_with("[tool-batch") {
                    continue;
                }
                // 优先尝试用 EventPipeline 解析
                let parsed_list =
                    try_parse_stderr_with_pipeline(&mut pipeline, &line, &tx, &task_id, workspace_id);

                if !parsed_list.is_empty() {
                    for parsed in parsed_list {
                        log_flusher.push(parsed).await;
                    }
                    continue;
                }

                // 跳过 [thinking] 原始行（atomcode 已在 EventPipeline 中聚合），避免 raw 行混入日志
                if line.trim_start().starts_with("[thinking") {
                    continue;
                }

                // 回退到 executor 自定义解析
                // 跳过空行：parse_stderr_line 对空行返回 None 但 unwrap_or_else 仍会创建 stderr 条目
                if line.trim().is_empty() {
                    continue;
                }
                let entry = executor
                    .parse_stderr_line(&line)
                    .unwrap_or_else(|| ParsedLogEntry::stderr(line.clone()));
                log_flusher.push(entry.clone()).await;
                send_event(
                    &tx,
                    ExecEvent::Output {
                        task_id: task_id.clone(),
                        entry,
                        workspace_id,
                    },
                );
            }

            // 循环结束，finalize pipeline 并 flush 剩余事件
            let len_before = pipeline.len();
            pipeline.finalize();
            for event in &pipeline.events()[len_before..] {
                let parsed = emit_execution_event(event, &tx, &task_id, workspace_id);
                log_flusher.push(parsed).await;
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
///
/// 这三件事没法再下沉到 LogFlusher（一个是 DB 写，一个是 progress 事件），所以留在这里。
///
/// workspace_id：执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标，
/// 必须贯穿到每个事件发送路径，否则推送服务无法匹配到对应的推送目标。
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_stdout_reader<R>(
    stdout_handle: Option<R>,
    executor: &Arc<dyn CodeExecutor>,
    db: &Arc<Database>,
    log_flusher: &Arc<LogFlusher>,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    record_id: i64,
    workspace_id: Option<i64>,
    initial_session_id: Option<String>,
) -> Option<JoinHandle<()>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let stdout_reader = stdout_handle?;
    // 克隆 Arc/Sender 等引用计数类型以移入 async block；clone 开销仅为原子加，不影响性能
    let tx_clone = tx.clone();
    let executor_clone = executor.clone();
    let db_for_todo = db.clone();
    let log_flusher_for_stdout = log_flusher.clone();
    let tid = task_id.to_string();
    let rid = record_id;
    let wid = workspace_id;

    Some(tokio::spawn(async move {
        // 创建 EventPipeline 用于结构化解析
        let mut pipeline = create_pipeline_for_executor(executor_clone.as_ref())
            .unwrap_or_else(|| EventPipeline::new(executor_clone.executor_type().as_str()));

        let mut reader = BufReader::new(stdout_reader).lines();
        let mut log_count = 0u64;
        // session_id 只更新一次：避免每次重复出现 session_id 时反复触发 DB UPDATE。
        // resume 场景下 DB 已有正确 session_id，跳过覆盖。
        let mut session_id_updated = initial_session_id.is_some();

        while let Ok(Some(line)) = reader.next_line().await {
            // 优先尝试用 EventPipeline 解析
            let parsed_list =
                try_parse_with_pipeline(&mut pipeline, &line, &tx_clone, &tid, wid);

            if !parsed_list.is_empty() {
                for parsed in parsed_list {
                    // 从 EventPipeline 的元数据获取 session_id 并更新 DB
                    if !session_id_updated {
                        if let Some(sid) = pipeline.metadata().session_id.clone() {
                            let _ = db_for_todo
                                .update_execution_record_session_id(rid, &sid)
                                .await;
                            session_id_updated = true;
                        }
                    }

                    // todo_progress：写库 + 发事件
                    emit_todo_progress_if_present(&db_for_todo, &tx_clone, &tid, rid, &parsed, wid).await;

                    // 统计：工具调用必发；普通日志每 10 条发一次。
                    log_count += 1;
                    maybe_emit_execution_stats(
                        &log_flusher_for_stdout,
                        &tx_clone,
                        &tid,
                        &parsed,
                        log_count,
                        wid,
                    )
                    .await;

                    // 推入 flusher：内部 CAS 触发后台 flush。
                    log_flusher_for_stdout.push(parsed).await;
                }
                continue;
            }

            // 回退到 executor 自定义解析
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
            emit_todo_progress_if_present(&db_for_todo, &tx_clone, &tid, rid, &parsed, wid).await;

            // 统计：工具调用必发；普通日志每 10 条发一次。
            // 用 with_logs 持锁只读扫描，避免与 push 路径冲突。
            log_count += 1;
            maybe_emit_execution_stats(
                &log_flusher_for_stdout,
                &tx_clone,
                &tid,
                &parsed,
                log_count,
                wid,
            )
            .await;

            // 推入 flusher：内部 CAS 触发后台 flush。
            log_flusher_for_stdout.push(parsed.clone()).await;
            send_event(
                &tx_clone,
                ExecEvent::Output {
                    task_id: tid.clone(),
                    entry: parsed,
                    workspace_id: wid,
                },
            );
        }

        // 循环结束，finalize pipeline 并 flush 剩余事件（SessionEnd 等）
        let len_before = pipeline.len();
        pipeline.finalize();
        for event in &pipeline.events()[len_before..] {
            let parsed = emit_execution_event(event, &tx_clone, &tid, wid);
            log_flusher_for_stdout.push(parsed).await;
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
/// workspace_id：执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标，
/// 必须贯穿到每个事件发送路径，否则推送服务无法匹配到对应的推送目标。
async fn emit_todo_progress_if_present(
    db: &Arc<Database>,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    record_id: i64,
    parsed: &ParsedLogEntry,
    workspace_id: Option<i64>,
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
            workspace_id,
        },
    );
}

/// 工具调用必发 stats；普通日志每 10 条发一次。
/// workspace_id：执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标，
/// 必须贯穿到每个事件发送路径，否则推送服务无法匹配到对应的推送目标。
async fn maybe_emit_execution_stats(
    log_flusher: &Arc<LogFlusher>,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    parsed: &ParsedLogEntry,
    log_count: u64,
    workspace_id: Option<i64>,
) {
    let is_tool_call = is_tool_call_log_type(&parsed.log_type);
    // is_multiple_of 需要 MSRV 1.87.0，用取模运算兼容 1.81.0
    if !is_tool_call && log_count % 10 != 0 {
        return;
    }
    let stats = compute_execution_stats(log_flusher).await;
    send_event(
        tx,
        ExecEvent::ExecutionStats {
            task_id: task_id.to_string(),
            stats,
            workspace_id,
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
/// workspace_id：执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标，
/// 必须贯穿到每个事件发送路径，否则推送服务无法匹配到对应的推送目标。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn setup_log_capture_pipeline<SO, SE>(
    stdout_handle: Option<SO>,
    stderr_handle: Option<SE>,
    executor: Arc<dyn CodeExecutor>,
    db: Arc<Database>,
    tx: broadcast::Sender<ExecEvent>,
    task_id: String,
    record_id: i64,
    workspace_id: Option<i64>,
    initial_session_id: Option<String>,
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
    // 传递引用避免不必要的 Arc 克隆；函数内部仅在 async block 需要所有权时才 clone
    let stdout_task = spawn_stdout_reader(
        stdout_handle,
        &executor,
        &db,
        &log_flusher,
        &tx,
        &task_id,
        record_id,
        workspace_id,
        initial_session_id,
    );
    // tx/task_id 在此调用后不再使用，直接 move 避免多余克隆
    let stderr_task = spawn_stderr_reader(
        stderr_handle,
        executor.clone(),
        log_flusher.clone(),
        tx,
        task_id,
        workspace_id,
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
pub(crate) async fn drain_readers_and_flush(
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
pub(crate) async fn flush_and_extract_result(
    log_flusher: Arc<LogFlusher>,
    flush_timer: JoinHandle<()>,
    db: &Arc<Database>,
    record_id: i64,
) -> (Vec<ParsedLogEntry>, String) {
    log_flusher.finalize().await;
    let _ = flush_timer.await;
    let all_logs_snapshot = db.get_all_execution_logs(record_id).await.unwrap_or_default();
    let result_str = super::completion::get_final_result_from_logs(&all_logs_snapshot)
        .unwrap_or_default();
    (all_logs_snapshot, result_str)
}

/// 从日志中提取执行统计信息。
///
/// 单次遍历日志，计算 tool_calls、conversation_turns、thinking_count。
/// 如果 executor 提供了自己的 tool_calls_count，则使用 executor 的值（更准确）。
pub(crate) fn extract_execution_stats(
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
pub(crate) async fn await_readers(
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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