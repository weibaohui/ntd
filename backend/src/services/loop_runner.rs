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

use crate::executor_service::{run_todo_execution_with_params, RunTodoExecutionRequest};
use crate::hooks::HookService;
use crate::models::ExecutionStatus;
use crate::service_context::ServiceContext;
use crate::db::entity::{loop_steps, steps};

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

/// LoopRunner 依赖：与现有 HookService 共享一个 spawn-friendly 结构。
pub struct LoopRunner {
    ctx: ServiceContext,
    hook_service: Arc<HookService>,
    tx: broadcast::Sender<crate::handlers::ExecEvent>,
}

impl LoopRunner {
    pub fn new(
        ctx: ServiceContext,
        hook_service: Arc<HookService>,
        tx: broadcast::Sender<crate::handlers::ExecEvent>,
    ) -> Self {
        Self { ctx, hook_service, tx }
    }

    /// 暴露 ServiceContext 供 LoopScheduler / 测试 / 上层使用。
    /// 只读引用,不会让调用方修改 ctx 内部状态。
    pub fn ctx_ref(&self) -> &ServiceContext {
        &self.ctx
    }

    /// Spawn 一条 loop 执行（fire-and-forget）。返回 loop_execution_id 给调用方。
    pub fn spawn_run(
        self: Arc<Self>,
        loop_id: i64,
        trigger_id: Option<i64>,
        trigger_type: &str,
        trigger_meta: serde_json::Value,
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
        tokio::spawn(async move {
            if let Err(e) = this2
                .run_inner(loop_id, loop_execution_id, trigger_type)
                .await
            {
                error!("loop_runner: run failed: {}", e);
                // 终态化 loop execution
                let _ = this2_for_err
                    .ctx
                    .db
                    .finish_loop_execution(
                        loop_execution_id,
                        "failed",
                        0,
                        0,
                    )
                    .await;
            }
        });

        loop_execution_id
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
                loop_execution_id, final_status, completed, failed,
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
    ) -> Result<(), String> {
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
    ) -> Result<(), String> {
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
                .finish_loop_execution(loop_execution_id, "success", 0, 0)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }

