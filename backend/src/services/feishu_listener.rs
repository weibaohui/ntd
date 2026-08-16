use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::feishu::sdk::token_manager::TokenManager;
use crate::feishu::{
    create_channel, ChannelMessage, FeishuChannelService, FeishuConfig, FeishuConnectionMode,
    FeishuDomain,
};

use crate::service_context::ServiceContext;
use crate::task_manager::TaskManager;
use crate::db::{Database, NewFeishuMessage};
use crate::models::{AgentBot, BotConfig, build_trigger_params};
use crate::services::feishu_api_client::FeishuApiClient;
use crate::services::feishu_card_actions::CardActionHandler;
use crate::services::feishu_slash_commands::SlashCommandHandler;
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

pub(crate) struct FeishuCommandContext<'a> {
    pub(crate) db: &'a Arc<Database>,
    pub(crate) credentials: &'a DashMap<i64, (String, String, String)>,
    pub(crate) token_manager: &'a Arc<TokenManager>,
    /// ServiceContext：供 handle_help 查可用执行器列表（assemble_help_card_state 需要）。
    pub(crate) ctx: &'a ServiceContext,
    pub(crate) bot_id: i64,
    pub(crate) chat_type: &'a str,
    pub(crate) sender: &'a str,
    pub(crate) channel: &'a str,
    pub(crate) message_id: &'a str,
    pub(crate) content: &'a str,
    pub(crate) reaction_id: Option<&'a str>,
}

/// 卡片 act:/ 动作（点击按钮要执行的副作用）。
/// parse_card_action 把 "act:/xxx" 解析成它，handle_card_callback 的 act 分支按它分发。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CardAction {
    Stop,
    New,
    /// 切换工作空间，参数为 workspace_id
    Bind(i64),
    /// 触发事项，参数为 todo_id
    RunTodo(i64),
    /// 触发环路，参数为 loop_id
    RunLoop(i64),
    /// 触发任务再执行（104 新增），参数为 task_id；仅环路模式任务可执行
    RunTask(i64),
    /// 设置推送级别，参数为 disabled/result_only/all
    Push(String),
    /// 设置管家执行器，参数为执行器名（ExecutorType::as_str）
    SetButlerExecutor(String),
}


