//! 黑板（Blackboard）防抖服务。
//!
//! 核心思路：不再每次 todo 执行完毕立即触发黑板更新，而是将 execution_record_id 追加到
//! 黑板的 pending 队列，周期到点后通过 channel 通知监听方执行实际 LLM 调用。
//!
//! 职责边界（避免 cycle）：
//!   - 本模块只管 pending 队列 + timer，不调用 blackboard service 或 executor_service
//!   - 调用方（本模块外部）负责启动监听 channel 的后台任务，由其调用 update_blackboard
//!
//! 防抖阈值（周期秒数、条数阈值）从各工作空间的黑板配置（blackboards 表）读取，
//! 实现 per-workspace 隔离。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use tokio::time::Duration;

use crate::db::Database;

/// 防抖消息：周期到点时通知监听方处理
#[derive(Clone)]
pub struct BlackboardFlushMsg {
    pub workspace_id: i64,
}

/// 全局 channel 发送端（监听方持有接收端）
static FLUSH_TX: RwLock<Option<mpsc::Sender<BlackboardFlushMsg>>> = RwLock::const_new(None);

/// workspace 维度的计时器状态，供 flush listener 读取以计算剩余秒数
#[derive(Clone)]
pub struct WorkspaceTimerState {
    /// 计时器启动时的时间戳（unix ms），用于计算 remaining_secs
    pub started_at_ms: u64,
    /// 防抖周期秒数（来自黑板配置的运行时值）
    pub debounce_secs: i64,
}

/// 全局计时器状态（只读，供 flush listener 查询）
static TIMER_STATES: RwLock<Option<HashMap<i64, WorkspaceTimerState>>> = RwLock::const_new(None);

/// 全局 timer 运行状态：记录哪个 workspace 的 timer 正在运行。
/// Arc 包装使 static 可跨 tokio::spawn 共享引用。
/// 使用类型别名消除 clippy::type_complexity 告警，同时让 static 声明更易读。
type ActiveTimersMap = Arc<RwLock<Option<HashMap<i64, bool>>>>;
static ACTIVE_TIMERS: std::sync::OnceLock<ActiveTimersMap> =
    std::sync::OnceLock::new();

/// 查询 workspace 的当前计时器状态，返回 None 表示无 active timer。
pub async fn get_timer_state(workspace_id: i64) -> Option<WorkspaceTimerState> {
    let guard = TIMER_STATES.read().await;
    guard.as_ref().and_then(|m| m.get(&workspace_id).cloned())
}

/// 黑板防抖阈值变更后，调整运行中的计时器：
/// - 若已计时长 ≥ 新阈值 → 立即触发 flush（已超时不继续等），清除计时器状态
/// - 若已计时长 < 新阈值 → 更新 TIMER_STATES 的 debounce_secs 为新值，
///   保持 started_at_ms 不变，让计时器用新阈值继续运行
///
/// 全程持 TIMER_STATES 写锁（读→算→改），防止后台 timer 任务在间隙中插入操作。
pub async fn reconcile_timer_after_config_change(workspace_id: i64, new_debounce_secs: i64) {
    // 持写锁进行读取-判断-修改，避免与后台 timer 任务产生竞态
    let should_flush = {
        let mut states = TIMER_STATES.write().await;
        let map = states.as_mut();
        let Some(map) = map else {
            // TIMER_STATES 尚未初始化（理论上不会发生），跳过
            return;
        };
        let Some(state) = map.get(&workspace_id) else {
            // 没有活跃 timer，无需处理
            return;
        };

        // 计算已计时长（秒）；saturating_sub 防御时钟回拨
        // duration_since 在系统时钟早于 UNIX_EPOCH 时返回 Err；as_millis() 超出 u64 范围时回退 0
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let elapsed_secs = now_ms.saturating_sub(state.started_at_ms) / 1000;

        if elapsed_secs >= new_debounce_secs as u64 {
            // 已计时长已达到或超过新阈值 → 立即触发 flush
            tracing::info!(
                "黑板阈值变更：已计时 {}s ≥ 新阈值 {}s，立即触发 flush: workspace_id={}",
                elapsed_secs, new_debounce_secs, workspace_id
            );
            map.remove(&workspace_id);
            true // 标记需要发送 flush
        } else {
            // 已计时长还未达新阈值 → 更新 debounce_secs，继续计时
            tracing::info!(
                "黑板阈值变更：已计时 {}s < 新阈值 {}s，更新 debounce_secs 继续计时: workspace_id={}",
                elapsed_secs, new_debounce_secs, workspace_id
            );
            map.insert(workspace_id, WorkspaceTimerState {
                started_at_ms: state.started_at_ms,
                debounce_secs: new_debounce_secs,
            });
            false // 不需要发送 flush
        }
    }; // TIMER_STATES 写锁在此释放

    if should_flush {
        // 标记 timer 未运行（与 TIMER_STATES 无锁序依赖，单独持锁）
        // ACTIVE_TIMERS 在 init() 中初始化；若未初始化则跳过标记，但仍继续发送 flush 消息
        if let Some(timers) = ACTIVE_TIMERS.get() {
            let mut timers = timers.write().await;
            if let Some(map) = timers.as_mut() {
                map.insert(workspace_id, false);
            }
        }
        // 发送 flush 消息
        let tx = {
            let guard = FLUSH_TX.read().await;
            guard.as_ref().cloned()
        };
        if let Some(tx) = tx {
            let msg = BlackboardFlushMsg { workspace_id };
            if let Err(e) = tx.send(msg).await {
                tracing::warn!("reconcile_timer: 发送 flush 消息失败: workspace_id={}, error={}", workspace_id, e);
            }
        }
    }
}

