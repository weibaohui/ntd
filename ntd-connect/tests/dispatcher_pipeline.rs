//! Dispatcher 端到端集成测试：完整流水线。
//!
//! 模拟消息从 Channel 进 Dispatcher → worker 调 Router → Decision=
//! ForwardToAgent → agent.send + events → channel.reply。
//!
//! 步骤 9（dispatcher 接入）落地的验证点。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex as TokioMutex;

use ntd_connect::agent::{Agent, AgentSession, Event};
use ntd_connect::agent_impl::claude_code::ClaudeCodeAgent;
use ntd_connect::channel::{Channel, MessageHandler};
use ntd_connect::dispatcher::{Dispatcher, DispatcherConfig};
use ntd_connect::error::Result;
use ntd_connect::http::SharedHttpClient;
use ntd_connect::router::{Decision, Router};
use ntd_connect::types::{
    AgentContext, AgentSessionInfo, Attachment, FeishuChatType, IncomingContent,
    IncomingMessage, OutgoingContent, PermissionResult, PlatformKind, ReplyContext, ReplyTarget,
    SenderId, SenderKind, SessionKey,
};

// ============================================================
// Mock channel：reply/send 写到共享 vec
// ============================================================

#[derive(Default)]
struct MockChannel {
    replies: TokioMutex<Vec<String>>,
}

impl MockChannel {
    async fn reply_count(&self) -> usize {
        self.replies.lock().await.len()
    }
    async fn replies_snapshot(&self) -> Vec<String> {
        self.replies.lock().await.clone()
    }
}

