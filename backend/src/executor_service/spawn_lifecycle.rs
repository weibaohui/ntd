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
use tokio::task::JoinHandle;

use crate::adapters::{insert_dir_arg, CodeExecutor};
use crate::db::Database;
use crate::executor_service::ExecEvent;
use crate::models::ParsedLogEntry;

use super::completion::{
    emit_post_execution_todo_progress, finalize_normal_completion, handle_cancellation_branch,
    handle_timeout_branch, persist_completion_record,
};
use super::log_capture::{
    await_readers, drain_readers_and_flush, flush_and_extract_result, send_event,
    setup_log_capture_pipeline,
};
use super::worktree::{cleanup_worktree_if_needed, kill_process_tree};
// SpawnContext/SpawnRuntime 两个参数对象是本文件各 stage 函数的签名基座
// （093-B3 塌缩后的统一入参形态），集中导入避免函数内反复全路径书写
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
    // 仅当 worktree 真正启用（effective_workspace_path 有值）时才让 stdin payload 预写生效，
    // 避免 pi 在未切目录场景下被多余的 `y` 污染 stdin 输入流。
    let worktree_active = runtime.worktree_ctx.effective_workspace_path.is_some();
    save_child_pid_and_close_stdin(&mut child, runtime.executor_spawn.as_ref(), &runtime.db, runtime.record_id, worktree_active).await;

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
            cleanup_worktree_if_needed(&runtime.worktree_ctx).await;
            handle_spawn_failure(runtime, e).await;
            None
        }
    }
}

