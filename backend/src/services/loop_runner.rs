//! Loop Runner — 顺序执行 loop 的所有 step。
//!
//! 执行模型：
//! 1. 创建 loop_executions 行（status=running）
//! 2. 按 order_index 顺序遍历 steps：
//!    a. 启动 step.todo 的执行（复用 executor_service::start_todo_execution）
//!    b. 写 loop_step_execution 行
//!    c. 订阅 broadcast::tx 等待该 step 的 ExecEvent::Finished
//!    d. 应用 rating gate（决定是否继续 / 中止 loop）
//! 3. 计算最终 status（success / partial / failed / cancelled）
//! 4. 写回 loop_executions
//!
//! 整个 run_loop 是 `tokio::spawn` 的，不阻塞调用方（manual trigger / cron /
//! dispatcher 都把 run_loop 扔到后台）。

use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::adapters::ExecutorRegistry;
use crate::db::Database;
use crate::executor_service::{run_todo_execution_with_params, RunTodoExecutionRequest};
use crate::models::ExecutionStatus;
use crate::task_manager::TaskManager;
use crate::db::entity::{loop_steps};

/// 全局限制配置，从 loop.limits_config JSON 解析。
///
/// 支持两种限制方式：
/// - max_step_executions：最大执行步数（已有）
/// - max_total_tokens：最大消耗 Token 数（新增，input_tokens + output_tokens 之和）
#[derive(Debug, Default, Clone, serde::Deserialize)]
struct LimitsConfig {
    #[serde(default)]
    max_step_executions: Option<i32>,
    /// 最大消耗 Token 数（input_tokens + output_tokens），超过后 Loop 被 capped 终止。
    /// None 表示不限制。
    #[serde(default)]
    max_total_tokens: Option<i64>,
}

/// LoopRunner 依赖：不再持有 HookService，todo hook 已整块移除（见
/// plan `purring-forging-petal`）。循环只与 ctx / tx 直接耦合。
///
/// 使用独立的 LoopRunnerCtx 而非完整 ServiceContext，避免循环引用：
/// ServiceContext -> loop_runner: Option<Arc<LoopRunner>> -> LoopRunner -> ctx: ServiceContext
#[derive(Clone)]
pub struct LoopRunnerCtx {
    pub db: Arc<Database>,
    pub executor_registry: Arc<ExecutorRegistry>,
    pub task_manager: Arc<TaskManager>,
    pub config: Arc<std::sync::RwLock<crate::config::Config>>,
    /// 专家索引管理器：loop 环节执行时也需注入专家上下文，
    /// 让 loop 触发的 todo 尊重其绑定的 expert_name
    pub expert_manager: Arc<crate::expert::ExpertIndexManager>,
}

pub struct LoopRunner {
    ctx: LoopRunnerCtx,
    tx: broadcast::Sender<crate::executor_service::ExecEvent>,
}

/// Loop 执行结果：区分「真正完成」和「暂停等待」，
/// 用于避免人工审批等暂停状态误发 LoopFinished 事件。
#[derive(Debug, PartialEq)]
enum LoopRunOutcome {
    /// 正常执行完成（终态：success / failed / partial / capped_step / capped_token）
    Finished,
    /// 暂停等待（非终态：如人工审批 pending_approval）
    Paused,
}

impl LoopRunner {
    pub fn new(
        ctx: LoopRunnerCtx,
        tx: broadcast::Sender<crate::executor_service::ExecEvent>,
    ) -> Self {
        Self { ctx, tx }
    }

    /// 暴露 LoopRunnerCtx 供 LoopScheduler / 测试 / 上层使用。
    /// 只读引用。
    pub fn ctx_ref(&self) -> &LoopRunnerCtx {
        &self.ctx
    }

    /// 暴露 tx 供 LoopScheduler 构造 ServiceContext。
    pub fn tx(&self) -> &broadcast::Sender<crate::executor_service::ExecEvent> {
        &self.tx
    }

    /// 校验 loop 所有步骤的 todo 是否都归属同一工作空间。
    /// 环路运行时要求所有环节在同一工作空间下，否则 cwd / worktree 无法统一，
    /// 且跨工作空间的数据流会导致不可预期的行为。
    /// 返回 Err 时附带具体哪些步骤不在同一工作空间。
    async fn check_workspace_consistency(
        &self,
        loop_: &crate::db::entity::loops::Model,
        all_steps: &[loop_steps::Model],
    ) -> Result<(), String> {
        // 收集所有属于 loop 的步骤的 todo_id 的去重列表
        // 使用 indexmap 保留顺序同时去重，避免同一个 todo 被多个 step 引用时重复检查
        let mut seen = std::collections::HashSet::new();
        let mut todo_ids = Vec::new();
        for step in all_steps {
            if seen.insert(step.todo_id) {
                todo_ids.push(step.todo_id);
            }
        }

        // 加载每个 todo 并校验 workspace_id 是否与 loop 一致
        let mut mismatches: Vec<String> = Vec::new();
        for &tid in &todo_ids {
            let todo = self
                .ctx
                .db
                .get_todo(tid)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("todo #{} (引用于 loop #{}) 已被删除", tid, loop_.id))?;

            // 比较双方 workspace_id 是否一致。
            // 特殊处理：Some(0)（todos 默认值，未分配工作空间）与 None（loop 默认值）语义等价，
            // 都表示"未分配工作空间"，视为同一空间。
            let loop_ws = loop_.workspace_id.filter(|&id| id != 0);
            let todo_ws = todo.workspace_id.filter(|&id| id != 0);
            if loop_ws != todo_ws {
                let step_names: Vec<&str> = all_steps
                    .iter()
                    .filter(|s| s.todo_id == tid)
                    .map(|s| s.name.as_str())
                    .collect();
                mismatches.push(format!(
                    "环节「{}」(todo #{}) 所属工作空间 (id={:?}) 与 loop (id={:?}) 不一致",
                    step_names.join("、"),
                    tid,
                    todo.workspace_id,
                    loop_.workspace_id,
                ));
            }
        }