#[async_trait]
impl Channel for MockChannel {
    fn name(&self) -> &'static str {
        "mock"
    }
    async fn start(&self, _handler: Arc<dyn MessageHandler>) -> Result<()> {
        Ok(())
    }
    async fn reply(
        &self,
        _ctx: &ReplyContext,
        _target: ReplyTarget,
        content: OutgoingContent,
    ) -> Result<()> {
        let text = match content {
            OutgoingContent::Text(s) => s,
            _ => String::from("<non-text>"),
        };
        self.replies.lock().await.push(text);
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

// ============================================================
// Mock Agent：每次 send 立即 push Text + Result 到 events
// ============================================================

struct MockAgent {
    events_tx: TokioMutex<Option<tokio::sync::mpsc::Sender<Event>>>,
}

impl MockAgent {
    fn new() -> Self {
        Self {
            events_tx: TokioMutex::new(None),
        }
    }
}

#[async_trait]
impl Agent for MockAgent {
    fn name(&self) -> &'static str {
        "mock-agent"
    }
    async fn start_session(
        &self,
        _ctx: &AgentContext,
        _sid: Option<&str>,
    ) -> Result<Box<dyn AgentSession>> {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        *self.events_tx.lock().await = Some(tx.clone());
        struct S {
            tx: tokio::sync::mpsc::Sender<Event>,
            rx: Option<tokio::sync::mpsc::Receiver<Event>>,
        }
        #[async_trait]
        impl AgentSession for S {
            async fn send(
                &self,
                _: &str,
                _: &[Attachment],
            ) -> ntd_connect::error::Result<()> {
                let _ = self.tx.try_send(Event::Text("agent-reply".into()));
                let _ = self.tx.try_send(Event::Result {
                    usage: Default::default(),
                    duration_ms: 1,
                });
                Ok(())
            }
            async fn respond_permission(
                &self,
                _: &str,
                _: PermissionResult,
            ) -> ntd_connect::error::Result<()> {
                Ok(())
            }
            fn take_events(&mut self) -> tokio::sync::mpsc::Receiver<Event> {
                self.rx.take().expect("take_events called twice")
            }
            fn session_id(&self) -> &str {
                "mock-session"
            }
            fn alive(&self) -> bool {
                true
            }
            async fn close(&self) -> ntd_connect::error::Result<()> {
                Ok(())
            }
        }
        Ok(Box::new(S { tx, rx: Some(rx) }))
    }
    async fn list_sessions(&self, _ctx: &AgentContext) -> Result<Vec<AgentSessionInfo>> {
        Ok(Vec::new())
    }
    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

// ============================================================
// Mock Router：可定制 Decision 的 router
// ============================================================

struct MockRouter {
    decision: Decision,
}

#[async_trait]
impl Router for MockRouter {
    async fn route(&self, _msg: IncomingMessage) -> Decision {
        self.decision.clone()
    }
}

fn sample_msg(ts: i64, key: &str) -> IncomingMessage {
    IncomingMessage {
        platform: PlatformKind::Feishu,
        session_key: SessionKey::derive(PlatformKind::Feishu, key, None),
        sender: SenderId::new("ou_a"),
        content: IncomingContent::Text("hi".into()),
        reply_target: ReplyTarget::feishu(key, None, FeishuChatType::P2p),
        timestamp_ms: ts,
        raw_message_id: format!("om_{ts}"),
        is_mention: false,
        sender_kind: SenderKind::User,
        is_from_self: false,
            mentioned_open_ids: vec![],    }
}

// ============================================================
// 测试：完整流水线（dispatcher + router + agent）
// ============================================================

/// Router::ForwardToAgent → dispatcher worker → agent.send → channel.reply。
#[tokio::test(flavor = "current_thread")]
async fn test_dispatcher_pipeline_forward_to_agent() {
    let channel = Arc::new(MockChannel::default());
    let dyn_ch: Arc<dyn Channel> = channel.clone();
    let dyn_agent: Arc<dyn Agent> = Arc::new(MockAgent::new());

    let router = Arc::new(MockRouter {
        decision: Decision::ForwardToAgent,
    });
    let dyn_router: Arc<dyn Router> = router.clone();

    let mut dispatcher = Dispatcher::with_router(
        dyn_ch,
        dyn_agent,
        Some(dyn_router),
        DispatcherConfig::default(),
    );

    let msg = sample_msg(1000, "oc_a");
    dispatcher
        .on_message(channel.clone(), msg)
        .await
        .unwrap();
    dispatcher.join().await;

    // agent.send 触发 Event::Text("agent-reply") → channel.reply。
    let replies = channel.replies_snapshot().await;
    assert_eq!(replies.len(), 1, "应恰好 1 次 reply");
    assert_eq!(replies[0], "agent-reply");
}

/// Router::Skip → dispatcher worker 立即 return，不调 agent，不回 reply。
#[tokio::test(flavor = "current_thread")]
async fn test_dispatcher_pipeline_router_skip() {
    let channel = Arc::new(MockChannel::default());
    let dyn_ch: Arc<dyn Channel> = channel.clone();
    let dyn_agent: Arc<dyn Agent> = Arc::new(MockAgent::new());

    let router = Arc::new(MockRouter {
        decision: Decision::Skip,
    });
    let dyn_router: Arc<dyn Router> = router.clone();

    let mut dispatcher = Dispatcher::with_router(
        dyn_ch,
        dyn_agent,
        Some(dyn_router),
        DispatcherConfig::default(),
    );

    let msg = sample_msg(1000, "oc_b");
    dispatcher
        .on_message(channel.clone(), msg)
        .await
        .unwrap();
    dispatcher.join().await;

    // Skip 路径：channel.reply 不被调。
    assert_eq!(channel.reply_count().await, 0, "Skip 时不应 reply");
}

/// Router::Handled → dispatcher worker 立即 return（router 已处理）。
#[tokio::test(flavor = "current_thread")]
async fn test_dispatcher_pipeline_router_handled() {
    let channel = Arc::new(MockChannel::default());
    let dyn_ch: Arc<dyn Channel> = channel.clone();
    let dyn_agent: Arc<dyn Agent> = Arc::new(MockAgent::new());

    let router = Arc::new(MockRouter {
        decision: Decision::Handled,
    });
    let dyn_router: Arc<dyn Router> = router.clone();

    let mut dispatcher = Dispatcher::with_router(
        dyn_ch,
        dyn_agent,
        Some(dyn_router),
        DispatcherConfig::default(),
    );

    let msg = sample_msg(1000, "oc_c");
    dispatcher
        .on_message(channel.clone(), msg)
        .await
        .unwrap();
    dispatcher.join().await;

    // Handled 路径：dispatcher 不调 agent（agent.send 不被调），
    // channel.reply 也不被 dispatcher 调（router 内部可能已回，但 MockRouter 不实现）。
    assert_eq!(channel.reply_count().await, 0);
}

/// 不传 router：dispatcher 跳过 router 调用直接 ForwardToAgent（v1 fallback）。
#[tokio::test(flavor = "current_thread")]
async fn test_dispatcher_pipeline_no_router_fallback() {
    let channel = Arc::new(MockChannel::default());
    let dyn_ch: Arc<dyn Channel> = channel.clone();
    let dyn_agent: Arc<dyn Agent> = Arc::new(MockAgent::new());

    let mut dispatcher = Dispatcher::new(
        dyn_ch,
        dyn_agent,
        DispatcherConfig::default(),
    );

    let msg = sample_msg(1000, "oc_d");
    dispatcher
        .on_message(channel.clone(), msg)
        .await
        .unwrap();
    dispatcher.join().await;

    // router=None 时也走 ForwardToAgent 路径。
    assert_eq!(channel.reply_count().await, 1);
}

/// burst 10 条消息路由都返回 ForwardToAgent：10 次 reply。
#[tokio::test(flavor = "current_thread")]
async fn test_dispatcher_burst_with_router() {
    let channel = Arc::new(MockChannel::default());
    let dyn_ch: Arc<dyn Channel> = channel.clone();
    let dyn_agent: Arc<dyn Agent> = Arc::new(MockAgent::new());

    let router = Arc::new(MockRouter {
        decision: Decision::ForwardToAgent,
    });
    let dyn_router: Arc<dyn Router> = router.clone();

    let mut dispatcher = Dispatcher::with_router(
        dyn_ch,
        dyn_agent,
        Some(dyn_router),
        DispatcherConfig {
            max_concurrent_turns: 8,
            max_pending_per_session: 64,
            max_sessions: 16,
            ..DispatcherConfig::default()
        },
    );

    let start = std::time::Instant::now();
    for i in 0..10 {
        let key = format!("oc_{}", i % 3);
        let m = sample_msg(2000 + i, &key);
        dispatcher
            .on_message(channel.clone(), m)
            .await
            .unwrap();
    }
    dispatcher.join().await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "burst 10 took {elapsed:?}, expected < 5s"
    );
    assert_eq!(
        channel.reply_count().await,
        10,
        "10 条消息应触发 10 次 reply"
    );
}

