//! Router trait：把 `IncomingMessage` 路由到正确的业务处理路径。
//!
//! # 与 cc-connect 的对应
//!
//! 对应 `cc-connect/core/engine.go:2669-2947 handle_message` 的 7 阶段
//! 编排（`prepare_message` / `try_route_builtin_command` /
//! `should_skip_for_message_filters` / `try_promote_pending_binding` /
//! `try_route_project_binding` / `route_slash_or_default_response`）。
//!
//! # 设计
//!
//! - `Router` trait：决策入口（`route(msg) -> Decision`）
//! - `RouterHooks` trait：业务侧副作用（DB 读、debounce push、消息持久化等）
//!   的抽象。backend 实现 RouterHooks（参见 `backend/src/services/router_hooks.rs`），
//!   通过 trait 注入到 `MessageRouter`。这样 ntd-connect 不反向依赖 backend 类型。
//!
//! # 决策树（v2 完整版）
//!
//! ```text
//! on_message(msg)
//! ├─ is_from_self → Skip
//! ├─ hooks.should_skip_filters() → true → Skip
//! ├─ hooks.try_route_builtin() → Some(text) → Handled（reply text）
//! ├─ hooks.try_route_slash() → Some(text) → Handled（reply text）
//! └─ ForwardToAgent（dispatcher worker 调 agent.send + events）
//! ```

use std::sync::Arc;

use async_trait::async_trait;

use crate::channel::Channel;
use crate::error::Result;
use crate::types::IncomingMessage;

/// 路由结果：dispatcher 根据这个决定 worker 下一步动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// 跳过：消息不该被处理（self / disabled / 群白名单未命中）。
    Skip,
    /// 已处理：router 自己处理完了（builtin 命令 / slash 命令）。
    Handled,
    /// 转给 Agent 执行：默认回复 / 项目绑定触发 todo 等。
    ForwardToAgent,
}

/// Router trait：把消息路由到正确处理路径。
///
/// backend 实现这个 trait（参见 `backend::services::message_router::MessageRouter`）。
#[async_trait]
pub trait Router: Send + Sync {
    /// 路由一条入站消息。返回 [`Decision`]。
    async fn route(&self, msg: IncomingMessage) -> Decision;
}

/// RouterHooks：业务侧副作用的抽象。
///
/// MessageRouter 在编排 7 阶段决策时调用 hooks 做：
/// - 持久化入站消息（DB 写）
/// - 查 bot 配置 / 项目绑定（DB 读）
/// - 触发 debounce push
/// - builtin 命令处理（/sethome /bind 等）
/// - slash 命令规则匹配
///
/// 这些是 backend 关心的事，ntd-connect 不应该知道细节；通过 trait
/// 解耦。v2 backend 实现 `RouterHooks` 的 FeishuRouterHooks 子 trait
/// 把每个 hook 接到现有的 `feishu_listener` / `message_debounce` 上。
///
/// 错误处理：hook 返回 `Err` 时，MessageRouter 优雅降级（warn log +
/// 继续下一阶段），不 panic。
#[async_trait]
pub trait RouterHooks: Send + Sync {
    /// 阶段 0：跳过机器人自己发的消息。
    fn is_self_message(&self, msg: &IncomingMessage) -> bool;

    /// 阶段 1：持久化入站消息 + 加 typing reaction。
    /// 返回 reaction_id 用于后续 cleanup；None 表示反应添加失败但消息已持久化。
    async fn prepare_message(&self, msg: &IncomingMessage) -> Result<Option<String>>;

    /// 阶段 2：builtin 命令路由（/sethome /feishupush /list /bind /unbind /new /stop）。
    async fn try_route_builtin(&self, msg: &IncomingMessage) -> Result<Option<String>>;

    /// 阶段 3：消息接收过滤（dm_enabled / group_enabled / 群白名单）。
    async fn should_skip_filters(&self, msg: &IncomingMessage) -> Result<bool>;

