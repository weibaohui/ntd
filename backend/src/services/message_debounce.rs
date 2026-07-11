use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::adapters::parse_executor_type;
use crate::db::Database;
use crate::execution_events::EventPipeline;
use crate::executor_service::log_capture;
use crate::executor_service::{
    run_todo_execution, run_todo_execution_with_params, RunTodoExecutionRequest,
};
use crate::executor_service::ExecEvent;
use crate::models::ParsedLogEntry;
use crate::service_context::ServiceContext;
use crate::task_manager::TaskManager;

#[derive(Debug, Clone)]
pub struct PendingMessage {
    pub bot_id: i64,
    pub chat_id: String,
    pub chat_type: String,
    pub sender: String,
    pub content: String,
    pub todo_id: i64,
    pub todo_prompt: String,
    pub executor: Option<String>,
    pub trigger_type: String,
    pub params: Option<HashMap<String, String>>,
    pub message_id: Option<String>,
    /// For project-bound resume: the session_id to resume
    pub resume_session_id: Option<String>,
    /// For project-bound resume: the message content as resume_message
    pub resume_message: Option<String>,
    /// feishu_project_bindings.id — set when this message comes from a bound chat
    pub binding_id: Option<i64>,
    /// 工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
    pub workspace_id: Option<i64>,
    /// 群聊 @提及的显式请求：跳过 debounce 等待，收到立即执行。
    /// 非 @mention 的群聊消息仍走 debounce 合并窗口。
    pub immediate: bool,
}

struct DebounceEntry {
    messages: Vec<PendingMessage>,
    timer: JoinHandle<()>,
}

pub struct MessageDebounce {
    entries: Arc<DashMap<(i64, String), DebounceEntry>>,
    ctx: ServiceContext,
    /// Loop Runner，用于处理 default_response_loop 类型的消息
    loop_runner: Option<Arc<crate::services::loop_runner::LoopRunner>>,
}

