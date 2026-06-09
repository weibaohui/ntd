use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use crate::feishu::sdk::config::Config as FeishuSdkConfig;
use crate::feishu::sdk::token_manager::TokenManager;
use crate::feishu::{
    create_channel, ChannelMessage, FeishuChannelService, FeishuConfig, FeishuConnectionMode,
    FeishuDomain,
};

use crate::service_context::ServiceContext;
use crate::config::Config as AppConfig;
use crate::db::{Database, NewFeishuMessage};
use crate::models::{AgentBot, BotConfig, build_trigger_params};
use crate::services::message_debounce::{MessageDebounce, PendingMessage};

/// Manages WebSocket connections to Feishu for all bound bots.
#[derive(Clone)]
pub struct FeishuListener {
    ctx: ServiceContext,
    pub token_manager: Arc<TokenManager>,
    channels: Arc<DashMap<i64, Arc<FeishuChannelService>>>,
    /// bot_id → (app_id, app_secret, domain)
    pub bot_credentials: Arc<DashMap<i64, (String, String, String)>>,
    debounce: Arc<MessageDebounce>,
}

struct ListenerMessageContext<'a> {
    db: &'a Arc<Database>,
    config: &'a Arc<RwLock<AppConfig>>,
    token_manager: &'a Arc<TokenManager>,
    credentials: &'a DashMap<i64, (String, String, String)>,
    debounce: &'a Arc<MessageDebounce>,
    bot_id: i64,
    bot_open_id: &'a str,
    bot_config: &'a BotConfig,
}

struct FeishuCommandContext<'a> {
    db: &'a Arc<Database>,
    credentials: &'a DashMap<i64, (String, String, String)>,
    token_manager: &'a Arc<TokenManager>,
    bot_id: i64,
    chat_type: &'a str,
    sender: &'a str,
    channel: &'a str,
    message_id: &'a str,
    content: &'a str,
    reaction_id: Option<&'a str>,
}

impl FeishuListener {
    /// 创建飞书监听器。
    pub fn new(
        ctx: ServiceContext,
        debounce: Arc<MessageDebounce>,
    ) -> Self {
        Self {
            ctx,
            debounce,
            token_manager: Arc::new(TokenManager::new()),
            channels: Arc::new(DashMap::new()),
            bot_credentials: Arc::new(DashMap::new()),
        }
    }

    pub fn has_bot(&self, bot_id: i64) -> bool {
        self.channels.contains_key(&bot_id)
    }

    pub async fn start_bot(&self, bot: &AgentBot) -> anyhow::Result<()> {
        let domain = match bot.domain.as_deref() {
            Some("lark") => FeishuDomain::Lark,
            _ => FeishuDomain::Feishu,
        };

        let bot_config: BotConfig = serde_json::from_str(&bot.config).unwrap_or_default();

        let config = FeishuConfig {
            app_id: bot.app_id.clone(),
            app_secret: bot.app_secret.clone(),
            domain: domain.clone(),
            connection_mode: FeishuConnectionMode::WebSocket,
            allowed_users: vec!["*".into()],
            group_require_mention: bot_config.group_require_mention,
            dm_policy: None,
            group_policy: None,
            allow_from: None,
            group_allow_from: vec![],
            encrypt_key: None,
            verification_token: None,
            webhook_port: None,
        };

        let channel = Arc::new(create_channel(config));
        let (tx, mut rx) = mpsc::channel::<ChannelMessage>(256);

        let ch = channel.clone();
        let bot_id = bot.id;
        tokio::spawn(async move {
            tracing::info!("[feishu:{}] starting listen()", bot_id);
            match ch.listen(tx).await {
                Ok(()) => tracing::warn!("[feishu:{}] listen() returned Ok", bot_id),
                Err(e) => tracing::error!("[feishu:{}] listen() error: {e}", bot_id),
            }
        });

        self.channels.insert(bot.id, channel);
        let domain_str = match domain {
            FeishuDomain::Lark => "lark",
            _ => "feishu",
        };
        self.bot_credentials.insert(
            bot.id,
            (
                bot.app_id.clone(),
                bot.app_secret.clone(),
                domain_str.to_string(),
            ),
        );

        let real_bot_open_id =
            Self::resolve_bot_open_id(&self.bot_credentials, &self.token_manager, bot.id)
                .await
                .or(bot.bot_open_id.clone())
                .unwrap_or_default();
        if real_bot_open_id != bot.bot_open_id.clone().unwrap_or_default() {
            tracing::info!(
                "[feishu:{}] corrected bot_open_id from {:?} to {}",
                bot.id,
                bot.bot_open_id,
                real_bot_open_id
            );
        }

        let db = self.ctx.db.clone();
        let bot_open_id = real_bot_open_id;
        let bot_config_clone = bot_config;
        let credentials = self.bot_credentials.clone();
        let config = self.ctx.config.clone();
        let token_manager = self.token_manager.clone();
        let debounce = self.debounce.clone();
        tokio::spawn(async move {
            tracing::info!("[feishu:{}] message receiver loop started", bot_id);
            while let Some(msg) = rx.recv().await {
                let context = ListenerMessageContext {
                    db: &db,
                    config: &config,
                    token_manager: &token_manager,
                    credentials: &credentials,
                    debounce: &debounce,
                    bot_id,
                    bot_open_id: &bot_open_id,
                    bot_config: &bot_config_clone,
                };
                Self::handle_message(context, &msg).await;
            }
            tracing::warn!("[feishu:{}] message receiver loop ended", bot_id);
        });

        tracing::info!(
            "feishu listener started for bot {} ({})",
            bot.id,
            bot.bot_name
        );
        Ok(())
    }

