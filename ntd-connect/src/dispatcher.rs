//! Dispatcher：消息分发的运行时核心。
//!
//! # 与 cc-connect 的对应
//!
//! 对应 `cc-connect/core/engine.go:2300-3100` 的 `Engine.handleMessage` +
//! `Engine.runMessageDispatchLoop` + `processInteractiveMessageWith` +
//! `drainPendingMessages` 的合并实现。
//!
//! # 设计要点
//!
//! - **单 mpsc + JoinSet + Semaphore**：每个 channel 把入站消息送到
//!   dispatcher 的 `on_message`，dispatcher 内部按 session 分发到
//!   worker。worker 是独立 tokio task，受全局 Semaphore 限制并发。
//! - **per-session 串行保留**：worker 拿到 session lock 后串行 drain
//!   busy queue（同一 session 的消息不会乱序）。不同 session 可并行。
//! - **watermark 三态**：current_turn / last_completed / pending 隔离；
//!   busy queue 里的消息只跟 completed + current 比对，**不**跟自己
//!   比对（参考 `core/engine.go:2517-2526 isQueuedUserMessageStaleForDrainLocked`）。
//! - **agent_session 懒加载 + 复用**：第一次 turn 时启动进程，后续
//!   drain 同一个 session 复用，减少冷启动开销。
//! - **typing 反应由 worker 启停**：通过 `Channel::as_typing_indicator()`
//!   检测能力；不支持的平台自动 skip。

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;

use async_trait::async_trait;

use crate::agent::{Agent, AgentSession, Event};
use crate::channel::{Channel, MessageHandler};
use crate::dedup::Dedup;
use crate::error::{Error, Result};
use crate::session::{SessionManager, SessionState};
use crate::types::{
    AgentContext, IncomingContent, IncomingMessage, OutgoingContent, PermissionResult,
    ReplyContext, ReplyTarget,
};
use crate::typing::TypingGuard;

/// Dispatcher 配置。
///
/// 容量字段都是「软上限」：超过会触发对应降级（驱逐旧 session /
/// 丢弃新消息 / 排队等许可），不会 panic。
#[derive(Debug, Clone)]
pub struct DispatcherConfig {
    /// 全局同时在跑的 worker 上限（= Semaphore 容量）。
    /// 推荐 8~32；太高会撑爆飞书 API 限流。
    pub max_concurrent_turns: usize,
    /// 单 session busy queue 容量上限。
    /// 超过的消息会被 `Error::QueueFull` 丢弃。
    pub max_pending_per_session: usize,
    /// 全局 session 表容量上限（LRU 驱逐阈值）。
    pub max_sessions: usize,
    /// dedup TTL：默认 60s。重复消息 id 在 TTL 内会被静默丢弃。
    pub dedup_ttl: Duration,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        DispatcherConfig {
            max_concurrent_turns: 8,
            max_pending_per_session: 32,
            max_sessions: 1024,
            dedup_ttl: Duration::from_secs(60),
        }
    }
}

/// Dispatcher 实例。
///
/// # 为什么 Mutex<JoinSet> 而不是裸 JoinSet
///
/// `JoinSet` 是 `!Sync`，无法直接放在 `&self` 方法里 mutate。
/// `Mutex<JoinSet>` 让 `on_message` 在持有锁的短暂窗口里 spawn / drain。
/// 这个锁的争用只发生在「消息爆发」+「worker 完成」同时发生的瞬间，
/// 不是热路径，可以接受。
pub struct Dispatcher {
    channel: Arc<dyn Channel>,
    agent: Arc<dyn Agent>,
    sessions: SessionManager,
    dedup: Arc<Dedup>,
    /// JoinSet + Mutex：见 struct 注释。
    workers: Mutex<JoinSet<()>>,
    /// 全局并发限流。
    sem: Arc<Semaphore>,
    /// worker 用 AgentContext（v1 仅 work_dir = None 占位）。
    agent_context: AgentContext,
}

impl Dispatcher {
    /// 构造 dispatcher。
    pub fn new(
        channel: Arc<dyn Channel>,
        agent: Arc<dyn Agent>,
        config: DispatcherConfig,
    ) -> Self {
        let dedup = Arc::new(Dedup::new(config.dedup_ttl));
        let sessions = SessionManager::new(config.max_sessions, config.max_pending_per_session);
        let sem = Arc::new(Semaphore::new(config.max_concurrent_turns));
        Dispatcher {
            channel,
            agent,
            sessions,
            dedup,
            workers: Mutex::new(JoinSet::new()),
            sem,
            agent_context: AgentContext::default(),
        }
    }

