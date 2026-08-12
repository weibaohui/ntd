//! 黑板防抖 flush 监听与调度服务（096-W3-PR1：从 `executor_service::completion` 整族搬入）。
//!
//! ## 搬迁背景
//!
//! 这组函数（listener / worker / ticker / 状态构建）职责是「黑板 pending 队列的防抖 flush
//! 调度」，属黑板服务域（`services::blackboard` / `blackboard_debouncer` 同族），
//! 此前埋在执行器完成模块里（completion.rs），且 `blackboard_flush_listener` 以 pub 形态
//! 被 handlers 层跨模块召唤，构成跨层倒挂。本模块是它们的归宿。
//!
//! ## 职责边界
//!
//! - `blackboard_flush_listener`（pub）：监听 debouncer channel + 每秒 ticker 广播状态；
//! - `handle_flush_msg` / `spawn_flush_worker`：flush 消息处理与 wiki 更新 worker（per-workspace 互斥）；
//! - `broadcast_ticker_status` / `build_blackboard_status` / `get_workspace_debounce`：
//!   状态事件构建与 per-workspace 防抖配置读取。
//!
//! 执行完成后的「record 入队」动作（push_pending_record）仍留在 completion 的
//! finalize 流程里——那是执行域的动作，不随本模块搬迁。

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::db::Database;
use crate::executor_service::types::ExecutionDeps;
use crate::executor_service::ExecEvent;

/// 构建黑板防抖状态事件（用于 WebSocket 推送）。
/// 从 DB 读取 pending 队列，从 debouncer 读取 timer 状态。
/// `is_refreshing` 由调用方根据 refreshing_workspaces 集合传入，
/// 确保 ticker 不会覆盖 spawned task 发出的 refreshing=true 状态。
async fn build_blackboard_status(
    db: &Database,
    debouncer: &crate::services::blackboard_debouncer::BlackboardDebouncer,
    workspace_id: i64,
    debounce_secs: i64,
    debounce_count: i64,
    is_refreshing: bool,
) -> ExecEvent {
    let pending_count = db
        .get_blackboard(workspace_id)
        .await
        .ok()
        .flatten()
        .map(|b| {
            serde_json::from_str::<Vec<i64>>(&b.pending_record_ids)
                .map(|v| v.len() as u64)
                .unwrap_or(0)
        })
        .unwrap_or(0);

    let remaining_secs = debouncer.get_timer_state(workspace_id)
        .await
        .map(|state| {
            let elapsed_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
                - state.started_at_ms as i64;
            let remaining = state.debounce_secs - elapsed_ms / 1000;
            remaining.max(0)
        })
        .unwrap_or(-1);

    ExecEvent::BlackboardDebounceStatus {
        workspace_id,
        pending_count,
        threshold: debounce_count as u64,
        debounce_secs: debounce_secs as u64,
        remaining_secs,
        refreshing: is_refreshing,
    }
}

/// 从 per-workspace 黑板配置中读取防抖参数的 helper。
/// DB 查询失败时回退默认值（600s / 10 条），避免静默丢弃配置读取错误。
async fn get_workspace_debounce(db: &Database, ws_id: i64) -> (i64, i64) {
    match db.get_blackboard_config(ws_id).await {
        Ok(Some(cfg)) => (cfg.debounce_secs, cfg.debounce_count),
        Ok(None) => (600, 10), // 未配置时回退默认值
        Err(e) => {
            // DB 错误不应静默吞掉，记录 warn 后回退默认值以保证可用性
            tracing::warn!("读取黑板防抖配置失败，使用默认值: workspace_id={}, error={}", ws_id, e);
            (600, 10)
        }
    }
}

