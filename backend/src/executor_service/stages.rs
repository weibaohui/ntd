//! 三阶段 stage 函数 —— 编排层。
//!
//! 模块职责：把 `run_todo_execution` 拆为 3 个 stage + 各自的 helper：
//!   - Stage 1: [`prepare_execution_state`] —— 拆解 request / 注册 task / 加载 todo /
//!     并发控制 / 触发 pre-hook / 选定 executor / 创建 execution_record
//!   - Stage 2: [`start_todo_and_prepare_spawn`] —— 落 worktree / start_todo /
//!     注册 TaskInfo / 准备要 move 进 spawn 闭包的字段
//!   - Stage 3: [`dispatch_spawned_executor_task`] —— `tokio::spawn` 子任务
//!
//! 每个 stage 函数返回 `Result<T, ExecutionResult>`：`Ok` 进入下一阶段，
//! `Err(ExecutionResult)` 表示需要把 ExecutionResult 直接返回给调用方。
//!
//! spawn 闭包内部的实现（[`super::spawn_lifecycle`]）单独成模块，避免本文件膨胀。

use std::sync::Arc;

use tracing::Instrument;
use uuid::Uuid;

use crate::adapters::CodeExecutor;
use crate::models::Todo;

use super::pre_spawn::{
    create_run_execution_record, enforce_concurrency_limit, fire_pre_execution_hook_if_needed,
    reject_start_todo_failure, select_executor_and_build_command,
    substitute_message_placeholders,
};
use super::spawn_lifecycle::run_spawned_executor_task;
use super::types::{PreparedExecution, SpawnInputs};
use super::worktree::{
    cleanup_worktree_if_needed, record_worktree_path, resolve_worktree_context, WorktreeContext,
};
use super::{ExecutionResult, RunTodoExecutionRequest};

// Re-export types from types module for sub-modules.
pub(crate) use super::types::TaskState;

/// Stage 1: 把 request 拆解并完成「executor 选定 + record 创建」前所有同步/异步检查。
///
/// 该阶段**不**启动 todo 状态变更，也**不**创建 worktree —— 这两步属于 stage 2，
/// 这样 stage 1 出错时无需清理 worktree，副作用面更窄。
///
/// 拆解思路：6 个独立子职责分别抽到 helper（substitute / register / concurrency /
/// hook / executor / record），顶层只负责串联；每个 helper 函数 ≤30 行。
pub(crate) async fn prepare_execution_state(
    request: RunTodoExecutionRequest,
) -> Result<PreparedExecution, ExecutionResult> {
    // 1) 占位符替换 + 拆解 request 字段。
    let substituted = substitute_message_placeholders(&request);
    // 2) 注册 task（生成 task_id + guard + cancel_rx）并加载 todo。
    let task_state = register_task_and_load_todo(&request).await?;
    // 3) 取运行时配置（max_concurrent / timeout_secs）。
    let (max_concurrent, timeout_secs) = read_runtime_config(&request);
    // 4) 加载 todo 后做并发检查 + pre-hook 触发。
    let initial_todo = task_state.todo.clone();
    let todo =
        enforce_concurrency_limit(&request, initial_todo, max_concurrent, &task_state.task_id)
            .await?;
    fire_pre_execution_hook_if_needed(&request, &todo, substituted.chain).await?;
    // 5) 选定 executor 并构造 command_args。
    let selected = select_executor_and_build_command(&request, &todo, &substituted.message).await?;
    // 6) 创建 execution record 并把 stage 1 产物聚合成 PreparedExecution。
    create_run_execution_record(request, task_state, todo, timeout_secs, selected).await
}

/// Stage 1 步骤 2：注册 task 并加载 todo。返回 task_id + guard + cancel_rx + todo。
///
/// Issue #506：用 RAII guard 注册 task，确保即便后续路径 panic/早返回忘了
/// remove，sender 也会被 guard drop 时清理。
pub(crate) async fn register_task_and_load_todo(
    request: &RunTodoExecutionRequest,
) -> Result<TaskState, ExecutionResult> {
    let task_id = Uuid::new_v4().to_string();
    let mut task_guard = request
        .task_manager
        .register_with_guard(task_id.clone())
        .await;
    let cancel_rx = task_guard.take_receiver();

    // 加载 todo；load 失败时仍然继续（不阻断执行），把 todo 视作 None。
    let todo = match request.db.get_todo(request.todo_id).await {
        Ok(Some(t)) => Some(t),
        Ok(None) => None,
        Err(e) => {
            tracing::error!(
                "Failed to fetch todo {} for executor selection: {}",
                request.todo_id,
                e
            );
            None
        }
    };
    Ok(TaskState {
        task_id,
        task_guard,
        cancel_rx,
        todo,
    })
}

