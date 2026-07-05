//! Spawn 闭包生命周期管理。
//!
//! 模块职责：`run_spawned_executor_task` spawn 出来后的全部子任务实现，包括：
//!   - 启动子进程 + 关 stdin + 写 PID
//!   - 装配 log capture pipeline（stdout/stderr/flusher/timer）
//!   - select! 等待 outcome（cancel / timeout / child exit）
//!   - 按 outcome 分发到 cancel / timeout / completed 三个分支
//!   - 各分支末段的 DB / event / cleanup
//!
//! 与 [`super::stages`] 的区别：stages 只负责"stage 之间的数据搬运 + 入口编排"，
//! 本模块负责"spawn 闭包内部的事"。

use std::sync::Arc;

use command_group::AsyncCommandGroup;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::adapters::CodeExecutor;
use crate::db::Database;
use crate::executor_service::ExecEvent;
use crate::models::ParsedLogEntry;
use crate::task_manager::TaskManager;

use super::completion::{
    emit_post_execution_todo_progress, finalize_normal_completion, handle_cancellation_branch,
    handle_timeout_branch, persist_completion_record,
};
use super::log_capture::{
    await_readers, drain_readers_and_flush, flush_and_extract_result, send_event,
    setup_log_capture_pipeline,
};
use super::worktree::{cleanup_worktree_if_needed, kill_process_tree, WorktreeContext};
use super::types::{SpawnContext, SpawnRuntime};

/// issue #660: 原来 449 行 `run_todo_execution` 的 spawn 闭包体。
///
/// 该函数由 `dispatch_spawned_executor_task` 通过 `tokio::spawn` 调用，是 fire-and-forget
/// 子任务的真正实现。设计上与重构前的闭包体逐位等价——所有副作用（emit event、写 DB、
/// fire hook、清理 worktree）都按原顺序保留。
pub(crate) async fn run_spawned_executor_task(spawned: super::types::SpawnInputs) {
    // 编排流程：构建 runtime → 启动子进程 → 等待 outcome → dispatch。
    let execution_start = std::time::Instant::now();
    let mut runtime = move_into_runtime(spawned);

    super::completion::emit_started_event(
        &runtime.tx,
        &runtime.task_id,
        runtime.todo_id,
        &runtime.todo_title,
        runtime.executor_spawn.as_ref(),
        runtime.prepared.request.workspace_id,
    );

    let Some(mut child) = try_spawn_executor_child(&runtime).await else {
        return;
    };
    save_child_pid_and_close_stdin(&mut child, runtime.executor_spawn.as_ref(), &runtime.db, runtime.record_id).await;

    let (log_flusher, stdout_task, stderr_task, flush_timer) =
        setup_log_capture_pipeline_for(&runtime, &mut child).await;
    let outcome = await_run_outcome_with_timeout(&mut runtime, &mut child).await;
    dispatch_outcome(
        outcome,
        &mut child,
        stdout_task,
        stderr_task,
        log_flusher,
        flush_timer,
        runtime,
        execution_start,
    )
    .await;
}

/// 启动子进程；spawn 失败时清理 worktree 并触发 spawn failure 路径，返回 `None`。
///
/// 返回 `Option` 让调用点用 `let ... else { return; }` 早退，省去 match/Err 分支。
pub(crate) async fn try_spawn_executor_child(
    runtime: &SpawnRuntime,
) -> Option<command_group::AsyncGroupChild> {
    match spawn_executor_child(runtime) {
        Ok(c) => Some(c),
        Err(e) => {
            cleanup_worktree_if_needed(&runtime.worktree_ctx);
            handle_spawn_failure(
                &runtime.db,
                &runtime.tx,
                &runtime.task_manager,
                &runtime.task_id,
                runtime.todo_id,
                &runtime.todo_title,
                runtime.executor_spawn.as_ref(),
                runtime.feishu_bot_id,
                runtime.feishu_receive_id.clone(),
                e,
                runtime.prepared.request.workspace_id,
            )
            .await;
            None
        }
    }
}