// ============================================================
// 测试：ClaudeCodeAgent 真实 spawn（端到端）
// ============================================================

/// spawn `/bin/sh` mock claude binary，验证 dispatcher 整条流水线
/// （router ForwardToAgent → ClaudeCodeAgent → mock binary → events）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_dispatcher_with_real_claude_code_agent() {
    // 写 mock claude binary
    let tmpdir = std::env::temp_dir().join(format!(
        "ntd-connect-pipeline-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&tmpdir).unwrap();
    let mock_path = tmpdir.join("mock-claude.sh");
    // mock 行为：每条 stdin prompt 都回一条 assistant + 一条 result
    // （避免 worker 死等 Result event）。
    std::fs::write(
        &mock_path,
        r#"#!/bin/sh
while IFS= read -r line; do
    text=$(echo "$line" | sed -n 's/.*"text":"\([^"]*\)".*/\1/p')
    echo "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"claude-says: $text\"}]}}"
    echo '{"type":"result","subtype":"success","duration_ms":99,"usage":{"input_tokens":1,"output_tokens":1}}'
done
"#,
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&mock_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&mock_path, perms).unwrap();

    // 构造 dispatcher + router + 真 ClaudeCodeAgent
    let channel = Arc::new(MockChannel::default());
    let dyn_ch: Arc<dyn Channel> = channel.clone();
    let dyn_agent: Arc<dyn Agent> = Arc::new(ClaudeCodeAgent::with_path(mock_path.clone()));

    let router = Arc::new(MockRouter {
        decision: Decision::ForwardToAgent,
    });
    let dyn_router: Arc<dyn Router> = router.clone();

    let mut dispatcher = Dispatcher::with_router(
        dyn_ch,
        dyn_agent,
        Some(dyn_router),
        DispatcherConfig::default(),
    );

    let msg = sample_msg(3000, "oc_real");
    dispatcher
        .on_message(channel.clone(), msg)
        .await
        .unwrap();
    dispatcher.join().await;

    // mock binary 应该 echo 回 "claude-says: hi"。
    let replies = channel.replies_snapshot().await;
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0], "claude-says: hi");

    let _ = std::fs::remove_dir_all(&tmpdir);
}

/// 不传 SharedHttpClient 也能跑（ClaudeCodeAgent 内部不直接用）。
/// 这里仅证明 ClaudeCodeAgent 字段构造对 dispatcher 不依赖 SharedHttpClient。
#[test]
fn test_claude_code_agent_does_not_need_http_client() {
    let agent = ClaudeCodeAgent::new();
    assert_eq!(agent.name(), "claude-code");
    // 强制使用 SharedHttpClient 防止 dead-code warning。
    let _http = SharedHttpClient::new();
}