impl MessageDebounce {
    pub fn new(
        ctx: ServiceContext,
        loop_runner: Option<Arc<crate::services::loop_runner::LoopRunner>>,
    ) -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            ctx,
            loop_runner,
        }
    }

    /// Push a message into the debounce buffer. Resets the timer for this key.
    pub fn push(&self, msg: PendingMessage) {
        let key = (msg.bot_id, msg.chat_id.clone());

        // Remove old entry and collect existing messages
        let mut all_msgs = self
            .entries
            .remove(&key)
            .map(|(_, old)| {
                old.timer.abort();
                old.messages
            })
            .unwrap_or_default();
        all_msgs.push(msg);

        // @提及是显式点名请求，跳过 debounce 等待立即执行；
        // 在 all_msgs move 进闭包前先计算好。
        let has_immediate = all_msgs.iter().any(|m| m.immediate);

        // Create new timer
        let new_timer = {
            let entries = self.entries.clone();
            let db = self.ctx.db.clone();
            let executor_registry = self.ctx.executor_registry.clone();
            let tx = self.ctx.tx.clone();
            let task_manager = self.ctx.task_manager.clone();
            let config = self.ctx.config.clone();
            // loop_runner 需要在 async block 之前 clone，避免 self 生命周期问题
            let loop_runner = self.loop_runner.clone();
            // todo hook 已整块移除（plan `purring-forging-petal`），debounce 触发的
            // 执行不再需要透传 hook_service。
            let bot_id = key.0;
            let chat_id = key.1.clone();
            let target_type = all_msgs
                .first()
                .map(|m| m.chat_type.clone())
                .unwrap_or_default();

            tokio::spawn(async move {
                // 群聊需要 debounce 等待窗口，避免多条消息触发多次执行；
                // 但 @提及是显式点名请求，跳过等待立即执行。
                if target_type == "group" && !has_immediate {
                    let secs = db
                        .get_debounce_secs(bot_id, &target_type)
                        .await
                        .unwrap_or(20)
                        .max(1);
                    tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await;
                }

                // Timer fired: drain all pending messages for this key
                let key = (bot_id, chat_id);
                let pending = entries.remove(&key);
                if let Some((_, entry)) = pending {
                    if entry.messages.is_empty() {
                        return;
                    }

                    let merged_content = merge_pending_messages(&entry.messages);

                    // entry.messages 在上面已确认非空（is_empty 检查），last() 必然有值
                    let Some(last) = entry.messages.last() else { return; };
                    let mut merged_params = last.params.clone().unwrap_or_default();
                    merged_params.insert("content".to_string(), merged_content.clone());
                    merged_params.insert("message".to_string(), merged_content.clone());

                    // For resume sessions: use the user's content as the message to resume with
                    let resume_msg = last.resume_message.clone();
                    let mut resume_sid = last.resume_session_id.clone();

                    // 防御 TOCTOU：debounce 等待期间 binding 可能被重新绑定到不同项目（todo_id 变了）
                    // todo_id 变了才降级；只要有 session_id 就应该继续多轮对话
                    if resume_sid.is_some() {
                        if let Some(binding_id) = last.binding_id {
                            if let Ok(Some(binding)) = db.get_feishu_project_binding_by_id(binding_id).await {
                                let todo_changed = binding.todo_id != last.todo_id;
                                if todo_changed {
                                    tracing::warn!(
                                        "[debounce] binding {} todo_id changed ({} → {}), dropping resume",
                                        binding_id, last.todo_id, binding.todo_id
                                    );
                                    // Todo 变了，降级为新执行
                                    resume_sid = None;
                                }
                            }
                        }
                    }

                    let exec_message = if resume_sid.is_some() {
                        // resume: include system prompt with user content so Claude retains project context
                        last.todo_prompt.replace("{{message}}", &merged_content)
                    } else {
                        // new execution: send todo_prompt with params (replace_placeholders will substitute {{message}})
                        last.todo_prompt.clone()
                    };

                    // Clone before move: resume_sid is consumed by the request below,
                    // but we still need it for the TOCTOU-correct binding update after.
                    let is_resume = resume_sid.is_some();
                    let sid_for_binding = resume_sid.clone();

                    // 根据 trigger_type 分发到不同的处理函数
                    // 错误类型为 Option<String>：Some("loop_paused") 表示环路暂停，None 表示其他错误
                    let result: Result<crate::executor_service::ExecutionResult, Option<String>> = match last.trigger_type.as_str() {
                        "default_response_loop" | "slash_command_loop" => {
                            // 环路默认响应 或 斜杠命令触发环路：直接触发环路执行
                            // 群聊回复到群（chat_id），私聊回复到个人（open_id）
                            let (loop_receive_id, receive_id_type) = if last.chat_type == "group" {
                                (last.chat_id.clone(), "chat_id".to_string())
                            } else {
                                (last.sender.clone(), "open_id".to_string())
                            };
                            Self::handle_default_response_loop(
                                db.clone(),
                                loop_runner.clone(),
                                last.todo_id, // loop_id
                                &merged_content,
                                Some(last.bot_id),
                                Some(loop_receive_id),
                                Some(receive_id_type),
                            )
                            .await
                        }
                        "default_response_executor" => {
                            // 执行器默认响应：直接调用执行器交互（不存储执行记录）
                            // 群聊回复到群（chat_id），私聊回复到个人（open_id）
                            let (resp_receive_id, resp_receive_id_type) = if last.chat_type == "group" {
                                (last.chat_id.clone(), "chat_id".to_string())
                            } else {
                                (last.sender.clone(), "open_id".to_string())
                            };
                            Self::handle_default_response_executor(
                                &db,
                                &executor_registry,
                                &task_manager,
                                &config,
                                &tx,
                                last.bot_id,
                                resp_receive_id,
                                &resp_receive_id_type,
                                last.executor.as_deref(),
                                last.workspace_id,
                                &merged_content,
                                resume_sid.clone(),
                            )
                            .await
                        }
                        _ => {
                            // 普通的默认响应（todo 类型）或斜杠命令
                            let request = RunTodoExecutionRequest {
                                db: db.clone(),
                                executor_registry,
                                tx,
                                task_manager,
                                config,
                                todo_id: last.todo_id,
                                message: exec_message,
                                req_executor: last.executor.clone(),
                                trigger_type: last.trigger_type.clone(),
                                params: if is_resume { None } else { Some(merged_params) },
                                resume_session_id: resume_sid,
                                resume_message: resume_msg,
                                source_todo_id: None,
                                source_todo_title: None,
                                loop_step_execution_id: None,
                                step_id: None,
                                feishu_bot_id: Some(last.bot_id),
                                // 根据 chat_type 决定回复目标：群聊回复到群（chat_id），
                                // 私聊回复到个人（open_id）。
                                feishu_receive_id: if last.chat_type == "group" {
                                    Some(last.chat_id.clone())
                                } else {
                                    Some(last.sender.clone())
                                },
                                feishu_receive_id_type: if last.chat_type == "group" {
                                    Some("chat_id".to_string())
                                } else {
                                    Some("open_id".to_string())
                                },
                                workspace_path: None,
                                workspace_id: last.workspace_id,
                            };
                            let result = if request.params.is_some() {
                                run_todo_execution_with_params(request).await
                            } else {
                                run_todo_execution(request).await
                            };
                            Ok(result)
                        }
                    };

                    let record_id = match &result {
                        Ok(r) => Some(r.record_id),
                        Err(_) => None,
                    };
                    tracing::debug!("[debounce] timer fired for bot_id={}, chat_id={}, msg_count={}, record_id={:?}", bot_id, key.1, entry.messages.len(), record_id);
                    // 执行结果处理：
                    // - 成功：更新 binding 状态为 running，记录 session_id + latest_record_id
                    // - 失败：重置 binding 状态为 idle（让下次消息尝试开新 session）
                    // session_id 策略（重要）：
                    //   - 首次执行（resume_session_id=None）：不设 session_id！task_id 是随机 UUID，
                    //     Claude Code 的真实 session_id 来自 stdout JSONL 的 system 消息，
                    //     保存在 execution_records.session_id 中。listener 的 resume 决策从那里读取。
                    //   - resume 执行（resume_session_id=Some）：保持原 session_id 不变（同一个 Claude Code 会话）
                    match result {
                        Ok(exec_result) => {
                            // If this message came from a project-bound chat, update binding state
                            if let Some(binding_id) = last.binding_id {
                                if let Some(rid) = exec_result.record_id {
                                    if is_resume {
                                        // Resume: preserve session_id (from sid_for_binding), update latest_record_id + status
                                        // is_resume is post-TOCTOU, so if todo_id changed it will be false
                                        if let Err(e) = db
                                            .update_feishu_project_binding_session(
                                                binding_id,
                                                sid_for_binding.as_deref(),
                                                rid,
                                                crate::models::binding_status::RUNNING,
                                            )
                                            .await
                                        {
                                            tracing::warn!(
                                                "[debounce] failed to update binding {} session on resume: {:?}",
                                                binding_id, e
                                            );
                                        }
                                    } else {
                                        // First execution: save real session_id from execution record
                                        // so subsequent messages can resume this session.
                                        let real_sid = db.get_execution_record(rid).await
                                            .ok()
                                            .flatten()
                                            .and_then(|r| r.session_id);
                                        if let Err(e) = db
                                            .update_feishu_project_binding_session(
                                                binding_id,
                                                real_sid.as_deref(),
                                                rid,
                                                crate::models::binding_status::RUNNING,
                                            )
                                            .await
                                        {
                                            tracing::warn!(
                                                "[debounce] failed to update binding {} session on first exec: {:?}",
                                                binding_id, e
                                            );
                                        }
                                    }
                                } else {
                                    // Record ID missing: still update status
                                    if let Err(e) = db
                                        .update_feishu_project_binding_status(binding_id, crate::models::binding_status::RUNNING)
                                        .await
                                    {
                                        tracing::warn!(
                                            "[debounce] failed to update binding {} status: {:?}",
                                            binding_id, e
                                        );
                                    }
                                }
                            }

                            // Update all pending messages with todo_id and execution_record_id
                            let record_id = exec_result.record_id;
                            for msg in &entry.messages {
                                if let Some(ref msg_id) = msg.message_id {
                                    if let Err(e) = db
                                        .mark_feishu_message_processed(
                                            msg_id,
                                            msg.todo_id,
                                            record_id,
                                            Some(&msg.trigger_type),
                                        )
                                        .await
                                    {
                                        tracing::warn!("[debounce] failed to mark message {} as processed: {:?}", msg_id, e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "[debounce] failed to execute todo {}: {:?}",
                                last.todo_id,
                                e
                            );
                            // Reset binding status to idle on failure
                            if let Some(binding_id) = last.binding_id {
                                let _ = db
                                    .update_feishu_project_binding_status(binding_id, crate::models::binding_status::IDLE)
                                    .await;
                            }
                            // e is Option<String>: Some("loop_paused") for paused loops, None for other errors
                            for msg in &entry.messages {
                                if let Some(ref msg_id) = msg.message_id {
                                    if let Some(ref error_reason) = e {
                                        // 环路暂停：标记为已处理 + 记录错误
                                        if let Err(mark_err) = db
                                            .mark_feishu_message_processed_with_error(
                                                msg_id,
                                                msg.todo_id,
                                                Some(&msg.trigger_type),
                                                error_reason,
                                            )
                                            .await
                                        {
                                            tracing::warn!("[debounce] failed to mark message {} as processed_with_error: {:?}", msg_id, mark_err);
                                        }
                                    } else {
                                        // 其他错误：标记为未处理
                                        if let Err(mark_err) = db
                                            .mark_feishu_message_failed(msg_id)
                                            .await
                                        {
                                            tracing::warn!("[debounce] failed to mark message {} as failed: {:?}", msg_id, mark_err);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            })
        };

        self.entries.insert(
            key,
            DebounceEntry {
                messages: all_msgs,
                timer: new_timer,
            },
        );
    }

    pub fn pending_count(&self) -> usize {
        self.entries.iter().map(|e| e.messages.len()).sum()
    }
}

/// 把一个 chat 在 debounce 窗口里攒下来的所有消息合并成一段文本。
///
/// 规则: 消息之间用 `\n---\n` 分隔(飞书用户复制粘贴的多段对话可读性最好,
///  Claude 也习惯用 `---` 识别章节边界)。原始消息里的换行不会做进一步处理,
/// 因为 `{{message}}` 替换的目标 prompt 大多有自己的格式。
///
/// 这是纯函数,只读 `messages` 字段,不触发任何 I/O —— `push` 在 debounce
/// 窗口到期时调用它,网络/DB 调用全部留在外面。
pub fn merge_pending_messages(messages: &[PendingMessage]) -> String {
    messages
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<&str>>()
        .join("\n---\n")
}

// ============================================================================
// 执行器直接响应的反馈消息格式化 + 超时运行辅助
// ============================================================================
//
// `handle_default_response_executor` 原先把"等子进程退出"和"发飞书消息"耦合在一起，
// 且用 `wait_with_output()` 无超时地等——provider 挂起时整条 debounce 任务永久卡死，
// 用户侧表现为"发了消息毫无反馈"。这一组纯函数/小函数把可测的逻辑抽出来：
// 三类反馈消息（开始/错误/空结束）的格式 + 带超时地运行子进程并在超时时 kill 回收。

/// 把 config 里的 `execution_timeout_secs` 解析成直连执行器要用的超时。
///
/// 飞书直连执行器与 todo 执行路径共用同一把全局超时旋钮（`execution_timeout_secs`），
/// 不再维护独立的写死阈值。`0` 在 todo pipeline 里表示「不限制」
/// （`timeout_enabled = v > 0`），这里遵循同一语义：`0` → `None`，走无超时等待分支。
/// 原因是 `tokio::time::timeout(Duration::from_secs(0), ..)` 会立刻返回 `Elapsed`，
/// 若把 0 直接当超时传进去，所有直连调用会被瞬间判死。
fn direct_executor_timeout(secs: u64) -> Option<std::time::Duration> {
    if secs == 0 {
        None
    } else {
        Some(std::time::Duration::from_secs(secs))
    }
}

/// 开始执行时发回飞书的标志文本。
///
/// 复用全仓 `status_icon`（`cli/commands.rs`）的 ⏳ 约定。preview 只取前 30 个
/// Unicode scalar 并加省略号：用户发来的 prompt 可能很长，原样刷到飞书会话里
/// 是噪声；这里只是让用户知道"开始处理了 + 处理的是哪段话"。
/// 按 char 而非 byte 截断，避免把多字节中文切成乱码。
fn executor_start_message(executor_type: &str, message_preview: &str) -> String {
    const PREVIEW_CHAR_LIMIT: usize = 30;
    let preview: String = message_preview.chars().take(PREVIEW_CHAR_LIMIT).collect();
    // 取到的长度等于原文长度说明没被截断，不加省略号；否则加 … 提示被裁了
    let suffix = if message_preview.chars().count() > PREVIEW_CHAR_LIMIT { "…" } else { "" };
    format!("⏳ {} 开始处理：{}{}", executor_type, preview, suffix)
}

/// 执行失败时发回飞书的错误文本，原因原样透传。
///
/// 调用方传入具体原因（超时秒数 / spawn 失败 / wait 失败 / 非零退出码+输出片段），
/// 让用户侧不再是静默失败，而是明确知道"执行器挂了 + 挂在哪一步"。
fn executor_error_message(executor_type: &str, reason: &str) -> String {
    format!("❌ {} 执行失败：{}", executor_type, reason)
}

/// 执行成功但没有任何输出时的结束标志。
///
/// 【待清理】已废弃：结束消息统一使用"✅ <执行器名称> 处理完成"，不再区分有无输出。
/// 保留此函数供参考，确认无用后可删除。
#[allow(dead_code)]
fn executor_empty_end_message(executor_type: &str) -> String {
    format!("✅ {} 执行完成（无输出）", executor_type)
}

/// 根据执行结果决定发回飞书的最终内容。
///
/// 成功时统一返回简洁的结束标志（"✅ <执行器名称> 处理完成"），不再重复输出
/// result_text 或 stdout（过程消息已通过 ExecutorDirectOutput 实时推送，避免重复）。
/// 失败时返回错误消息 + 输出片段，方便诊断问题。
/// 把这段决策从 `handle_default_response_executor` 主体抽成纯函数，是为了能直接单测
/// 非零退出 + 多字节中文 stdout 的截断行为——原来内联时 `&stdout[..1500]` 按字节切片，
/// 落在中文中间会 panic，反而把"执行器非零退出"变成 debounce 任务崩溃。
fn build_executor_end_content(
    executor_type: &str,
    status: &std::process::ExitStatus,
    _result_text: Option<String>,
    _stdout: &str,
    stderr: &str,
) -> String {
    // 非零退出时 stderr 给用户的预览上限，按 char 计：与开始消息 preview 同语义，
    // 避免 `&str[..n]` 这种按字节切片切到多字节字符中间触发 panic。
    const STDERR_PREVIEW_CHAR_LIMIT: usize = 1500;
    if status.success() {
        // 进程退出 0：统一发简洁结束标志，过程内容已实时推送，不再重复输出
        format!("✅ {} 处理完成", executor_type)
    } else {
        // 非零退出：用 stderr 展示错误信息（执行器错误信息通常走 stderr），
        // stderr 为空则只报退出码。
        let diagnostic = stderr.chars().take(STDERR_PREVIEW_CHAR_LIMIT).collect::<String>();
        if diagnostic.trim().is_empty() {
            executor_error_message(executor_type, &format!("退出码 {:?}", status.code()))
        } else {
            executor_error_message(
                executor_type,
                &format!("退出码 {:?}\n{}", status.code(), diagnostic),
            )
        }
    }
}

/// `run_executor_with_timeout` 的失败原因。
///
/// 区分"超时"和"wait 本身出错"是因为两者的用户提示不同：超时基本是 provider
/// 挂起，wait 失败更可能是本地进程/系统层问题。调用方据此拼错误消息。
#[derive(Debug)]
enum ExecutorRunError {
    /// 子进程在 `timeout` 内未退出，已被 kill 回收。
    Timeout { secs: u64 },
    /// `child.wait()` 本身返回了错误（例如系统层 IO 失败）。
    WaitFailed(String),
}

/// 带超时地运行子进程：并发读取 stdout + `child.wait()` 与计时器竞赛。
///
/// 不用 `Child::wait_with_output`——它按值消费 `child`，超时分支就拿不到句柄
/// 去 kill，pi 会变成孤儿进程继续占资源。这里改成先 `take()` 出 stdout
/// 在独立 task 里 `read_to_end`，再用 `&mut child.wait()` 参与超时竞赛；
/// 超时则 `start_kill`（发 SIGKILL）+ `wait` 回收僵尸，返回 `Timeout`。
/// 成功则把 (退出状态, stdout 字节) 一起返回给上层解析。
///
/// `timeout` 为 `Some` 时按上述超时竞赛执行；为 `None` 时表示用户把
/// `execution_timeout_secs` 设为 `0`（不限制），此时直接 `child.wait()` 等到进程退出，
/// 不包 `tokio::time::timeout`、不 kill——语义与 todo pipeline 的
/// `timeout_enabled = v > 0` 对齐。
///
/// 注意：生产代码已改用 `wait_child_with_timeout` + `stream_executor_stdout` 替代此函数，
/// 保留此函数仅供测试覆盖超时/kill 行为。
#[allow(dead_code)]
async fn run_executor_with_timeout(
    mut child: tokio::process::Child,
    timeout: Option<std::time::Duration>,
) -> Result<(std::process::ExitStatus, Vec<u8>), ExecutorRunError> {
    // 先把 stdout 拿走，独立 task 里读完整缓冲。这样 wait 的计时竞赛只管
    // 进程退出，不阻塞在 stdout 读取上；超时分支也还能 kill child。
    let stdout_handle = child.stdout.take();
    let stdout_task = stdout_handle.map(|mut reader| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            // 读取失败时返回已收到的部分；调用方按字节解析，不因读错误整体失败
            let _ = reader.read_to_end(&mut buf).await;
            buf
        })
    });

    // Some → 计时竞赛；None → 无超时等待。两条分支成功后都要 join stdout task。
    // 错误分支用 `return Err(..)` 提前退出，故此处 `wait_outcome` 即 `ExitStatus`。
    let wait_outcome = match timeout {
        Some(t) => match tokio::time::timeout(t, child.wait()).await {
            Ok(Ok(status)) => status,
            // wait 本身出错：本地进程/系统层问题，带上错误信息让用户看得到
            Ok(Err(e)) => return Err(ExecutorRunError::WaitFailed(e.to_string())),
            Err(_) => {
                // 超时：先 SIGKILL 再 wait 回收，避免 pi 孤儿进程；两步失败都不致命，用 _ 忽略
                let _ = child.start_kill();
                let _ = child.wait().await;
                // 此处故意不 join stdout_task：kill 后子进程管道关闭，read_to_end 收到 EOF
                // 自行结束，task 变 detached 但不会泄漏；超时路径本就不要输出，buf 随 task 丢弃。
                return Err(ExecutorRunError::Timeout { secs: t.as_secs() });
            }
        },
        None => match child.wait().await {
            Ok(status) => status,
            // wait 本身出错的情况与 Some 分支一致，复用同一个错误变体
            Err(e) => return Err(ExecutorRunError::WaitFailed(e.to_string())),
        },
    };

    // 进程已退出，join 读 stdout 的 task 拿完整输出；task 若 panic 则当无输出。
    // 用 async block 而非闭包，才能在里面 await join handle。
    let buf = match stdout_task {
        Some(task) => task.await.unwrap_or_default(),
        None => Vec::new(),
    };
    Ok((wait_outcome, buf))
}

/// 仅等待子进程退出（不读 stdout），超时则 kill 回收
///
/// 与 run_executor_with_timeout 的区别：不负责读取 stdout，因为 stdout 句柄
/// 在调用前已被 take() 走用于流式读取。此函数只做 wait + timeout + kill。
async fn wait_child_with_timeout(
    mut child: tokio::process::Child,
    timeout: Option<std::time::Duration>,
) -> Result<std::process::ExitStatus, ExecutorRunError> {
    match timeout {
        // Some → 计时竞赛；None → 无超时等待
        Some(t) => match tokio::time::timeout(t, child.wait()).await {
            Ok(Ok(status)) => Ok(status),
            Ok(Err(e)) => Err(ExecutorRunError::WaitFailed(e.to_string())),
            Err(_) => {
                // 超时：SIGKILL + wait 回收，避免孤儿进程
                let _ = child.start_kill();
                let _ = child.wait().await;
                Err(ExecutorRunError::Timeout { secs: t.as_secs() })
            }
        },
        None => match child.wait().await {
            Ok(status) => Ok(status),
            Err(e) => Err(ExecutorRunError::WaitFailed(e.to_string())),
        },
    }
}

/// 流式读取执行器 stdout 的结果
///
/// logs：解析后的日志条目，用于提取最终结果（get_final_result_from_logs）
/// raw_stdout：原始 stdout 文本，用于错误诊断和兜底回复
struct StdoutStreamResult {
    logs: Vec<ParsedLogEntry>,
    raw_stdout: String,
}

/// 流式读取执行器 stdout：逐行解析并发送 Output 事件
///
/// 复用 log_capture.rs 的 EventPipeline 创建和解析逻辑，确保 executor 直连执行
/// 与 todo 执行产生完全相同格式的事件。发送的 ExecEvent::Output 会被
/// FeishuPushService 按 push_level 推送到飞书（"all" 时推送过程事件）。
///
/// `direct_output_info` 为 Some 时，每条解析出的日志额外发一条
/// ExecutorDirectOutput 直接推送给触发用户（一对一私聊场景），
/// 这样即使 push target 未配置该用户，用户也能在聊天中看到执行过程。
async fn stream_executor_stdout<R: tokio::io::AsyncRead + Unpin + Send>(
    stdout_handle: Option<R>,
    executor: &Arc<dyn crate::adapters::CodeExecutor>,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    workspace_id: Option<i64>,
    direct_output_info: Option<DirectOutputInfo>,
) -> StdoutStreamResult {
    let Some(stdout) = stdout_handle else {
        return StdoutStreamResult { logs: Vec::new(), raw_stdout: String::new() };
    };
    // 复用 log_capture 的 pipeline 创建逻辑，确保与 todo 执行路径一致
    let mut pipeline = log_capture::create_pipeline_for_executor(executor.as_ref())
        .unwrap_or_else(|| EventPipeline::new(executor.executor_type().as_str()));
    let mut reader = BufReader::new(stdout).lines();
    let mut result = StdoutStreamResult { logs: Vec::new(), raw_stdout: String::new() };

    while let Ok(Some(line)) = reader.next_line().await {
        process_executor_stdout_line(
            &line, &mut pipeline, executor, tx, task_id, workspace_id,
            direct_output_info.as_ref(), &mut result,
        );
    }
    finalize_executor_pipeline(
        &mut pipeline, tx, task_id, workspace_id,
        direct_output_info.as_ref(), &mut result,
    );
    result
}

/// 一对一私聊场景下，过程消息直接推送的目标信息
struct DirectOutputInfo {
    bot_id: i64,
    receive_id: String,
    receive_id_type: String,
}

/// 处理单行 stdout：先尝试 pipeline 解析，回退到 executor 自定义解析
///
/// 该函数参数较多是因为需要同时处理 pipeline 解析、事件广播、结果收集三个职责，
/// 拆分成多个函数反而会导致数据结构重复传递，故用 allow 抑制参数数量告警。
#[allow(clippy::too_many_arguments)]
fn process_executor_stdout_line(
    line: &str,
    pipeline: &mut EventPipeline,
    executor: &Arc<dyn crate::adapters::CodeExecutor>,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    workspace_id: Option<i64>,
    direct_output: Option<&DirectOutputInfo>,
    result: &mut StdoutStreamResult,
) {
    result.raw_stdout.push_str(line);
    result.raw_stdout.push('\n');
    // 一对一私聊场景：手动解析，只发送 ExecutorDirectOutput，不调用 try_parse_with_pipeline（避免发送 Output 导致重复）
    if let Some(info) = direct_output {
        let parsed_list = parse_pipeline_manually(pipeline, line);
        if !parsed_list.is_empty() {
            for entry in &parsed_list {
                send_executor_direct_output(tx, info, entry.clone());
            }
            result.logs.extend(parsed_list);
            return;
        }
        // 回退到 executor 自定义解析
        let Some(parsed) = executor.parse_output_line(line) else { return };
        send_executor_direct_output(tx, info, parsed.clone());
        result.logs.push(parsed);
        return;
    }
    // 非私聊场景：走标准流程，调用 try_parse_with_pipeline 发送 Output 事件
    let parsed_list = log_capture::try_parse_with_pipeline(
        pipeline, line, tx, task_id, workspace_id,
    );
    if !parsed_list.is_empty() {
        result.logs.extend(parsed_list);
        return;
    }
    // 回退到 executor 自定义解析（非 JSONL 格式的行）
    let Some(parsed) = executor.parse_output_line(line) else { return };
    log_capture::send_event(
        tx,
        ExecEvent::Output {
            task_id: task_id.to_string(),
            entry: parsed.clone(),
            workspace_id,
        },
    );
    result.logs.push(parsed);
}

/// 手动解析 pipeline：只返回解析结果，不发送 Output 事件（由调用方决定发送方式）
///
/// 与 try_parse_with_pipeline 的区别：后者会发送 ExecEvent::Output，
/// 前者只返回 ParsedLogEntry，用于私聊场景避免重复发送。
fn parse_pipeline_manually(
    pipeline: &mut EventPipeline,
    line: &str,
) -> Vec<ParsedLogEntry> {
    let line_trimmed = line.trim();
    if line_trimmed.is_empty() {
        return Vec::new();
    }
    let len_before = pipeline.len();
    pipeline.feed(line_trimmed);
    let new_events: Vec<&crate::execution_events::ExecutionEvent> = pipeline.events()[len_before..].iter().collect();
    if new_events.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    for event in &new_events {
        // 只处理对用户有价值的事件类型（与 send_executor_direct_output 的过滤规则一致）
        match event {
            crate::execution_events::ExecutionEvent::Info { message } => {
                if message.starts_with('{') || message.is_empty() {
                    continue;
                }
                let parsed = crate::execution_events::DbLogEntry::from_event_to_parsed_log_entry(event);
                results.push(parsed);
            }
            crate::execution_events::ExecutionEvent::Thinking { .. }
            | crate::execution_events::ExecutionEvent::ToolCall { .. }
            | crate::execution_events::ExecutionEvent::ToolResult { .. }
            | crate::execution_events::ExecutionEvent::Assistant { .. }
            | crate::execution_events::ExecutionEvent::Result { .. } => {
                let parsed = crate::execution_events::DbLogEntry::from_event_to_parsed_log_entry(event);
                results.push(parsed);
            }
            // 跳过内部状态事件（与 send_executor_direct_output 的过滤规则一致）
            crate::execution_events::ExecutionEvent::SessionStart { .. }
            | crate::execution_events::ExecutionEvent::SessionEnd { .. }
            | crate::execution_events::ExecutionEvent::StepStart { .. }
            | crate::execution_events::ExecutionEvent::StepFinish { .. }
            | crate::execution_events::ExecutionEvent::Tokens { .. }
            | crate::execution_events::ExecutionEvent::Cost { .. }
            | crate::execution_events::ExecutionEvent::Duration { .. }
            | crate::execution_events::ExecutionEvent::ModelSwitch { .. }
            | crate::execution_events::ExecutionEvent::Error { .. }
            | crate::execution_events::ExecutionEvent::Progress { .. }
            | crate::execution_events::ExecutionEvent::User { .. }
            | crate::execution_events::ExecutionEvent::System { .. } => {}
        }
    }
    results
}

/// finalize pipeline 并收集剩余事件（SessionEnd 等）
fn finalize_executor_pipeline(
    pipeline: &mut EventPipeline,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    workspace_id: Option<i64>,
    direct_output: Option<&DirectOutputInfo>,
    result: &mut StdoutStreamResult,
) {
    let len_before = pipeline.len();
    pipeline.finalize();
    for event in &pipeline.events()[len_before..] {
        // 一对一私聊场景：手动转换，只发送 ExecutorDirectOutput
        if let Some(info) = direct_output {
            match event {
                crate::execution_events::ExecutionEvent::Thinking { .. }
                | crate::execution_events::ExecutionEvent::ToolCall { .. }
                | crate::execution_events::ExecutionEvent::ToolResult { .. }
                | crate::execution_events::ExecutionEvent::Assistant { .. }
                | crate::execution_events::ExecutionEvent::Result { .. } => {
                    let parsed = crate::execution_events::DbLogEntry::from_event_to_parsed_log_entry(event);
                    send_executor_direct_output(tx, info, parsed.clone());
                    result.logs.push(parsed);
                }
                _ => {}
            }
            continue;
        }
        // 非私聊场景：走标准流程，发送 Output 事件
        let parsed = log_capture::emit_execution_event(event, tx, task_id, workspace_id);
        result.logs.push(parsed);
    }
}

/// 发送 ExecutorDirectOutput 事件：把单条日志直接推送给触发用户
///
/// 与 Output 事件的区别：绕过 push target 过滤和 workspace 隔离，
/// 一对一直接发送，用户在聊天中就能看到执行过程。
///
/// 过滤规则（参考 cc-connect 的简洁风格）：
/// - 保留：thinking（思考）、tool_call（工具调用）、tool_result（工具结果）、assistant/text（助手回复）、result（最终结果）
/// - 跳过：session_start/session_end、step_start/step_finish、tokens、model_switch、info、error（内部状态事件不打扰用户）
fn send_executor_direct_output(
    tx: &broadcast::Sender<ExecEvent>,
    info: &DirectOutputInfo,
    entry: ParsedLogEntry,
) {
    // 过滤掉内部状态事件，只保留对用户有价值的思考和工具交互
    match entry.log_type.as_str() {
        "session_start" | "session_end" | "step_start" | "step_finish" => return,
        "tokens" | "model_switch" | "cost" | "duration" => return,
        "info" | "error" | "stderr" | "warning" => return,
        _ => {}
    }
    let _ = tx.send(ExecEvent::ExecutorDirectOutput {
        bot_id: info.bot_id,
        receive_id: info.receive_id.clone(),
        receive_id_type: info.receive_id_type.clone(),
        entry,
    });
}

// ============================================================================
// 默认响应处理器：处理 loop 和 executor 类型的默认响应
// ============================================================================

impl MessageDebounce {
    /// 处理默认响应类型为 loop 的情况
    /// 直接通过 LoopRunner 触发环路执行（fire-and-forget）
    async fn handle_default_response_loop(
        db: Arc<Database>,
        loop_runner: Option<Arc<crate::services::loop_runner::LoopRunner>>,
        loop_id: i64,
        message: &str,
        feishu_bot_id: Option<i64>,
        feishu_receive_id: Option<String>,
        feishu_receive_id_type: Option<String>,
    ) -> Result<crate::executor_service::ExecutionResult, Option<String>> {
        // 检查环路是否存在且状态为 enabled
        let loop_ = match db.get_loop(loop_id).await {
            Ok(Some(l)) => l,
            Ok(None) => {
                tracing::warn!("[debounce] loop {} not found", loop_id);
                return Err(None);
            }
            Err(e) => {
                tracing::error!("[debounce] failed to get loop {}: {}", loop_id, e);
                return Err(None);
            }
        };

        // 环路状态不是 enabled（暂停或禁用），返回 loop_paused 错误
        if loop_.status != "enabled" {
            tracing::warn!("[debounce] loop {} is not enabled (status={})", loop_id, loop_.status);
            return Err(Some("环路未开启".to_string()));
        }

        // 构建 trigger_meta
        let meta = serde_json::json!({
            "source": "default_response",
            "message": message,
        });

        // 通过 LoopRunner 触发环路执行
        let Some(runner) = loop_runner else {
            tracing::error!("[debounce] loop_runner not available");
            return Err(None);
        };

        // spawn_run 消费 Arc<Self>，runner 后续不再使用，直接 move 而非 clone
        let execution_id = runner.spawn_run(
            loop_id,
            None, // trigger_id
            "default_response",
            meta,
            feishu_bot_id,
            feishu_receive_id,
            feishu_receive_id_type,
        );

        if execution_id < 0 {
            tracing::error!("[debounce] loop_runner.spawn_run failed for loop {}", loop_id);
            return Err(None);
        }

        tracing::info!(
            "[debounce] triggered loop {} as default response, execution_id={}",
            loop_id,
            execution_id
        );

        Ok(crate::executor_service::ExecutionResult {
            task_id: format!("loop-{}", execution_id),
            record_id: Some(execution_id),
        })
    }

    /// 处理默认响应类型为 executor 的情况
    /// 直接调用执行器进行交互，不创建执行记录
    #[allow(clippy::too_many_arguments)] // 参数来自上游 handler 的独立数据源，合并为 struct 增加认知负担
    async fn handle_default_response_executor(
        db: &Arc<Database>,
        executor_registry: &Arc<crate::adapters::ExecutorRegistry>,
        _task_manager: &Arc<TaskManager>,
        config: &Arc<std::sync::RwLock<crate::config::Config>>,
        tx: &broadcast::Sender<ExecEvent>,
        bot_id: i64,
        receive_id: String,
        receive_id_type: &str,
        executor_type: Option<&str>,
        workspace_id: Option<i64>,
        message: &str,
        _resume_session_id: Option<String>,
    ) -> Result<crate::executor_service::ExecutionResult, Option<String>> {
        let executor_type = executor_type.unwrap_or("claudecode");

        // 统一的飞书回复出口：开始/结束/错误三类消息都走 ExecutorDirectResponse，
        // FeishuPushService 会绕过 workspace 过滤直接 send_raw 发回用户（feishu_push.rs:65）。
        // 每次 clone receive_id：原来函数末尾一次性 move，改成多次 clone 语义不变，
        // 换来能在多个分支复用同一个发送出口。
        let send_msg = |content: String| {
            let _ = tx.send(ExecEvent::ExecutorDirectResponse {
                bot_id,
                receive_id: receive_id.clone(),
                receive_id_type: receive_id_type.to_string(),
                content,
            });
        };

        // 获取工作空间路径
        let workspace_path = if let Some(wid) = workspace_id {
            match db.get_project_directory_by_id(wid).await {
                Ok(Some(pd)) => pd.path,
                Ok(None) => {
                    tracing::warn!("[debounce] workspace {} not found", wid);
                    send_msg(executor_error_message(executor_type, &format!("工作空间 {} 不存在", wid)));
                    return Err(None);
                }
                Err(e) => {
                    tracing::error!("[debounce] failed to get workspace {}: {}", wid, e);
                    send_msg(executor_error_message(executor_type, &format!("读取工作空间失败：{}", e)));
                    return Err(None);
                }
            }
        } else {
            tracing::warn!("[debounce] no workspace_id for executor default response");
            send_msg(executor_error_message(executor_type, "未配置工作空间"));
            return Err(None);
        };

        // 获取执行器
        let exec_type = match parse_executor_type(executor_type) {
            Some(t) => t,
            None => {
                tracing::warn!("[debounce] unknown executor type: {}", executor_type);
                send_msg(executor_error_message(executor_type, &format!("未知执行器类型：{}", executor_type)));
                return Err(None);
            }
        };
        let executor = match executor_registry.get(exec_type).await {
            Some(e) => e,
            None => {
                tracing::warn!("[debounce] executor {} not found", executor_type);
                send_msg(executor_error_message(executor_type, "执行器未注册"));
                return Err(None);
            }
        };

        tracing::info!(
            "[debounce] executor {} direct response in workspace {:?}, message len={}",
            executor_type,
            workspace_path,
            message.len()
        );

        // 构建执行器命令
        let command_args = executor.command_args(message);
        let program = executor.executable_path();
        tracing::info!(
            "[debounce] spawning: {} {:?} (cwd={:?})",
            program, command_args, workspace_path
        );
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(&command_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped())
            .current_dir(&workspace_path);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("[debounce] failed to spawn executor {}: {}", executor_type, e);
                send_msg(executor_error_message(executor_type, &format!("启动进程失败：{}", e)));
                return Err(None);
            }
        };

        // 提取 stdout 句柄用于流式读取：与 todo 执行路径一致，逐行解析并通过
        // EventPipeline 发送 ExecEvent::Output 事件，让 FeishuPushService 按 push_level 推送。
        let stdout_handle = child.stdout.take();
        // 提取 stderr 句柄：非零退出时把 stderr 内容带进错误消息
        let stderr_handle = child.stderr.take();

        // 为流式 Output 事件生成 task_id（提前生成，供 streaming reader 和返回值共用）
        let task_id = format!("executor-{}-{}", executor_type, uuid::Uuid::new_v4());

        // spawn 成功立即发"开始处理"标志，让飞书侧知道请求已被接收并在跑
        send_msg(executor_start_message(executor_type, message));

        // 预写 stdin payload（部分执行器需要，如 pi）：写入后立即 flush 并 drop 以关闭 stdin
        if let Some(payload) = executor.stdin_payload() {
            if let Some(mut stdin) = child.stdin.take() {
                if let Err(e) = stdin.write_all(payload.as_bytes()).await {
                    tracing::warn!("[debounce] failed to write stdin payload for {}: {}", executor_type, e);
                } else if let Err(e) = stdin.flush().await {
                    tracing::warn!("[debounce] failed to flush stdin for {}: {}", executor_type, e);
                }
                drop(stdin);
            }
        }

        // 启动流式读取任务：并发读取 stdout 并通过 EventPipeline 发送 Output 事件。
        // 复用 log_capture 的 pipeline 创建和解析逻辑，确保与 todo 执行产生完全相同格式的事件。
        // FeishuPushService 按 push_level 配置推送（"all" 时推送过程事件到飞书）。
        let tx_for_stream = tx.clone();
        let executor_for_stream = executor.clone();
        let tid_for_stream = task_id.clone();
        // 一对一私聊场景：先查一次 push_level，仅当配置为 "all" 时才发送过程消息。
        // 在发送端判断而非 FeishuPushService 端判断，避免每条日志都查一次 DB。
        let push_level = match db.get_feishu_push_target(bot_id).await {
            Ok(Some(target)) => target.push_level,
            Ok(None) => "result_only".to_string(),
            Err(e) => {
                tracing::warn!("[debounce] failed to get push_level for bot {}: {}", bot_id, e);
                "result_only".to_string()
            }
        };
        let direct_output_info = if push_level == "all" {
            Some(DirectOutputInfo {
                bot_id,
                receive_id: receive_id.clone(),
                receive_id_type: receive_id_type.to_string(),
            })
        } else {
            None
        };
        let stream_task = tokio::spawn(async move {
            stream_executor_stdout(
                stdout_handle, &executor_for_stream, &tx_for_stream, &tid_for_stream,
                workspace_id, direct_output_info,
            ).await
        });

        // 带超时地等待子进程退出（stdout 已被流式读取，此处只管 wait + timeout + kill）
        let timeout_secs = {
            #[allow(clippy::expect_used)]
            let cfg = config
                .read()
                .expect("config RwLock poisoned in handle_default_response_executor");
            cfg.execution_timeout_secs
        };
        let timeout = direct_executor_timeout(timeout_secs);
        let status = match wait_child_with_timeout(child, timeout).await {
            Ok(s) => {
                tracing::info!("[debounce] executor {} finished, exit_code={:?}", executor_type, s.code());
                s
            }
            Err(ExecutorRunError::Timeout { secs }) => {
                tracing::error!("[debounce] executor {} timed out after {}s", executor_type, secs);
                send_msg(executor_error_message(executor_type, &format!("执行超时（{}s）", secs)));
                return Err(None);
            }
            Err(ExecutorRunError::WaitFailed(msg)) => {
                tracing::error!("[debounce] failed to wait for executor {}: {}", executor_type, msg);
                send_msg(executor_error_message(executor_type, &format!("等待进程失败：{}", msg)));
                return Err(None);
            }
        };

        // 等待流式读取任务完成，收集解析结果和原始 stdout
        let stream_result = stream_task.await.unwrap_or(StdoutStreamResult {
            logs: Vec::new(),
            raw_stdout: String::new(),
        });

        // 读取 stderr：非零退出时用于诊断失败原因
        let stderr_text = match stderr_handle {
            Some(mut reader) => {
                let mut buf = Vec::new();
                let _ = reader.read_to_end(&mut buf).await;
                String::from_utf8_lossy(&buf).to_string()
            }
            None => String::new(),
        };
        if !stderr_text.is_empty() {
            // 按 char 截断日志预览，避免按字节切片切断多字节中文导致 panic
            let stderr_preview: String = stderr_text.chars().take(2000).collect();
            tracing::info!(
                "[debounce] executor {} stderr:\n{}",
                executor_type,
                stderr_preview
            );
        }

        // 从流式解析收集的日志中提取最终结果（与 todo 执行路径一致）
        // 按 char 截断日志预览，避免按字节切片切断多字节中文导致 panic
        let stdout_preview: String = stream_result.raw_stdout.chars().take(2000).collect();
        tracing::info!(
            "[debounce] executor {} stdout:\n{}",
            executor_type,
            stdout_preview
        );
        let result_text = crate::executor_service::completion::get_final_result_from_logs(&stream_result.logs);

        // 构建结束消息：成功有解析结果就用结果；无结果但退出 0 且有 stdout 就用 stdout 兜底；
        // 非 0 退出走错误通道带退出码+输出片段
        let content = build_executor_end_content(
            executor_type, &status, result_text, &stream_result.raw_stdout, &stderr_text,
        );

        tracing::info!(
            "[debounce] executor {} result_text={:?}",
            executor_type,
            content.chars().take(200).collect::<String>()
        );
        send_msg(content);

        Ok(crate::executor_service::ExecutionResult {
            task_id,
            record_id: None, // executor 类型不存储执行记录
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod merge_pending_messages_tests {
    //! 验证 debounce 窗口内多条消息的合并规则。`push` 把消息丢进 bucket,
    //! 定时器到期时再调 `merge_pending_messages` 合并成一段 ——
    //! 如果合并规则错了,Claude 收到的就是几段被错误拼接的脏文本。
    use super::{merge_pending_messages, PendingMessage};

    fn msg(content: &str) -> PendingMessage {
        PendingMessage {
            bot_id: 1,
            chat_id: "chat-1".to_string(),
            chat_type: "group".to_string(),
            sender: "user-1".to_string(),
            content: content.to_string(),
            todo_id: 42,
            todo_prompt: "stub".to_string(),
            executor: Some("claudecode".to_string()),
            trigger_type: "feishu".to_string(),
            params: None,
            message_id: None,
            resume_session_id: None,
            resume_message: None,
            binding_id: None,
            workspace_id: None,
            immediate: false,
        }
    }

    /// 单条消息: 合并后应该和原文一致,不应该被加上 `---` 之类装饰。
    /// 边界用例 —— 如果 join 永远会加 separator,空 list 都得返回 `---`,
    /// 显然不对。
    #[test]
    fn test_single_message_returns_content_unchanged() {
        let merged = merge_pending_messages(&[msg("hello world")]);
        assert_eq!(merged, "hello world");
    }

    /// 两条消息: 中间用 `\n---\n` 分隔。这是飞书用户连续发"前一行/后一行"
    /// 时 AI 收到的样子。
    #[test]
    fn test_two_messages_joined_with_separator() {
        let merged = merge_pending_messages(&[msg("line A"), msg("line B")]);
        assert_eq!(merged, "line A\n---\nline B");
    }

    /// 任意 N 条都按顺序 join,顺序必须保持稳定 —— AI 会把它当成时间序列读。
    #[test]
    fn test_many_messages_preserve_order() {
        let merged = merge_pending_messages(&[
            msg("first"),
            msg("second"),
            msg("third"),
            msg("fourth"),
        ]);
        assert_eq!(merged, "first\n---\nsecond\n---\nthird\n---\nfourth");
    }

    /// 空切片: 返回空串。这是 `pending_count` 防御 if-empty 的上游,
    /// 如果空切片返回 `---`,filter 那行能放行但 Claude 会收到莫名其妙的
    /// 标点符号。
    #[test]
    fn test_empty_slice_returns_empty_string() {
        let merged = merge_pending_messages(&[]);
        assert_eq!(merged, "");
    }

    /// 消息内的换行不参与合并规则 —— 内部换行应该原样保留,只在消息之间
    /// 插入 `---`。这跟 `replace_placeholders` 的占位符替换是配套的:
    /// 用户的多行消息在 `{{message}}` 位置原样展开。
    #[test]
    fn test_internal_newlines_preserved_verbatim() {
        let merged = merge_pending_messages(&[
            msg("line 1\nline 2"),
            msg("single line"),
        ]);
        assert_eq!(merged, "line 1\nline 2\n---\nsingle line");
    }
}

/// 飞书默认响应执行器的反馈消息格式化与超时运行逻辑测试。
///
/// 这组测试覆盖 `handle_default_response_executor` 里抽出来的纯逻辑：
/// 三类飞书反馈消息（开始/错误/空结束）的格式，以及带超时地运行子进程
/// 并在超时时 kill 回收的行为。把 I/O 隔在 `handle_default_response_executor`
/// 主体里，这里只测可复现的纯函数 + 用 `echo`/`sleep` 真进程验证超时分支。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod executor_feedback_tests {
    use super::*;

    /// 短消息：开始标志应包含执行器名 + 原文 preview，前缀 ⏳ 与全仓 status_icon 约定一致。
    #[test]
    fn test_executor_start_message_basic() {
        let msg = executor_start_message("pi", "你叫啥");
        assert_eq!(msg, "⏳ pi 开始处理：你叫啥");
    }

    /// 长 preview 必须在 30 字（按 Unicode scalar，不切断多字节中文）处截断，
    /// 避免把整段用户 prompt 原样刷到飞书会话里造成噪声。
    #[test]
    fn test_executor_start_message_truncates_long() {
        let long = "一二三四五六七八九十一二三四五六七八九十一二三四五六七八九十多余";
        let msg = executor_start_message("pi", long);
        // 前 30 个 char + 省略号，提示用户内容被裁了
        let expected_preview: String = long.chars().take(30).collect();
        assert_eq!(msg, format!("⏳ pi 开始处理：{}…", expected_preview));
    }

    /// 错误消息格式：前缀 ❌ + 执行器名 + 原因，原因原样透传（含超时秒数、wait 错误等）。
    #[test]
    fn test_executor_error_message_format() {
        let msg = executor_error_message("pi", "执行超时（300s）");
        assert_eq!(msg, "❌ pi 执行失败：执行超时（300s）");
    }

    /// 成功但无输出时的结束标志，让用户知道执行跑完了只是没产出文本，
    /// 而不是静默无响应（这正是本次要消除的"静默失败"体验）。
    #[test]
    fn test_executor_empty_end_message() {
        let msg = executor_empty_end_message("pi");
        assert_eq!(msg, "✅ pi 执行完成（无输出）");
    }

    /// 成功路径：`echo hi` 应在超时内退出，stdout 含 `hi`。
    /// 用 `sh -c` 包一层保证跨平台（macOS/Linux 都有 sh）。
    #[tokio::test]
    async fn test_run_executor_with_timeout_success() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "echo hi"]).stdout(std::process::Stdio::piped());
        let child = cmd.spawn().expect("spawn sh echo");
        let (status, stdout) = run_executor_with_timeout(child, Some(std::time::Duration::from_secs(5)))
            .await
            .expect("echo should succeed within timeout");
        assert!(status.success(), "echo should exit 0");
        let out = String::from_utf8_lossy(&stdout);
        assert!(out.contains("hi"), "stdout should contain hi, got: {}", out);
    }

    /// 超时路径：`sleep 30` 在 1s 超时后应被 kill，返回 Timeout 而不是挂起整个测试。
    /// 这是本次修复的核心——挂起的执行器不再永久卡死 debounce 任务。
    #[tokio::test]
    async fn test_run_executor_with_timeout_kills_on_timeout() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "sleep 30"]).stdout(std::process::Stdio::piped());
        let child = cmd.spawn().expect("spawn sh sleep");
        let start = std::time::Instant::now();
        let err = run_executor_with_timeout(child, Some(std::time::Duration::from_secs(1)))
            .await
            .expect_err("sleep 30 should time out");
        // 超时分支应在略超 1s 处返回，而不是等满 30s
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "should return shortly after timeout");
        match err {
            ExecutorRunError::Timeout { secs } => assert_eq!(secs, 1),
            other => panic!("expected Timeout, got {:?}", other),
        }
    }

    /// 无超时路径：`execution_timeout_secs == 0` 表示「不限制」，传 `None` 时
    /// `echo hi` 应正常退出且 stdout 含 `hi`，不能因为没设超时就把进程卡死或判死。
    /// 这是 `0 = 不限制` 语义在直连执行器路径的回归保护。
    #[tokio::test]
    async fn test_run_executor_with_timeout_no_timeout_completes() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "echo hi"]).stdout(std::process::Stdio::piped());
        let child = cmd.spawn().expect("spawn sh echo");
        let (status, stdout) = run_executor_with_timeout(child, None)
            .await
            .expect("echo should complete when timeout is disabled");
        assert!(status.success(), "echo should exit 0 even without timeout");
        let out = String::from_utf8_lossy(&stdout);
        assert!(out.contains("hi"), "stdout should contain hi, got: {}", out);
    }

    /// `0` 必须解析成 `None`：`tokio::time::timeout` 吃 0 秒会立刻 Elapsed，
    /// 0 走 None 分支才能正确表达「不限制」语义。
    #[test]
    fn test_direct_executor_timeout_zero_is_none() {
        assert!(direct_executor_timeout(0).is_none(), "0 must map to None (no timeout)");
    }

    /// 正值必须解析成 `Some(Duration)`，且秒数原样透传。
    #[test]
    fn test_direct_executor_timeout_positive_is_some() {
        let d = direct_executor_timeout(3600).expect("positive secs must map to Some");
        assert_eq!(d.as_secs(), 3600);
    }

    /// 成功时有解析结果：统一返回简洁结束标志，result_text 已通过过程消息实时推送。
    #[test]
    fn test_build_executor_end_content_success_with_result_text() {
        let status = std::process::Command::new("sh")
            .args(["-c", "true"])
            .status()
            .expect("spawn sh true");
        let content = build_executor_end_content("pi", &status, Some("最终答案".to_string()), "ignored stdout", "");
        assert_eq!(content, "✅ pi 处理完成");
    }

    /// 退出 0 + 有原始 stdout：统一返回简洁结束标志，stdout 已通过过程消息实时推送。
    #[test]
    fn test_build_executor_end_content_success_returns_marker() {
        let status = std::process::Command::new("sh")
            .args(["-c", "true"])
            .status()
            .expect("spawn sh true");
        let content = build_executor_end_content("pi", &status, None, "hello world", "");
        assert_eq!(content, "✅ pi 处理完成");
    }

    /// 退出 0 + stdout 为空：统一返回简洁结束标志，与有输出时保持一致。
    #[test]
    fn test_build_executor_end_content_success_empty_returns_marker() {
        let status = std::process::Command::new("sh")
            .args(["-c", "true"])
            .status()
            .expect("spawn sh true");
        let content = build_executor_end_content("pi", &status, None, "   \n  ", "");
        assert_eq!(content, "✅ pi 处理完成");
    }

    /// 非零退出 + 远超上限的多字节中文 stderr：必须按 char 截断前 1500 个，
    /// 不能用 `&str[..1500]` 按字节切片——那会落在中文中间触发 panic。
    /// 这是本次修复的核心回归保护：执行器非零退出不能再变成 debounce 任务崩溃。
    #[test]
    fn test_build_executor_end_content_nonzero_chinese_no_panic() {
        // 1600 个全中文字符，确保命中截断分支
        let chinese = "执行出错".repeat(400);
        let status = std::process::Command::new("sh")
            .args(["-c", "exit 1"])
            .status()
            .expect("spawn sh exit 1");
        // 关键断言：这一行不 panic 即说明按 char 截断生效
        let content = build_executor_end_content("pi", &status, None, "", &chinese);
        assert!(
            content.starts_with("❌ pi 执行失败：退出码 Some(1)"),
            "should start with error prefix, got: {}",
            content
        );
        // 输出区被裁到 1500 个 char：总长度远小于原文 1600 + 前缀
        assert!(
            content.chars().count() < 1600,
            "stderr preview should be truncated to 1500 chars, got len {}",
            content.chars().count()
        );
    }
}