    async fn handle_message(context: ListenerMessageContext<'_>, msg: &ChannelMessage) {
        let ListenerMessageContext {
            db,
            config,
            token_manager,
            credentials,
            debounce,
            bot_id,
            bot_open_id,
            bot_config,
        } = context;

        // 消息入口统一清理过期 binding：执行器崩溃或重启后 binding.status 可能卡在 running
        // 必须放在每条消息处理前，确保路由决策基于正确的状态
        if let Err(e) = db.cleanup_stale_running_bindings().await {
            tracing::warn!("[feishu:{}] cleanup_stale_running_bindings failed: {e}", bot_id);
        }

        tracing::info!(
            "[feishu:{}] handle_message: sender={}, bot_open_id={}, content={:?}, chat_type={:?}",
            bot_id,
            msg.sender,
            bot_open_id,
            msg.content,
            msg.chat_type
        );

        if msg.sender == bot_open_id {
            tracing::info!("[feishu:{}] skipping self-sent message", bot_id);
            return;
        }

        let chat_type = msg.chat_type.as_deref().unwrap_or("p2p");
        let is_mention = !msg.mentioned_open_ids.is_empty();

        db.save_feishu_message(NewFeishuMessage {
            bot_id,
            message_id: &msg.id,
            chat_id: &msg.channel,
            chat_type,
            sender_open_id: &msg.sender,
            sender_type: msg.sender_type.as_deref(),
            content: Some(&msg.content),
            msg_type: "text",
            is_mention,
        })
        .await
        .ok();

        let content = msg.content.trim();

        // Add "processing" reaction
        let reaction_id =
            Self::add_reaction(credentials, token_manager, bot_id, &msg.id, "THUMBSUP").await;

        // /sethome command
        if content == "/sethome" {
            Self::handle_sethome(FeishuCommandContext {
                db,
                credentials,
                token_manager,
                bot_id,
                chat_type,
                sender: &msg.sender,
                channel: &msg.channel,
                message_id: &msg.id,
                content,
                reaction_id: reaction_id.as_deref(),
            })
            .await;
            return;
        }

        // /feishupush command (toggle push)
        if content == "/feishupush" {
            Self::handle_feishupush(FeishuCommandContext {
                db,
                credentials,
                token_manager,
                bot_id,
                chat_type,
                sender: &msg.sender,
                channel: &msg.channel,
                message_id: &msg.id,
                content,
                reaction_id: reaction_id.as_deref(),
            })
            .await;
            return;
        }

        // /list command — list all registered project directories
        if content == "/list" {
            Self::handle_list(FeishuCommandContext {
                db,
                credentials,
                token_manager,
                bot_id,
                chat_type,
                sender: &msg.sender,
                channel: &msg.channel,
                message_id: &msg.id,
                content,
                reaction_id: reaction_id.as_deref(),
            })
            .await;
            return;
        }

        // /bind or /bind <project_name>
        if content == "/bind" || content.starts_with("/bind ") {
            Self::handle_bind(FeishuCommandContext {
                db,
                credentials,
                token_manager,
                bot_id,
                chat_type,
                sender: &msg.sender,
                channel: &msg.channel,
                message_id: &msg.id,
                content,
                reaction_id: reaction_id.as_deref(),
            })
            .await;
            return;
        }

        // /unbind command
        if content == "/unbind" {
            Self::handle_unbind(FeishuCommandContext {
                db,
                credentials,
                token_manager,
                bot_id,
                chat_type,
                sender: &msg.sender,
                channel: &msg.channel,
                message_id: &msg.id,
                content,
                reaction_id: reaction_id.as_deref(),
            })
            .await;
            return;
        }

        if !Self::is_message_allowed(chat_type, is_mention, bot_config) {
            if let Some(rid) = &reaction_id {
                Self::delete_reaction(credentials, token_manager, bot_id, &msg.id, rid).await;
            }
            return;
        }

        // Check if message response is enabled for this chat type
        let response_enabled = db
            .get_feishu_response_enabled(bot_id, chat_type)
            .await
            .unwrap_or(false);

        if !response_enabled {
            tracing::info!(
                "[feishu:{}] message response is disabled for {} chat type",
                bot_id,
                chat_type
            );
            if let Some(rid) = &reaction_id {
                Self::delete_reaction(credentials, token_manager, bot_id, &msg.id, rid).await;
            }
            return;
        }

        // Check group whitelist for group chats
        if chat_type == "group" {
            let in_whitelist = match db.is_sender_in_whitelist(bot_id, &msg.sender).await {
                Ok(allowed) => allowed,
                Err(e) => {
                    tracing::warn!(
                        "[feishu:{}] whitelist check failed for sender {}, defaulting to allow: {}",
                        bot_id,
                        msg.sender,
                        e
                    );
                    true
                }
            };
            if !in_whitelist {
                tracing::info!(
                    "[feishu:{}] sender {} not in group whitelist, skipping",
                    bot_id,
                    msg.sender
                );
                if let Some(rid) = &reaction_id {
                    Self::delete_reaction(credentials, token_manager, bot_id, &msg.id, rid).await;
                }
                return;
            }
        }

        // 检查当前聊天是否有项目目录绑定 → 走项目执行路径
        // 绑定路径优先级高于斜杠命令和默认回复：
        //   有绑定 → 最近一条 execution 还在运行 → resume 同一 session
        //   有绑定 → 最近一条已结束（或从未执行）→ 开新 session
        //   无绑定 → 降级到斜杠命令/默认回复
        // 为什么用 latest_record_id 判断而非 get_execution_record_by_task_id？
        //   resume 执行时新 record 的 task_id ≠ session_id，用 task_id 查会找到旧的
        //   已结束的 record 导致 should_resume=false，会话链断裂。
        match db.get_feishu_project_binding(bot_id, &msg.channel).await {
            Ok(Some(binding)) => {
                // 检查绑定的 todo 是否存在
                if let Ok(Some(todo)) = db.get_todo(binding.todo_id).await {
                    // Determine if we should resume an existing session or start fresh
                    // resume if: session_id exists AND the latest record is still running
                    // NOTE: we check latest_record_id (not get_execution_record_by_task_id)
                    // because resume executions have different task_ids — the session_id
                    // stays the same across turns, but the latest execution_record changes.
                    let should_resume = if let Some(rid) = binding.latest_record_id {
                        if let Ok(Some(record)) = db.get_execution_record(rid).await {
                            record.status == crate::models::ExecutionStatus::Running
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    let (resume_session_id, resume_message) = if should_resume {
                        // ⚠️ 不能使用 binding.session_id：debounce 首次执行时把它设为了 task_id（随机 UUID）
                        // Claude Code 真正的 session_id 来自 stdout JSONL 输出，
                        // 保存在 execution_records.session_id 中。必须从 latest_record 读取。
                        let real_sid = if let Some(rid) = binding.latest_record_id {
                            match db.get_execution_record(rid).await {
                                Ok(Some(r)) => r.session_id.or(binding.session_id.clone()),
                                _ => binding.session_id.clone(),
                            }
                        } else {
                            binding.session_id.clone()
                        };
                        (real_sid, Some(content.to_string()))
                    } else {
                        (None, None)
                    };

                    // Use the todo's configured executor, fallback to claudecode
                    let executor = todo.executor.as_deref().unwrap_or("claudecode");

                    debounce.push(PendingMessage {
                        bot_id,
                        chat_id: msg.channel.clone(),
                        chat_type: chat_type.to_string(),
                        sender: msg.sender.clone(),
                        content: content.to_string(),
                        todo_id: binding.todo_id,
                        todo_prompt: content.to_string(),
                        executor: Some(executor.to_string()),
                        trigger_type: "feishu_project_bind".to_string(),
                        params: None,
                        message_id: Some(msg.id.clone()),
                        resume_session_id,
                        resume_message,
                        binding_id: Some(binding.id),
                    });
                    return;
                } else {
                    tracing::warn!(
                        "[feishu:{}] bound todo #{} not found for chat {}",
                        bot_id, binding.todo_id, msg.channel
                    );
                }
            }
            Ok(None) => {} // No binding — fall through
            Err(e) => {
                tracing::error!("[feishu:{}] query binding failed: {e}", bot_id);
            }
        }

        if let Some(command_ctx) = Self::parse_slash_command(content) {
            // Try slash command match
            let matched_rule = {
                let cfg = config.read().await;
                cfg.slash_command_rules
                    .iter()
                    .find(|r| r.slash_command == command_ctx.command && r.enabled)
                    .cloned()
            };

            if let Some(rule) = matched_rule {
                if !command_ctx.body.is_empty() {
                    let todo = match db.get_todo(rule.todo_id).await {
                        Ok(Some(t)) => Some(t),
                        Ok(None) => None,
                        Err(e) => {
                            tracing::error!(
                                "Failed to fetch todo {} for slash command: {}",
                                rule.todo_id,
                                e
                            );
                            None
                        }
                    };
                    if let Some(todo) = todo {
                        let (_, params) = build_trigger_params(&format!("{} {}", command_ctx.command, command_ctx.body));

                        debounce.push(PendingMessage {
                            bot_id,
                            chat_id: msg.channel.clone(),
                            chat_type: chat_type.to_string(),
                            sender: msg.sender.clone(),
                            content: command_ctx.body.to_string(),
                            todo_id: todo.id,
                            todo_prompt: todo.prompt.clone(),
                            executor: todo.executor.clone(),
                            trigger_type: "slash_command".to_string(),
                            params: Some(params),
                            message_id: Some(msg.id.clone()),
                            resume_session_id: None,
                            resume_message: None,
                            binding_id: None,
                        });
                    }
                }
            } else {
                // No matching slash rule, fall through to default response
                let default_todo_id = {
                    let cfg = config.read().await;
                    cfg.default_response_todo_id
                };
                if let Some(todo_id) = default_todo_id {
                    if !content.is_empty() {
                        let todo_prompt = match db.get_todo(todo_id).await {
                            Ok(Some(t)) => Some(t.prompt.clone()),
                            Ok(None) => None,
                            Err(e) => {
                                tracing::error!(
                                    "Failed to fetch todo {} for debounce: {}",
                                    todo_id,
                                    e
                                );
                                None
                            }
                        }
                        .unwrap_or_default();
                        let (_, params) = build_trigger_params(content);
                        debounce.push(PendingMessage {
                            bot_id,
                            chat_id: msg.channel.clone(),
                            chat_type: chat_type.to_string(),
                            sender: msg.sender.clone(),
                            content: content.to_string(),
                            todo_id,
                            todo_prompt,
                            executor: None,
                            trigger_type: "default_response".to_string(),
                            params: Some(params),
                            message_id: Some(msg.id.clone()),
                            resume_session_id: None,
                            resume_message: None,
                            binding_id: None,
                        });
                    }
                }
            }
        } else {
            // Non-slash message, check default response
            let default_todo_id = {
                let cfg = config.read().await;
                cfg.default_response_todo_id
            };

            if let Some(todo_id) = default_todo_id {
                if !content.is_empty() {
                    let todo_prompt = match db.get_todo(todo_id).await {
                        Ok(Some(t)) => Some(t.prompt.clone()),
                        Ok(None) => None,
                        Err(e) => {
                            tracing::error!(
                                "Failed to fetch todo {} for message debounce: {}",
                                todo_id,
                                e
                            );
                            None
                        }
                    }
                    .unwrap_or_default();
                    let (_, params) = build_trigger_params(content);
                    debounce.push(PendingMessage {
                        bot_id,
                        chat_id: msg.channel.clone(),
                        chat_type: chat_type.to_string(),
                        sender: msg.sender.clone(),
                        content: content.to_string(),
                        todo_id,
                        todo_prompt,
                        executor: None,
                        trigger_type: "default_response".to_string(),
                        params: Some(params),
                        message_id: Some(msg.id.clone()),
                        resume_session_id: None,
                        resume_message: None,
                        binding_id: None,
                    });
                }
            }
        }

        if chat_type == "p2p" && bot_config.echo_reply {
            tracing::info!(
                "[feishu:{}] 收到私聊消息: sender={}, content={}",
                bot_id,
                msg.sender,
                content
            );
        }
        if chat_type == "group" && bot_config.echo_reply {
            tracing::info!(
                "[feishu:{}] 收到群聊消息: channel={}, sender={}, content={}",
                bot_id,
                msg.channel,
                msg.sender,
                content
            );
        }

        if let Some(rid) = &reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, &msg.id, rid).await;
        }
    }