/// 卡片 act 动作的执行结果（供 patch_after_action 渲染顶部提示）。
pub(crate) struct ActionOutcome {
    pub(crate) success: bool,
    pub(crate) message: String,
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
            // 106 体检 C3 修复：listen() 返回（连续重连失败耗尽额度）后此前只打一行
            // 日志，bot 永久收不到消息直到 daemon 重启。这里加 supervisor：
            // 带退避地重启监听（30s 起步、上限 10 分钟），配置修复后无需重启进程即可自愈。
            let mut restart_delay_secs = 30u64;
            const MAX_RESTART_DELAY_SECS: u64 = 600;
            loop {
                tracing::info!("[feishu:{}] starting listen()", bot_id);
                // listen() 的存活时长：曾稳定运行过说明故障已恢复，重启退避应回零。
                let listen_started = std::time::Instant::now();
                match ch.listen(tx.clone()).await {
                    Ok(()) => tracing::warn!("[feishu:{}] listen() returned Ok", bot_id),
                    Err(e) => tracing::error!("[feishu:{}] listen() error: {e}", bot_id),
                }
                // 106 评审修复：退避只用于「反复快速失败」——listen() 稳定运行过
                // （>10 分钟）再退出时，重置回起步值；否则配置恢复后每次重启都要
                // 先等已爬升到上限的退避（最长 10 分钟才自愈）。
                if listen_started.elapsed() >= std::time::Duration::from_secs(MAX_RESTART_DELAY_SECS) {
                    restart_delay_secs = 30;
                }
                tracing::warn!(
                    "[feishu:{}] restarting listener in {}s", bot_id, restart_delay_secs
                );
                tokio::time::sleep(std::time::Duration::from_secs(restart_delay_secs)).await;
                restart_delay_secs = (restart_delay_secs * 2).min(MAX_RESTART_DELAY_SECS);
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
            FeishuApiClient::resolve_bot_open_id(&self.bot_credentials, &self.token_manager, bot.id)
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
    pub(crate) async fn handle_message(context: ListenerMessageContext<'_>, msg: &ChannelMessage) {
        // 入口日志：排查"消息为什么没反应"的第一线索
        tracing::info!(
            "[feishu:{}] handle_message: sender={}, bot_open_id={}, content={:?}, chat_type={:?}",
            context.bot_id, msg.sender, context.bot_open_id, msg.content, msg.chat_type
        );
        // 阶段 0：卡片回调处理（由飞书卡片按钮点击触发）
        if msg.chat_type.as_deref() == Some("card_callback") {
            CardActionHandler::handle_card_callback(context, msg).await;
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
        // 一个 bot 一个工作空间，chat 消息全走阶段6（斜杠规则 或 空间管家）。
        Self::route_slash_or_butler(&context, msg, &prep).await;
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
        FeishuApiClient::add_reaction(credentials, token_manager, bot_id, message_id, "THUMBSUP").await
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
        if prep.content == "/feishupush" { SlashCommandHandler::handle_feishupush(mk_ctx()).await; return true; }
        if prep.content == "/list" { SlashCommandHandler::handle_list(mk_ctx()).await; return true; }
        if prep.content == "/help" || prep.content.starts_with("/help ") {
            SlashCommandHandler::handle_help(mk_ctx()).await; return true;
        }
        if prep.content == "/new" { SlashCommandHandler::handle_new(mk_ctx()).await; return true; }
        if prep.content == "/stop" {
            SlashCommandHandler::handle_stop(context.task_manager, mk_ctx()).await; return true;
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
        FeishuApiClient::delete_reaction(
            context.credentials, context.token_manager, context.bot_id, &message.id, rid,
        ).await;
    }

    /// 阶段 6：斜杠精确匹配 或 空间管家（108：不再有默认响应兜底）
    pub(crate) async fn route_slash_or_butler(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) {
        // 是斜杠命令 → 走规则精确匹配；未命中规则的消息（含未知 /xxx）与普通
        // 消息同路：全部交给空间管家，由管家 AI 自主决定下一步，不做任何硬编码降级。
        if let Some(command_ctx) = Self::parse_slash_command(prep.content) {
            Self::dispatch_slash_command(context, msg, prep, &command_ctx).await;
        } else {
            Self::dispatch_butler_chat(context, msg, prep).await;
        }
    }

    /// 阶段 6a：自定义斜杠命令规则精确匹配 + debounce push
    /// 规则未命中 / workspace 异常时转空间管家——斜杠命令没有兜底执行。
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
                tracing::warn!("bot {} has no workspace_id, routing slash command to butler", context.bot_id);
                return Self::dispatch_butler_chat(context, msg, prep).await;
            }
            Err(e) => {
                tracing::error!("failed to get workspace_id for bot {}: {}", context.bot_id, e);
                return Self::dispatch_butler_chat(context, msg, prep).await;
            }
        };

        let matched_rule = Self::find_slash_rule(context.db, workspace_id, command_ctx.command).await;
        let Some(rule) = matched_rule else {
            // 未命中规则：消息原文（含 /xxx）交给管家，管家可追问或按闲聊处理
            return Self::dispatch_butler_chat(context, msg, prep).await;
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

    /// 阶段 6b：聊天直连出口（108 修订）。
    ///
    /// 单聊（p2p）未命中斜杠 → 与「对话执行器」直聊（dm_chat，不注入专家）；
    /// 群聊（group）未命中斜杠 → 群聊管家（butler_chat，注入管家专家 prompt）。
    /// 两者共用 butler_executor 字段选执行器；未配置 → 回复配置引导提示
    /// （提示不是兜底执行：不触发任何 todo/loop/执行器）。
    async fn dispatch_butler_chat(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) {
        tracing::debug!(
            "[feishu:{}] dispatch_butler_chat: content={:?}, chat_type={}",
            context.bot_id, prep.content, prep.chat_type
        );
        // 空消息不触发任何响应
        if prep.content.is_empty() {
            return;
        }
        // 对话执行器配置挂在工作空间上：bot 未绑定工作空间时无法路由，
        // 提示用户先完成绑定（提示本身不执行任何任务）。
        let workspace_id = match context.db.get_agent_bot_workspace_id(context.bot_id).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::warn!("bot {} has no workspace_id, cannot route to butler", context.bot_id);
                return Self::reply_butler_hint(context, msg, prep.chat_type, "⚠️ 本 bot 尚未绑定工作空间，请先在 ntd 中完成绑定。").await;
            }
            Err(e) => {
                tracing::error!("failed to get workspace_id for bot {}: {}", context.bot_id, e);
                return Self::reply_butler_hint(context, msg, prep.chat_type, "⚠️ 查询工作空间失败，请稍后重试。").await;
            }
        };

        // 读取工作空间的对话执行器配置；DB 失败与「无配置」对用户同等表现为未配置提示，
        // 但日志里区分出来便于排查。
        let settings = crate::db::workspace_setting::get_workspace_settings(context.db, workspace_id)
            .await
            .map_err(|e| tracing::warn!("[feishu:{}] read workspace settings failed: {}", context.bot_id, e))
            .ok()
            .flatten();
        let butler_executor = Self::resolve_butler_executor(settings);

        match butler_executor {
            Some(executor) => Self::debounce_push_butler_chat(
                context.debounce,
                context.bot_id,
                msg,
                prep.chat_type,
                &executor,
                prep.content,
                Some(workspace_id),
                prep.is_mention,
            ),
            // 提示语按场景分：单聊配的是「对话执行器」，群聊配的是「群聊管家」——
            // 用户按收到的提示去找对应的配置项，不混用概念
            None => {
                let hint = if prep.chat_type == "p2p" {
                    "🤖 尚未配置对话执行器，请在工作空间设置中选择执行器后再对话。"
                } else {
                    "🤖 本群尚未配置群聊管家，请在工作空间设置中选择管家执行器。"
                };
                Self::reply_butler_hint(context, msg, prep.chat_type, hint).await;
            }
        }
    }