/// Stage 1 步骤 3：从 config 读 max_concurrent + timeout_secs。
///
/// config lock 释放后两个值就各自独立可用，避免后续代码块带 lock。
pub(crate) fn read_runtime_config(request: &RunTodoExecutionRequest) -> (u32, u64) {
    let cfg = request.config.read().unwrap();
    (cfg.max_concurrent_todos, cfg.execution_timeout_secs)
}

/// Stage 2: 落 worktree / start_todo / 注册 TaskInfo，准备 spawn 闭包所需的全部字段。
///
/// 失败路径包括 start_todo_execution：失败时必须先清理已创建的 worktree，再返回
/// reject 路径的 ExecutionResult，避免「未启动成功」的执行记录与 worktree 残留错位。
pub(crate) async fn start_todo_and_prepare_spawn(
    prepared: PreparedExecution,
) -> Result<SpawnInputs, ExecutionResult> {
    let worktree_ctx = resolve_worktree_context(
        &prepared.request.db,
        &prepared.todo,
        prepared.request.workspace.as_deref(),
    )
    .await;
    record_worktree_path(
        &prepared.request.db,
        prepared.record_id,
        worktree_ctx.record_path.as_deref(),
    )
    .await;

    start_todo_or_cleanup(&prepared, &worktree_ctx).await?;
    let todo_title = {
        let t = extract_todo_title(&prepared.todo);
        if t.is_empty() {
            prepared.request.source_todo_title.clone().unwrap_or_default()
        } else {
            t
        }
    };
    let executor_spawn = prepared.executor.clone();
    let execution_timeout_secs = prepared.timeout_secs;

    register_websocket_task_info(&prepared, &todo_title, &executor_spawn).await;

    // effective_workspace 优先级：worktree 路径 > todo.workspace > request.workspace（loop 场景）
    let effective_workspace = worktree_ctx
        .effective_workspace
        .clone()
        .or(prepared.todo_workspace.clone())
        .or(prepared.request.workspace.clone());

    Ok(SpawnInputs {
        prepared,
        todo_title,
        executor_spawn,
        effective_workspace,
        execution_timeout_secs,
        worktree_ctx,
    })
}

/// 在 TaskManager 注册本次执行的任务信息（task_id / todo_id / executor 类型），
/// WebSocket 同步会从这里取最新 title + logs。
pub(crate) async fn register_websocket_task_info(
    prepared: &PreparedExecution,
    todo_title: &str,
    executor_spawn: &Arc<dyn CodeExecutor>,
) {
    prepared
        .request
        .task_manager
        .register_info(crate::task_manager::TaskInfo {
            task_id: prepared.task_id.clone(),
            todo_id: prepared.request.todo_id,
            todo_title: todo_title.to_string(),
            executor: executor_spawn.executor_type().to_string(),
            // 初始为空，WebSocket 同步时会从数据库获取实际日志。
            logs: "[]".to_string(),
        })
        .await;
}

/// 从 `Option<Todo>` 提取 title；todo 已删除时返回空串。
///
/// 之所以是独立 helper：spawn 闭包内多处需要 `todo_title: String`（emit event、
/// TaskInfo 注册、feishu 推送），抽出来后调用方都走同一处 title 解析逻辑，
/// 不必在每处重复 `todo.as_ref().map(...)`。
pub(crate) fn extract_todo_title(todo: &Option<Todo>) -> String {
    todo.as_ref().map(|t| t.title.clone()).unwrap_or_default()
}

