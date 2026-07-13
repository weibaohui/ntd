use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::feishu::sdk::config::Config as FeishuSdkConfig;
use crate::feishu::sdk::token_manager::TokenManager;
use crate::feishu::{
    create_channel, ChannelMessage, FeishuChannelService, FeishuConfig, FeishuConnectionMode,
    FeishuDomain,
};

use crate::service_context::ServiceContext;
use crate::task_manager::TaskManager;
use crate::db::{Database, NewFeishuMessage};
use crate::services::feishu_card::{
    Card, CardElement, CardMarkdown, ExecutorOption, HelpCardState, HistoryItem, LoopItem, RecentTaskItem,
    TodoItem, WorkspaceItem, WorkspaceSummary, build_help_console_card, build_history_card,
    render_card,
};
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

pub(crate) struct ListenerMessageContext<'a> {
    pub(crate) db: &'a Arc<Database>,
    pub(crate) token_manager: &'a Arc<TokenManager>,
    pub(crate) credentials: &'a DashMap<i64, (String, String, String)>,
    pub(crate) debounce: &'a Arc<MessageDebounce>,
    pub(crate) task_manager: &'a Arc<TaskManager>,
    pub(crate) bot_id: i64,
    pub(crate) bot_open_id: &'a str,
    pub(crate) bot_config: &'a BotConfig,
    /// ServiceContext：供 act:/runtodo 构造 RunTodoExecutionRequest（需 executor_registry/tx/config）。
    pub(crate) ctx: &'a ServiceContext,
}

struct FeishuCommandContext<'a> {
    db: &'a Arc<Database>,
    credentials: &'a DashMap<i64, (String, String, String)>,
    token_manager: &'a Arc<TokenManager>,
    /// ServiceContext：供 handle_help 查可用执行器列表（assemble_help_card_state 需要）。
    ctx: &'a ServiceContext,
    bot_id: i64,
    chat_type: &'a str,
    sender: &'a str,
    channel: &'a str,
    message_id: &'a str,
    content: &'a str,
    reaction_id: Option<&'a str>,
}

/// 卡片 act:/ 动作（点击按钮要执行的副作用）。
/// parse_card_action 把 "act:/xxx" 解析成它，handle_card_callback 的 act 分支按它分发。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CardAction {
    Stop,
    New,
    SetHome,
    /// 切换工作空间，参数为 workspace_id
    Bind(i64),
    /// 触发事项，参数为 todo_id
    RunTodo(i64),
    /// 触发环路，参数为 loop_id
    RunLoop(i64),
    /// 设置推送级别，参数为 disabled/result_only/all
    Push(String),
    /// 设置默认执行器，参数为执行器名（ExecutorType::as_str）
    SetExecutor(String),
}

/// 把 started_at（ISO，如 "2026-07-11T14:04:37Z"）格式化成可读时间：取前 16 位 + T 换空格。
/// 不引入 chrono 依赖，精度足够卡片展示。
fn format_record_time(started_at: &str) -> String {
    started_at.get(..16).unwrap_or(started_at).replace('T', " ")
}

/// 卡片 act 动作的执行结果（供 patch_after_action 渲染顶部提示）。
struct ActionOutcome {
    success: bool,
    message: String,
}

