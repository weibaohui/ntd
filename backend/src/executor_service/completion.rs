//! 终态分支（正常完成 / 取消 / 超时）+ emit event。
//!
//! 模块职责：
//!   1. [`emit_started_event`] —— 启动阶段：Started 事件 + 首条 info 日志
//!   2. [`persist_completion_record`] —— 正常完成：把 stats / usage / model 写库
//!   3. [`emit_post_execution_todo_progress`] —— 后置 todo_progress 钩子
//!   4. [`finalize_normal_completion`] —— 正常完成末段：auto-review + finish + emit
//!   5. [`handle_cancellation_branch`] / [`handle_timeout_branch`] —— 终态分支
//!   6. [`apply_wall_clock_duration`] —— 用 wall-clock 覆盖 executor 报的 duration
//!   7. [`format_timeout_secs`] —— 把超时秒数格式化为人类可读字符串
//!
//! 各函数 ≤ 30 行；编排层只调用入口。

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::adapters::{CodeExecutor, ExecutorRegistry};
use crate::db::Database;
use crate::executor_service::ExecEvent;
use crate::models::{ExecutionUsage, ParsedLogEntry};
use crate::task_manager::TaskManager;

use super::auto_review::run_auto_review;
use super::log_capture::send_event;

/// 从 execution_events pipeline 生成的 tokens 日志条目中提取最终 usage。
///
/// 统一 usage 来源：所有 executor 的 token 用量都通过 EventPipeline 解析为
/// ExecutionEvent::Tokens 事件 + LogFlusher 写库，不再依赖各 executor 各自的 get_usage() 实现。
/// tokens 条目中的 usage 是累积值（非增量），取最后一条作为最终 total。
pub(crate) fn get_usage_from_tokens_logs(logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> {
    logs.iter().rev().find(|l| l.log_type == "tokens")?.usage.clone()
}

/// 从日志条目中提取最终结果文本。
///
/// 统一来源：pipeline 的 Result 事件写入 "result" 类型日志。
/// 回退扫描 "text" 类型条目（某些 executor 可能不产生 result 类型）。
/// 再回退到 "assistant" 类型（atomcode 等执行器的累积文本块）。
pub(crate) fn get_final_result_from_logs(logs: &[ParsedLogEntry]) -> Option<String> {
    logs.iter()
        .rev()
        .find(|l| l.log_type == "result" || l.log_type == "text" || l.log_type == "assistant")
        .map(|l| l.content.clone())
}

/// 从日志条目中提取模型名称。
///
/// 统一来源：pipeline 的 ModelSwitch 事件写入 "model_switch" 类型日志，
/// 内容格式为 "model: {name}"。
/// 回退到 "system" 类型日志中查找含 "model" 关键字的条目。
pub(crate) fn get_model_from_logs(logs: &[ParsedLogEntry]) -> Option<String> {
    // 优先找 model_switch 条目
    if let Some(log) = logs.iter().rev().find(|l| l.log_type == "model_switch") {
        if let Some(model) = log.content.strip_prefix("model: ") {
            return Some(model.to_string());
        }
    }
    // 回退：从 system 条目中提取（旧格式："Model: claude-3-sonnet" 或含 model 字的）
    logs.iter().rev().find_map(|l| {
        if l.log_type == "system" {
            l.content
                .strip_prefix("Model: ")
                .or_else(|| l.content.strip_prefix("model: "))
                .map(|m| m.to_string())
        } else {
            None
        }
    })
}

/// 把 executor 报回的 `usage.duration_ms` 统一覆盖成 wall-clock 实际耗时。
///
/// 设计意图（issue #513 之后）：
/// - 不同 executor 自己报的 duration 可能与"spawn 到 child.wait 返回"的实际耗时不一致；
/// - UI / 日志需要的是真实墙钟时间，而不是 executor 内部估算。
/// - usage 为 `None` 时构造一个全 0 + wall-clock duration 的占位，保证 DB 列一定有值。
pub(crate) fn apply_wall_clock_duration(
    usage: Option<ExecutionUsage>,
    execution_start: std::time::Instant,
) -> Option<ExecutionUsage> {
    let wall_clock_duration_ms = execution_start.elapsed().as_millis() as u64;
    match usage {
        Some(mut u) => {
            u.duration_ms = Some(wall_clock_duration_ms);
            Some(u)
        }
        None => Some(ExecutionUsage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            total_cost_usd: None,
            duration_ms: Some(wall_clock_duration_ms),
        }),
    }
}

