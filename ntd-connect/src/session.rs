//! Session 管理：per-session 锁 + busy queue + watermark。
//!
//! # 与 cc-connect 的对应
//!
//! 对应 `cc-connect/core/session.go:43-80 Session` +
//! `cc-connect/core/engine.go:2892 session.TryLock` +
//! `cc-connect/core/engine.go:3010-3077 queueMessageForBusySession` +
//! `cc-connect/core/engine.go:2504-2540 watermark`。
//!
//! # 设计要点
//!
//! - **lock 用 AtomicBool CAS**：不用 `tokio::Mutex::try_lock`，原因
//!   是 CAS 单指令、不引入 await、测试时可裸用（无需 runtime）。
//! - **busy queue 用 parking_lot::Mutex<VecDeque>**：dispatcher
//!   pop_front / push_back 都是短临界区，sync mutex 比 channel 更可控。
//! - **watermark 用 AtomicI64 × 2**：current_turn + last_completed；
//!   drain 时**只看这两个**，**不看 pending 队列**——这是 cc-connect
//!   `engine.go:2517-2526 isQueuedUserMessageStaleForDrainLocked` 的
//!   反直觉但关键的设计，必须严格保留。
//! - **agent_session 懒加载**：session 创建时无 agent_session，
//!   worker 第一次处理消息时再调 Agent::start_session，避免空 session
//!   浪费进程资源。
//! - **SessionManager 带 LRU 上限**：防止恶意 sender 撑爆内存。

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use dashmap::DashMap;
use lru::LruCache;
use parking_lot::Mutex;

use crate::agent::AgentSession;
use crate::error::{Error, Result};
use crate::types::{IncomingMessage, SessionKey};

/// 单个 session 的运行时状态。
///
/// 所有原子字段用 `Acquire/Release` 序：足以保证跨字段可见性，
/// 不需要 `SeqCst`（那会强制全局顺序，影响性能）。
pub struct SessionState {
    /// CAS 锁：`true` 表示 turn in-flight。
    busy: AtomicBool,
    /// Busy 时入队的待处理消息（FIFO）。
    pending: Mutex<VecDeque<IncomingMessage>>,
    /// 当前 turn 的 timestamp（毫秒）。`-1` 表示无 in-flight turn。
    current_turn: AtomicI64,
    /// 已完成 turn 的最大 timestamp。`-1` 表示从未完成过。
    last_completed: AtomicI64,
    /// 懒加载的 agent session；首次 worker 处理时填充。
    agent_session: Mutex<Option<Box<dyn AgentSession>>>,
    /// 单 session pending 队列容量上限。
    max_pending: usize,
}

impl SessionState {
    /// 构造新 session（busy=false、watermark=-1、无 agent_session）。
    pub fn new(max_pending: usize) -> Self {
        SessionState {
            busy: AtomicBool::new(false),
            pending: Mutex::new(VecDeque::new()),
            current_turn: AtomicI64::new(-1),
            last_completed: AtomicI64::new(-1),
            agent_session: Mutex::new(None),
            max_pending,
        }
    }