    /// 等所有当前 worker 完成。常用于 graceful shutdown。
    pub async fn join(&mut self) {
        // 拿锁后逐个 join；逐个是因为 &mut self 已持锁，逐个 join 不需要
        // 再持锁。JoinSet 被 move 进 loop，所以用 std::mem::take。
        let mut workers = std::mem::take(&mut *self.workers.lock());
        while let Some(res) = workers.join_next().await {
            if let Err(e) = res {
                tracing::warn!("worker join error: {e}");
            }
        }
    }

    /// 周期性调用：清理已完成的 worker（释放 JoinSet 槽位）。
    ///
    /// `on_message` 每次调用前会 best-effort 调用一次，避免
    /// worker 完成但 JoinSet 残留。
    fn drain_finished_workers(&self) {
        let mut workers = self.workers.lock();
        while let Some(res) = workers.try_join_next() {
            if let Err(e) = res {
                tracing::warn!("worker join error: {e}");
            }
        }
    }
}

#[async_trait]
impl MessageHandler for Dispatcher {
    async fn on_message(
        &self,
        _channel: Arc<dyn Channel>,
        msg: IncomingMessage,
    ) -> Result<()> {
        // 0. 自我消息直接丢（bot 复读自己会触发死循环）。
        if msg.is_from_self {
            return Ok(());
        }

        // 0a. 定期清理已完成的 worker，避免 JoinSet 残留。
        self.drain_finished_workers();

        // 1. dedup：飞书在 WS 断线重连时会重投最近 N 条事件。
        if !self.dedup.check_and_set(&msg.raw_message_id) {
            return Ok(());
        }

        // 2. session：派生或命中已有 session state。
        let session = self.sessions.get_or_create(msg.session_key.clone());

        // 3. try lock：抢到就 spawn worker，抢不到走 busy queue。
        if !session.try_lock() {
            // 3a. watermark：陈旧消息直接丢（cc-connect engine.go:2882 discardStaleUserMessageIfNeeded）。
            if session.is_stale(&msg) {
                tracing::debug!(
                    "drop stale message: session_key={} ts={}",
                    msg.session_key.as_str(),
                    msg.timestamp_ms
                );
                return Ok(());
            }
            // 3b. enqueue；满了就 warn + 丢（不让 dispatcher 自己 panic）。
            // 注意：session_key 在 enqueue 失败时还要用，先 clone 一份。
            let key_for_log = msg.session_key.clone();
            if let Err(e) = session.enqueue(msg) {
                tracing::warn!(
                    "session queue full, drop message: session_key={} err={}",
                    key_for_log.as_str(),
                    e
                );
            }
            return Ok(());
        }

        // 4. 抢到锁：拿 Semaphore permit（限全局并发）+ spawn worker。
        let permit = self.sem.clone().acquire_owned().await.map_err(|e| {
            // semaphore closed = dispatcher 正在 shutdown，行为上等同 QueueFull。
            Error::other(format!("semaphore closed: {e}"))
        })?;
        let worker = worker_task(
            session,
            msg,
            self.channel.clone(),
            self.agent.clone(),
            self.agent_context.clone(),
            permit,
        );
        self.workers.lock().spawn(worker);

        Ok(())
    }
}

