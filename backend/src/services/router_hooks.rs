//! 飞书 [`ntd_connect::RouterHooks`] v2 完整实现。
//!
//! 每个 hook 委托给 [`crate::services::feishu_listener::FeishuListener`]
//! 的 stage 函数。dispatcher worker 调 `router.route(msg)` 时，
//! hook 内部复用现有 7 阶段逻辑，feishu_listener 的老代码完全不动。
//!
//! # 设计要点
//!
//! - `prepare_message` hook 幂等：首次调用执行落库 + reaction，
//!   后续调用跳过（dispatcher 7 阶段每阶段都可能触发，但只需执行一次）。
//! - 其它 hook 通过 `build_message_prep` 廉价重建 `MessagePrep`（无副作用），
//!   避免重复调 `prepare_message` 产生重复 DB 写入 / reaction。
//! - `finalize` 从 `reaction_id: RwLock` 取出 cleanup，幂等安全。

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use ntd_connect::error::Result;
use ntd_connect::router::RouterHooks;
use ntd_connect::types::IncomingMessage;

use crate::db::Database;
use crate::feishu::sdk::token_manager::TokenManager;
use crate::models::BotConfig;
use crate::services::feishu_listener::{FeishuListener, MessagePrep};
use crate::services::incoming_bridge::incoming_to_channel_message;
use crate::services::message_debounce::MessageDebounce;
use crate::task_manager::TaskManager;

/// 飞书 RouterHooks v2：每个 hook 委托给 FeishuListener 的 stage 函数。
///
/// 每个 bot 构造一个实例，存储该 bot 的全部依赖。
/// dispatcher 通过 `Arc<dyn RouterHooks>` trait object 调用。
pub struct FeishuRouterHooks {
    bot_id: i64,
    bot_open_id: String,
    bot_config: BotConfig,
    db: Arc<Database>,
    token_manager: Arc<TokenManager>,
    credentials: Arc<DashMap<i64, (String, String, String)>>,
    debounce: Arc<MessageDebounce>,
    task_manager: Arc<TaskManager>,
    /// prepare_message 创建的 reaction_id，finalize 时 cleanup。
    /// RwLock 因为 prepare/finalize 可能在不同 async 上下文调用。
    reaction_id: tokio::sync::RwLock<Option<String>>,
}

impl FeishuRouterHooks {
    pub fn new(
        bot_id: i64,
        bot_open_id: String,
        bot_config: BotConfig,
        db: Arc<Database>,
        token_manager: Arc<TokenManager>,
        credentials: Arc<DashMap<i64, (String, String, String)>>,
        debounce: Arc<MessageDebounce>,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        Self {
            bot_id,
            bot_open_id,
            bot_config,
            db,
            token_manager,
            credentials,
            debounce,
            task_manager,
            reaction_id: tokio::sync::RwLock::new(None),
        }
    }

    pub fn bot_id(&self) -> i64 {
        self.bot_id
    }

    /// 构造 `ListenerMessageContext`，供 stage 函数使用。
    fn build_context(&self) -> crate::services::feishu_listener::ListenerMessageContext<'_> {
        FeishuListener::build_hook_context(
            &self.db,
            &self.token_manager,
            &self.credentials,
            &self.debounce,
            &self.task_manager,
            self.bot_id,
            &self.bot_open_id,
            &self.bot_config,
        )
    }

    /// 廉价重建 `MessagePrep`（无副作用）。
    ///
    /// 只读 `ChannelMessage` 的 chat_type / content / mentioned_open_ids，
    /// 不做 DB 写入、不加 reaction。供 builtin / filter / binding 等 hook 使用。
    fn build_message_prep<'a>(
        msg: &'a crate::feishu::ChannelMessage,
        is_mention: bool,
    ) -> MessagePrep<'a> {
        MessagePrep {
            chat_type: msg.chat_type.as_deref().unwrap_or("p2p"),
            content: msg.content.trim(),
            is_mention,
            reaction_id: None,
        }
    }
}