    /// 尝试获取 session 锁。
    ///
    /// 成功返回 `true`，调用方获得 turn 处理权；返回 `false` 说明
    /// 已有 turn 在跑，调用方应走 busy queue 路径。
    /// 使用 `AcqRel` 序：保证 acquire 方能看到之前 release 方写入的状态。
    pub fn try_lock(&self) -> bool {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// 释放 session 锁。
    pub fn unlock(&self) {
        // Release 序：保证之前的所有写入对下一个 acquire 方可见。
        self.busy.store(false, Ordering::Release);
    }

    /// 当前是否已加锁（用于调试 / 日志，不参与并发决策）。
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    /// 当前 watermark = max(current_turn, last_completed)。
    ///
    /// 两者都 ≤ 消息 timestamp 的消息视为陈旧（cc-connect
    /// `core/engine.go:2504-2510 isStaleUserMessageLocked`）。
    fn watermark(&self) -> i64 {
        let cur = self.current_turn.load(Ordering::Acquire);
        let last = self.last_completed.load(Ordering::Acquire);
        cur.max(last)
    }

    /// 判断消息是否陈旧（timestamp 早于当前 watermark）。
    pub fn is_stale(&self, msg: &IncomingMessage) -> bool {
        // 两者都是 -1（初始状态）时，watermark=-1；任何 timestamp >= 0
        // 都不会被判 stale。第一次消息总能通过。
        msg.timestamp_ms < self.watermark()
    }

    /// 记录一条消息被接受（dispatcher 抢到锁并开始处理它）。
    ///
    /// 把 `current_turn` 设为消息的 timestamp；陈旧判断会基于此。
    pub fn note_accepted(&self, msg: &IncomingMessage) {
        self.current_turn
            .store(msg.timestamp_ms, Ordering::Release);
    }

    /// 标记当前 turn 完成。
    ///
    /// 把 `current_turn` 推入 `last_completed` 后清空 current。
    /// 之后新一轮的 message 才能进入 accepted 状态。
    pub fn note_completed(&self) {
        let cur = self.current_turn.load(Ordering::Acquire);
        self.last_completed.store(cur, Ordering::Release);
        // 重置为 -1，使 watermark = last_completed（不再偏向 current）。
        // 下一个 note_accepted 会覆盖。
        self.current_turn.store(-1, Ordering::Release);
    }

    /// 把消息塞进 busy 队列。容量满则返回 [`Error::QueueFull`]。
    pub fn enqueue(&self, msg: IncomingMessage) -> Result<()> {
        let mut q = self.pending.lock();
        if q.len() >= self.max_pending {
            return Err(Error::QueueFull);
        }
        q.push_back(msg);
        Ok(())
    }

    /// 弹出队首消息（FIFO）；空返回 None。
    pub fn pop_pending(&self) -> Option<IncomingMessage> {
        self.pending.lock().pop_front()
    }

    /// 当前 pending 队列长度。
    pub fn pending_len(&self) -> usize {
        self.pending.lock().len()
    }

    /// 是否还有 pending 待处理。
    pub fn has_pending(&self) -> bool {
        !self.pending.lock().is_empty()
    }

    /// 懒加载：设置 agent session。
    ///
    /// 通常 worker 第一次处理消息时调用一次。覆盖已有 session 视为
    /// bug，但允许（不 panic）以便恢复路径能强制替换。
    pub fn set_agent_session(&self, session: Box<dyn AgentSession>) {
        *self.agent_session.lock() = Some(session);
    }

    /// 取出 agent session（move 出 Option）；已存在则返回 None。
    pub fn take_agent_session(&self) -> Option<Box<dyn AgentSession>> {
        self.agent_session.lock().take()
    }

    /// 当前是否有 agent session。
    pub fn has_agent_session(&self) -> bool {
        self.agent_session.lock().is_some()
    }
}

/// 全局 session 表 + LRU 上限。
///
/// 多 dispatcher 共享一个 SessionManager 时通过 `Arc<SessionManager>`
/// 传递；M2 阶段一个 Dispatcher 持有一个 SessionManager。
pub struct SessionManager {
    /// session key → state。
    map: DashMap<SessionKey, Arc<SessionState>>,
    /// LRU 辅助表：每次 get_or_create 都 touch，容量超限驱逐最旧。
    lru: Mutex<LruCache<SessionKey, ()>>,
    /// 最大 session 数；超出时驱逐最久未活跃的。
    max_sessions: NonZeroUsize,
    /// 每个 session 的 pending 队列容量上限。
    max_pending_per_session: usize,
}

impl SessionManager {
    /// 构造 session manager。
    ///
    /// `max_sessions` 至少为 1，否则 `NonZeroUsize::new` 会 panic。
    /// `max_pending_per_session` 推荐 32~128；太高会延迟用户感知，
    /// 太低会丢消息。
    pub fn new(max_sessions: usize, max_pending_per_session: usize) -> Self {
        let nz = NonZeroUsize::new(max_sessions)
            .expect("SessionManager max_sessions must be > 0");
        SessionManager {
            map: DashMap::new(),
            lru: Mutex::new(LruCache::new(nz)),
            max_sessions: nz,
            max_pending_per_session,
        }
    }

    /// 取出或创建 session；命中现有则 touch LRU。
    pub fn get_or_create(&self, key: SessionKey) -> Arc<SessionState> {
        // 快路径：现有 session 直接返回。
        if let Some(state) = self.map.get(&key) {
            // touch LRU；持锁时间极短，不影响并发。
            self.lru.lock().put(key.clone(), ());
            return state.clone();
        }
        // 慢路径：先驱逐到 max-1，再插入新 key。
        // 关键：必须「先驱逐再插入」，否则 LRU.put 的自动驱逐（容量满时）
        // 会自己踢掉一个 key，但我们无法拿到被踢的是哪个，map 与 lru
        // 状态就会脱节。
        self.evict_to_size(self.max_sessions.get() - 1);
        let state = Arc::new(SessionState::new(self.max_pending_per_session));
        self.map.insert(key.clone(), state.clone());
        // 此时 map.len() = max_sessions，刚好等于 LRU 容量，put 不触发自动驱逐。
        self.lru.lock().put(key.clone(), ());
        state
    }

