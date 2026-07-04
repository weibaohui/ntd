//! 黑板（Blackboard）防抖服务。
//!
//! 核心思路：不再每次 todo 执行完毕立即触发黑板更新，而是将 todo_id 追加到
//! 黑板的 pending 队列，周期到点后通过 channel 通知监听方执行实际 LLM 调用。
//!
//! 职责边界（避免 cycle）：
//!   - 本模块只管 pending 队列 + timer，不调用 blackboard service 或 executor_service
//!   - 调用方（本模块外部）负责启动监听 channel 的后台任务，由其调用 update_blackboard

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{Duration, Instant};

use crate::db::Database;

/// 防抖消息：周期到点时通知监听方处理
#[derive(Clone)]
pub struct BlackboardFlushMsg {
    pub workspace_id: i64,
}

/// 防抖器全局状态
static DEBOUNCER: RwLock<Option<Debouncer>> = RwLock::const_new(None);

/// 全局 channel 发送端（监听方持有接收端）
static FLUSH_TX: RwLock<Option<mpsc::Sender<BlackboardFlushMsg>>> = RwLock::const_new(None);

/// workspace 维度的计时器状态，供 flush listener 读取以计算剩余秒数
#[derive(Clone)]
pub struct WorkspaceTimerState {
    /// 计时器启动时的时间戳（unix ms），用于计算 remaining_secs
    pub started_at_ms: u64,
    /// 防抖周期秒数
    pub debounce_secs: u64,
}

/// 全局计时器状态（只读，供 flush listener 查询）。使用 Option 包装以支持 const_new。
static TIMER_STATES: RwLock<Option<HashMap<i64, WorkspaceTimerState>>> = RwLock::const_new(None);

/// 查询 workspace 的当前计时器状态，返回 None 表示无 active timer。
pub async fn get_timer_state(workspace_id: i64) -> Option<WorkspaceTimerState> {
    let guard = TIMER_STATES.read().await;
    match guard.as_ref() {
        Some(map) => map.get(&workspace_id).cloned(),
        None => None,
    }
}

#[derive(Clone)]
struct Debouncer {
    timers: Arc<RwLock<HashMap<i64, bool>>>,
    debounce_secs: u64,
    debounce_count: u64,
}

impl Debouncer {
    fn new(debounce_secs: u64, debounce_count: u64) -> Self {
        Self {
            timers: Arc::new(RwLock::new(HashMap::new())),
            debounce_secs,
            debounce_count,
        }
    }
}

/// 全局初始化：启动 channel，注册到全局，在 `build_app_state` 中调用
pub async fn init(debounce_secs: u64, debounce_count: u64) -> mpsc::Receiver<BlackboardFlushMsg> {
    let (tx, rx) = mpsc::channel::<BlackboardFlushMsg>(100);

    let debouncer = Debouncer::new(debounce_secs, debounce_count);

    {
        let mut w = DEBOUNCER.write().await;
        *w = Some(debouncer);
    }
    {
        let mut w = FLUSH_TX.write().await;
        *w = Some(tx);
    }

    rx
}