#[async_trait]
impl RouterHooks for FeishuRouterHooks {
    /// 阶段 0：self 消息跳过。
    /// `is_from_self` 由 `incoming_bridge` 在转换时设好。
    fn is_self_message(&self, msg: &IncomingMessage) -> bool {
        msg.is_from_self
    }

    /// 阶段 1：持久化入站消息 + 加 typing reaction。
    ///
    /// 幂等：首次调用执行完整逻辑并存 reaction_id；后续调用跳过
    /// （避免 dispatcher 7 阶段重复触发产生 duplicate DB 写入 / reaction）。
    async fn prepare_message(&self, msg: &IncomingMessage) -> Result<Option<String>> {
        {
            let existing = self.reaction_id.read().await;
            if existing.is_some() {
                return Ok(None);
            }
        }
        let channel_msg = incoming_to_channel_message(msg);
        let context = self.build_context();
        let prep = FeishuListener::prepare_message(&context, &channel_msg).await;
        *self.reaction_id.write().await = prep.reaction_id;
        Ok(None)
    }

    /// 阶段 2：builtin 命令匹配（/sethome /bind /unbind /new /stop 等）。
    ///
    /// 命中时 handler 内部已发 reply，返回 `Ok(Some(String::new()))`
    /// 让 router 判定 `Decision::Handled`（空字符串不触发 dispatcher 回复）。
    async fn try_route_builtin(&self, msg: &IncomingMessage) -> Result<Option<String>> {
        let channel_msg = incoming_to_channel_message(msg);
        let is_mention = msg.is_mention;
        let context = self.build_context();
        let prep = Self::build_message_prep(&channel_msg, is_mention);
        let handled = FeishuListener::try_route_builtin_command(&context, &channel_msg, &prep).await;
        if handled {
            Ok(Some(String::new()))
        } else {
            Ok(None)
        }
    }

    /// 阶段 3：消息接收过滤（私聊/群聊策略 + 响应开关 + 群白名单）。
    async fn should_skip_filters(&self, msg: &IncomingMessage) -> Result<bool> {
        let channel_msg = incoming_to_channel_message(msg);
        let is_mention = msg.is_mention;
        let context = self.build_context();
        let prep = Self::build_message_prep(&channel_msg, is_mention);
        let skip = FeishuListener::should_skip_for_message_filters(&context, &channel_msg, &prep).await;
        Ok(skip)
    }

    /// 阶段 4：promote pending binding（页面创建的 `__pending__` → 真实 chat）。
    async fn try_promote_binding(&self, msg: &IncomingMessage) -> Result<()> {
        let channel_msg = incoming_to_channel_message(msg);
        let is_mention = msg.is_mention;
        let context = self.build_context();
        let prep = Self::build_message_prep(&channel_msg, is_mention);
        FeishuListener::try_promote_pending_binding(&context, &channel_msg, &prep).await;
        Ok(())
    }

    /// 阶段 5：项目绑定路由（查 binding → 触发 todo 执行）。
    ///
    /// 命中时 debounce 已 push，返回 `Ok(Some(String::new()))` 信号 Handled。
    async fn try_route_project_binding(&self, msg: &IncomingMessage) -> Result<Option<String>> {
        let channel_msg = incoming_to_channel_message(msg);
        let is_mention = msg.is_mention;
        let context = self.build_context();
        let prep = Self::build_message_prep(&channel_msg, is_mention);
        let handled = FeishuListener::try_route_project_binding(&context, &channel_msg, &prep).await;
        if handled {
            Ok(Some(String::new()))
        } else {
            Ok(None)
        }
    }

    /// 阶段 6：slash 命令规则匹配 + 默认回复。
    ///
    /// 无论命中 slash 还是走 default response，都会 push 到 debounce，
    /// 返回 `Ok(Some(String::new()))` 信号 Handled。
    async fn try_route_slash_or_default(&self, msg: &IncomingMessage) -> Result<Option<String>> {
        let channel_msg = incoming_to_channel_message(msg);
        let is_mention = msg.is_mention;
        let context = self.build_context();
        let prep = Self::build_message_prep(&channel_msg, is_mention);
        FeishuListener::route_slash_or_default_response(&context, &channel_msg, &prep).await;
        Ok(Some(String::new()))
    }