/// 全局初始化：启动 channel，注册到全局，在 `build_app_state` 中调用。
/// 注意：不再在启动时传入默认防抖值——防抖阈值现在从 per-workspace DB 配置读取。
pub async fn init() -> mpsc::Receiver<BlackboardFlushMsg> {
    let (tx, rx) = mpsc::channel::<BlackboardFlushMsg>(100);
    {
        let mut w = FLUSH_TX.write().await;
        *w = Some(tx);
    }
    {
        let mut w = TIMER_STATES.write().await;
        *w = Some(HashMap::new());
    }
    // OnceLock 初始化 RwLock（ RwLock::new(None) 创建时内部 Option 为 None）
    // 需要额外一步：获取写锁后将内部 Option 填充为 Some(HashMap::new())）
    let timers = ACTIVE_TIMERS.get_or_init(|| Arc::new(RwLock::new(None)));
    {
        let mut w = timers.write().await;
        *w = Some(HashMap::new());
    }
    rx
}

/// 追加一个 execution_record_id 到 pending 队列；若 timer 未运行则启动。
///
/// 核心流程：入队 → 检查阈值是否达到立即触发 → 检查 timer 是否在运行 → 启动 timer。
/// 防抖阈值（debounce_secs、debounce_count）从 per-workspace 黑板配置（blackboards 表）读取，
/// 实现各工作空间独立的防抖策略。不调用任何 blackboard/executor_service 函数，职责纯粹为"入队 + 启动 timer"。
pub async fn push_pending_record(workspace_id: i64, record_id: i64, db: &Arc<Database>) {
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
        "push_pending_record called: workspace_id={}, record_id={}",
        workspace_id, record_id
    );

    // 检查黑板功能总开关：关闭时跳过入队，不阻塞主流程
    match db.get_blackboard_config(workspace_id).await {
        Ok(Some(cfg)) if !cfg.enabled => {
            tracing::debug!(
                "黑板功能已禁用，跳过 push_pending_record: workspace_id={}",
                workspace_id
            );
            return;
        }
        Err(e) => {
            tracing::warn!("读取黑板配置失败（继续入队）: workspace_id={}, error={}", workspace_id, e);
        }
        _ => {}
    }

    // 追加到 DB
    if let Err(e) = db.append_pending_record_id(workspace_id, record_id).await {
        tracing::warn!(
            "追加 pending_record_id 失败: workspace_id={}, record_id={}, error={}",
            workspace_id, record_id, e
        );
        return;
    }

    tracing::info!("append_pending_record_id 成功: workspace_id={}, record_id={}", workspace_id, record_id);

    // 读取 per-workspace 防抖配置（从 blackboards 表）
    let (debounce_secs, debounce_count) = match db.get_blackboard_config(workspace_id).await {
        Ok(Some(cfg)) => (cfg.debounce_secs, cfg.debounce_count),
        Ok(None) => (600, 10), // 无配置时用默认值（理论上不会发生）
        Err(e) => {
            tracing::warn!("读取黑板配置失败，使用默认值: workspace_id={}, error={}", workspace_id, e);
            (600, 10)
        }
    };

    // 检查队列长度是否达到阈值，达到则立即触发
    if let Ok(Some(board)) = db.get_blackboard(workspace_id).await {
        let queue_len = serde_json::from_str::<Vec<i64>>(&board.pending_record_ids)
            .map(|v| v.len())
            .unwrap_or(0);
        tracing::info!(
            "pending 队列检查: workspace_id={}, queue_len={}, threshold={}, debounce_secs={}",
            workspace_id, queue_len, debounce_count, debounce_secs
        );
        if queue_len as u64 >= debounce_count as u64 {
            tracing::info!(
                "黑板 pending 队列达到阈值 {} 条，立即触发: workspace_id={}",
                queue_len, workspace_id
            );
            // 注意：此处不再清除 TIMER_STATES。
            // 旧实现提前删除 timer 状态，会在 worker 真正 spawn 起来、把 workspace
            // 加入 refreshing_workspaces 之前，制造一个时间窗口：每秒 ticker 调用
            // build_blackboard_status 时 get_timer_state 返回 None → remaining_secs=-1
            // → 前端 hasTimer=false 且 pending_count>0 → 渲染为「等待刷新」，与
            // 「条数已远超阈值」的事实矛盾。
            // 保留 timer 状态让前端继续显示倒计时；worker 接管后由 refreshing=true
            // 表达「正在刷新」语义。即使 timer 自然到期再发一次 flush 消息，
            // handle_flush_msg 的 per-workspace 互斥会安全丢弃重复消息，不会 spawn
            // 第二个 worker。
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

    // 未达阈值，启动防抖 timer（若尚未运行）
    start_timer(workspace_id, debounce_secs).await;
}

/// 启动 per-workspace 防抖 timer（若该 workspace 已有 timer 运行中则不重复启动）。
///
/// 抽取自 `push_pending_record` 的后半段，同时供 worker 失败重试调用：
/// worker 处理失败时 pending 队列保留，需要重新启动 timer 让队列在下一周期再次触发，
/// 否则队列会永久卡住（阈值分支注释「达到阈值触发后，不等 timer」意味着失败后没人会再触发）。
async fn start_timer(workspace_id: i64, debounce_secs: i64) {
    // 检查并标记 timer 运行中：用 ACTIVE_TIMERS 的 bool 标志做互斥，
    // 避免同一 workspace 重复 spawn 多个 sleep 任务导致重复 flush。
    // ACTIVE_TIMERS 在 init() 中初始化；若未初始化则无法操作，静默返回
    {
        let Some(timers) = ACTIVE_TIMERS.get() else { return; };
        let mut timers = timers.write().await;
        let Some(timers_map) = timers.as_mut() else { return; };
        if timers_map.get(&workspace_id).copied().unwrap_or(false) {
            return; // timer 已在运行
        }
        timers_map.insert(workspace_id, true);
    }

    // 记录 timer 启动时间，供 flush listener 计算剩余秒数
    // duration_since 在系统时钟早于 UNIX_EPOCH 时返回 Err；as_millis() 超出 u64 范围时回退 0
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    {
        let mut states = TIMER_STATES.write().await;
        states.get_or_insert_with(HashMap::new).insert(workspace_id, WorkspaceTimerState {
            started_at_ms: now_ms,
            debounce_secs,
        });
    }

    // 获取 channel sender 和 active timers 的 Arc 句柄，供 timer task 使用
    let tx = {
        let guard = FLUSH_TX.read().await;
        guard.as_ref().cloned()
    };

    // 获取 ACTIVE_TIMERS 的 Arc 句柄供 timer task 使用
    // ACTIVE_TIMERS 在 init() 中初始化；若未初始化则无法启动 timer，直接返回
    let Some(timers_handle) = ACTIVE_TIMERS.get().cloned() else { return };

    // 启动 timer（per-workspace 防抖时长）
    tokio::spawn(async move {
        // 使用 sleep 而非 interval：interval.tick() 第一次立即返回，不符合"等待周期"的需求
        tracing::info!("黑板 debounce timer 已启动: workspace_id={}, {}s 后触发", workspace_id, debounce_secs);
        tokio::time::sleep(Duration::from_secs(debounce_secs as u64)).await;
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

        // 重置 timer 运行状态
        {
            let mut timers = timers_handle.write().await;
            if let Some(map) = timers.as_mut() {
                map.insert(workspace_id, false);
            }
        }
    });
}

/// 重新启动 per-workspace 防抖 timer，用于 worker 处理失败后的恢复。
///
/// 场景：`update_blackboard_wiki` 失败时 pending 队列保留（`remove_specific_pending_record_ids`
/// 未调用），但阈值触发分支不会启动 timer。若不主动恢复，残留队列将永久卡住，UI 持续显示
/// 「等待刷新 / N / 阈值 条」却永不触发刷新。
///
/// 本函数从 DB 重新读取该 workspace 的防抖配置后调用 `start_timer`，
/// 让队列在 `debounce_secs` 秒后再次触发 flush，给 LLM 一个重试窗口。
/// 若 DB 读取失败则回退默认防抖值（600s），保证队列最终能再次触发而非永久挂起。
pub async fn restart_timer(workspace_id: i64, db: &Arc<Database>) {
    // 复用 push_pending_record 的默认值逻辑：DB 错误时回退默认值保证可用性
    let debounce_secs = match db.get_blackboard_config(workspace_id).await {
        Ok(Some(cfg)) => cfg.debounce_secs,
        Ok(None) => 600,
        Err(e) => {
            tracing::warn!(
                "restart_timer 读取黑板配置失败，使用默认值: workspace_id={}, error={}",
                workspace_id, e
            );
            600
        }
    };
    start_timer(workspace_id, debounce_secs).await;
}