/// ticker 分支：每秒广播所有已知 workspace 的黑板防抖状态。
/// refreshing 字段根据 refreshing_workspaces 集合动态设置，
/// 确保 spawned task 发出的 refreshing=true 不被 ticker 覆盖。
async fn broadcast_ticker_status(
    db: &Database,
    debouncer: &crate::services::blackboard_debouncer::BlackboardDebouncer,
    tx: &broadcast::Sender<ExecEvent>,
    known_workspaces: &mut Vec<i64>,
    refreshing_workspaces: &Arc<tokio::sync::Mutex<std::collections::HashSet<i64>>>,
) {
    // 首次：从 DB 拉取所有已知 workspace
    if known_workspaces.is_empty() {
        if let Ok(boards) = db.get_all_blackboards().await {
            *known_workspaces = boards.iter().map(|b| b.workspace_id).collect();
        }
    }
    for ws_id in known_workspaces.iter() {
        let (debounce_secs, debounce_count) = get_workspace_debounce(db, *ws_id).await;
        // 检查该 workspace 是否正在刷新中
        let is_refreshing = {
            let guard = refreshing_workspaces.lock().await;
            guard.contains(ws_id)
        };
        let event = build_blackboard_status(db, debouncer, *ws_id, debounce_secs, debounce_count, is_refreshing).await;
        let _ = tx.send(event);
    }
}

/// 派生独立 worker 任务执行 wiki 更新。
///
/// worker 内部循环处理：每次非破坏性读取 pending 队列，
/// 处理成功后移除已处理 ID（保留期间新到达的记录），
/// 若队列仍有剩余则继续处理下一批。
/// 失败时保留队列不删除，退出循环避免死循环。
fn spawn_flush_worker(
    ws_id: i64,
    debounce_secs: i64,
    debounce_count: i64,
    // 096-W2-PR3：五元组依赖已对象化（ExecutionDeps），签名从 9 参塌缩为 5 参
    deps: ExecutionDeps,
    refreshing_workspaces: Arc<tokio::sync::Mutex<std::collections::HashSet<i64>>>,
) {
    tokio::spawn(async move {
        // 单批处理的 record 上限。LLM 输出受 output_tokens（通常 4096）限制，
        // 一次塞太多 record 会让 LLM 整合的 Markdown JSON 过长被截断，extract_json_from_output
        // 解析失败 → Phase 2 失败 → 队列不清 → 下次更多 → 更易截断，恶性循环。
        // 分批让单次 LLM 输出量可控，worker 内循环会继续处理后续批次。
        // 10 条是经验值：每条 record 结论约几百字，10 条整合后约几千字，在 4096 token 内有富余。
        const MAX_BATCH_SIZE: usize = 10;
        // 循环处理，直到队列为空或某次处理失败
        loop {
            // 非破坏性读取 pending 队列（不用 take_pending_record_ids）
            let all_record_ids = match deps.db.get_blackboard(ws_id).await {
                Ok(Some(board)) => {
                    serde_json::from_str::<Vec<i64>>(&board.pending_record_ids).unwrap_or_default()
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!("读取 pending 队列失败: workspace_id={}, error={}", ws_id, e);
                    break;
                }
            };
            if all_record_ids.is_empty() {
                break;
            }

            // 分批：只取前 MAX_BATCH_SIZE 条，剩余留给下一轮循环
            let batch_len = all_record_ids.len().min(MAX_BATCH_SIZE);
            let record_ids: Vec<i64> = all_record_ids.iter().take(batch_len).copied().collect();
            if record_ids.len() < all_record_ids.len() {
                tracing::info!(
                    "黑板 worker 分批处理: workspace_id={}, 本批={}/{}",
                    ws_id, record_ids.len(), all_record_ids.len()
                );
            }

            // 广播 refreshing=true 状态（用全队列长度，让 UI 看到真实剩余量）
            let _ = deps.tx.send(ExecEvent::BlackboardDebounceStatus {
                workspace_id: ws_id,
                pending_count: all_record_ids.len() as u64,
                threshold: debounce_count as u64,
                debounce_secs: debounce_secs as u64,
                remaining_secs: -1,
                refreshing: true,
            });

            let update_result = crate::services::blackboard::update_blackboard_wiki(
                deps.db.clone(),
                deps.executor_registry.clone(),
                deps.tx.clone(),
                deps.task_manager.clone(),
                deps.config.clone(),
                deps.blackboard_debouncer.clone(),
                ws_id,
                record_ids,
            )
            .await;

            if let Err(ref e) = update_result {
                tracing::warn!(
                    "黑板 update_blackboard_wiki 失败: workspace_id={}, error={:?}",
                    ws_id, e
                );
                // 失败时保留 pending 队列不删除（update_blackboard_wiki 内部
                // 已改用 remove_specific_pending_record_ids，失败时不调用）。
                // 退出循环避免死循环；
                // 剩余队列由下方的 restart_timer 统一处理（不再区分失败/成功）。
                break;
            }
            // 成功：继续循环检查是否有新记录在处理期间到达
        }

        // 退出前从 DB 重新读取真实 pending_count。
        // 旧实现写死 pending_count=0，但失败时队列实际仍有残留（update_blackboard_wiki
        // 失败不调 remove_specific_pending_record_ids），下一秒 ticker 会从 DB 读回真实值，
        // 造成 UI 在 0 和真实值之间反复跳；这里一次性广播真实值，避免抖动。
        let final_pending_count = match deps.db.get_blackboard(ws_id).await {
            Ok(Some(board)) => {
                serde_json::from_str::<Vec<i64>>(&board.pending_record_ids)
                    .map(|v| v.len() as u64)
                    .unwrap_or(0)
            }
            _ => 0,
        };

        // 广播 refreshing=false 状态（携带真实 pending_count）
        let _ = deps.tx.send(ExecEvent::BlackboardDebounceStatus {
            workspace_id: ws_id,
            pending_count: final_pending_count,
            threshold: debounce_count as u64,
            debounce_secs: debounce_secs as u64,
            remaining_secs: -1,
            refreshing: false,
        });

        // 有残留队列时重启防抖 timer，让队列在 debounce_secs 后再次触发 flush。
        // 剩余记录可能来自：
        // 1. 分批处理后的下一批（worker 内循环已清空，但期间又到达了新的）
        // 2. 失败后保留的队列（update_blackboard_wiki 失败不清理）
        // 3. worker 运行期间新到达、但 flush 消息被 per-workspace 互斥丢弃的
        // 注意：不管 had_failure 真假都要重启——即使成功清空了本批，期间新到达
        // 的记录也不会触发新一轮 push（阈值只在 append 时检查，没有新 append 就
        // 不会触发），必须靠 timer 到期发起新的 flush。
        if final_pending_count > 0 {
            tracing::info!(
                "worker 退出，队列仍有 {} 条残留，重启防抖 timer 触发下一轮: workspace_id={}",
                final_pending_count, ws_id
            );
            deps.blackboard_debouncer.restart_timer(ws_id, &deps.db).await;
        }

        // 释放 per-workspace 互斥锁
        let mut guard = refreshing_workspaces.lock().await;
        guard.remove(&ws_id);
    });
}