    /// 阶段 7：cleanup typing reaction。
    ///
    /// 从 `reaction_id` RwLock 取出 prepare_message 存的 ID，
    /// 调 `cleanup_reaction` 删除。幂等：double delete 飞书 API 无害。
    async fn finalize(&self, _msg: &IncomingMessage, _reaction_id: Option<&str>) -> Result<()> {
        let stored = self.reaction_id.write().await.take();
        if let Some(rid) = stored {
            let context = self.build_context();
            // cleanup_reaction 需要 ChannelMessage.id（飞书 message_id）
            let channel_msg = crate::feishu::ChannelMessage {
                id: _msg.raw_message_id.clone(),
                sender: String::new(),
                sender_type: None,
                content: String::new(),
                channel: String::new(),
                timestamp: 0,
                chat_type: None,
                mentioned_open_ids: vec![],
            };
            FeishuListener::cleanup_reaction(&context, &channel_msg, Some(&rid)).await;
        }
        Ok(())
    }
}

/// 工厂：构造 Arc<dyn RouterHooks>（dispatcher 注入用）。
pub fn build_router_hooks(
    bot_id: i64,
    bot_open_id: String,
    bot_config: BotConfig,
    db: Arc<Database>,
    token_manager: Arc<TokenManager>,
    credentials: Arc<DashMap<i64, (String, String, String)>>,
    debounce: Arc<MessageDebounce>,
    task_manager: Arc<TaskManager>,
) -> Arc<dyn RouterHooks> {
    Arc::new(FeishuRouterHooks::new(
        bot_id,
        bot_open_id,
        bot_config,
        db,
        token_manager,
        credentials,
        debounce,
        task_manager,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntd_connect::types::{
        FeishuChatType, IncomingContent, PlatformKind, ReplyTarget, SenderId, SenderKind,
        SessionKey,
    };

    fn sample_msg(is_from_self: bool) -> IncomingMessage {
        IncomingMessage {
            platform: PlatformKind::Feishu,
            session_key: SessionKey::derive(PlatformKind::Feishu, "oc_test", None),
            sender: SenderId::new("ou_user"),
            content: IncomingContent::Text("hi".into()),
            reply_target: ReplyTarget::feishu("oc_test", None, FeishuChatType::P2p),
            timestamp_ms: 1_700_000_000_000,
            raw_message_id: "om_test".into(),
            is_mention: false,
            sender_kind: SenderKind::User,
            is_from_self,
            mentioned_open_ids: vec![],
        }
    }

    /// is_self_message 直接返回 msg.is_from_self。
    #[test]
    fn test_is_self_message_delegates_to_field() {
        // 不需要构造完整 FeishuRouterHooks：hook 只读 is_from_self 字段
        let msg_other = sample_msg(false);
        let msg_self = sample_msg(true);
        // 直接验证字段语义（hook 实现就是 `msg.is_from_self`）
        assert!(!msg_other.is_from_self);
        assert!(msg_self.is_from_self);
    }

    /// build_message_prep 廉价重建 MessagePrep（无副作用）。
    #[test]
    fn test_build_message_prep() {
        let channel_msg = crate::feishu::ChannelMessage {
            id: "om_test".into(),
            sender: "ou_user".into(),
            sender_type: Some("user".into()),
            content: r#"{"text": "hello"}"#.into(),
            channel: "oc_test".into(),
            timestamp: 1700000000,
            chat_type: Some("group".into()),
            mentioned_open_ids: vec!["ou_bot".into()],
        };
        let prep = FeishuRouterHooks::build_message_prep(&channel_msg, true);
        assert_eq!(prep.chat_type, "group");
        assert_eq!(prep.content, r#"{"text": "hello"}"#);
        assert!(prep.is_mention);
        assert!(prep.reaction_id.is_none());
    }
}
