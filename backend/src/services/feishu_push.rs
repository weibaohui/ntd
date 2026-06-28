use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::handlers::ExecEvent;
use crate::services::feishu_listener::FeishuListener;

/// Subscribe to ExecEvent broadcast and push formatted messages to Feishu.
pub struct FeishuPushService {
    db: Arc<Database>,
    feishu_listener: Arc<FeishuListener>,
    mutator: broadcast::Sender<PushConfigUpdate>,
}

#[derive(Clone, Debug)]
pub enum PushConfigUpdate {
    Refresh,
}

impl FeishuPushService {
    pub fn new(
        db: Arc<Database>,
        feishu_listener: Arc<FeishuListener>,
    ) -> (Self, broadcast::Sender<PushConfigUpdate>) {
        let (mutator, _) = broadcast::channel(4);
        let service = Self {
            db,
            feishu_listener,
            mutator: mutator.clone(),
        };
        (service, mutator)
    }

    /// Start the push loop. Call this once after construction.
    pub fn start(&self, mut rx: broadcast::Receiver<ExecEvent>) {
        let db = self.db.clone();
        let feishu_listener = self.feishu_listener.clone();
        let mut config_rx = self.mutator.subscribe();
        // workspace_id → [(bot_id, receive_id, receive_id_type, push_level)]
        let mut targets_cache: std::collections::HashMap<Option<i64>, Vec<(i64, String, String, String)>> = std::collections::HashMap::new();
        let mut last_refresh = Instant::now();

        tokio::spawn(async move {
            // Initial load
            Self::refresh_targets(&db, &mut targets_cache).await;

            loop {
                tokio::select! {
                    event = rx.recv() => {
                        match event {
                            Ok(ev) => {
                                // Refresh targets every 60 seconds
                                if last_refresh.elapsed().as_secs() > 60 {
                                    Self::refresh_targets(&db, &mut targets_cache).await;
                                    last_refresh = Instant::now();
                                }

                                // 执行器直接响应：绕过 push_level 过滤和 workspace 隔离，直接发回飞书
                                if let ExecEvent::ExecutorDirectResponse { bot_id, receive_id, receive_id_type, content } = &ev {
                                    // M3 起：优先用 FeishuPlatform 委托（共享连接池 + token 缓存）；
                                    // 失败时回退到老 send_raw（不阻塞推送链）。
                                    let platform_send = feishu_listener
                                        .send_raw_via_platform(*bot_id, receive_id, receive_id_type, content)
                                        .await;
                                    let res = match platform_send {
                                        Ok(()) => Ok(()),
                                        Err(_) => feishu_listener.send_raw(*bot_id, receive_id, receive_id_type, content).await,
                                    };
                                    if let Err(e) = res {
                                        warn!("[feishu-push] executor direct response failed for bot {}: {}", bot_id, e);
                                    } else {
                                        debug!("[feishu-push] executor direct response sent to bot {}: {}", bot_id, &content[..content.len().min(60)]);
                                    }
                                    continue;
                                }

                                // For Finished events with feishu_chat_id (binding chat), send directly
                                if let ExecEvent::Finished { feishu_bot_id, feishu_receive_id, .. } = &ev {
                                    if let (Some(bot_id), Some(receive_id)) = (feishu_bot_id, feishu_receive_id) {
                                        let text = Self::format_event(&ev).unwrap_or_default();
                                        let receive_id_type = "open_id"; // binding chats are p2p
                                        // 同上：优先 FeishuPlatform，回退 send_raw。
                                        let platform_send = feishu_listener
                                            .send_raw_via_platform(*bot_id, receive_id, receive_id_type, &text)
                                            .await;
                                        let res = match platform_send {
                                            Ok(()) => Ok(()),
                                            Err(_) => feishu_listener.send_raw(*bot_id, receive_id, receive_id_type, &text).await,
                                        };
                                        if let Err(e) = res {
                                            warn!("[feishu-push] binding direct send failed for bot {}: {}", bot_id, e);
                                        } else {
                                            debug!("[feishu-push] binding direct sent to bot {}: {}", bot_id, &text[..text.len().min(60)]);
                                        }
                                    }
                                }

                                // Extract workspace_id from Finished event
                                let event_workspace_id = match &ev {
                                    ExecEvent::Finished { workspace_id, .. } => *workspace_id,
                                    _ => None,
                                };

                                // Only send to bots in the same workspace
                                let Some(targets) = targets_cache.get(&event_workspace_id).or_else(|| {
                                    if event_workspace_id.is_some() {
                                        targets_cache.get(&None)
                                    } else {
                                        None
                                    }
                                }) else {
                                    continue;
                                };

                                if targets.is_empty() {
                                    continue;
                                }

                                let Some(text) = Self::format_event(&ev) else {
                                    continue;
                                };

                                for (bot_id, receive_id, receive_id_type, push_level) in targets.iter() {
                                    if !Self::should_send(push_level, &ev) {
                                        continue;
                                    }
                                    // 同上：优先 FeishuPlatform 委托（连接池复用），失败回退。
                                    let platform_send = feishu_listener
                                        .send_raw_via_platform(*bot_id, receive_id, receive_id_type, &text)
                                        .await;
                                    let res = match platform_send {
                                        Ok(()) => Ok(()),
                                        Err(_) => feishu_listener.send_raw(*bot_id, receive_id, receive_id_type, &text).await,
                                    };
                                    if let Err(e) = res {
                                        warn!("[feishu-push] send failed for bot {}: {}", bot_id, e);
                                    } else {
                                        debug!("[feishu-push] sent to bot {}: {}", bot_id, &text[..text.len().min(60)]);
                                    }
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!("[feishu-push] lagged {} events, skipping", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                info!("[feishu-push] channel closed, stopping");
                                break;
                            }
                        }
                    }
                    update = config_rx.recv() => {
                        if update.is_ok() {
                            Self::refresh_targets(&db, &mut targets_cache).await;
                            last_refresh = Instant::now();
                            debug!("[feishu-push] targets refreshed");
                        }
                    }
                }
            }
        });
    }

    pub fn mutator(&self) -> broadcast::Sender<PushConfigUpdate> {
        self.mutator.clone()
    }

    /// Check if an event should be sent based on push_level.
    /// - "disabled": never send
    /// - "result_only": only send Finished events
    /// - "all": send all events
    fn should_send(push_level: &str, event: &ExecEvent) -> bool {
        match push_level {
            "disabled" => false,
            "result_only" => matches!(event, ExecEvent::Finished { .. }),
            "all" => true,
            _ => false,
        }
    }

    async fn refresh_targets(db: &Database, targets: &mut std::collections::HashMap<Option<i64>, Vec<(i64, String, String, String)>>) {
        targets.clear();
        match db.get_all_push_targets_by_workspace().await {
            Ok(targets_map) => {
                *targets = targets_map;
            }
            Err(e) => {
                error!("[feishu-push] failed to load push targets: {}", e);
            }
        }
    }

    /// Format an ExecEvent into a text message (if not a sync event).
    fn format_event(event: &ExecEvent) -> Option<String> {
        match event {
            ExecEvent::Started { task_id, todo_title, executor, .. } => {
                Some(format!(
                    "🟢 [开始执行]\n📋 {}\n⚡ 执行器: {}\n🆔 TaskID: {}",
                    todo_title, executor, task_id
                ))
            }
            ExecEvent::Output { task_id, entry } => {
                let prefix = match entry.log_type.as_str() {
                    "error" | "stderr" => "🔴",
                    "warning" => "⚠️",
                    "success" => "✅",
                    "user" | "input" => "👤",
                    _ => "📝",
                };
                let content = entry.content.trim();
                if content.is_empty() {
                    None
                } else {
                    let preview = if content.chars().count() > 200 {
                        content.chars().take(200).collect::<String>() + "..."
                    } else {
                        content.to_string()
                    };
                    Some(format!("{} {}\n🆔 {}", prefix, preview, task_id))
                }
            }
            ExecEvent::Finished { success, result, todo_title, executor, .. } => {
                let result_preview = result.as_ref()
                    .map(|r| format!("\n\n📤 结果: {}", if r.chars().count() > 100 { r.chars().take(100).collect::<String>() + "..." } else { r.clone() }))
                    .unwrap_or_default();
                Some(format!(
                    "📋 {}\n⚡ 执行器: {}\n{}{}",
                    todo_title,
                    executor,
                    if *success { "✅ 成功" } else { "❌ 失败" },
                    result_preview
                ))
            }
            ExecEvent::TodoProgress { task_id, progress } => {
                if progress.is_empty() {
                    None
                } else {
                    let items: Vec<String> = progress.iter().take(5).map(|t| {
                        format!("• {} [{}]", t.content, t.status)
                    }).collect();
                    Some(format!(
                        "📋 [进度更新] TaskID: {}\n{}",
                        task_id,
                        items.join("\n")
                    ))
                }
            }
            ExecEvent::ExecutionStats { task_id, stats } => {
                Some(format!(
                    "📊 [执行统计] TaskID: {}\n🔧 工具调用: {}\n💬 对话轮次: {}",
                    task_id, stats.tool_calls, stats.conversation_turns
                ))
            }
            ExecEvent::Sync { .. } => None,
            ExecEvent::ReviewStatusChanged { .. } => None,
            // ExecutorDirectResponse 由 FeishuPushService 直接发送，不走 format_event
            ExecEvent::ExecutorDirectResponse { .. } => None,
        }
    }
}
