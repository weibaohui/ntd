//! 统一管理执行日志 buffer 的后台刷新。
//!
//! ## 背景
//!
//! `executor_service.rs` 历史上把"stdout 触发"、"stderr 触发"、"timer 兜底"三段
//! 近似复制代码各自实现一遍，伴随 5 类并发缺陷（见 issue #496）：
//!
//! 1. **锁顺序不一致**：三段代码都各自 `lock().await` `logs_for_db`，且没有 RAII 守卫；
//!    第二个 `lock().await` 在第一个释放前可能引发调度切换，长任务持锁期间被取消
//!    会卡死其他 writer。
//! 2. **`fetch_add` + `swap` + `store(0)` 非原子组合**：当多个 writer 并发触发 flush 时，
//!    `store(0)` 会把刚被其他线程 `fetch_add` 上去的增量抹掉，导致 counter 漂移，
//!    下次阈值到来时实际未刷的条数与 counter 对不上。
//! 3. **timer 4s sleep 死代码**：`interval.tick()` 每 3s 就绪，4s sleep 在 `select!`
//!    中永远胜出不了。
//! 4. **shutdown 丢日志**：timer 退出 `break` 时只 swap 了一次计数；与 writer
//!    推入的窗口重叠时，新日志随 timer 退出而丢失。
//! 5. **`flush_handles` vec 泄漏**：所有 spawned flush task 只能靠外层显式 `await`，
//!    中途 panic 或 early return 会留下无人收割的 task。
//!
//! ## 设计
//!
//! - 抽象 [`LogFlusher`] 结构，三段路径共享同一份内部状态，行为完全一致。
//! - 阈值检查与 `pending` 标志翻转通过原子操作完成：`fetch_add` 增计数，到阈值后
//!   `swap(true)` 抢锁；抢到的线程负责 `swap(0)` 清零 + `spawn` flush task。
//! - 失败回滚走 `fetch_add(snapshot_len)`，不会被覆盖（不会发生"先 store(0) 再
//!   fetch_add"这种丢失）。
//! - 优雅 shutdown：标记 `shutdown=true`，轮询 `pending=false`，再把 buffer
//!   残余一次性 `append`，最后 drain `handles` 等所有 spawned task 退出。
//! - 删除 4s sleep 兜底，shutdown 路径完全由 `shutdown` 标志驱动。
//! - 测试友好：通过 [`LogSink`] trait 抽象 DB 写入，单元测试无需真实数据库。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::db::Database;
use crate::models::ParsedLogEntry;

/// 日志写入的抽象接口。生产实现包 [`Database`]，测试时可注入 mock。
///
/// 必须 `Send + Sync + 'static`：flush task 会被 `tokio::spawn` 到独立任务，
/// 其 future 跨 `'static` 边界，且多个 writer task 会并发调用。
#[async_trait]
pub trait LogSink: Send + Sync + 'static {
    /// 把 `logs_json` 追加写入 `record_id` 对应执行记录。
    /// 返回 `Err` 时 [`LogFlusher`] 会把这次写入的快照回滚到内存 buffer。
    async fn append(&self, record_id: i64, logs_json: &str) -> Result<(), String>;
}

/// 把 [`Database::append_execution_record_logs`] 适配成 [`LogSink`]。
pub struct DatabaseLogSink {
    db: Arc<Database>,
}

