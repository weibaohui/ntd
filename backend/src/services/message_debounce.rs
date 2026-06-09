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
                    let resume_sid = last.resume_session_id.clone();
                    let exec_message = if resume_sid.is_some() {
                        // resume: send user content as the single message
                        merged_content
                    } else {
                        // new execution: send todo_prompt with params
                        last.todo_prompt.clone()
                    };

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
                        params: if resume_sid.is_some() { None } else { Some(merged_params) },
                        resume_session_id: resume_sid,
                        resume_message: resume_msg,
                        chain: vec![],
                        source_todo_id: None,
                        source_todo_title: None,
                        source_hook_id: None,
                    })
                    .await;

                    let record_id = match &result {
                        Ok(r) => Some(r.record_id),
                        Err(_) => None,
                    };
                    tracing::debug!("[debounce] timer fired for bot_id={}, chat_id={}, msg_count={}, record_id={:?}", bot_id, key.1, entry.messages.len(), record_id);
                    match result {
                        Ok(exec_result) => {
                            // If this message came from a project-bound chat, update binding state
                            if let Some(binding_id) = last.binding_id {
                                let sid = if last.resume_session_id.is_some() {
                                    // Resume: session_id stays the same, just update record
                                    last.resume_session_id.clone()
                                } else {
                                    // New execution: use task_id as session_id
                                    Some(exec_result.task_id.clone())
                                };
                                if let Some(session_id) = sid {
                                    if let Some(rid) = exec_result.record_id {
                                        let _ = db
                                            .update_feishu_project_binding_session(
                                                binding_id,
                                                &session_id,
                                                rid,
                                                "running",
                                            )
                                            .await;
                                    } else {
                                        // Record ID missing: still update session + status
                                        let _ = db
                                            .update_feishu_project_binding_status(binding_id, "running")
                                            .await;
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
                                    .update_feishu_project_binding_status(binding_id, "idle")
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