/// 编排器专用：handle_message 阶段函数之间传递的"消息预处理结果"。
/// 把 trim content / chat_type / is_mention / reaction_id 这类一次性解析的字段聚在一起，
/// 避免每个阶段函数都重复算一遍，编排器也只需要在 phases 间传一个 &MessagePrep。
pub(crate) struct MessagePrep<'a> {
    pub(crate) chat_type: &'a str,
    pub(crate) content: &'a str,
    pub(crate) is_mention: bool,
    pub(crate) reaction_id: Option<String>,
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
        let token_manager = self.token_manager.clone();
        let debounce = self.debounce.clone();
        let task_manager = self.ctx.task_manager.clone();
        let ctx_clone = self.ctx.clone();
        tokio::spawn(async move {
            tracing::info!("[feishu:{}] message receiver loop started", bot_id);
            while let Some(msg) = rx.recv().await {
                tracing::debug!(
                    "[feishu:{}] receiver got message: sender={}, channel={}, content_len={}",
                    bot_id, msg.sender, msg.channel, msg.content.len()
                );
                let context = ListenerMessageContext {
                    db: &db,
                    token_manager: &token_manager,
                    credentials: &credentials,
                    debounce: &debounce,
                    task_manager: &task_manager,
                    bot_id,
                    bot_open_id: &bot_open_id,
                    bot_config: &bot_config_clone,
                    ctx: &ctx_clone,
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


    // ---------------------------------------------------------------
    // handle_message 编排：把原来 519 行的单体函数拆成"阶段函数"串联。
    // 每个阶段职责单一，bool 返回值告知编排器是否终止。
    // 新增命令 / 改权限逻辑 / 改绑定逻辑时只动对应阶段，不会牵动整段流程。
    // ---------------------------------------------------------------
    async fn handle_message(context: ListenerMessageContext<'_>, msg: &ChannelMessage) {
        // 入口日志：排查"消息为什么没反应"的第一线索
        tracing::info!(
            "[feishu:{}] handle_message: sender={}, bot_open_id={}, content={:?}, chat_type={:?}",
            context.bot_id, msg.sender, context.bot_open_id, msg.content, msg.chat_type
        );
        // 阶段 0：卡片回调处理（由飞书卡片按钮点击触发）
        if msg.chat_type.as_deref() == Some("card_callback") {
            Self::handle_card_callback(context, msg).await;
            return;
        }
        // 阶段 0a：跳过机器人自己发的消息（不持久化、不加 reaction）
        if msg.sender == context.bot_open_id {
            tracing::info!("[feishu:{}] skipping self-sent message", context.bot_id);
            return;
        }
        // 阶段 1：解析消息 + 持久化 + 加 reaction，产出 MessagePrep 给后续阶段复用
        let prep = Self::prepare_message(&context, msg).await;
        // 阶段 2~7：每个阶段 bool 返回 true → 编排器直接 return
        if Self::try_route_builtin_command(&context, msg, &prep).await { return; }
        if Self::should_skip_for_message_filters(&context, msg, &prep).await { return; }
        // 阶段4/5（pending binding 晋升 / project binding 路由）已废弃：
        // 一个 bot 一个工作空间，chat 消息全走 default_response（阶段6）。
        Self::route_slash_or_default_response(&context, msg, &prep).await;
        Self::log_echo_reply(context.bot_id, msg, prep.chat_type, context.bot_config);
        Self::cleanup_reaction(&context, msg, prep.reaction_id.as_deref()).await;
    }

    /// 阶段 1：解析消息基本信息 + 持久化入站消息 + 加 processing reaction
    /// 返回 MessagePrep 供后续阶段复用（避免每个阶段重复 trim content / 查 chat_type）
    pub(crate) async fn prepare_message<'a>(
        context: &ListenerMessageContext<'_>,
        msg: &'a ChannelMessage,
    ) -> MessagePrep<'a> {
        let chat_type = msg.chat_type.as_deref().unwrap_or("p2p");
        let is_mention = !msg.mentioned_open_ids.is_empty();
        let content = msg.content.trim();
        // 持久化是 audit 用途，失败仅记录；不影响主流程决策
        // workspace_id 在消息接收时确定，记录该 bot 所属的工作空间
        let workspace_id = context.db.get_agent_bot_workspace_id(context.bot_id).await.unwrap_or(None);
        Self::persist_inbound_message(context.db, context.bot_id, msg, chat_type, is_mention, workspace_id).await;
        // 非扫码创建的 bot 没有 owner_open_id，私聊时兜底捕获说话人作为所有者（推送目标）。
        // 放在阶段1：无论后续是否被过滤，只要是私聊就捕获一次，覆盖"首次私聊"语义。
        Self::capture_owner_if_p2p(context.db, context.bot_id, msg, chat_type).await;
        let reaction_id = Self::add_processing_reaction(
            context.credentials, context.token_manager, context.bot_id, &msg.id,
        ).await;
        MessagePrep { chat_type, content, is_mention, reaction_id }
    }

    /// 阶段 1a：把入站消息落库到 feishu_messages 表
    async fn persist_inbound_message(
        db: &Arc<Database>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        is_mention: bool,
        workspace_id: Option<i64>,
    ) {
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
            workspace_id,
        })
        .await
        .ok();
    }

    /// 阶段 1c：私聊场景兜底捕获 owner_open_id。
    ///
    /// 扫码创建的 bot 在建表时已写入 owner_open_id；非扫码创建（手动填 app_id）的 bot
    /// 该字段为空，这里靠"首次私聊"补上。群聊不捕获——群消息 sender 是群里某个人，
    /// 并非 bot 所有者。实际写入由 set_owner_open_id_if_empty 的「为空才写」护栏决定，
    /// 因此后到的私聊用户不会覆盖已锁定的所有者。
    async fn capture_owner_if_p2p(
        db: &Arc<Database>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
    ) {
        if chat_type != "p2p" {
            return;
        }
        match db.set_owner_open_id_if_empty(bot_id, &msg.sender).await {
            Ok(true) => tracing::info!("[feishu] bot {} owner_open_id 兜底设置为 {}", bot_id, &msg.sender),
            Ok(false) => tracing::debug!("[feishu] bot {} owner_open_id 已存在，跳过兜底", bot_id),
            Err(e) => tracing::warn!("[feishu] bot {} 兜底 owner_open_id 失败: {}", bot_id, e),
        }
    }

    /// 阶段 1b：加 THUMBSUP reaction 表示"处理中"
    async fn add_processing_reaction(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        message_id: &str,
    ) -> Option<String> {
        Self::add_reaction(credentials, token_manager, bot_id, message_id, "THUMBSUP").await
    }

    /// 阶段 2：内置斜杠命令路由（命中并处理后返回 true）
    /// 命令与处理函数的映射写在内部 if 链里，新增命令时在这里加一行
    pub(crate) async fn try_route_builtin_command(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) -> bool {
        // 把 listener 字段聚成 builder，命令分支只关心命令名 + 处理函数
        let mk_ctx = || FeishuCommandContext {
            db: context.db,
            credentials: context.credentials,
            token_manager: context.token_manager,
            ctx: context.ctx,
            bot_id: context.bot_id,
            chat_type: prep.chat_type,
            sender: &msg.sender,
            channel: &msg.channel,
            message_id: &msg.id,
            content: prep.content,
            reaction_id: prep.reaction_id.as_deref(),
        };
        if prep.content == "/sethome" { Self::handle_sethome(mk_ctx()).await; return true; }
        if prep.content == "/feishupush" { Self::handle_feishupush(mk_ctx()).await; return true; }
        if prep.content == "/list" { Self::handle_list(mk_ctx()).await; return true; }
        if prep.content == "/help" || prep.content.starts_with("/help ") {
            Self::handle_help(mk_ctx()).await; return true;
        }
        if prep.content == "/new" { Self::handle_new(mk_ctx()).await; return true; }
        if prep.content == "/stop" {
            Self::handle_stop(context.task_manager, mk_ctx()).await; return true;
        }
        false
    }

    /// 阶段 3：消息接收过滤（命中任一条就 return true）
    /// 三道闸：bot 是否接收此类消息 → 该 chat_type 是否启用响应 → 群聊白名单
    pub(crate) async fn should_skip_for_message_filters(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) -> bool {
        // 闸 1：bot 接收策略（私聊启用 / 群聊启用 + 是否需要 @）
        if !Self::is_message_allowed(prep.chat_type, prep.is_mention, context.bot_config) {
            tracing::info!(
                "[feishu:{}] message not allowed: chat_type={}, is_mention={}, group_enabled={}, group_require_mention={}, dm_enabled={}",
                context.bot_id, prep.chat_type, prep.is_mention,
                context.bot_config.group_enabled, context.bot_config.group_require_mention,
                context.bot_config.dm_enabled
            );
            Self::cleanup_reaction(context, msg, prep.reaction_id.as_deref()).await;
            return true;
        }
        // 闸 2：当前 chat_type 是否开启消息响应（用户可在 bot 配置里单独关闭群/私聊）
        if !context.db.get_feishu_response_enabled(context.bot_id, prep.chat_type)
            .await.unwrap_or(false)
        {
            tracing::info!(
                "[feishu:{}] message response is disabled for {} chat type",
                context.bot_id, prep.chat_type
            );
            Self::cleanup_reaction(context, msg, prep.reaction_id.as_deref()).await;
            return true;
        }
        // 闸 3：群聊白名单；DB 失败默认放行（fail-open，避免 DB 抖动让所有群聊哑火）
        if prep.chat_type == "group"
            && !Self::is_group_sender_allowed(context.db, context.bot_id, &msg.sender).await
        {
            tracing::info!(
                "[feishu:{}] sender {} not in group whitelist, skipping",
                context.bot_id, msg.sender
            );
            Self::cleanup_reaction(context, msg, prep.reaction_id.as_deref()).await;
            return true;
        }
        false
    }

    /// 闸 3 的子步骤：群聊 sender 是否在白名单
    /// 抽出来让 should_skip_for_message_filters 保持简洁；DB 错误默认放行
    async fn is_group_sender_allowed(
        db: &Arc<Database>,
        bot_id: i64,
        sender: &str,
    ) -> bool {
        match db.is_sender_in_whitelist(bot_id, sender).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "[feishu:{}] whitelist check failed for sender {}, defaulting to allow: {}",
                    bot_id, sender, e
                );
                true
            }
        }
    }

    /// 删除 THUMBSUP reaction（reaction_id 为 None 时是 no-op）
    pub(crate) async fn cleanup_reaction(
        context: &ListenerMessageContext<'_>,
        message: &ChannelMessage,
        reaction_id: Option<&str>,
    ) {
        let Some(rid) = reaction_id else { return };
        Self::delete_reaction(
            context.credentials, context.token_manager, context.bot_id, &message.id, rid,
        ).await;
    }

    /// 阶段 6：兜底路由（自定义斜杠命令规则 或 默认回复 todo）
    pub(crate) async fn route_slash_or_default_response(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) {
        // 是斜杠命令 → 走规则匹配；规则没命中再降级到默认回复
        if let Some(command_ctx) = Self::parse_slash_command(prep.content) {
            Self::dispatch_slash_command(context, msg, prep, &command_ctx).await;
        } else {
            Self::dispatch_default_response(context, msg, prep).await;
        }
    }

    /// 阶段 6a：自定义斜杠命令规则匹配 + debounce push
    /// 没匹配上规则时降级到默认回复，避免静默丢消息
    async fn dispatch_slash_command(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
        command_ctx: &SlashCommandMatch<'_>,
    ) {
        // 先获取 bot 的 workspace_id
        let workspace_id = match context.db.get_agent_bot_workspace_id(context.bot_id).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::warn!("bot {} has no workspace_id, skipping slash command", context.bot_id);
                return Self::dispatch_default_response(context, msg, prep).await;
            }
            Err(e) => {
                tracing::error!("failed to get workspace_id for bot {}: {}", context.bot_id, e);
                return Self::dispatch_default_response(context, msg, prep).await;
            }
        };

        let matched_rule = Self::find_slash_rule(context.db, workspace_id, command_ctx.command).await;
        let Some(rule) = matched_rule else {
            // 没匹配上规则 → 走默认回复路径，保持向后兼容
            return Self::dispatch_default_response(context, msg, prep).await;
        };

        // 根据 command_type 分发到 todo 或 loop 处理
        match rule.command_type.as_str() {
            "loop" => {
                // 斜杠命令触发环路
                let Some(loop_id) = rule.loop_id else {
                    tracing::error!("slash command {} has loop_id=null, skipping", command_ctx.command);
                    return;
                };
                Self::push_slash_command_loop_message(
                    context.debounce,
                    context.bot_id,
                    msg,
                    prep.chat_type,
                    loop_id,
                    command_ctx.body,
                    Some(workspace_id),
                );
            }
            _ => {
                // 默认为 todo 类型（保持向后兼容）
                let Ok(Some(todo)) = context.db.get_todo(rule.todo_id).await else {
                    tracing::error!("Failed to fetch todo {} for slash command", rule.todo_id);
                    return;
                };
                let trigger_str = if command_ctx.body.is_empty() {
                    command_ctx.command.to_string()
                } else {
                    format!("{} {}", command_ctx.command, command_ctx.body)
                };
                let (_, params) = build_trigger_params(&trigger_str);
                Self::push_slash_command_message(context.debounce, context.bot_id, msg, prep.chat_type, &todo, command_ctx.body, params, Some(workspace_id));
            }
        }
    }

    /// 阶段 6a-i：查 enabled 的斜杠命令规则（按 workspace 查询）
    pub(crate) async fn find_slash_rule(
        db: &Database,
        workspace_id: i64,
        command: &str,
    ) -> Option<crate::db::entity::workspace_slash_commands::Model> {
        crate::db::workspace_slash_command::get_workspace_slash_command(db, workspace_id, command)
            .await
            .ok()
            .flatten()
            .filter(|r| r.enabled)
    }

    /// 阶段 6a-ii：把斜杠命令消息塞进 debounce
    #[allow(clippy::too_many_arguments)] // 参数来自上游 handler 的独立数据源，合并为 struct 增加认知负担
    fn push_slash_command_message(
        debounce: &Arc<MessageDebounce>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        todo: &crate::models::Todo,
        body: &str,
        params: std::collections::HashMap<String, String>,
        workspace_id: Option<i64>,
    ) {
        debounce.push(PendingMessage {
            bot_id,
            chat_id: msg.channel.clone(),
            chat_type: chat_type.to_string(),
            sender: msg.sender.clone(),
            content: body.to_string(),
            todo_id: todo.id,
            todo_prompt: todo.prompt.clone(),
            executor: todo.executor.clone(),
            trigger_type: "slash_command".to_string(),
            params: Some(params),
            message_id: Some(msg.id.clone()),
            resume_session_id: None,
            resume_message: None,
            binding_id: None,
            workspace_id,
            immediate: false,
        });
    }

    /// 阶段 6a-iii：把斜杠命令触发环路的消息塞进 debounce
    fn push_slash_command_loop_message(
        debounce: &Arc<MessageDebounce>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        loop_id: i64,
        body: &str,
        workspace_id: Option<i64>,
    ) {
        debounce.push(PendingMessage {
            bot_id,
            chat_id: msg.channel.clone(),
            chat_type: chat_type.to_string(),
            sender: msg.sender.clone(),
            content: body.to_string(),
            todo_id: loop_id, // 复用 todo_id 字段存储 loop_id
            todo_prompt: String::new(), // 环路不使用 todo_prompt
            executor: None,
            trigger_type: "slash_command_loop".to_string(),
            params: None,
            message_id: Some(msg.id.clone()),
            resume_session_id: None,
            resume_message: None,
            binding_id: None,
            immediate: false,
            workspace_id,
        });
    }

    /// 阶段 6b：默认回复——根据工作空间配置的响应类型分发
    /// todo 拉取失败时降级用空 prompt，避免整条消息被吞掉
    async fn dispatch_default_response(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) {
        tracing::debug!(
            "[feishu:{}] dispatch_default_response: content={:?}, chat_type={}",
            context.bot_id, prep.content, prep.chat_type
        );
        // 从数据库获取 bot 的 workspace_id，然后查询 workspace 设置
        let workspace_id = match context.db.get_agent_bot_workspace_id(context.bot_id).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::warn!("bot {} has no workspace_id, skipping default response", context.bot_id);
                return;
            }
            Err(e) => {
                tracing::error!("failed to get workspace_id for bot {}: {}", context.bot_id, e);
                return;
            }
        };

        // 读取工作空间的完整默认响应配置
        let settings = crate::db::workspace_setting::get_workspace_settings(context.db, workspace_id)
            .await
            .ok()
            .flatten();

        let Some(settings) = settings else {
            tracing::info!(
                "[feishu:{}] no workspace settings found for workspace {}, skipping default response",
                context.bot_id, workspace_id
            );
            return;
        };
        // 空消息不触发任何响应
        if prep.content.is_empty() {
            return;
        }

        // 根据 default_response_type 分发到不同的处理路径
        match settings.default_response_type.as_str() {
            "executor" => {
                // 执行器类型：直接调用执行器交互（不存储执行记录）
                let executor = settings.default_response_executor.clone()
                    .unwrap_or_else(|| "claudecode".to_string());
                Self::debounce_push_executor_default(
                    context.debounce,
                    context.bot_id,
                    msg,
                    prep.chat_type,
                    &executor,
                    prep.content,
                    Some(workspace_id),
                    prep.is_mention,
                );
            }
            "loop" => {
                // 环路类型：触发环路执行
                let Some(loop_id) = settings.default_response_loop_id else { return };
                Self::debounce_push_loop_default(
                    context.debounce,
                    context.bot_id,
                    msg,
                    prep.chat_type,
                    loop_id,
                    prep.content,
                    Some(workspace_id),
                    prep.is_mention,
                );
            }
            _ => {
                // todo 类型（默认值）：通过 todo 执行
                let Some(todo_id) = settings.default_response_todo_id else { return };
                let todo_prompt = context.db.get_todo(todo_id).await
                    .ok().flatten().map(|t| t.prompt).unwrap_or_default();
                let (_, params) = build_trigger_params(prep.content);
                Self::debounce_push_default(
                    context.debounce, context.bot_id, msg, prep.chat_type,
                    todo_id, todo_prompt, prep.content, params, Some(workspace_id),
                    prep.is_mention,
                );
            }
        }
    }

    /// 阶段 6b-i：把默认回复消息塞进 debounce
    #[allow(clippy::too_many_arguments)] // 参数来自上游 handler 的独立数据源，合并为 struct 增加认知负担
    fn debounce_push_default(
        debounce: &Arc<MessageDebounce>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        todo_id: i64,
        todo_prompt: String,
        content: &str,
        params: std::collections::HashMap<String, String>,
        workspace_id: Option<i64>,
        immediate: bool,
    ) {
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
            immediate,
            binding_id: None,
            workspace_id,
        });
    }

    /// 阶段 6b-ii：把默认响应为 executor 类型的消息塞进 debounce
    #[allow(clippy::too_many_arguments)]
    fn debounce_push_executor_default(
        debounce: &Arc<MessageDebounce>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        executor: &str,
        content: &str,
        workspace_id: Option<i64>,
        immediate: bool,
    ) {
        debounce.push(PendingMessage {
            bot_id,
            chat_id: msg.channel.clone(),
            chat_type: chat_type.to_string(),
            sender: msg.sender.clone(),
            content: content.to_string(),
            todo_id: 0, // executor 类型不使用 todo_id
            todo_prompt: String::new(),
            executor: Some(executor.to_string()),
            trigger_type: "default_response_executor".to_string(),
            params: None,
            message_id: Some(msg.id.clone()),
            resume_session_id: None,
            immediate,
            resume_message: None,
            binding_id: None,
            workspace_id,
        });
    }

    /// 阶段 6b-iii：把默认响应为 loop 类型的消息塞进 debounce
    #[allow(clippy::too_many_arguments)]
    fn debounce_push_loop_default(
        debounce: &Arc<MessageDebounce>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        loop_id: i64,
        content: &str,
        workspace_id: Option<i64>,
        immediate: bool,
    ) {
        debounce.push(PendingMessage {
            bot_id,
            chat_id: msg.channel.clone(),
            chat_type: chat_type.to_string(),
            sender: msg.sender.clone(),
            content: content.to_string(),
            todo_id: loop_id, // 复用 todo_id 字段存储 loop_id
            todo_prompt: String::new(), // 环路不使用 todo_prompt
            executor: None,
            trigger_type: "default_response_loop".to_string(),
            params: None,
            message_id: Some(msg.id.clone()),
            immediate,
            resume_session_id: None,
            resume_message: None,
            binding_id: None,
            workspace_id,
        });
    }

    /// 阶段 7：调试回显日志（仅在 bot_config.echo_reply 开启时记录）
    /// 纯 tracing! 调用、无 IO，保持 fn 而非 async fn，避免编排器里 .await 噪音
    pub fn log_echo_reply(
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        bot_config: &BotConfig,
    ) {
        if !bot_config.echo_reply {
            return;
        }
        if chat_type == "p2p" {
            tracing::info!(
                "[feishu:{}] 收到私聊消息: sender={}, content={}",
                bot_id, msg.sender, msg.content
            );
        } else if chat_type == "group" {
            tracing::info!(
                "[feishu:{}] 收到群聊消息: channel={}, sender={}, content={}",
                bot_id, msg.channel, msg.sender, msg.content
            );
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
    pub(crate) fn parse_slash_command(content: &str) -> Option<SlashCommandMatch<'_>> {
        let trimmed = content.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let command = parts.next()?.trim();
        let body = parts.next().unwrap_or("").trim();
        Some(SlashCommandMatch { command, body })
    }

    /// /sethome：推送目标已改为自动捕获所有者 open_id（扫码创建/首次私聊），
    /// 故本命令退化为只读查询——回显当前推送目标是谁，不再写任何 push target 字段。
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
        let text = Self::owner_push_status_text(db, bot_id, sender).await;
        // 回信目标：私聊回个人、群聊回群，仅用于把查询结果展示给当前说话人
        let (reply_id, reply_type) = if chat_type == "group" {
            (channel.to_string(), "chat_id")
        } else {
            (sender.to_string(), "open_id")
        };
        Self::send_text(credentials, token_manager, bot_id, &reply_id, reply_type, &text).await;
        if let Some(rid) = reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// 构造推送目标状态文案：已设置则回显所有者（脱敏），未设置则提示去私聊触发自动捕获。
    async fn owner_push_status_text(db: &Arc<Database>, bot_id: i64, sender: &str) -> String {
        match db.get_owner_open_id(bot_id).await {
            Ok(Some(owner)) => format!(
                "ℹ️ 当前推送目标：机器人所有者私聊 {}\n推送目标在首次私聊时自动设置，无需手动指定。\n如需调整推送级别，发送 /feishupush",
                Self::mask_open_id(&owner)
            ),
            Ok(None) => format!(
                "ℹ️ 尚未设置推送目标。请与机器人在私聊发一条消息，系统会自动捕获为推送目标。\n当前发送者：{}",
                Self::mask_open_id(sender)
            ),
            Err(e) => format!("查询推送目标失败：{e}"),
        }
    }

    /// 脱敏 open_id：保留 ou_ 前缀与末 4 位，中间省略，用于卡片/日志展示，避免泄露完整 ID。
    fn mask_open_id(open_id: &str) -> String {
        // open_id 形如 ou_xxxxxxxx，全 ASCII，按字节切片安全
        if open_id.len() <= 8 {
            return "***".to_string();
        }
        format!("{}...{}", &open_id[..4], &open_id[open_id.len() - 4..])
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

    /// Handle /new — start a fresh session without resuming the previous one.
    /// 全局内置斜杠命令，用于清空当前会话的 session，开启全新对话。
    ///
    /// 支持两种场景：
    /// 1. 项目绑定场景：清除绑定的 todo/loop 会话
    /// 2. 私聊默认响应执行器场景：清除默认执行器的会话
    async fn handle_new(context: FeishuCommandContext<'_>) {
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

        // 先尝试项目绑定场景
        match db.get_feishu_project_binding(bot_id, channel).await {
            Ok(Some(binding)) => {
                // 清除 session_id 和 latest_record_id，使下一条消息无法 resume
                // should_resume 的判断依赖 latest_record.session_id.is_some()，
                // 清除后 latest_record_id=None → latest_record=None → should_resume=false
                if let Err(e) = db.clear_feishu_binding_session(binding.id).await {
                    tracing::error!("[feishu:{}] /new clear session failed: {e}", bot_id);
                    Self::send_text(
                        credentials,
                        token_manager,
                        bot_id,
                        &receive_id,
                        receive_id_type,
                        "⚠️ 清除会话失败，请稍后重试。",
                    )
                    .await;
                    if let Some(rid) = reaction_id {
                        Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                    }
                    return;
                }

                tracing::info!(
                    "[feishu:{}] /new command: cleared session for binding {}, next message will start fresh",
                    bot_id,
                    binding.id
                );
                Self::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "🆕 已开启新会话。\n\n发送你的任务，我将使用全新 session 执行，不再resume之前的对话。",
                )
                .await;

                if let Some(rid) = reaction_id {
                    Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
            Ok(None) => {
                // 没有绑定项目，尝试私聊默认响应执行器场景
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /new query binding failed: {e}", bot_id);
                Self::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 查询绑定失败，请稍后重试。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
        }

        // 私聊默认响应执行器场景：获取 workspace 和默认执行器配置
        let workspace_id = match db.get_agent_bot_workspace_id(bot_id).await {
            Ok(Some(wid)) => wid,
            Ok(None) => {
                tracing::warn!("[feishu:{}] /new: bot has no workspace", bot_id);
                Self::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 未找到工作空间，无法使用 /new。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /new query workspace failed: {e}", bot_id);
                Self::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 查询工作空间失败，请稍后重试。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
        };

        // 获取 workspace 设置，判断默认响应类型
        let settings = match crate::db::workspace_setting::get_workspace_settings(db, workspace_id).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                Self::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "📭 当前未配置默认响应，无法使用 /new。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /new query workspace settings failed: {e}", bot_id);
                Self::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 查询工作空间设置失败，请稍后重试。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
        };

        // 只处理 executor 类型的默认响应
        if settings.default_response_type != "executor" {
            Self::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                "📭 当前默认响应类型不是执行器，无法使用 /new 清空会话。",
            )
            .await;
            if let Some(rid) = reaction_id {
                Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
            }
            return;
        }

        let executor_name = settings.default_response_executor
            .unwrap_or_else(|| "claudecode".to_string());

        // 清空执行器 session：设置为 None
        if let Err(e) = db.set_executor_session(workspace_id, &executor_name, None).await {
            tracing::error!("[feishu:{}] /new clear executor session failed: {e}", bot_id);
            Self::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                "⚠️ 清除执行器会话失败，请稍后重试。",
            )
            .await;
            if let Some(rid) = reaction_id {
                Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
            }
            return;
        }

        tracing::info!(
            "[feishu:{}] /new command: cleared executor session for {}, workspace={}",
            bot_id,
            executor_name,
            workspace_id
        );
        Self::send_text(
            credentials,
            token_manager,
            bot_id,
            &receive_id,
            receive_id_type,
            &format!("🆕 已开启新会话。\n\n下次对话将使用全新的 {} 会话，不再接续之前的对话。", executor_name),
        )
        .await;

        if let Some(rid) = reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /stop — stop the currently running execution for this binding.
    /// 与前端「停止」按钮逻辑相同：通过 task_manager 取消任务。
    async fn handle_stop(
        task_manager: &Arc<TaskManager>,
        context: FeishuCommandContext<'_>,
    ) {
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
                // 获取当前 binding 的最新执行记录
                if let Some(record_id) = binding.latest_record_id {
                    match db.get_execution_record(record_id).await {
                        Ok(Some(record)) => {
                            if record.status == crate::models::ExecutionStatus::Running {
                                // 任务正在运行，尝试停止
                                if let Some(ref task_id) = record.task_id {
                                    let cancelled = task_manager.cancel(task_id).await;
                                    if cancelled {
                                        tracing::info!(
                                            "[feishu:{}] /stop: cancelled task {} for record {}",
                                            bot_id,
                                            task_id,
                                            record_id
                                        );
                                        Self::send_text(
                                            credentials,
                                            token_manager,
                                            bot_id,
                                            &receive_id,
                                            receive_id_type,
                                            "⏹️ 已发送停止信号，任务即将终止。",
                                        )
                                        .await;
                                    } else {
                                        // 任务不在 task_manager 中（可能已崩溃），强制更新 DB
                                        tracing::warn!(
                                            "[feishu:{}] /stop: task {} not in task_manager, forcing DB update",
                                            bot_id,
                                            task_id
                                        );
                                        let _ = db.force_fail_execution_record(record_id).await;
                                        Self::send_text(
                                            credentials,
                                            token_manager,
                                            bot_id,
                                            &receive_id,
                                            receive_id_type,
                                            "⚠️ 任务已不在运行中（可能已异常退出），已更新状态。",
                                        )
                                        .await;
                                    }
                                } else {
                                    Self::send_text(
                                        credentials,
                                        token_manager,
                                        bot_id,
                                        &receive_id,
                                        receive_id_type,
                                        "⚠️ 该执行记录没有 task_id，无法停止。",
                                    )
                                    .await;
                                }
                            } else {
                                Self::send_text(
                                    credentials,
                                    token_manager,
                                    bot_id,
                                    &receive_id,
                                    receive_id_type,
                                    "ℹ️ 当前没有正在执行的任务。",
                                )
                                .await;
                            }
                        }
                        Ok(None) => {
                            Self::send_text(
                                credentials,
                                token_manager,
                                bot_id,
                                &receive_id,
                                receive_id_type,
                                "⚠️ 执行记录不存在。",
                            )
                            .await;
                        }
                        Err(e) => {
                            tracing::error!("[feishu:{}] /stop query record failed: {e}", bot_id);
                            Self::send_text(
                                credentials,
                                token_manager,
                                bot_id,
                                &receive_id,
                                receive_id_type,
                                "⚠️ 查询执行记录失败，请稍后重试。",
                            )
                            .await;
                        }
                    }
                } else {
                    Self::send_text(
                        credentials,
                        token_manager,
                        bot_id,
                        &receive_id,
                        receive_id_type,
                        "ℹ️ 当前没有执行记录可停止。",
                    )
                    .await;
                }
            }
            Ok(None) => {
                Self::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "📭 当前聊天未绑定任何项目，无可停止的任务。",
                )
                .await;
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /stop query binding failed: {e}", bot_id);
                Self::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 查询绑定失败，请稍后重试。",
                )
                .await;
            }
        }

        if let Some(rid) = reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /help — show interactive help card with tabbed navigation.
    /// 点击 Tab 按钮会触发 card.action.trigger 事件，由飞书平台回调处理。
    async fn handle_help(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            ctx,
            bot_id,
            chat_type: _,
            sender,
            message_id,
            content,
            reaction_id,
            ..
        } = context;

        // 解析当前分组，默认 "status"（状态页是控制台首页）
        let parsed = content.strip_prefix("/help ").unwrap_or("").trim().to_lowercase();
        let group = if parsed.is_empty() { "status".to_string() } else { parsed };

        // 状态感知控制台：查当前绑定/运行状态/推送级别/最近任务后渲染
        let state = Self::assemble_help_card_state(ctx, db, bot_id, &group, 1).await;
        let card = build_help_console_card(&state);
        let card_json = render_card(&card, &format!("feishu:{}", sender));

        // 发送卡片（reply API），失败降级纯文本
        if let Err(e) = Self::reply_card(credentials, token_manager, bot_id, message_id, &card_json).await {
            tracing::error!("[feishu:{}] /help send card failed: {}", bot_id, e);
            Self::send_text(credentials, token_manager, bot_id, sender, "open_id", "📋 NTD 控制台\n\n发送 /help 打开任务控制台。").await;
        }

        if let Some(rid) = reaction_id {
            Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// 处理飞书卡片按钮点击回调。
    /// card_callback 消息的 content 是 action value，按前缀分三种处理：
    /// - `nav:/help <group>`：原地 patch 原卡片，切到对应分组；
    /// - `cmd:/<command>`：转成命令文本，复用 handle_message 分发链路执行，
    ///   等价于点击者在会话里发送了 `/<command>`；
    /// - `act:/<action>`：执行动作 + patch 刷新控制台。
    async fn handle_card_callback(context: ListenerMessageContext<'_>, msg: &ChannelMessage) {
        let action = msg.content.trim();

        // nav: 前缀 - 原地 patch 刷新控制台/历史页，拦截后直接返回。
        if Self::handle_nav_action(&context, msg, action).await {
            return;
        }

        // cmd: 前缀 - 把卡片点击转成命令执行。
        // 构造一条虚拟命令消息复用 handle_message 的完整分发链路（内置命令 try_route_builtin_command
        // + 自定义规则 route_slash_or_default_response），与用户在会话里手动发送该命令效果一致。
        if let Some(cmd_text) = Self::parse_card_command(action) {
            tracing::info!(
                "[feishu:{}] card cmd → redispatch as message: {:?}",
                context.bot_id, cmd_text
            );
            // chat_type 改成 p2p：避免 handle_message 又把这条消息当作 card_callback 递归处理；
            // sender/channel/id 沿用卡片回调，让命令处理函数的回复落到原会话、指向点击者。
            let mut cmd_msg = msg.clone();
            cmd_msg.content = cmd_text;
            cmd_msg.chat_type = Some("p2p".to_string());
            // handle_message → handle_card_callback → handle_message 是静态递归，
            // async fn 递归必须 Box::pin 引入间接层，否则 future 大小无限无法编译。
            // 运行时 cmd_msg.chat_type 已是 p2p，不会再进 card_callback 分支，实际只递归一层。
            Box::pin(Self::handle_message(context, &cmd_msg)).await;
            return;
        }

        // act: 前缀 - 执行动作（新会话/停止/设推送/切绑定/解绑）+ patch 刷新控制台。
        // 解析失败（未知 verb）落到下面的 unknown warn。
        if let Some(parsed) = Self::parse_card_action(action) {
            Self::execute_card_action(context, msg, parsed).await;
            return;
        }

        tracing::warn!("[feishu:{}] unknown card action: {}", context.bot_id, action);
    }

    /// 处理 nav: 前缀的卡片动作，返回 true 表示已处理。
    /// nav 动作是只读 patch 刷新，不产生副作用。
    async fn handle_nav_action(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, action: &str) -> bool {
        // nav:/help <group> 重查最新状态后刷控制台（运行状态可能已变）。
        if let Some(group) = action.strip_prefix("nav:/help ") {
            let group_key = group.trim().to_lowercase();
            let state = Self::assemble_help_card_state(context.ctx, context.db, context.bot_id, &group_key, 1).await;
            let card = build_help_console_card(&state);
            Self::patch_rendered_card(context, msg, &card).await;
            return true;
        }
        // nav:/history [page] - 分页查看执行历史。
        if let Some(page_arg) = action.strip_prefix("nav:/history") {
            let page = page_arg.trim().parse::<usize>().unwrap_or(1).max(1);
            Self::patch_history_page(context, msg, page).await;
            return true;
        }
        // nav:/todos <page> - 事项分页（每页 10）。
        if let Some(page_arg) = action.strip_prefix("nav:/todos") {
            let page = page_arg.trim().parse::<usize>().unwrap_or(1).max(1);
            let state = Self::assemble_help_card_state(context.ctx, context.db, context.bot_id, "todo", page).await;
            let card = build_help_console_card(&state);
            Self::patch_rendered_card(context, msg, &card).await;
            return true;
        }
        // nav:/loops <page> - 环路分页（每页 10）。
        if let Some(page_arg) = action.strip_prefix("nav:/loops") {
            let page = page_arg.trim().parse::<usize>().unwrap_or(1).max(1);
            let state = Self::assemble_help_card_state(context.ctx, context.db, context.bot_id, "loop", page).await;
            let card = build_help_console_card(&state);
            Self::patch_rendered_card(context, msg, &card).await;
            return true;
        }
        false
    }

    /// 执行卡片 act 动作，执行后 patch 刷新控制台。
    async fn execute_card_action(context: ListenerMessageContext<'_>, msg: &ChannelMessage, action: CardAction) {
        let outcome = Self::run_card_action(&context, msg, &action).await;
        let group = Self::action_target_group(&action);
        Self::patch_after_action(&context, msg, group, &outcome).await;
    }

    /// 按 CardAction 分发到具体执行函数。
    async fn run_card_action(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        action: &CardAction,
    ) -> ActionOutcome {
        match action {
            CardAction::Push(level) => Self::act_push(context, level).await,
            CardAction::New => Self::act_new(context).await,
            CardAction::Stop => Self::act_stop(context).await,
            CardAction::SetHome => Self::act_sethome(context, msg).await,
            CardAction::Bind(workspace_id) => Self::act_bind(context, *workspace_id).await,
            CardAction::RunTodo(todo_id) => Self::act_run_todo(context, msg, *todo_id).await,
            CardAction::RunLoop(loop_id) => Self::act_run_loop(context, msg, *loop_id).await,
            CardAction::SetExecutor(name) => Self::act_set_executor(context, name).await,
        }
    }

    /// act 动作执行后刷新到的目标 Tab。
    fn action_target_group(action: &CardAction) -> &'static str {
        match action {
            CardAction::Bind(_) | CardAction::Push(_) | CardAction::SetExecutor(_) => "workspace",
            CardAction::RunTodo(_) => "todo",
            CardAction::RunLoop(_) => "loop",
            _ => "status",
        }
    }

    /// bot 所属 workspace 的默认执行器（如 dev 的 pi）。
    async fn workspace_default_executor(db: &Database, bot_id: i64) -> Option<String> {
        let wid = db.get_agent_bot_workspace_id(bot_id).await.ok().flatten()?;
        let settings = crate::db::workspace_setting::get_workspace_settings(db, wid).await.ok().flatten()?;
        settings.default_response_executor
    }

    /// auto-seed：确保 workspace 的 default_response_type=executor。
    /// 移除 binding 路径后 chat 全走 default_response，只 executor 分支可靠回复；切换 workspace 时兜底。
    async fn ensure_default_response_executor(db: &Database, workspace_id: i64) {
        let existing = crate::db::workspace_setting::get_workspace_settings(db, workspace_id).await.ok().flatten();
        let need_seed = existing.as_ref().map(|s| s.default_response_type != "executor").unwrap_or(true);
        if need_seed {
            // executor 用该 workspace 已配的（若有），否则 None（dispatch 时兜底 claudecode）
            let executor = existing.and_then(|s| s.default_response_executor);
            let _ = crate::db::workspace_setting::upsert_workspace_settings(
                db,
                workspace_id,
                Some("executor".to_string()),
                None,
                None,
                executor,
            )
            .await;
        }
    }

    /// 设置推送级别（直接设值，不走 /feishupush 循环）。
    async fn act_push(context: &ListenerMessageContext<'_>, level: &str) -> ActionOutcome {
        match context.db.update_feishu_push_level(context.bot_id, level).await {
            Ok(_) => ActionOutcome { success: true, message: format!("推送级别已更新为 {level}") },
            Err(e) => ActionOutcome { success: false, message: format!("设置失败：{e}") },
        }
    }

    /// 开启新会话：清当前 workspace 默认执行器的 session。
    async fn act_new(context: &ListenerMessageContext<'_>) -> ActionOutcome {
        let Some(wid) = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten() else {
            return ActionOutcome { success: false, message: "未设置工作空间".to_string() };
        };
        let executor = Self::workspace_default_executor(context.db, context.bot_id)
            .await
            .unwrap_or_else(|| "claudecode".to_string());
        match context.db.set_executor_session(wid, &executor, None).await {
            Ok(_) => ActionOutcome { success: true, message: "已开启新会话".to_string() },
            Err(e) => ActionOutcome { success: false, message: format!("失败：{e}") },
        }
    }

    /// 停止当前 workspace 的运行任务（by workspace + ExecutionStatus::Running 直接查）。
    async fn act_stop(context: &ListenerMessageContext<'_>) -> ActionOutcome {
        let Some(wid) = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten() else {
            return ActionOutcome { success: false, message: "未设置工作空间".to_string() };
        };
        // 直接按 workspace 查运行中的记录，不依赖最近 N 条（避免旧记录淹没了 running 记录）
        let Ok(records) = context.db.get_running_records_by_workspace(wid).await else {
            return ActionOutcome { success: false, message: "查询失败".to_string() };
        };
        let Some(running) = records.into_iter().next() else {
            return ActionOutcome { success: false, message: "没有运行中的任务".to_string() };
        };
        let Some(task_id) = running.task_id.as_deref() else {
            return ActionOutcome { success: false, message: "任务缺少 task_id".to_string() };
        };
        if context.task_manager.cancel(task_id).await {
            ActionOutcome { success: true, message: "已发送停止信号，任务即将终止".to_string() }
        } else {
            let _ = context.db.force_fail_execution_record(running.id).await;
            ActionOutcome { success: true, message: "任务未在运行，已强制标记结束".to_string() }
        }
    }

    /// act:/sethome：推送目标已自动捕获所有者 open_id，按钮退化为只读查询当前推送目标。
    async fn act_sethome(context: &ListenerMessageContext<'_>, _msg: &ChannelMessage) -> ActionOutcome {
        // 不再写 push target：所有者由扫码/首次私聊自动捕获。这里只回显，便于用户确认。
        let message = match context.db.get_owner_open_id(context.bot_id).await {
            Ok(Some(owner)) => format!("当前推送目标：所有者私聊 {}", Self::mask_open_id(&owner)),
            Ok(None) => "尚未设置推送目标，请与机器人私聊一次以自动捕获".to_string(),
            Err(e) => format!("查询失败：{e}"),
        };
        ActionOutcome { success: true, message }
    }

    /// 切换工作空间：级联清旧 binding + 改 agent_bot.workspace_id + auto-seed default_response。
    async fn act_bind(context: &ListenerMessageContext<'_>, workspace_id: i64) -> ActionOutcome {
        let bot_id = context.bot_id;
        // 级联（对齐 move_bot_to_workspace）：删 pending binding / disable 旧 binding
        if let Ok(bindings) = context.db.get_feishu_project_bindings(bot_id).await {
            for b in bindings {
                if b.chat_id == crate::models::PENDING_CHAT_ID {
                    let _ = context.db.delete_feishu_project_binding(b.id).await;
                } else {
                    let _ = context.db.update_feishu_project_binding_enabled(b.id, false).await;
                }
            }
        }
        if let Err(e) = context.db.update_agent_bot_workspace_id(bot_id, workspace_id).await {
            return ActionOutcome { success: false, message: format!("切换工作空间失败：{e}") };
        }
        // auto-seed default_response_type=executor，确保切完后 chat 消息有回复
        Self::ensure_default_response_executor(context.db, workspace_id).await;
        let name = context
            .db
            .get_workspace_name_by_id(workspace_id)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| format!("#{workspace_id}"));
        ActionOutcome { success: true, message: format!("已切换到工作空间「{name}」") }
    }

    /// 触发事项：后台跑该 todo，结果通过 ExecEvent::Finished → FeishuPushService 推回当前 chat。
    async fn act_run_todo(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, todo_id: i64) -> ActionOutcome {
        use crate::executor_service::{run_todo_execution, RunTodoExecutionRequest};
        let (receive_id, receive_id_type) = Self::resolve_receive_target(msg);
        let workspace_id = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten();
        let todo = match context.db.get_todo(todo_id).await {
            Ok(Some(t)) => t,
            _ => return ActionOutcome { success: false, message: format!("事项 #{todo_id} 不存在") },
        };
        // 校验 todo 的 workspace_id 与 bot 当前 workspace 一致，防止旧卡片跨 workspace 执行
        if todo.workspace_id != workspace_id {
            return ActionOutcome {
                success: false,
                message: format!("事项 #{todo_id} 不属于当前工作空间，无法执行"),
            };
        }
        let title = todo.title.clone();
        let req = RunTodoExecutionRequest {
            db: context.db.clone(),
            executor_registry: context.ctx.executor_registry.clone(),
            tx: context.ctx.tx.clone(),
            task_manager: context.task_manager.clone(),
            config: context.ctx.config.clone(),
            todo_id,
            message: todo.prompt,
            req_executor: todo.executor,
            trigger_type: "feishu_card".to_string(),
            params: None,
            resume_session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,
            step_id: None,
            feishu_bot_id: Some(context.bot_id),
            feishu_receive_id: Some(receive_id.to_string()),
            feishu_receive_id_type: Some(receive_id_type.to_string()),
            workspace_path: None,
            workspace_id,
        };
        // fire-and-forget：后台执行不阻塞卡片 patch；结果由推送通道发回 chat
        tokio::spawn(async move {
            let _ = run_todo_execution(req).await;
        });
        ActionOutcome { success: true, message: format!("已触发事项「{title}」") }
    }

    /// 触发环路：LoopRunner::spawn_run 后台执行，整环结束推回 chat。
    async fn act_run_loop(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, loop_id: i64) -> ActionOutcome {
        let Some(runner) = context.debounce.loop_runner() else {
            return ActionOutcome { success: false, message: "环路执行器未就绪".to_string() };
        };
        let (receive_id, receive_id_type) = Self::resolve_receive_target(msg);
        let loop_ = match context.db.get_loop(loop_id).await {
            Ok(Some(l)) => l,
            _ => return ActionOutcome { success: false, message: format!("环路 #{loop_id} 不存在") },
        };
        // 校验 loop 的 workspace_id 与 bot 当前 workspace 一致，防止旧卡片跨 workspace 执行
        let workspace_id = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten();
        if loop_.workspace_id != workspace_id {
            return ActionOutcome {
                success: false,
                message: format!("环路 #{loop_id} 不属于当前工作空间，无法执行"),
            };
        }
        let name = loop_.name;
        runner.clone().spawn_run(
            loop_id,
            None,
            "feishu_card",
            serde_json::json!({}),
            Some(context.bot_id),
            Some(receive_id.to_string()),
            Some(receive_id_type.to_string()),
        );
        ActionOutcome { success: true, message: format!("已触发环路「{name}」") }
    }

    /// 设当前 workspace 的默认执行器：写 workspace_settings.default_response_executor。
    /// executor 名必须是已注册的（ExecutorType::as_str），否则视为无效拒绝写入。
    async fn act_set_executor(context: &ListenerMessageContext<'_>, executor_name: &str) -> ActionOutcome {
        let Some(wid) = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten() else {
            return ActionOutcome { success: false, message: "未设置工作空间".to_string() };
        };
        // 校验 executor 已注册，避免把无效名写进 settings 让下次 dispatch 失败
        let registered: Vec<String> = context
            .ctx
            .executor_registry
            .list_executors()
            .await
            .into_iter()
            .map(|t| t.as_str().to_string())
            .collect();
        if !registered.iter().any(|s| s == executor_name) {
            return ActionOutcome {
                success: false,
                message: format!("执行器 {executor_name} 未注册（可用：{}）", registered.join(", ")),
            };
        }
        match crate::db::workspace_setting::upsert_workspace_settings(
            context.db,
            wid,
            None,
            None,
            None,
            Some(executor_name.to_string()),
        )
        .await
        {
            Ok(_) => ActionOutcome { success: true, message: format!("默认执行器已设为 {executor_name}") },
            Err(e) => ActionOutcome { success: false, message: format!("设置失败：{e}") },
        }
    }

    /// act 执行后 patch 刷新控制台：assemble 最新状态 + 顶部插入操作结果提示。
    async fn patch_after_action(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        group: &str,
        outcome: &ActionOutcome,
    ) {
        let state = Self::assemble_help_card_state(context.ctx, context.db, context.bot_id, group, 1).await;
        let mut card = build_help_console_card(&state);
        let icon = if outcome.success { "✅" } else { "⚠️" };
        let tip = CardElement::Markdown(CardMarkdown { content: format!("{icon} {}", outcome.message) });
        card.elements.insert(0, tip);
        Self::patch_rendered_card(context, msg, &card).await;
    }

    /// 渲染卡片并 patch 到原消息（nav/act 刷新共用）。
    async fn patch_rendered_card(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, card: &Card) {
        let session_key = format!("feishu:{}", msg.sender);
        let card_json = render_card(card, &session_key);
        if let Err(e) = Self::patch_card(context.credentials, context.token_manager, context.bot_id, &msg.id, &card_json).await {
            tracing::warn!("[feishu:{}] patch card failed: {e}", context.bot_id);
        }
    }

    /// 历史子页：按当前 workspace 分页查执行记录后 patch。
    async fn patch_history_page(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, page: usize) {
        const PER_PAGE: i64 = 10;
        let offset = page.saturating_sub(1) as i64 * PER_PAGE;
        let (items, total) = Self::query_history(context.db, context.bot_id, PER_PAGE, offset).await;
        let total_pages = (total.max(0) as usize).div_ceil(PER_PAGE as usize);
        let card = build_history_card(&items, page, total_pages.max(1));
        Self::patch_rendered_card(context, msg, &card).await;
    }

    /// 按 bot 的 workspace 分页查执行记录 → HistoryItem + 总数。
    async fn query_history(db: &Database, bot_id: i64, limit: i64, offset: i64) -> (Vec<HistoryItem>, i64) {
        let Some(wid) = db.get_agent_bot_workspace_id(bot_id).await.ok().flatten() else {
            return (vec![], 0);
        };
        match db.get_execution_records_by_workspace(wid, limit, offset).await {
            Ok((records, total)) => (records.into_iter().map(Self::record_to_history_item).collect(), total),
            Err(_) => (vec![], 0),
        }
    }

    /// ExecutionRecord → 历史子页列表项（状态 emoji + 标题 + 触发类型 + 时间）。
    fn record_to_history_item(r: crate::models::ExecutionRecord) -> HistoryItem {
        use crate::models::ExecutionStatus;
        let status_icon = match r.status {
            ExecutionStatus::Success => "✅",
            ExecutionStatus::Running => "⏳",
            ExecutionStatus::Failed => "❌",
        };
        HistoryItem {
            status_icon: status_icon.to_string(),
            title: r.source_todo_title.clone().unwrap_or_else(|| r.command.clone()),
            trigger: r.trigger_type,
            time_desc: format_record_time(&r.started_at),
        }
    }

    /// 解析卡片回调 action 里的命令文本，供 handle_card_callback 的 cmd: 分支使用。
    /// `cmd:/new` → `Some("/new")`；`cmd:/bind foo` → `Some("/bind foo")`（保留参数）；
    /// 非 `cmd:/` 前缀（nav:/act:/未知/空）→ None。
    /// 抽成纯函数便于单测命令文本拼装，也让 handle_card_callback 的 cmd: 分支保持简洁。
    fn parse_card_command(action: &str) -> Option<String> {
        action.strip_prefix("cmd:/").map(|cmd| format!("/{}", cmd))
    }

    /// 解析卡片 act:/ 动作字符串为 CardAction。
    /// "act:/stop"→Stop；"act:/bind myapp"→Bind("myapp")；"act:/push result_only"→Push("result_only")。
    /// bind/push 需要参数，缺参数返回 None；未知 verb 返回 None。纯函数便于单测。
    fn parse_card_action(action: &str) -> Option<CardAction> {
        let rest = action.strip_prefix("act:/")?;
        // splitn(2) 让参数部分可含空格（虽然当前不会，但留余地），verb 与 arg 用首个空白分隔。
        let mut parts = rest.splitn(2, char::is_whitespace);
        let verb = parts.next()?.trim();
        let arg = parts.next().map(|s| s.trim()).filter(|s| !s.is_empty());
        Some(match verb {
            "stop" => CardAction::Stop,
            "new" => CardAction::New,
            "sethome" => CardAction::SetHome,
            "push" => CardAction::Push(arg?.to_string()),
            "setexecutor" => CardAction::SetExecutor(arg?.to_string()),
            "bind" => CardAction::Bind(arg?.parse().ok()?),
            "runtodo" => CardAction::RunTodo(arg?.parse().ok()?),
            "runloop" => CardAction::RunLoop(arg?.parse().ok()?),
            _ => return None,
        })
    }

    /// 把卡片回调消息解析成回信接收者 (receive_id, receive_id_type)。
    /// card_callback 的 chat_type 不是 p2p/group：msg.channel(chat_id)非空 → 群聊用 chat_id；
    /// 否则回退到点击者 open_id（私聊）。供 act:/runtodo、act:/runloop 等「回信给点击者」的动作复用。
    /// 注意：推送目标已改用 owner_open_id，sethome 不再调用本函数。
    fn resolve_receive_target(msg: &ChannelMessage) -> (&str, &str) {
        if !msg.channel.is_empty() {
            (msg.channel.as_str(), "chat_id")
        } else {
            (msg.sender.as_str(), "open_id")
        }
    }

    /// 组装 /help 卡片状态：按 agent_bot.workspace_id 查该 workspace 的摘要/事项/环路/最近任务 + 所有工作空间。
    /// handle_help、nav 切页、act 执行后刷新都复用它（只读 db，运行状态取最近记录里的 running）。
    /// ctx 用于查询已注册的执行器列表（工作空间页渲染按钮排用）。
    async fn assemble_help_card_state(
        ctx: &ServiceContext,
        db: &Database,
        bot_id: i64,
        current_group: &str,
        page: usize,
    ) -> HelpCardState {
        let wid = db.get_agent_bot_workspace_id(bot_id).await.ok().flatten();
        let workspace = match wid {
            Some(id) => Self::build_workspace_summary(db, id).await,
            None => None,
        };
        let push_level = db
            .get_feishu_push_target(bot_id)
            .await
            .ok()
            .flatten()
            .map(|t| t.push_level)
            .unwrap_or_else(|| "result_only".to_string());
        // 最近任务 + 运行状态都来自该 workspace 的最近执行记录
        let (recent_records, is_running) = Self::recent_records_and_running(db, wid).await;
        let todos = match wid {
            Some(id) => db
                .get_todos_by_workspace_id(Some(id))
                .await
                .ok()
                .map(|ts| ts.into_iter().map(Self::todo_to_item).collect())
                .unwrap_or_default(),
            None => vec![],
        };
        let loops = match wid {
            Some(id) => db
                .list_loops_with_counts(Some(id))
                .await
                .ok()
                .map(|ls| ls.into_iter().map(Self::loop_to_item).collect())
                .unwrap_or_default(),
            None => vec![],
        };
        let workspaces = db
            .get_project_directories()
            .await
            .ok()
            .unwrap_or_default()
            .into_iter()
            .map(|d| WorkspaceItem {
                name: d.name.clone().unwrap_or_else(|| d.path.clone()),
                id: d.id,
                is_current: wid == Some(d.id),
            })
            .collect();
        // 已注册执行器列表 + 标记当前 workspace 配的默认执行器，供工作空间页渲染按钮排
        let current_executor = workspace.as_ref().map(|w| w.executor.as_str()).unwrap_or("");
        let available_executors = ctx
            .executor_registry
            .list_executors()
            .await
            .into_iter()
            .map(|t| {
                let name = t.as_str().to_string();
                let is_current = name == current_executor;
                ExecutorOption { name, is_current }
            })
            .collect();
        HelpCardState {
            current_group: current_group.to_string(),
            workspace,
            is_running,
            push_level,
            recent_records,
            todos,
            loops,
            workspaces,
            page,
            available_executors,
        }
    }

    /// 当前 workspace 摘要（名 + 默认执行器）。
    async fn build_workspace_summary(db: &Database, workspace_id: i64) -> Option<WorkspaceSummary> {
        let name = db.get_workspace_name_by_id(workspace_id).await.ok().flatten()?;
        let executor = crate::db::workspace_setting::get_workspace_settings(db, workspace_id)
            .await
            .ok()
            .flatten()
            .and_then(|s| s.default_response_executor)
            .unwrap_or_else(|| "claudecode".to_string());
        Some(WorkspaceSummary { id: workspace_id, name, executor })
    }

    /// 该 workspace 最近 5 条执行记录 → RecentTaskItem；顺带判断是否有 running。
    async fn recent_records_and_running(db: &Database, wid: Option<i64>) -> (Vec<RecentTaskItem>, bool) {
        let Some(id) = wid else {
            return (vec![], false);
        };
        let Ok((records, _)) = db.get_execution_records_by_workspace(id, 5, 0).await else {
            return (vec![], false);
        };
        let is_running = records.iter().any(|r| r.status == crate::models::ExecutionStatus::Running);
        let items = records.into_iter().map(|r| Self::record_to_recent_item(&r)).collect();
        (items, is_running)
    }

    /// Todo → 事项页列表项。
    fn todo_to_item(t: crate::models::Todo) -> TodoItem {
        use crate::models::TodoStatus;
        let status_icon = match t.status {
            TodoStatus::Completed => "✅",
            TodoStatus::Running | TodoStatus::InProgress => "▶️",
            _ => "⏸️",
        };
        TodoItem { id: t.id, title: t.title, status_icon: status_icon.to_string() }
    }

    /// LoopListRow → 环路页列表项。
    fn loop_to_item(l: crate::db::loop_::LoopListRow) -> LoopItem {
        LoopItem { id: l.loop_.id, name: l.loop_.name, status: l.loop_.status }
    }

    /// ExecutionRecord → 卡片「最近任务」项（状态 emoji + 标题 + 时间）。
    fn record_to_recent_item(r: &crate::models::ExecutionRecord) -> RecentTaskItem {
        use crate::models::ExecutionStatus;
        let status_icon = match r.status {
            ExecutionStatus::Success => "✅",
            ExecutionStatus::Running => "⏳",
            ExecutionStatus::Failed => "❌",
        };
        // 标题优先用触发源标题，其次结果文本，最后命令
        let title = r.source_todo_title.clone().or(r.result.clone()).unwrap_or_else(|| r.command.clone());
        RecentTaskItem {
            status_icon: status_icon.to_string(),
            title,
            time_desc: format_record_time(&r.started_at),
        }
    }

    /// Patch an existing interactive card message with new content.
    async fn patch_card(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        message_id: &str,
        card_json: &str,
    ) -> anyhow::Result<()> {
        let base_url = Self::base_url(credentials, bot_id)
            .ok_or_else(|| anyhow::anyhow!("no base_url for bot {}", bot_id))?;
        let token = Self::get_tenant_token(credentials, token_manager, bot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no token for bot {}", bot_id))?;

        let client = reqwest::Client::new();
        let url = format!(
            "{}/open-apis/im/v1/messages/{}",
            base_url, message_id
        );

        let body = serde_json::json!({
            "msg_type": "interactive",
            "content": card_json
        });

        let res = client
            .patch(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("patch_card request failed: {}", e))?;

        let status = res.status();
        if !status.is_success() {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            return Err(anyhow::anyhow!("patch_card failed: {} {:?}", status, body));
        }

        tracing::debug!("[feishu:{}] patch_card ok for message {}", bot_id, message_id);
        Ok(())
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

    /// Send an interactive card message to a Feishu recipient.
    #[allow(dead_code)]
    async fn send_card(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        receive_id: &str,
        receive_id_type: &str,
        card_json: &str,
    ) -> anyhow::Result<()> {
        let base_url = Self::base_url(credentials, bot_id)
            .ok_or_else(|| anyhow::anyhow!("no base_url for bot {}", bot_id))?;
        let token = Self::get_tenant_token(credentials, token_manager, bot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no token for bot {}", bot_id))?;

        let client = reqwest::Client::new();
        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type={}",
            base_url, receive_id_type
        );

        // 飞书 Interactive Card 的 content 直接是 JSON 字符串，不需要额外的嵌套
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "interactive",
            "content": card_json
        });

        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("send_card request failed: {}", e))?;

        let status = res.status();
        if !status.is_success() {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            return Err(anyhow::anyhow!("send_card failed: {} {:?}", status, body));
        }

        tracing::debug!(
            "[feishu:{}] send_card ok to {} ({})",
            bot_id, receive_id, receive_id_type
        );
        Ok(())
    }

    /// Reply to a message with an interactive card.
    async fn reply_card(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        message_id: &str,
        card_json: &str,
    ) -> anyhow::Result<()> {
        let base_url = Self::base_url(credentials, bot_id)
            .ok_or_else(|| anyhow::anyhow!("no base_url for bot {}", bot_id))?;
        let token = Self::get_tenant_token(credentials, token_manager, bot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no token for bot {}", bot_id))?;

        let client = reqwest::Client::new();
        // 使用 reply API 而不是 create
        let url = format!(
            "{}/open-apis/im/v1/messages/{}/reply",
            base_url, message_id
        );

        let body = serde_json::json!({
            "msg_type": "interactive",
            "content": card_json
        });

        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("reply_card request failed: {}", e))?;

        let status = res.status();
        if !status.is_success() {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            return Err(anyhow::anyhow!("reply_card failed: {} {:?}", status, body));
        }

        tracing::debug!(
            "[feishu:{}] reply_card ok to message {}",
            bot_id, message_id
        );
        Ok(())
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

    /// Send a card message using a specific receive_id_type.
    pub async fn send_card_raw(
        &self,
        bot_id: i64,
        receive_id: &str,
        receive_id_type: &str,
        card_json: &str,
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
        // 飞书 API 要求 content 字段是字符串格式的 JSON
        // json! 宏会自动将 &str 转义为 JSON 字符串值，无需手动处理
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "interactive",
            "content": card_json
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
            return Err(anyhow::anyhow!("send_card_raw failed: {} {:?}", status, body));
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

pub(crate) struct SlashCommandMatch<'a> {
    command: &'a str,
    body: &'a str,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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
    fn test_parse_card_command_extracts_command_text() {
        // 无参命令：cmd:/new → /new
        assert_eq!(
            FeishuListener::parse_card_command("cmd:/new"),
            Some("/new".to_string())
        );
        // 带参命令：参数原样保留，交由后续 parse_slash_command 解析
        assert_eq!(
            FeishuListener::parse_card_command("cmd:/bind my-project"),
            Some("/bind my-project".to_string())
        );
    }

    #[test]
    fn test_parse_card_command_returns_none_for_non_cmd_prefix() {
        // nav:/act:/未知前缀/空串都不是命令点击，返回 None
        assert_eq!(FeishuListener::parse_card_command("nav:/help common"), None);
        assert_eq!(FeishuListener::parse_card_command("act:/delete-mode cancel"), None);
        assert_eq!(FeishuListener::parse_card_command(""), None);
    }

    #[test]
    fn test_parse_card_action_variants() {
        use super::CardAction;
        // 各 verb 正常解析（bind/runtodo/runloop 参数是 i64）
        assert_eq!(FeishuListener::parse_card_action("act:/stop"), Some(CardAction::Stop));
        assert_eq!(FeishuListener::parse_card_action("act:/new"), Some(CardAction::New));
        assert_eq!(FeishuListener::parse_card_action("act:/sethome"), Some(CardAction::SetHome));
        assert_eq!(FeishuListener::parse_card_action("act:/bind 5"), Some(CardAction::Bind(5)));
        assert_eq!(FeishuListener::parse_card_action("act:/runtodo 10"), Some(CardAction::RunTodo(10)));
        assert_eq!(FeishuListener::parse_card_action("act:/runloop 20"), Some(CardAction::RunLoop(20)));
        assert_eq!(
            FeishuListener::parse_card_action("act:/push result_only"),
            Some(CardAction::Push("result_only".to_string()))
        );
        // 缺参数 → None
        assert_eq!(FeishuListener::parse_card_action("act:/bind"), None);
        assert_eq!(FeishuListener::parse_card_action("act:/runtodo"), None);
        assert_eq!(FeishuListener::parse_card_action("act:/push"), None);
        // 非 i64 参数 / 未知 verb / 非 act 前缀 → None
        assert_eq!(FeishuListener::parse_card_action("act:/bind abc"), None);
        assert_eq!(FeishuListener::parse_card_action("act:/unknown"), None);
        assert_eq!(FeishuListener::parse_card_action("nav:/help task"), None);
        assert_eq!(FeishuListener::parse_card_action("cmd:/new"), None);
    }

    #[test]
    fn test_resolve_receive_target_group_vs_private() {
        use crate::feishu::ChannelMessage;
        // 群聊：channel(chat_id)非空 → 用 chat_id 作为推送目标
        let group_msg = ChannelMessage {
            id: "om1".to_string(),
            sender: "ou_user".to_string(),
            sender_type: None,
            content: "act:/stop".to_string(),
            channel: "oc_group".to_string(),
            timestamp: 0,
            chat_type: Some("card_callback".to_string()),
            mentioned_open_ids: vec![],
        };
        assert_eq!(FeishuListener::resolve_receive_target(&group_msg), ("oc_group", "chat_id"));
        // 私聊：channel 空 → 回退到点击者 open_id
        let private_msg = ChannelMessage { channel: String::new(), ..group_msg.clone() };
        assert_eq!(FeishuListener::resolve_receive_target(&private_msg), ("ou_user", "open_id"));
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