    /// 从工作空间设置解析对话执行器：无设置行 / 字段 NULL / 空串统一视为未配置。
    /// 抽成纯函数便于单测——dispatch_butler_chat 的「提示 vs 直聊」分支全挂在这个
    /// 解析结果上，是聊天直连通路的唯一判据（108 修订：单聊/群聊共用此执行器）。
    fn resolve_butler_executor(
        settings: Option<crate::db::entity::workspace_settings::Model>,
    ) -> Option<String> {
        settings?.butler_executor.filter(|e| !e.is_empty())
    }

    /// 聊天直连的 trigger_type 落值（108 修订）：p2p→dm_chat（单聊直聊）、
    /// 其余（群聊）→butler_chat（群聊管家）。下游 dispatch_execution 据此分流
    /// 专家注入（仅 butler_chat 注入），消息监控据标签区分两种对话。
    fn chat_trigger_type(chat_type: &str) -> &'static str {
        if chat_type == "p2p" { "dm_chat" } else { "butler_chat" }
    }

    /// 阶段 6b 的提示回复出口：按 chat_type 解析 receive target 后发文本。
    /// 仅用于「无法路由到管家」的引导场景，本身不执行任何任务。
    async fn reply_butler_hint(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        chat_type: &str,
        text: &str,
    ) {
        let (receive_id, receive_id_type) = match chat_type {
            "p2p" => (msg.sender.clone(), "open_id"),
            _ => (msg.channel.clone(), "chat_id"),
        };
        FeishuApiClient::send_text(
            context.credentials,
            context.token_manager,
            context.bot_id,
            &receive_id,
            receive_id_type,
            text,
        )
        .await;
    }

    /// 阶段 6b-i：把聊天直连消息塞进 debounce（108 修订：单聊/群聊共用）
    /// trigger_type 按 chat_type 拆分：p2p→dm_chat（纯直聊）、group→butler_chat（管家），
    /// 消息监控可分开展示两种对话；dispatch_execution 把两者路由到同一 handler。
    #[allow(clippy::too_many_arguments)] // 参数来自上游 handler 的独立数据源，合并为 struct 增加认知负担
    fn debounce_push_butler_chat(
        debounce: &Arc<MessageDebounce>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        butler_executor: &str,
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
            todo_id: 0, // 聊天直连不绑定 todo
            todo_prompt: String::new(),
            executor: Some(butler_executor.to_string()),
            trigger_type: Self::chat_trigger_type(chat_type).to_string(),
            params: None,
            message_id: Some(msg.id.clone()),
            resume_session_id: None,
            resume_message: None,
            immediate,
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







































    /// Send a message via a specific bot's channel.
    pub async fn send(&self, bot_id: i64, text: &str, recipient: &str) -> anyhow::Result<()> {
        if let Some(ch) = self.channels.get(&bot_id) {
            ch.send(text, recipient).await?;
            Ok(())
        } else {
            anyhow::bail!("bot {} not running", bot_id)
        }
    }



    // --- Feishu API helpers ---






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
    fn test_group_message_requires_mention_when_enabled() {
        let cfg = BotConfig {
            group_enabled: true,
            group_require_mention: true,
            ..Default::default()
        };
        assert!(!FeishuListener::is_message_allowed("group", false, &cfg));
        assert!(FeishuListener::is_message_allowed("group", true, &cfg));
    }

    /// 管家执行器解析的构造辅助：只需关心 butler_executor 一个字段，其余给默认值
    fn settings_with_butler(butler_executor: Option<String>) -> crate::db::entity::workspace_settings::Model {
        crate::db::entity::workspace_settings::Model {
            id: 1,
            workspace_id: 1,
            butler_expert_name: None,
            butler_executor,
            system_prompt: None,
            delegate_max_rounds: None,
            updated_at: None,
        }
    }

    /// 无设置行 → None（走未配置提示分支）
    #[test]
    fn test_resolve_butler_executor_no_settings_row() {
        assert_eq!(FeishuListener::resolve_butler_executor(None), None);
    }

    /// 字段 NULL → None
    #[test]
    fn test_resolve_butler_executor_null_field() {
        assert_eq!(
            FeishuListener::resolve_butler_executor(Some(settings_with_butler(None))),
            None
        );
    }

    /// 空串 = 显式清空，与 NULL 同义 → None
    #[test]
    fn test_resolve_butler_executor_empty_string_is_unconfigured() {
        assert_eq!(
            FeishuListener::resolve_butler_executor(Some(settings_with_butler(Some(String::new())))),
            None
        );
    }

    /// 已配置 → Some(原名)
    #[test]
    fn test_resolve_butler_executor_configured() {
        assert_eq!(
            FeishuListener::resolve_butler_executor(Some(settings_with_butler(Some("pi".to_string())))),
            Some("pi".to_string())
        );
    }

    /// trigger_type 按 chat_type 分流（108 修订核心行为）：p2p→dm_chat、群聊→butler_chat。
    /// 抽出落值表达式为纯函数，让分流规则可被直接单测（改动词/加场景先改测试）。
    #[test]
    fn test_chat_trigger_type_p2p_is_dm_chat() {
        assert_eq!(FeishuListener::chat_trigger_type("p2p"), "dm_chat");
    }

    #[test]
    fn test_chat_trigger_type_group_is_butler_chat() {
        // 非明确 p2p 一律按群聊处理：飞书 chat_type 只有 p2p/group 两种，
        // 用 else 分支兜底避免未来新增类型时静默落错 trigger_type
        assert_eq!(FeishuListener::chat_trigger_type("group"), "butler_chat");
        assert_eq!(FeishuListener::chat_trigger_type("unknown"), "butler_chat");
    }

    #[tokio::test]
    async fn test_capture_owner_if_p2p_writes_only_for_p2p() {
        // 私聊消息：sender 被捕获为 owner；群聊消息不覆盖已锁定的 owner
        use crate::db::Database;
        use crate::feishu::ChannelMessage;
        use std::sync::Arc;
        let db = Arc::new(Database::new(":memory:").await.unwrap());
        let bot_id = db
            .create_agent_bot("feishu", "t", "app", "secret", None, None, 1)
            .await
            .unwrap();
        let p2p_msg = ChannelMessage {
            id: "om1".to_string(),
            sender: "ou_owner".to_string(),
            sender_type: None,
            content: "hi".to_string(),
            channel: String::new(),
            timestamp: 0,
            chat_type: Some("p2p".to_string()),
            mentioned_open_ids: vec![],
        };
        FeishuListener::capture_owner_if_p2p(&db, bot_id, &p2p_msg, "p2p").await;
        assert_eq!(
            db.get_owner_open_id(bot_id).await.unwrap(),
            Some("ou_owner".to_string())
        );
        // 群聊：不捕获，防群里他人覆盖已锁定的所有者
        let group_msg = ChannelMessage {
            sender: "ou_other".to_string(),
            channel: "oc_group".to_string(),
            ..p2p_msg
        };
        FeishuListener::capture_owner_if_p2p(&db, bot_id, &group_msg, "group").await;
        assert_eq!(
            db.get_owner_open_id(bot_id).await.unwrap(),
            Some("ou_owner".to_string())
        );
    }

}
