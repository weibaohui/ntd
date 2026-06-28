//! 集成测试：dispatcher 在 burst 负载下的表现。
//!
//! 区别于 dispatcher.rs 内部的单元测试，这里走 ntd-connect 公开 API
//! （crate 外部使用者视角），验证 trait + dispatcher 协同的稳定性。
//!
//! 参考 docs/ntd-connect-design.md §11 测试策略。

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::mpsc;

use ntd_connect::agent::{Agent, AgentSession, Event};
use ntd_connect::channel::{Channel, MessageHandler};
use ntd_connect::dispatcher::{Dispatcher, DispatcherConfig};
use ntd_connect::error::Result;
use ntd_connect::types::{
    AgentContext, AgentSessionInfo, Attachment, FeishuChatType, IncomingContent, IncomingMessage,
    OutgoingContent, PermissionResult, PlatformKind, ReplyContext, ReplyTarget, SenderId,
    SessionKey,
};

/// Mock channel：调 reply/send 时不实际发任何 HTTP，只记录调用计数。
struct CountingChannel {
    name: &'static str,
    replies: parking_lot::Mutex<u32>,
}

impl CountingChannel {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            name: "counting",
            replies: parking_lot::Mutex::new(0),
        })
    }
    fn reply_count(&self) -> u32 {
        *self.replies.lock()
    }
}

#[async_trait]
impl Channel for CountingChannel {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn start(&self, _handler: Arc<dyn MessageHandler>) -> Result<()> {
        Ok(())
    }
    async fn reply(
        &self,
        _ctx: &ReplyContext,
        _target: ReplyTarget,
        _content: OutgoingContent,
    ) -> Result<()> {
        *self.replies.lock() += 1;
        Ok(())
    }
    async fn send(
        &self,
        _ctx: &ReplyContext,
        _target: ReplyTarget,
        _content: OutgoingContent,
    ) -> Result<()> {
        Ok(())
    }
    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

/// Mock agent：每次 send 立即 emit 一段 Text + Result（不发 Closed），
/// 让 worker 的 recv 循环在 Result 处 break；Closed 由 session drop 时
/// tx 销毁自然产生，dispatcher 不主动读它。
struct FastAgent;

#[async_trait]
impl Agent for FastAgent {
    fn name(&self) -> &'static str {
        "fast"
    }
    async fn start_session(
        &self,
        _ctx: &AgentContext,
        _session_id: Option<&str>,
    ) -> Result<Box<dyn AgentSession>> {
        Ok(Box::new(FastSession::new()))
    }
    async fn list_sessions(&self, _ctx: &AgentContext) -> Result<Vec<AgentSessionInfo>> {
        Ok(Vec::new())
    }
    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

struct FastSession {
    /// Worker take 走的 receiver；只能 take 一次。
    events: Option<mpsc::Receiver<Event>>,
    /// Session 内部持有的 sender；send() 时通过它 push events 到 channel。
    /// 这样多 turn 的事件都进同一个 channel，drain 循环才能继续读。
    tx: mpsc::Sender<Event>,
}

impl FastSession {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel(8);
        FastSession {
            events: Some(rx),
            tx,
        }
    }
}

#[async_trait]
impl AgentSession for FastSession {
    async fn send(&self, _prompt: &str, _attachments: &[Attachment]) -> Result<()> {
        // 模拟 agent 立即回复：push 一条 Text + Result。
        // 不 push Closed——Channel 在 FastSession drop 时由 tx 销毁自然关闭。
        // 这样 drain 循环下次 send() 又能 push 新一组，receiver 不卡住。
        let _ = self.tx.try_send(Event::Text("reply".into()));
        let _ = self.tx.try_send(Event::Result {
            usage: Default::default(),
            duration_ms: 1,
        });
        Ok(())
    }
    async fn respond_permission(
        &self,
        _request_id: &str,
        _result: PermissionResult,
    ) -> Result<()> {
        Ok(())
    }
    fn take_events(&mut self) -> mpsc::Receiver<Event> {
        self.events
            .take()
            .expect("FastSession::take_events called twice")
    }
    fn session_id(&self) -> &str {
        "fast"
    }
    fn alive(&self) -> bool {
        true
    }
    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

fn sample_msg(ts: i64, key: &str, raw_id: &str) -> IncomingMessage {
    IncomingMessage {
        platform: PlatformKind::Feishu,
        session_key: SessionKey::derive(PlatformKind::Feishu, key, None),
        sender: SenderId::new("ou_a"),
        content: IncomingContent::Text("hi".into()),
        reply_target: ReplyTarget::feishu("oc", None, FeishuChatType::P2p),
        timestamp_ms: ts,
        raw_message_id: raw_id.into(),
        is_mention: false,
        sender_kind: ntd_connect::types::SenderKind::User,
        is_from_self: false,
            mentioned_open_ids: vec![],    }
}

/// 公开 API 端到端：100 条消息分 5 个 session，断言总耗时 < 5s。
///
/// 这是 docs/ntd-connect-design.md §11 性能目标的核心断言。
///
/// 用 `current_thread` runtime：避免与 cargo test 并发跑其他测试时
/// 出现 worker 调度竞争（multi_thread 下偶发 99/100 漏一条）。
#[tokio::test(flavor = "current_thread")]
async fn integration_burst_100_under_5s() {
    let channel = CountingChannel::new();
    let dyn_ch: Arc<dyn Channel> = channel.clone();
    let dyn_agent: Arc<dyn Agent> = Arc::new(FastAgent);
    let mut dispatcher = Dispatcher::new(
        dyn_ch,
        dyn_agent,
        DispatcherConfig {
            max_concurrent_turns: 16,
            max_pending_per_session: 256,
            max_sessions: 32,
            dedup_ttl: Duration::from_secs(60),
        },
    );

    let start = Instant::now();
    for i in 0..100 {
        let key = format!("oc_{}", i % 5);
        let raw_id = format!("om_{i}");
        let m = sample_msg(10_000 + i, &key, &raw_id);
        dispatcher.on_message(channel.clone(), m).await.unwrap();
    }
    dispatcher.join().await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "burst 100 took {elapsed:?}, expected < 5s"
    );
    // 100 条消息每条触发 1 次 reply（Text event）。
    assert_eq!(
        channel.reply_count(),
        100,
        "每条消息应触发恰好 1 次 reply"
    );
}

/// dedup 在公开 API 下也生效：同 raw_message_id 第二次被丢。
#[tokio::test(flavor = "current_thread")]
async fn integration_dedup_at_public_api() {
    let channel = CountingChannel::new();
    let dyn_ch: Arc<dyn Channel> = channel.clone();
    let dyn_agent: Arc<dyn Agent> = Arc::new(FastAgent);
    let mut dispatcher = Dispatcher::new(
        dyn_ch,
        dyn_agent,
        DispatcherConfig::default(),
    );

    // 同 raw_message_id "om_dup" 来两次。
    let m1 = sample_msg(1000, "oc_d", "om_dup");
    let m2 = sample_msg(1001, "oc_d", "om_dup");
    dispatcher.on_message(channel.clone(), m1).await.unwrap();
    dispatcher.on_message(channel.clone(), m2).await.unwrap();
    dispatcher.join().await;

    // 只有第一条触发 reply。
    assert_eq!(channel.reply_count(), 1);
}
