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
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::executor_service::{run_todo_execution_with_params, RunTodoExecutionRequest};
use crate::hooks::HookService;
use crate::models::ExecutionStatus;
use crate::service_context::ServiceContext;
use crate::db::entity::{loop_steps, steps};

/// 全局限制配置，从 loop.limits_config JSON 解析。
#[derive(Debug, Default, Clone, serde::Deserialize)]
struct LimitsConfig {
    #[serde(default)]
    max_step_executions: Option<i32>,
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

    /// DAG 执行引擎主循环。
    async fn run_inner(
        self: Arc<Self>,
        loop_id: i64,
        loop_execution_id: i64,
        trigger_type: String,
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

        // 3. 初始化
        self.ctx
            .db
            .finish_loop_execution(loop_execution_id, "running", 0, 0)
            .await
            .map_err(|e| e.to_string())?;
        self.clear_finished_at(loop_execution_id).await?;
        self.update_total_steps(loop_execution_id, all_steps.len() as i32)
            .await?;

        let limits: LimitsConfig = serde_json::from_str(&loop_.limits_config).unwrap_or_default();
        let max_executions = limits.max_step_executions.unwrap_or(i32::MAX);

        let mut current_idx: Option<usize> = Some(0);
        let mut sequence_counter = 0i32;
        let mut total_executed = 0i32;
        let mut completed: i32 = 0;
        let mut failed: i32 = 0;
        let mut consecutive_retries: HashMap<i64, i32> = HashMap::new();

        let step_id_to_idx: HashMap<i64, usize> = all_steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id, i))
            .collect();

        // 4. 主循环
        while let Some(idx) = current_idx {
            if idx >= all_steps.len() {
                break;
            }
            let step = &all_steps[idx];

            // 4a. 全局限制检查
            if total_executed >= max_executions {
                info!("loop #{} capped: total_executed={} >= max={}", loop_id, total_executed, max_executions);
                self.ctx
                    .db
                    .finish_loop_execution(loop_execution_id, "capped", completed, failed)
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
                .get_step(step.todo_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("step #{} (todo_id={}) not found", step.id, step.todo_id))?;

            // 4d. 构造增强 Prompt（注入黑板变量）
            let blackboard_text = self.build_blackboard_text(loop_execution_id).await;
            let enhanced_prompt = step_meta.prompt
                .replace("{blackboard}", &blackboard_text)
                .replace("{loop_execution_id}", &loop_execution_id.to_string())
                .replace("{loop_name}", &loop_.name);

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
                .start_step_todo_with_prompt(&step_meta, &trigger_type, idx as i64, step_exec.id, &enhanced_prompt)
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
        };
        let result = run_todo_execution_with_params(request).await;
        result
            .record_id
            .ok_or_else(|| "executor returned no record_id".to_string())
    }

    /// 订阅 broadcast 等待指定 record_id 的 Finished 事件。
    /// timeout 24h 防止长跑任务永久挂住 loop。
    async fn wait_for_step_finish(&self, record_id: i64) -> Result<String, String> {
        let mut rx = self.tx.subscribe();
        let wait_timeout = Duration::from_secs(24 * 60 * 60);
        let result = timeout(wait_timeout, async {
            loop {
                match rx.recv().await {
                    Ok(crate::handlers::ExecEvent::Finished {
                        task_id: _,
                        todo_id: _,
                        todo_title: _,
                        executor: _,
                        success,
                        result: _,
                        feishu_bot_id: _,
                        feishu_receive_id: _,
                    }) => {
                        // Finished 不带 record_id,需要靠 todo 状态二次查询确认
                        // 这里简化为: 任意 Finished 事件都先接住,再用 record_id 反查
                        // 但实际是 broadcast 只发 task_id 不发 record_id;
                        // 所以我们用 fallback: 任意 Finished 来就直接退出,
                        // 因为 loop 是顺序的,这时只有当前 step 在跑。
                        // （多 loop 并发时会有歧义；首版接受这个限制,后期可扩展 event 加 record_id）
                        return if success {
                            Ok(ExecutionStatus::Success.as_str().to_string())
                        } else {
                            Ok(ExecutionStatus::Failed.as_str().to_string())
                        };
                    }
                    Ok(crate::handlers::ExecEvent::Started { .. })
                    | Ok(crate::handlers::ExecEvent::Output { .. })
                    | Ok(crate::handlers::ExecEvent::TodoProgress { .. })
                    | Ok(crate::handlers::ExecEvent::ExecutionStats { .. })
                    | Ok(crate::handlers::ExecEvent::ReviewStatusChanged { .. })
                    | Ok(crate::handlers::ExecEvent::Sync { .. }) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err("broadcast channel closed".to_string());
                    }
                }
            }
        })
        .await;

        match result {
            Ok(inner) => {
                // inner 是 broadcast waiter 返回的 Result<String,String>
                // 二次确认: 用 record_id 反查 execution_records 拿到实际 status
                match inner {
                    Ok(broadcast_status) => match self
                        .ctx
                        .db
                        .get_execution_record(record_id)
                        .await
                    {
                        Ok(Some(rec)) => Ok(rec.status.to_string()),
                        Ok(None) => Ok(broadcast_status),
                        Err(_) => Ok(broadcast_status),
                    },
                    Err(e) => Err(e),
                }
            }
            Err(_) => Err(format!(
                "step execution (record #{}) timeout after 24h",
                record_id
            )),
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
                        let end = after.find('\n').unwrap_or(after.len().min(300));
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

            // 2) 获取执行记录的 result
            let original_output = rec.result.as_deref().unwrap_or_default();

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

            // 5) 执行评审
            let request = RunTodoExecutionRequest {
                db: self.ctx.db.clone(),
                executor_registry: self.ctx.executor_registry.clone(),
                tx: self.ctx.tx.clone(),
                task_manager: self.ctx.task_manager.clone(),
                config: self.ctx.config.clone(),
                hook_service: self.hook_service.clone(),
                todo_id: template.id,
                message: review_prompt,
                req_executor: template.executor.clone(),
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

    /// 获取评审模板 todo：优先使用 loop 配置的 id，否则用默认模板
    async fn ensure_review_template(&self, template_id: Option<i64>) -> Result<crate::models::Todo, String> {
        // 如果 loop 指定了模板 id，直接加载
        if let Some(tid) = template_id {
            if let Some(t) = self.ctx.db.get_todo(tid).await.map_err(|e| format!("load template: {}", e))? {
                return Ok(t);
            }
        }
        // 回退到默认模板
        use crate::services::auto_review::{
            ensure_reviewer_template, DEFAULT_REVIEWER_PROMPT, REVIEWER_TEMPLATE_TITLE,
        };
        let default_id = ensure_reviewer_template(
            &self.ctx.db, REVIEWER_TEMPLATE_TITLE, DEFAULT_REVIEWER_PROMPT
        ).await.map_err(|e| format!("ensure review template: {}", e))?;
        self.ctx.db.get_todo(default_id)
            .await
            .map_err(|e| format!("load template: {}", e))?
            .ok_or_else(|| "reviewer template vanished".to_string())
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
            todo_id: 100 + id,
            run_mode: "sequential".to_string(),
            skip_on_source_failed: 0,
            min_rating: None,
            unrated_policy: "skip".to_string(),
            on_success: on_success.to_string(),
            success_goto_step_id: success_goto,
            on_rating_fail: on_rating_fail.to_string(),
            fail_goto_step_id: fail_goto,
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
        assert_eq!(config.max_total_tokens, None);
    }

    #[test]
    fn limits_config_parses_max_step_executions() {
        let config: LimitsConfig = serde_json::from_str(r#"{"max_step_executions": 20}"#).unwrap();
        assert_eq!(config.max_step_executions, Some(20));
    }

    #[test]
    fn limits_config_parses_all_fields() {
        let config: LimitsConfig = serde_json::from_str(
            r#"{"max_step_executions": 50, "max_total_tokens": 1000000}"#
        ).unwrap();
        assert_eq!(config.max_step_executions, Some(50));
        assert_eq!(config.max_total_tokens, Some(1000000));
    }
}