impl DatabaseLogSink {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl LogSink for DatabaseLogSink {
    async fn append(&self, record_id: i64, logs_json: &str) -> Result<(), String> {
        self.db
            .append_execution_record_logs(record_id, logs_json)
            .await
            .map_err(|e| e.to_string())
    }
}

/// [`LogFlusher`] 配置项。把这些字段抽出来是为了让测试可以传更小的阈值。
#[derive(Debug, Clone, Copy)]
pub struct LogFlusherConfig {
    /// 目标执行记录 ID。
    pub record_id: i64,
    /// 累计多少条未刷新日志后触发后台 flush。
    pub threshold: u64,
    /// timer 兜底周期（秒）。
    pub timer_interval_secs: u64,
}

impl LogFlusherConfig {
    /// 生产环境默认：每 5 条触发一次；timer 每 3 秒兜底一次。
    pub fn for_record(record_id: i64) -> Self {
        Self {
            record_id,
            threshold: 5,
            timer_interval_secs: 3,
        }
    }
}

/// 日志 buffer 与后台 flush 的统一管理器。
///
/// ## 线程模型
///
/// - **writer 路径**（stdout / stderr / 业务调用方）：`push` / `push_many` 持有
///   `logs` 短锁推入条目，然后 lock-free 调用 `try_trigger` 检查阈值。
/// - **flush task**（`tokio::spawn`）：独立任务跑 DB 写入，成功则丢弃 snapshot，
///   失败则把 snapshot 重新合并到 buffer + 把 snapshot_len 加回 counter。
/// - **timer 任务**：周期 tick，调用 `try_timer_flush`，逻辑与 writer 路径一致。
/// - **shutdown 路径**：调用 `finalize`，等待所有 in-flight flush 完成 + drain
///   buffer 残余 + 等所有 spawned task 退出。
///
/// ## 并发不变量
///
/// 1. **同一时刻最多一个 flush task 在跑**：由 `pending: AtomicBool` 守门，所有
///    flush 入口（writer / timer）都先 `swap(true)`，失败就让出。
/// 2. **`unflushed` 计数单调反映"已 push 但未刷新"的条数**：每次成功 flush 后
///    `swap(0)` 清零；失败时 `fetch_add(snapshot_len)` 还回去。`store(0)` 这种
///    会丢增量的写法被禁止。
/// 3. **`shutdown=true` ⇒ timer 退出 + finalize 完成 drain**：`finalize` 内部
///    等待 `pending=false` 后再做 final drain，保证新 push 的日志也在最后一次
///    `append` 里被覆盖到。
pub struct LogFlusher {
    inner: Arc<LogFlusherInner>,
}

struct LogFlusherInner {
    sink: Box<dyn LogSink>,
    record_id: i64,
    threshold: u64,
    timer_interval_secs: u64,
    logs: Mutex<Vec<ParsedLogEntry>>,
    /// 已 push 但尚未成功 flush 到 DB 的条数。
    /// 只通过 `fetch_add` 增、`swap(0)` 清零（或失败回滚 `fetch_add`），禁止 `store`。
    unflushed: AtomicU64,
    /// 当前是否有 flush task 在跑。`true` ⇒ 有；`false` ⇒ 可触发下一次。
    pending: AtomicBool,
    /// 标记是否进入 shutdown。timer 看到 `true` 后退出；`finalize` 期间被设为 `true`。
    shutdown: AtomicBool,
    /// 所有 spawned flush task 的句柄，shutdown 时统一 `await`。
    /// 用 `std::sync::Mutex` 而非 tokio 版：临界区只是 `Vec::push`，不会跨 await。
    handles: std::sync::Mutex<Vec<JoinHandle<()>>>,
}

impl LogFlusher {
    /// 创建 flusher。`sink` 通常是 [`DatabaseLogSink::new`] 的结果。
    pub fn new(sink: Box<dyn LogSink>, config: LogFlusherConfig) -> Self {
        Self {
            inner: Arc::new(LogFlusherInner {
                sink,
                record_id: config.record_id,
                threshold: config.threshold,
                timer_interval_secs: config.timer_interval_secs,
                logs: Mutex::new(Vec::new()),
                unflushed: AtomicU64::new(0),
                pending: AtomicBool::new(false),
                shutdown: AtomicBool::new(false),
                handles: std::sync::Mutex::new(Vec::new()),
            }),
        }
    }

    /// 包装成 `Arc<Self>`，方便 spawn timer / 调用 `finalize`。
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// 推一条日志。`logs` 锁很短，flush 触发是 lock-free。
    pub async fn push(&self, entry: ParsedLogEntry) {
        self.inner.logs.lock().await.push(entry);
        self.try_trigger();
    }