    /// 判断当前消息是否符合接收配置。
    fn is_message_allowed(chat_type: &str, is_mention: bool, bot_config: &BotConfig) -> bool {
        match chat_type {
            "p2p" => bot_config.dm_enabled,
            "group" => {
                if !bot_config.group_enabled {
                    return false;
                }
                if bot_config.group_require_mention && !is_mention {
                    return false;
                }
                true
            }
            _ => true,
        }
    }

    /// 解析斜杠命令，只匹配首个词。
    fn parse_slash_command(content: &str) -> Option<SlashCommandMatch<'_>> {
        let trimmed = content.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let command = parts.next()?.trim();
        let body = parts.next().unwrap_or("").trim();
        Some(SlashCommandMatch { command, body })
    }

    async fn handle_sethome(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            bot_id,
            chat_type,
            sender,
            channel,
            message_id,
            reaction_id,
            ..
        } = context;
        let target_type = if chat_type == "p2p" { "p2p" } else { "group" };
        let (receive_id, receive_id_type, chat_id) = match chat_type {
            "p2p" => (sender.to_string(), "open_id", None),
            _ => (channel.to_string(), "chat_id", Some(channel.to_string())),
        };

        // Update feishu_home
        match db
            .set_feishu_home(
                bot_id,
                sender,
                chat_id.as_deref(),
                &receive_id,
                receive_id_type,
            )
            .await
        {
            Ok(_) => {
                tracing::info!(
                    "[feishu:{}] /sethome by {} → {} ({})",
                    bot_id,
                    sender,
                    receive_id,
                    receive_id_type
                );
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /sethome failed: {e}", bot_id);
            }
        }

        // Update only the relevant push target field
        if chat_type == "p2p" {
            if let Err(e) = db.set_p2p_receive_id(bot_id, &receive_id).await {
                tracing::error!("[feishu:{}] set p2p push target failed: {e}", bot_id);
            }
        } else if let Err(e) = db.set_group_chat_id(bot_id, channel).await {
            tracing::error!("[feishu:{}] set group push target failed: {e}", bot_id);
        }

        // Enable message response for this chat type
        if let Err(e) = db
            .set_feishu_response_enabled(bot_id, target_type, true)
            .await
        {
            tracing::error!("[feishu:{}] enable response failed: {e}", bot_id);
        }

        // Send confirmation
        let chat_type_label = if chat_type == "p2p" {
            "私聊"
        } else {
            "群聊"
        };
        let target_desc = if chat_type == "p2p" {
            "此私聊"
        } else {
            channel
        };
        let confirm = format!("✅ 已设置推送目标为此 {chat_type_label} ({target_desc})，执行过程将实时推送。\n\n如需关闭推送，请发送 /feishupush");
        Self::send_text(
            credentials,
            token_manager,
            bot_id,
            &receive_id,
            receive_id_type,
            &confirm,
        )
        .await;

        if let Some(rid) = reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /feishupush - cycle push level: disabled -> result_only -> all -> disabled.
    async fn handle_feishupush(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            bot_id,
            chat_type,
            sender,
            channel,
            message_id,
            reaction_id,
            ..
        } = context;
        let (receive_id, receive_id_type) = match chat_type {
            "p2p" => (sender.to_string(), "open_id"),
            _ => (channel.to_string(), "chat_id"),
        };

        let target = db.get_feishu_push_target(bot_id).await.ok().flatten();
        let current_level = target
            .as_ref()
            .map(|t| t.push_level.as_str())
            .unwrap_or("disabled");
        let new_level = match current_level {
            "disabled" => "result_only",
            "result_only" => "all",
            "all" => "disabled",
            _ => "disabled",
        };

        if let Err(e) = db.update_feishu_push_level(bot_id, new_level).await {
            tracing::error!("[feishu:{}] /feishupush update failed: {e}", bot_id);
            let msg = "⚠️ 操作失败，请稍后重试";
            Self::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                msg,
            )
            .await;
        } else {
            let (status_text, status_emoji) = match new_level {
                "disabled" => ("关闭", "ℹ️"),
                "result_only" => ("已切换为仅结论", "✅"),
                "all" => ("已切换为全部", "✅"),
                _ => ("未知", "⚠️"),
            };
            let msg = format!("{} 推送{}。", status_emoji, status_text);
            Self::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                &msg,
            )
            .await;
            tracing::info!(
                "[feishu:{}] /feishupush: push level changed to {} for bot_id={}",
                bot_id,
                new_level,
                bot_id
            );
        }

        if let Some(rid) = reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /list — list all registered project directories.
    async fn handle_list(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            bot_id,
            chat_type,
            sender,
            channel,
            message_id,
            reaction_id,
            ..
        } = context;
        let (receive_id, receive_id_type) = match chat_type {
            "p2p" => (sender.to_string(), "open_id"),
            _ => (channel.to_string(), "chat_id"),
        };

        let directories = db.get_project_directories().await.unwrap_or_default();
        if directories.is_empty() {
            Self::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                "📂 暂无已注册的项目目录。\n\n请在 Web 设置页「项目目录」中添加，或使用 /bind <名称> 绑定一个项目（首次使用会自动创建）。",
            )
            .await;
        } else {
            let mut lines: Vec<String> = directories
                .iter()
                .map(|d| {
                    let name = d.name.as_deref().unwrap_or("(未命名)");
                    format!("• {}  →  {}", name, d.path)
                })
                .collect();
            lines.insert(0, format!("📂 已注册的项目目录（共 {} 个）：", directories.len()));
            lines.push(String::new());
            lines.push("💡 使用 /bind <名称> 绑定到本项目聊天".to_string());
            Self::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                &lines.join("\n"),
            )
            .await;
        }

        if let Some(rid) = reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /bind — show current binding, or /bind <name> to bind to a project.
    async fn handle_bind(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            bot_id,
            chat_type,
            sender,
            channel,
            message_id,
            content,
            reaction_id,
        } = context;
        let (receive_id, receive_id_type) = match chat_type {
            "p2p" => (sender.to_string(), "open_id"),
            _ => (channel.to_string(), "chat_id"),
        };

        // /bind with no args → show current binding status
        if content == "/bind" {
            match db.get_feishu_project_binding(bot_id, channel).await {
                Ok(Some(binding)) => {
                    let dir = db.get_project_directory_by_id(binding.project_dir_id).await.ok().flatten();
                    let dir_name = dir.as_ref().and_then(|d| d.name.as_deref()).unwrap_or("(unknown)");
                    let dir_path = dir.as_ref().map(|d| d.path.as_str()).unwrap_or("(unknown)");
                    let status_icon = if binding.status == "running" { "🟢" } else { "⏸️" };
                    let msg = format!(
                        "📎 当前绑定详情：\n项目：{dir_name}\n目录：{dir_path}\nTodo：#{binding_id}\n状态：{status_icon} {binding_status}\nSession：{session}\n\n💡 使用 /unbind 解绑",
                        binding_id = binding.todo_id,
                        binding_status = binding.status,
                        session = binding.session_id.as_deref().unwrap_or("(无)"),
                    );
                    Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, &msg).await;
                }
                Ok(None) => {
                    Self::send_text(
                        credentials, token_manager, bot_id, &receive_id, receive_id_type,
                        "📭 当前聊天未绑定任何项目。\n\n使用 /bind <项目名称> 绑定一个项目。\n使用 /list 查看可用项目。",
                    )
                    .await;
                }
                Err(e) => {
                    tracing::error!("[feishu:{}] /bind query failed: {e}", bot_id);
                    Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, "⚠️ 查询绑定失败，请稍后重试").await;
                }
            }
            if let Some(rid) = reaction_id {
                Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
            }
            return;
        }

        // /bind <name> — bind to a project by name
        let project_name = content.strip_prefix("/bind ").unwrap_or("").trim();
        if project_name.is_empty() {
            Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, "⚠️ 请输入项目名称，例如：/bind my-app").await;
            if let Some(rid) = reaction_id {
                Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
            }
            return;
        }

        // 按项目名称查找：先精确匹配，再前缀匹配
        // ⚠️ 前缀匹配时若多个目录共享相同前缀（如 my-app / my-application），
        //    优先匹配数据库中先插入的那条，不会弹出歧义提示。
        //    用户可通过输入完整名称避免歧义。
        let directories = db.get_project_directories().await.unwrap_or_default();
        let dir = directories.iter().find(|d| d.name.as_deref() == Some(project_name))
            .or_else(|| directories.iter().find(|d| d.name.as_deref().map(|n| n.starts_with(project_name)).unwrap_or(false)))
            .cloned();

        match dir {
            Some(dir) => {
                // Check if already bound
                if let Ok(Some(existing)) = db.get_feishu_project_binding(bot_id, channel).await {
                    let _ = db.delete_feishu_project_binding(existing.id).await;
                }

                // Try to find a pending binding created via Web UI (chat_id='__pending__')
                let pending_bindings = db.get_feishu_project_bindings(bot_id).await.unwrap_or_default();
                let pending = pending_bindings.iter()
                    .find(|b| b.project_dir_id == dir.id && b.chat_id == "__pending__")
                    .cloned();

                if let Some(pending_binding) = pending {
                    // Reuse the pending binding and its todo — just update chat_id/chat_type
                    match db.attach_feishu_project_binding(pending_binding.id, channel, chat_type).await {
                        Ok(_) => {
                            let dir_name = dir.name.as_deref().unwrap_or("unknown");
                            let msg = format!(
                                "✅ 已绑定到项目「{dir_name}」\n项目目录：{path}\nTodo：#{todo_id}\n\n现在可以直接向我发送任务了。",
                                path = dir.path,
                                todo_id = pending_binding.todo_id,
                            );
                            Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, &msg).await;
                        }
                        Err(e) => {
                            tracing::error!("[feishu:{}] update pending binding failed: {e}", bot_id);
                            Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, "⚠️ 绑定更新失败，请稍后重试").await;
                        }
                    }
                } else {
                    // No pending binding — create a new Todo + binding
                    let todo_title = format!("飞书-{}", dir.name.as_deref().unwrap_or(&dir.path));
                    let todo_prompt = format!(
                        "你是飞书Bot的AI助手，正在项目「{name}」({path})中工作。\n\
                         用户通过飞书与你交流，请根据用户的需求在项目目录中完成开发任务。\n\
                         你可以读取、修改项目文件，运行命令等。\n\n\
                         项目目录：{path}",
                        name = dir.name.as_deref().unwrap_or("unknown"),
                        path = dir.path,
                    );

                    match db.create_todo(&todo_title, &todo_prompt).await {
                        Ok(todo_id) => {
                            let _ = db.update_todo_workspace(todo_id, Some(&dir.path)).await;
                            let _ = db.update_todo_worktree_enabled(todo_id, true).await;
                            match db.create_feishu_project_binding(bot_id, channel, chat_type, dir.id, todo_id).await {
                                Ok(binding_id) => {
                                    let dir_name = dir.name.as_deref().unwrap_or("unknown");
                                    let msg = format!(
                                        "✅ 已绑定到项目「{dir_name}」\n项目目录：{path}\nTodo：#{todo_id}\n\n现在可以直接向我发送任务了。",
                                        path = dir.path,
                                    );
                                    Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, &msg).await;
                                    tracing::info!("[feishu:{}] bound chat {} to project {} (binding={}, todo={})", bot_id, channel, dir.path, binding_id, todo_id);
                                }
                                Err(e) => {
                                    tracing::error!("[feishu:{}] create binding failed: {e}", bot_id);
                                    Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, "⚠️ 创建绑定失败，请稍后重试").await;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("[feishu:{}] create todo failed: {e}", bot_id);
                            Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, "⚠️ 创建 Todo 失败，请稍后重试").await;
                        }
                    }
                }
            }
            None => {
                let msg = format!(
                    "⚠️ 未找到名为「{name}」的项目。\n\n使用 /list 查看所有可用项目。",
                    name = project_name
                );
                Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, &msg).await;
            }
        }

        if let Some(rid) = reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /unbind — unbind current chat from its project.
    async fn handle_unbind(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            bot_id,
            chat_type,
            sender,
            channel,
            message_id,
            reaction_id,
            ..
        } = context;
        let (receive_id, receive_id_type) = match chat_type {
            "p2p" => (sender.to_string(), "open_id"),
            _ => (channel.to_string(), "chat_id"),
        };

        match db.get_feishu_project_binding(bot_id, channel).await {
            Ok(Some(binding)) => {
                // If a task is running, warn user before deleting
                if binding.status == "running" {
                    Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type,
                        "⚠️ 当前有任务正在执行，解绑后任务仍会在后台运行。\n如需强制终止，请使用 Web 界面「运行管理」停止。")
                        .await;
                }
                if let Err(e) = db.delete_feishu_project_binding(binding.id).await {
                    tracing::error!("[feishu:{}] /unbind failed: {e}", bot_id);
                    Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, "⚠️ 解绑失败，请稍后重试").await;
                } else {
                    Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, "✅ 已解绑。使用 /bind <名称> 重新绑定到其他项目。").await;
                }
            }
            Ok(None) => {
                Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, "📭 当前聊天未绑定任何项目，无需解绑。").await;
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /unbind query failed: {e}", bot_id);
                Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, "⚠️ 查询绑定失败，请稍后重试").await;
            }
        }

        if let Some(rid) = reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Send a plain text message to a Feishu recipient.
    async fn send_text(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        receive_id: &str,
        receive_id_type: &str,
        text: &str,
    ) {
        let base_url = match Self::base_url(credentials, bot_id) {
            Some(u) => u,
            None => return,
        };
        let token = match Self::get_tenant_token(credentials, token_manager, bot_id).await {
            Some(t) => t,
            None => return,
        };

        let client = reqwest::Client::new();
        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type={}",
            base_url, receive_id_type
        );
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": serde_json::to_string(&serde_json::json!({ "text": text })).unwrap_or_default()
        });

        match client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(res) => {
                let status = res.status();
                if !status.is_success() {
                    tracing::error!("[feishu:{}] send_text failed: status={}", bot_id, status);
                } else {
                    tracing::debug!(
                        "[feishu:{}] send_text ok to {} ({})",
                        bot_id,
                        receive_id,
                        receive_id_type
                    );
                }
            }
            Err(e) => {
                tracing::error!("[feishu:{}] send_text request failed: {e}", bot_id);
            }
        }
    }

    /// Send a message via a specific bot's channel.
    pub async fn send(&self, bot_id: i64, text: &str, recipient: &str) -> anyhow::Result<()> {
        if let Some(ch) = self.channels.get(&bot_id) {
            ch.send(text, recipient).await?;
            Ok(())
        } else {
            anyhow::bail!("bot {} not running", bot_id)
        }
    }

    /// Send a raw text message using a specific receive_id_type.
    pub async fn send_raw(
        &self,
        bot_id: i64,
        receive_id: &str,
        receive_id_type: &str,
        text: &str,
    ) -> anyhow::Result<()> {
        let base_url = Self::base_url(&self.bot_credentials, bot_id)
            .ok_or_else(|| anyhow::anyhow!("no credentials for bot {}", bot_id))?;
        let token = Self::get_tenant_token(&self.bot_credentials, &self.token_manager, bot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no token for bot {}", bot_id))?;

        let client = reqwest::Client::new();
        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type={}",
            base_url, receive_id_type
        );
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": serde_json::to_string(&serde_json::json!({ "text": text })).unwrap_or_default()
        });

        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = res.status();
        if !status.is_success() {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            return Err(anyhow::anyhow!("send_raw failed: {} {:?}", status, body));
        }

        Ok(())
    }

    // --- Feishu API helpers ---

    fn base_url(
        credentials: &DashMap<i64, (String, String, String)>,
        bot_id: i64,
    ) -> Option<String> {
        let domain = credentials.get(&bot_id)?.2.clone();
        Some(if domain == "lark" {
            "https://open.larksuite.com".to_string()
        } else {
            "https://open.feishu.cn".to_string()
        })
    }

    fn build_sdk_config(
        credentials: &DashMap<i64, (String, String, String)>,
        bot_id: i64,
    ) -> Option<FeishuSdkConfig> {
        let ref_val = credentials.get(&bot_id)?;
        let (app_id, app_secret, domain) =
            (ref_val.0.clone(), ref_val.1.clone(), ref_val.2.clone());
        let base_url = if domain == "lark" {
            "https://open.larksuite.com"
        } else {
            "https://open.feishu.cn"
        };

        Some(
            FeishuSdkConfig::builder()
                .app_id(app_id)
                .app_secret(app_secret)
                .base_url(base_url)
                .enable_token_cache(true)
                .http_client(reqwest::Client::new())
                .build(),
        )
    }

    async fn get_tenant_token(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
    ) -> Option<String> {
        let sdk_config = Self::build_sdk_config(credentials, bot_id)?;
        match token_manager.get_tenant_access_token(&sdk_config).await {
            Ok(token) => Some(token),
            Err(err) => {
                tracing::warn!("[feishu:{}] 获取 tenant_access_token 失败: {}", bot_id, err);
                None
            }
        }
    }

    async fn resolve_bot_open_id(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
    ) -> Option<String> {
        let token = Self::get_tenant_token(credentials, token_manager, bot_id).await?;
        let base_url = Self::base_url(credentials, bot_id)?;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/open-apis/bot/v3/info"))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .ok()?;

        let body: serde_json::Value = res.json().await.ok()?;
        body.get("bot")
            .and_then(|b| b.get("open_id"))
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    /// Add reaction, returns reaction_id on success.
    async fn add_reaction(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        message_id: &str,
        emoji_type: &str,
    ) -> Option<String> {
        let token = Self::get_tenant_token(credentials, token_manager, bot_id).await?;
        let base_url = Self::base_url(credentials, bot_id)?;

        let client = reqwest::Client::new();
        let url = format!("{base_url}/open-apis/im/v1/messages/{message_id}/reactions");
        let body_json = serde_json::json!({
            "reaction_type": {
                "emoji_type": emoji_type
            }
        });
        tracing::info!(
            "[feishu:{}] add_reaction POST {} token={}... body={}",
            bot_id,
            url,
            &token[..token.len().min(10)],
            body_json
        );
        let res = match client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&body_json)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[feishu:{}] add_reaction request failed: {e}", bot_id);
                return None;
            }
        };

        let status = res.status();
        let body: serde_json::Value = match res.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("[feishu:{}] add_reaction parse failed: {e}", bot_id);
                return None;
            }
        };

        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            tracing::error!(
                "[feishu:{}] add_reaction API error (status={}): {body}",
                bot_id,
                status
            );
            return None;
        }

        let reaction_id = body
            .get("data")
            .and_then(|d| d.get("reaction_id"))
            .and_then(|v| v.as_str())
            .map(String::from);

        tracing::info!(
            "[feishu:{}] add_reaction {} ok, reaction_id={:?}",
            bot_id,
            emoji_type,
            reaction_id
        );
        reaction_id
    }

    /// Delete reaction by reaction_id.
    async fn delete_reaction(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        message_id: &str,
        reaction_id: &str,
    ) {
        let token = match Self::get_tenant_token(credentials, token_manager, bot_id).await {
            Some(t) => t,
            None => return,
        };
        let base_url = match Self::base_url(credentials, bot_id) {
            Some(u) => u,
            None => return,
        };

        let client = reqwest::Client::new();
        match client
            .delete(format!(
                "{base_url}/open-apis/im/v1/messages/{message_id}/reactions/{reaction_id}"
            ))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
        {
            Ok(res) => {
                let body: serde_json::Value = res.json().await.unwrap_or_default();
                let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
                if code == 0 {
                    tracing::info!("[feishu:{}] delete_reaction ok", bot_id);
                } else {
                    tracing::error!("[feishu:{}] delete_reaction API error: {body}", bot_id);
                }
            }
            Err(e) => {
                tracing::error!("[feishu:{}] delete_reaction request failed: {e}", bot_id);
            }
        }
    }
}

struct SlashCommandMatch<'a> {
    command: &'a str,
    body: &'a str,
}

#[cfg(test)]
mod tests {
    use super::FeishuListener;
    use crate::models::BotConfig;

    #[test]
    fn test_parse_slash_command_exact_first_token() {
        let parsed = FeishuListener::parse_slash_command("/todo 帮我整理今天任务").unwrap();
        assert_eq!(parsed.command, "/todo");
        assert_eq!(parsed.body, "帮我整理今天任务");
    }

    #[test]
    fn test_parse_slash_command_without_body() {
        let parsed = FeishuListener::parse_slash_command("/todo").unwrap();
        assert_eq!(parsed.command, "/todo");
        assert_eq!(parsed.body, "");
    }

    #[test]
    fn test_group_message_requires_mention_when_enabled() {
        let cfg = BotConfig {
            group_enabled: true,
            group_require_mention: true,
            ..Default::default()
        };
        assert!(!FeishuListener::is_message_allowed("group", false, &cfg));
        assert!(FeishuListener::is_message_allowed("group", true, &cfg));
    }
}