/// `group_spawn` 失败时的清理：发 Output/Finished 事件 + finish_todo_execution + remove task。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_spawn_failure(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn CodeExecutor,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
    error: std::io::Error,
    workspace_id: Option<i64>,
) {
    let error_msg = format!("Failed to spawn executor: {}", error);
    let entry = ParsedLogEntry::error(error_msg.clone());
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
            result: Some(error_msg),
            feishu_bot_id,
            feishu_receive_id,
            workspace_id,
            // spawn 阶段尚未产生任何执行时长与 token 消耗，置 0 避免阻塞 Finished 事件下发
            duration_secs: 0,
            total_tokens: 0,
            // spawn 失败属于早期阶段，没有 trigger_type 上下文，传 None。
            trigger_type: None,
        },
    );
    let _ = db.finish_todo_execution(todo_id, false).await;
    task_manager.remove(task_id).await;
}

/// 关掉子进程 stdin 并把进程组 leader PID 写库。
///
/// 关 stdin 是必须的：不少 executor 在执行完后会再读一次 stdin，没有 EOF 就会 hang。
/// PID 写库是为了后续 cancel / status 查询能定位进程；child.id() == None 表示
/// 进程已退出（race），跳过写库即可。
///
/// `executor` 用于查询 `stdin_payload()`：部分执行器（pi 等）需要在关闭 stdin 之前
/// 预写自动应答，避免子进程卡在交互式 prompt 上；等价于 `echo y | pi -p ...`。
pub(crate) async fn save_child_pid_and_close_stdin(
    child: &mut command_group::AsyncGroupChild,
    executor: &dyn crate::adapters::CodeExecutor,
    db: &Database,
    record_id: i64,
) {
    // 若执行器声明需要预写 stdin（典型场景：pi 启用 Worktree 后会在交互式 prompt
    // 卡住，等价于 `echo y | pi ...` 的管道输入），先一次性写入再关闭 stdin。
    // 写入失败不视为致命：关 stdin 本身仍能让子进程正常退出。
    if let Some(payload) = executor.stdin_payload() {
        if let Some(stdin) = child.inner().stdin.as_mut() {
            if let Err(e) = stdin.write_all(payload.as_bytes()).await {
                tracing::warn!(
                    "[spawn] 写入执行器 stdin payload 失败: executor={} err={}",
                    executor.executor_type().as_str(),
                    e
                );
            }
            if let Err(e) = stdin.flush().await {
                tracing::warn!(
                    "[spawn] flush 执行器 stdin 失败: executor={} err={}",
                    executor.executor_type().as_str(),
                    e
                );
            }
        }
    }
    // 关 stdin 让子进程在读完 payload 后立即收到 EOF，避免挂起。
    drop(child.inner().stdin.take());
    let child_id = child.id().unwrap_or(0);
    if child_id > 0 {
        let _ = db
            .update_execution_record_pid(record_id, Some(child_id as i32))
            .await;
    }
}

/// 构造 executor 子进程命令，统一设置 stdout/stderr/stdin 为 piped。
///
/// workspace_path 设置为 `cmd.current_dir`，但仅在 todo 指定 workspace_path 时生效——
/// 没设 workspace_path 的 todo 让 executor 用 daemon 当前目录即可。
pub(crate) fn build_executor_command(
    executable_path: &str,
    command_args: &[String],
    workspace_path: Option<&str>,
) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(executable_path);
    cmd.args(command_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::piped());
    if let Some(ws) = workspace_path {
        cmd.current_dir(ws);
    }
    cmd
}

/// `build_executor_command` + `group_spawn` 两步合一：argv 已就绪，直接
/// 创建进程组让 kill 时能整组杀，避免留下 zombie 子进程。
pub(crate) fn spawn_executor_child(
    runtime: &SpawnRuntime,
) -> Result<command_group::AsyncGroupChild, std::io::Error> {
    let mut cmd = build_executor_command(
        &runtime.prepared.executable_path,
        &runtime.prepared.command_args,
        runtime.effective_workspace_path.as_deref(),
    );
    cmd.group_spawn()
}