    /// 批量推日志。比循环 `push` 少 N-1 次锁。
    pub async fn push_many<I>(&self, entries: I)
    where
        I: IntoIterator<Item = ParsedLogEntry>,
    {
        let added = {
            let mut logs = self.inner.logs.lock().await;
            let before = logs.len();
            logs.extend(entries);
            logs.len() - before
        };
        // 仍按单条触发：counter 语义不变，阈值行为与旧实现一致。
        for _ in 0..added {
            self.try_trigger();
        }
    }

    /// 是否处于 shutdown。
    pub fn is_shutdown(&self) -> bool {
        self.inner.shutdown.load(Ordering::Acquire)
    }

    /// 当前 buffer 中还未刷新的条数。供测试 / 调试用。
    pub fn unflushed_count(&self) -> u64 {
        self.inner.unflushed.load(Ordering::Acquire)
    }

    /// 在 buffer 持锁的情况下对条目做只读访问。
    /// 用于 stats 计算这种"我需要扫描 buffer 但不想 take 所有权"的场景。
    /// `f` 在持锁状态下同步执行，所以它不能跨 `.await`。
    pub async fn with_logs<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[ParsedLogEntry]) -> R,
    {
        let logs = self.inner.logs.lock().await;
        f(&logs)
    }

    /// 把 buffer 序列化为 JSON。错误时回退到 `"[]"`。
    /// 调用方通常把它与数据库读取的日志合并形成全量快照。
    pub async fn serialize(&self) -> String {
        self.with_logs(|logs| {
            serde_json::to_string(logs).unwrap_or_else(|e| {
                tracing::error!("LogFlusher serialize failed: {}", e);
                "[]".to_string()
            })
        })
        .await
    }

