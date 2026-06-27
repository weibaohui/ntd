use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::adapters::parse_executor_type;
use crate::db::Database;
use crate::executor_service::RunTodoExecutionRequest;
use crate::handlers::{ExecEvent, execution::start_todo_execution};
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
}

struct DebounceEntry {
    messages: Vec<PendingMessage>,
    timer: JoinHandle<()>,
}

pub struct MessageDebounce {
    entries: Arc<DashMap<(i64, String), DebounceEntry>>,
    ctx: ServiceContext,
}

impl MessageDebounce {
    pub fn new(ctx: ServiceContext) -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            ctx,
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

        // Create new timer
        let new_timer = {
            let entries = self.entries.clone();
            let db = self.ctx.db.clone();
            let executor_registry = self.ctx.executor_registry.clone();
            let tx = self.ctx.tx.clone();
            let task_manager = self.ctx.task_manager.clone();
            let config = self.ctx.config.clone();
            // todo hook 已整块移除（plan `purring-forging-petal`），debounce 触发的
            // 执行不再需要透传 hook_service。
            let bot_id = key.0;
            let chat_id = key.1.clone();
            let target_type = all_msgs
                .first()
                .map(|m| m.chat_type.clone())
                .unwrap_or_default();

            tokio::spawn(async move {
                let secs = db
                    .get_debounce_secs(bot_id, &target_type)
                    .await
                    .unwrap_or(20)
                    .max(1);
                tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await;

                // Timer fired: drain all pending messages for this key
                let key = (bot_id, chat_id);
                let pending = entries.remove(&key);
                if let Some((_, entry)) = pending {
                    if entry.messages.is_empty() {
                        return;
                    }

                    let merged_content = merge_pending_messages(&entry.messages);

                    let last = entry.messages.last().unwrap();
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
                    let result: Result<crate::executor_service::ExecutionResult, ()> = match last.trigger_type.as_str() {
                        "default_response_loop" => {
                            // 环路默认响应：直接触发环路执行
                            Self::handle_default_response_loop(
                                &db,
                                &task_manager,
                                &config,
                                last.todo_id, // loop_id
                                &merged_content,
                                last.workspace_id,
                            )
                            .await
                        }
                        "default_response_executor" => {
                            // 执行器默认响应：直接调用执行器交互（不存储执行记录）
                            Self::handle_default_response_executor(
                                &db,
                                &executor_registry,
                                &task_manager,
                                &config,
                                &tx,
                                last.bot_id,
                                last.sender.clone(),
                                last.executor.as_deref(),
                                last.workspace_id,
                                &merged_content,
                                resume_sid.clone(),
                            )
                            .await
                        }
                        _ => {
                            // 普通的默认响应（todo 类型）或斜杠命令
                            start_todo_execution(RunTodoExecutionRequest {
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
                                feishu_bot_id: if last.binding_id.is_some() { Some(last.bot_id) } else { None },
                                feishu_receive_id: if last.binding_id.is_some() { Some(last.sender.clone()) } else { None },
                                workspace_path: None,
                                workspace_id: last.workspace_id,
                            })
                            .await
                            .map_err(|_| ())
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
                            // Mark messages as failed (processed=false) so they can be retried
                            for msg in &entry.messages {
                                if let Some(ref msg_id) = msg.message_id {
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
// 默认响应处理器：处理 loop 和 executor 类型的默认响应
// ============================================================================

impl MessageDebounce {
    /// 处理默认响应类型为 loop 的情况
    /// 直接触发环路执行，类似于 dispatch_manual 的逻辑
    async fn handle_default_response_loop(
        db: &Arc<Database>,
        _task_manager: &Arc<TaskManager>,
        _config: &Arc<std::sync::RwLock<crate::config::Config>>,
        loop_id: i64,
        message: &str,
        _workspace_id: Option<i64>,
    ) -> Result<crate::executor_service::ExecutionResult, ()> {
        // 检查环路是否存在且状态为 enabled
        let loop_ = match db.get_loop(loop_id).await {
            Ok(Some(l)) => l,
            Ok(None) => {
                tracing::warn!("[debounce] loop {} not found", loop_id);
                return Err(());
            }
            Err(e) => {
                tracing::error!("[debounce] failed to get loop {}: {}", loop_id, e);
                return Err(());
            }
        };

        if loop_.status != "enabled" {
            tracing::warn!("[debounce] loop {} is not enabled (status={})", loop_id, loop_.status);
            return Err(());
        }

        // 创建环路执行记录
        let meta = serde_json::json!({
            "source": "default_response",
            "message": message,
        });

        // 获取环路步骤数量
        let steps = match db.list_loop_steps_by_loop(loop_id).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("[debounce] failed to get loop steps: {}", e);
                return Err(());
            }
        };

        // 创建 loop_execution 记录
        let execution = match db.create_loop_execution(
            loop_id,
            None, // trigger_id
            "default_response",
            &meta.to_string(),
            steps.len() as i32,
        ).await {
            Ok(exec) => exec,
            Err(e) => {
                tracing::error!("[debounce] failed to create loop execution: {}", e);
                return Err(());
            }
        };

        tracing::info!(
            "[debounce] triggered loop {} as default response, execution_id={}",
            loop_id,
            execution.id
        );

        // 注意：环路的后续执行由 loop_trigger_dispatcher 处理
        // 这里我们只是创建了执行记录并触发它

        Ok(crate::executor_service::ExecutionResult {
            task_id: format!("loop-{}", execution.id),
            record_id: Some(execution.id),
        })
    }

    /// 处理默认响应类型为 executor 的情况
    /// 直接调用执行器进行交互，不创建执行记录
    async fn handle_default_response_executor(
        db: &Arc<Database>,
        executor_registry: &Arc<crate::adapters::ExecutorRegistry>,
        _task_manager: &Arc<TaskManager>,
        _config: &Arc<std::sync::RwLock<crate::config::Config>>,
        tx: &broadcast::Sender<ExecEvent>,
        bot_id: i64,
        receive_id: String,
        executor_type: Option<&str>,
        workspace_id: Option<i64>,
        message: &str,
        _resume_session_id: Option<String>,
    ) -> Result<crate::executor_service::ExecutionResult, ()> {
        let executor_type = executor_type.unwrap_or("claudecode");

        // 获取工作空间路径
        let workspace_path = if let Some(wid) = workspace_id {
            match db.get_project_directory_by_id(wid).await {
                Ok(Some(pd)) => pd.path,
                Ok(None) => {
                    tracing::warn!("[debounce] workspace {} not found", wid);
                    return Err(());
                }
                Err(e) => {
                    tracing::error!("[debounce] failed to get workspace {}: {}", wid, e);
                    return Err(());
                }
            }
        } else {
            tracing::warn!("[debounce] no workspace_id for executor default response");
            return Err(());
        };

        // 获取执行器
        let exec_type = match parse_executor_type(executor_type) {
            Some(t) => t,
            None => {
                tracing::warn!("[debounce] unknown executor type: {}", executor_type);
                return Err(());
            }
        };
        let executor = match executor_registry.get(exec_type).await {
            Some(e) => e,
            None => {
                tracing::warn!("[debounce] executor {} not found", executor_type);
                return Err(());
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
        let mut cmd = tokio::process::Command::new(executor.executable_path());
        cmd.args(&command_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped())
            .current_dir(&workspace_path);

        // 预写 stdin payload（部分执行器需要，如 pi）
        if let Some(payload) = executor.stdin_payload() {
            cmd.arg(payload);
        }

        // 静默丢弃 stderr，只捕获 stdout 发送给 Feishu
        cmd.stderr(std::process::Stdio::null());

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("[debounce] failed to spawn executor {}: {}", executor_type, e);
                return Err(());
            }
        };

        // 等待执行器完成并捕获输出
        let output = match child.wait_with_output().await {
            Ok(o) => o,
            Err(e) => {
                tracing::error!("[debounce] failed to wait for executor {}: {}", executor_type, e);
                return Err(());
            }
        };

        // 解析执行器输出：按行解析，提取 result/text 类型的日志
        let stdout = String::from_utf8_lossy(&output.stdout);
        let logs: Vec<ParsedLogEntry> = stdout
            .lines()
            .filter_map(|line| executor.parse_output_line(line))
            .collect();
        let result_text = executor.get_final_result(&logs);

        // 发送结果到 Feishu
        let content = result_text.unwrap_or_else(|| {
            if output.status.success() {
                stdout.to_string()
            } else {
                format!("执行失败（退出码：{:?}）\n\n输出：\n{}", output.status.code(), stdout)
            }
        });

        let receive_id_type = "open_id"; // 默认用 open_id，环路直接响应场景通常是 p2p
        let _ = tx.send(ExecEvent::ExecutorDirectResponse {
            bot_id,
            receive_id,
            receive_id_type: receive_id_type.to_string(),
            content,
        });

        Ok(crate::executor_service::ExecutionResult {
            task_id: format!("executor-{}-{}", executor_type, uuid::Uuid::new_v4()),
            record_id: None, // executor 类型不存储执行记录
        })
    }
}

#[cfg(test)]
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