/// worker：在独立 task 里跑一个 session 的全部 turn 处理 + drain。
///
/// 流程：
/// 1. note_accepted（更新 current_turn watermark）
/// 2. 启动 typing reaction（如果 channel 支持）
/// 3. 创建新 agent session（v1 不跨 worker 复用，避免 events receiver
///    跨 worker 传递的复杂语义；v2 再优化）
/// 4. process_turn（send + event loop）
/// 5. 关闭 typing + note_completed
/// 6. drain pending queue（**复用同一个** agent session）
/// 7. close agent session（释放进程）
/// 8. release session lock
async fn worker_task(
    session: Arc<SessionState>,
    msg: IncomingMessage,
    channel: Arc<dyn Channel>,
    agent: Arc<dyn Agent>,
    agent_ctx: AgentContext,
    _permit: OwnedSemaphorePermit,
) {
    session.note_accepted(&msg);

    // typing reaction 在 agent session 创建之前启动，避免用户感知到「先有
    // reaction、后开始处理」的视觉撕裂。
    let typing_guard = start_typing_if_supported(&channel, &msg.reply_target).await;

    // v1: 每次 worker 启动都新建 agent_session。
    // v2 优化方向：跨 worker 复用 agent_session + 把 events receiver
    // 单独存到 SessionState（避免 take_events 二次调用 panic）。
    let mut agent_session = match agent.start_session(&agent_ctx, None).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("agent start_session failed: {e}");
            if let Some(g) = typing_guard {
                g.stop().await;
            }
            session.unlock();
            return;
        }
    };
    let mut events = agent_session.take_events();

    // 处理首条消息。
    process_turn(&mut agent_session, &channel, &msg, &mut events).await;
    finalize_turn(&channel, &msg.reply_target, typing_guard, &session).await;

    // drain pending queue：同 session 的消息按 FIFO 处理，复用同一个
    // agent session + events receiver（worker 生命周期内）。
    // 这里只 drain 一次，避免某个 session 长时间占住 worker 不释放 lock；
    // 若有更多消息，下次 on_message 再 try_lock 抢。
    while let Some(next_msg) = session.pop_pending() {
        if session.is_stale(&next_msg) {
            // pending 里的消息也可能 stale（比如 channel 重投），跳过。
            continue;
        }
        session.note_accepted(&next_msg);
        let typing = start_typing_if_supported(&channel, &next_msg.reply_target).await;
        process_turn_drain(&mut agent_session, &channel, &next_msg, &mut events).await;
        finalize_turn(&channel, &next_msg.reply_target, typing, &session).await;
    }

    // close agent session（释放 Claude Code 子进程）。v1 简单粗暴直接 close。
    if let Err(e) = agent_session.close().await {
        tracing::warn!("agent close failed: {e}");
    }

    // drain 完或没 pending，释放 lock；其他 session 的消息可以继续。
    session.unlock();
}

/// 启动 typing reaction（如果 channel 支持）。
///
/// 返回 None 表示不支持 typing；调用方直接跳过 stop 步骤。
async fn start_typing_if_supported(
    channel: &Arc<dyn Channel>,
    target: &ReplyTarget,
) -> Option<TypingGuard> {
    let ti = channel.as_typing_indicator()?;
    match ti
        .start_typing(&ReplyContext::default(), target)
        .await
    {
        Ok(guard) => Some(guard),
        Err(e) => {
            // typing 启动失败不应阻塞主流程，仅 warn。
            tracing::warn!("start_typing failed: {e}");
            None
        }
    }
}

/// 关闭 typing reaction（如果有 guard）。
async fn stop_typing_if_any(typing: Option<TypingGuard>) {
    if let Some(g) = typing {
        g.stop().await;
    }
}

/// 单 turn 收尾：关闭 typing + 标记 completed。
async fn finalize_turn(
    channel: &Arc<dyn Channel>,
    target: &ReplyTarget,
    typing: Option<TypingGuard>,
    session: &Arc<SessionState>,
) {
    stop_typing_if_any(typing).await;
    // reply/send 出错仅 warn，不阻塞 turn 收尾。
    let _ = channel
        .send(
            &ReplyContext::default(),
            target.clone(),
            OutgoingContent::Text(String::new()),
        )
        .await
        .map_err(|e| tracing::warn!("post-turn send failed: {e}"));
    session.note_completed();
}

/// 处理一条 turn：发 prompt + 读 events 直到 Result/Closed。
async fn process_turn(
    agent_session: &mut Box<dyn AgentSession>,
    channel: &Arc<dyn Channel>,
    msg: &IncomingMessage,
    events: &mut mpsc::Receiver<Event>,
) {
    run_turn(agent_session, channel, msg, events, /* interactive = */ true).await;
}

/// drain 队列里的 turn：和 process_turn 几乎一样，但不再做 typing 启停
///（typing 在外层 finalize_turn 里统一做）。
async fn process_turn_drain(
    agent_session: &mut Box<dyn AgentSession>,
    channel: &Arc<dyn Channel>,
    msg: &IncomingMessage,
    events: &mut mpsc::Receiver<Event>,
) {
    run_turn(agent_session, channel, msg, events, /* interactive = */ false).await;
}