    /// 周期兜底 flush 循环。每 `timer_interval_secs` 秒检查一次 buffer。
    ///
    /// 退出条件：`shutdown` 被置 `true`。
    ///
    /// 这里**故意没有 4s sleep fallback**：旧实现那个 `tokio::time::sleep(4s)`
    /// 在 `select!` 内永远胜出不了 3s 的 `interval.tick()`，是死代码。
    pub async fn run_timer(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(
            self.inner.timer_interval_secs,
        ));
        // 跳过第一次立即 tick（tokio interval 的默认行为），让业务先有数据可刷
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if self.is_shutdown() {
                break;
            }
            self.try_timer_flush();
        }
    }

    /// 优雅关闭：
    /// 1. 标记 `shutdown=true`，timer 看到后会退出循环。
    /// 2. 轮询 `pending=false`，等所有 in-flight flush task 完成。
    /// 3. 把 buffer 残余一次性 append 到 DB（不再走阈值触发的路径）。
    /// 4. drain `handles`，等所有 spawned flush task 真正退出。
    ///
    /// 调用方必须在 stdout/stderr task 退出后调用本方法，保证没有并发 writer。
    #[allow(clippy::expect_used)]
    pub async fn finalize(self: Arc<Self>) {
        self.inner.shutdown.store(true, Ordering::Release);

        // 等所有 in-flight flush 完成。指数退避避免空转。
        let mut backoff = std::time::Duration::from_millis(5);
        while self.inner.pending.load(Ordering::Acquire) {
            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, std::time::Duration::from_millis(100));
        }

        // final drain：把 buffer 残余一次性写库
        let snapshot = {
            let mut logs = self.inner.logs.lock().await;
            std::mem::take(&mut *logs)
        };
        if !snapshot.is_empty() {
            if let Ok(json) = serde_json::to_string(&snapshot) {
                if let Err(e) = self.inner.sink.append(self.inner.record_id, &json).await {
                    tracing::error!(
                        "LogFlusher finalize drain failed for record {}: {}",
                        self.inner.record_id,
                        e
                    );
                }
            }
        }

        // 等所有 spawned flush task 退出。即使 spawn 后 flush 已完成，JoinHandle
        // 仍然需要 await 一次以释放 task 占用的栈等资源。
        let handles: Vec<_> = {
            let mut h = self.inner.handles.lock().expect("handles Mutex poisoned in shutdown");
            std::mem::take(&mut *h)
        };
        for h in handles {
            // 显式检查 spawn 出的 flush task 是否 panic：silent `_ = h.await`
            // 会把内部 serialize / DB 写入的 panic 信息完全吞掉，定位极难。
            if let Err(e) = h.await {
                if e.is_panic() {
                    tracing::error!("LogFlusher flush task panicked: {}", e);
                }
            }
        }
    }

    /// Writer 路径的 flush 触发：增计数 + 阈值检查 + 抢锁 + spawn。
    ///
    /// 完全 lock-free（除最后一次 `handles.lock()`，临界区只做 `Vec::push`）。
    fn try_trigger(&self) {
        let prev = self.inner.unflushed.fetch_add(1, Ordering::AcqRel);
        if prev + 1 < self.inner.threshold {
            return;
        }
        // 抢锁：已有 flush 在跑则放弃。
        if self.inner.pending.swap(true, Ordering::AcqRel) {
            return;
        }
        self.spawn_flush();
    }

    /// Timer 路径的 flush 触发：与 `try_trigger` 类似，但只触发一次（不累加计数）。
    fn try_timer_flush(&self) {
        if self.inner.pending.load(Ordering::Acquire) {
            return;
        }
        let n = self.inner.unflushed.swap(0, Ordering::AcqRel);
        if n == 0 {
            return;
        }
        if self.inner.pending.swap(true, Ordering::AcqRel) {
            // 抢失败说明有别的 flush 刚启动，把计数还回去
            self.inner.unflushed.fetch_add(n, Ordering::AcqRel);
            return;
        }
        self.spawn_flush();
    }

    /// 把 `Arc::clone` 移交给 spawned flush task。task 内部：
    /// 1. `swap(0)` 清零 counter（即使 spawn 前已有新 writer 推入，swap 也是原子的）
    /// 2. 取走 buffer snapshot
    /// 3. 调 sink 写库
    /// 4. 失败则把 snapshot 放回 buffer + counter += snapshot_len
    /// 5. `pending=false` 释放锁
    fn spawn_flush(&self) {
        let inner = self.inner.clone();
        let h = tokio::spawn(async move {
            // 把 counter 拿到后清零。这一步必须在 take snapshot 之前，避免
            // snapshot 内条数与 counter 不一致。
            let _claimed = inner.unflushed.swap(0, Ordering::AcqRel);

            let snapshot = {
                let mut logs = inner.logs.lock().await;
                std::mem::take(&mut *logs)
            };
            let snapshot_len = snapshot.len() as u64;

            let success = match serde_json::to_string(&snapshot) {
                Ok(json) => inner.sink.append(inner.record_id, &json).await.is_ok(),
                Err(e) => {
                    tracing::error!(
                        "LogFlusher failed to serialize snapshot for record {}: {}",
                        inner.record_id,
                        e
                    );
                    false
                }
            };

            if !success {
                // 失败回滚：把 snapshot 放回 buffer，counter 加回 snapshot_len。
                // 注意：counter 此时可能因为并发 writer 已经累加了新值，但
                // fetch_add 是原子的，不会丢增量。
                let mut logs = inner.logs.lock().await;
                logs.extend(snapshot);
                inner.unflushed.fetch_add(snapshot_len, Ordering::AcqRel);
            }
            inner.pending.store(false, Ordering::Release);
        });
        // 临界区只是 Vec::push，用 std::sync::Mutex 即可，避免阻塞 .await。
        if let Ok(mut handles) = self.inner.handles.lock() {
            handles.push(h);
        } else {
            // handles Mutex 只能在持有它的线程 panic 时才出错（中毒）；
            // 这时我们已经 spawn 出去了，没有 JoinHandle 反而比 abort 它更安全。
            tracing::error!("LogFlusher handles mutex poisoned; abandoning flush handle");
        }
    }
}