    /// 阶段 4：promote pending binding（项目绑定 lifecycle）。
    async fn try_promote_binding(&self, msg: &IncomingMessage) -> Result<()>;

    /// 阶段 5：项目绑定路由（已绑定项目的 chat 直接触发 todo）。
    async fn try_route_project_binding(&self, msg: &IncomingMessage) -> Result<Option<String>>;

    /// 阶段 6：slash 命令规则匹配 + 默认回复。
    async fn try_route_slash_or_default(&self, msg: &IncomingMessage) -> Result<Option<String>>;

    /// 阶段 7：cleanup typing reaction + echo log。
    async fn finalize(&self, msg: &IncomingMessage, reaction_id: Option<&str>) -> Result<()>;
}

/// MessageRouter v2 完整版：编排 7 阶段决策。
///
/// 持有：
/// - `hooks`: backend 实现的 RouterHooks（DB / debounce 等）
/// - `channel`: 用于 reply Skipped/Handled 的回复文本
pub struct MessageRouter {
    hooks: Arc<dyn RouterHooks>,
    channel: Arc<dyn Channel>,
}

impl MessageRouter {
    /// 用 RouterHooks + Channel 构造 MessageRouter。
    ///
    /// 通常 backend 在 `build_app_state` 里 `Arc::new(FeishuRouterHooks::new(...))`
    /// 后传入；dispatcher 通过 `Arc<dyn RouterHooks>` trait object 调用。
    pub fn new(hooks: Arc<dyn RouterHooks>, channel: Arc<dyn Channel>) -> Self {
        MessageRouter { hooks, channel }
    }
}

#[async_trait]
impl Router for MessageRouter {
    async fn route(&self, msg: IncomingMessage) -> Decision {
        // 阶段 0：self 消息跳过
        if self.hooks.is_self_message(&msg) {
            tracing::debug!("[router] skip self message: {}", msg.session_key.as_str());
            return Decision::Skip;
        }

        // 阶段 1：持久化 + 加 typing reaction
        let reaction_id = match self.hooks.prepare_message(&msg).await {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!("[router] prepare_message failed (continue): {e}");
                None
            }
        };

        // 阶段 2：builtin 命令
        match self.hooks.try_route_builtin(&msg).await {
            Ok(Some(text)) => {
                // 空字符串表示 handler 已内部回复，dispatcher 无需再发
                if !text.is_empty() {
                    self.reply(&msg, &text).await;
                }
                let _ = self.hooks.finalize(&msg, reaction_id.as_deref()).await;
                return Decision::Handled;
            }
            Ok(None) => {} // 不是 builtin，继续
            Err(e) => {
                tracing::warn!("[router] try_route_builtin failed (continue): {e}");
            }
        }

        // 阶段 3：消息接收过滤
        match self.hooks.should_skip_filters(&msg).await {
            Ok(true) => {
                tracing::debug!("[router] filtered out: {}", msg.session_key.as_str());
                let _ = self.hooks.finalize(&msg, reaction_id.as_deref()).await;
                return Decision::Skip;
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!("[router] should_skip_filters failed (continue): {e}");
            }
        }

        // 阶段 4：promote pending binding
        if let Err(e) = self.hooks.try_promote_binding(&msg).await {
            tracing::warn!("[router] try_promote_binding failed (continue): {e}");
        }

        // 阶段 5：项目绑定路由
        match self.hooks.try_route_project_binding(&msg).await {
            Ok(Some(text)) => {
                if !text.is_empty() {
                    self.reply(&msg, &text).await;
                }
                let _ = self.hooks.finalize(&msg, reaction_id.as_deref()).await;
                return Decision::Handled;
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("[router] try_route_project_binding failed (continue): {e}");
            }
        }

        // 阶段 6：slash 命令 / 默认回复
        match self.hooks.try_route_slash_or_default(&msg).await {
            Ok(Some(text)) => {
                if !text.is_empty() {
                    self.reply(&msg, &text).await;
                }
                let _ = self.hooks.finalize(&msg, reaction_id.as_deref()).await;
                return Decision::Handled;
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("[router] try_route_slash_or_default failed (continue): {e}");
            }
        }

