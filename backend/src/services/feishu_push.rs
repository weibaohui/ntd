use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::executor_service::ExecEvent;
use crate::services::feishu_card::{build_error_card, build_success_card, CardBuilder, render_card};
use crate::services::feishu_listener::FeishuListener;

/// 推送目标类型：workspace_id → [(bot_id, receive_id, receive_id_type, push_level)]
/// 使用类型别名消除 clippy::type_complexity 告警
type PushTargets = std::collections::HashMap<Option<i64>, Vec<(i64, String, String, String)>>;

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
        let mut targets_cache: PushTargets = std::collections::HashMap::new();
        let mut last_refresh = Instant::now();

        tokio::spawn(async move {
            // Initial load
            Self::refresh_targets(&db, &mut targets_cache).await;
            // 工具调用计数器：按 (bot_id, receive_id) 隔离，每次新执行开始时重置为 0
            // 每次 tool_call 事件递增 1，用于显示 "🔧 工具 #N: 工具名"
            let mut tool_call_counters: std::collections::HashMap<(i64, String), usize> =
                std::collections::HashMap::new();

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
                                if let ExecEvent::DirectCardMessage { bot_id, receive_id, receive_id_type, content } = &ev {
                                    // 开始消息（含 "开始处理"）时重置该用户的工具调用计数器
                                    // 每次新执行从 #1 开始计数
                                    if content.contains("开始处理") {
                                        tool_call_counters.insert((*bot_id, receive_id.clone()), 0);
                                    }
                                    // 使用卡片形式发送执行器输出
                                    let card_json = render_card_message(content);
                                    let res = feishu_listener.send_card_raw(*bot_id, receive_id, receive_id_type, &card_json).await;
                                    if let Err(e) = res {
                                        warn!("[feishu-push] executor direct response (card) failed for bot {}: {}", bot_id, e);
                                    } else {
                                        info!("[feishu-push] executor direct response (CARD) sent to bot {}", bot_id);
                                    }
                                    continue;
                                }

                                // 执行器直接输出：executor 默认响应场景下，过程日志直接推送给触发用户。
                                // 与 ExecutorDirectResponse 的区别：后者是开始/结束等关键节点的卡片消息，
                                // 前者是执行过程中流式输出的纯文本消息。
                                // 受 push_level 控制：仅当配置为 "all" 时才发送过程消息，
                                // "result_only" 和 "disabled" 时不发过程消息。
                                if let ExecEvent::DirectStreamMessage { bot_id, receive_id, receive_id_type, entry } = &ev {
                                    // 先查该 bot 的 push_level 配置
                                    let push_level = Self::get_bot_push_level(&db, *bot_id).await;
                                    if push_level != "all" {
                                        debug!("[feishu-push] executor direct output skipped for bot {} due to push_level={}", bot_id, push_level);
                                        continue;
                                    }
                                    // tool_call 类型：递增计数器并传入编号，用于显示 "🔧 工具 #N: 工具名"
                                    let tool_idx = if entry.log_type == "tool_call"
                                        || entry.log_type == "tool_use"
                                        || entry.log_type == "tool"
                                    {
                                        let key = (*bot_id, receive_id.clone());
                                        let counter = tool_call_counters.entry(key).or_insert(0);
                                        *counter += 1;
                                        Some(*counter)
                                    } else {
                                        None
                                    };
                                    // 使用无标题卡片样式发送过程消息，支持 markdown 格式（不显示"执行器输出"标题）
                                    let Some(text) = render_log_entry("", entry, tool_idx) else { continue };
                                    let card_json = render_card_message(&text);
                                    let res = feishu_listener.send_card_raw(*bot_id, receive_id, receive_id_type, &card_json).await;
                                    if let Err(e) = res {
                                        warn!("[feishu-push] executor direct output (card) failed for bot {}: {}", bot_id, e);
                                    } else {
                                        debug!("[feishu-push] executor direct output (CARD) sent to bot {}", bot_id);
                                    }
                                    continue;
                                }

                                // For Finished events with feishu_chat_id (binding chat), send directly
                                // 但需要先检查该 bot 的 push_level 配置
                                let mut binding_sent = false;
                                if let ExecEvent::Finished { feishu_bot_id, feishu_receive_id, feishu_receive_id_type, .. } = &ev {
                                    if let (Some(bot_id), Some(receive_id)) = (feishu_bot_id, feishu_receive_id) {
                                        // 查询该 bot 的 push_level 配置
                                        let push_level = Self::get_bot_push_level(&db, *bot_id).await;

                                        // 检查 push_level 配置是否允许发送
                                        if Self::should_send(&push_level, &ev) {
                                            // 群聊用 chat_id，私聊用 open_id；None 时降级为 open_id
                                            let receive_id_type = feishu_receive_id_type.as_deref().unwrap_or("open_id");
                                            // 使用卡片形式发送执行结果
                                            if let Some(card_json) = format_finished_card(&ev) {
                                                let res = feishu_listener.send_card_raw(*bot_id, receive_id, receive_id_type, &card_json).await;
                                                if let Err(e) = res {
                                                    warn!("[feishu-push] binding card send failed for bot {}: {}", bot_id, e);
                                                } else {
                                                    debug!("[feishu-push] binding card sent to bot {}", bot_id);
                                                }
                                            } else {
                                                // 降级为纯文本
                                                let text = Self::format_event(&ev).unwrap_or_default();
                                                let res = feishu_listener.send_raw(*bot_id, receive_id, receive_id_type, &text).await;
                                                if let Err(e) = res {
                                                    warn!("[feishu-push] binding direct send failed for bot {}: {}", bot_id, e);
                                                }
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

                                let finished_card_json: Option<String> = if let ExecEvent::Finished { .. } = &ev {
                                    format_finished_card(&ev)
                                } else {
                                    None
                                };
                                let text = Self::format_event(&ev);

                                // Started 事件：重置所有 target 的工具调用计数器，
                                // 确保每个新任务的工具调用从 #1 开始
                                if let ExecEvent::Started { .. } = &ev {
                                    for (bot_id, receive_id, _, _) in targets.iter() {
                                        tool_call_counters.insert((*bot_id, receive_id.clone()), 0);
                                    }
                                }

                                for (bot_id, receive_id, receive_id_type, push_level) in targets.iter() {
                                    if !Self::should_send(push_level, &ev) {
                                        continue;
                                    }
                                    // Finished 事件：使用卡片形式发送
                                    if let Some(card_json) = finished_card_json.as_ref() {
                                        let res = feishu_listener.send_card_raw(*bot_id, receive_id, receive_id_type, card_json).await;
                                        if let Err(e) = res {
                                            warn!("[feishu-push] card send failed for bot {}: {}", bot_id, e);
                                        } else {
                                            debug!("[feishu-push] card sent to bot {}", bot_id);
                                        }
                                        continue;
                                    }
                                    // Output 事件：使用卡片形式发送，带工具调用编号累积
                                    if let ExecEvent::Output { task_id, entry, .. } = &ev {
                                        // 计算工具调用编号：tool_call 类型递增计数器
                                        let tool_idx = if entry.log_type == "tool_call"
                                            || entry.log_type == "tool_use"
                                            || entry.log_type == "tool"
                                        {
                                            let key = (*bot_id, receive_id.clone());
                                            let counter = tool_call_counters.entry(key).or_insert(0);
                                            *counter += 1;
                                            Some(*counter)
                                        } else {
                                            None
                                        };
                                        let Some(msg_text) = render_log_entry(task_id, entry, tool_idx) else { continue };
                                        let card_json = render_card_message(&msg_text);
                                        let res = feishu_listener.send_card_raw(*bot_id, receive_id, receive_id_type, &card_json).await;
                                        if let Err(e) = res {
                                            warn!("[feishu-push] output card send failed for bot {}: {}", bot_id, e);
                                        } else {
                                            debug!("[feishu-push] output card sent to bot {}", bot_id);
                                        }
                                        continue;
                                    }
                                    // 其他事件使用纯文本形式
                                    let Some(text) = text.as_ref() else {
                                        continue;
                                    };
                                    let res = feishu_listener.send_raw(*bot_id, receive_id, receive_id_type, text).await;
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
            "all" => !matches!(event, ExecEvent::TodoProgress { .. } | ExecEvent::ExecutionStats { .. }),
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

    async fn refresh_targets(db: &Database, targets: &mut PushTargets) {
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
                // workspace push target 场景暂不做工具调用编号累积，传 None 保持原样
                render_log_entry(task_id, entry, None)
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
            // DirectCardMessage 由 FeishuPushService 直接发送，不走 format_event
            ExecEvent::DirectCardMessage { .. } => None,
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
            // BlackboardDebounceStatus 仅用于前端 WebSocket 推送，不发飞书
            ExecEvent::BlackboardDebounceStatus { .. } => None,
            // WikiChat* 系列事件仅用于前端 WebSocket 推送，不发飞书
            ExecEvent::WikiChatStarted { .. } => None,
            ExecEvent::WikiChatOutput { .. } => None,
            ExecEvent::WikiChatFinished { .. } => None,
            // DirectStreamMessage 由 FeishuPushService 在循环顶部单独处理（直接发送），
            // 不走 format_event 的通用格式路径，故此处返回 None。
            ExecEvent::DirectStreamMessage { .. } => None,
        }
    }
}

/// 根据日志类型返回 emoji 前缀和中文标签
///
/// 让飞书侧能一眼区分思考、工具调用、工具结果、助手回复、会话事件等不同类型，
/// 而不是全部显示为 📝。
fn output_event_prefix_and_label(log_type: &str) -> (&'static str, &'static str) {
    match log_type {
        "thinking" => ("💭", "思考"),
        "tool_call" | "tool_use" | "tool" => ("🔧", "工具调用"),
        "tool_result" => ("📤", "工具结果"),
        "assistant" | "text" => ("💬", "助手"),
        "result" => ("✅", "结果"),
        "error" | "stderr" => ("🔴", "错误"),
        "warning" => ("⚠️", "警告"),
        "session_start" => ("🔄", "会话开始"),
        "session_end" => ("🔄", "会话结束"),
        "tokens" => ("📊", "Token"),
        "model_switch" => ("🔄", "模型切换"),
        "cost" => ("💰", "成本"),
        "duration" => ("⏱️", "耗时"),
        "step_start" => ("🚀", "开始步骤"),
        "step_finish" => ("✅", "完成步骤"),
        "info" => ("📝", "信息"),
        _ => ("📝", "日志"),
    }
}

/// 格式化 Output 事件为飞书消息文本
///
/// 参考 cc-connect 的简洁风格：
/// - 思考：直接显示内容，不加标签前缀
/// - 工具调用：只显示工具名和命令（不显示完整 JSON），使用代码块格式，带编号（#1, #2, ...）
/// - 工具结果：截断到可读长度（最多 300 字符）
/// - 助手回复：直接显示内容
/// - 去掉 task_id 行（私聊场景不需要）
///
/// `tool_call_index` 为工具调用的累积编号（从 1 开始），仅 tool_call 类型使用，
/// 其他类型传 None 即可。
fn render_log_entry(
    _task_id: &str,
    entry: &crate::models::ParsedLogEntry,
    tool_call_index: Option<usize>,
) -> Option<String> {
    let content = entry.content.trim();
    if content.is_empty() {
        return None;
    }
    let result = match entry.log_type.as_str() {
        "thinking" => format!("💭 {}", content),
        "tool_call" | "tool_use" | "tool" => {
            let tool_name = entry.tool_name.as_deref().unwrap_or("");
            // 从 tool_input_json 中提取 command 字段，或者直接用 content
            let command = entry.tool_input_json.as_deref()
                .and_then(extract_command_from_json)
                .unwrap_or_else(|| content.to_string());
            // 去掉引号，只保留命令内容
            let command = command.trim_matches('"').trim().to_string();
            if command.is_empty() {
                return None;
            }
            // 有编号时显示 "🔧 工具 #N: 工具名"，无编号时显示 "🔧 工具名:"
            if let Some(idx) = tool_call_index {
                format!("🔧 工具 #{}: {}\n```bash\n{}\n```", idx, tool_name, command)
            } else {
                format!("🔧 {}:\n```bash\n{}\n```", tool_name, command)
            }
        }
        "tool_result" => {
            return None;
        }
        "assistant" | "text" => format!("💬 {}", content),
        "result" => format!("✅ {}", content),
        "error" | "stderr" => format!("🔴 {}", content),
        "warning" => format!("⚠️ {}", content),
        // 噪音类型：不推送
        // system 包括 Pi 执行器的 agent_start/agent_end/compaction 等无意义状态消息
        // info 包括 Pi 执行器的 "Stopped: xxx" 等停止状态信息，无实际价值
        "tokens" | "step_start" | "step_finish" | "model_switch"
            | "cost" | "duration" | "session_start" | "session_end"
            | "system" | "info" => {
            return None;
        }
        // 其他类型保留，显示通用前缀
        _ => {
            let (prefix, _label) = output_event_prefix_and_label(&entry.log_type);
            format!("{} {}", prefix, content)
        }
    };
    Some(result)
}

/// 从 JSON 字符串中提取 command 字段
///
/// 用于工具调用场景，只显示命令内容而不是完整 JSON 对象
fn extract_command_from_json(json: &str) -> Option<String> {
    // 使用字节级别操作，确保索引一致性
    let bytes = json.as_bytes();
    let cmd_key = b"\"command\":\"";
    let cmd_start = bytes.windows(cmd_key.len()).position(|w| w == cmd_key)?;
    let after_key = &bytes[cmd_start + cmd_key.len()..];
    let mut i = 0;
    while i < after_key.len() {
        if after_key[i] == b'"' {
            let is_escaped = i > 0 && after_key[i - 1] == b'\\';
            if !is_escaped {
                return String::from_utf8(after_key[..i].to_vec()).ok();
            }
        }
        i += 1;
    }
    None
}

/// 将文本内容渲染为无标题卡片 JSON 字符串
///
/// 统一卡片消息样式：不使用标题，只显示 markdown 内容。
/// DirectCardMessage 和 DirectStreamMessage 两条私聊直达路径共用此函数。
fn render_card_message(content: &str) -> String {
    let card = CardBuilder::new().markdown(content).build();
    render_card(&card, "")
}

/// 将 Finished 事件格式化为卡片 JSON 字符串
fn format_finished_card(event: &ExecEvent) -> Option<String> {
    match event {
        ExecEvent::Finished { success, todo_title, executor, duration_secs, total_tokens, .. } => {
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
                format!("{}s", duration_secs)
            };

            let content = format!(
                "**📋 {}**\n\n⚡ 执行器: `{}`\n⏱️ 用时: {}\n🔤 Token: {}",
                todo_title, executor, duration_str, total_tokens
            );

            let card = if *success {
                build_success_card("执行成功", &content)
            } else {
                build_error_card("执行失败", &content)
            };
            Some(render_card(&card, ""))
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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
            feishu_receive_id_type: None,
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
            feishu_receive_id_type: None,
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
            feishu_receive_id_type: None,
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

    /// 工具调用带编号：传入 Some(N) 时，应显示 "🔧 工具 #N: 工具名"
    #[test]
    fn test_render_log_entry_tool_call_with_index() {
        let entry = crate::models::ParsedLogEntry {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            log_type: "tool_call".to_string(),
            content: r#"{"command":"ls -la"}"#.to_string(),
            usage: None,
            tool_name: Some("Bash".to_string()),
            tool_input_json: Some(r#"{"command":"ls -la"}"#.to_string()),
        };
        let result = render_log_entry("", &entry, Some(3)).unwrap();
        assert!(result.contains("🔧 工具 #3: Bash"), "should contain tool index #3, got: {}", result);
        assert!(result.contains("ls -la"), "should contain command, got: {}", result);
    }

    /// 工具调用无编号：传入 None 时，保持原有格式 "🔧 工具名:"
    #[test]
    fn test_render_log_entry_tool_call_without_index() {
        let entry = crate::models::ParsedLogEntry {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            log_type: "tool_call".to_string(),
            content: r#"{"command":"ls -la"}"#.to_string(),
            usage: None,
            tool_name: Some("Bash".to_string()),
            tool_input_json: Some(r#"{"command":"ls -la"}"#.to_string()),
        };
        let result = render_log_entry("", &entry, None).unwrap();
        assert!(result.contains("🔧 Bash:"), "should contain old format without index, got: {}", result);
        assert!(!result.contains("工具 #"), "should not contain tool index, got: {}", result);
    }

    /// 非 tool_call 类型：传入编号也不应该显示工具编号
    #[test]
    fn test_render_log_entry_non_tool_call_ignores_index() {
        let entry = crate::models::ParsedLogEntry {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            log_type: "thinking".to_string(),
            content: "我需要分析一下这个问题".to_string(),
            usage: None,
            tool_name: None,
            tool_input_json: None,
        };
        let result = render_log_entry("", &entry, Some(5)).unwrap();
        assert!(result.starts_with("💭 "), "should keep thinking format, got: {}", result);
        assert!(!result.contains("工具 #"), "should not contain tool index for thinking, got: {}", result);
    }
}
