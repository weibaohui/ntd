//! Loop Runner — 顺序执行 loop 的所有 stage。
//!
//! 执行模型：
//! 1. 创建 loop_executions 行（status=running）
//! 2. 按 order_index 顺序遍历 stages：
//!    a. 启动 stage.todo 的执行（复用 executor_service::start_todo_execution）
//!    b. 写 loop_stage_execution 行
//!    c. 订阅 broadcast::tx 等待该 stage 的 ExecEvent::Finished
//!    d. 应用 rating gate（决定是否继续 / 中止 loop）
//! 3. 计算最终 status（success / partial / failed / cancelled）
//! 4. 写回 loop_executions
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
use crate::models::ExecutionStatus;
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
        let initial_total_stages = 0i32; // 创建时还没确定 stage 数,后面在 run_inner 里 update
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
                        initial_total_stages,
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
        // 更新 total_stages
        self.ctx
            .db
            .finish_loop_execution(loop_execution_id, "running", 0, 0)
            .await
            .map_err(|e| e.to_string())?; // placeholder,会再被覆盖
        // 上面 finish 是把 status 写回 running 但也设置了 finished_at,这里重新刷一下
        // 改为直接 SQL 清掉 finished_at
        self.clear_finished_at(loop_execution_id).await?;
        self.update_total_stages(loop_execution_id, stages.len() as i32)
            .await?;

        // 3. 顺序遍历 stages
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

            // a. 启动 stage execution
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

            // b. 实际执行 todo
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

            // 4e.
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
    async fn wait_for_stage_finish(&self, record_id: i64) -> Result<String, String> {
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
                        // 因为 loop 是顺序的,这时只有当前 stage 在跑。
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
                "stage execution (record #{}) timeout after 24h",
                record_id
            )),
        }
    }

    /// 把 loop_executions 的 finished_at 清空（避免被 finish_loop_execution 错填）。
    async fn clear_finished_at(&self, id: i64) -> Result<(), String> {
        use sea_orm::{ConnectionTrait, Statement};
        let sql = format!("UPDATE loop_executions SET finished_at = NULL WHERE id = {}", id);
        self.ctx
            .db
            .conn
            .execute(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn update_total_stages(&self, id: i64, total: i32) -> Result<(), String> {
        use sea_orm::{ConnectionTrait, Statement};
        let sql = format!(
            "UPDATE loop_executions SET total_stages = {} WHERE id = {}",
            total, id
        );
        self.ctx
            .db
            .conn
            .execute(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
