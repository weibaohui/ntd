//! Channel trait：抽象一个 IM 平台（飞书/钉钉/微信/TG/Slack）。
//!
//! # 与 cc-connect 的对应
//!
//! 对应 `cc-connect/core/interfaces.go:10-16 Platform` 与
//! `core/interfaces.go:379 MessageHandler`。
//!
//! # 设计要点
//!
//! - `Channel` 与 `MessageHandler` 都是 `async_trait`：M1.5 阶段 Rust
//!   还不原生支持 trait 里的 async fn（直到 edition 2024）。
//! - `handler: Arc<dyn MessageHandler>`：dispatcher 通常被多个 channel
//!   共享，handler 也要能跨 task 传递，Arc 是最少惊讶的方案。
//! - `start()` 是 async 因为底层 WS 握手常常是异步的（飞书需要
//!   获取 tenant_access_token 后才能建连）。`stop()` 同样 async。
//! - `reply` / `send` 接受 `&ReplyContext`：上层可以传超时 / trace。
//! - `Channel` 不直接调 `Agent`，所有消息都走 `MessageHandler` 派发；
//!   channel 与 agent 解耦是 dispatcher 设计的核心约束。

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;
use crate::types::{IncomingMessage, OutgoingContent, ReplyContext, ReplyTarget};

/// IM 平台抽象。
///
/// `Send + Sync` 是约束：handler 可能跨 task 持有 channel，必须能在
/// 多线程环境传递。`'static` 由 trait 默认约束保证（trait object 默认
/// 隐含 `'static` lifetime）。
#[async_trait]
pub trait Channel: Send + Sync {
    /// 平台名（用于日志 / metrics 区分）。
    /// 返回 `'static str` 而不是 `String` 避免每次调用都分配。
    fn name(&self) -> &'static str;

    /// 启动长连接，注册入站消息 handler。
    ///
    /// 调用完成后 channel 进入「收消息中」状态；handler 可能在 await
    /// 中持续被调用。返回 `Err` 通常表示启动失败（token 错误 / 网络不可达），
    /// 上层应决定是否重试或停机。
    async fn start(&self, handler: Arc<dyn MessageHandler>) -> Result<()>;

    /// 回复到某个具体消息（threaded reply）。
    ///
    /// `target.message_id` 通常存在；首次 reply 时填，后续 edit 可空。
    async fn reply(
        &self,
        ctx: &ReplyContext,
        target: ReplyTarget,
        content: OutgoingContent,
    ) -> Result<()>;

    /// 主动发一条新消息（非 reply）。
    ///
    /// 与 `reply` 的区别：不绑定到某条具体消息；用于主动推送 /
    /// 异步通知。
    async fn send(
        &self,
        ctx: &ReplyContext,
        target: ReplyTarget,
        content: OutgoingContent,
    ) -> Result<()>;

    /// 关闭长连接。
    ///
    /// 调用后 channel 不再发消息；handler 也不会再被调用。上层在
    /// shutdown / 配置变更时调用。
    async fn stop(&self) -> Result<()>;

    /// 「处理中」typing 反应能力探测。
    ///
    /// 默认返回 `None`，表示 channel 不支持 typing（如没有 typing API 的平台）。
    /// 飞书等支持 reaction 作为 typing 指示的平台应 override 返回
    /// `Some(self)`（前提是 self 也实现了 [`crate::typing::TypingIndicator`]）。
    ///
    /// 这是 cc-connect Go 的「optional interface assertion」模式：
    /// `if p, ok := p.(TypingIndicator); ok` 的 Rust 版。
    /// default impl 让不支持的平台不用写 boilerplate。
    fn as_typing_indicator(&self) -> Option<&dyn crate::typing::TypingIndicator> {
        None
    }
}

/// Dispatcher 实现此 trait 接收 channel 派发的入站消息。
///
/// channel 与 dispatcher 之间是 N:1：多个 channel（飞书 + 钉钉 + ...）
/// 都把消息送到同一个 dispatcher。dispatcher 内部按 session 分发。
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// 处理一条入站消息。
    ///
    /// 返回 `Err` 不应该 panic channel；dispatcher 应当自己记日志、
    /// 转换错误类型、并继续接收后续消息。Channel 实现通常对返回值
    /// 不敏感（飞书等平台收到 `Err` 也不会有特别反应）。
    async fn on_message(
        &self,
        channel: Arc<dyn Channel>,
        msg: IncomingMessage,
    ) -> Result<()>;
}

#[cfg(test)]
pub mod tests {
    //! Channel / MessageHandler 的单元测试 + mock 实现。
    use super::*;
    use crate::types::{FeishuChatType, IncomingContent, PlatformKind, SenderId, SessionKey};
    use parking_lot::Mutex;

    /// MockChannel：用 `Mutex<Vec<Call>>` 记录收到的调用，作为 mock 实现。
    ///
    /// 设计要点：测试全程直接持有 `Arc<MockChannel>`，需要时再 cast 成
    /// `Arc<dyn Channel>` 调 trait 方法。避免 trait object downcast 的麻烦。
    #[derive(Debug)]
    pub struct MockChannel {
        name: &'static str,
        calls: Mutex<Vec<&'static str>>,
    }

    /// MockMessageHandler：把收到的消息存到内部 Vec，测试断言时取。
    #[derive(Debug, Default)]
    struct MockMessageHandler {
        received: Mutex<Vec<IncomingMessage>>,
    }