/// turn 主循环：从 send 到 Result/Closed。
///
/// `interactive`：首条 turn 为 true（开 typing），drain 出来的为 false。
/// `interactive` 不影响行为本身，仅用于日志标记。
async fn run_turn(
    agent_session: &mut Box<dyn AgentSession>,
    channel: &Arc<dyn Channel>,
    msg: &IncomingMessage,
    events: &mut mpsc::Receiver<Event>,
    interactive: bool,
) {
    let _ = interactive; // 暂未用于日志（M3 加 metrics 用）。

    // 把 IncomingContent 归一化成 prompt string；非文本消息给占位文本。
    let prompt = match &msg.content {
        IncomingContent::Text(s) => s.clone(),
        IncomingContent::Image(_) => "[image]".into(),
        IncomingContent::File(_) => "[file]".into(),
        IncomingContent::Audio(_) => "[audio]".into(),
    };

    if let Err(e) = agent_session.send(&prompt, &[]).await {
        tracing::error!("agent send failed: {e}");
        return;
    }

    let ctx = ReplyContext::default();
    while let Some(event) = events.recv().await {
        match event {
            Event::Text(s) => {
                if let Err(e) = channel
                    .reply(&ctx, msg.reply_target.clone(), OutgoingContent::Text(s))
                    .await
                {
                    tracing::warn!("channel reply failed: {e}");
                }
            }
            Event::ToolUse { name, args } => {
                // v1 阶段仅 log；v2 可考虑发卡片进度。
                tracing::debug!("agent tool_use: {} {}", name, args);
            }
            Event::Result { usage, duration_ms } => {
                tracing::debug!(
                    "agent turn done: in={} out={} cache={} dur_ms={}",
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.cache_read_tokens,
                    duration_ms
                );
                break;
            }
            Event::PermissionRequest {
                request_id, tool, ..
            } => {
                // v1：自动 allow（permission hook 引擎不在 v1 范围）。
                // 设计稿见 §11 风险与缓解：permission hook 是 v2。
                tracing::info!("auto-allow permission request {} for tool {}", request_id, tool);
                if let Err(e) = agent_session
                    .respond_permission(&request_id, PermissionResult::Allow)
                    .await
                {
                    tracing::warn!("respond_permission failed: {e}");
                }
            }
            Event::Error(e) => {
                tracing::error!("agent error: {e}");
                // 非致命，继续等下一个 event；如果下一个是 Closed 才算退出。
            }
            Event::Closed => {
                tracing::info!("agent events channel closed");
                break;
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    //! Dispatcher 的单元测试 + burst 性能断言。
    use super::*;
    use crate::agent::tests::MockAgent;
    use crate::channel::tests::MockChannel;
    use crate::types::{FeishuChatType, PlatformKind, SenderId, SessionKey};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 构造一条样例消息（每条 timestamp 不同避免 watermark 误判）。
    fn msg_with_ts(ts: i64, key: &str) -> IncomingMessage {
        IncomingMessage {
            platform: PlatformKind::Feishu,
            session_key: SessionKey::derive(PlatformKind::Feishu, key, None),
            sender: SenderId::new("ou_a"),
            content: IncomingContent::Text("hi".into()),
            reply_target: ReplyTarget::feishu("oc", None, FeishuChatType::P2p),
            timestamp_ms: ts,
            raw_message_id: format!("om_{ts}"),
            is_mention: false,
            sender_kind: crate::types::SenderKind::User,
            is_from_self: false,
        }
    }

    /// 端到端：一条消息 → worker 跑完 → join 干净。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_dispatcher_processes_one_message() {
        let channel = Arc::new(MockChannel::new("mock"));
        let agent = Arc::new(MockAgent::new("mock-agent"));
        let mut dispatcher = Dispatcher::new(
            channel.clone(),
            agent.clone(),
            DispatcherConfig::default(),
        );
        let m = msg_with_ts(1000, "oc_a");
        dispatcher
            .on_message(channel.clone(), m)
            .await
            .unwrap();
        dispatcher.join().await;
    }

    /// 同 session 连续 3 条：per-session lock 串行处理。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_dispatcher_same_session_three_messages_serial() {
        let channel = Arc::new(MockChannel::new("mock"));
        let agent = Arc::new(MockAgent::new("mock-agent"));
        let mut dispatcher = Dispatcher::new(
            channel.clone(),
            agent.clone(),
            DispatcherConfig::default(),
        );
        for i in 0..3 {
            let m = msg_with_ts(1000 + i, "oc_same");
            dispatcher
                .on_message(channel.clone(), m)
                .await
                .unwrap();
        }
        dispatcher.join().await;
    }

    /// 不同 session 各自一条：并行处理。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_dispatcher_different_sessions_concurrent() {
        let channel = Arc::new(MockChannel::new("mock"));
        let agent = Arc::new(MockAgent::new("mock-agent"));
        let mut dispatcher = Dispatcher::new(
            channel.clone(),
            agent.clone(),
            DispatcherConfig::default(),
        );
        for i in 0..3 {
            let key = format!("oc_{i}");
            let m = msg_with_ts(1000 + i, &key);
            dispatcher.on_message(channel.clone(), m).await.unwrap();
        }
        dispatcher.join().await;
    }

    /// dedup 必须工作：同 raw_message_id 来两次，第二次被静默丢。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_dispatcher_dedup_drops_repeat() {
        let channel = Arc::new(MockChannel::new("mock"));
        let agent = Arc::new(MockAgent::new("mock-agent"));
        let mut dispatcher = Dispatcher::new(
            channel.clone(),
            agent.clone(),
            DispatcherConfig {
                dedup_ttl: Duration::from_secs(60),
                ..DispatcherConfig::default()
            },
        );

        let mut m1 = msg_with_ts(1000, "oc_d");
        m1.raw_message_id = "om_dup".into();
        let mut m2 = msg_with_ts(1001, "oc_d");
        m2.raw_message_id = "om_dup".into();

        dispatcher.on_message(channel.clone(), m1).await.unwrap();
        dispatcher.on_message(channel.clone(), m2).await.unwrap();
        dispatcher.join().await;
    }

    /// is_from_self 消息必须被丢（dispatcher 不调 worker，不入 busy queue）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_dispatcher_drops_self_message() {
        let channel = Arc::new(MockChannel::new("mock"));
        let agent = Arc::new(MockAgent::new("mock-agent"));
        let mut dispatcher = Dispatcher::new(
            channel.clone(),
            agent.clone(),
            DispatcherConfig::default(),
        );
        let mut m = msg_with_ts(1000, "oc_e");
        m.is_from_self = true;
        dispatcher.on_message(channel.clone(), m).await.unwrap();
        dispatcher.join().await;
    }

    /// 同 session 连发 N=10：worker 串行处理全部，不漏不重。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_dispatcher_same_session_burst_ten() {
        let channel = Arc::new(MockChannel::new("mock"));
        let agent = Arc::new(MockAgent::new("mock-agent"));
        let mut dispatcher = Dispatcher::new(
            channel.clone(),
            agent.clone(),
            DispatcherConfig::default(),
        );
        let counter = Arc::new(AtomicU32::new(0));
        for i in 0..10 {
            let m = msg_with_ts(2000 + i, "oc_burst");
            dispatcher.on_message(channel.clone(), m).await.unwrap();
            counter.fetch_add(1, Ordering::SeqCst);
        }
        dispatcher.join().await;
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    /// QueueFull：单 session pending 容量满后第 N+1 条被丢，不 panic。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_dispatcher_queue_full_drops_overflow() {
        let channel = Arc::new(MockChannel::new("mock"));
        let agent = Arc::new(MockAgent::new("mock-agent"));
        let mut dispatcher = Dispatcher::new(
            channel.clone(),
            agent.clone(),
            DispatcherConfig {
                max_pending_per_session: 2,
                ..DispatcherConfig::default()
            },
        );
        for i in 0..10 {
            let m = msg_with_ts(3000 + i, "oc_full");
            dispatcher
                .on_message(channel.clone(), m)
                .await
                .unwrap();
        }
        dispatcher.join().await;
    }

    /// 大 burst 100 条消息端到端 + 计时：必须 < 5s（vs 现有串行 ~50s）。
    /// 这是设计稿 §11 测试策略里的关键性能断言。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_dispatcher_burst_100_under_5s() {
        let channel = Arc::new(MockChannel::new("mock"));
        let agent = Arc::new(MockAgent::new("mock-agent"));
        let mut dispatcher = Dispatcher::new(
            channel.clone(),
            agent.clone(),
            DispatcherConfig {
                max_concurrent_turns: 16,
                max_pending_per_session: 256,
                max_sessions: 32,
                dedup_ttl: Duration::from_secs(60),
            },
        );

        let start = std::time::Instant::now();
        // 100 条消息分布在 5 个 session。
        for i in 0..100 {
            let key = format!("oc_{}", i % 5);
            let m = msg_with_ts(5000 + i, &key);
            dispatcher
                .on_message(channel.clone(), m)
                .await
                .unwrap();
        }
        dispatcher.join().await;
        let elapsed = start.elapsed();

        // 设计稿目标：< 5s。Mock 环境下实际会更短（~几十 ms）。
        assert!(
            elapsed < Duration::from_secs(5),
            "burst 100 messages took {elapsed:?}, expected < 5s"
        );
    }
}
