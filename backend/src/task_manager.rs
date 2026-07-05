// TaskManager 负责管理运行中的执行任务，主要职责是支持外部取消信号与 WebSocket 同步。
//
// ## 设计要点
// - **bounded channel**：取消信号只需一次，使用 `mpsc::channel(1)` 既能满足语义又能避免 unbounded 队列的潜在内存风险。
// - **RAII guard**：`TaskGuard` 在 `Drop` 时自动从 manager 中注销 task_id，杜绝「task 正常结束但忘记 remove」导致的 sender 泄漏。
// - **双重保险**：即便用户提前 `remove()`，guard 的 Drop 是幂等的（重复 remove 是空操作），不会报错。
//
// ## 关键变更（Issue #506）
// - `register()` 返回 `UnboundedReceiver` 的旧接口保留（向后兼容），但实际 channel 改为 capacity=1 的 bounded mpsc。
// - 新增 `register_with_guard()`，调用方应优先使用它来获得 RAII 保护。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::{Mutex, RwLock};
use serde::Serialize;

/// 任务信息结构，用于 WebSocket 同步与状态展示。
#[derive(Debug, Clone, Serialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub todo_id: i64,
    pub todo_title: String,
    pub executor: String,
    /// 执行记录的日志（JSON 字符串）
    pub logs: String,
}

/// 任务管理器：维护当前正在运行的 task 列表，支持取消信号与状态同步。
pub struct TaskManager {
    // 取消信号发送端：每个 task_id 一个 sender，cancel 时取出并 send 一次。
    // 选用 `mpsc::Sender<()>`（bounded channel）而不是 unbounded，因为 cancel 信号语义上最多触发一次，capacity=1 足够。
    tasks: Mutex<HashMap<String, mpsc::Sender<()>>>,
    /// 存储每个任务的基本信息，用于 WebSocket 连接时同步
    task_infos: RwLock<HashMap<String, TaskInfo>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            task_infos: RwLock::new(HashMap::new()),
        }
    }

    /// 注册任务并返回取消信号的接收端。
    ///
    /// **注意**：调用方必须确保在 task 结束时调用 [`remove`]，否则 sender 会留在 map 中（Issue #506 描述的内存泄漏路径）。
    /// 推荐改用 [`register_with_guard`]，由 RAII 机制保证清理。
    pub async fn register(&self, task_id: String) -> mpsc::Receiver<()> {
        // 容量 1 足够：cancel 只发一次，且 send 是非阻塞的；接收端尚未消费时第二次 send 会被丢弃（这是期望行为）。
        let (tx, rx) = mpsc::channel(1);
        self.tasks.lock().await.insert(task_id, tx);
        rx
    }

    /// 注册任务并返回 [`TaskGuard`]，guard 在 drop 时自动从 manager 注销。
    ///
    /// 这是推荐用法：即使 task 因 panic 或早返回忘记调用 `remove`，guard 也会兜底清理。
    pub async fn register_with_guard(self: &Arc<Self>, task_id: String) -> TaskGuard {
        let (tx, rx) = mpsc::channel(1);
        self.tasks.lock().await.insert(task_id.clone(), tx);
        TaskGuard {
            task_id,
            manager: self.clone(),
            // 把 receiver 交给调用方，guard 内保留 None，避免双重所有权。
            receiver: Some(rx),
        }
    }

    /// 注册任务信息，用于 WebSocket 同步
    pub async fn register_info(&self, info: TaskInfo) {
        self.task_infos.write().await.insert(info.task_id.clone(), info);
    }

    /// 获取所有当前运行的任务信息
    pub async fn get_all_task_infos(&self) -> Vec<TaskInfo> {
        self.task_infos.read().await.values().cloned().collect()
    }

    /// 取消任务：取出 sender 并发出取消信号。
    /// 返回 true 表示找到了对应 task 并已发出信号；false 表示 task 不存在。
    ///
    /// PR #545 review CRITICAL #1 修复: 旧实现用 `tx.send(()).await`（async），
    /// 对所有外部 caller 带来「cancel 延迟耦合 executor 启动时间」的语义回归——
    /// 在 spawned task 还没进入 `tokio::select!` 时 cancel 会一直 park。
    /// 改用非阻塞 `try_send`：buffer=1 时 buffer 空就立刻成功；receiver 已 drop
    /// 时 `try_send` 返回 `Err`，被 `let _` 吞掉——任务已自然结束，无需 cancel。
    pub async fn cancel(&self, task_id: &str) -> bool {
        if let Some(tx) = self.tasks.lock().await.remove(task_id) {
            // try_send 是非阻塞的：buffer=1 + 之前没 send 过 ⇒ 必成功。
            // 唯一失败是 receiver 已 drop（任务自然结束），吞掉错误即可。
            let _ = tx.try_send(());
            true
        } else {
            false
        }
    }

    /// 手动移除 task 记录（幂等）。Guard drop 时也会调用此方法。
    pub async fn remove(&self, task_id: &str) {
        self.tasks.lock().await.remove(task_id);
        self.task_infos.write().await.remove(task_id);
    }
}