/// 发送 Started 事件 + 首条 info 日志。
///
/// 这两条信息是前端"执行已开始"的视觉信号：Started 用来切 tab / 滚动日志区，
/// info 日志用来让用户的"日志空"状态立刻出现一行，避免疑惑是否卡住。
pub(crate) fn emit_started_event(
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn CodeExecutor,
    workspace_id: Option<i64>,
) {
    send_event(
        tx,
        ExecEvent::Started {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: executor.executor_type().to_string(),
            workspace_id,
        },
    );
    let entry = ParsedLogEntry::info(format!("Starting {}", executor.executor_type()));
    send_event(
        tx,
        ExecEvent::Output {
            task_id: task_id.to_string(),
            entry,
            workspace_id,
        },
    );
}

/// 正常完成分支：把 stats、usage、model 写库；status 更新交给 `update_execution_record`。
///
/// 日志写入不在这条路径上：执行过程中 [`crate::log_flusher::LogFlusher`] 已经按阈值 / timer
/// 批量写库，进入本函数前 [`LogFlusher::finalize`] 也已 drain 残余 buffer（详见
/// `run_todo_execution` 的 `RunOutcome::Completed` 分支）。这里再把"全量日志"以
/// `remaining_logs` 传入会触发 [`crate::db::Database::update_execution_record`] 的
/// `insert_execution_logs` 分支，导致每条日志被插两次（issue #653）。因此固定传 `"[]"`。
pub(crate) async fn persist_completion_record(
    db: &Database,
    record_id: i64,
    all_logs: &[ParsedLogEntry],
    success: bool,
    execution_start: std::time::Instant,
) {
    let result_str = get_final_result_from_logs(all_logs).unwrap_or_default();
    let stats = super::log_capture::extract_execution_stats(all_logs, None);
    if let Ok(stats_json) = serde_json::to_string(&stats) {
        let _ = db
            .update_execution_record_stats(record_id, &stats_json)
            .await;
    }
    let final_status = if success {
        crate::models::ExecutionStatus::Success.as_str()
    } else {
        crate::models::ExecutionStatus::Failed.as_str()
    };
    // wall-clock duration 覆盖交给 helper 集中处理，避免三个终态分支各自维护。
    // 统一从 execution_events pipeline 解析的 tokens 日志条目中获取 usage。
    let usage = apply_wall_clock_duration(
        get_usage_from_tokens_logs(all_logs),
        execution_start,
    );
    let model = get_model_from_logs(all_logs);
    // remaining_logs 故意传 "[]"：日志已由 LogFlusher 全部入库，再传全量会导致重复插入。
    let _ = db
        .update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: record_id,
            status: final_status,
            remaining_logs: "[]",
            result: &result_str,
            usage: usage.as_ref(),
            model: model.as_deref(),
            review_meta: None,
        })
        .await;
}

/// executor 后置 todo_progress 钩子：把 executor 内部 state 推出的进度写库 + 发事件。
///
/// 部分 executor（如 hermes）不在 stdout 中暴露 tool call，但内部已经累积了
/// todo_progress —— 这里给它们一个补 push 的口子。
pub(crate) async fn emit_post_execution_todo_progress(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    executor: &dyn CodeExecutor,
    task_id: &str,
    record_id: i64,
    workspace_id: Option<i64>,
) {
    if let Some(progress) = executor.post_execution_todo_progress() {
        if let Ok(progress_json) = serde_json::to_string(&progress) {
            let _ = db
                .update_execution_record_todo_progress(record_id, &progress_json)
                .await;
            send_event(
                tx,
                ExecEvent::TodoProgress {
                    task_id: task_id.to_string(),
                    progress,
                    workspace_id,
                },
            );
        }
    }
}