/// 把 stdout/stderr handle 拆出来，连同 db/tx 一起喂给 `setup_log_capture_pipeline`。
pub(crate) async fn setup_log_capture_pipeline_for(
    runtime: &SpawnRuntime,
    child: &mut command_group::AsyncGroupChild,
) -> (
    Arc<crate::log_flusher::LogFlusher>,
    Option<JoinHandle<()>>,
    Option<JoinHandle<()>>,
    JoinHandle<()>,
) {
    let stdout_handle = child.inner().stdout.take();
    let stderr_handle = child.inner().stderr.take();
    setup_log_capture_pipeline(
        stdout_handle,
        stderr_handle,
        runtime.executor_spawn.clone(),
        runtime.db.clone(),
        runtime.tx.clone(),
        runtime.task_id.clone(),
        runtime.record_id,
        runtime.prepared.request.workspace_id,
        runtime.prepared.request.resume_session_id.clone(),
    )
    .await
}

/// 配置超时 + 在 select! 中 await outcome（cancel / timeout / child exit）。
///
/// 把 timeout_sleep 的 pin 留在 helper 内。cancel_rx 通过 `runtime.prepared.cancel_rx`
/// 借用，避免 SpawnRuntime 顶层冗余 cancel_rx 字段。
pub(crate) async fn await_run_outcome_with_timeout(
    runtime: &mut SpawnRuntime,
    child: &mut command_group::AsyncGroupChild,
) -> super::types::RunOutcome {
    let mut timeout_sleep = configure_timeout_sleep(runtime.execution_timeout_secs);
    await_run_outcome(
        &mut runtime.prepared.cancel_rx,
        &mut timeout_sleep,
        runtime.execution_timeout_secs,
        child,
    )
    .await
}

/// 把 `SpawnInputs` 全部字段展开到 `SpawnRuntime`。
///
/// 先把 `prepared` 整体下沉到本地变量（避开 `spawned.prepared.cancel_rx` 与
/// `prepared: spawned.prepared` 同时部分 move 触发 E0382）。
pub(crate) fn move_into_runtime(spawned: super::types::SpawnInputs) -> SpawnRuntime {
    let prepared = spawned.prepared;
    SpawnRuntime {
        db: prepared.request.db.clone(),
        tx: prepared.request.tx.clone(),
        task_manager: prepared.request.task_manager.clone(),
        todo_id: prepared.request.todo_id,
        todo_title: spawned.todo_title.clone(),
        executor_spawn: spawned.executor_spawn.clone(),
        record_id: prepared.record_id,
        worktree_ctx: spawned.worktree_ctx,
        task_id: prepared.task_id.clone(),
        execution_timeout_secs: spawned.execution_timeout_secs,
        feishu_bot_id: prepared.request.feishu_bot_id,
        feishu_receive_id: prepared.request.feishu_receive_id.clone(),
        // 关键：把 effective_workspace_path 整字段 move 进 runtime，
        // 避免 spawn_executor_child 误用 todo_workspace_path（worktree 失效）。
        effective_workspace_path: spawned.effective_workspace_path,
        prepared,
    }
}

/// 把超时换算成 `Pin<Box<Sleep>>`。`execution_timeout_secs == 0` 表示禁用超时，
/// 此时返回「永久 sleep」的 future，select! 永远不命中该分支。
pub(crate) fn configure_timeout_sleep(
    execution_timeout_secs: u64,
) -> std::pin::Pin<Box<tokio::time::Sleep>> {
    let timeout_enabled = execution_timeout_secs > 0;
    let duration = std::time::Duration::from_secs(execution_timeout_secs);
    let sleep = tokio::time::sleep(if timeout_enabled {
        duration
    } else {
        // 用一个非常大的 duration（u64::MAX 秒 ≈ 5.8 亿年）模拟「永不超时」。
        std::time::Duration::from_secs(u64::MAX)
    });
    Box::pin(sleep)
}

/// select! 收口：cancel 优先 → timeout 次之 → child wait。
///
/// `biased;` 让取消分支优先于超时分支，避免「按 timeout_secs 比较大、但用户已经
/// 点了取消」的请求被超时路径抢走（issue #606 提到的边界 case）。
pub(crate) async fn await_run_outcome(
    cancel_rx: &mut tokio::sync::mpsc::Receiver<()>,
    timeout_sleep: &mut std::pin::Pin<Box<tokio::time::Sleep>>,
    execution_timeout_secs: u64,
    child: &mut command_group::AsyncGroupChild,
) -> super::types::RunOutcome {
    let timeout_enabled = execution_timeout_secs > 0;
    tokio::select! {
        biased;
        _ = cancel_rx.recv() => super::types::RunOutcome::Cancelled,
        _ = timeout_sleep, if timeout_enabled => super::types::RunOutcome::TimedOut,
        status = child.wait() => super::types::RunOutcome::Completed(status),
    }
}