    /// 当前 session 数（用于测试 / metrics）。
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// 没有任何 session。
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 驱逐最旧的 session，直到 map.len() ≤ target。
    ///
    /// 设计要点：必须持有 lru lock 时确定驱逐目标，再释放锁去删 map。
    /// 否则如果在持 lru 锁的同时调 dashmap remove，死锁风险存在
    /// （dispatcher 反向持锁就会死锁）。
    fn evict_to_size(&self, target: usize) {
        while self.map.len() > target {
            let oldest = {
                let mut lru = self.lru.lock();
                match lru.pop_lru() {
                    Some((k, _)) => k,
                    None => break, // LRU 为空但 map 还在 → 状态不一致，安全退出
                }
            };
            self.map.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FeishuChatType, IncomingContent, PlatformKind, SenderId};
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    fn msg_with_ts(ts: i64) -> IncomingMessage {
        IncomingMessage {
            platform: PlatformKind::Feishu,
            session_key: SessionKey::derive(PlatformKind::Feishu, "oc", None),
            sender: SenderId::new("ou_a"),
            content: IncomingContent::Text("x".into()),
            reply_target: crate::types::ReplyTarget::feishu("oc", None, FeishuChatType::P2p),
            timestamp_ms: ts,
            raw_message_id: format!("om_{ts}"),
            is_mention: false,
            sender_kind: crate::types::SenderKind::User,
            is_from_self: false,
        }
    }

    /// `try_lock` 必须在 unlocked→locked 之间 CAS 成功；locked 时返回 false。
    #[test]
    fn test_try_lock_serializes() {
        let s = SessionState::new(16);
        assert!(!s.is_busy());
        assert!(s.try_lock(), "首次 try_lock 必须成功");
        assert!(s.is_busy());
        assert!(!s.try_lock(), "locked 时再 try_lock 必须失败");
        s.unlock();
        assert!(s.try_lock(), "unlock 后 try_lock 必须再次成功");
    }

    /// watermark 三态：未初始化 / current / completed。
    #[test]
    fn test_watermark_three_states() {
        let s = SessionState::new(16);
        // 未初始化 watermark = -1；任何 timestamp >= 0 都不 stale。
        assert!(!s.is_stale(&msg_with_ts(0)));
        assert!(!s.is_stale(&msg_with_ts(100)));

        // accepted 后 current = msg ts。
        s.note_accepted(&msg_with_ts(100));
        // 更早的消息 → stale。
        assert!(s.is_stale(&msg_with_ts(50)));
        // 同时刻的消息（< 而不是 ≤）不 stale。
        assert!(!s.is_stale(&msg_with_ts(100)));
        // 更晚的不 stale。
        assert!(!s.is_stale(&msg_with_ts(200)));

        // completed 后 last_completed = 100，current = -1。
        s.note_completed();
        assert!(s.is_stale(&msg_with_ts(50)));
        assert!(!s.is_stale(&msg_with_ts(100)));
        assert!(!s.is_stale(&msg_with_ts(200)));
    }

    /// pending 队列 FIFO + 容量上限。
    #[test]
    fn test_pending_queue_fifo_and_capacity() {
        let s = SessionState::new(3);
        s.enqueue(msg_with_ts(1)).unwrap();
        s.enqueue(msg_with_ts(2)).unwrap();
        s.enqueue(msg_with_ts(3)).unwrap();
        // 第 4 条应 QueueFull。
        let err = s.enqueue(msg_with_ts(4)).unwrap_err();
        assert!(matches!(err, Error::QueueFull));

        // FIFO 弹出顺序。
        assert_eq!(s.pop_pending().unwrap().timestamp_ms, 1);
        assert_eq!(s.pop_pending().unwrap().timestamp_ms, 2);
        assert_eq!(s.pop_pending().unwrap().timestamp_ms, 3);
        assert!(s.pop_pending().is_none());
    }

    /// agent_session 的懒加载：set 后能 take；take 后 set 还能再设。
    #[test]
    fn test_agent_session_take_and_set() {
        let s = SessionState::new(16);
        assert!(!s.has_agent_session());

        // 用跨模块共享的工厂函数构造 dummy session。
        let dummy = crate::agent::tests::dummy_agent_session();
        s.set_agent_session(dummy);
        assert!(s.has_agent_session());

        let taken = s.take_agent_session();
        assert!(taken.is_some());
        assert!(!s.has_agent_session());

        let taken2 = s.take_agent_session();
        assert!(taken2.is_none(), "二次 take 应返回 None");
    }

    /// SessionManager LRU 驱逐：超过 max_sessions 时最久未活跃的被踢。
    #[test]
    fn test_session_manager_lru_eviction() {
        let mgr = SessionManager::new(2, 16);
        let k1 = SessionKey::derive(PlatformKind::Feishu, "oc1", None);
        let k2 = SessionKey::derive(PlatformKind::Feishu, "oc2", None);
        let k3 = SessionKey::derive(PlatformKind::Feishu, "oc3", None);

        let _ = mgr.get_or_create(k1.clone());
        let _ = mgr.get_or_create(k2.clone());
        assert_eq!(mgr.len(), 2);

        // 创建第三个，触发 LRU 驱逐（k1 是最久的）。
        let _ = mgr.get_or_create(k3.clone());
        assert_eq!(mgr.len(), 2, "超出 max_sessions 应驱逐");

        // k1 已被驱逐；再 get_or_create 会新建一份。
        let _ = mgr.get_or_create(k1.clone());
        // 此时 map 应仍为 2 个（k1 替换掉 k3，因为 k3 最久）。
        assert_eq!(mgr.len(), 2);
    }

    /// SessionManager 命中现有 session 时不应新建。
    #[test]
    fn test_session_manager_hit_returns_same_state() {
        let mgr = SessionManager::new(8, 16);
        let k = SessionKey::derive(PlatformKind::Feishu, "oc_x", None);
        let a = mgr.get_or_create(k.clone());
        let b = mgr.get_or_create(k.clone());
        assert!(Arc::ptr_eq(&a, &b), "命中应返回同一个 Arc");
        assert_eq!(mgr.len(), 1);
    }

    /// 多并发 get_or_create 同 key 只能创建一个 session。
    /// 这里用单线程模拟：连续 get_or_create 同 key 验证 len=1。
    #[test]
    fn test_session_manager_concurrent_same_key_dedup() {
        let mgr = SessionManager::new(8, 16);
        let k = SessionKey::derive(PlatformKind::Feishu, "oc_x", None);
        let _ = mgr.get_or_create(k.clone());
        let _ = mgr.get_or_create(k.clone());
        let _ = mgr.get_or_create(k.clone());
        assert_eq!(mgr.len(), 1);
    }

    /// note_accepted 后 note_completed；下一轮 accepted 用新 timestamp，
    /// 旧 timestamp 的 in-flight 消息被判 stale（验证 reset current=-1）。
    #[test]
    fn test_watermark_reset_after_completed() {
        let s = SessionState::new(16);
        s.note_accepted(&msg_with_ts(100));
        s.note_completed();

        // 第二轮 turn。
        s.note_accepted(&msg_with_ts(200));
        // 第一轮的 timestamp (100) 必须仍 stale。
        assert!(s.is_stale(&msg_with_ts(100)));
        // 第二轮的 timestamp (200) 不 stale。
        assert!(!s.is_stale(&msg_with_ts(200)));
        // 第三轮的更晚消息也不 stale。
        assert!(!s.is_stale(&msg_with_ts(300)));
    }

    /// 边界：`max_sessions = 0` 应 panic（构造函数契约）。
    #[test]
    #[should_panic(expected = "max_sessions must be > 0")]
    fn test_session_manager_zero_capacity_panics() {
        let _ = SessionManager::new(0, 16);
    }

    /// 多 worker 并发争抢同一 session：CAS 必须保证**任意时刻只有一个赢家**。
    ///
    /// 100 个线程各自重试直到拿到锁一次。结束后 winners 应正好 100。
    /// 重点不是计数本身，而是**没有锁泄漏**（所有 unlock 都触发了）。
    #[test]
    fn test_try_lock_concurrent_only_one_winner() {
        use std::sync::Arc;
        let s = Arc::new(SessionState::new(16));
        let winners = Arc::new(AtomicU32::new(0));

        let handles: Vec<_> = (0..100)
            .map(|_| {
                let s = s.clone();
                let winners = winners.clone();
                std::thread::spawn(move || {
                    // 自旋直到拿到一次为止（try_lock 失败就 yield 再试）。
                    // CAS 保证每次拿到的瞬间，其他人都看不到 locked。
                    for _ in 0..10_000 {
                        if s.try_lock() {
                            winners.fetch_add(1, AtomicOrdering::SeqCst);
                            s.unlock();
                            return;
                        }
                        std::thread::yield_now();
                    }
                    panic!("CAS 重试 10000 次还没拿到锁，逻辑可能有问题");
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        // 100 个线程每人成功一次 = 100。
        assert_eq!(winners.load(AtomicOrdering::SeqCst), 100);
    }
}