/// 把 todo 标为 in_progress 并关联 task_id。失败时清掉 worktree，再走 reject 路径。
///
/// start_todo_execution 失败必须先 cleanup worktree：worktree 已在 stage 2 入口
/// 创建并写入 record_path，若启用了 auto_cleanup 不在这里清理会留下孤儿 worktree
/// 目录/分支与「未启动成功」的执行记录错位。
pub(crate) async fn start_todo_or_cleanup(
    prepared: &PreparedExecution,
    worktree_ctx: &WorktreeContext,
) -> Result<(), ExecutionResult> {
    if prepared.request.todo_id == 0 { return Ok(()); } // 环节独立执行
    if let Err(e) = prepared
        .request
        .db
        .start_todo_execution(prepared.request.todo_id, &prepared.task_id)
        .await
    {
        cleanup_worktree_if_needed(worktree_ctx);
        return Err(reject_start_todo_failure(
            &prepared.request.db,
            &prepared.request.tx,
            &prepared.request.task_manager,
            &prepared.task_id,
            prepared.request.todo_id,
            prepared
                .todo
                .as_ref()
                .map(|t| t.title.as_str())
                .unwrap_or(""),
            &prepared.executor_str,
            prepared.record_id,
            e,
        )
        .await);
    }
    Ok(())
}

/// Stage 3: `tokio::spawn` 出 fire-and-forget 子任务，并立刻返回 ExecutionResult。
///
/// 实际的 select! / match 逻辑放在 [`super::spawn_lifecycle::run_spawned_executor_task`] 里，
/// 这样 spawn 闭包退化为单行 `async move { run_spawned_executor_task(...).await }`，
/// 编排与执行两段清晰分离。
pub(crate) async fn dispatch_spawned_executor_task(spawned: SpawnInputs) -> ExecutionResult {
    let task_id_return = spawned.prepared.task_id.clone();
    let record_id = spawned.prepared.record_id;

    // 为整个 spawn 闭包建立 executor_run span：
    // tokio::spawn 不会自动继承外层 span（参见 issue #513），所以需要把异步块整体包到
    // Instrument 中。这样 child process spawn / stdout/stderr / log flush / db update /
    // hook fire 这一长串环节的日志都会被 executor_run span 包住。
    let executor_span = tracing::info_span!(
        "executor_run",
        task_id = %spawned.prepared.task_id,
        todo_id = spawned.prepared.request.todo_id,
        record_id = spawned.prepared.record_id,
        executor = %spawned.executor_spawn.executor_type(),
    );

    tokio::spawn(
        async move {
            run_spawned_executor_task(spawned).await;
        }
        .instrument(executor_span),
    );

    ExecutionResult {
        task_id: task_id_return,
        record_id: Some(record_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 顶层 `run_todo_execution` 必须是 `async fn(request) -> ExecutionResult`。
    #[test]
    fn test_run_todo_execution_signature_is_preserved() {
        fn _check_return_type(
            _req: RunTodoExecutionRequest,
        ) -> impl std::future::Future<Output = ExecutionResult> {
            super::super::run_todo_execution(_req)
        }
    }

    /// `PreparedExecution` 持有 `task_guard` 与 `cancel_rx` 两个 RAII 句柄。
    #[test]
    fn test_prepared_execution_carries_task_guard_and_cancel_rx() {
        fn _assert_fields(p: &PreparedExecution) -> &crate::task_manager::TaskGuard {
            &p.task_guard
        }
        fn _assert_cancel(p: &PreparedExecution) -> &tokio::sync::mpsc::Receiver<()> {
            &p.cancel_rx
        }
    }

    /// `SpawnInputs` 通过 `prepared` 字段持有 `task_guard` / `cancel_rx` / `executor_spawn`。
    #[test]
    fn test_spawn_inputs_carries_required_handles() {
        fn _assert_guard(s: &SpawnInputs) -> &crate::task_manager::TaskGuard {
            &s.prepared.task_guard
        }
        fn _assert_cancel(s: &SpawnInputs) -> &tokio::sync::mpsc::Receiver<()> {
            &s.prepared.cancel_rx
        }
        fn _assert_executor(s: &SpawnInputs) -> &Arc<dyn CodeExecutor> {
            &s.executor_spawn
        }
    }

    /// stage 函数之间通过 Result<_, ExecutionResult> 串联。
    #[test]
    fn test_stage_signatures_are_stable() {
        fn _check_return_type(
            _req: RunTodoExecutionRequest,
        ) -> impl std::future::Future<Output = Result<PreparedExecution, ExecutionResult>>
        {
            prepare_execution_state(_req)
        }
    }
}