/// select! 收口之后按 outcome 分发到 cancellation / timeout / completion 三个分支。
///
/// 拆分为 3 个 dispatch_* helper + 1 个 match wrapper；每个 helper 只负责本分支的
/// 参数组装与路径调用，match 本身退化为纯枚举映射。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_outcome(
    outcome: super::types::RunOutcome,
    child: &mut command_group::AsyncGroupChild,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<crate::log_flusher::LogFlusher>,
    flush_timer: JoinHandle<()>,
    runtime: SpawnRuntime,
    execution_start: std::time::Instant,
) {
    match outcome {
        super::types::RunOutcome::Cancelled => {
            dispatch_cancellation(
                child,
                stdout_task,
                stderr_task,
                log_flusher,
                flush_timer,
                runtime,
            ).await;
        }
        super::types::RunOutcome::TimedOut => {
            dispatch_timeout(
                child,
                stdout_task,
                stderr_task,
                log_flusher,
                flush_timer,
                runtime,
            )
            .await;
        }
        super::types::RunOutcome::Completed(status) => {
            dispatch_completed(
                status,
                stdout_task,
                stderr_task,
                log_flusher,
                flush_timer,
                runtime,
                execution_start,
            )
            .await;
        }
    }
}

/// Cancelled 分支：kill + drain + handle_cancellation_branch + cleanup worktree。
#[allow(clippy::too_many_arguments)]
async fn dispatch_cancellation(
    child: &mut command_group::AsyncGroupChild,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<crate::log_flusher::LogFlusher>,
    flush_timer: JoinHandle<()>,
    runtime: SpawnRuntime,
) {
    run_cancellation_path(
        child,
        stdout_task,
        stderr_task,
        log_flusher,
        flush_timer,
        &runtime.db,
        &runtime.tx,
        &runtime.task_manager,
        &runtime.task_id,
        runtime.todo_id,
        &runtime.todo_title,
        runtime.executor_spawn.as_ref(),
        runtime.record_id,
        runtime.feishu_bot_id,
        runtime.feishu_receive_id.clone(),
        &runtime.worktree_ctx,
        runtime.prepared.request.workspace_id,
    )
    .await;
}

/// TimedOut 分支：kill + drain + handle_timeout_branch + cleanup worktree。
#[allow(clippy::too_many_arguments)]
async fn dispatch_timeout(
    child: &mut command_group::AsyncGroupChild,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<crate::log_flusher::LogFlusher>,
    flush_timer: JoinHandle<()>,
    runtime: SpawnRuntime,
) {
    run_timeout_path(
        child,
        stdout_task,
        stderr_task,
        log_flusher,
        flush_timer,
        &runtime.db,
        &runtime.tx,
        &runtime.task_manager,
        &runtime.task_id,
        runtime.todo_id,
        &runtime.todo_title,
        runtime.executor_spawn.as_ref(),
        runtime.record_id,
        runtime.execution_timeout_secs,
        runtime.feishu_bot_id,
        runtime.feishu_receive_id.clone(),
        &runtime.worktree_ctx,
        runtime.prepared.request.workspace_id,
    )
    .await;
}

/// Completed 分支：装配 SpawnContext + handle_completed_branch。
async fn dispatch_completed(
    status: std::io::Result<std::process::ExitStatus>,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<crate::log_flusher::LogFlusher>,
    flush_timer: JoinHandle<()>,
    runtime: SpawnRuntime,
    execution_start: std::time::Instant,
) {
    handle_completed_branch(
        status,
        stdout_task,
        stderr_task,
        log_flusher,
        flush_timer,
        SpawnContext {
            db: runtime.db,
            tx: runtime.tx,
            task_manager: runtime.task_manager,
            executor_registry: runtime.prepared.request.executor_registry.clone(),
            config: runtime.prepared.request.config.clone(),
            executor: runtime.executor_spawn,
            task_id: runtime.task_id,
            todo_id: runtime.todo_id,
            todo_title: runtime.todo_title,
            record_id: runtime.record_id,
            execution_start,
            worktree_ctx: runtime.worktree_ctx,
            trigger_type: runtime.prepared.request.trigger_type,
            feishu_bot_id: runtime.feishu_bot_id,
            feishu_receive_id: runtime.feishu_receive_id,
            workspace_id: runtime.prepared.request.workspace_id,
        },
    )
    .await;
}