/// cancel 分支末段：写 DB（cancelled/failed + 空 result） + 发 Output/Finished 事件 + remove task。
///
/// 抽出来让 cancel 分支的 select! 臂保持 ≤ 30 行：杀进程 + drain 已经抽到
/// `drain_readers_and_flush`，剩下的就是"DB + 事件 + cleanup"。
///
/// 日志写入不再走 `remaining_logs`：进入本函数前 `drain_readers_and_flush` 已经调用
/// `log_flusher.finalize()` 把残余 buffer 一次性入库；再传全量日志会触发
/// `update_execution_record` 的 `insert_execution_logs` 分支重复插入（issue #653）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_cancellation_branch(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn CodeExecutor,
    record_id: i64,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
    workspace_id: Option<i64>,
) {
    let _ = db
        .update_todo_status(todo_id, crate::models::TodoStatus::Cancelled)
        .await;
    let _ = db.update_todo_task_id(todo_id, None).await;
    let _ = db
        .update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: record_id,
            status: crate::models::ExecutionStatus::Failed.as_str(),
            // 日志已由 LogFlusher 全部入库；传 "[]" 避免重复插入。
            remaining_logs: "[]",
            result: "任务已被手动停止",
            usage: None,
            model: None,
            review_meta: None,
        })
        .await;
    let entry = ParsedLogEntry::error("Execution cancelled by user");
    send_event(
        tx,
        ExecEvent::Output {
            task_id: task_id.to_string(),
            entry,
            workspace_id,
        },
    );
    send_event(
        tx,
        ExecEvent::Finished {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: executor.executor_type().to_string(),
            success: false,
            result: Some("Task was cancelled by user".to_string()),
            feishu_bot_id,
            feishu_receive_id,
            workspace_id,
            duration_secs: 0,
            total_tokens: 0,
            // cancel 分支无法拿到原始 trigger_type（参数链已断），传 None。
            // 取消是失败终态，黑板更新只在 success 时触发，不影响。
            trigger_type: None,
        },
    );
    task_manager.remove(task_id).await;
}

/// timeout 分支末段：写 DB（failed + 包含超时常量文案） + 发 Output/Finished 事件 + remove task。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_timeout_branch(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn CodeExecutor,
    record_id: i64,
    execution_timeout_secs: u64,
    timeout_str: String,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
    workspace_id: Option<i64>,
) {
    tracing::warn!(
        "Execution timeout, terminating process: timeout={}s, todo_id={}, task_id={}",
        execution_timeout_secs, todo_id, task_id
    );
    let _ = db
        .update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: record_id,
            status: crate::models::ExecutionStatus::Failed.as_str(),
            // 日志已由 LogFlusher 全部入库；传 "[]" 避免重复插入。
            remaining_logs: "[]",
            result: "Execution timeout",
            usage: None,
            model: None,
            review_meta: None,
        })
        .await;
    let entry = ParsedLogEntry::error("Execution timeout, process terminated by system");
    send_event(
        tx,
        ExecEvent::Output {
            task_id: task_id.to_string(),
            entry,
            workspace_id,
        },
    );
    send_event(
        tx,
        ExecEvent::Finished {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: executor.executor_type().to_string(),
            success: false,
            result: Some(format!("Execution timeout, exceeded {}", timeout_str)),
            feishu_bot_id,
            feishu_receive_id,
            workspace_id,
            duration_secs: 0,
            total_tokens: 0,
            // timeout 分支与 cancel 一样：失败终态不触发黑板，传 None 即可。
            trigger_type: None,
        },
    );
    task_manager.remove(task_id).await;
}