        // 阶段 7：cleanup（最终兜底）
        let _ = self.hooks.finalize(&msg, reaction_id.as_deref()).await;

        // 没命中任何路由：转给 Agent
        Decision::ForwardToAgent
    }
}

impl MessageRouter {
    /// 通过 channel 回复文本（Handled 路径使用）。
    async fn reply(&self, msg: &IncomingMessage, text: &str) {
        if let Err(e) = self
            .channel
            .reply(
                &crate::types::ReplyContext::default(),
                msg.reply_target.clone(),
                crate::types::OutgoingContent::Text(text.to_string()),
            )
            .await
        {
            tracing::warn!("[router] reply failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::MessageHandler;
    use crate::error::Result;
    use crate::types::{FeishuChatType, PlatformKind, SenderId, SenderKind, SessionKey};
    use async_trait::async_trait;
    use parking_lot::Mutex;
    use std::sync::Arc;

    fn sample_msg(ts: i64, key: &str) -> IncomingMessage {
        IncomingMessage {
            platform: PlatformKind::Feishu,
            session_key: SessionKey::derive(PlatformKind::Feishu, key, None),
            sender: SenderId::new("ou_user"),
            content: crate::types::IncomingContent::Text("hi".into()),
            reply_target: crate::types::ReplyTarget::feishu(key, None, FeishuChatType::P2p),
            timestamp_ms: ts,
            raw_message_id: format!("om_{ts}"),
            is_mention: false,
            sender_kind: SenderKind::User,
            is_from_self: false,
            mentioned_open_ids: vec![],
        }
    }

    /// Mock RouterHooks：可定制各阶段返回值（用于测试 7 阶段决策树）。
    ///
    /// 每个 hook 的返回值用 `Arc<Result<T>>` 包装（Arc 让整体 Clone）：
    /// - `Arc::new(Ok(v))` 表示正常返回 v
    /// - `Arc::new(Err(e))` 表示 hook 报错（测降级路径）
    ///
    /// `Mutex<Option<...>>` 让测试在构造后覆盖 hook 返回值。
    struct MockHooks {
        is_self: bool,
        prepare: Mutex<Option<Arc<Result<Option<String>>>>>,
        builtin: Mutex<Option<Arc<Result<Option<String>>>>>,
        filter_skip: Mutex<Option<Arc<Result<bool>>>>,
        promote: Mutex<Option<Arc<Result<()>>>>,
        binding: Mutex<Option<Arc<Result<Option<String>>>>>,
        slash: Mutex<Option<Arc<Result<Option<String>>>>>,
    }

    impl MockHooks {
        /// 构造所有 hook 都返回 Ok(default) 的 mock。
        fn ok_all() -> Self {
            MockHooks {
                is_self: false,
                prepare: Mutex::new(Some(Arc::new(Ok(None)))),
                builtin: Mutex::new(Some(Arc::new(Ok(None)))),
                filter_skip: Mutex::new(Some(Arc::new(Ok(false)))),
                promote: Mutex::new(Some(Arc::new(Ok(())))),
                binding: Mutex::new(Some(Arc::new(Ok(None)))),
                slash: Mutex::new(Some(Arc::new(Ok(None)))),
            }
        }
        /// 覆盖某个 hook 的返回值。
        fn set_ok<T: Send + Sync + 'static, F>(cell: &Mutex<Option<Arc<Result<T>>>>, value: F)
        where
            F: FnOnce() -> T,
        {
            *cell.lock() = Some(Arc::new(Ok(value())));
        }
        /// 覆盖某个 hook 为 Err。
        fn set_err<T: Send + Sync + 'static>(cell: &Mutex<Option<Arc<Result<T>>>>, msg: &str) {
            *cell.lock() = Some(Arc::new(Err(crate::error::Error::other(msg.to_string()))));
        }
    }

    /// 把 Mutex<Option<Arc<Result<T>>>> 解包成 Result<T>。
    /// - None → 未配置错误
    /// - Some(Ok(v)) → Ok(v)
    /// - Some(Err(e)) → Err(clone(e))
    fn resolve_hook<T: Clone + Send + Sync + 'static>(
        cell: &Mutex<Option<Arc<Result<T>>>>,
        name: &str,
    ) -> Result<T> {
        cell.lock()
            .as_ref()
            .map(|arc| match arc.as_ref() {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(clone_error(e)),
            })
            .unwrap_or_else(|| Err(crate::error::Error::other(format!("{name} not configured"))))
    }

    #[async_trait]
    impl RouterHooks for MockHooks {
        fn is_self_message(&self, _msg: &IncomingMessage) -> bool {
            self.is_self
        }
        async fn prepare_message(
            &self,
            _msg: &IncomingMessage,
        ) -> Result<Option<String>> {
            resolve_hook(&self.prepare, "prepare")
        }
        async fn try_route_builtin(
            &self,
            _msg: &IncomingMessage,
        ) -> Result<Option<String>> {
            resolve_hook(&self.builtin, "builtin")
        }
        async fn should_skip_filters(&self, _msg: &IncomingMessage) -> Result<bool> {
            resolve_hook(&self.filter_skip, "filter_skip")
        }
        async fn try_promote_binding(&self, _msg: &IncomingMessage) -> Result<()> {
            resolve_hook(&self.promote, "promote")
        }
        async fn try_route_project_binding(
            &self,
            _msg: &IncomingMessage,
        ) -> Result<Option<String>> {
            resolve_hook(&self.binding, "binding")
        }
        async fn try_route_slash_or_default(
            &self,
            _msg: &IncomingMessage,
        ) -> Result<Option<String>> {
            resolve_hook(&self.slash, "slash")
        }
        async fn finalize(
            &self,
            _msg: &IncomingMessage,
            _reaction_id: Option<&str>,
        ) -> Result<()> {
            Ok(())
        }
    }

    /// Error clone helper：`crate::error::Error` 当前不实现 Clone，
    /// 测试场景下我们需要把 Mutex 里存的 Err 复制一份返回。
    /// v2：给 Error 加 derive Clone（需权衡 PartialEq 派生）。
    fn clone_error(e: &crate::error::Error) -> crate::error::Error {
        crate::error::Error::other(format!("{e}"))
    }

    /// Mock channel：记录 reply 调用。
    #[derive(Default)]
    struct MockChannel {
        replies: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl Channel for MockChannel {
        fn name(&self) -> &'static str {
            "mock"
        }
        async fn start(&self, _: Arc<dyn MessageHandler>) -> Result<()> {
            Ok(())
        }
        async fn reply(
            &self,
            _: &crate::types::ReplyContext,
            _: crate::types::ReplyTarget,
            c: crate::types::OutgoingContent,
        ) -> Result<()> {
            if let crate::types::OutgoingContent::Text(t) = c {
                self.replies.lock().push(t);
            }
            Ok(())
        }
        async fn send(
            &self,
            _: &crate::types::ReplyContext,
            _: crate::types::ReplyTarget,
            _: crate::types::OutgoingContent,
        ) -> Result<()> {
            Ok(())
        }
        async fn stop(&self) -> Result<()> {
            Ok(())
        }
    }

    /// 场景 1：self 消息 → Skip，不调任何 hook（除 is_self_message）。
    #[tokio::test]
    async fn test_route_self_message_skips() {
        let hooks = Arc::new(MockHooks {
            is_self: true,
            ..MockHooks::ok_all()
        });
        let ch = Arc::new(MockChannel::default());
        let router = MessageRouter::new(hooks, ch.clone());
        let decision = router.route(sample_msg(1000, "oc_x")).await;
        assert_eq!(decision, Decision::Skip);
        assert_eq!(ch.replies.lock().len(), 0);
    }

    /// 场景 2：builtin 命令命中 → Handled + reply。
    #[tokio::test]
    async fn test_route_builtin_handled() {
        let hooks = Arc::new(MockHooks {
            is_self: false,
            ..MockHooks::ok_all()
        });
        MockHooks::set_ok(&hooks.builtin, || Some("✅ 已设置".to_string()));
        let ch = Arc::new(MockChannel::default());
        let router = MessageRouter::new(hooks, ch.clone());
        let decision = router.route(sample_msg(1000, "oc_x")).await;
        assert_eq!(decision, Decision::Handled);
        assert_eq!(ch.replies.lock().len(), 1);
        assert_eq!(ch.replies.lock()[0], "✅ 已设置");
    }

    /// 场景 3：过滤器跳过 → Skip。
    #[tokio::test]
    async fn test_route_filter_skips() {
        let hooks = Arc::new(MockHooks {
            is_self: false,
            ..MockHooks::ok_all()
        });
        MockHooks::set_ok(&hooks.filter_skip, || true);
        let ch = Arc::new(MockChannel::default());
        let router = MessageRouter::new(hooks, ch.clone());
        let decision = router.route(sample_msg(1000, "oc_x")).await;
        assert_eq!(decision, Decision::Skip);
    }

    /// 场景 4：项目绑定命中 → Handled + reply。
    #[tokio::test]
    async fn test_route_binding_handled() {
        let hooks = Arc::new(MockHooks {
            is_self: false,
            ..MockHooks::ok_all()
        });
        MockHooks::set_ok(&hooks.binding, || Some("▶️ 已触发 todo".to_string()));
        let ch = Arc::new(MockChannel::default());
        let router = MessageRouter::new(hooks, ch.clone());
        let decision = router.route(sample_msg(1000, "oc_x")).await;
        assert_eq!(decision, Decision::Handled);
        assert_eq!(ch.replies.lock().len(), 1);
    }

    /// 场景 5：slash 命令命中 → Handled + reply。
    #[tokio::test]
    async fn test_route_slash_handled() {
        let hooks = Arc::new(MockHooks {
            is_self: false,
            ..MockHooks::ok_all()
        });
        MockHooks::set_ok(&hooks.slash, || Some("✅ 任务已创建".to_string()));
        let ch = Arc::new(MockChannel::default());
        let router = MessageRouter::new(hooks, ch.clone());
        let decision = router.route(sample_msg(1000, "oc_x")).await;
        assert_eq!(decision, Decision::Handled);
    }

    /// 场景 6：什么都没命中 → ForwardToAgent（dispatcher 后续调 agent）。
    #[tokio::test]
    async fn test_route_default_to_agent() {
        let hooks = Arc::new(MockHooks::ok_all());
        let ch = Arc::new(MockChannel::default());
        let router = MessageRouter::new(hooks, ch.clone());
        let decision = router.route(sample_msg(1000, "oc_x")).await;
        assert_eq!(decision, Decision::ForwardToAgent);
        // ForwardToAgent 路径不调 reply。
        assert_eq!(ch.replies.lock().len(), 0);
    }

    /// 场景 7：hook 出错 → 优雅降级到下一阶段，不 panic。
    #[tokio::test]
    async fn test_route_hook_errors_degrade_gracefully() {
        let hooks = Arc::new(MockHooks {
            is_self: false,
            ..MockHooks::ok_all()
        });
        // 所有 hook 都返回 Err
        MockHooks::set_err(&hooks.builtin, "builtin failed");
        MockHooks::set_err(&hooks.promote, "promote failed");
        MockHooks::set_err(&hooks.binding, "binding failed");
        MockHooks::set_err(&hooks.slash, "slash failed");
        let ch = Arc::new(MockChannel::default());
        let router = MessageRouter::new(hooks, ch.clone());
        // hook 全报错的 fallback：ForwardToAgent（dispatcher 兜底）
        let decision = router.route(sample_msg(1000, "oc_x")).await;
        assert_eq!(decision, Decision::ForwardToAgent);
    }
}
