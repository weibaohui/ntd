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

use crate::adapters::CodeExecutor;
use crate::db::Database;
use crate::executor_service::ExecEvent;
use crate::models::{ExecutionUsage, ParsedLogEntry};

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
    // 多 Agent 协作：完成态一次性扫描日志写入 agent_runs（抽到 helper 控制本函数行数 ≤30）。
    persist_agent_runs(db, record_id, all_logs, success).await;
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

/// 完成态把多 Agent 子 agent 元数据写入 `execution_records.agent_runs`。
///
/// 与 todo_progress 平行，但 todo_progress 是实时写、这里是完成时一次性写（用户选 2b）。
/// `success` 透传：成功记录子 agent 标 completed，失败标 unknown。空结果不写。
async fn persist_agent_runs(
    db: &Database,
    record_id: i64,
    all_logs: &[ParsedLogEntry],
    success: bool,
) {
    let agent_runs = crate::agent_progress::extract_agent_runs(all_logs, success);
    if agent_runs.is_empty() {
        return;
    }
    if let Ok(agent_runs_json) = serde_json::to_string(&agent_runs) {
        let _ = db
            .update_execution_record_agent_runs(record_id, &agent_runs_json)
            .await;
    }
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
/// 093-B3：12 参塌缩为 `&SpawnRuntime`（全部字段都是 runtime 成员的逐字段解包）。
pub(crate) async fn handle_cancellation_branch(
    runtime: &crate::executor_service::types::SpawnRuntime,
) {
    // 解出原名局部量保持函数体零改动（引用取向，零克隆）：
    // 每个局部量的名字与原位置参数逐一相同，下游函数体因此一行不用改，
    // 重构期 diff 核对「行为等价」时只需检查本解包段
    let db = runtime.db.as_ref();
    let tx = &runtime.tx;
    let task_manager = runtime.task_manager.as_ref();
    let task_id = runtime.task_id.as_str();
    let todo_id = runtime.todo_id;
    let todo_title = runtime.todo_title.as_str();
    // 取消/超时/失败路径共用 runtime.executor_spawn（spawn 期选定的执行器实例）；
    // 与 SpawnContext.executor 同源，分支里只用其元数据（不再发起执行）
    let executor = runtime.executor_spawn.as_ref();
    let record_id = runtime.record_id;
    let feishu_bot_id = runtime.feishu_bot_id;
    // feishu_receive_id(_type) clone 原因同 emit_completion_events：
    // Finished 事件 owned，生命周期独立于 runtime 借用
    let feishu_receive_id = runtime.feishu_receive_id.clone();
    let feishu_receive_id_type = runtime.feishu_receive_id_type.clone();
    // workspace_id 嵌在 prepared.request 里而非平铺——设计取舍见 PreparedExecution
    // 字段注释（嵌入 request 整段，加字段时只动 1 处）
    let workspace_id = runtime.prepared.request.workspace_id;
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
            feishu_receive_id_type,
            workspace_id,
            duration_secs: 0,
            total_tokens: 0,
            // cancel 分支无法拿到原始 trigger_type（参数链已断），传 None。
            // 取消是失败终态，黑板更新只在 success 时触发，不影响。
            trigger_type: None,
        },
    );
    task_manager.remove(task_id).await;
    // 056：终态落定后主动失效 dashboard 缓存，统计立刻可见
    crate::handlers::execution::invalidate_dashboard_cache().await;
}

/// timeout 分支末段：写 DB（failed + 包含超时常量文案） + 发 Output/Finished 事件 + remove task。
/// 093-B3：14 参塌缩为 `&SpawnRuntime`；timeout_str 由 runtime.execution_timeout_secs
/// 在函数内现算（原由调用点格式化后传入，是 Replace Parameter with Query 的应用）。
pub(crate) async fn handle_timeout_branch(
    runtime: &crate::executor_service::types::SpawnRuntime,
) {
    // 同名解包段与 handle_cancellation_branch 同构（理由见其注释）；
    // 刻意保持两分支解包顺序一致——diff 对照时任何字段缺失都一眼可见
    let db = runtime.db.as_ref();
    let tx = &runtime.tx;
    let task_manager = runtime.task_manager.as_ref();
    let task_id = runtime.task_id.as_str();
    let todo_id = runtime.todo_id;
    let todo_title = runtime.todo_title.as_str();
    let executor = runtime.executor_spawn.as_ref();
    let record_id = runtime.record_id;
    let execution_timeout_secs = runtime.execution_timeout_secs;
    // timeout_str 函数内现算（Replace Parameter with Query）：
    // 调用点原本也只是把 execution_timeout_secs 格式化一遍，参数传递纯属转发；
    // 收进函数后格式化口径全仓唯一，不会出现两处文案漂移
    let timeout_str = format_timeout_secs(execution_timeout_secs);
    let feishu_bot_id = runtime.feishu_bot_id;
    let feishu_receive_id = runtime.feishu_receive_id.clone();
    let feishu_receive_id_type = runtime.feishu_receive_id_type.clone();
    let workspace_id = runtime.prepared.request.workspace_id;
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
            feishu_receive_id_type,
            workspace_id,
            duration_secs: 0,
            total_tokens: 0,
            // timeout 分支与 cancel 一样：失败终态不触发黑板，传 None 即可。
            trigger_type: None,
        },
    );
    task_manager.remove(task_id).await;
    // 056：终态落定后主动失效 dashboard 缓存
    crate::handlers::execution::invalidate_dashboard_cache().await;
}

