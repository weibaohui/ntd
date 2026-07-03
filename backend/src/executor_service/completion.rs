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
    );
    task_manager.remove(&task_id).await;

    // ===== 黑板更新 (blackboard) =====
    // 任务执行成功后，异步更新黑板内容（仅当源任务不是 blackboard update todo 时）。
    // 黑板更新任务自身完成后也会触发此 hook，但通过 action_type 检查避免无限循环。
    if success {
        if let Some(ws_id) = workspace_id {
            // 查询当前 todo 的 action_type，跳过黑板更新任务自身
            let is_blackboard = db
                .get_todo(todo_id)
                .await
                .ok()
                .flatten()
                .and_then(|t| t.action_type)
                .map(|at| at == "blackboard")
                .unwrap_or(false);

            if !is_blackboard {
                spawn_blackboard_update(
                    db.clone(),
                    executor_registry.clone(),
                    tx.clone(),
                    task_manager.clone(),
                    config.clone(),
                    ws_id,
                    &result_str,
                    todo_id,
                    &todo_title,
                );
            }
        }
    }
}

/// 在后台异步触发黑板更新。
///
/// 将 async 调用包裹在独立的非 async 函数中，避免 Rust 编译器的 async 递归类型循环
/// （`finalize_normal_completion` → `update_blackboard` → `run_todo_execution` → `dispatch_spawned_executor_task` → `finalize_normal_completion`）。
fn spawn_blackboard_update(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
    workspace_id: i64,
    result_str: &str,
    todo_id: i64,
    todo_title: &str,
) {
    let result_str = result_str.to_string();
    let todo_title = todo_title.to_string();
    tokio::spawn(async move {
        if let Err(e) = crate::services::blackboard::update_blackboard(
            db,
            executor_registry,
            tx,
            task_manager,
            config,
            workspace_id,
            &result_str,
            todo_id,
            &todo_title,
        )
        .await
        {
            tracing::warn!("黑板更新失败: todo_id={}, error={:?}", todo_id, e);
        }
    });
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