/// 正常完成末段：auto-review + finish_todo_execution + 末段事件 + remove task。
///
/// auto-review 仅在 `trigger_type != "auto_review"` 时启动（防止评审实例自身再触发评审）。
/// 从 DB 查询 record 的 usage 获取 duration 和 tokens，传给 emit_completion_events。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn finalize_normal_completion(
    db: Arc<Database>,
    executor_registry: Arc<crate::adapters::ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
    executor: Arc<dyn CodeExecutor>,
    task_id: String,
    todo_id: i64,
    todo_title: String,
    record_id: i64,
    success: bool,
    exit_code: i32,
    result_str: String,
    trigger_type: String,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
    workspace_id: Option<i64>,
) {
    // ===== 自动评审 (auto-review) =====
    // 仅在以下条件同时满足时启动:
    //   - trigger_type != "auto_review" 避免评审实例本身反向触发评审
    //   - 正常执行 (success/failed), 不是被中断
    maybe_run_auto_review(
        &db,
        &executor_registry,
        &tx,
        &task_manager,
        &config,
        todo_id,
        record_id,
        &trigger_type,
    )
    .await;
    let _ = db.finish_todo_execution(todo_id, success).await;

    // 从 DB 查询 record 的 usage，获取 duration 和 tokens
    let (duration_secs, total_tokens) = match db.get_execution_record(record_id).await {
        Ok(Some(record)) => {
            let dur = record.usage.as_ref()
                .and_then(|u| u.duration_ms)
                .map(|ms| (ms / 1000) as i64)
                .unwrap_or(0);
            let tok = record.usage.as_ref()
                .map(|u| (u.input_tokens + u.output_tokens) as i64)
                .unwrap_or(0);
            (dur, tok)
        }
        _ => {
            // 查询失败时降级为 0，不阻塞正常完成流程
            tracing::warn!("查询执行记录 usage 失败, record_id={}, 降级为 0", record_id);
            (0, 0)
        }
    };

    emit_completion_events(
        &tx,
        &executor,
        &task_id,
        todo_id,
        &todo_title,
        success,
        exit_code,
        &result_str,
        feishu_bot_id,
        feishu_receive_id,
        workspace_id,
        duration_secs,
        total_tokens,
        Some(trigger_type.clone()),
    );
    task_manager.remove(&task_id).await;

    // ===== 黑板更新 (blackboard) =====
    // 任务执行成功后，将 execution_record_id 追加到 pending 队列，由 debouncer 周期汇总触发。
    // 用 trigger_type 判定"自身"——黑板更新任务的 trigger_type == "blackboard"，
    // 避免无限循环；即使以后新增相同 action_type 的非黑板 todo，也不会被错误地跳过。
    if success && trigger_type != "blackboard" {
        if let Some(ws_id) = workspace_id {
            crate::services::blackboard_debouncer::push_pending_record(ws_id, record_id, &db).await;
        }
    }
}

/// 构建黑板防抖状态事件（用于 WebSocket 推送）。
/// 从 DB 读取 pending 队列，从 debouncer 读取 timer 状态。
/// `is_refreshing` 由调用方根据 refreshing_workspaces 集合传入，
/// 确保 ticker 不会覆盖 spawned task 发出的 refreshing=true 状态。
async fn build_blackboard_status(
    db: &Database,
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

    let remaining_secs = crate::services::blackboard_debouncer::get_timer_state(workspace_id)
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
        let event = build_blackboard_status(db, *ws_id, debounce_secs, debounce_count, is_refreshing).await;
        let _ = tx.send(event);
    }
}