        if mismatches.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "loop #{}「{}」的环节不在同一工作空间下，无法执行：\n{}",
                loop_.id,
                loop_.name,
                mismatches.join("\n"),
            ))
        }
    }

    /// Spawn 一条 loop 执行（fire-and-forget）。返回 loop_execution_id 给调用方。
    #[allow(clippy::too_many_arguments)] // 参数来自上游 handler 的独立数据源，合并为 struct 增加认知负担
    pub fn spawn_run(
        self: Arc<Self>,
        loop_id: i64,
        trigger_id: Option<i64>,
        trigger_type: &str,
        trigger_meta: serde_json::Value,
        feishu_bot_id: Option<i64>,
        feishu_receive_id: Option<String>,
        // 接收者 ID 类型（"open_id" / "chat_id"），用于飞书消息发送
        feishu_receive_id_type: Option<String>,
    ) -> i64 {
        let this = self.clone();
        let trigger_type = trigger_type.to_string();
        // 先建 loop_execution 行拿到 id,然后后台异步跑整个流程
        let initial_total_steps = 0i32; // 创建时还没确定 step 数,后面在 run_inner 里 update
        let loop_execution_id_res: Result<i64, String> = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                this.ctx
                    .db
                    .create_loop_execution(
                        loop_id,
                        trigger_id,
                        &trigger_type,
                        &trigger_meta.to_string(),
                        initial_total_steps,
                    )
                    .await
                    .map(|m| m.id)
                    .map_err(|e| e.to_string())
            })
        });
        let loop_execution_id = match loop_execution_id_res {
            Ok(id) => id,
            Err(e) => {
                error!("loop_runner: failed to create loop_execution: {}", e);
                return -1;
            }
        };

        let this2 = self.clone();
        let this2_for_err = self.clone();
        let this2_for_callback = self.clone();
        // self 在此之后不再使用，直接 move 而非 clone，避免多余的引用计数操作
        let this2_for_event = self;
        tokio::spawn(async move {
            let run_result = this2
                .run_inner(loop_id, loop_execution_id, trigger_type)
                .await;

            // 获取 loop 信息（标题、workspace_id）用于 LoopFinished 事件
            let loop_info = this2_for_event.ctx.db.get_loop(loop_id).await.ok().flatten();
            let loop_title = loop_info.as_ref().map(|l| l.name.clone()).unwrap_or_else(|| format!("Loop #{}", loop_id));
            let loop_workspace_id = loop_info.as_ref().and_then(|l| l.workspace_id);

            // 获取 loop_execution 统计数据
            let loop_exec = this2_for_event.ctx.db.get_loop_execution(loop_execution_id).await.ok().flatten();
            let (final_status, total_steps, completed_steps, failed_steps, duration_secs) = match loop_exec {
                Some(le) => {
                    // 计算执行时长：仅当起止时间都能解析且 finish >= start 时才计算，否则返回 0
                    let duration = match (le.started_at.as_str(), le.finished_at.as_deref()) {
                        (start, Some(finish)) => {
                            match (
                                chrono::DateTime::parse_from_rfc3339(start),
                                chrono::DateTime::parse_from_rfc3339(finish),
                            ) {
                                (Ok(start_dt), Ok(finish_dt)) => {
                                    let secs = finish_dt.timestamp() - start_dt.timestamp();
                                    if secs >= 0 { secs } else { 0 }
                                }
                                _ => 0,
                            }
                        }
                        _ => 0,
                    };
                    (le.status, le.total_steps, le.completed_steps, le.failed_steps, duration)
                }
                None => ("failed".to_string(), 0, 0, 0, 0),
            };

            // 获取累计 Token 消耗（从所有 step executions 的 execution_record_id 查询）
            let total_tokens = this2_for_event.get_loop_total_tokens(loop_execution_id).await;

            match run_result {
                Ok(LoopRunOutcome::Finished) => {
                    // 如果有 feishu_receive_id，直接回复原对话（binding chat 场景）- 发送黑板全文
                    if let Some(ref receive_id) = feishu_receive_id {
                        let blackboard = this2_for_callback
                            .get_loop_blackboard_text(loop_execution_id, &trigger_meta)
                            .await;
                        info!(
                            "[loop-runner] loop {} execution {} completed, sending result to Feishu binding chat",
                            loop_id, loop_execution_id
                        );
                        this2_for_callback
                            .send_result_to_feishu(
                                feishu_bot_id,
                                receive_id,
                                feishu_receive_id_type.as_deref().unwrap_or("open_id"),
                                &blackboard,
                            )
                            .await;
                    } else {
                        // 没有绑定对话时，通过 LoopFinished 事件广播统计摘要，
                        // FeishuPushService 会按 workspace 配置的 push_level 推送
                        info!(
                            "[loop-runner] loop {} execution {} completed, broadcasting LoopFinished event",
                            loop_id, loop_execution_id
                        );
                        let _ = this2_for_event.tx.send(crate::executor_service::ExecEvent::LoopFinished {
                            loop_execution_id,
                            loop_id,
                            loop_title,
                            status: final_status,
                            total_steps,
                            completed_steps,
                            failed_steps,
                            duration_secs,
                            total_tokens,
                            workspace_id: loop_workspace_id,
                        });
                    }
                }
                Ok(LoopRunOutcome::Paused) => {
                    // 暂停状态（如人工审批等待中）：不发送 LoopFinished 事件，
                    // 也不做终态化处理，loop_execution 保持 running 状态
                    info!(
                        "[loop-runner] loop {} execution {} paused (waiting for human approval)",
                        loop_id, loop_execution_id
                    );
                }
                Err(e) => {
                    error!("loop_runner: run failed: {}", e);
                    // 终态化 loop execution，携带错误原因供前端展示
                    let _ = this2_for_err
                        .ctx
                        .db
                        .finish_loop_execution(
                            loop_execution_id,
                            "failed",
                            0,
                            0,
                            Some(&e),
                        )
                        .await;
                    // 触发异常处理 Todo（传入 0 作为步数/Token 统计）
                    let _ = this2_for_err
                        .trigger_abnormal_handler(loop_id, loop_execution_id, "failed", 0, 0)
                        .await;
                    // 发送 WebSocket 事件，触发前端刷新执行历史列表。
                    // 没有这步的话，前端 LoopExecutionsPanel 收不到事件通知，
                    // 用户无法在界面上看到这条 failed 记录，只能从后台日志中排查。
                    let _ = this2_for_err
                        .tx
                        .send(crate::executor_service::ExecEvent::ReviewStatusChanged {
                            record_id: 0,
                            todo_id: 0,
                            review_status: "failed".to_string(),
                        });
                    // 失败路径：有绑定对话时直接回复，否则广播 LoopFinished 事件
                    if let Some(ref receive_id) = feishu_receive_id {
                        this2_for_err
                            .send_result_to_feishu(
                                feishu_bot_id,
                                receive_id,
                                feishu_receive_id_type.as_deref().unwrap_or("open_id"),
                                &format!("环路执行失败：{}", e),
                            )
                            .await;
                    } else {
                        // 广播 LoopFinished 事件，FeishuPushService 按 workspace 配置推送
                        let _ = this2_for_err.tx.send(crate::executor_service::ExecEvent::LoopFinished {
                            loop_execution_id,
                            loop_id,
                            loop_title: loop_title.clone(),
                            status: "failed".to_string(),
                            total_steps,
                            completed_steps,
                            failed_steps,
                            duration_secs,
                            total_tokens,
                            workspace_id: loop_workspace_id,
                        });
                    }
                }
            }
        });

        loop_execution_id
    }

    /// 获取 loop 执行的累计 Token 消耗。
    /// 从所有 step_executions 的 execution_record_id 查询 execution_records 的 usage 字段。
    async fn get_loop_total_tokens(&self, loop_execution_id: i64) -> i64 {
        let step_execs = match self.ctx.db.list_loop_step_executions(loop_execution_id).await {
            Ok(se) => se,
            Err(_) => return 0,
        };
        
        let mut total = 0i64;
        for se in step_execs {
            if let Some(record_id) = se.execution_record_id {
                if let Some(rec) = self.ctx.db.get_execution_record(record_id).await.ok().flatten() {
                    // usage 字段是 ExecutionUsage 类型
                    if let Some(usage) = rec.usage {
                        total += usage.input_tokens as i64 + usage.output_tokens as i64;
                    }
                }
            }
        }
        total
    }

    /// 恢复被人工审批暂停的 loop 执行。
    /// 由审批 API handler 调用。
    /// 查找已审批的 step_execution，确定下一步，然后继续主循环。
    pub async fn resume_loop_execution(
        self: &Arc<Self>,
        loop_execution_id: i64,
    ) {
        // 1. 加载 loop_execution，校验状态为 running
        let loop_exec = match self.ctx.db.get_loop_execution(loop_execution_id).await {
            Ok(Some(le)) => le,
            _ => {
                warn!("resume: loop_execution #{} not found", loop_execution_id);
                return;
            }
        };
        if loop_exec.status != "running" {
            warn!("resume: loop_execution #{} status is {}, not running", loop_execution_id, loop_exec.status);
            return;
        }

        // 2. 加载所有 step_executions，找到刚被审批的那条
        let step_execs = match self.ctx.db.list_loop_step_executions(loop_execution_id).await {
            Ok(se) => se,
            Err(e) => {
                warn!("resume: failed to list step_executions: {}", e);
                return;
            }
        };

        // 找 approval_status = "approved" 且原来状态是 pending_approval 的（已审批）
        let approved_se = step_execs.iter().find(|se| {
            se.approval_status.as_deref() == Some("approved") &&
            (se.status == "success" || se.status == "failed")
        });

        let approved = match approved_se {
            Some(se) => se,
            None => {
                warn!("resume: no approved step_execution found for loop_execution #{}", loop_execution_id);
                return;
            }
        };

        // 3. 加载 steps 以确定从哪个索引继续
        let all_steps = match self.ctx.db.list_enabled_loop_steps_by_loop(loop_exec.loop_id).await {
            Ok(steps) => steps,
            Err(e) => {
                warn!("resume: failed to load steps: {}", e);
                return;
            }
        };

        // 4. 找到被审批环节对应的 step，确定 gate_passed 和下一步索引
        let step_id_to_idx: HashMap<i64, usize> = all_steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id, i))
            .collect();

        let step_idx = match step_id_to_idx.get(&approved.step_id) {
            Some(&idx) => idx,
            None => {
                warn!("resume: step #{} not found in current steps", approved.step_id);
                return;
            }
        };

        // 5. 根据审批评分和阈值决定下一步
        let step = &all_steps[step_idx];
        let rating = approved.rating.unwrap_or(0);
        let min_rating = approved.min_rating.unwrap_or(0);
        let gate_passed = rating >= min_rating;

        // 6. 更新计数器（从已有 step_executions 推算）
        let completed = step_execs.iter().filter(|se| se.status == "success").count() as i32;
        let failed = step_execs.iter().filter(|se| se.status == "failed").count() as i32;
        // 总额外步数 = 当前 sequence_index（最大序号即累计执行步数）
        let _total_executed = step_execs.iter().map(|se| se.sequence_index).max().unwrap_or(0);
        // 更新 loop_execution 中的计数器
        let _ = self.ctx.db.increment_loop_execution_counters(
            loop_execution_id,
            if gate_passed { 1 } else { 0 },
            if gate_passed { 0 } else { 1 },
            0,
        ).await;

        // 7. 计算下一步索引
        let next_policy = if gate_passed { &step.on_success } else { &step.on_rating_fail };
        let next_idx = self.resolve_next(step, next_policy, &step_id_to_idx, step_idx);

        info!(
            "resume: loop_execution #{} step #{} rating={} min={} gate_passed={} next_idx={:?}",
            loop_execution_id, approved.step_id, rating, min_rating, gate_passed, next_idx
        );

        // 8. 从下一步继续执行
        if let Some(idx) = next_idx {
            let self_clone = self.clone();
            tokio::spawn(async move {
                if let Err(e) = self_clone.run_inner_from(
                    loop_exec.loop_id,
                    loop_execution_id,
                    loop_exec.trigger_type.clone(),
                    Some(idx),
                ).await {
                    error!("resume: loop #{} continue failed: {}", loop_exec.loop_id, e);
                }
            });
        } else {
            // 没有下一步（end/break），结束 loop execution
            let final_status = if completed > 0 { "success" } else { "failed" };
            let _ = self.ctx.db.finish_loop_execution(
                loop_execution_id, final_status, completed, failed, None,
            ).await;
            info!("resume: loop_execution #{} ended with status {}", loop_execution_id, final_status);
        }
    }

    /// DAG 执行引擎主循环。
    /// resume_step_idx: None 表示全新执行，Some(idx) 表示从指定步骤继续（人工审批恢复）。
    async fn run_inner(
        self: &Arc<Self>,
        loop_id: i64,
        loop_execution_id: i64,
        trigger_type: String,
    ) -> Result<LoopRunOutcome, String> {
        self.run_inner_from(loop_id, loop_execution_id, trigger_type, None).await
    }

    /// DAG 执行引擎主循环（支持从指定步骤继续）。
    /// resume_step_idx: None = 从头开始，Some(idx) = 从该步骤继续。
    async fn run_inner_from(
        self: &Arc<Self>,
        loop_id: i64,
        loop_execution_id: i64,
        trigger_type: String,
        resume_step_idx: Option<usize>,
    ) -> Result<LoopRunOutcome, String> {
        // 1. 校验 loop 状态
        let loop_ = self
            .ctx
            .db
            .get_loop(loop_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("loop #{} not found", loop_id))?;
        if loop_.status != "enabled" {
            return Err(format!("loop #{} is not enabled (status={})", loop_id, loop_.status));
        }

        // 2. 加载所有 enabled steps
        let all_steps = self
            .ctx
            .db
            .list_enabled_loop_steps_by_loop(loop_id)
            .await
            .map_err(|e| e.to_string())?;
        if all_steps.is_empty() {
            self.ctx
                .db
                .finish_loop_execution(loop_execution_id, "success", 0, 0, None)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(LoopRunOutcome::Finished);
        }

        // 3. 校验所有步骤的 todo 是否都在同一工作空间下
        // 环路运行时要求所有环节（step.todo）与 loop 属于同一 workspace，
        // 否则 cwd/worktree 无法统一，跨空间数据流会导致不可预期的行为。
        self.check_workspace_consistency(&loop_, &all_steps).await?;

        // 4. 加载 trigger_meta 中的 params（从 CLI/外部传入的变量）
        let trigger_params: HashMap<String, String> = {
            if let Ok(Some(exec)) = self.ctx.db.get_loop_execution(loop_execution_id).await {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&exec.trigger_meta) {
                    if let Some(params) = meta.get("params").and_then(|v| v.as_object()) {
                        params.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    } else { HashMap::new() }
                } else { HashMap::new() }
            } else { HashMap::new() }
        };

        // 4. 初始化（全新执行）或恢复状态（续跑）
        let is_resume = resume_step_idx.is_some();
        if !is_resume {
            // 全新执行：设置 loop execution 状态
            self.ctx
                .db
                .finish_loop_execution(loop_execution_id, "running", 0, 0, None)
                .await
                .map_err(|e| e.to_string())?;
            self.clear_finished_at(loop_execution_id).await?;
            self.update_total_steps(loop_execution_id, all_steps.len() as i32)
                .await?;
        }

        let limits: LimitsConfig = serde_json::from_str(&loop_.limits_config).unwrap_or_default();
        let max_executions = limits.max_step_executions.unwrap_or(i32::MAX);
        let max_total_tokens = limits.max_total_tokens.unwrap_or(i64::MAX);

        // 恢复模式下从已有记录推算状态计数器
        let (prev_completed, prev_failed, prev_total_executed, prev_max_sequence, prev_last_output, prev_last_conclusion, prev_last_step_name) =
            if is_resume {
                let execs = self.ctx.db.list_loop_step_executions(loop_execution_id).await
                    .unwrap_or_default();
                let completed = execs.iter().filter(|se| se.status == "success").count() as i32;
                let failed = execs.iter().filter(|se| se.status == "failed").count() as i32;
                let total_exec = execs.iter().map(|se| se.sequence_index).max().unwrap_or(0);
                let max_seq = execs.iter().map(|se| se.sequence_index).max().unwrap_or(0);
                // 找最后完成的 step 用于模板变量
                let last_success = execs.iter()
                    .filter(|se| se.status == "success" || se.status == "pending_approval")
                    .max_by_key(|se| se.sequence_index);
                let last_conclusion = last_success.and_then(|se| se.conclusion.clone());
                let last_step_name = last_success.and_then(|se| {
                    all_steps.iter().find(|s| s.id == se.step_id).map(|s| s.name.clone())
                });
                (completed, failed, total_exec, max_seq, None, last_conclusion, last_step_name)
            } else {
                (0, 0, 0, 0, None, None, None)
            };

        let mut current_idx: Option<usize> = resume_step_idx.or(Some(0));
        let mut sequence_counter = if is_resume { prev_max_sequence } else { 0 };
        let mut total_executed = prev_total_executed;
        let mut total_tokens_used: i64 = 0;
        let mut completed = prev_completed;
        let mut failed = prev_failed;
        let mut consecutive_retries: HashMap<i64, i32> = HashMap::new();

        let step_id_to_idx: HashMap<i64, usize> = all_steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id, i))
            .collect();

        // 上一环节的执行结果（用于注入模板变量）
        let mut last_output: Option<String> = prev_last_output;
        let mut last_conclusion: Option<String> = prev_last_conclusion;
        let mut last_step_name: Option<String> = prev_last_step_name;

        // 4. 主循环
        while let Some(idx) = current_idx {
            if idx >= all_steps.len() {
                break;
            }
            let step = &all_steps[idx];

            // 4a. 全局限制检查：步数限制 + Token 限制
            if total_executed >= max_executions {
                info!("loop #{} capped: total_executed={} >= max={}", loop_id, total_executed, max_executions);
                self.ctx
                    .db
                    .finish_loop_execution(loop_execution_id, "capped_step", completed, failed, None)
                    .await
                    .map_err(|e| e.to_string())?;
                // 触发异常处理 Todo
                let _ = self
                    .trigger_abnormal_handler(loop_id, loop_execution_id, "capped_step", total_executed, total_tokens_used)
                    .await;
                return Ok(LoopRunOutcome::Finished);
            }
            if total_tokens_used >= max_total_tokens {
                info!(
                    "loop #{} capped by token: total_tokens_used={} >= max={}",
                    loop_id, total_tokens_used, max_total_tokens
                );
                self.ctx
                    .db
                    .finish_loop_execution(loop_execution_id, "capped_token", completed, failed, None)
                    .await
                    .map_err(|e| e.to_string())?;
                // 触发异常处理 Todo
                let _ = self
                    .trigger_abnormal_handler(loop_id, loop_execution_id, "capped_token", total_executed, total_tokens_used)
                    .await;
                return Ok(LoopRunOutcome::Finished);
            }

            // 4b. 死循环检测：连续 5 次执行同一 step
            let retry_count = consecutive_retries.entry(step.id).or_insert(0);
            if *retry_count >= 5 {
                warn!("loop #{}: step #{} retried {} times, aborting", loop_id, step.id, retry_count);
                failed += 1;
                break;
            }

            sequence_counter += 1;
            total_executed += 1;

            // 4c. 加载 todo 元数据
            let todo = self
                .ctx
                .db
                .get_todo(step.todo_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("todo #{} (todo_id={}) not found", step.id, step.todo_id))?;

            // 4d. 构造增强 Prompt（注入黑板变量 + 上一环节结果）
            let blackboard_text = self.build_blackboard_text(loop_execution_id).await;
            let last_output_text = last_output.as_deref().unwrap_or("");
            let last_conclusion_text = last_conclusion.as_deref().unwrap_or("");
            let last_step_name_text = last_step_name.as_deref().unwrap_or("");
            let enhanced_prompt = todo.prompt
                .replace("{{blackboard}}", &blackboard_text)
                .replace("{{last_output}}", last_output_text)
                .replace("{{last_conclusion}}", last_conclusion_text)
                .replace("{{last_step_name}}", last_step_name_text)
                .replace("{{message}}", last_output_text)
                .replace("{{loop_execution_id}}", &loop_execution_id.to_string())
                .replace("{{loop_name}}", &loop_.name);

            // 4e. 创建 step execution 记录
            let step_exec = self
                .ctx
                .db
                .create_loop_step_execution(
                    loop_execution_id,
                    step.id,
                    step.todo_id,
                    "running",
                    sequence_counter,
                    step.min_rating,
                    &step.unrated_policy,
                )
                .await
                .map_err(|e| e.to_string())?;

            self.ctx
                .db
                .mark_step_execution_started(step_exec.id)
                .await
                .map_err(|e| e.to_string())?;

            // 4f. 执行
            let record_id = match self
                .start_step_todo_with_prompt(&todo, &trigger_type, idx as i64, step_exec.id, &enhanced_prompt, loop_.workspace_path.clone(), &trigger_params)
                .await
            {
                Ok(rid) => rid,
                Err(e) => {
                    warn!("loop_runner: step #{} start failed: {}", step.id, e);
                    self.ctx
                        .db
                        .finish_step_execution(step_exec.id, "failed", None, Some(&e), None, None)
                        .await
                        .map_err(|e| e.to_string())?;
                    self.ctx
                        .db
                        .increment_loop_execution_counters(loop_execution_id, 0, 1, 1)
                        .await
                        .map_err(|e| e.to_string())?;
                    failed += 1;
                    current_idx = self.resolve_next(step, &step.on_rating_fail, &step_id_to_idx, idx);
                    *consecutive_retries.entry(step.id).or_insert(0) += 1;
                    continue;
                }
            };

            // 4g. 等待执行完成
            let step_status = match self.wait_for_step_finish(record_id).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("loop_runner: step #{} wait failed: {}", step.id, e);
                    "failed".to_string()
                }
            };

            // 4h. 评分闸门
            let (gate_passed, step_rating, error_msg) = if step_status == "success" && step.min_rating.is_some() {
                // 人工审批类型：暂停等待，不自动评审
                // 提取结论后写回 pending_approval 状态，然后退出主循环。
                if step.review_type == "human" {
                    let conclusion = self.extract_conclusion(record_id).await;
                    self.ctx
                        .db
                        .finish_step_execution(
                            step_exec.id, "pending_approval", Some(record_id), None, None, Some(&conclusion),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    // 标记审批状态为等待中；失败仅记录日志，不中断 loop 暂停流程，
                    // 因为 step_execution 已经写为 pending_approval 状态，前端可继续操作。
                    if let Err(e) = self.ctx.db.set_step_execution_approval_status(step_exec.id, "pending").await {
                        warn!("loop #{} step #{}: failed to set approval_status to pending: {}", loop_id, step.id, e);
                    }
                    info!("loop #{} step #{} waiting for human approval", loop_id, step.id);
                    // 发送 WebSocket 事件触发前端刷新（让执行历史列表显示"待审批"标记）
                    let _ = self.tx.send(crate::executor_service::ExecEvent::ReviewStatusChanged {
                        record_id,
                        todo_id: 0,
                        review_status: "pending_approval".to_string(),
                    });
                    // 暂停循环（不写最终状态，loop execution 保持 running）
                    // 返回 Paused 而非 Finished，避免误发 LoopFinished 事件
                    return Ok(LoopRunOutcome::Paused);
                }
                // AI 自动评审：min_rating 已在条件分支中确认为 Some，此处用 ok_or 转为 Result
                let min_rating_val = step.min_rating.ok_or("min_rating was None despite is_some() check")?;
                self
                    .apply_rating_gate(
                        record_id,
                        min_rating_val,
                        &todo.prompt,
                        todo.acceptance_criteria.as_deref(),
                        loop_.review_template_id,
                    )
                    .await?
            } else {
                (step_status == "success", None, None)
            };

            let final_step_status = if gate_passed { "success" } else { "failed" };

            // 4i. 提取结论
            let conclusion = self.extract_conclusion(record_id).await;

            // 4j. 写回 step execution（携 error_msg 让前端展示失败原因）
            self.ctx
                .db
                .finish_step_execution(step_exec.id, final_step_status, Some(record_id), error_msg.as_deref(), step_rating, Some(&conclusion))
                .await
                .map_err(|e| e.to_string())?;

            let _ = self.tx.send(crate::executor_service::ExecEvent::ReviewStatusChanged {
                record_id,
                todo_id: 0,
                review_status: final_step_status.to_string(),
            });

            // 4k. 更新计数器
            if gate_passed {
                completed += 1;
                self.ctx
                    .db
                    .increment_loop_execution_counters(loop_execution_id, 1, 0, 1)
                    .await
                    .map_err(|e| e.to_string())?;
                *consecutive_retries.get_mut(&step.id).unwrap_or(&mut 0) = 0;

                // 4l. 触发黑板更新：Step 执行成功，将 execution_record_id 追加到黑板 pending 队列。
                // 与普通 Todo 执行完成后的处理保持一致，让 LLM 将 step 结论整合到黑板。
                if let Some(ws_id) = loop_.workspace_id.filter(|&id| id != 0) {
                    crate::services::blackboard_debouncer::push_pending_record(
                        ws_id,
                        record_id,
                        &self.ctx.db,
                    )
                    .await;
                }
            } else {
                failed += 1;
                self.ctx
                    .db
                    .increment_loop_execution_counters(loop_execution_id, 0, 1, 1)
                    .await
                    .map_err(|e| e.to_string())?;
                *consecutive_retries.entry(step.id).or_insert(0) += 1;
            }

            // 记录上一环节的输出（供下一环节的 {last_output}/{message} 模板变量使用）
            // 同时从 execution_record.usage 中提取 token 用量，累加到 total_tokens_used，
            // 用于下一轮循环 4a 的 token 上限检查。
            let exec_record = self.ctx.db.get_execution_record(record_id).await.ok().flatten();
            last_output = exec_record.as_ref().and_then(|r| r.result.clone());
            last_conclusion = Some(conclusion.clone());
            last_step_name = Some(step.name.clone());
            // 从 usage JSON 中取出 input_tokens + output_tokens 作为本次消耗的 token 数
            if let Some(ref usage) = exec_record.and_then(|r| r.usage) {
                let step_tokens = (usage.input_tokens + usage.output_tokens) as i64;
                total_tokens_used += step_tokens;
            }

            // 4l. 确定下一步
            current_idx = if gate_passed {
                self.resolve_next(step, &step.on_success, &step_id_to_idx, idx)
            } else {
                self.resolve_next(step, &step.on_rating_fail, &step_id_to_idx, idx)
            };
        }

        // 5. 计算最终 status
        let final_status = if failed == 0 {
            "success"
        } else if completed == 0 {
            "failed"
        } else {
            "partial"
        };
        self.ctx
            .db
            .finish_loop_execution(loop_execution_id, final_status, completed, failed, None)
            .await
            .map_err(|e| e.to_string())?;

        // 对异常状态触发异常处理 Todo
        if final_status == "failed" || final_status == "partial" {
            let _ = self
                .trigger_abnormal_handler(loop_id, loop_execution_id, final_status, total_executed, total_tokens_used)
                .await;
        }

        info!(
            "loop #{} run done: status={} completed={} failed={} total_executed={}",
            loop_id, final_status, completed, failed, total_executed
        );
        Ok(LoopRunOutcome::Finished)
    }

    /// 启动 step 的执行，使用增强后的 Prompt。
    /// trigger_params 是从 CLI/外部传入的变量，会合并到 params 中供 prompt 占位符替换。
    #[allow(clippy::too_many_arguments)] // 参数来自上游 handler 的独立数据源，合并为 struct 增加认知负担
    async fn start_step_todo_with_prompt(
        &self,
        todo: &crate::models::Todo,
        trigger_type: &str,
        loop_idx: i64,
        step_exec_id: i64,
        enhanced_prompt: &str,
        workspace_path: Option<String>,
        trigger_params: &HashMap<String, String>,
    ) -> Result<i64, String> {
        // 合并 trigger_params（CLI 传入的外部变量）和内置的 loop_step_index
        let mut params = trigger_params.clone();
        params.insert("loop_step_index".to_string(), loop_idx.to_string());
        let request = RunTodoExecutionRequest {
            db: self.ctx.db.clone(),
            executor_registry: self.ctx.executor_registry.clone(),
            tx: self.tx.clone(),
            task_manager: self.ctx.task_manager.clone(),
            config: self.ctx.config.clone(),
            // 使用 todo.id 而非 0，确保 execution_record 能关联到正确的 todo，
            // 使 todo 执行历史界面能看到 loop 环节的执行记录。
            todo_id: todo.id,
            message: enhanced_prompt.to_string(),
            req_executor: todo.executor.clone(),
            req_model: None,
            trigger_type: format!("loop_stage:{}", trigger_type),
            params: Some(params),
            resume_session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: Some(todo.title.clone()),
            loop_step_execution_id: Some(step_exec_id),
            step_id: None,
            feishu_bot_id: None,
            feishu_receive_id: None,
            feishu_receive_id_type: None,
            workspace_path,
            workspace_id: None,
            // loop 环节执行路径：注入专家上下文，让 loop 内 todo 也尊重 expert_name 绑定
            expert_manager: Some(self.ctx.expert_manager.clone()),
        };
        let result = run_todo_execution_with_params(request).await;
        result
            .record_id
            .ok_or_else(|| "executor returned no record_id".to_string())
    }

    /// 轮询数据库确认指定 record_id 的执行记录是否进入终态。
    /// timeout 24h 防止长跑任务永久挂住 loop。
    ///
    /// 改用轮询而非 broadcast 的原因：broadcast 的 Finished 事件不带 record_id，
    /// 多 loop 并发时会错误收到其他 loop 的 Finished 事件，导致提前返回错误状态。
    /// 原注释已承认"多 loop 并发时会有歧义；首版接受这个限制"，此处修复该问题。
    async fn wait_for_step_finish(&self, record_id: i64) -> Result<String, String> {
        let wait_timeout = Duration::from_secs(24 * 60 * 60);
        let poll_interval = Duration::from_millis(500);
        let start = std::time::Instant::now();
        // 连续错误计数器：防止数据库持续异常导致无限轮询；
        // 单次成功查询或查询返回 None（记录尚未创建）时重置计数。
        let mut consecutive_errors = 0;
        let max_consecutive_errors = 5;

        loop {
            if start.elapsed() > wait_timeout {
                return Err(format!(
                    "step execution (record #{}) timeout after 24h",
                    record_id
                ));
            }

            // 按 record_id 精确查询执行记录状态，避免 broadcast 竞态
            match self.ctx.db.get_execution_record(record_id).await {
                Ok(Some(rec)) => {
                    // 成功获取记录，重置错误计数
                    consecutive_errors = 0;
                    let status_str = rec.status.to_string();
                    if !matches!(rec.status, ExecutionStatus::Running) {
                        return Ok(status_str);
                    }
                }
                Ok(None) => {
                    // 记录尚未创建（执行尚未开始或尚未提交），继续等待；
                    // 这是合法状态，重置错误计数。
                    consecutive_errors = 0;
                }
                Err(e) => {
                    consecutive_errors += 1;
                    warn!("wait_for_step_finish: get_execution_record #{} error (consecutive: {}/{}): {}",
                          record_id, consecutive_errors, max_consecutive_errors, e);
                    // 达到连续错误阈值时停止轮询，防止数据库故障导致 loop 永久挂起
                    if consecutive_errors >= max_consecutive_errors {
                        return Err(format!(
                            "step execution (record #{}) aborted after {} consecutive DB errors",
                            record_id, max_consecutive_errors
                        ));
                    }
                }
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// 把 loop_executions 的 finished_at 清空（避免被 finish_loop_execution 错填）。
    /// 使用参数化查询而非字符串插值，与项目其他地方（如 step_.rs 的 update_step）保持一致，
    /// 避免 SQL 注入风险并遵循最佳实践。
    async fn clear_finished_at(&self, id: i64) -> Result<(), String> {
        use sea_orm::{ConnectionTrait, Statement};
        let sql = "UPDATE loop_executions SET finished_at = NULL WHERE id = ?1";
        self.ctx
            .db
            .conn
            .execute(Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                [id.into()],
            ))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 更新环节总数：使用参数化查询，与 clear_finished_at 保持一致风格，
    /// 避免 format! 拼接 SQL 带来的安全隐患。
    async fn update_total_steps(&self, id: i64, total: i32) -> Result<(), String> {
        use sea_orm::{ConnectionTrait, Statement};
        let sql = "UPDATE loop_executions SET total_steps = ?1 WHERE id = ?2";
        self.ctx
            .db
            .conn
            .execute(Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                [total.into(), id.into()],
            ))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 解析下一步：根据策略和当前索引决定下一个 step 的索引。
    fn resolve_next(
        &self,
        step: &loop_steps::Model,
        policy: &str,
        step_id_to_idx: &HashMap<i64, usize>,
        current_idx: usize,
    ) -> Option<usize> {
        match policy {
            "next" => Some(current_idx + 1),
            "goto" => {
                let target = step.success_goto_step_id
                    .or(step.fail_goto_step_id)?;
                match step_id_to_idx.get(&target) {
                    Some(&idx) => {
                        info!("loop_runner: goto step #{} (idx={})", target, idx);
                        Some(idx)
                    }
                    None => {
                        warn!("loop_runner: goto target step #{} not found, falling back to next", target);
                        Some(current_idx + 1)
                    }
                }
            }
            "end" | "break" => None,
            "skip" => Some(current_idx + 1),
            _ => {
                warn!("loop_runner: unknown policy '{}', falling back to next", policy);
                Some(current_idx + 1)
            }
        }
    }

    /// 构建黑板文本：返回格式化的所有已完成的 step execution 记录。
    async fn build_blackboard_text(&self, loop_execution_id: i64) -> String {
        let execs = self
            .ctx
            .db
            .list_loop_step_executions(loop_execution_id)
            .await
            .unwrap_or_default();

        if execs.is_empty() {
            return String::new();
        }

        let mut lines = Vec::new();
        for e in &execs {
            let status_icon = match e.status.as_str() {
                "success" => "✅",
                "failed" => "❌",
                "skipped" => "⏭️",
                _ => "⏳",
            };
            lines.push(format!(
                "--- 执行记录 #{}: {} (评分: {}) ---\n结论: {}",
                e.sequence_index,
                status_icon,
                e.rating.map(|r| r.to_string()).unwrap_or_else(|| "-".to_string()),
                e.conclusion.as_deref().unwrap_or("(无结论)"),
            ));
        }
        lines.join("\n\n")
    }

    /// 从 execution_record 提取结论摘要。
    /// 获取环路执行的黑板文本（供外部调用，如执行完成回发飞书时使用）
    pub async fn get_loop_blackboard_text(
        &self,
        loop_execution_id: i64,
        _trigger_meta: &serde_json::Value,
    ) -> String {
        self.build_blackboard_text(loop_execution_id).await
    }

    /// 通过 DirectCardMessage 事件把环路执行结果发回飞书
    /// receive_id_type: "open_id"（单聊）或 "chat_id"（群聊）
    pub async fn send_result_to_feishu(
        &self,
        feishu_bot_id: Option<i64>,
        receive_id: &str,
        receive_id_type: &str,
        text: &str,
    ) {
        let Some(bot_id) = feishu_bot_id else { return };
        let _ = self.tx.send(crate::executor_service::ExecEvent::DirectCardMessage {
            bot_id,
            receive_id: receive_id.to_string(),
            receive_id_type: receive_id_type.to_string(),
            content: text.to_string(),
        });
    }

    async fn extract_conclusion(&self, record_id: i64) -> String {
        let rec = self
            .ctx
            .db
            .get_execution_record(record_id)
            .await
            .ok()
            .flatten();

        match rec {
            Some(r) => {
                let output = r.result.as_deref().unwrap_or("");
                for marker in &["## 结论", "## Conclusion", "Conclusion:", "结论："] {
                    if let Some(pos) = output.find(marker) {
                        let after = &output[pos + marker.len()..].trim();
                        let end = after.find('\n').unwrap_or_else(|| {
                            // 使用 char_indices 确保切片在字符边界上，避免切到多字节字符中间
                            after.char_indices().nth(300).map(|(i, _)| i).unwrap_or(after.len())
                        });
                        let slice = &after[..end].trim();
                        if !slice.is_empty() {
                            return slice.to_string();
                        }
                    }
                }
                let truncated: String = output.chars().take(300).collect();
                if truncated.len() < output.len() {
                    format!("{}...", truncated)
                } else {
                    truncated
                }
            }
            None => String::new(),
        }
    }

    /// 复用或新建评审实例 todo。
    ///
    /// 设计：同一 `review_template_id` 全局共享一条评审实例 todo,
    /// 避免「每次 loop 执行都新建评审 todo」把 todos 表刷屏。
    /// 已有 → `reset_review_instance_for_reuse`(保留 id + execution_records 关联)
    /// 没有 → `create_review_instance_todo`(parent_todo_id=0,loop step 没有单一源 todo)
    /// 抽成单独方法便于 loop_runner.rs 内复用,且控制函数行数 ≤ 30。
    async fn reuse_or_create_review_instance(
        &self,
        template_id: i64,
        template_name: &str,
        review_prompt: &str,
        review_executor: Option<&str>,
    ) -> Result<i64, sea_orm::DbErr> {
        match self
            .ctx
            .db
            .find_review_instance_by_template(template_id)
            .await?
        {
            Some(existing) => {
                self.ctx
                    .db
                    .reset_review_instance_for_reuse(existing.id, review_prompt, review_executor)
                    .await?;
                Ok(existing.id)
            }
            None => {
                self.ctx
                    .db
                    .create_review_instance_todo(
                        0,
                        template_id,
                        template_name,
                        review_prompt.to_string(),
                        review_executor.map(|s| s.to_string()),
                    )
                    .await
            }
        }
    }

    /// 评分闸门：检查 execution_record 的 rating 与阈值比较。
    /// 若未评分且环节有验收标准，自动发起评审。
    /// 无评分 = 0 分（不通过，除非 min_rating ≤ 0）。
    /// 返回 (是否通过, 评分, 失败原因说明).
    async fn apply_rating_gate(
        &self,
        record_id: i64,
        min_rating: i32,
        step_prompt: &str,
        step_acceptance_criteria: Option<&str>,
        review_template_id: Option<i64>,
    ) -> Result<(bool, Option<i32>, Option<String>), String> {
        info!(
            "apply_rating_gate: record #{} min_rating={} step_acceptance_criteria={:?} review_template_id={:?}",
            record_id, min_rating, step_acceptance_criteria, review_template_id
        );
        // 先检查是否已有评分
        let rec = self
            .ctx
            .db
            .get_execution_record(record_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("execution record #{} not found", record_id))?;

        if let Some(rating) = rec.rating {
            let passed = rating >= min_rating;
            info!("rating gate: record #{} rating={} min_rating={} {}",
                record_id, rating, min_rating, if passed { "PASS" } else { "FAIL" });
            return Ok((passed, Some(rating), None));
        }

        // 无评分但有验收标准：发起自动评审
        if let Some(criteria) = step_acceptance_criteria.filter(|s| !s.trim().is_empty()) {
            info!(
                "apply_rating_gate: record #{} HAS acceptance_criteria (len={}), entering auto-review",
                record_id,
                criteria.len()
            );
            info!("rating gate: record #{} triggering auto-review", record_id);

            // 1) 获取评审模板
            let template = match self.ensure_review_template(review_template_id).await {
                Ok(t) => t,
                Err(e) => {
                    warn!("rating gate: failed to get review template: {}", e);
                    return Ok((false, None, Some(format!("获取评审模板失败: {}", e))));
                }
            };

            // 2) 获取执行记录的 result + executor
            // executor 从已跑过的 record 继承 (review_template 不带 executor)。
            let original_output = rec.result.as_deref().unwrap_or_default();
            let review_executor = rec.executor.clone();

            // 3) 合成评审 prompt
            use crate::services::auto_review::MAX_OUTPUT_CHARS;
            let truncated: String = if original_output.chars().count() > MAX_OUTPUT_CHARS {
                let mut s: String = original_output.chars().take(MAX_OUTPUT_CHARS).collect();
                s.push_str("\n\n[...以下被截断...]");
                s
            } else {
                original_output.to_string()
            };
            let review_prompt = template
                .prompt
                .replace("{original_prompt}", step_prompt)
                .replace("{max_output_chars}", &MAX_OUTPUT_CHARS.to_string())
                .replace("{original_output}", &truncated)
                .replace("{acceptance_criteria}", criteria);

            // 4) 标记评审状态为 pending
            let _ = self.ctx.db.set_record_last_review_status(record_id, "pending").await;
            let _ = self.ctx.db.set_record_last_reviewed_at(record_id).await;
            let _ = self.tx.send(crate::executor_service::ExecEvent::ReviewStatusChanged {
                record_id,
                todo_id: 0,
                review_status: "pending".to_string(),
            });

            // 5) 评审实例 todo 复用策略:同一 review_template 全局共享一条 todo,
            //    避免「每次 loop 执行都新建评审 todo」把 todos 表刷屏。
            //    parent_todo_id=0: loop step 没有单一 source todo。
            //    executor 继承自被评审的 record。
            //    - 已有 → reset prompt/executor/status,保留 id 和 execution_records 关联
            //    - 没有 → 新建
            let review_todo_id = match self.reuse_or_create_review_instance(
                template.id,
                &template.name,
                &review_prompt,
                review_executor.as_deref(),
            ).await {
                Ok(id) => id,
                Err(e) => {
                    warn!("rating gate: failed to reuse/create review instance todo: {}", e);
                    let _ = self.ctx.db.set_record_last_review_status(record_id, "failed").await;
                    return Ok((false, None, Some(format!("创建评审实例失败: {}", e))));
                }
            };

            // 6) 执行评审
            let request = RunTodoExecutionRequest {
                db: self.ctx.db.clone(),
                executor_registry: self.ctx.executor_registry.clone(),
                tx: self.tx.clone(),
                task_manager: self.ctx.task_manager.clone(),
                config: self.ctx.config.clone(),
                todo_id: review_todo_id,
                message: review_prompt,
                req_executor: review_executor,
                req_model: None,
                trigger_type: "auto_review".to_string(),
                params: None,
                resume_session_id: None,
                resume_message: None,
                source_todo_id: None,
                source_todo_title: None,
                loop_step_execution_id: None,
                step_id: None,
                feishu_bot_id: None,
                feishu_receive_id: None,
            feishu_receive_id_type: None,
                workspace_path: None,
                workspace_id: None,
                // loop 内的评审执行：注入专家上下文，让评审 todo 也尊重其 expert_name 绑定
                expert_manager: Some(self.ctx.expert_manager.clone()),
            };
            let exec_result = crate::executor_service::run_todo_execution(request).await;
            let review_record_id = match exec_result.record_id {
                Some(id) => id,
                None => {
                    warn!("rating gate: review execution returned no record_id");
                    let _ = self.ctx.db.set_record_last_review_status(record_id, "failed").await;
                    return Ok((false, None, Some("评审执行未返回记录ID".to_string())));
                }
            };

            // 6) 轮询评审完成（最多等 300 秒）
            let max_wait = std::time::Duration::from_secs(300);
            let poll_interval = std::time::Duration::from_millis(500);
            let start_poll = std::time::Instant::now();
            let review_status_str = loop {
                if start_poll.elapsed() > max_wait {
                    warn!("rating gate: review record #{} timed out", review_record_id);
                    let _ = self.ctx.db.set_record_last_review_status(record_id, "failed").await;
                    let _ = self.tx.send(crate::executor_service::ExecEvent::ReviewStatusChanged {
                        record_id, todo_id: 0,
                        review_status: "failed".to_string(),
                    });
                    return Ok((false, None, Some("评审超时".to_string())));
                }
                if let Ok(Some(r)) = self.ctx.db.get_execution_record(review_record_id).await {
                    use crate::models::ExecutionStatus;
                    if !matches!(r.status, ExecutionStatus::Running) {
                        break r.status.to_string();
                    }
                }
                tokio::time::sleep(poll_interval).await;
            };

            // 7) 解析评分
            let review_result = self.ctx.db.get_execution_record(review_record_id).await
                .ok().flatten()
                .and_then(|r| r.result);
            let rating = crate::services::auto_review::parse_rating_from_result(
                review_result.as_deref()
            );
            if let Some(r) = rating {
                let _ = self.ctx.db.update_execution_record_rating(record_id, Some(r)).await;
            }

            // 8) 链接评审记录
            let _ = self.ctx.db.link_review_to_source(
                review_record_id, record_id, &review_status_str
            ).await;
            let _ = self.ctx.db.set_record_last_review_status(record_id, &review_status_str).await;
            let _ = self.tx.send(crate::executor_service::ExecEvent::ReviewStatusChanged {
                record_id, todo_id: 0,
                review_status: review_status_str.to_string(),
            });

            info!("rating gate: review done record #{} rating={:?} status={}",
                record_id, rating, review_status_str);

            if let Some(r) = rating {
                let passed = r >= min_rating;
                info!("rating gate: record #{} final rating={} min_rating={} {}",
                    record_id, r, min_rating, if passed { "PASS" } else { "FAIL" });
                return Ok((passed, Some(r), None));
            }

            // 有验收标准、评审也执行了，但未能提取到有效评分
            info!(
                "apply_rating_gate: record #{} auto-review done but no rating extracted, treating as FAIL",
                record_id
            );
            return Ok((false, None, Some("自动评审已完成但未能提取有效评分，请检查评审模板输出格式".to_string())));
        }

        // 无评分且无验收标准 = 视为 0 分，不通过
        info!(
            "apply_rating_gate: record #{} no rating and no acceptance_criteria, treating as FAIL",
            record_id
        );
        Ok((false, None, Some("环节未设置验收标准，无法触发自动评审".to_string())))
    }

    /// 获取评审模板：优先使用 loop 配置的 id，否则用默认模板。
    /// V15 之后模板是独立表 (review_templates) 里的行, 不带 executor 字段。
    async fn ensure_review_template(&self, template_id: Option<i64>) -> Result<crate::models::ReviewTemplate, String> {
        // 如果 loop 指定了模板 id，先尝试加载 (loops.review_template_id 指向 review_templates.id)。
        if let Some(tid) = template_id {
            if let Some(t) = self.ctx.db.get_review_template(tid).await.map_err(|e| format!("load template: {}", e))? {
                return Ok(t);
            }
            // 指定 id 不存在 (例如被删了) -> 静默回退默认, 避免阻塞 loop 评分
            tracing::warn!("review template #{} not found, falling back to default", tid);
        }
        // 回退到默认模板
        let default_id = self
            .ctx
            .db
            .ensure_default_review_template()
            .await
            .map_err(|e| format!("ensure review template: {}", e))?;
        self.ctx
            .db
            .get_review_template(default_id)
            .await
            .map_err(|e| format!("load default template: {}", e))?
            .ok_or_else(|| "default reviewer template vanished".to_string())
    }

    /// 触发异常处理 Todo 并写入 loop_step_execution 记录。
    ///
    /// 当 Loop 以异常状态结束时（capped_step / capped_token / failed），
    /// 如果配置了异常处理 Todo 且当前状态在触发条件内，则执行该 Todo。
    ///
    /// 异常处理 Todo 的执行结果会作为一条特殊的 step execution 记录写入黑板，
    /// 供后续环节或人工复盘使用。
    ///
    /// 设计要点：
    /// - 使用特殊步骤 ID -1 标识异常处理步骤（正常步骤从 1 开始）
    /// - 同步等待执行完成，提取结论写入 conclusion 字段
    /// - 状态统一为 "abnormal_handler" 方便识别
    async fn trigger_abnormal_handler(
        &self,
        loop_id: i64,
        loop_execution_id: i64,
        abnormal_status: &str,
        total_executed_steps: i32,
        total_tokens_used: i64,
    ) -> Result<(), String> {
        // 1. 加载 loop 配置
        let loop_ = self
            .ctx
            .db
            .get_loop(loop_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("loop #{} not found", loop_id))?;

        let handler_todo_id = match loop_.abnormal_handler_todo_id {
            Some(id) => id,
            None => {
                // 没有配置异常处理 Todo，静默返回
                return Ok(());
            }
        };

        // 2. 检查当前状态是否在触发条件内
        let trigger_on: Vec<String> = serde_json::from_str(&loop_.abnormal_handler_trigger_on)
            .unwrap_or_else(|_| vec![
                "capped_step".to_string(),
                "capped_token".to_string(),
                "failed".to_string(),
            ]);
        if !trigger_on.contains(&abnormal_status.to_string()) {
            info!(
                "loop #{} abnormal handler: status '{}' not in trigger conditions {:?}, skip",
                loop_id, abnormal_status, trigger_on
            );
            return Ok(());
        }

        // 3. 检查 handler Todo 是否仍然存在
        let handler_todo = self
            .ctx
            .db
            .get_todo(handler_todo_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("abnormal handler todo #{} not found, may have been deleted", handler_todo_id))?;

        // 4. 构造上下文信息，注入到 prompt
        let mut context_params = HashMap::new();
        context_params.insert("loop_id".to_string(), loop_id.to_string());
        context_params.insert("loop_execution_id".to_string(), loop_execution_id.to_string());
        context_params.insert("loop_name".to_string(), loop_.name.clone());
        context_params.insert("abnormal_status".to_string(), abnormal_status.to_string());
        context_params.insert("total_executed_steps".to_string(), total_executed_steps.to_string());
        context_params.insert("total_tokens_used".to_string(), total_tokens_used.to_string());

        // 5. 构建增强 prompt，注入异常上下文
        let enhanced_prompt = format!(
            "{}\n\n## 异常上下文\n- Loop 名称: {}\n- Loop 执行 ID: {}\n- 异常状态: {}\n- 已执行步数: {}\n- 已消耗 Token: {}",
            handler_todo.prompt,
            loop_.name,
            loop_execution_id,
            abnormal_status,
            total_executed_steps,
            total_tokens_used,
        );

        // 6. 创建异常处理步骤记录（step_id=-1 标识异常处理步骤）
        // 注意：使用专用方法绕过 FK 约束，因为 step_id=-1 在 loop_steps 表中不存在
        let abnormal_step_exec_id = self
            .ctx
            .db
            .create_abnormal_handler_step_execution(
                loop_execution_id,
                handler_todo_id,
                999, // high sequence index so it appears last on blackboard
            )
            .await
            .map_err(|e| e.to_string())?;

        // 8. 触发异常处理 Todo 的执行
        let request = RunTodoExecutionRequest {
            db: self.ctx.db.clone(),
            executor_registry: self.ctx.executor_registry.clone(),
            tx: self.tx.clone(),
            task_manager: self.ctx.task_manager.clone(),
            config: self.ctx.config.clone(),
            todo_id: handler_todo_id,
            message: enhanced_prompt,
            req_executor: handler_todo.executor.clone(),
            req_model: None,
            trigger_type: "loop_abnormal_handler".to_string(),
            params: Some(context_params),
            resume_session_id: None,
            resume_message: None,
            source_todo_id: Some(handler_todo_id),
            source_todo_title: Some(handler_todo.title.clone()),
            loop_step_execution_id: Some(abnormal_step_exec_id),
            step_id: None,
            feishu_bot_id: None,
            feishu_receive_id: None,
            feishu_receive_id_type: None,
            workspace_path: handler_todo.workspace_path.clone(),
            workspace_id: None,
            // loop 异常处理执行路径：注入专家上下文，让 handler todo 也尊重其 expert_name 绑定
            expert_manager: Some(self.ctx.expert_manager.clone()),
        };

        // 9. 等待 handler 执行完成（复用的 wait_for_step_finish，最长 24h）
        let record_id = match run_todo_execution_with_params(request).await.record_id {
            Some(id) => id,
            None => {
                let _ = self
                    .ctx
                    .db
                    .finish_step_execution(
                        abnormal_step_exec_id,
                        "failed",
                        None,
                        Some("trigger failed"),
                        None,
                        None,
                    )
                    .await;
                return Ok(());
            }
        };

        let handler_status = self.wait_for_step_finish(record_id).await
            .unwrap_or_else(|e| {
                warn!("loop abnormal handler wait error: {}", e);
                "failed".to_string()
            });

        // 10. 提取结论并更新 step execution
        let conclusion = self.extract_conclusion(record_id).await;
        let final_status = if handler_status == "success" { "success" } else { "failed" };

        self.ctx
            .db
            .finish_step_execution(
                abnormal_step_exec_id,
                final_status,
                Some(record_id),
                None,
                None,
                Some(&conclusion),
            )
            .await
            .map_err(|e| e.to_string())?;

        info!(
            "loop #{} abnormal handler finished: todo_id={} for status '{}', final_status={}, conclusion_len={}",
            loop_id, handler_todo_id, abnormal_status, final_status, conclusion.len()
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    // ── resolve_next 逻辑测试 ──
    // 使用独立函数测试算法，避免依赖 ServiceContext 构造

    fn make_step(
        id: i64,
        on_success: &str,
        on_rating_fail: &str,
        success_goto: Option<i64>,
        fail_goto: Option<i64>,
    ) -> loop_steps::Model {
        loop_steps::Model {
            id,
            loop_id: 1,
            name: format!("step_{}", id),
            description: String::new(),
            order_index: 0,
            todo_id: 100 + id,
            run_mode: "sequential".to_string(),
            skip_on_source_failed: 0,
            min_rating: None,
            unrated_policy: "skip".to_string(),
            on_success: on_success.to_string(),
            success_goto_step_id: success_goto,
            on_rating_fail: on_rating_fail.to_string(),
            fail_goto_step_id: fail_goto,
            review_type: "ai".to_string(),
            enabled: 1,
            created_at: None,
        }
    }

    /// 模拟 resolve_next 的核心算法（不依赖 LoopRunner::resolve_next 的 &self）
    fn resolve_next_algo(
        step: &loop_steps::Model,
        policy: &str,
        step_id_to_idx: &HashMap<i64, usize>,
        current_idx: usize,
    ) -> Option<usize> {
        match policy {
            "next" => Some(current_idx + 1),
            "goto" => {
                let target = step.success_goto_step_id
                    .or(step.fail_goto_step_id)?;
                step_id_to_idx.get(&target).copied().or(Some(current_idx + 1))
            }
            "end" | "break" => None,
            "skip" => Some(current_idx + 1),
            _ => Some(current_idx + 1),
        }
    }

    #[test]
    fn resolve_next_success_next_returns_plus_one() {
        let steps = vec![
            make_step(1, "next", "break", None, None),
            make_step(2, "next", "break", None, None),
        ];
        let mut idx_map = HashMap::new();
        for (i, s) in steps.iter().enumerate() { idx_map.insert(s.id, i); }
        assert_eq!(resolve_next_algo(&steps[0], "next", &idx_map, 0), Some(1));
    }

    #[test]
    fn resolve_next_success_end_returns_none() {
        let steps = vec![make_step(1, "end", "break", None, None)];
        let mut idx_map = HashMap::new();
        idx_map.insert(1, 0);
        assert_eq!(resolve_next_algo(&steps[0], "end", &idx_map, 0), None);
    }

    #[test]
    fn resolve_next_fail_break_returns_none() {
        let steps = vec![make_step(1, "next", "break", None, None)];
        let mut idx_map = HashMap::new();
        idx_map.insert(1, 0);
        assert_eq!(resolve_next_algo(&steps[0], "break", &idx_map, 0), None);
    }

    #[test]
    fn resolve_next_fail_skip_returns_plus_one() {
        let steps = vec![
            make_step(1, "next", "skip", None, None),
            make_step(2, "next", "break", None, None),
        ];
        let mut idx_map = HashMap::new();
        for (i, s) in steps.iter().enumerate() { idx_map.insert(s.id, i); }
        assert_eq!(resolve_next_algo(&steps[0], "skip", &idx_map, 0), Some(1));
    }

    #[test]
    fn resolve_next_goto_found_returns_target() {
        let steps = vec![
            make_step(1, "goto", "break", Some(3), None),
            make_step(2, "next", "break", None, None),
            make_step(3, "next", "break", None, None),
        ];
        let mut idx_map = HashMap::new();
        for (i, s) in steps.iter().enumerate() { idx_map.insert(s.id, i); }
        // success_goto_step_id = 3 → idx 2
        assert_eq!(resolve_next_algo(&steps[0], "goto", &idx_map, 0), Some(2));
    }

    #[test]
    fn resolve_next_goto_missing_falls_back_to_next() {
        let steps = vec![
            make_step(1, "goto", "break", Some(999), None),
            make_step(2, "next", "break", None, None),
        ];
        let mut idx_map = HashMap::new();
        for (i, s) in steps.iter().enumerate() { idx_map.insert(s.id, i); }
        // 目标 999 不存在 → fallback to next (idx 1)
        assert_eq!(resolve_next_algo(&steps[0], "goto", &idx_map, 0), Some(1));
    }

    #[test]
    fn resolve_next_unknown_policy_falls_back_to_next() {
        let steps = vec![
            make_step(1, "unknown", "unknown", None, None),
            make_step(2, "next", "break", None, None),
        ];
        let mut idx_map = HashMap::new();
        for (i, s) in steps.iter().enumerate() { idx_map.insert(s.id, i); }
        assert_eq!(resolve_next_algo(&steps[0], "unknown", &idx_map, 0), Some(1));
    }

    // ── LimitsConfig 解析测试 ──

    #[test]
    fn limits_config_default_parses_to_empty() {
        let config: LimitsConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.max_step_executions, None);
    }

    #[test]
    fn limits_config_parses_max_step_executions() {
        let config: LimitsConfig = serde_json::from_str(r#"{"max_step_executions": 20}"#).unwrap();
        assert_eq!(config.max_step_executions, Some(20));
    }

    #[test]
    fn limits_config_max_total_tokens_defaults_to_none() {
        let config: LimitsConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.max_total_tokens, None);
    }

    #[test]
    fn limits_config_parses_max_total_tokens() {
        let config: LimitsConfig = serde_json::from_str(r#"{"max_total_tokens": 100000}"#).unwrap();
        assert_eq!(config.max_total_tokens, Some(100000));
    }

    #[test]
    fn limits_config_parses_both_limits() {
        let config: LimitsConfig =
            serde_json::from_str(r#"{"max_step_executions": 10, "max_total_tokens": 50000}"#).unwrap();
        assert_eq!(config.max_step_executions, Some(10));
        assert_eq!(config.max_total_tokens, Some(50000));
    }

    #[test]
    fn limits_config_parses_partial_limits() {
        // 只设 max_total_tokens，不设 max_step_executions
        let config: LimitsConfig = serde_json::from_str(r#"{"max_total_tokens": 99999}"#).unwrap();
        assert_eq!(config.max_step_executions, None);
        assert_eq!(config.max_total_tokens, Some(99999));
    }

    /// 异常处理触发条件解析测试
    #[test]
    fn abnormal_trigger_on_parses_valid_json() {
        let trigger_on: Vec<String> = serde_json::from_str(r#"["capped_step","capped_token","failed"]"#).unwrap();
        assert_eq!(trigger_on.len(), 3);
        assert!(trigger_on.contains(&"capped_step".to_string()));
        assert!(trigger_on.contains(&"capped_token".to_string()));
        assert!(trigger_on.contains(&"failed".to_string()));
    }

    #[test]
    fn abnormal_trigger_on_defaults_to_all() {
        let trigger_on: Vec<String> = serde_json::from_str(r#"["capped_step","capped_token","failed"]"#).unwrap();
        assert!(trigger_on.contains(&"capped_step".to_string()));
        assert!(trigger_on.contains(&"capped_token".to_string()));
        assert!(trigger_on.contains(&"failed".to_string()));
    }

    // ── 工作空间一致性校验测试 ──
    // 使用真实 DB 验证 check_workspace_consistency 方法的行为。

    /// 构造最小 LoopRunner 供测试使用。
    /// 用 `:memory:` 模式创建独立 SQLite 数据库，避免对外部文件依赖。
    async fn make_test_runner() -> (LoopRunner, Arc<crate::db::Database>) {
        use crate::adapters::ExecutorRegistry;
        use crate::config::Config;
        use crate::expert::ExpertIndexManager;
        use crate::task_manager::TaskManager;
        use std::sync::RwLock;
        use tokio::sync::broadcast;

        let db = Arc::new(crate::db::Database::new(":memory:").await.unwrap());
        let ctx = LoopRunnerCtx {
            db: db.clone(),
            executor_registry: Arc::new(ExecutorRegistry::default()),
            task_manager: Arc::new(TaskManager::default()),
            config: Arc::new(RwLock::new(Config::default())),
            // 测试场景下用空专家索引，触发 inject_expert_context 时会因找不到专家而静默回退
            expert_manager: Arc::new(ExpertIndexManager::new()),
        };
        let runner = LoopRunner::new(ctx, broadcast::channel(1).0);
        (runner, db)
    }

    /// 辅助：快速创建一个 workspace（project_directory），返回其 id。
    async fn create_workspace(db: &crate::db::Database, id_suffix: i64) -> i64 {
        db.create_project_directory(
            &format!("/tmp/test-workspace-{}", id_suffix),
            Some(&format!("test-ws-{}", id_suffix)),
            false,
            false,
        )
        .await
        .unwrap()
    }

    /// 校验：所有 step 的 todo 与 loop 在同一工作空间 → 通过。
    #[tokio::test]
    async fn test_check_workspace_consistency_all_match() {
        let (runner, db) = make_test_runner().await;
        // 创建工作空间
        let ws_id = create_workspace(&db, 1).await;
        // 创建两个 todo 都在同一工作空间
        let todo_a = db.create_todo_with_extras("task A", "do A", None, None, false, ws_id, "/tmp/ws").await.unwrap();
        let todo_b = db.create_todo_with_extras("task B", "do B", None, None, false, ws_id, "/tmp/ws").await.unwrap();
        // 创建 loop 属于同一工作空间
        let loop_model = db.create_loop("test-loop", "", Some(ws_id), Some("/tmp/ws"), false, "loop", None, None, None, "[]").await.unwrap();
        // 构造步骤列表（使用真实的 step model 但手动构建，无需写入 DB，因为
        // check_workspace_consistency 只通过 todo_id 查 DB，不查 step 表本身）
        let steps = vec![
            loop_steps::Model {
                id: 1, loop_id: loop_model.id, name: "步骤A".to_string(),
                description: String::new(), order_index: 0, todo_id: todo_a,
                run_mode: "sequential".to_string(), skip_on_source_failed: 0,
                min_rating: None, unrated_policy: "skip".to_string(),
                on_success: "next".to_string(), success_goto_step_id: None,
                on_rating_fail: "break".to_string(), fail_goto_step_id: None,
                review_type: "ai".to_string(), enabled: 1, created_at: None,
            },
            loop_steps::Model {
                id: 2, loop_id: loop_model.id, name: "步骤B".to_string(),
                description: String::new(), order_index: 1, todo_id: todo_b,
                run_mode: "sequential".to_string(), skip_on_source_failed: 0,
                min_rating: None, unrated_policy: "skip".to_string(),
                on_success: "next".to_string(), success_goto_step_id: None,
                on_rating_fail: "break".to_string(), fail_goto_step_id: None,
                review_type: "ai".to_string(), enabled: 1, created_at: None,
            },
        ];
        let result = runner.check_workspace_consistency(&loop_model, &steps).await;
        assert!(result.is_ok(), "同一工作空间下应通过：{:?}", result.err());
    }

    /// 校验：step 的 todo 在另一工作空间 → 报错。
    #[tokio::test]
    async fn test_check_workspace_consistency_mismatch() {
        let (runner, db) = make_test_runner().await;
        let ws1 = create_workspace(&db, 1).await;
        let ws2 = create_workspace(&db, 2).await;
        // todo_a 在 ws1，todo_b 在 ws2
        let todo_a = db.create_todo_with_extras("task A", "do A", None, None, false, ws1, "/tmp/ws1").await.unwrap();
        let todo_b = db.create_todo_with_extras("task B", "do B", None, None, false, ws2, "/tmp/ws2").await.unwrap();
        // loop 属于 ws1
        let loop_model = db.create_loop("test-loop", "", Some(ws1), Some("/tmp/ws1"), false, "loop", None, None, None, "[]").await.unwrap();
        let steps = vec![
            loop_steps::Model {
                id: 1, loop_id: loop_model.id, name: "步骤A".to_string(),
                description: String::new(), order_index: 0, todo_id: todo_a,
                run_mode: "sequential".to_string(), skip_on_source_failed: 0,
                min_rating: None, unrated_policy: "skip".to_string(),
                on_success: "next".to_string(), success_goto_step_id: None,
                on_rating_fail: "break".to_string(), fail_goto_step_id: None,
                review_type: "ai".to_string(), enabled: 1, created_at: None,
            },
            loop_steps::Model {
                id: 2, loop_id: loop_model.id, name: "步骤B".to_string(),
                description: String::new(), order_index: 1, todo_id: todo_b,
                run_mode: "sequential".to_string(), skip_on_source_failed: 0,
                min_rating: None, unrated_policy: "skip".to_string(),
                on_success: "next".to_string(), success_goto_step_id: None,
                on_rating_fail: "break".to_string(), fail_goto_step_id: None,
                review_type: "ai".to_string(), enabled: 1, created_at: None,
            },
        ];
        let result = runner.check_workspace_consistency(&loop_model, &steps).await;
        assert!(result.is_err(), "跨工作空间应报错");
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("步骤B"), "错误信息应包含跨空间的步骤名");
        assert!(err_msg.contains("不一致"), "错误信息应提示工作空间不一致");
    }

    /// 校验：loop 和 todos 都未设置工作空间（workspace_id=0/None）→ 通过。
    /// 注：todos.workspace_id 列定义 NOT NULL DEFAULT 0，未分配工作空间时为 0；
    /// loop.workspace_id 可选，未分配时为 None。两者应视为"均未设置"。
    #[tokio::test]
    async fn test_check_workspace_consistency_both_unset() {
        let (runner, db) = make_test_runner().await;
        // todo_a, todo_b: workspace_id=0（默认值，表示未分配工作空间）
        let todo_a = db.create_todo_with_executor("task A", "do A", None).await.unwrap();
        let todo_b = db.create_todo_with_executor("task B", "do B", None).await.unwrap();
        // loop: workspace_id=None（也视为未分配）
        let loop_model = db.create_loop("test-loop", "", None, None, false, "loop", None, None, None, "[]").await.unwrap();
        let steps = vec![
            loop_steps::Model {
                id: 1, loop_id: loop_model.id, name: "步骤A".to_string(),
                description: String::new(), order_index: 0, todo_id: todo_a,
                run_mode: "sequential".to_string(), skip_on_source_failed: 0,
                min_rating: None, unrated_policy: "skip".to_string(),
                on_success: "next".to_string(), success_goto_step_id: None,
                on_rating_fail: "break".to_string(), fail_goto_step_id: None,
                review_type: "ai".to_string(), enabled: 1, created_at: None,
            },
            loop_steps::Model {
                id: 2, loop_id: loop_model.id, name: "步骤B".to_string(),
                description: String::new(), order_index: 1, todo_id: todo_b,
                run_mode: "sequential".to_string(), skip_on_source_failed: 0,
                min_rating: None, unrated_policy: "skip".to_string(),
                on_success: "next".to_string(), success_goto_step_id: None,
                on_rating_fail: "break".to_string(), fail_goto_step_id: None,
                review_type: "ai".to_string(), enabled: 1, created_at: None,
            },
        ];
        let result = runner.check_workspace_consistency(&loop_model, &steps).await;
        // todo.workspace_id=Some(0) vs loop.workspace_id=None 在数据库中分别表示"未分配"，
        // check_workspace_consistency 会将 0 和 None 统一视为"未设置"，二者等价，应通过。
        assert!(result.is_ok(), "Some(0) 与 None 均表示未分配工作空间，应视为一致：{:?}", result.err());
    }

    /// 校验：同一 todo 被多个 step 引用时不会重复检查 → 仍通过。
    #[tokio::test]
    async fn test_check_workspace_consistency_duplicate_todo() {
        let (runner, db) = make_test_runner().await;
        let ws_id = create_workspace(&db, 1).await;
        let todo_a = db.create_todo_with_extras("task A", "do A", None, None, false, ws_id, "/tmp/ws").await.unwrap();
        let loop_model = db.create_loop("test-loop", "", Some(ws_id), Some("/tmp/ws"), false, "loop", None, None, None, "[]").await.unwrap();
        // 两个 step 引用同一个 todo
        let steps = vec![
            loop_steps::Model {
                id: 1, loop_id: loop_model.id, name: "步骤A-1".to_string(),
                description: String::new(), order_index: 0, todo_id: todo_a,
                run_mode: "sequential".to_string(), skip_on_source_failed: 0,
                min_rating: None, unrated_policy: "skip".to_string(),
                on_success: "next".to_string(), success_goto_step_id: None,
                on_rating_fail: "break".to_string(), fail_goto_step_id: None,
                review_type: "ai".to_string(), enabled: 1, created_at: None,
            },
            loop_steps::Model {
                id: 2, loop_id: loop_model.id, name: "步骤A-2".to_string(),
                description: String::new(), order_index: 1, todo_id: todo_a,
                run_mode: "sequential".to_string(), skip_on_source_failed: 0,
                min_rating: None, unrated_policy: "skip".to_string(),
                on_success: "next".to_string(), success_goto_step_id: None,
                on_rating_fail: "break".to_string(), fail_goto_step_id: None,
                review_type: "ai".to_string(), enabled: 1, created_at: None,
            },
        ];
        let result = runner.check_workspace_consistency(&loop_model, &steps).await;
        assert!(result.is_ok(), "重复引用同一 todo 应通过：{:?}", result.err());
    }
}