/// 正常完成末段：auto-review + finish_todo_execution + 末段事件 + remove task。
///
/// auto-review 仅在 `trigger_type != "auto_review"` 时启动（防止评审实例自身再触发评审）。
/// 从 DB 查询 record 的 usage 获取 duration 和 tokens，传给 emit_completion_events。
/// 093-B3：19 个位置参数塌缩为 `&SpawnContext`（全部字段本来就从 ctx 逐字段解包）
/// + `CompletionOutcome`（success/exit_code/result_str 数据团聚合）。
pub(crate) async fn finalize_normal_completion(
    ctx: &crate::executor_service::types::SpawnContext,
    outcome: crate::executor_service::types::CompletionOutcome,
) {
    // 从 ctx/outcome 解出与原签名同名的局部量，保持函数体零改动。
    // Arc::clone 只是原子计数自增，成本与原调用点逐字段 clone 完全一致。
    let db = ctx.db.clone();
    let executor_registry = ctx.executor_registry.clone();
    let tx = ctx.tx.clone();
    let task_manager = ctx.task_manager.clone();
    let config = ctx.config.clone();
    let executor = ctx.executor.clone();
    let task_id = ctx.task_id.clone();
    let todo_id = ctx.todo_id;
    let record_id = ctx.record_id;
    let success = outcome.success;
    // result_str 克隆而非 move：emit_completion_events 还需借用整个 outcome，
    // 提前 move 会触发部分移动错误；一次 String 克隆的成本可忽略
    let result_str = outcome.result_str.clone();
    let trigger_type = ctx.trigger_type.clone();
    let workspace_id = ctx.workspace_id;
    let expert_manager = ctx.expert_manager.clone();
    // todo_title / exit_code / feishu_* 不再单独解包：它们只被 emit_completion_events
    // 使用，该函数已直接读 ctx/outcome
    // ===== 自动评审 (auto-review) =====
    // 仅在以下条件同时满足时启动:
    //   - trigger_type != "auto_review" 避免评审实例本身反向触发评审
    //   - 正常执行 (success/failed), 不是被中断
    maybe_run_auto_review(
        // 096-W2-PR3：五元组依赖由 ctx 提取（execution_deps 方法），不再逐字段解包传递
        &ctx.execution_deps(),
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

    // 完成事件必须放在 usage 查询之后发送：事件载荷要携带 duration/tokens，
    // 提前发送会让飞书/前端收到缺统计信息的完成卡片（CodeRabbit #1008 评审点）
    emit_completion_events(ctx, &outcome, duration_secs, total_tokens);
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

    // ===== 讨论帖回写 (discussion / discussion_auto) =====
    // 任务讨论区 @触发的执行（trigger_type ∈ {discussion, discussion_auto}）：把结论回写到
    // 对应的智能体占位帖，并软删载体 todo（隐藏兜底）。回写失败只记 warn，不影响执行本身的
    // 成功落定（帖子可由前端轮询兜底）。与 auto_review/blackboard 同级的并列分支，按 trigger_type 分派。
    if trigger_type == "discussion" || trigger_type == "discussion_auto" {
        writeback_discussion_post(&db, &executor, record_id, success, &result_str).await;
    }

    // ===== 自动接力 (delegate relay，需求 092 P2) =====
    // 仅讨论类执行（discussion/discussion_auto）成功后、且本路径持有专家索引时，尝试接力。
    // continue_delegated_task 内部会再校验「delegate + auto_continue + 专家 assignee」，
    // 不满足则静默跳过（环路任务 / 手动单跑 / 执行器委派都直接返回，零副作用）。
    if success && (trigger_type == "discussion" || trigger_type == "discussion_auto") {
        if let Some(em) = expert_manager {
            let handles = crate::handlers::task_posts::DelegateRelayHandles {
                db: &db,
                executor_registry: &executor_registry,
                tx: &tx,
                task_manager: &task_manager,
                config: &config,
                expert_manager: &em,
            };
            crate::handlers::task_posts::continue_delegated_task(&handles, record_id, &result_str)
                .await;
        }
    }
}

/// 讨论帖回写（对称 maybe_run_auto_review）：@触发执行的结论落定后，把结果回写到对应
/// 智能体占位帖并软删载体 todo。回写失败只记 warn，不阻断执行成功落定（前端轮询兜底）。
async fn writeback_discussion_post(
    db: &Arc<Database>,
    executor: &Arc<dyn CodeExecutor>,
    record_id: i64,
    success: bool,
    result_str: &str,
) {
    // 补全执行器名：@专家 占位帖创建时 executor=None（用默认执行器承载、人设由专家决定），
    // 回写时从实际执行的 CodeExecutor 取规范名补上徽标（review c1，对齐 DAO 设计意图）。
    let executor_name = executor.executor_type().to_string();
    if let Err(e) = db
        .finalize_discussion_post(record_id, success, result_str, Some(&executor_name))
        .await
    {
        tracing::warn!(error = %e, record_id, "finalize discussion post failed");
    }
}

/// 仅在 trigger_type != "auto_review" 时启动自动评审，避免评审实例反向触发评审。
///
/// 设计 034 统一路径：所有 loop_stage 步骤（无论 min_rating 是否有值）都走统一评审，
/// 查出 step 内联 `review_prompt` 和所属 loop 的 `review_template_id` 传给
/// `run_auto_review`，由 `resolve_review_template` 实现三级回退。
async fn maybe_run_auto_review(
    // 096-W2-PR3：五元组依赖已对象化（ExecutionDeps），借用现场按引用接参
    deps: &super::types::ExecutionDeps,
    todo_id: i64,
    record_id: i64,
    trigger_type: &str,
) {
    if trigger_type == "auto_review" || todo_id == 0 {
        return;
    }

    // 环路步骤：查出 step 内联 review_prompt，用于评审 prompt 回退。
    // 044：loops.review_template_id 已下线（评审模板归环节），环路级模板回退取消，
    // 非环路步骤传 None，回退到默认模板。
    // step_review_prompt 非空时会被 auto_review 直接优先使用（作为评审 prompt 的 system 指令），
    // 仅当其为空时才回退到默认模板。只对 loop_stage 类型的触发查环节表，其他触发直接取默认。
    let step_review_prompt = if trigger_type.starts_with("loop_stage") {
        // loop_stage 触发：按当前 todo 反查启用中的 loop_step，取其中联的 review_prompt。
        // 一个 todo 至多被一个启用中的 step 引用，用 LIMIT 1 取首个。
        // 查询返回 None（无关联 step/step 未启用）→ auto_review 走默认模板，不阻塞评审。
        deps.db.find_loop_step_review_prompt_by_todo(todo_id)
            .await
            .ok()       // 查询出错（如 DB 连接断开）转为 None，不阻断 auto_review 流程
            .flatten()  // 解开 Option<Option<String>> 的嵌套：外层 Option 是 Result 转换结果，
                        // 内层是 SQL 查询的 nullable 列值，flatten 把两层合并为 Option<String>
    } else {
        // 非 loop_stage 触发（如手动 auto_review、todo 变更触发等）：无环节级模板，传 None
        None
    };

    run_auto_review(
        deps.clone(),
        todo_id,
        record_id,
        step_review_prompt,
        // 044：环路级评审模板已下线，统一回退到默认模板
        None,
    )
    .await;
}

/// 末段事件：Output (executor finished) + Finished。
///
/// `duration_secs` 和 `total_tokens` 由调用方从 DB 查询 usage 后传入；
/// 异常路径（cancel/timeout/spawn 失败等）传 0 即可。
/// `trigger_type` 透传本次执行的触发类型，供下游识别"自身"避免递归（如 blackboard）。
/// 093-B3：15 参塌缩为 4 参（ctx 承载任务/飞书/workspace/trigger 字段，
/// outcome 承载终态三元组，仅 duration/tokens 是调用点计算值）。
fn emit_completion_events(
    ctx: &crate::executor_service::types::SpawnContext,
    outcome: &crate::executor_service::types::CompletionOutcome,
    duration_secs: i64,
    total_tokens: i64,
) {
    // 解出原名局部量保持函数体零改动（引用取向，零克隆）
    let tx = &ctx.tx;
    let executor = &ctx.executor;
    // 字符串字段用 as_str() 借用而非 clone：本函数只读不持有，省一次堆分配
    let task_id = ctx.task_id.as_str();
    let todo_id = ctx.todo_id;
    let todo_title = ctx.todo_title.as_str();
    let success = outcome.success;
    let exit_code = outcome.exit_code;
    let result_str = outcome.result_str.as_str();
    let feishu_bot_id = ctx.feishu_bot_id;
    // feishu_receive_id(_type) 必须 clone：ExecEvent::Finished 是 owned 结构，
    // 跨 await/广播后事件生命周期独立于 ctx，借用会悬垂
    let feishu_receive_id = ctx.feishu_receive_id.clone();
    let feishu_receive_id_type = ctx.feishu_receive_id_type.clone();
    let workspace_id = ctx.workspace_id;
    // trigger_type 包装成 Some：ExecEvent 的该字段是 Option<String>，
    // ctx 侧恒有值——包装点集中在此，下游不再判空
    let trigger_type = Some(ctx.trigger_type.clone());
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
            feishu_receive_id_type,
            workspace_id,
            duration_secs,
            total_tokens,
            trigger_type,
        },
    );
    // 056：正常终态落定后主动失效 dashboard 缓存。
    // emit_completion_events 是 sync fn 无法 await，spawn 异步失效（下一拍执行）。
    tokio::spawn(crate::handlers::execution::invalidate_dashboard_cache());
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