/// 派生独立 worker 任务执行 wiki 更新。
///
/// worker 内部循环处理：每次非破坏性读取 pending 队列，
/// 处理成功后移除已处理 ID（保留期间新到达的记录），
/// 若队列仍有剩余则继续处理下一批。
/// 失败时保留队列不删除，退出循环避免死循环。
#[allow(clippy::too_many_arguments)]
fn spawn_flush_worker(
    ws_id: i64,
    debounce_secs: i64,
    debounce_count: i64,
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
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
            let all_record_ids = match db.get_blackboard(ws_id).await {
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
            let _ = tx.send(ExecEvent::BlackboardDebounceStatus {
                workspace_id: ws_id,
                pending_count: all_record_ids.len() as u64,
                threshold: debounce_count as u64,
                debounce_secs: debounce_secs as u64,
                remaining_secs: -1,
                refreshing: true,
            });

            let update_result = crate::services::blackboard::update_blackboard_wiki(
                db.clone(),
                executor_registry.clone(),
                tx.clone(),
                task_manager.clone(),
                config.clone(),
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
        let final_pending_count = match db.get_blackboard(ws_id).await {
            Ok(Some(board)) => {
                serde_json::from_str::<Vec<i64>>(&board.pending_record_ids)
                    .map(|v| v.len() as u64)
                    .unwrap_or(0)
            }
            _ => 0,
        };

        // 广播 refreshing=false 状态（携带真实 pending_count）
        let _ = tx.send(ExecEvent::BlackboardDebounceStatus {
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
            crate::services::blackboard_debouncer::restart_timer(ws_id, &db).await;
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
#[allow(clippy::too_many_arguments)]
async fn handle_flush_msg(
    msg: crate::services::blackboard_debouncer::BlackboardFlushMsg,
    db: &Arc<Database>,
    executor_registry: &Arc<ExecutorRegistry>,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &Arc<TaskManager>,
    config: &Arc<std::sync::RwLock<crate::config::Config>>,
    refreshing_workspaces: &Arc<tokio::sync::Mutex<std::collections::HashSet<i64>>>,
    known_workspaces: &mut Vec<i64>,
) {
    let ws_id = msg.workspace_id;

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
    let (debounce_secs, debounce_count) = get_workspace_debounce(db, ws_id).await;

    spawn_flush_worker(
        ws_id,
        debounce_secs,
        debounce_count,
        db.clone(),
        executor_registry.clone(),
        tx.clone(),
        task_manager.clone(),
        config.clone(),
        refreshing_workspaces.clone(),
    );
}

/// 启动黑板 flush 监听器：
/// - 监听 debouncer channel，收到消息后 spawn 独立任务执行 update_blackboard_wiki
/// - 每秒通过 broadcast::tx 推送一次 BlackboardDebounceStatus 事件
/// - per-workspace 互斥：同一 workspace 同时只运行一个 wiki 更新 worker
///
/// 防抖阈值（周期秒数、条数阈值）从 per-workspace 黑板配置（blackboards 表）读取，
/// 实现工作空间隔离。
pub async fn blackboard_flush_listener(
    mut rx: tokio::sync::mpsc::Receiver<crate::services::blackboard_debouncer::BlackboardFlushMsg>,
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
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
        let rescan_timer = db.clone();
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
                            crate::services::blackboard_debouncer::restart_timer(
                                board.workspace_id,
                                &rescan_timer,
                            )
                            .await;
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
                    &db, &tx, &mut known_workspaces, &refreshing_workspaces,
                ).await;
            }

            // flush 消息：非破坏性读取 pending 队列并派生 worker
            msg = rx.recv() => {
                match msg {
                    Some(msg) => {
                        handle_flush_msg(
                            msg, &db, &executor_registry, &tx, &task_manager, &config,
                            &refreshing_workspaces, &mut known_workspaces,
                        ).await;
                    }
                    None => break,
                }
            }
        }
    }
}

/// 仅在 trigger_type != "auto_review" 时启动自动评审，避免评审实例反向触发评审。
#[allow(clippy::too_many_arguments)]
async fn maybe_run_auto_review(
    db: &Arc<Database>,
    executor_registry: &Arc<crate::adapters::ExecutorRegistry>,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &Arc<TaskManager>,
    config: &Arc<std::sync::RwLock<crate::config::Config>>,
    todo_id: i64,
    record_id: i64,
    trigger_type: &str,
) {
    if trigger_type == "auto_review" || todo_id == 0 {
        return;
    }
    run_auto_review(
        db.clone(),
        executor_registry.clone(),
        tx.clone(),
        task_manager.clone(),
        config.clone(),
        todo_id,
        record_id,
    )
    .await;
}

/// 末段事件：Output (executor finished) + Finished。
///
/// `duration_secs` 和 `total_tokens` 由调用方从 DB 查询 usage 后传入；
/// 异常路径（cancel/timeout/spawn 失败等）传 0 即可。
/// `trigger_type` 透传本次执行的触发类型，供下游识别"自身"避免递归（如 blackboard）。
#[allow(clippy::too_many_arguments)]
fn emit_completion_events(
    tx: &broadcast::Sender<ExecEvent>,
    executor: &Arc<dyn CodeExecutor>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    success: bool,
    exit_code: i32,
    result_str: &str,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
    workspace_id: Option<i64>,
    duration_secs: i64,
    total_tokens: i64,
    trigger_type: Option<String>,
) {
    let entry = ParsedLogEntry::new(
        if success { "info" } else { "error" },
        format!(
            "Executor finished with exit_code: {}, result: {}",
            exit_code, result_str
        ),
    );
    send_event(
        tx,
        ExecEvent::Output {
            task_id: task_id.to_string(),
            entry,
            workspace_id,
        },
    );
    send_event(
        tx,
        ExecEvent::Finished {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: executor.executor_type().to_string(),
            success,
            result: Some(result_str.to_string()),
            feishu_bot_id,
            feishu_receive_id,
            workspace_id,
            duration_secs,
            total_tokens,
            trigger_type,
        },
    );
}

/// 格式化超时秒数为人类可读字符串。
///
/// 使用 hours for >=60 min, days for >=24 h to keep the output readable.
/// 精度取舍：只精确到分钟级别（秒数只在 <60s 时显示），后端 timeout 精度
/// 为秒级，分钟以上的秒数误差在 UI 上无感知差异。
pub(crate) fn format_timeout_secs(secs: u64) -> String {
    let total_min = secs / 60;
    let remaining_secs = secs % 60;
    if remaining_secs == 0 {
        if total_min >= 1440 {
            format!("{} day(s)", total_min / 1440)
        } else if total_min >= 60 {
            format!("{} hour(s)", total_min / 60)
        } else {
            format!("{} min", total_min)
        }
    } else {
        format!("{} min {} sec", total_min, remaining_secs)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_wall_clock_duration_overrides_executor_report() {
        // Some 情况：override duration_ms，其他字段保留。
        let usage = ExecutionUsage {
            input_tokens: 10,
            output_tokens: 20,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            total_cost_usd: None,
            duration_ms: Some(999),
        };
        let start = std::time::Instant::now();
        // sleep 1ms 确保 wall_clock > 0（避免极端情况下 elapsed=0 被实现忽略）
        std::thread::sleep(std::time::Duration::from_millis(1));
        let updated = apply_wall_clock_duration(Some(usage.clone()), start).unwrap();
        assert_eq!(updated.input_tokens, 10);
        assert_eq!(updated.output_tokens, 20);
        // 关键断言：duration 一定是 wall-clock，不是 executor 报的 999。
        let wall = updated.duration_ms.unwrap();
        assert!(wall < 999, "wall-clock should override executor-reported 999");
        assert!(wall >= 1);

        // None 情况：构造全 0 + wall-clock 的占位 usage。
        let start2 = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let placeholder = apply_wall_clock_duration(None, start2).unwrap();
        assert_eq!(placeholder.input_tokens, 0);
        assert_eq!(placeholder.output_tokens, 0);
        assert!(placeholder.duration_ms.unwrap() >= 1);
    }

    #[test]
    fn test_format_timeout_secs() {
        assert_eq!(format_timeout_secs(0), "0 min");
        assert_eq!(format_timeout_secs(60), "1 min");
        assert_eq!(format_timeout_secs(90), "1 min 30 sec");
        assert_eq!(format_timeout_secs(3600), "1 hour(s)");
        assert_eq!(format_timeout_secs(86400), "1 day(s)");
        assert_eq!(format_timeout_secs(7200), "2 hour(s)");
    }

    #[test]
    fn test_format_timeout_secs_edges_under_minute() {
        assert_eq!(format_timeout_secs(0), "0 min");
        assert_eq!(format_timeout_secs(1), "0 min 1 sec");
        assert_eq!(format_timeout_secs(59), "0 min 59 sec");
    }

    #[test]
    fn test_format_timeout_secs_exact_minutes() {
        assert_eq!(format_timeout_secs(60), "1 min");
        assert_eq!(format_timeout_secs(120), "2 min");
        assert_eq!(format_timeout_secs(3540), "59 min");
        // 60 min 是 3600 秒，进 hour 分支。
        assert_eq!(format_timeout_secs(3600), "1 hour(s)");
    }
}