    impl MockChannel {
        /// 构造一个新的 mock channel，name 是 `Channel::name()` 的返回值。
        pub fn new(name: &'static str) -> Self {
            Self {
                name,
                calls: Mutex::new(Vec::new()),
            }
        }
        /// 测试断言：拿到一份调用记录的快照。
        pub fn calls_snapshot(&self) -> Vec<&'static str> {
            self.calls.lock().clone()
        }
    }

    impl MockMessageHandler {
        fn received_snapshot(&self) -> Vec<IncomingMessage> {
            self.received.lock().clone()
        }
    }

    #[async_trait]
    impl Channel for MockChannel {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn start(&self, _handler: Arc<dyn MessageHandler>) -> Result<()> {
            self.calls.lock().push("start");
            Ok(())
        }

        async fn reply(
            &self,
            _ctx: &ReplyContext,
            _target: ReplyTarget,
            _content: OutgoingContent,
        ) -> Result<()> {
            self.calls.lock().push("reply");
            Ok(())
        }

        async fn send(
            &self,
            _ctx: &ReplyContext,
            _target: ReplyTarget,
            _content: OutgoingContent,
        ) -> Result<()> {
            self.calls.lock().push("send");
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            self.calls.lock().push("stop");
            Ok(())
        }
    }

    #[async_trait]
    impl MessageHandler for MockMessageHandler {
        async fn on_message(
            &self,
            _channel: Arc<dyn Channel>,
            msg: IncomingMessage,
        ) -> Result<()> {
            self.received.lock().push(msg);
            Ok(())
        }
    }

    /// 构造测试用的样例消息，集中常量避免散落。
    /// 字段顺序对齐 types.rs 的 struct 定义（含 M1.5 followup 新增的
    /// is_mention / sender_kind / is_from_self），编译期强制更新。
    fn sample_message() -> IncomingMessage {
        IncomingMessage {
            platform: PlatformKind::Feishu,
            session_key: SessionKey::derive(PlatformKind::Feishu, "oc_test", None),
            sender: SenderId::new("ou_test"),
            content: IncomingContent::Text("hello".into()),
            reply_target: ReplyTarget::feishu("oc_test", None, FeishuChatType::P2p),
            timestamp_ms: 1_700_000_000_000,
            raw_message_id: "om_test".into(),
            is_mention: false,
            sender_kind: crate::types::SenderKind::User,
            is_from_self: false,
        }
    }

    /// Channel trait 方法（reply/send/stop）必须按调用顺序记入 mock。
    /// 直接持有 Arc<MockChannel> 读状态，避免 dyn downcast。
    #[tokio::test]
    async fn test_channel_reply_send_stop_recording() {
        let mock = Arc::new(MockChannel::new("mock"));
        let ch: Arc<dyn Channel> = mock.clone();
        let target = ReplyTarget::feishu("oc_x", None, FeishuChatType::P2p);
        let ctx = ReplyContext::default();

        ch.reply(&ctx, target.clone(), OutgoingContent::Text("r".into()))
            .await
            .unwrap();
        ch.send(&ctx, target.clone(), OutgoingContent::Text("s".into()))
            .await
            .unwrap();
        ch.stop().await.unwrap();

        assert_eq!(
            mock.calls_snapshot(),
            vec!["reply", "send", "stop"],
            "调用顺序必须被 mock 精确记录"
        );
    }

    /// Channel::start 返回 Ok(()) 即可；handler 参数的「有效性」由
    /// 后续 on_message 触发来验证。Channel impl 收到什么 handler
    /// 是 channel 自身的契约（M3 feishu 实现会验证 token 后再握手）。
    #[tokio::test]
    async fn test_channel_start_succeeds() {
        let mock = Arc::new(MockChannel::new("mock"));
        let ch: Arc<dyn Channel> = mock.clone();
        let handler: Arc<dyn MessageHandler> = Arc::new(MockMessageHandler::default());

        ch.start(handler).await.unwrap();
        assert_eq!(mock.calls_snapshot(), vec!["start"]);
    }

    /// MessageHandler::on_message 必须把消息存进内部 Vec，重复调用
    /// 累积多条。模拟 dispatcher 收到 N 条入站消息的场景。
    #[tokio::test]
    async fn test_message_handler_accumulates() {
        let handler = Arc::new(MockMessageHandler::default());
        let mock_ch = Arc::new(MockChannel::new("mock"));
        let ch: Arc<dyn Channel> = mock_ch.clone();

        // 模拟 3 条入站消息。
        for _ in 0..3 {
            handler
                .on_message(ch.clone(), sample_message())
                .await
                .unwrap();
        }
        assert_eq!(handler.received_snapshot().len(), 3);
    }

    /// ReplyContext::default 必须有 30s 超时（设计文档里规定的默认）。
    #[test]
    fn test_reply_context_default_timeout() {
        let ctx = ReplyContext::default();
        assert_eq!(ctx.timeout, std::time::Duration::from_secs(30));
    }

    /// Channel trait 的所有 async 方法在没有底层 I/O 时必须能直接 await。
    /// 这里同时验证 mock 在 `Arc<dyn Channel>` 下的多态调用。
    #[tokio::test]
    async fn test_channel_via_trait_object_polymorphism() {
        let mock = MockChannel::new("polymorphic");
        let ch: Arc<dyn Channel> = Arc::new(mock);
        // 通过 trait object 调 name()。
        assert_eq!(ch.name(), "polymorphic");
    }
}