/// 取消分支：kill 进程组 → drain readers → handle_cancellation_branch → cleanup worktree。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_cancellation_path(
    child: &mut command_group::AsyncGroupChild,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<crate::log_flusher::LogFlusher>,
    flush_timer: JoinHandle<()>,
    db: &Arc<Database>,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &Arc<TaskManager>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn CodeExecutor,
    record_id: i64,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
    worktree_ctx: &WorktreeContext,
    workspace_id: Option<i64>,
) {
    kill_process_tree(child).await;
    drain_readers_and_flush(child, stdout_task, stderr_task, log_flusher, flush_timer).await;
    handle_cancellation_branch(
        db,
        tx,
        task_manager,
        task_id,
        todo_id,
        todo_title,
        executor,
        record_id,
        feishu_bot_id,
        feishu_receive_id,
        workspace_id,
    )
    .await;
    cleanup_worktree_if_needed(worktree_ctx);
}

/// 超时分支：kill → drain → handle_timeout_branch → cleanup worktree。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_timeout_path(
    child: &mut command_group::AsyncGroupChild,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<crate::log_flusher::LogFlusher>,
    flush_timer: JoinHandle<()>,
    db: &Arc<Database>,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &Arc<TaskManager>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor: &dyn CodeExecutor,
    record_id: i64,
    execution_timeout_secs: u64,
    feishu_bot_id: Option<i64>,
    feishu_receive_id: Option<String>,
    worktree_ctx: &WorktreeContext,
    workspace_id: Option<i64>,
) {
    kill_process_tree(child).await;
    drain_readers_and_flush(child, stdout_task, stderr_task, log_flusher, flush_timer).await;
    handle_timeout_branch(
        db,
        tx,
        task_manager,
        task_id,
        todo_id,
        todo_title,
        executor,
        record_id,
        execution_timeout_secs,
        super::completion::format_timeout_secs(execution_timeout_secs),
        feishu_bot_id,
        feishu_receive_id,
        workspace_id,
    )
    .await;
    cleanup_worktree_if_needed(worktree_ctx);
}

/// 把「正常退出 → await readers → finalize flusher → emit progress →
/// 解析 result → persist record → finalize_normal_completion → cleanup worktree」
/// 整条完成路径抽到一个函数，让 `run_spawned_executor_task` 的 match 分支只剩下
/// kill + drain + 调对应 helper 的骨架。
pub(crate) async fn handle_completed_branch(
    status: std::io::Result<std::process::ExitStatus>,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    log_flusher: Arc<crate::log_flusher::LogFlusher>,
    flush_timer: JoinHandle<()>,
    ctx: SpawnContext,
) {
    // 编排「正常完成」路径：await readers → 解析 exit → 发进度 → flush 提取 →
    // persist record + finalize → cleanup worktree。
    await_readers(stdout_task, stderr_task).await;
    let (exit_code, success) = resolve_exit_outcome(&status, ctx.executor.as_ref());
    emit_post_execution_todo_progress(
        &ctx.db,
        &ctx.tx,
        ctx.executor.as_ref(),
        &ctx.task_id,
        ctx.record_id,
        ctx.workspace_id,
    )
    .await;
    let (logs_snapshot, result_str) =
        flush_and_extract_result(log_flusher, flush_timer, &ctx.db, ctx.record_id).await;
    persist_and_finalize_completion(&ctx, success, exit_code, &logs_snapshot, result_str).await;
    cleanup_worktree_if_needed(&ctx.worktree_ctx);
}

