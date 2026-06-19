//! Loop Runner — 顺序执行 loop 的所有 stage。
//!
//! 执行模型：
//! 1. 创建 loop_executions 行（status=running）
//! 2. fire pre_loop hooks
//! 3. 按 order_index 顺序遍历 stages：
//!    a. fire pre_stage hooks
//!    b. 启动 stage.todo 的执行（复用 executor_service::start_todo_execution）
//!    c. 写 loop_stage_execution 行
//!    d. 订阅 broadcast::tx 等待该 stage 的 ExecEvent::Finished
//!    e. 应用 rating gate（决定是否继续 / 中止 loop）
//!    f. fire post_stage hooks
//! 4. fire post_loop hooks
//! 5. 计算最终 status（success / partial / failed / cancelled）
//! 6. 写回 loop_executions
//!
//! 与 HookService 的关键差异：hook 是 fire-and-forget（不等待 target 执行完），
//! loop 必须同步等每个 stage 完成才能做 rating gate 评估和「失败是否继续」决策。
//! 因此这里用 broadcast::tx.subscribe() 监听 Finished 事件，按 record_id 过滤。
//!
//! 整个 run_loop 是 `tokio::spawn` 的，不阻塞调用方（manual trigger / cron /
//! dispatcher 都把 run_loop 扔到后台）。

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::executor_service::{run_todo_execution_with_params, RunTodoExecutionRequest};
use crate::hooks::HookService;
use crate::service_context::ServiceContext;

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
    ///
    /// 之前用 `block_in_place + block_on` 在 current_thread runtime 下会 panic;
    /// 现在改为 async,所有调用方本来就在 async 上下文,直接 await 即可。
    pub async fn spawn_run(
        self: Arc<Self>,
        loop_id: i64,
        trigger_id: Option<i64>,
        trigger_type: &str,
        trigger_meta: serde_json::Value,
    ) -> i64 {
        let this = self.clone();
        let trigger_type = trigger_type.to_string();
        // 先建 loop_execution 行拿到 id,然后后台异步跑整个流程
        // total_stages 暂记 0,run_inner 在确认 stage 数后通过 mark_loop_execution_running 一次写入
        let initial_total_stages = 0i32;
        let loop_execution_id = match this
            .ctx
            .db
            .create_loop_execution(
                loop_id,
                trigger_id,
                &trigger_type,
                &trigger_meta.to_string(),
                initial_total_stages,
            )
            .await
        {
            Ok(m) => m.id,
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

    /// 实际的执行逻辑。
    async fn run_inner(
        self: Arc<Self>,
        loop_id: i64,
        loop_execution_id: i64,
        trigger_type: String,
    ) -> Result<(), String> {
        // 1. 校验 loop 状态（如果已被禁用,直接拒绝）
        let loop_ = self
            .ctx
            .db
            .get_loop(loop_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("loop #{} not found", loop_id))?;
        if loop_.status != "enabled" {
            return Err(format!(
                "loop #{} is not enabled (status={})",
                loop_id, loop_.status
            ));
        }

        // 2. 加载所有 enabled stages
        let stages = self
            .ctx
            .db
            .list_enabled_stages_by_loop(loop_id)
            .await
            .map_err(|e| e.to_string())?;
        if stages.is_empty() {
            // 没有 stage,直接 mark 为 success
            self.ctx
                .db
                .finish_loop_execution(loop_execution_id, "success", 0, 0)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        // 一次性把 status=running / total_stages=N / finished_at=NULL 写回,
        // 避免「先写 running 带 finished_at,再清 finished_at」的可见窗口
        self.ctx
            .db
            .mark_loop_execution_running(loop_execution_id, stages.len() as i32)
            .await
            .map_err(|e| e.to_string())?;

        // 3. fire pre_loop hooks
        let pre_loop_hooks = self
            .ctx
            .db
            .list_hooks_by_loop_and_position(loop_id, "pre_loop")
            .await
            .map_err(|e| e.to_string())?;
        for h in pre_loop_hooks {
            let _ = self.fire_single_loop_hook(&h, &loop_).await;
        }

        // 4. 顺序遍历 stages
        let mut completed: i32 = 0;
        let mut failed: i32 = 0;
        let mut last_failed_record: Option<i64> = None;
        for (idx, stage) in stages.iter().enumerate() {
            // 若上一阶段失败且当前 stage 设置了 skip_on_source_failed,则跳过
            if last_failed_record.is_some() && stage.skip_on_source_failed != 0 {
                info!(
                    "loop #{} stage #{} skipped: upstream stage failed and skip_on_source_failed=true",
                    loop_id, stage.id
                );
                self.ctx
                    .db
                    .create_loop_stage_execution(
                        loop_execution_id,
                        stage.id,
                        stage.todo_id,
                        "skipped",
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                continue;
            }

            // 4a. pre_stage hooks
            let pre_stage_hooks = self
                .ctx
                .db
                .list_hooks_by_loop_and_position(loop_id, "pre_stage")
                .await
                .map_err(|e| e.to_string())?;
            for h in pre_stage_hooks.iter().filter(|h| h.source_stage_id == Some(stage.id)) {
                let _ = self.fire_single_loop_hook(h, &loop_).await;
            }

            // 4b. 启动 stage execution
            let stage_exec = self
                .ctx
                .db
                .create_loop_stage_execution(
                    loop_execution_id,
                    stage.id,
                    stage.todo_id,
                    "running",
                )
                .await
                .map_err(|e| e.to_string())?;
            self.ctx
                .db
                .mark_stage_execution_started(stage_exec.id)
                .await
                .map_err(|e| e.to_string())?;

            // 4c. 实际执行 todo
            let todo = match self
                .ctx
                .db
                .get_todo(stage.todo_id)
                .await
                .map_err(|e| e.to_string())?
            {
                Some(t) => t,
                None => {
                    let msg = format!("stage #{} todo #{} not found", stage.id, stage.todo_id);
                    warn!("loop_runner: {}", msg);
                    self.ctx
                        .db
                        .finish_stage_execution(
                            stage_exec.id,
                            "failed",
                            None,
                            Some(&msg),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    failed += 1;
                    last_failed_record = None;
                    let _ = self
                        .ctx
                        .db
                        .increment_loop_execution_counters(loop_execution_id, 0, 1)
                        .await;
                    continue;
                }
            };

            let record_id = match self
                .start_stage_todo(&todo, &trigger_type, idx as i64)
                .await
            {
                Ok(rid) => rid,
                Err(e) => {
                    let msg = format!("stage #{} start failed: {}", stage.id, e);
                    warn!("loop_runner: {}", msg);
                    self.ctx
                        .db
                        .finish_stage_execution(
                            stage_exec.id,
                            "failed",
                            None,
                            Some(&msg),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    failed += 1;
                    last_failed_record = None;
                    let _ = self
                        .ctx
                        .db
                        .increment_loop_execution_counters(loop_execution_id, 0, 1)
                        .await;
                    continue;
                }
            };

            // 4d. 等待 stage 执行结束
            let stage_status = match self.wait_for_stage_finish(record_id).await {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!("stage #{} wait failed: {}", stage.id, e);
                    warn!("loop_runner: {}", msg);
                    "failed".to_string()
                }
            };

            // 4e. 写回 stage execution
            let final_stage_status = stage_status.clone();
            self.ctx
                .db
                .finish_stage_execution(
                    stage_exec.id,
                    &final_stage_status,
                    Some(record_id),
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;

            if stage_status == "success" {
                completed += 1;
                last_failed_record = None;
                let _ = self
                    .ctx
                    .db
                    .increment_loop_execution_counters(loop_execution_id, 1, 0)
                    .await;
            } else {
                failed += 1;
                last_failed_record = Some(record_id);
                let _ = self
                    .ctx
                    .db
                    .increment_loop_execution_counters(loop_execution_id, 0, 1)
                    .await;
            }

            // 4f. post_stage hooks（不论成功失败都 fire,gate 由 hook 自身处理）
            let post_stage_hooks = self
                .ctx
                .db
                .list_post_stage_hooks(loop_id, stage.id)
                .await
                .map_err(|e| e.to_string())?;
            for h in post_stage_hooks {
                let _ = self.fire_single_loop_hook(&h, &loop_).await;
            }
        }

        // 5. fire post_loop hooks
        let post_loop_hooks = self
            .ctx
            .db
            .list_hooks_by_loop_and_position(loop_id, "post_loop")
            .await
            .map_err(|e| e.to_string())?;
        for h in post_loop_hooks {
            let _ = self.fire_single_loop_hook(&h, &loop_).await;
        }

        // 6. 计算最终 status
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
            "loop #{} run done: status={} completed={} failed={}",
            loop_id, final_status, completed, failed
        );
        Ok(())
    }

    /// 启动 stage.todo 的执行,返回 execution_record_id。
    async fn start_stage_todo(
        &self,
        todo: &crate::models::Todo,
        trigger_type: &str,
        loop_idx: i64,
    ) -> Result<i64, String> {
        let request = RunTodoExecutionRequest {
            db: self.ctx.db.clone(),
            executor_registry: self.ctx.executor_registry.clone(),
            tx: self.ctx.tx.clone(),
            task_manager: self.ctx.task_manager.clone(),
            config: self.ctx.config.clone(),
            hook_service: self.hook_service.clone(),
            todo_id: todo.id,
            message: todo.prompt.clone(),
            req_executor: todo.executor.clone(),
            trigger_type: format!("loop_stage:{}", trigger_type),
            params: Some({
                let mut p = std::collections::HashMap::new();
                p.insert("loop_stage_index".to_string(), loop_idx.to_string());
                p
            }),
            resume_session_id: None,
            resume_message: None,
            chain: vec![],
            source_todo_id: None,
            source_todo_title: None,
            source_hook_id: None,
            feishu_bot_id: None,
            feishu_receive_id: None,
        };

        // run_todo_execution_with_params 是 async,我们 await
        let result = run_todo_execution_with_params(request).await;
        result
            .record_id
            .ok_or_else(|| "executor returned no record_id".to_string())
    }

    /// 订阅 broadcast 等待指定 record_id 的 Finished 事件。
    /// timeout 24h 防止长跑任务永久挂住 loop。
    ///
    /// broadcast 事件本身不带 record_id,这里在收到 Finished 后用 record_id 反查
    /// execution_records 状态:若本 record 还未到终态,继续等下一个 Finished;
    /// 多 loop 并发时,别人的 Finished 不会误判为本 record 完成。
    async fn wait_for_stage_finish(&self, record_id: i64) -> Result<String, String> {
        let mut rx = self.tx.subscribe();
        let wait_timeout = Duration::from_secs(24 * 60 * 60);
        let result = timeout(wait_timeout, async {
            loop {
                match rx.recv().await {
                    Ok(crate::handlers::ExecEvent::Finished { .. }) => {
                        // 反查: 这个 Finished 是不是 record_id 触发的?
                        // 不是则继续等
                        match self.ctx.db.get_execution_record(record_id).await {
                            Ok(Some(rec))
                                if matches!(
                                    rec.status.as_str(),
                                    "success" | "failed" | "cancelled" | "partial"
                                ) =>
                            {
                                return Ok(rec.status.to_string());
                            }
                            Ok(Some(_)) => continue, // 还在 running,等下一个
                            Ok(None) => continue,    // 已被清理/查不到,等下一个
                            Err(_) => continue,      // DB 抖动,等下一个,最后再兜底
                        }
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
            // result 是 `Result<Result<String, String>, tokio::time::error::Elapsed>`,
            // 外层 Ok 是 timeout 没到,内层是 waiter 自己返回的结果
            Ok(inner) => inner,
            Err(_) => Err(format!(
                "stage execution (record #{}) timeout after 24h",
                record_id
            )),
        }
    }

    /// 触发单个 loop hook（fire-and-forget,不阻塞 loop 主流程）。
    async fn fire_single_loop_hook(
        &self,
        h: &crate::db::entity::loop_hooks::Model,
        _loop_: &crate::db::entity::loops::Model,
    ) -> Result<(), String> {
        // 简化版: 复用 hooks::service 的 fire_for_todo 行为
        // 创建一个对应 target_todo 的执行,把 source 设成 loop
        let target = self
            .ctx
            .db
            .get_todo(h.target_todo_id)
            .await
            .map_err(|e| e.to_string())?;
        let Some(target) = target else {
            if h.skip_if_missing != 0 {
                return Ok(());
            }
            return Err(format!("target todo #{} not found", h.target_todo_id));
        };

        let request = RunTodoExecutionRequest {
            db: self.ctx.db.clone(),
            executor_registry: self.ctx.executor_registry.clone(),
            tx: self.ctx.tx.clone(),
            task_manager: self.ctx.task_manager.clone(),
            config: self.ctx.config.clone(),
            hook_service: self.hook_service.clone(),
            todo_id: target.id,
            message: target.prompt.clone(),
            req_executor: target.executor.clone(),
            trigger_type: format!("loop_hook:{}", h.hook_position),
            params: None,
            resume_session_id: None,
            resume_message: None,
            chain: vec![],
            source_todo_id: None,
            source_todo_title: None,
            source_hook_id: Some(h.id),
            feishu_bot_id: None,
            feishu_receive_id: None,
        };
        let _ = run_todo_execution_with_params(request).await;
        Ok(())
    }
}