        // 3. 初始化（全新执行）或恢复状态（续跑）
        let is_resume = resume_step_idx.is_some();
        if !is_resume {
            // 全新执行：设置 loop execution 状态
            self.ctx
                .db
                .finish_loop_execution(loop_execution_id, "running", 0, 0)
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
                    .finish_loop_execution(loop_execution_id, "capped_step", completed, failed)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }
            if total_tokens_used >= max_total_tokens {
                info!(
                    "loop #{} capped by token: total_tokens_used={} >= max={}",
                    loop_id, total_tokens_used, max_total_tokens
                );
                self.ctx
                    .db
                    .finish_loop_execution(loop_execution_id, "capped_token", completed, failed)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(());
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

            // 4c. 加载 step 元数据
            let step_meta = self
                .ctx
                .db
                .get_step(step.step_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("step #{} (step_id={}) not found", step.id, step.step_id))?;

            // 4d. 构造增强 Prompt（注入黑板变量 + 上一环节结果）
            let blackboard_text = self.build_blackboard_text(loop_execution_id).await;
            let last_output_text = last_output.as_deref().unwrap_or("");
            let last_conclusion_text = last_conclusion.as_deref().unwrap_or("");
            let last_step_name_text = last_step_name.as_deref().unwrap_or("");
            let enhanced_prompt = step_meta.prompt
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
                    step.step_id,
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
                .start_step_todo_with_prompt(&step_meta, &trigger_type, idx as i64, step_exec.id, &enhanced_prompt, loop_.workspace.clone())
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
            let (gate_passed, step_rating) = if step_status == "success" && step.min_rating.is_some() {
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
                    let _ = self.tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
                        record_id,
                        todo_id: 0,
                        review_status: "pending_approval".to_string(),
                    });
                    // 暂停循环（不写最终状态，loop execution 保持 running）
                    return Ok(());
                }
                // AI 自动评审：原有逻辑
                self
                    .apply_rating_gate(
                        record_id,
                        step.min_rating.unwrap(),
                        &step_meta.prompt,
                        step_meta.acceptance_criteria.as_deref(),
                        loop_.review_template_id,
                    )
                    .await
                    .map_err(|e| e.to_string())?
            } else {
                (step_status == "success", None)
            };

            let final_step_status = if gate_passed { "success" } else { "failed" };

            // 4i. 提取结论
            let conclusion = self.extract_conclusion(record_id).await;

            // 4j. 写回 step execution
            self.ctx
                .db
                .finish_step_execution(step_exec.id, final_step_status, Some(record_id), None, step_rating, Some(&conclusion))
                .await
                .map_err(|e| e.to_string())?;

            let _ = self.tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
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
            .finish_loop_execution(loop_execution_id, final_status, completed, failed)
            .await
            .map_err(|e| e.to_string())?;

        info!(
            "loop #{} run done: status={} completed={} failed={} total_executed={}",
            loop_id, final_status, completed, failed, total_executed
        );
        Ok(())
    }

    /// 启动 step 的执行，使用增强后的 Prompt。
    async fn start_step_todo_with_prompt(
        &self,
        step_meta: &steps::Model,
        trigger_type: &str,
        loop_idx: i64,
        step_exec_id: i64,
        enhanced_prompt: &str,
        workspace: Option<String>,
    ) -> Result<i64, String> {
        let request = RunTodoExecutionRequest {
            db: self.ctx.db.clone(),
            executor_registry: self.ctx.executor_registry.clone(),
            tx: self.ctx.tx.clone(),
            task_manager: self.ctx.task_manager.clone(),
            config: self.ctx.config.clone(),
            hook_service: self.hook_service.clone(),
            todo_id: 0,
            message: enhanced_prompt.to_string(),
            req_executor: step_meta.executor.clone(),
            trigger_type: format!("loop_stage:{}", trigger_type),
            params: Some({
                let mut p = HashMap::new();
                p.insert("loop_step_index".to_string(), loop_idx.to_string());
                p
            }),
            resume_session_id: None,
            resume_message: None,
            chain: vec![],
            source_todo_id: None,
            source_todo_title: Some(step_meta.title.clone()),
            source_hook_id: None,
            loop_step_execution_id: Some(step_exec_id),
            step_id: Some(step_meta.id),
            feishu_bot_id: None,
            feishu_receive_id: None,
            workspace,
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
    /// 返回 (是否通过, 评分)。
    async fn apply_rating_gate(
        &self,
        record_id: i64,
        min_rating: i32,
        step_prompt: &str,
        step_acceptance_criteria: Option<&str>,
        review_template_id: Option<i64>,
    ) -> Result<(bool, Option<i32>), String> {
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
            return Ok((passed, Some(rating)));
        }

        // 无评分但有验收标准：发起自动评审
        if let Some(criteria) = step_acceptance_criteria.filter(|s| !s.trim().is_empty()) {
            info!("rating gate: record #{} triggering auto-review", record_id);

            // 1) 获取评审模板
            let template = match self.ensure_review_template(review_template_id).await {
                Ok(t) => t,
                Err(e) => {
                    warn!("rating gate: failed to get review template: {}", e);
                    return Ok((false, None));
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
            let _ = self.tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
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
                    return Ok((false, None));
                }
            };

            // 6) 执行评审
            let request = RunTodoExecutionRequest {
                db: self.ctx.db.clone(),
                executor_registry: self.ctx.executor_registry.clone(),
                tx: self.ctx.tx.clone(),
                task_manager: self.ctx.task_manager.clone(),
                config: self.ctx.config.clone(),
                hook_service: self.hook_service.clone(),
                todo_id: review_todo_id,
                message: review_prompt,
                req_executor: review_executor,
                trigger_type: "auto_review".to_string(),
                params: None,
                resume_session_id: None,
                resume_message: None,
                chain: vec![],
                source_todo_id: None,
                source_todo_title: None,
                source_hook_id: None,
                loop_step_execution_id: None,
                step_id: None,
                feishu_bot_id: None,
                feishu_receive_id: None,
                workspace: None,
            };
            let exec_result = crate::executor_service::run_todo_execution(request).await;
            let review_record_id = match exec_result.record_id {
                Some(id) => id,
                None => {
                    warn!("rating gate: review execution returned no record_id");
                    let _ = self.ctx.db.set_record_last_review_status(record_id, "failed").await;
                    return Ok((false, None));
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
                    let _ = self.tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
                        record_id, todo_id: 0,
                        review_status: "failed".to_string(),
                    });
                    return Ok((false, None));
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
            let _ = self.tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
                record_id, todo_id: 0,
                review_status: review_status_str.to_string(),
            });

            info!("rating gate: review done record #{} rating={:?} status={}",
                record_id, rating, review_status_str);

            if let Some(r) = rating {
                let passed = r >= min_rating;
                info!("rating gate: record #{} final rating={} min_rating={} {}",
                    record_id, r, min_rating, if passed { "PASS" } else { "FAIL" });
                return Ok((passed, Some(r)));
            }
        }

        // 无评分且无验收标准 = 视为 0 分，不通过
        info!("rating gate: record #{} no rating and no criteria, treating as FAIL", record_id);
        Ok((false, None))
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
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------
#[cfg(test)]
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
            step_id: 100 + id,
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
}
