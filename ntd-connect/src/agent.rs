//! Agent trait 与事件流：抽象 AI coding CLI（Claude Code / Codex / Hermes）。
//!
//! # 与 cc-connect 的对应
//!
//! 对应 `cc-connect/core/interfaces.go:382-390 Agent` 与
//! `core/interfaces.go:393-406 AgentSession`，以及 [`Event`] 对应
//! `core/message.go:210-218 Event`。
//!
//! # 设计要点
//!
//! - `Agent` 是「项目级」长生命周期对象（一个 Agent 实例对应一个
//!   claude 命令模板），`AgentSession` 是「对话级」短生命周期对象
//!   （一个 session 对应一次 claude 进程）。这是 cc-connect 关键的
//!   长/短生命周期分层。
//! - `Event` 是单一通道的 mpsc 消息流，事件类型由 enum 表达。Agent
//!   进程 stdout 的所有协议帧都被归一化到这些 variant 上。
//! - `take_events()` 一次性 ownership 转移，Worker 跨整个 session
//!   持有 receiver 处理多 turn；跨 worker 复用 agent_session 的优化
//!   在 v1 不做（每次新 turn 都新建 session，性能代价在 v2 再优化）。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::Result;
use crate::types::{AgentContext, AgentSessionInfo, Attachment, PermissionResult, Usage};

/// AI executor 抽象。
///
/// 一个 Agent 实例 = 一个 AI CLI 的「项目配置」（claude 模板、codex
/// 模板等），通常与 workspace 一一对应。Agent 自己几乎是无状态的；
/// 真正在跑的是它 spawn 出来的 [`AgentSession`]。
#[async_trait]
pub trait Agent: Send + Sync {
    /// executor 名字（用于日志 / metrics）。
    fn name(&self) -> &'static str;

    /// 启动一个新 session 或恢复一个已有 session。
    ///
    /// `session_id == None` 表示全新 session；`Some(id)` 表示 resume
    /// 已有 session（Claude Code 用 `--resume <id>`）。
    /// 返回的 `AgentSession` 持有底层子进程的 stdin/stdout，调用方
    /// 通过 [`AgentSession::take_events`] 持续读 events。
    async fn start_session(
        &self,
        ctx: &AgentContext,
        session_id: Option<&str>,
    ) -> Result<Box<dyn AgentSession>>;

    /// 列出已存在的 session（用于「恢复上次对话」等 UI 场景）。
    async fn list_sessions(&self, ctx: &AgentContext) -> Result<Vec<AgentSessionInfo>>;

    /// 关闭 Agent：杀掉所有仍在跑的 session，释放资源。
    async fn stop(&self) -> Result<()>;
}

/// 一个真正在跑的 AI executor 会话。
///
/// 生命周期：调用方持有 `Box<dyn AgentSession>` → 调
/// [`take_events`](Self::take_events) 拿到 receiver（一次性 ownership 转移）→
/// 在循环里调 [`send`](Self::send) + 处理 events → 最后 [`close`](Self::close)。
///
/// # `Send` 而非 `Send + Sync`
///
/// `AgentSession` 通常持有 tokio child process handle + 内部 Mutex，
/// 不能安全跨线程共享，所以只要求 `Send`（允许 move 到另一个 task）。
/// 多消费者场景由 `mpsc::Receiver<Event>` 自然支持，不需要 `Sync`。
#[async_trait]
pub trait AgentSession: Send {
    /// 往 session 发一条用户消息（含可选附件）。
    ///
    /// 返回 `Ok(())` 只表示「消息已成功写入 stdin」，不代表 agent 已经
    /// 处理完。处理进度通过 events channel 体现。
    async fn send(&self, prompt: &str, attachments: &[Attachment]) -> Result<()>;

    /// 回复 agent 的权限请求（见 [`Event::PermissionRequest`]）。
    ///
    /// dispatcher 在收到 `PermissionRequest` 事件后，把决策结果通过
    /// 此方法回传给 agent（Claude Code 是写一条 stdin JSON line）。
    async fn respond_permission(
        &self,
        request_id: &str,
        result: PermissionResult,
    ) -> Result<()>;

    /// 取出 events channel 的 receiver（**一次性** ownership 转移）。
    ///
    /// 多次调用是 bug，会 panic（实现层用 `Option::take().expect()`）。
    /// Worker 在持有 receiver 后跨越整个 session 读 events。
    fn take_events(&mut self) -> mpsc::Receiver<Event>;

    /// 当前 session ID（Claude Code 是磁盘 JSONL 文件名）。
    fn session_id(&self) -> &str;

    /// session 进程是否仍在运行。
    fn alive(&self) -> bool;

    /// 优雅关闭 session。
    ///
    /// 三段式：close stdin → SIGTERM group → SIGKILL group（参考
    /// cc-connect `agent/claudecode/session.go:1159-1216`）。
    async fn close(&self) -> Result<()>;
}