/// `group_spawn` 失败时的清理：发 Output/Finished 事件 + finish_todo_execution + remove task。
/// 093-B3：12 参塌缩为 `&SpawnRuntime` + error（全部上下文字段均为 runtime 成员）。
pub(crate) async fn handle_spawn_failure(
    runtime: &SpawnRuntime,
    error: std::io::Error,
) {
    // 解出原名局部量保持函数体零改动（引用取向，零克隆）
    let db = runtime.db.as_ref();
    let tx = &runtime.tx;
    let task_manager = runtime.task_manager.as_ref();
    let task_id = runtime.task_id.as_str();
    let todo_id = runtime.todo_id;
    let todo_title = runtime.todo_title.as_str();
    let executor = runtime.executor_spawn.as_ref();
    let feishu_bot_id = runtime.feishu_bot_id;
    let feishu_receive_id = runtime.feishu_receive_id.clone();
    let feishu_receive_id_type = runtime.feishu_receive_id_type.clone();
    let workspace_id = runtime.prepared.request.workspace_id;
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
            feishu_receive_id_type,
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
///
/// `worktree_active` 仅在本次执行真正启用了 git worktree（即 `WorktreeContext.effective_workspace_path`
/// 为 `Some`）时传 true。pi 的 stdin 应答只有在这种场景下才需要——pi 切换到 worktree
/// 目录后会弹"directory changed, continue? [y/N]"的交互式确认；未启用 worktree 时
/// pi 不会弹该 prompt，预写 `y` 反而会污染 pi 的 stdin 输入流（被 pi 当作用户对
/// 其问题的应答消费掉），所以 gate 在本参数上精准开关。
pub(crate) async fn save_child_pid_and_close_stdin(
    child: &mut command_group::AsyncGroupChild,
    executor: &dyn crate::adapters::CodeExecutor,
    db: &Database,
    record_id: i64,
    worktree_active: bool,
) {
    // 仅当本次执行启用了 worktree 才预写 stdin payload。pi 的交互式确认 prompt
    // 只在切到 worktree 目录时出现；其他场景预写只会污染 stdin 输入流。
    // 写入失败不视为致命：关 stdin 本身仍能让子进程正常退出。
    if worktree_active {
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
///
/// spawn 前补一道目录参数注入（NTD-019）：zhanlu 系 CLI 不跟随进程 cwd，
/// 需要 `--dir <生效目录>` 显式指定。注入放这里而不是 argv 构造期
/// （pre_spawn），是因为 effective_workspace_path 此时才最终确定——
/// worktree 启用时要传 worktree 路径，argv 构造期只能拿到 todo.workspace_path。
pub(crate) fn spawn_executor_child(
    runtime: &SpawnRuntime,
) -> Result<command_group::AsyncGroupChild, std::io::Error> {
    // dir_arg() 为 None 的执行器（其余 10 家）insert_dir_arg 原样返回，argv 零变化。
    let command_args = insert_dir_arg(
        runtime.prepared.command_args.clone(),
        runtime.executor_spawn.dir_arg(),
        runtime.effective_workspace_path.as_deref(),
    );
    let mut cmd = build_executor_command(
        &runtime.prepared.executable_path,
        &command_args,
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
        feishu_receive_id_type: prepared.request.feishu_receive_id_type.clone(),
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
        // 093-B3：dispatch_cancellation / dispatch_timeout 纯转发壳已删除
        // （Remove Middle Man），match 臂直接装配 ProcessTeardown 调 run_*_path。
        super::types::RunOutcome::Cancelled => {
            run_cancellation_path(
                super::types::ProcessTeardown {
                    child,
                    stdout_task,
                    stderr_task,
                    log_flusher,
                    flush_timer,
                },
                &runtime,
            ).await;
        }
        super::types::RunOutcome::TimedOut => {
            run_timeout_path(
                super::types::ProcessTeardown {
                    child,
                    stdout_task,
                    stderr_task,
                    log_flusher,
                    flush_timer,
                },
                &runtime,
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
            feishu_receive_id_type: runtime.feishu_receive_id_type,
            workspace_id: runtime.prepared.request.workspace_id,
            // 092 P2：接力回写需要专家索引解析管家结论里的 @，从原 request 透传（Arc 浅克隆）。
            expert_manager: runtime.prepared.request.expert_manager.clone(),
            // 096-W4-5：DI 实例随 request 透传（与 expert_manager 同链路）
            blackboard_debouncer: runtime.prepared.request.blackboard_debouncer.clone(),
        },
    )
    .await;
}

/// 取消分支：kill 进程组 → drain readers → handle_cancellation_branch → cleanup worktree。
/// 093-B3：17 参塌缩为 2 参——进程句柄簇归 ProcessTeardown，上下文字段全在 SpawnRuntime 里。
pub(crate) async fn run_cancellation_path(
    teardown: super::types::ProcessTeardown<'_>,
    runtime: &SpawnRuntime,
) {
    // 先 kill：用户取消的第一语义是「立刻停」，任何后续读日志都不能阻塞停机
    kill_process_tree(teardown.child).await;
    // kill 后 drain：读取器可能还缓冲着进程退出前的最后输出，
    // 不 drain 直接丢会截断日志（且 flush_timer 需随结构 abort 防泄漏）
    drain_readers_and_flush(
        teardown.child,
        teardown.stdout_task,
        teardown.stderr_task,
        teardown.log_flusher,
        teardown.flush_timer,
    ).await;
    // 终态落库/发事件放在 drain 之后：保证 record 关联的日志完整可查
    handle_cancellation_branch(runtime).await;
    // worktree 清理放最后：前面步骤若读 worktree 内文件（如 result 解析），先清理会读空
    cleanup_worktree_if_needed(&runtime.worktree_ctx).await;
}

/// 超时分支：kill → drain → handle_timeout_branch → cleanup worktree。
/// 093-B3：17 参塌缩为 2 参——同 run_cancellation_path 的参数对象化。
pub(crate) async fn run_timeout_path(
    teardown: super::types::ProcessTeardown<'_>,
    runtime: &SpawnRuntime,
) {
    // 与取消分支同序（kill → drain → 终态 → 清理），理由一致；
    // 两分支刻意保持镜像结构：差异只在终态 handler（文案与状态码不同），
    // 读代码时对称性即正确性证据
    kill_process_tree(teardown.child).await;
    drain_readers_and_flush(
        teardown.child,
        teardown.stdout_task,
        teardown.stderr_task,
        teardown.log_flusher,
        teardown.flush_timer,
    ).await;
    handle_timeout_branch(runtime).await;
    cleanup_worktree_if_needed(&runtime.worktree_ctx).await;
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
    cleanup_worktree_if_needed(&ctx.worktree_ctx).await;
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
    // 093-B3：19 个逐字段解包塌缩为 ctx 透传 + CompletionOutcome 三元组聚合。
    // outcome 在此构造即转交（by value move）：finalize 之后 success/exit_code/result_str
    // 不再被本路径使用，move 语义恰好表达「终态数据的最终归宿」
    finalize_normal_completion(
        ctx,
        super::types::CompletionOutcome {
            success,
            exit_code,
            result_str,
        },
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

    /// issue 回归：pi 的 `echo y` stdin 预写只应在启用 worktree 时生效。
    /// 用 `cat` 作 mock child（把 stdin 回打到 stdout），分别传 worktree_active=false / true，
    /// 断言前者 stdout 空（未预写）、后者 stdout 含 payload（预写生效）。
    #[tokio::test]
    async fn test_save_child_pid_and_close_stdin_gates_by_worktree_active() {
        // 用 cat 把 stdin 内容回打到 stdout，作为「子进程是否收到 payload」的可观测代理。
        // Memory DB 让 update_execution_record_pid 不依赖真实表结构（record_id 不存在时 update no-op）。
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        // 用 pi executor：唯一 override stdin_payload 返回 Some("y\n") 的执行器，
        // 让 gate 的「不进入 stdin_payload 分支」语义可被 stdout 是否含 y 验证。
        let executor = crate::adapters::pi::PiExecutor::new("pi".to_string());

        // 未启用 worktree：gate 关闭，不应预写任何 payload，cat 收到空 stdin → stdout 空。
        let mut child_false = build_executor_command("cat", &[], None)
            .group_spawn()
            .expect("cat spawn must succeed");
        save_child_pid_and_close_stdin(
            &mut child_false,
            &executor,
            &db,
            0,
            false,
        )
        .await;
        let stdout_false = {
            // child 的 stdout 在 spawn 后由 group 持有；take 出来读全部内容直到 EOF。
            // 用 read_to_end 而非 readline，因为 cat 关 stdin 后会立刻退出。
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            if let Some(mut out) = child_false.inner().stdout.take() {
                out.read_to_end(&mut buf).await.expect("cat stdout read ok");
            }
            String::from_utf8_lossy(&buf).to_string()
        };
        assert_eq!(
            stdout_false, "",
            "worktree_active=false 时不应预写 stdin payload，cat stdout 应为空"
        );

        // 启用 worktree：gate 打开，预写 "y\n"，cat 回打 → stdout 含 "y\n"。
        let mut child_true = build_executor_command("cat", &[], None)
            .group_spawn()
            .expect("cat spawn must succeed");
        save_child_pid_and_close_stdin(
            &mut child_true,
            &executor,
            &db,
            0,
            true,
        )
        .await;
        let stdout_true = {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            if let Some(mut out) = child_true.inner().stdout.take() {
                out.read_to_end(&mut buf).await.expect("cat stdout read ok");
            }
            String::from_utf8_lossy(&buf).to_string()
        };
        assert_eq!(
            stdout_true, "y\n",
            "worktree_active=true 时应预写 pi 的 stdin payload 'y\\n'，cat stdout 应回打该内容"
        );
    }

    /// 构造最小 SpawnRuntime 夹具（CodeRabbit #1008 评审补充 handle_spawn_failure 单测）。
    /// 被测函数只读 db/tx/task_manager/executor_spawn/todo_id/todo_title/task_id
    /// 与 prepared.request.workspace_id，其余字段一律占位——但 PreparedExecution
    /// 字段全集必须 owned 构造，无部分构造捷径，故集中成 helper 表达
    /// 「这些占位与本次断言无关」的意图。
    /// 返回 (runtime, 事件接收端)：接收端用于断言广播出去的 Output/Finished。
    async fn make_spawn_runtime(
        db: std::sync::Arc<crate::db::Database>,
        task_id: &str,
    ) -> (super::SpawnRuntime, tokio::sync::broadcast::Receiver<crate::executor_service::ExecEvent>) {
        use crate::executor_service::types::{PreparedExecution, SpawnRuntime};
        use crate::executor_service::RunTodoExecutionRequest;
        use std::sync::Arc;

        let task_manager = Arc::new(crate::task_manager::TaskManager::default());
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let executor: Arc<dyn crate::adapters::CodeExecutor> =
            Arc::new(crate::adapters::pi::PiExecutor::new("pi".to_string()));
        // guard 必须真实注册：被测函数末尾 remove(task_id) 的行为断言依赖注册表先有这个 task
        let mut task_guard = task_manager.register_with_guard(task_id.to_string()).await;
        // 与生产路径同口径：cancel_rx 从 guard 中 take（register_task_and_load_todo 的模式）
        let cancel_rx = task_guard.take_receiver();
        let request = RunTodoExecutionRequest {
            blackboard_debouncer: crate::services::blackboard_debouncer::BlackboardDebouncer::new(),
            db: db.clone(),
            executor_registry: Arc::new(crate::adapters::ExecutorRegistry::new()),
            tx: tx.clone(),
            task_manager: task_manager.clone(),
            config: Arc::new(std::sync::RwLock::new(crate::config::Config::default())),
            todo_id: 0,
            message: String::new(),
            req_executor: None,
            req_model: None,
            trigger_type: "test".to_string(),
            params: None,
            resume_session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            feishu_bot_id: None,
            feishu_receive_id: None,
            feishu_receive_id_type: None,
            loop_step_execution_id: None,
            step_id: None,
            workspace_path: None,
            workspace_id: None,
            expert_manager: None,
        };
        let prepared = PreparedExecution {
            request,
            task_guard,
            cancel_rx,
            task_id: task_id.to_string(),
            command_args: vec![],
            executable_path: "pi".to_string(),
            executor: executor.clone(),
            executor_str: "pi".to_string(),
            record_id: 0,
            todo: None,
            todo_workspace_path: None,
            timeout_secs: 0,
        };
        let runtime = SpawnRuntime {
            db,
            tx,
            task_manager,
            todo_id: 0,
            todo_title: "placeholder".to_string(),
            executor_spawn: executor,
            record_id: 0,
            worktree_ctx: Default::default(),
            task_id: task_id.to_string(),
            execution_timeout_secs: 0,
            feishu_bot_id: None,
            feishu_receive_id: None,
            feishu_receive_id_type: None,
            effective_workspace_path: None,
            prepared,
        };
        (runtime, rx)
    }

    /// spawn 失败路径编排：todo 落 failed、Output/Finished 各广播一次、task 从注册表移除。
    /// 覆盖正常分支（有 todo 行）与边界（todo_id=0 的环节独立执行不碰 DB）。
    #[tokio::test]
    async fn test_handle_spawn_failure_marks_failed_and_emits_events() {
        let db = std::sync::Arc::new(crate::db::Database::new(":memory:").await.unwrap());
        // 真实 seed 一个 todo：finish_todo_execution(id, false) 要把它的 status 翻成 failed
        let todo_id = db.create_todo("T", "prompt").await.unwrap();
        let (mut runtime, mut rx) = make_spawn_runtime(db.clone(), "task-spawn-fail").await;
        // 夹具 todo_id 默认 0（独立执行不碰 DB），本用例覆盖「有 todo」主路径
        runtime.todo_id = todo_id;

        super::handle_spawn_failure(&runtime, std::io::Error::other("spawn boom")).await;

        // 断言 1：todo 状态翻转为 failed（DB 侧最终一致性证据；status 是 TodoStatus 枚举）
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert_eq!(todo.status, crate::models::TodoStatus::Failed, "spawn 失败应把 todo 置为 failed");
        // 断言 2：广播恰好 Output(error) + Finished(success=false) 两条
        let first = rx.try_recv().expect("应收到 Output 事件");
        let second = rx.try_recv().expect("应收到 Finished 事件");
        assert!(matches!(first, crate::executor_service::ExecEvent::Output { .. }));
        match second {
            crate::executor_service::ExecEvent::Finished { success, result, .. } => {
                assert!(!success, "spawn 失败的 Finished 必须 success=false");
                assert!(result.unwrap_or_default().contains("spawn boom"), "Finished 应携带原始错误文案");
            }
            other => panic!("第二帧应为 Finished，实际: {other:?}"),
        }
        // 断言 3：task 已从注册表移除（cancel 查无此 task 返回 false）
        assert!(!runtime.task_manager.cancel("task-spawn-fail").await, "task 应已被 remove");
    }

    /// spawn 失败路径边界：todo_id=0（环节独立执行）时 finish_todo_execution 短路，
    /// 不碰 DB 也不报错，事件仍正常广播。
    #[tokio::test]
    async fn test_handle_spawn_failure_todo_id_zero_skips_db() {
        let db = std::sync::Arc::new(crate::db::Database::new(":memory:").await.unwrap());
        let (runtime, mut rx) = make_spawn_runtime(db, "task-spawn-fail-zero").await;

        super::handle_spawn_failure(&runtime, std::io::Error::other("no process")).await;

        // todo_id=0 时 DB 无写入目标，只需验证事件帧完整（Output + Finished）
        assert!(rx.try_recv().is_ok(), "Output 事件应照常广播");
        assert!(rx.try_recv().is_ok(), "Finished 事件应照常广播");
        assert!(!runtime.task_manager.cancel("task-spawn-fail-zero").await);
    }
}