/// 追加一个 todo_id 到 pending 队列；若 timer 未运行则启动。
///
/// 不调用任何 blackboard/executor_service 函数，职责纯粹为"入队 + 启动 timer"。
pub async fn push_pending_todo(workspace_id: i64, todo_id: i64, db: &Arc<Database>) {
    // 确保黑板记录已存在：首次有 todo 执行完成时，黑板记录还未创建。
    // create_blackboard 是幂等的（ON CONFLICT DO NOTHING），重复调用安全。
    if let Err(e) = db.create_blackboard(workspace_id).await {
        tracing::warn!(
            "创建黑板记录失败: workspace_id={}, error={}",
            workspace_id, e
        );
        // 黑板不存在时跳过入队，不阻塞主流程
        return;
    }

    tracing::info!(
        "push_pending_todo called: workspace_id={}, todo_id={}",
        workspace_id, todo_id
    );

    // 追加到 DB
    if let Err(e) = db.append_pending_todo_id(workspace_id, todo_id).await {
        tracing::warn!(
            "追加 pending_todo_id 失败: workspace_id={}, todo_id={}, error={}",
            workspace_id, todo_id, e
        );
        return;
    }

    tracing::info!("append_pending_todo_id 成功: workspace_id={}, todo_id={}", workspace_id, todo_id);

    // 获取 debouncer
    let debouncer = {
        let guard = DEBOUNCER.read().await;
        guard.as_ref().cloned()
    };
    let Some(debouncer) = debouncer else {
        tracing::warn!("BlackboardDebouncer 未初始化");
        return;
    };

    // 检查队列长度是否达到阈值，达到则立即触发
    if let Ok(Some(board)) = db.get_blackboard(workspace_id).await {
        let queue_len = serde_json::from_str::<Vec<i64>>(&board.pending_todo_ids)
            .map(|v| v.len())
            .unwrap_or(0);
        tracing::info!(
            "pending 队列检查: workspace_id={}, queue_len={}, threshold={}, debounce_secs={}",
            workspace_id, queue_len, debouncer.debounce_count, debouncer.debounce_secs
        );
        if queue_len as u64 >= debouncer.debounce_count {
            tracing::info!(
                "黑板 pending 队列达到阈值 {} 条，立即触发: workspace_id={}",
                queue_len, workspace_id
            );
            // 阈值触发时清除 timer 状态，避免 flush listener 再次等待
            {
                let mut states = TIMER_STATES.write().await;
                if let Some(map) = states.as_mut() {
                    map.remove(&workspace_id);
                }
            }
            let tx = {
                let guard = FLUSH_TX.read().await;
                guard.as_ref().cloned()
            };
            if let Some(tx) = tx {
                let msg = BlackboardFlushMsg { workspace_id };
                if let Err(e) = tx.send(msg).await {
                    tracing::warn!("发送 flush 消息失败: workspace_id={}, error={}", workspace_id, e);
                }
            }
            // 达到阈值触发后，不等 timer，等下次 append 再检查
            return;
        }
    }

    // 未达阈值，检查并启动 timer
    let mut timers = debouncer.timers.write().await;
    if timers.get(&workspace_id).copied().unwrap_or(false) {
        return; // timer 已在运行
    }
    timers.insert(workspace_id, true);
    // 记录 timer 启动时间，供 flush listener 计算剩余秒数
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    drop(timers);
    {
        let mut states = TIMER_STATES.write().await;
        states.get_or_insert_with(HashMap::new);
        states.as_mut().unwrap().insert(workspace_id, WorkspaceTimerState {
            started_at_ms: now_ms,
            debounce_secs: debouncer.debounce_secs,
        });
    }

    // 克隆所需数据
    let timers = debouncer.timers.clone();
    let debounce_secs = debouncer.debounce_secs;
    let tx = {
        let guard = FLUSH_TX.read().await;
        guard.as_ref().cloned()
    };

    // 启动 timer
    tokio::spawn(async move {
        // 使用 sleep 而非 interval：interval.tick() 第一次立即返回，不符合"等待周期"的需求
        tokio::time::sleep(Duration::from_secs(debounce_secs)).await;
        tracing::debug!("黑板 debounce timer 触发: workspace_id={}", workspace_id);

        // 清除 timer 状态
        {
            let mut states = TIMER_STATES.write().await;
            if let Some(map) = states.as_mut() {
                map.remove(&workspace_id);
            }
        }

        if let Some(tx) = tx {
            let msg = BlackboardFlushMsg { workspace_id };
            if let Err(e) = tx.send(msg).await {
                tracing::warn!("发送 flush 消息失败: workspace_id={}, error={}", workspace_id, e);
            }
        }

        let mut timers = timers.write().await;
        timers.insert(workspace_id, false);
    });
}