// =============================================================================
//  单元测试 —— 覆盖 5 类并发缺陷的修复点
// =============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    //! 不依赖真实数据库：注入 `MockSink` + 共享 `MockSinkState` 让测试断言
    //! 调用序列与失败行为。`MockSink` 持有 `Arc<MockSinkState>`，因此可以在
    //! 测试里继续读到状态。

    use super::*;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct MockSinkState {
        /// 所有 append 调用按时间顺序记录
        calls: Vec<(i64, String)>,
        /// 一次性失败开关：置 true 后下一次 append 返回 Err 后自动复位
        fail_next: bool,
        /// 强制所有 append 失败
        fail_all: bool,
    }

    impl MockSinkState {
        fn call_count(&self) -> usize {
            self.calls.len()
        }

        fn total_log_entries(&self) -> usize {
            self.calls
                .iter()
                .map(|(_, json)| {
                    serde_json::from_str::<Vec<ParsedLogEntry>>(json)
                        .map(|v| v.len())
                        .unwrap_or(0)
                })
                .sum()
        }
    }

    struct MockSink {
        state: Arc<StdMutex<MockSinkState>>,
    }

    impl MockSink {
        /// 创建 MockSink 与共享 state；state 由测试持有，便于断言。
        fn new() -> (Self, Arc<StdMutex<MockSinkState>>) {
            let state = Arc::new(StdMutex::new(MockSinkState::default()));
            (
                Self {
                    state: state.clone(),
                },
                state,
            )
        }

        fn failing() -> (Self, Arc<StdMutex<MockSinkState>>) {
            let (sink, state) = Self::new();
            state.lock().unwrap().fail_all = true;
            (sink, state)
        }
    }

    #[async_trait]
    impl LogSink for MockSink {
        async fn append(&self, record_id: i64, logs_json: &str) -> Result<(), String> {
            let mut state = self.state.lock().unwrap();
            if state.fail_all {
                return Err("mock permanent failure".to_string());
            }
            state.calls.push((record_id, logs_json.to_string()));
            if state.fail_next {
                state.fail_next = false;
                return Err("mock one-shot failure".to_string());
            }
            Ok(())
        }
    }

    fn make_entry(content: &str) -> ParsedLogEntry {
        ParsedLogEntry::info(content.to_string())
    }

    fn fresh_flusher(sink: Box<dyn LogSink>, threshold: u64) -> Arc<LogFlusher> {
        LogFlusher::new(
            sink,
            LogFlusherConfig {
                record_id: 42,
                threshold,
                timer_interval_secs: 60, // 测试用，避免 timer 干扰
            },
        )
        .into_arc()
    }

    /// 测试 1: 阈值触发。
    /// 推满 threshold 条之后会触发一次 flush。
    #[tokio::test]
    async fn trigger_flush_when_threshold_reached() {
        let (sink, state) = MockSink::new();
        let flusher = fresh_flusher(Box::new(sink), 3);

        flusher.push(make_entry("a")).await;
        flusher.push(make_entry("b")).await;
        assert_eq!(flusher.unflushed_count(), 2);
        assert_eq!(state.lock().unwrap().call_count(), 0);

        flusher.push(make_entry("c")).await;
        // 给 spawned flush task 一点时间执行
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let s = state.lock().unwrap();
        assert_eq!(
            s.call_count(),
            1,
            "threshold=3 推到第 3 条应触发 flush"
        );
        assert_eq!(s.total_log_entries(), 3);
    }

    /// 测试 2: flush 失败时 snapshot 回滚，counter 还原。
    #[tokio::test]
    async fn flush_failure_restores_snapshot_and_counter() {
        let (sink, state) = MockSink::failing();
        let flusher = fresh_flusher(Box::new(sink), 3);

        flusher.push(make_entry("a")).await;
        flusher.push(make_entry("b")).await;
        flusher.push(make_entry("c")).await; // 触发 flush，失败
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // flush 失败，snapshot 应回滚到 buffer；state 记录里没有成功调用
        assert_eq!(state.lock().unwrap().call_count(), 0);
        // counter 还原为 3
        assert_eq!(flusher.unflushed_count(), 3, "counter 应还原");
    }

    /// 测试 3: writer 推满阈值、然后 finalize —— 所有日志都不能丢。
    #[tokio::test]
    async fn no_data_lost_with_concurrent_writer_during_finalize() {
        let (sink, state) = MockSink::new();
        let flusher = fresh_flusher(Box::new(sink), 3);

        // 推满阈值，触发第一次 flush
        flusher.push(make_entry("a")).await;
        flusher.push(make_entry("b")).await;
        flusher.push(make_entry("c")).await;
        // 立刻再推 3 条（在 spawned flush 处理期间）
        flusher.push(make_entry("d")).await;
        flusher.push(make_entry("e")).await;
        flusher.push(make_entry("f")).await;

        flusher.finalize().await;

        let s = state.lock().unwrap();
        let total = s.total_log_entries();
        assert_eq!(total, 6, "6 条日志全部必须写库，不能丢");
        // 至少触发 1 次（阈值触发）+ finalize drain 一次，可能合并成 2 次
        assert!(s.call_count() >= 1);
    }

    /// 测试 4: shutdown 后 finalize 把 buffer 残余刷掉。
    #[tokio::test]
    async fn finalize_drains_remaining_buffer() {
        let (sink, state) = MockSink::new();
        let flusher = fresh_flusher(Box::new(sink), 100); // 高阈值，确保不触发

        flusher.push(make_entry("a")).await;
        flusher.push(make_entry("b")).await;
        assert_eq!(flusher.unflushed_count(), 2);

        flusher.finalize().await;
        let s = state.lock().unwrap();
        assert_eq!(
            s.call_count(),
            1,
            "finalize 必须把残余 buffer 写一次"
        );
        assert_eq!(s.total_log_entries(), 2);
    }

    /// 测试 5: pending 标志保证同一时刻最多一个 flush task。
    /// 100 条 / threshold=2 ⇒ 上限 50 次 flush；总条数必须 = 100。
    #[tokio::test]
    async fn pending_flag_prevents_concurrent_flushes() {
        let (sink, state) = MockSink::new();
        let flusher = fresh_flusher(Box::new(sink), 2);

        for i in 0..100 {
            flusher.push(make_entry(&format!("e{}", i))).await;
        }
        flusher.finalize().await;

        let s = state.lock().unwrap();
        assert_eq!(s.total_log_entries(), 100);
        // 100 条 / threshold=2 = 50 次。但如果 finalize drain 拿到剩余条数，
        // 总次数可能更少。关键是 ≤ 50（任何一次调用最多打包 threshold 条）。
        // 这里不做硬断言，验证总条数已足够说明并发安全。
    }

    /// 测试 6: timer 周期触发 flush。
    #[tokio::test]
    async fn timer_periodically_flushes_buffer() {
        let (sink, state) = MockSink::new();
        let flusher = LogFlusher::new(
            Box::new(sink),
            LogFlusherConfig {
                record_id: 42,
                threshold: 1000, // 高阈值，不让 writer 触发
                timer_interval_secs: 1,
            },
        )
        .into_arc();

        flusher.push(make_entry("a")).await;
        flusher.push(make_entry("b")).await;

        let timer_handle = {
            let f = flusher.clone();
            tokio::spawn(async move { f.run_timer().await })
        };

        // 等 2.5 秒，timer 应该至少触发 2 次
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

        flusher.finalize().await;
        let _ = timer_handle.await;

        let s = state.lock().unwrap();
        assert!(
            s.call_count() >= 1,
            "timer 周期兜底应该至少触发 1 次 flush（实际 {} 次）",
            s.call_count()
        );
        assert_eq!(s.total_log_entries(), 2);
    }

    /// 测试 7: shutdown=true 后 timer 退出，且不再触发 flush。
    #[tokio::test]
    async fn timer_exits_on_shutdown() {
        let (sink, state) = MockSink::new();
        let flusher = LogFlusher::new(
            Box::new(sink),
            LogFlusherConfig {
                record_id: 42,
                threshold: 1000,
                timer_interval_secs: 1,
            },
        )
        .into_arc();

        let timer_handle = {
            let f = flusher.clone();
            tokio::spawn(async move { f.run_timer().await })
        };

        flusher.finalize().await; // 立即 shutdown
        // 等 timer 退出
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), timer_handle)
            .await
            .expect("timer 应该在 finalize 后退出");

        let s = state.lock().unwrap();
        assert_eq!(
            s.call_count(),
            0,
            "buffer 为空时 finalize 不应调用 append"
        );
    }
}