/// Agent → Dispatcher 的事件流。
///
/// 这是 cc-connect 把 30+ 种协议帧归一化后的最小可用集合：
/// - `Text`：assistant 文本片段（可能多条拼接才是完整回复）
/// - `ToolUse`：tool 调用（用于 UI 展示执行步骤）
/// - `Result`：turn 结束，含 usage 统计
/// - `PermissionRequest`：阻塞型，dispatcher 必须回 [`AgentSession::respond_permission`]
/// - `Error`：agent 内部错误
/// - `Closed`：events channel 关闭（通常因为进程退出）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Event {
    /// assistant 文本片段。多次 `Text` 累积 = 完整回复。
    Text(String),

    /// tool 调用事件（dispatcher 可用于展示进度，不需要回执）。
    ToolUse {
        /// tool 名（如 `Edit` / `Write` / `Bash`）。
        name: String,
        /// tool 参数，原始 JSON（不同 agent schema 不同，保留灵活性）。
        args: serde_json::Value,
    },

    /// turn 结束。
    ///
    /// dispatcher 收到 `Result` 后应检查 busy queue 是否有待处理消息：
    /// 有则 drain，无则释放 session lock。
    Result {
        /// 本次 turn 的 usage 统计（可能为空，agent 不一定上报）。
        usage: Usage,
        /// turn 耗时（毫秒）。
        duration_ms: u64,
    },

    /// 权限请求（**阻塞型**，dispatcher 必须回 [`AgentSession::respond_permission`]）。
    ///
    /// agent 在等回执期间不会继续推进；dispatcher 可以选择转给用户、
    /// 或按规则自动 allow/deny。
    PermissionRequest {
        /// 权限请求 ID（用于回执时匹配）。
        request_id: String,
        /// tool 名。
        tool: String,
        /// tool 参数。
        args: serde_json::Value,
    },

    /// agent 内部错误。
    ///
    /// 非致命：dispatcher 可以选择继续读 events，或 abort session。
    Error(String),

    /// events channel 已关闭。
    ///
    /// 通常意味着 agent 进程退出。dispatcher 收到 `Closed` 后应
    /// 清理 session 状态、释放 lock。
    Closed,
}

#[cfg(test)]
pub mod tests {
    //! Agent / AgentSession 的单元测试 + mock 实现。

    use super::*;
    // Arc 在 trait 定义里不需要（Agent 返回 Box<dyn AgentSession>），
    // 只在 mock 多态调用里用，所以 import 放在 tests mod 内部。
    use std::sync::Arc;
    use parking_lot::Mutex;

    /// 工厂：构造一个 dummy agent session 给跨模块测试用。
    ///
    /// 例如 `session.rs` 测试需要验证 `set_agent_session` / `take_agent_session`，
    /// 不想自己重新写 mock。pub 让 sibling 测试 mod 能直接调用。
    /// 编译时受 `#[cfg(test)]` 约束，不会进 release 构建。
    pub fn dummy_agent_session() -> Box<dyn AgentSession> {
        Box::new(MockAgentSession::new("dummy".into()))
    }

    /// MockAgent：实现 Agent trait，spawn MockAgentSession。
    pub struct MockAgent {
        name: &'static str,
        /// 启动过的 session 计数（用于断言）。
        started: Arc<Mutex<u32>>,
    }

    impl MockAgent {
        /// 构造一个新的 mock agent，name 是 `Agent::name()` 的返回值。
        pub fn new(name: &'static str) -> Self {
            Self {
                name,
                started: Arc::new(Mutex::new(0)),
            }
        }
        /// 测试断言：累计启动过的 session 数。
        pub fn started_count(&self) -> u32 {
            *self.started.lock()
        }
    }

    #[async_trait]
    impl Agent for MockAgent {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn start_session(
            &self,
            _ctx: &AgentContext,
            session_id: Option<&str>,
        ) -> Result<Box<dyn AgentSession>> {
            *self.started.lock() += 1;
            // session_id 作为 mock session ID 透传，便于测试 resume 路径。
            let id = session_id.unwrap_or("new").to_string();
            Ok(Box::new(MockAgentSession::new(id)))
        }

        async fn list_sessions(&self, _ctx: &AgentContext) -> Result<Vec<AgentSessionInfo>> {
            Ok(Vec::new())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }
    }

    /// MockAgentSession：用 mpsc channel 模拟 agent event 流。
    /// `take_events` 一次性返回 receiver；后续 `send` 不真做事。
    pub struct MockAgentSession {
        id: String,
        events: Option<mpsc::Receiver<Event>>,
        alive: bool,
    }