/// 处理单条 flush 消息：非破坏性读取 pending 队列并派生 worker。
///
/// 若 workspace 已有 worker 运行中（refreshing_workspaces 包含），
/// 不丢弃消息：worker 内部循环会自然处理新到达的记录。
async fn handle_flush_msg(
    msg: crate::services::blackboard_debouncer::BlackboardFlushMsg,
    // 096-W2-PR3：五元组依赖已对象化（ExecutionDeps），借用现场按引用接参
    deps: &ExecutionDeps,
    refreshing_workspaces: &Arc<tokio::sync::Mutex<std::collections::HashSet<i64>>>,
    known_workspaces: &mut Vec<i64>,
) {
    let ws_id = msg.workspace_id;

    // 优先检查黑板功能总开关：关闭时直接返回，不执行任何 wiki 维护操作。
    // 这是第二道防线（第一道在 push_pending_record 阻止新记录入队）：
    // 即使 timer 在用户禁用前已调度、在禁用后自然到期发送了 flush 消息，也在此拦截。
    match deps.db.get_blackboard_config(ws_id).await {
        Ok(Some(cfg)) if !cfg.enabled => {
            tracing::debug!(
                "黑板功能已禁用，跳过 flush 消息处理: workspace_id={}",
                ws_id
            );
            return;
        }
        Err(e) => {
            // DB 错误时 warn 并继续处理（降级策略：假定启用，避免配置读取临时失败导致黑板永久卡死）
            tracing::warn!(
                "读取黑板配置失败（继续处理 flush）: workspace_id={}, error={}",
                ws_id, e
            );
        }
        _ => {}
    }

    // 确保 workspace 在已知列表中
    if !known_workspaces.contains(&ws_id) {
        known_workspaces.push(ws_id);
    }

    // per-workspace 互斥：同一 workspace 同时只运行一个 worker
    let should_spawn = {
        let mut guard = refreshing_workspaces.lock().await;
        if guard.contains(&ws_id) {
            // 已有 worker 在运行：不丢弃消息也不重复 spawn。
            // worker 内部循环会非破坏性读取队列，新到达的记录会被下一轮循环处理。
            tracing::debug!(
                "黑板 flush listener: workspace {} 已有 worker 运行中，依赖其内循环处理新记录",
                ws_id
            );
            false
        } else {
            guard.insert(ws_id);
            true
        }
    };

    if !should_spawn {
        return;
    }

    // 读取 per-workspace 防抖配置
    let (debounce_secs, debounce_count) = get_workspace_debounce(&deps.db, ws_id).await;

    spawn_flush_worker(
        ws_id,
        debounce_secs,
        debounce_count,
        // spawn move 闭包需要 owned deps，Arc 字段克隆廉价
        deps.clone(),
        refreshing_workspaces.clone(),
    );
}

