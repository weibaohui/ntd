use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::task::JoinHandle;

use crate::executor_service::RunTodoExecutionRequest;
use crate::handlers::execution::start_todo_execution;
use crate::service_context::ServiceContext;

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

                    let merged_content: String = entry
                        .messages
                        .iter()
                        .map(|m| m.content.as_str())
                        .collect::<Vec<&str>>()
                        .join("\n---\n");

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

                    let result = start_todo_execution(RunTodoExecutionRequest {
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
                        chain: vec![],
                        source_todo_id: None,
                        source_todo_title: None,
                        source_hook_id: None,
                        feishu_bot_id: if last.binding_id.is_some() { Some(last.bot_id) } else { None },
                        feishu_receive_id: if last.binding_id.is_some() { Some(last.sender.clone()) } else { None },
                    })
                    .await;

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
                                                crate::models::binding_status::IDLE,
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
