use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::executor_service::ExecEvent;
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
                                    let res = feishu_listener.send_raw(*bot_id, receive_id, receive_id_type, content).await;
                                    if let Err(e) = res {
                                        warn!("[feishu-push] executor direct response failed for bot {}: {}", bot_id, e);
                                    } else {
                                        debug!("[feishu-push] executor direct response sent to bot {}: {}", bot_id, &content[..content.len().min(60)]);
                                    }
                                    continue;
                                }

                                // For Finished events with feishu_chat_id (binding chat), send directly
                                // 但需要先检查该 bot 的 push_level 配置
                                let mut binding_sent = false;
                                if let ExecEvent::Finished { feishu_bot_id, feishu_receive_id, .. } = &ev {
                                    if let (Some(bot_id), Some(receive_id)) = (feishu_bot_id, feishu_receive_id) {
                                        // 查询该 bot 的 push_level 配置
                                        let push_level = Self::get_bot_push_level(&db, *bot_id).await;
                                        
                                        // 检查 push_level 配置是否允许发送
                                        if Self::should_send(&push_level, &ev) {
                                            let text = Self::format_event(&ev).unwrap_or_default();
                                            let receive_id_type = "open_id"; // binding chats are p2p
                                            let res = feishu_listener.send_raw(*bot_id, receive_id, receive_id_type, &text).await;
                                            if let Err(e) = res {
                                                warn!("[feishu-push] binding direct send failed for bot {}: {}", bot_id, e);
                                            } else {
                                                debug!("[feishu-push] binding direct sent to bot {}: {}", bot_id, &text[..text.len().min(60)]);
                                            }
                                            binding_sent = true;
                                        } else {
                                            debug!("[feishu-push] binding send skipped for bot {} due to push_level={}", bot_id, push_level);
                                            // 仅跳过 binding 路径，继续执行下面的 workspace push target 逻辑
                                        }
                                    }
                                }
                                // binding direct send 成功后跳过 push target 路径，避免重复发送
                                if binding_sent {
                                    continue;
                                }

                                // Extract workspace_id from event (支持 Started, Output, Finished, TodoProgress, ExecutionStats, LoopFinished)
                                let event_workspace_id = match &ev {
                                    ExecEvent::Started { workspace_id, .. } => *workspace_id,
                                    ExecEvent::Output { workspace_id, .. } => *workspace_id,
                                    ExecEvent::Finished { workspace_id, .. } => *workspace_id,
                                    ExecEvent::TodoProgress { workspace_id, .. } => *workspace_id,
                                    ExecEvent::ExecutionStats { workspace_id, .. } => *workspace_id,
                                    ExecEvent::LoopFinished { workspace_id, .. } => *workspace_id,
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
                                    let res = feishu_listener.send_raw(*bot_id, receive_id, receive_id_type, &text).await;
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
    /// - "result_only": only send Finished / LoopFinished events (执行结果)
    /// - "all": send all events
    fn should_send(push_level: &str, event: &ExecEvent) -> bool {
        match push_level {
            "disabled" => false,
            "result_only" => matches!(event, ExecEvent::Finished { .. } | ExecEvent::LoopFinished { .. }),
            "all" => true,
            _ => false,
        }
    }

    /// 获取指定 bot 的 push_level 配置。
    /// - bot 存在且有配置：返回实际配置值
    /// - bot 未配置 push_target：返回默认 "result_only"（兼容旧数据）
    /// - 数据库查询失败：fail closed 返回 "disabled"（安全优先，避免配置无法校验时误发）
    async fn get_bot_push_level(db: &Database, bot_id: i64) -> String {
        match db.get_feishu_push_target(bot_id).await {
            Ok(Some(target)) => target.push_level,
            Ok(None) => {
                // bot 未配置 push_target，使用默认值
                debug!("[feishu-push] bot {} has no push_target config, using default 'result_only'", bot_id);
                "result_only".to_string()
            }
            Err(e) => {
                // 数据库读取失败时 fail closed：宁可漏发也不能误发
                warn!("[feishu-push] failed to get push_target for bot {}: {}, fail closed to 'disabled'", bot_id, e);
                "disabled".to_string()
            }
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
            ExecEvent::Output { task_id, entry, .. } => {
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
            ExecEvent::Finished { success, todo_title, executor, duration_secs, total_tokens, .. } => {
                // 格式化时长（与 LoopFinished 风格一致）
                let duration_str = if *duration_secs >= 3600 {
                    let hours = *duration_secs / 3600;
                    let mins = (*duration_secs % 3600) / 60;
                    format!("{}h {}m", hours, mins)
                } else if *duration_secs >= 60 {
                    let mins = *duration_secs / 60;
                    let secs = *duration_secs % 60;
                    format!("{}m {}s", mins, secs)
                } else {
                    format!("{}s", *duration_secs)
                };
                Some(format!(
                    "📋 {}\n⚡ 执行器: {}\n{}\n⏱️ 用时 {} | 🔤 Token {}",
                    todo_title,
                    executor,
                    if *success { "✅ 成功" } else { "❌ 失败" },
                    duration_str,
                    total_tokens
                ))
            }
            ExecEvent::TodoProgress { task_id, progress, .. } => {
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
            ExecEvent::ExecutionStats { task_id, stats, .. } => {
                Some(format!(
                    "📊 [执行统计] TaskID: {}\n🔧 工具调用: {}\n💬 对话轮次: {}",
                    task_id, stats.tool_calls, stats.conversation_turns
                ))
            }
            ExecEvent::Sync { .. } => None,
            ExecEvent::ReviewStatusChanged { .. } => None,
            // ExecutorDirectResponse 由 FeishuPushService 直接发送，不走 format_event
            ExecEvent::ExecutorDirectResponse { .. } => None,
            // LoopFinished 事件的格式化消息 - 统计摘要
            ExecEvent::LoopFinished { loop_title, status, total_steps, completed_steps, failed_steps, duration_secs, total_tokens, .. } => {
                let status_icon = match status.as_str() {
                    "success" => "✅ 成功",
                    "failed" => "❌ 失败",
                    "partial" => "⚠️ 部分成功",
                    "capped_step" => "🚫 步数超限",
                    "capped_token" => "🚫 Token超限",
                    _ => "ℹ️ 完成",
                };
                
                // 格式化时长
                let duration_str = if *duration_secs >= 3600 {
                    let hours = *duration_secs / 3600;
                    let mins = (*duration_secs % 3600) / 60;
                    format!("{}h {}m", hours, mins)
                } else if *duration_secs >= 60 {
                    let mins = *duration_secs / 60;
                    let secs = *duration_secs % 60;
                    format!("{}m {}s", mins, secs)
                } else {
                    format!("{}s", *duration_secs)
                };
                
                Some(format!(
                    "🔁 [环路执行完成]\n📋 {}\n{} | 共 {} 步 | 成功 {} | 失败 {}\n⏱️ 用时 {} | 🔤 Token {}",
                    loop_title, status_icon, *total_steps, *completed_steps, *failed_steps, duration_str, *total_tokens
                ))
            }
        }
    }
}

#[cfg(test)]
mod feishu_push_binding_tests {
    //! 验证 binding direct send 场景下的 push_level 检查逻辑。
    //! 
    //! 修复背景：事项执行完成后，Finished 事件带有 feishu_bot_id/receive_id 时，
    //! 原代码直接发送，绕过了 push_level 配置检查。用户配置"仅结论"时，
    //! 应该发送完成消息；配置"关闭"时，应该不发送任何消息。
    //! 
    //! 修复方案：在 binding direct send 前查询 bot 的 push_level 配置，
    //! 使用 should_send 函数判断是否允许发送。

    use super::*;

    /// 测试 push_level="disabled" 时，Finished 事件不应该发送
    #[test]
    fn test_should_send_disabled_blocks_finished_event() {
        let push_level = "disabled";
        let event = ExecEvent::Finished {
            task_id: "test-123".to_string(),
            todo_id: 1,
            todo_title: "Test Todo".to_string(),
            executor: "claudecode".to_string(),
            success: true,
            result: Some("完成".to_string()),
            feishu_bot_id: Some(1),
            feishu_receive_id: Some("user_open_id".to_string()),
            workspace_id: Some(1),
            duration_secs: 0,
            total_tokens: 0,
            // 测试构造，无需真实 trigger_type 上下文
            trigger_type: None,
        };
        
        assert!(!FeishuPushService::should_send(push_level, &event));
    }

    /// 测试 push_level="result_only" 时，Finished 事件应该发送
    #[test]
    fn test_should_send_result_only_allows_finished_event() {
        let push_level = "result_only";
        let event = ExecEvent::Finished {
            task_id: "test-123".to_string(),
            todo_id: 1,
            todo_title: "Test Todo".to_string(),
            executor: "claudecode".to_string(),
            success: true,
            result: Some("完成".to_string()),
            feishu_bot_id: Some(1),
            feishu_receive_id: Some("user_open_id".to_string()),
            workspace_id: Some(1),
            duration_secs: 0,
            total_tokens: 0,
            // 测试构造，无需真实 trigger_type 上下文
            trigger_type: None,
        };
        
        assert!(FeishuPushService::should_send(push_level, &event));
    }

    /// 测试 push_level="all" 时，Finished 事件应该发送
    #[test]
    fn test_should_send_all_allows_finished_event() {
        let push_level = "all";
        let event = ExecEvent::Finished {
            task_id: "test-123".to_string(),
            todo_id: 1,
            todo_title: "Test Todo".to_string(),
            executor: "claudecode".to_string(),
            success: true,
            result: Some("完成".to_string()),
            feishu_bot_id: Some(1),
            feishu_receive_id: Some("user_open_id".to_string()),
            workspace_id: Some(1),
            duration_secs: 0,
            total_tokens: 0,
            // 测试构造，无需真实 trigger_type 上下文
            trigger_type: None,
        };
        
        assert!(FeishuPushService::should_send(push_level, &event));
    }

    /// 测试 push_level="result_only" 时，Started 事件不应该发送
    #[test]
    fn test_should_send_result_only_blocks_started_event() {
        let push_level = "result_only";
        let event = ExecEvent::Started {
            task_id: "test-123".to_string(),
            todo_id: 1,
            todo_title: "Test Todo".to_string(),
            executor: "claudecode".to_string(),
            workspace_id: None,
        };
        
        assert!(!FeishuPushService::should_send(push_level, &event));
    }

    /// 测试 push_level="all" 时，Started 事件应该发送
    #[test]
    fn test_should_send_all_allows_started_event() {
        let push_level = "all";
        let event = ExecEvent::Started {
            task_id: "test-123".to_string(),
            todo_id: 1,
            todo_title: "Test Todo".to_string(),
            executor: "claudecode".to_string(),
            workspace_id: None,
        };
        
        assert!(FeishuPushService::should_send(push_level, &event));
    }

    /// 测试 push_level="disabled" 时，Started 事件不应该发送
    #[test]
    fn test_should_send_disabled_blocks_started_event() {
        let push_level = "disabled";
        let event = ExecEvent::Started {
            task_id: "test-123".to_string(),
            todo_id: 1,
            todo_title: "Test Todo".to_string(),
            executor: "claudecode".to_string(),
            workspace_id: None,
        };
        
        assert!(!FeishuPushService::should_send(push_level, &event));
    }

    // ========== LoopFinished 事件测试 ==========

    /// 测试 push_level="disabled" 时，LoopFinished 事件不应该发送
    #[test]
    fn test_should_send_disabled_blocks_loop_finished_event() {
        let push_level = "disabled";
        let event = ExecEvent::LoopFinished {
            loop_execution_id: 1,
            loop_id: 1,
            loop_title: "Test Loop".to_string(),
            status: "success".to_string(),
            total_steps: 3,
            completed_steps: 3,
            failed_steps: 0,
            duration_secs: 120,
            total_tokens: 500,
            workspace_id: Some(1),
        };
        
        assert!(!FeishuPushService::should_send(push_level, &event));
    }

    /// 测试 push_level="result_only" 时，LoopFinished 事件应该发送
    #[test]
    fn test_should_send_result_only_allows_loop_finished_event() {
        let push_level = "result_only";
        let event = ExecEvent::LoopFinished {
            loop_execution_id: 1,
            loop_id: 1,
            loop_title: "Test Loop".to_string(),
            status: "success".to_string(),
            total_steps: 3,
            completed_steps: 3,
            failed_steps: 0,
            duration_secs: 120,
            total_tokens: 500,
            workspace_id: Some(1),
        };
        
        assert!(FeishuPushService::should_send(push_level, &event));
    }

    /// 测试 push_level="all" 时，LoopFinished 事件应该发送
    #[test]
    fn test_should_send_all_allows_loop_finished_event() {
        let push_level = "all";
        let event = ExecEvent::LoopFinished {
            loop_execution_id: 1,
            loop_id: 1,
            loop_title: "Test Loop".to_string(),
            status: "success".to_string(),
            total_steps: 3,
            completed_steps: 3,
            failed_steps: 0,
            duration_secs: 120,
            total_tokens: 500,
            workspace_id: Some(1),
        };
        
        assert!(FeishuPushService::should_send(push_level, &event));
    }

    /// 测试 LoopFinished 事件的格式化输出 - 成功状态
    #[test]
    fn test_format_loop_finished_success() {
        let event = ExecEvent::LoopFinished {
            loop_execution_id: 1,
            loop_id: 1,
            loop_title: "测试环路".to_string(),
            status: "success".to_string(),
            total_steps: 3,
            completed_steps: 3,
            failed_steps: 0,
            duration_secs: 125,
            total_tokens: 500,
            workspace_id: Some(1),
        };
        
        let formatted = FeishuPushService::format_event(&event).unwrap();
        assert!(formatted.contains("🔁 [环路执行完成]"));
        assert!(formatted.contains("测试环路"));
        assert!(formatted.contains("✅ 成功"));
        assert!(formatted.contains("共 3 步"));
        assert!(formatted.contains("成功 3"));
        assert!(formatted.contains("失败 0"));
        assert!(formatted.contains("2m 5s"));
        assert!(formatted.contains("Token 500"));
    }

    /// 测试 LoopFinished 失败状态的格式化输出
    #[test]
    fn test_format_loop_finished_failed() {
        let event = ExecEvent::LoopFinished {
            loop_execution_id: 1,
            loop_id: 1,
            loop_title: "测试环路".to_string(),
            status: "failed".to_string(),
            total_steps: 3,
            completed_steps: 0,
            failed_steps: 3,
            duration_secs: 30,
            total_tokens: 100,
            workspace_id: Some(1),
        };
        
        let formatted = FeishuPushService::format_event(&event).unwrap();
        assert!(formatted.contains("❌ 失败"));
        assert!(formatted.contains("成功 0"));
        assert!(formatted.contains("失败 3"));
        assert!(formatted.contains("30s"));
    }

    /// 测试 LoopFinished 部分成功状态的格式化输出
    #[test]
    fn test_format_loop_finished_partial() {
        let event = ExecEvent::LoopFinished {
            loop_execution_id: 1,
            loop_id: 1,
            loop_title: "测试环路".to_string(),
            status: "partial".to_string(),
            total_steps: 5,
            completed_steps: 3,
            failed_steps: 2,
            duration_secs: 3661,
            total_tokens: 1000,
            workspace_id: Some(1),
        };
        
        let formatted = FeishuPushService::format_event(&event).unwrap();
        assert!(formatted.contains("⚠️ 部分成功"));
        assert!(formatted.contains("共 5 步"));
        assert!(formatted.contains("成功 3"));
        assert!(formatted.contains("失败 2"));
        assert!(formatted.contains("1h 1m"));
        assert!(formatted.contains("Token 1000"));
    }
}