/// 把 `ExitStatus` 翻译成「exit_code + success」。executor 子类自行决定什么
/// exit code 算成功（claude_code 把 0 当成功，hermes 把 0/1 之外的都当失败等）。
pub(crate) fn resolve_exit_outcome(
    status: &std::io::Result<std::process::ExitStatus>,
    executor: &dyn CodeExecutor,
) -> (i32, bool) {
    let exit_code = status
        .as_ref()
        .map(|s| s.code().unwrap_or(-1))
        .unwrap_or(-1);
    let success = executor.check_success(exit_code);
    (exit_code, success)
}

/// persist_completion_record + finalize_normal_completion 二合一：
///
/// 把原本散落在 handle_completed_branch 末尾的 21 参数 finalize 调用收口到一个 helper。
pub(crate) async fn persist_and_finalize_completion(
    ctx: &SpawnContext,
    success: bool,
    exit_code: i32,
    logs_snapshot: &[ParsedLogEntry],
    result_str: String,
) {
    persist_completion_record(
        &ctx.db,
        ctx.record_id,
        logs_snapshot,
        success,
        ctx.execution_start,
    )
    .await;
    finalize_normal_completion(
        ctx.db.clone(),
        ctx.executor_registry.clone(),
        ctx.tx.clone(),
        ctx.task_manager.clone(),
        ctx.config.clone(),
        ctx.executor.clone(),
        ctx.task_id.clone(),
        ctx.todo_id,
        ctx.todo_title.clone(),
        ctx.record_id,
        success,
        exit_code,
        result_str,
        ctx.trigger_type.clone(),
        ctx.feishu_bot_id,
        ctx.feishu_receive_id.clone(),
        ctx.workspace_id,
    )
    .await;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    /// `build_executor_command` 必须：把 executable 作为 argv[0]、追加 args、设置 piped stdio。
    /// 工作目录仅在显式传 workspace 时设置；workspace=None 时 executor 沿用 daemon cwd。
    #[test]
    fn test_build_executor_command_basic_args() {
        let args = vec!["-p".to_string(), "hello".to_string()];
        let cmd = build_executor_command("/usr/bin/claude", &args, None);
        let std_cmd: &std::process::Command = cmd.as_std();
        let program = std_cmd.get_program();
        assert_eq!(program, "/usr/bin/claude");
        let std_args: Vec<&std::ffi::OsStr> = std_cmd.get_args().collect();
        assert_eq!(std_args.len(), 2);
        assert_eq!(std_args[0], "-p");
        assert_eq!(std_args[1], "hello");
    }

    #[test]
    fn test_build_executor_command_workspace_sets_current_dir() {
        let args = vec!["-p".to_string()];
        let with_ws = build_executor_command("/bin/echo", &args, Some("/tmp/work"));
        assert_eq!(
            with_ws.as_std().get_current_dir().unwrap(),
            std::path::Path::new("/tmp/work")
        );

        let no_ws = build_executor_command("/bin/echo", &args, None);
        assert!(no_ws.as_std().get_current_dir().is_none());
    }

    #[test]
    fn test_build_executor_command_constructs_cleanly() {
        let args = vec!["-p".to_string(), "build me a web app".to_string()];
        let cmd = build_executor_command("/usr/local/bin/codex", &args, None);
        let std_cmd = cmd.as_std();
        assert_eq!(std_cmd.get_program(), "/usr/local/bin/codex");
        let std_args: Vec<&std::ffi::OsStr> = std_cmd.get_args().collect();
        assert_eq!(std_args.len(), 2);
        assert_eq!(std_args[1], "build me a web app");
    }

    /// `SpawnRuntime` 持有 `effective_workspace_path` 字段；`prepared.todo_workspace_path` 与
    /// `effective_workspace_path` 是两个独立字段（issue #660 重构中的回归测试）。
    #[test]
    fn test_spawn_runtime_carries_effective_workspace_path() {
        fn _assert_field(rt: &SpawnRuntime) -> Option<&String> {
            rt.effective_workspace_path.as_ref()
        }
        fn _assert_distinct_fields(
            rt: &SpawnRuntime,
        ) -> (Option<&String>, Option<&String>) {
            (
                rt.effective_workspace_path.as_ref(),
                rt.prepared.todo_workspace_path.as_ref(),
            )
        }
    }
}