/// 黑板 flush 监听器：监听 debouncer 的 channel，收到消息后 spawn 独立 worker 执行
/// update_blackboard_wiki；每秒通过 broadcast::tx 推送一次 BlackboardDebounceStatus 事件。
///
/// - per-workspace 互斥：同一 workspace 同时只运行一个 wiki 更新 worker
/// - 防抖阈值（周期秒数、条数阈值）从 per-workspace 黑板配置（blackboards 表）读取，
///   实现工作空间隔离
///
/// 由 handlers 层在服务启动时以 `tokio::spawn` 召唤。
/// pub(crate)：仅在 crate 内被 handlers 召唤；参数类型 ExecutionDeps 同为 crate 内可见。
pub(crate) async fn blackboard_flush_listener(
    mut rx: tokio::sync::mpsc::Receiver<crate::services::blackboard_debouncer::BlackboardFlushMsg>,
    // 096-W2-PR3：五元组依赖已对象化（ExecutionDeps），6 参塌缩为 2 参
    deps: ExecutionDeps,
) {
    // 每秒 ticker 用于推送状态
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // 已知的 workspace_id 列表（首次发送时从 DB 拉取）
    let mut known_workspaces: Vec<i64> = Vec::new();

    // per-workspace 互斥：标记哪些 workspace 正在执行 wiki 更新
    let refreshing_workspaces =
        Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::<i64>::new()));

    // ===== 启动时扫描：为有残留队列的 workspace 重启防抖 timer =====
    // 实例重启后 TIMER_STATES / ACTIVE_TIMERS 全部丢失，DB 中残留的 pending 记录
    // 不会再有新的 push 触发阈值检查，也没有 worker 退出时调用 restart_timer，
    // 导致残留队列永久卡死。启动时统一检查所有 blackboard，若有非空 pending 队列
    // 则重启 timer，让它们在 debounce_secs 后重新触发 flush。
    {
        let rescan_timer = deps.db.clone();
        // 096-W4-5：debouncer 实例随闭包 move（Arc 克隆仅计数自增）
        let rescan_debouncer = deps.blackboard_debouncer.clone();
        tokio::spawn(async move {
            match rescan_timer.get_all_blackboards().await {
                Ok(boards) => {
                    for board in &boards {
                        let ids: Vec<i64> = serde_json::from_str(&board.pending_record_ids)
                            .unwrap_or_default();
                        if !ids.is_empty() {
                            tracing::info!(
                                "启动时检测到黑板残留队列，重启 timer: workspace_id={}, pending={}",
                                board.workspace_id, ids.len()
                            );
                            rescan_debouncer.restart_timer(board.workspace_id, &rescan_timer).await;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("启动时扫描黑板残留队列失败: {:?}", e);
                }
            }
        });
    }

    loop {
        tokio::select! {
            // 每秒 ticker：广播所有已知 workspace 的状态
            _ = ticker.tick() => {
                broadcast_ticker_status(
                    &deps.db, &deps.blackboard_debouncer, &deps.tx, &mut known_workspaces, &refreshing_workspaces,
                ).await;
            }

            // flush 消息：非破坏性读取 pending 队列并派生 worker
            msg = rx.recv() => {
                match msg {
                    Some(msg) => {
                        handle_flush_msg(
                            msg, &deps,
                            &refreshing_workspaces, &mut known_workspaces,
                        ).await;
                    }
                    None => break,
                }
            }
        }
    }
}
