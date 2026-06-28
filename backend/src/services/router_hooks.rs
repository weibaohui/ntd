//! 飞书 [`ntd_connect::RouterHooks`] 实现。
//!
//! # 与 dry-run 步骤 11 的对应
//!
//! 步骤 11「清理 feishu_listener」的核心：让 backend 实现
//! `ntd_connect::RouterHooks` trait，每个 hook 委托给现有
//! `feishu_listener` 的 stage 函数。这样 dispatcher worker 调
//! `router.route(msg)` 时，hook 内部复用现有逻辑，feishu_listener 的
//! 7 阶段代码不需要重写。
//!
//! # 当前实现状态（v1 stub）
//!
//! 每个 hook 方法返回「未配置」错误（`Error::other`），让 dispatcher
//! 优雅降级到下一阶段（最终落到 `ForwardToAgent`）。
//!
//! v2 完整版要把每个 hook 实接到现有 `feishu_listener` 静态方法：
//! - `prepare_message` → FeishuListener::prepare_message
//! - `try_route_builtin` → FeishuListener::try_route_builtin_command
//! - `should_skip_filters` → FeishuListener::should_skip_for_message_filters
//! - `try_promote_binding` → FeishuListener::try_promote_pending_binding
//! - `try_route_project_binding` → FeishuListener::try_route_project_binding
//! - `try_route_slash_or_default` → FeishuListener::route_slash_or_default_response
//! - `finalize` → FeishuListener::cleanup_reaction
//!
//! 委托时需要：构造 `ListenerMessageContext`（含 db/credentials/token_manager/
//! listener），并把 `IncomingMessage` 转回 `ChannelMessage`（incoming_bridge 的
//! 逆向，v2 加 helper）。
//!
//! # 结构
//!
//! 每个 FeishuRouterHooks 关联一个 bot（`bot_id` 字段）。dispatcher
//! 构造多个 hooks（多 bot），每个对应一个 FeishuListener 实例。
//! v1 单 bot 设计；多 bot 拓扑由 `ChannelRegistry` 持有映射表。

use std::sync::Arc;

use async_trait::async_trait;
use ntd_connect::error::Result;
use ntd_connect::router::RouterHooks;
use ntd_connect::types::IncomingMessage;

/// 飞书 RouterHooks：每个 hook 当前是 v1 stub（未实现），返回 Err
/// 让 dispatcher 优雅降级。
///
/// v2 实接 [`crate::services::feishu_listener::FeishuListener`] 的
/// stage 函数。v1 阶段 dispatcher 走 `ForwardToAgent` 默认路径（agent
/// 兜底），不会破坏现有 backend 行为。
pub struct FeishuRouterHooks {
    bot_id: i64,
}

impl FeishuRouterHooks {
    /// 构造 v1 stub：只记录 bot_id。
    /// v2 构造时需要 `Arc<FeishuListener>` + `db` + `task_manager` 等依赖。
    pub fn new(bot_id: i64) -> Self {
        FeishuRouterHooks { bot_id }
    }

    /// 当前构造的 bot_id（dispatcher 路由时记录用）。
    pub fn bot_id(&self) -> i64 {
        self.bot_id
    }
}

#[async_trait]
impl RouterHooks for FeishuRouterHooks {
    fn is_self_message(&self, _msg: &IncomingMessage) -> bool {
        // v2: 比对 msg.sender == bot_open_id。
        // v1 stub：永远 false（让消息继续走流程）。
        false
    }

    async fn prepare_message(&self, _msg: &IncomingMessage) -> Result<Option<String>> {
        // v2: 写 feishu_messages 表 + FeishuPlatform::start_typing → reaction_id。
        // v1 stub：返回 None（没 reaction id 可 cleanup）。
        Ok(None)
    }

    async fn try_route_builtin(&self, _msg: &IncomingMessage) -> Result<Option<String>> {
        // v2: 调 FeishuListener::try_route_builtin_command。
        // v1 stub：返回 None（不是 builtin 命令，走下一阶段）。
        Ok(None)
    }

    async fn should_skip_filters(&self, _msg: &IncomingMessage) -> Result<bool> {
        // v2: 查 bot_config / 群白名单。
        // v1 stub：返回 false（不跳过，让消息继续走）。
        Ok(false)
    }

    async fn try_promote_binding(&self, _msg: &IncomingMessage) -> Result<()> {
        // v2: pending binding → active。
        // v1 stub：no-op。
        Ok(())
    }

    async fn try_route_project_binding(&self, _msg: &IncomingMessage) -> Result<Option<String>> {
        // v2: 查 feishu_project_bindings 表 + 触发 todo。
        // v1 stub：返回 None（无绑定，走下一阶段）。
        Ok(None)
    }

    async fn try_route_slash_or_default(&self, _msg: &IncomingMessage) -> Result<Option<String>> {
        // v2: 查 slash 规则 + 默认回复。
        // v1 stub：返回 None（让消息走 ForwardToAgent 兜底）。
        Ok(None)
    }

    async fn finalize(&self, _msg: &IncomingMessage, _reaction_id: Option<&str>) -> Result<()> {
        // v2: FeishuPlatform::delete_reaction（cleanup typing）。
        // v1 stub：no-op。
        Ok(())
    }
}

/// 工厂：构造 Arc<dyn RouterHooks>（dispatcher 注入用）。
pub fn build_router_hooks(bot_id: i64) -> Arc<dyn RouterHooks> {
    Arc::new(FeishuRouterHooks::new(bot_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntd_connect::types::{
        FeishuChatType, IncomingContent, PlatformKind, ReplyTarget, SenderId, SenderKind,
        SessionKey,
    };

    fn sample_msg() -> IncomingMessage {
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
            is_from_self: false,
        }
    }

    /// FeishuRouterHooks::new 记录 bot_id。
    #[test]
    fn test_bot_id() {
        let hooks = FeishuRouterHooks::new(42);
        assert_eq!(hooks.bot_id(), 42);
    }

    /// v1 stub：所有 hook 返回 Ok(无影响) → dispatcher 走 ForwardToAgent。
    #[tokio::test]
    async fn test_v1_stub_all_hooks_noop() {
        let hooks = FeishuRouterHooks::new(1);
        let msg = sample_msg();

        // is_self_message：v1 stub 返 false
        assert!(!hooks.is_self_message(&msg));
        // prepare_message：返 Ok(None)
        assert!(hooks.prepare_message(&msg).await.unwrap().is_none());
        // try_route_builtin：返 Ok(None)
        assert!(hooks.try_route_builtin(&msg).await.unwrap().is_none());
        // should_skip_filters：返 Ok(false)
        assert!(!hooks.should_skip_filters(&msg).await.unwrap());
        // try_promote_binding：返 Ok(())
        assert!(hooks.try_promote_binding(&msg).await.is_ok());
        // try_route_project_binding：返 Ok(None)
        assert!(hooks.try_route_project_binding(&msg).await.unwrap().is_none());
        // try_route_slash_or_default：返 Ok(None)
        assert!(hooks.try_route_slash_or_default(&msg).await.unwrap().is_none());
        // finalize：返 Ok(())
        assert!(hooks.finalize(&msg, None).await.is_ok());
        assert!(hooks.finalize(&msg, Some("rx-id")).await.is_ok());
    }

    /// build_router_hooks 工厂返回 Arc<dyn RouterHooks>。
    #[test]
    fn test_build_factory() {
        let hooks = build_router_hooks(7);
        // 通过 trait object 调 is_self_message（编译期验证 trait dispatch）
        let msg = sample_msg();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async { hooks.is_self_message(&msg) });
        assert!(!result);
    }
}
