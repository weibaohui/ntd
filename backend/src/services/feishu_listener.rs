use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use std::sync::RwLock;

use crate::feishu::sdk::config::Config as FeishuSdkConfig;
use crate::feishu::sdk::token_manager::TokenManager;
use crate::feishu::{
    create_channel, ChannelMessage, FeishuChannelService, FeishuConfig, FeishuConnectionMode,
    FeishuDomain,
};

use crate::service_context::ServiceContext;
use crate::config::Config as AppConfig;
use crate::task_manager::TaskManager;
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
    task_manager: &'a Arc<TaskManager>,
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

/// 编排器专用：handle_message 阶段函数之间传递的"消息预处理结果"。
/// 把 trim content / chat_type / is_mention / reaction_id 这类一次性解析的字段聚在一起，
/// 避免每个阶段函数都重复算一遍，编排器也只需要在 phases 间传一个 &MessagePrep。
struct MessagePrep<'a> {
    chat_type: &'a str,
    content: &'a str,
    is_mention: bool,
    reaction_id: Option<String>,
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
        let task_manager = self.ctx.task_manager.clone();
        tokio::spawn(async move {
            tracing::info!("[feishu:{}] message receiver loop started", bot_id);
            while let Some(msg) = rx.recv().await {
                let context = ListenerMessageContext {
                    db: &db,
                    config: &config,
                    token_manager: &token_manager,
                    credentials: &credentials,
                    debounce: &debounce,
                    task_manager: &task_manager,
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
        // 阶段 0：跳过机器人自己发的消息（不持久化、不加 reaction）
        if msg.sender == context.bot_open_id {
            tracing::info!("[feishu:{}] skipping self-sent message", context.bot_id);
            return;
        }
        // 阶段 1：解析消息 + 持久化 + 加 reaction，产出 MessagePrep 给后续阶段复用
        let prep = Self::prepare_message(&context, msg).await;
        // 阶段 2~7：每个阶段 bool 返回 true → 编排器直接 return
        if Self::try_route_builtin_command(&context, msg, &prep).await { return; }
        if Self::should_skip_for_message_filters(&context, msg, &prep).await { return; }
        Self::try_promote_pending_binding(&context, msg, &prep).await;
        if Self::try_route_project_binding(&context, msg, &prep).await { return; }
        Self::route_slash_or_default_response(&context, msg, &prep).await;
        Self::log_echo_reply(context.bot_id, msg, prep.chat_type, context.bot_config);
        Self::cleanup_reaction(&context, msg, prep.reaction_id.as_deref()).await;
    }

    /// 阶段 1：解析消息基本信息 + 持久化入站消息 + 加 processing reaction
    /// 返回 MessagePrep 供后续阶段复用（避免每个阶段重复 trim content / 查 chat_type）
    async fn prepare_message<'a>(
        context: &ListenerMessageContext<'_>,
        msg: &'a ChannelMessage,
    ) -> MessagePrep<'a> {
        let chat_type = msg.chat_type.as_deref().unwrap_or("p2p");
        let is_mention = !msg.mentioned_open_ids.is_empty();
        let content = msg.content.trim();
        // 持久化是 audit 用途，失败仅记录；不影响主流程决策
        Self::persist_inbound_message(context.db, context.bot_id, msg, chat_type, is_mention).await;
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
        })
        .await
        .ok();
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
    async fn try_route_builtin_command(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) -> bool {
        // 把 listener 字段聚成 builder，命令分支只关心命令名 + 处理函数
        let mk_ctx = || FeishuCommandContext {
            db: context.db,
            credentials: context.credentials,
            token_manager: context.token_manager,
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
        // /bind 支持空参数（展示列表）或带参数（绑定指定项目）
        if prep.content == "/bind" || prep.content.starts_with("/bind ") {
            Self::handle_bind(mk_ctx()).await; return true;
        }
        if prep.content == "/unbind" { Self::handle_unbind(mk_ctx()).await; return true; }
        if prep.content == "/new" { Self::handle_new(mk_ctx()).await; return true; }
        if prep.content == "/stop" {
            Self::handle_stop(context.task_manager, mk_ctx()).await; return true;
        }
        false
    }

    /// 阶段 3：消息接收过滤（命中任一条就 return true）
    /// 三道闸：bot 是否接收此类消息 → 该 chat_type 是否启用响应 → 群聊白名单
    async fn should_skip_for_message_filters(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) -> bool {
        // 闸 1：bot 接收策略（私聊启用 / 群聊启用 + 是否需要 @）
        if !Self::is_message_allowed(prep.chat_type, prep.is_mention, context.bot_config) {
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
    async fn cleanup_reaction(
        context: &ListenerMessageContext<'_>,
        message: &ChannelMessage,
        reaction_id: Option<&str>,
    ) {
        let Some(rid) = reaction_id else { return };
        Self::delete_reaction(
            context.credentials, context.token_manager, context.bot_id, &message.id, rid,
        ).await;
    }

    /// 阶段 4：把页面创建的 __pending__ binding 关联到当前真实 chat
    /// 页面"新建绑定"时 chat_id 未知，先写 __pending__ 占位；todo 不存在则放弃晋升
    async fn try_promote_pending_binding(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) {
        // 守卫 1：当前 chat 已存在 binding（无论是 pending 还是已关联）就不再晋升；
        // 避免把 pending binding 错误覆盖到已关联的真实 binding 上
        // （也避免 unique 约束冲突：chat_id 在 (bot_id, chat_id) 上唯一）
        if context.db
            .get_feishu_project_binding(context.bot_id, &msg.channel)
            .await
            .ok()
            .flatten()
            .is_some()
        {
            return;
        }
        // 单行查询代替之前 `get_feishu_project_bindings(bot)` 全表扫；
        // PENDING_CHAT_ID 是约定的占位 chat_id，直接命中 unique 索引
        let pending = match context.db
            .get_feishu_project_binding(context.bot_id, crate::models::PENDING_CHAT_ID)
            .await
        {
            Ok(Some(p)) => p,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!("[feishu:{}] failed to query pending binding: {}", context.bot_id, e);
                return;
            }
        };
        // 防御：页面可能已经删了 todo 但 binding 残留，没 todo 就别关联过去
        if context.db.get_todo(pending.todo_id).await.ok().flatten().is_none() {
            return;
        }
        match context.db
            .attach_feishu_project_binding(pending.id, &msg.channel, prep.chat_type)
            .await
        {
            Ok(_) => tracing::info!(
                "[feishu:{}] promoted pending binding {} (project_dir_id={}) to chat {}",
                context.bot_id, pending.id, pending.project_dir_id, msg.channel
            ),
            Err(e) => tracing::warn!(
                "[feishu:{}] failed to promote pending binding: {}",
                context.bot_id, e
            ),
        }
    }

    /// 阶段 5：项目绑定执行路径
    /// - 无绑定 / 绑定 todo 不存在 → 返回 false，让控制流落到斜杠命令/默认回复
    /// - 绑定 enabled=false → **直接返回 false**，让控制流落到斜杠命令/默认回复
    ///   ⚠ 这是相对 pre-refactor 的**有意行为变化**：pre-refactor 的 disabled
    ///   分支只清 reaction 后继续走 todo/debounce；重构后 disabled 不再触发项目
    ///   执行，与 enabled 路径完全分离。reaction 清理统一由编排器末尾
    ///   （handle_message 收尾）兜底，本分支不再重复 `cleanup_reaction`。
    /// - 绑定 enabled 且 todo 在 → 决定 resume 还是新 session，push 到 debounce 后返回 true
    async fn try_route_project_binding(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) -> bool {
        let Some(binding) = Self::resolve_project_binding(context.db, context.bot_id, &msg.channel).await else {
            return false; // 无绑定 / DB 错误 → 让控制流落到兜底
        };
        // enabled=false 的绑定不参与路由：直接让控制流落到兜底；
        // reaction 清理交给 handle_message 末尾统一收尾（见 docstring）
        if !binding.enabled {
            tracing::info!("[feishu:{}] binding {} disabled, fall through", context.bot_id, binding.id);
            return false;
        }
        let Some(todo) = context.db.get_todo(binding.todo_id).await.ok().flatten() else {
            tracing::warn!("[feishu:{}] bound todo #{} missing for chat {}", context.bot_id, binding.todo_id, msg.channel);
            return false;
        };
        let latest_record = Self::fetch_latest_record(context.db, binding.latest_record_id).await;
        let (resume_session_id, resume_message) =
            Self::decide_resume_session(latest_record.as_ref(), prep.content);
        // 日志保留 binding.session_id 与 latest_record.status：排查「为什么 session 没 resume /
        // 串了」的关键线索（详见 PR #665 review #3 CANDIDATE #3）。
        tracing::info!(
            "[feishu:{}] binding check: todo_id={}, latest_record_id={:?}, should_resume={}, binding.session_id={:?}, latest_record_status={:?}",
            context.bot_id,
            binding.todo_id,
            binding.latest_record_id,
            resume_session_id.is_some(),
            binding.session_id,
            latest_record.as_ref().map(|r| r.status.clone()),
        );
        Self::push_binding_execution(
            context.debounce,
            msg,
            prep.chat_type,
            prep.content,
            &binding,
            &todo,
            resume_session_id,
            resume_message,
        );
        Self::cleanup_reaction(context, msg, prep.reaction_id.as_deref()).await;
        true
    }

    /// 阶段 5a-i：取最近一条 execution record（按 binding 引用）
    async fn fetch_latest_record(
        db: &Arc<Database>,
        latest_record_id: Option<i64>,
    ) -> Option<crate::models::ExecutionRecord> {
        match latest_record_id {
            Some(rid) => db.get_execution_record(rid).await.ok().flatten(),
            None => None,
        }
    }

    /// 阶段 5a：取 chat 当前的项目绑定；DB 错误按 None 处理（不阻塞主流程）
    async fn resolve_project_binding(
        db: &Arc<Database>,
        bot_id: i64,
        channel: &str,
    ) -> Option<crate::db::feishu_project_binding::FeishuProjectBinding> {
        match db.get_feishu_project_binding(bot_id, channel).await {
            Ok(Some(b)) => Some(b),
            Ok(None) => None,
            Err(e) => {
                tracing::error!("[feishu:{}] query binding failed: {e}", bot_id);
                None
            }
        }
    }

    /// 阶段 5b：决定 resume 还是新开 session
    /// 从 latest_record 读 session_id：record 没有就开新 session
    /// （早期版本曾尝试用 `binding.session_id` 兜底，但首次执行时 binding.session_id
    /// 被设成 task_id 占位，fallback 永远不触发，已删除。）
    fn decide_resume_session(
        latest_record: Option<&crate::models::ExecutionRecord>,
        content: &str,
    ) -> (Option<String>, Option<String>) {
        // resume 三条件：record 有 session_id、记录不是 running（running 时 stdout JSONL 还在写）
        let should_resume = latest_record
            .map(|r| r.session_id.is_some() && r.status != crate::models::ExecutionStatus::Running)
            .unwrap_or(false);
        if !should_resume {
            return (None, None);
        }
        // 已通过 should_resume 守卫：latest_record 是 Some 且 r.session_id 是 Some，
        // 用 expect 表达「在 guard 假设下必定成立」。
        let real_sid = Some(
            latest_record
                .and_then(|r| r.session_id.clone())
                .expect("should_resume=true guarantees latest_record.session_id is Some"),
        );
        (real_sid, Some(content.to_string()))
    }

    /// 阶段 5c：把项目绑定执行任务塞进 debounce
    #[allow(clippy::too_many_arguments)]
    fn push_binding_execution(
        debounce: &Arc<MessageDebounce>,
        msg: &ChannelMessage,
        chat_type: &str,
        content: &str,
        binding: &crate::db::feishu_project_binding::FeishuProjectBinding,
        todo: &crate::models::Todo,
        resume_session_id: Option<String>,
        resume_message: Option<String>,
    ) {
        let pending = Self::build_binding_execution_message(
            msg,
            chat_type,
            content,
            binding,
            todo,
            resume_session_id,
            resume_message,
        );
        debounce.push(pending);
    }

    /// 阶段 5c 纯函数：从上下文构造 PendingMessage，与 debounce 副作用解耦以便单测。
    /// `content` 必须是 trimmed 后的原始消息（区别于 resume 上下文的 `resume_message`），
    /// 避免 `should_resume=false` 时 executor 收到空 content（PR #665 review #3 #2 修复）。
    #[allow(clippy::too_many_arguments)]
    fn build_binding_execution_message(
        msg: &ChannelMessage,
        chat_type: &str,
        content: &str,
        binding: &crate::db::feishu_project_binding::FeishuProjectBinding,
        todo: &crate::models::Todo,
        resume_session_id: Option<String>,
        resume_message: Option<String>,
    ) -> PendingMessage {
        let executor = todo.executor.as_deref().unwrap_or("claudecode");
        PendingMessage {
            bot_id: binding.bot_id,
            chat_id: msg.channel.clone(),
            chat_type: chat_type.to_string(),
            sender: msg.sender.clone(),
            content: content.to_string(),
            todo_id: binding.todo_id,
            todo_prompt: todo.prompt.clone(),
            executor: Some(executor.to_string()),
            trigger_type: "feishu_project_bind".to_string(),
            params: None,
            message_id: Some(msg.id.clone()),
            resume_session_id,
            resume_message,
            binding_id: Some(binding.id),
        }
    }

    /// 阶段 6：兜底路由（自定义斜杠命令规则 或 默认回复 todo）
    async fn route_slash_or_default_response(
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
        let matched_rule = Self::find_slash_rule(context.config, command_ctx.command);
        let Some(rule) = matched_rule else {
            // 没匹配上规则 → 走默认回复路径，保持向后兼容
            return Self::dispatch_default_response(context, msg, prep).await;
        };
        // 没有命令体（只输入了 /xxx）→ 不执行
        if command_ctx.body.is_empty() {
            return;
        }
        // 防御：rule 关联的 todo 可能已被删，没拿到 todo 就不 push
        let Ok(Some(todo)) = context.db.get_todo(rule.todo_id).await else {
            tracing::error!("Failed to fetch todo {} for slash command", rule.todo_id);
            return;
        };
        let (_, params) = build_trigger_params(&format!("{} {}", command_ctx.command, command_ctx.body));
        Self::push_slash_command_message(context.debounce, context.bot_id, msg, prep.chat_type, &todo, command_ctx.body, params);
    }

    /// 阶段 6a-i：查 enabled 的斜杠命令规则；锁内只读后尽早释放
    fn find_slash_rule(
        config: &Arc<RwLock<AppConfig>>,
        command: &str,
    ) -> Option<crate::config::SlashCommandRule> {
        let cfg = config.read().unwrap();
        cfg.slash_command_rules.iter()
            .find(|r| r.slash_command == command && r.enabled)
            .cloned()
    }

    /// 阶段 6a-ii：把斜杠命令消息塞进 debounce
    fn push_slash_command_message(
        debounce: &Arc<MessageDebounce>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        todo: &crate::models::Todo,
        body: &str,
        params: std::collections::HashMap<String, String>,
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
        });
    }

    /// 阶段 6b：默认回复 todo（如果配置了的话）
    /// todo 拉取失败时降级用空 prompt，避免整条消息被吞掉
    async fn dispatch_default_response(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        prep: &MessagePrep<'_>,
    ) {
        let default_todo_id = context.config.read().unwrap().default_response_todo_id;
        let Some(todo_id) = default_todo_id else { return };
        // 空消息不触发 todo
        if prep.content.is_empty() {
            return;
        }
        // 防御：default todo 可能已被删，拉取失败用空 prompt 而不是吞消息
        let todo_prompt = context.db.get_todo(todo_id).await
            .ok().flatten().map(|t| t.prompt).unwrap_or_default();
        let (_, params) = build_trigger_params(prep.content);
        Self::debounce_push_default(context.debounce, context.bot_id, msg, prep.chat_type, todo_id, todo_prompt, prep.content, params);
    }

    /// 阶段 6b-i：把默认回复消息塞进 debounce
    fn debounce_push_default(
        debounce: &Arc<MessageDebounce>,
        bot_id: i64,
        msg: &ChannelMessage,
        chat_type: &str,
        todo_id: i64,
        todo_prompt: String,
        content: &str,
        params: std::collections::HashMap<String, String>,
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
            binding_id: None,
        });
    }

    /// 阶段 7：调试回显日志（仅在 bot_config.echo_reply 开启时记录）
    /// 纯 tracing! 调用、无 IO，保持 fn 而非 async fn，避免编排器里 .await 噪音
    fn log_echo_reply(
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
                    let status_icon = if binding.status == crate::models::binding_status::RUNNING { "🟢" } else { "⏸️" };
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
        // ⚠️ 前缀匹配时若有多个候选（如 my-app / my-application 都匹配 "my"），
        //    返回歧义提示让用户精确输入。
        let directories = db.get_project_directories().await.unwrap_or_default();
        // 精确匹配 — 唯一正确
        let dir = directories.iter().find(|d| d.name.as_deref() == Some(project_name)).cloned();
        let dir = match dir {
            Some(d) => Some(d),
            None => {
                // 前缀匹配 — 检查是否有多选歧义
                let candidates: Vec<_> = directories.iter()
                    .filter(|d| d.name.as_deref().map(|n| n.starts_with(project_name)).unwrap_or(false))
                    .collect();
                if candidates.is_empty() {
                    None
                } else if candidates.len() == 1 {
                    Some(candidates[0].clone())
                } else {
                    // 多个候选，返回歧义提示
                    let names: Vec<String> = candidates.iter()
                        .filter_map(|d| d.name.as_deref())
                        .map(|n| format!("• {}", n))
                        .collect();
                    let msg = format!(
                        "⚠️ 「{}」匹配到多个项目：\n{}\n\n请使用完整名称，例如：/bind {}",
                        project_name,
                        names.join("\n"),
                        candidates[0].name.as_deref().unwrap_or(""),
                    );
                    Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type, &msg).await;
                    if let Some(rid) = reaction_id {
                        Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                    }
                    return;
                }
            }
        };

        match dir {
            Some(dir) => {
                // Check if already bound
                if let Ok(Some(existing)) = db.get_feishu_project_binding(bot_id, channel).await {
                    if let Err(e) = db.delete_feishu_project_binding(existing.id).await {
                        tracing::warn!("[feishu:{}] failed to delete existing binding {} before rebind: {}", bot_id, existing.id, e);
                    }
                }

                // Try to find a pending binding created via Web UI (chat_id=PENDING_CHAT_ID)
                let pending_bindings = db.get_feishu_project_bindings(bot_id).await.unwrap_or_default();
                let pending = pending_bindings.iter()
                    .find(|b| b.project_dir_id == dir.id && b.chat_id == crate::models::PENDING_CHAT_ID)
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
                         用户诉求：{{message}}\n\
                         项目目录：{path}",
                        name = dir.name.as_deref().unwrap_or("unknown"),
                        path = dir.path,
                    );

                    match db.create_todo(&todo_title, &todo_prompt).await {
                        Ok(todo_id) => {
                            let _ = db.update_todo_workspace(todo_id, Some(&dir.path)).await;
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
                // 任务运行时拒绝解绑，避免 session 链丢失。
                // 用户必须先通过 Web UI 停止运行中的任务，才能解绑。
                if binding.status == crate::models::binding_status::RUNNING {
                    Self::send_text(credentials, token_manager, bot_id, &receive_id, receive_id_type,
                        "⚠️ 当前有任务正在执行（session 链会被丢弃）。\n请先通过 Web 界面「运行管理」停止任务，再发送 /unbind 解绑。")
                        .await;
                    if let Some(rid) = reaction_id {
                        Self::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                    }
                    return;
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

    /// Handle /new — start a fresh session without resuming the previous one.
    /// Unlike normal messages which resume existing sessions, this forces a new session.
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
            }
            Ok(None) => {
                Self::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "📭 当前聊天未绑定任何项目，无法使用 /new。\n\n请先使用 /bind <项目名称> 绑定一个项目。",
                )
                .await;
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
            }
        }

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
    use crate::config::{Config as AppConfig, SlashCommandRule};
    use crate::models::{BotConfig, ExecutionRecord, ExecutionStatus};
    use crate::db::feishu_project_binding::FeishuProjectBinding;
    use std::sync::{Arc, RwLock};

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

    #[test]
    fn test_decide_resume_session_no_record_returns_none() {
        // 没 record → 不 resume，返回 (None, None)
        let (sid, msg) = FeishuListener::decide_resume_session(None, "hello");
        assert!(sid.is_none());
        assert!(msg.is_none());
    }

    #[test]
    fn test_decide_resume_session_running_record_skips_resume() {
        // record.status == Running → 不 resume（避免和正在写 stdout JSONL 的进程抢文件）
        let record = dummy_record(ExecutionStatus::Running, Some("real_sid"));
        let (sid, msg) = FeishuListener::decide_resume_session(Some(&record), "hi");
        assert!(sid.is_none(), "running record should not resume");
        assert!(msg.is_none());
    }

    #[test]
    fn test_decide_resume_session_finished_record_uses_record_sid() {
        // record 已结束 + 有 session_id → 用 record 里的 sid
        let record = dummy_record(ExecutionStatus::Success, Some("real_claude_sid"));
        let (sid, msg) = FeishuListener::decide_resume_session(Some(&record), "继续");
        assert_eq!(sid.as_deref(), Some("real_claude_sid"));
        assert_eq!(msg.as_deref(), Some("继续"));
    }

    #[test]
    fn test_decide_resume_session_finished_no_sid_skips_resume() {
        // record 已结束但没有 session_id → 不满足 should_resume 条件（需要 sid），
        // 保持原行为：返回 (None, None)，由 caller 决定下一步
        let record = dummy_record(ExecutionStatus::Success, None);
        let (sid, msg) = FeishuListener::decide_resume_session(Some(&record), "msg");
        assert!(sid.is_none());
        assert!(msg.is_none());
    }

    #[test]
    fn test_find_slash_rule_picks_enabled_match() {
        // enabled 命中的规则返回；disabled 的不返回
        let mut config = AppConfig::default();
        config.slash_command_rules = vec![
            SlashCommandRule {
                slash_command: "/on".into(),
                todo_id: 1,
                enabled: true,
            },
            SlashCommandRule {
                slash_command: "/off".into(),
                todo_id: 2,
                enabled: false,
            },
        ];
        let cfg = Arc::new(RwLock::new(config));
        let rule = FeishuListener::find_slash_rule(&cfg, "/on");
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().todo_id, 1);
        let rule = FeishuListener::find_slash_rule(&cfg, "/off");
        assert!(rule.is_none(), "disabled rules must be filtered out");
        let rule = FeishuListener::find_slash_rule(&cfg, "/unknown");
        assert!(rule.is_none());
    }

    #[test]
    fn test_build_binding_execution_message_preserves_content_on_no_resume() {
        // PR #665 review #3 CANDIDATE #2 回归测试：resume_message=None 时 content
        // 仍必须是原始 trimmed 消息，绝不能吞成空串。
        let msg = dummy_msg("请帮我修复登录 bug");
        let binding = dummy_binding();
        let todo = dummy_todo();
        let pending = FeishuListener::build_binding_execution_message(
            &msg,
            "p2p",
            "请帮我修复登录 bug",
            &binding,
            &todo,
            None,
            None,
        );
        assert_eq!(pending.content, "请帮我修复登录 bug");
        assert!(pending.resume_message.is_none());
        assert!(pending.resume_session_id.is_none());
    }

    #[test]
    fn test_build_binding_execution_message_content_independent_of_resume_message() {
        // resume 场景下，content 仍是当前用户消息，resume_message 单独保留。
        // 防止以后误把 resume_message 当成 content 写。
        let msg = dummy_msg("继续");
        let binding = dummy_binding();
        let todo = dummy_todo();
        let pending = FeishuListener::build_binding_execution_message(
            &msg,
            "p2p",
            "继续",
            &binding,
            &todo,
            Some("real_sid".into()),
            Some("继续".into()),
        );
        assert_eq!(pending.content, "继续");
        assert_eq!(pending.resume_message.as_deref(), Some("继续"));
        assert_eq!(pending.resume_session_id.as_deref(), Some("real_sid"));
    }

    fn dummy_msg(content: &str) -> crate::feishu::message::ChannelMessage {
        crate::feishu::message::ChannelMessage {
            id: "m1".into(),
            sender: "user1".into(),
            sender_type: Some("user".into()),
            content: content.into(),
            channel: "c1".into(),
            timestamp: 0,
            chat_type: Some("p2p".into()),
            mentioned_open_ids: vec![],
        }
    }

    fn dummy_todo() -> crate::models::Todo {
        crate::models::Todo {
            id: 7,
            title: "飞书-bot".into(),
            prompt: "system prompt".into(),
            status: crate::models::TodoStatus::Pending,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            tag_ids: vec![],
            executor: Some("claudecode".into()),
            scheduler_enabled: false,
            scheduler_config: None,
            scheduler_timezone: None,
            scheduler_next_run_at: None,
            task_id: None,
            workspace: None,
            acceptance_criteria: None,
            todo_type: 0,
            parent_todo_id: None,
            review_template_id: None,
            auto_review_enabled: false,
        }
    }

    fn dummy_binding() -> FeishuProjectBinding {
        FeishuProjectBinding {
            id: 1,
            bot_id: 1,
            todo_id: 1,
            chat_id: "c1".into(),
            chat_type: "p2p".into(),
            status: "idle".into(),
            session_id: Some("s1".into()),
            latest_record_id: Some(42),
            project_dir_id: 1,
            enabled: true,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn dummy_record(status: ExecutionStatus, sid: Option<&str>) -> ExecutionRecord {
        ExecutionRecord {
            id: 42,
            todo_id: 1,
            status,
            command: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            result: None,
            started_at: String::new(),
            finished_at: None,
            usage: None,
            executor: None,
            model: None,
            trigger_type: String::new(),
            pid: None,
            task_id: None,
            session_id: sid.map(|s| s.to_string()),
            todo_progress: None,
            execution_stats: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,            rating: None,
            step_id: None,            source_execution_record_id: None,
            last_review_status: None,
            last_reviewed_at: None,
            worktree_path: None,
        }
    }
}