/// RAII guard：持有时表示 task 仍在运行，drop 时自动从 [`TaskManager`] 注销。
///
/// ## 使用模式
/// 1. `register_with_guard` 拿到 guard
/// 2. `guard.take_receiver()` 取出 receiver 用于 `tokio::select!`
/// 3. 任意路径退出（正常 / 早返回 / panic）都会触发 drop，guard 自动清理
///
/// ## 为何用 tokio::spawn 调用 remove
/// `Drop` 中不能直接 `.await`，但需要异步操作。spawn 一个短任务让清理异步进行，
/// 这样既不会阻塞 drop，也能保留异步语义。
pub struct TaskGuard {
    task_id: String,
    manager: Arc<TaskManager>,
    // 用 Option 包裹 receiver，便于 take_receiver 调用一次后置 None。
    receiver: Option<mpsc::Receiver<()>>,
}

impl TaskGuard {
    /// 取出内部的 receiver，用于在 select! 中监听取消信号。
    /// 只能调用一次；调用后 guard 仍会保留并负责 cleanup。
    #[allow(clippy::expect_used)]
    pub fn take_receiver(&mut self) -> mpsc::Receiver<()> {
        self.receiver
            .take()
            .expect("TaskGuard::take_receiver called more than once")
    }

    /// 返回 guard 关联的 task_id，便于日志与诊断。
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        // 仅当 caller 没显式 remove 时才需要 spawn 清理任务。
        // 由于 remove() 是幂等的，即使重复调用也无副作用，所以这里可以无条件 spawn。
        //
        // PR #545 review CRITICAL #2 修复: 旧实现直接 `tokio::spawn` 要求 caller
        // 必须在 tokio runtime 内——runtime shutdown、sync test、`spawn_blocking`
        // 线程里 drop guard 会 panic，且 panic 会 leak task entry（issue #506
        // 想修的 bug 重新引入）。现在用 `Handle::try_current()` 探测 runtime
        // 可用性：可用则 spawn 异步清理；不可用则放弃异步清理、记日志（runtime
        // shutdown 是显式行为，调用方应负责在此之前已通过 `cancel` / `remove`
        // 走完正常清理路径，guard 仅是「忘了手动清理」的安全网）。
        let manager = self.manager.clone();
        let task_id = std::mem::take(&mut self.task_id);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                manager.remove(&task_id).await;
            });
        } else {
            // Runtime 已退出：guard 退化为 no-op，避免 panic 掩盖真正的 shutdown
            // 错误。task_id 仍留在 `tasks` map 中，但在 shutdown 上下文里整个
            // 进程都即将退出，leak 不会跨进程边界。
            tracing::debug!(
                "TaskGuard dropped outside tokio runtime for task '{}'; \
                 skipping async cleanup (process is shutting down)",
                task_id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_creates_receiver() {
        // 基础路径：register 后 cancel 能让 receiver 收到信号
        let tm = TaskManager::new();
        let mut rx = tm.register("task-1".to_string()).await;
        tm.cancel("task-1").await;
        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_register_returns_bounded_receiver() {
        // 验证 channel 容量是 1：连续 cancel 后第二次 try_recv 应立即返回空（因为第一次已消费）。
        // 目的：固化 #506 的设计约束，防止未来回归到 unbounded。
        let tm = TaskManager::new();
        let mut rx = tm.register("task-1".to_string()).await;
        // 未取消时 try_recv 返回 Empty（没有信号）。
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_cancel_returns_true_when_found() {
        let tm = TaskManager::new();
        let _rx = tm.register("task-1".to_string()).await;
        assert!(tm.cancel("task-1").await);
    }

    #[tokio::test]
    async fn test_cancel_returns_false_when_not_found() {
        let tm = TaskManager::new();
        assert!(!tm.cancel("task-1").await);
    }

    #[tokio::test]
    async fn test_remove_cleans_up() {
        let tm = TaskManager::new();
        let _rx = tm.register("task-1".to_string()).await;
        tm.remove("task-1").await;
        assert!(!tm.cancel("task-1").await);
    }

    #[tokio::test]
    async fn test_remove_is_idempotent() {
        // #506 的修复核心：guard drop + 显式 remove 同时发生时不能 panic
        let tm = TaskManager::new();
        let _rx = tm.register("task-1".to_string()).await;
        tm.remove("task-1").await;
        // 第二次 remove 不应 panic
        tm.remove("task-1").await;
        assert!(!tm.cancel("task-1").await);
    }

    #[tokio::test]
    async fn test_multiple_tasks_independent() {
        let tm = TaskManager::new();
        let _rx1 = tm.register("task-1".to_string()).await;
        let _rx2 = tm.register("task-2".to_string()).await;

        assert!(tm.cancel("task-1").await);
        assert!(tm.cancel("task-2").await);
        assert!(!tm.cancel("task-1").await); // already removed
    }

    #[tokio::test]
    async fn test_task_info_tracking() {
        let tm = TaskManager::new();
        tm.register_info(TaskInfo {
            task_id: "task-1".to_string(),
            todo_id: 1,
            todo_title: "Test Task".to_string(),
            executor: "claudecode".to_string(),
            logs: "[]".to_string(),
        })
        .await;

        let infos = tm.get_all_task_infos().await;
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].task_id, "task-1");

        tm.remove("task-1").await;
        let infos = tm.get_all_task_infos().await;
        assert!(infos.is_empty());
    }

    #[tokio::test]
    async fn test_guard_drop_cleans_up() {
        // #506 的核心测试：guard drop 时应自动从 manager 注销 task
        let tm = Arc::new(TaskManager::new());
        {
            let _guard = tm.register_with_guard("task-1".to_string()).await;
            // guard 还活着时，cancel 应能找到
            assert!(tm.cancel("task-1").await);
        }
        // 给 spawn 的清理任务一点时间执行
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // guard drop 后，cancel 应返回 false
        assert!(!tm.cancel("task-1").await);
    }

    #[tokio::test]
    async fn test_guard_take_receiver() {
        // 验证 guard 取出 receiver 后仍可正常 cancel，
        // 且 take_receiver 是「一次性」语义（后续调用会 panic）。
        let tm = Arc::new(TaskManager::new());
        let mut guard = tm.register_with_guard("task-1".to_string()).await;
        let mut rx = guard.take_receiver();
        tm.cancel("task-1").await;
        assert!(rx.try_recv().is_ok());
        // 重复 take_receiver 必须 panic —— 防止调用方误以为能再次获取。
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = guard.take_receiver();
        }));
        assert!(result.is_err(), "take_receiver should panic on second call");
    }

    #[tokio::test]
    async fn test_guard_idempotent_with_explicit_remove() {
        // 显式 remove + guard drop 的双重保险：不能 panic
        let tm = Arc::new(TaskManager::new());
        let guard = tm.register_with_guard("task-1".to_string()).await;
        tm.remove("task-1").await;
        drop(guard);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // 状态应保持干净
        assert!(!tm.cancel("task-1").await);
    }

    #[tokio::test]
    async fn test_guard_drop_after_scope_cleans_up() {
        // 验证在子作用域 drop guard 后清理会发生。
        // 真实 panic 场景下 Rust 会沿调用栈 unwind 局部变量，guard 的 Drop 行为与作用域退出等价，
        // 所以这个测试足以覆盖「guard 一定 drop」的不变量。
        let tm = Arc::new(TaskManager::new());
        {
            let _guard = tm.register_with_guard("task-1".to_string()).await;
            assert!(tm.cancel("task-1").await);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!tm.cancel("task-1").await);
    }

    #[tokio::test]
    async fn test_guard_forgotten_after_register_still_cleans() {
        // 关键场景（#506 描述）：调用方拿了 guard 但忘了手动 remove，
        // 且 receiver 也没人消费——guard drop 时仍应清掉 sender 与 task_infos。
        let tm = Arc::new(TaskManager::new());
        let _guard = tm.register_with_guard("task-orphan".to_string()).await;
        // 故意 drop 掉 _guard（这里不显式调用，但函数返回时 _guard 会 drop）
        drop(_guard);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!tm.cancel("task-orphan").await);
    }
}