    impl MockAgentSession {
        /// 构造一个新 mock agent session，预置 Text + Result + Closed 三条 event。
        pub fn new(id: String) -> Self {
            // 预置一条 Text + Closed，模拟「agent 立即回复并退出」。
            let (tx, rx) = mpsc::channel(8);
            tx.try_send(Event::Text("hello".into())).unwrap();
            tx.try_send(Event::Result {
                usage: Usage::default(),
                duration_ms: 100,
            })
            .unwrap();
            tx.try_send(Event::Closed).unwrap();
            drop(tx);
            Self {
                id,
                events: Some(rx),
                alive: true,
            }
        }
    }

    #[async_trait]
    impl AgentSession for MockAgentSession {
        async fn send(&self, _prompt: &str, _attachments: &[Attachment]) -> Result<()> {
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
            // 一次性 ownership 转移。多次调用是 bug，expect 让它炸出来。
            // 比 `Option<Receiver>` 更明确：调用方拿到 receiver 后就
            // 不能再来第二次，文档和 panic 双重保证。
            self.events
                .take()
                .expect("AgentSession::take_events called twice — 这是 bug")
        }

        fn session_id(&self) -> &str {
            &self.id
        }

        fn alive(&self) -> bool {
            self.alive
        }

        async fn close(&self) -> Result<()> {
            Ok(())
        }
    }

    /// Agent::name 必须返回 'static str；多态调用通过 Arc<dyn Agent>。
    #[tokio::test]
    async fn test_agent_name_polymorphic() {
        let agent = MockAgent::new("claude-code");
        let dyn_agent: Arc<dyn Agent> = Arc::new(agent);
        assert_eq!(dyn_agent.name(), "claude-code");
    }

    /// Agent::start_session 必须 spawn session 并返回 sender session_id。
    /// take_events 一次性：第二次调用必须 panic（避免调用方误用）。
    #[tokio::test]
    async fn test_agent_start_session_returns_session() {
        let agent = Arc::new(MockAgent::new("claude-code"));
        let dyn_agent: Arc<dyn Agent> = agent.clone();
        let mut session = dyn_agent
            .start_session(&AgentContext::default(), None)
            .await
            .unwrap();
        assert_eq!(session.session_id(), "new");
        assert!(session.alive());
        assert_eq!(agent.started_count(), 1);

        // 第一次 take_events 拿到 receiver。
        let _rx = session.take_events();

        // 第二次必须 panic：所有权已转移。
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            session.take_events();
        }));
        assert!(result.is_err(), "take_events 二次调用应 panic（防误用）");
    }

    /// resume 模式：session_id=Some(id) 时透传给 session。
    #[tokio::test]
    async fn test_agent_start_session_resume() {
        let agent = MockAgent::new("claude-code");
        let dyn_agent: Arc<dyn Agent> = Arc::new(agent);
        let mut session = dyn_agent
            .start_session(&AgentContext::default(), Some("abc-123"))
            .await
            .unwrap();
        assert_eq!(session.session_id(), "abc-123");
        // take 掉 events 避免 drop warning。
        let _rx = session.take_events();
    }

    /// Event 序列化稳定：Text / Result / Closed 等 variant 必须能 serde。
    /// Dispatcher 和 channel 之间如果传 Event JSON，必须保证 roundtrip。
    #[test]
    fn test_event_serialize_roundtrip() {
        let events = vec![
            Event::Text("hello".into()),
            Event::ToolUse {
                name: "Edit".into(),
                args: serde_json::json!({"path": "/tmp/x"}),
            },
            Event::Result {
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_read_tokens: 0,
                },
                duration_ms: 100,
            },
            Event::PermissionRequest {
                request_id: "req-1".into(),
                tool: "Bash".into(),
                args: serde_json::json!({"cmd": "ls"}),
            },
            Event::Error("oops".into()),
            Event::Closed,
        ];
        for ev in &events {
            let json = serde_json::to_string(ev).unwrap();
            let back: Event = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, ev, "Event roundtrip 失败: {json}");
        }
    }

    /// AgentSession 必须能通过 mpsc channel 把事件喂给消费者。
    /// 这里验证 mock session 预置的 events 能被正确读出。
    #[tokio::test]
    async fn test_session_events_flow() {
        let agent = MockAgent::new("claude-code");
        let dyn_agent: Arc<dyn Agent> = Arc::new(agent);
        let mut session = dyn_agent
            .start_session(&AgentContext::default(), None)
            .await
            .unwrap();
        let mut rx = session.take_events();

        // 依次读取预置的 Text / Result / Closed。
        let e1 = rx.recv().await.unwrap();
        assert!(matches!(e1, Event::Text(s) if s == "hello"));
        let e2 = rx.recv().await.unwrap();
        assert!(matches!(e2, Event::Result { duration_ms: 100, .. }));
        let e3 = rx.recv().await;
        assert!(matches!(e3, Some(Event::Closed)));
        // channel 关闭后再 recv 返回 None。
        assert!(rx.recv().await.is_none());
    }
}
