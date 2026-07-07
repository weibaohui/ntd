//! Pre-spawn 失败短路与并发上限控制。
//!
//! 模块职责：
//!   1. [`resolve_executor_type`] —— 三层优先级（显式 > todo 存储 > 默认）选 executor 类型
//!   2. [`count_active_running_for_todo`] —— 过滤 zombie 后统计活跃并发
//!   3. `reject_*` 系列 —— 把"任务失败时的 cleanup + Finished event + 返回 ExecutionResult"
//!      集中到一处，让 stage 1 的 match arm 缩到 1 行
//!   4. [`select_executor_and_build_command`] —— 选 executor 实例 + 构造 command argv
//!
//! 各函数 ≤ 30 行；编排层（stages / completion）只调用本模块的入口。

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::adapters::{parse_executor_type, CodeExecutor};
use crate::db::{Database, NewExecutionRecord};
use crate::executor_service::ExecEvent;
use crate::models::{ExecutorType, ParsedLogEntry};
use crate::task_manager::TaskManager;

use super::ExecutionResult;
use super::RunTodoExecutionRequest;
use super::log_capture::send_event;

/// 选择执行器类型。优先级：调用方显式 `req_executor` > todo 存储的 `todo_executor` > 默认值。
///
/// - 输入字符串无法解析为已知 executor 时记 warn 并退到下一优先级；
/// - 所有输入都解析失败时返回 `ExecutorType::default()`。
///
/// 不返回 `Result`，因为解析失败是预期内的"软"行为，调用方直接走默认分支即可。
/// 把 warn 日志集中在这里，避免顶级函数里出现散落的 `tracing::warn!` 解释为什么降级。
pub(crate) fn resolve_executor_type(req_executor: Option<&str>, todo_executor: Option<&str>) -> ExecutorType {
    // 显式请求 > 存储值 > 默认。三层 or_else 形成优先级链。
    req_executor
        .and_then(|exec| {
            parse_executor_type(exec).or_else(|| {
                tracing::warn!("Unknown explicit executor '{}', trying todo executor", exec);
                None
            })
        })
        .or_else(|| {
            todo_executor.and_then(|exec| {
                parse_executor_type(exec).or_else(|| {
                    tracing::warn!("Unknown todo executor '{}', falling back to default", exec);
                    None
                })
            })
        })
        .unwrap_or_default()
}

/// 统计 `todo_id` 下"真正在跑"的执行记录数，自动过滤掉僵尸记录。
///
/// 僵尸 = 数据库标记 running，但 `task_manager` 里查不到对应 task（多半是上一次
/// daemon 重启 / 异常退出遗留的脏数据）。这种记录不算"占用并发配额"，否则 daemon
/// 重启后所有 todo 都会被并发上限挡死。
pub(crate) async fn count_active_running_for_todo(
    task_manager: &TaskManager,
    db: &Database,
    todo_id: i64,
) -> Result<usize, ()> {
    // 一次性拿到 task_manager 的活跃 task 列表，后面用它来过滤 DB 的 running 记录。
    let running_tasks = task_manager.get_all_task_infos().await;
    let running_records = db
        .get_running_records_by_todo_id(todo_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get running execution records: {}", e);
        })?;
    Ok(running_records
        .iter()
        .filter(|r| {
            // 没有 task_id 的记录也算僵尸（早期 schema 不写这个字段），
            // 一律不计入并发占用。
            r.task_id
                .as_ref()
                .map(|task_id| running_tasks.iter().any(|t| t.task_id == *task_id))
                .unwrap_or(false)
        })
        .count())
}

/// 并发上限拒接：仅发 Finished 事件 + 移除 task。
/// 不调 `finish_todo_execution` —— todo 状态没变过，DB 里还是上一态。
pub(crate) async fn reject_concurrency_limit(
    task_manager: &TaskManager,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    running_count: usize,
    max_concurrent: u32,
) -> ExecutionResult {
    tracing::warn!(
        "Todo {} has {} execution(s) still running (limit: {}), rejecting",
        todo_id, running_count, max_concurrent
    );
    task_manager.remove(task_id).await;
    send_event(
        tx,
        ExecEvent::Finished {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: "".to_string(),
            success: false,
            result: Some(format!(
                "Todo {} has {} execution(s) still running (limit: {}). Please stop them first.",
                todo_id, running_count, max_concurrent
            )),
            feishu_bot_id: None,
            feishu_receive_id: None,
            workspace_id: None,
            duration_secs: 0,
            total_tokens: 0,
            // pre-spawn 阶段没有 trigger_type 上下文：阻塞/无 executor 都属于早期失败，
            // 不会被黑板逻辑用到，None 即可。
            trigger_type: None,
        },
    );
    ExecutionResult {
        task_id: task_id.to_string(),
        record_id: None,
    }
}

/// 没有可用 executor：发 Finished + finish_todo_execution（DB 回滚 todo 到非 running）。
pub(crate) async fn reject_no_executor(
    db: &Database,
    task_manager: &TaskManager,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor_type: ExecutorType,
) -> ExecutionResult {
    tracing::error!(
        "No executor available for type {:?} and no default registered",
        executor_type
    );
    let _ = db.finish_todo_execution(todo_id, false).await;
    send_event(
        tx,
        ExecEvent::Finished {
            task_id: task_id.to_string(),
            todo_id,
            todo_title: todo_title.to_string(),
            executor: executor_type.to_string(),
            success: false,
            result: Some("No executor available".to_string()),
            feishu_bot_id: None,
            feishu_receive_id: None,
            workspace_id: None,
            duration_secs: 0,
            total_tokens: 0,
            trigger_type: None,
        },
    );
    task_manager.remove(task_id).await;
    ExecutionResult {
        task_id: task_id.to_string(),
        record_id: None,
    }
}

/// 创建 execution_record 失败：finish_todo_execution + 移除 task。
/// 不发 Finished 事件 —— 没记录 id，前端无从关联；只清理内存 task。
pub(crate) async fn reject_create_record_failure(
    db: &Database,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    error: impl std::fmt::Display,
) -> ExecutionResult {
    tracing::error!("Failed to create execution record: {}", error);
    let _ = db.finish_todo_execution(todo_id, false).await;
    task_manager.remove(task_id).await;
    ExecutionResult {
        task_id: task_id.to_string(),
        record_id: None,
    }
}

/// `start_todo_execution` 失败：发 Output/Finished 事件 + 写 record 为 Failed 状态 +
/// finish_todo_execution + 移除 task。返回的 `record_id` 仍是 `Some`，
/// 让调用方可以基于 record_id 后续追查失败记录。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn reject_start_todo_failure(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &TaskManager,
    task_id: &str,
    todo_id: i64,
    todo_title: &str,
    executor_str: &str,
    record_id: i64,
    error: impl std::fmt::Display,
    workspace_id: Option<i64>,
) -> ExecutionResult {
    tracing::error!("Failed to start todo execution: {}", error);
    let entry = ParsedLogEntry::error(format!("Failed to start todo execution: {}", error));
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
            executor: executor_str.to_string(),
            success: false,
            result: Some("Failed to start execution".to_string()),
            feishu_bot_id: None,
            feishu_receive_id: None,
            workspace_id,
            duration_secs: 0,
            total_tokens: 0,
            trigger_type: None,
        },
    );
    let _ = db.finish_todo_execution(todo_id, false).await;
    let _ = db
        .update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: record_id,
            status: crate::models::ExecutionStatus::Failed.as_str(),
            remaining_logs: "[]",
            result: &format!("Failed to start todo execution: {}", error),
            usage: None,
            model: None,
            review_meta: None,
        })
        .await;
    task_manager.remove(task_id).await;
    ExecutionResult {
        task_id: task_id.to_string(),
        record_id: Some(record_id),
    }
}

// `send_event` helper 复用自 `super::log_capture::send_event` —— 同一 helper 在两个
// 模块里重复定义过（pre_spawn.rs 与 log_capture.rs），review CRITICAL #2 指出后，
// 统一从 log_capture 引用；这里保留注释以表明此处刻意不重新定义。

/// Stage 1 步骤 5 产物：executor 选择 + command 构造。
///
/// 决策顺序：显式 req_executor > todo.executor > registry default。命令构造
/// 用 `command_args_with_session` 处理 resume / 非 resume 分支。
pub(crate) struct SelectedExecutor {
    pub executor: Arc<dyn CodeExecutor>,
    pub command_args: Vec<String>,
    pub executable_path: String,
    pub executor_str: String,
    pub todo_workspace_path: Option<String>,
    // 仅在构造时赋值（= request.resume_session_id），当前无读取方：create_record_or_reject
    // 刻意不用它（见该函数注释的 resume 语义说明），保留字段以承载未来 executor 对
    // resume session 的直接消费，故 allow(dead_code) 而非删除。
    #[allow(dead_code)]
    pub session_id_for_executor: Option<String>,
}

/// Stage 1 步骤 5：选定 executor 并构造 command argv。
///
/// 失败时返回 reject_no_executor 的 ExecutionResult，调用方据此 short-circuit。
pub(crate) async fn select_executor_and_build_command(
    request: &RunTodoExecutionRequest,
    todo: &Option<crate::models::Todo>,
    message: &str,
) -> Result<SelectedExecutor, ExecutionResult> {
    let (todo_workspace_path, todo_executor) = extract_todo_executor_fields(todo);
    let executor_type =
        resolve_executor_type(request.req_executor.as_deref(), todo_executor.as_deref());
    let executor =
        resolve_executor_instance(request, todo, executor_type).await?;
    let executable_path = executor.executable_path().to_string();
    let command_args =
        build_executor_command_args(&executor, message, request.resume_session_id.as_deref());
    let executor_str = executor.executor_type().to_string();
    persist_executor_choice(&request.db, request.todo_id, &executor_str).await;
    Ok(SelectedExecutor {
        executor,
        command_args,
        executable_path,
        executor_str,
        todo_workspace_path,
        session_id_for_executor: request.resume_session_id.clone(),
    })
}

/// 从 `Option<Todo>` 提取 (workspace, executor_str) 二元组。
fn extract_todo_executor_fields(
    todo: &Option<crate::models::Todo>,
) -> (Option<String>, Option<String>) {
    (
        todo.as_ref().and_then(|t| t.workspace_path.clone()),
        todo.as_ref().and_then(|t| t.executor.clone()),
    )
}

/// 从 registry 取 executor：先按类型取，取不到 fallback 到 default，再失败就走 reject。
///
/// 每次执行前从 DB 重新读取 executor 路径，如果 DB 路径与缓存不一致则刷新 registry，
/// 确保用户在设置中修改路径后立即生效，无需重启。
async fn resolve_executor_instance(
    request: &RunTodoExecutionRequest,
    todo: &Option<crate::models::Todo>,
    executor_type: ExecutorType,
) -> Result<Arc<dyn CodeExecutor>, ExecutionResult> {
    // 每次执行前从 DB 重新读取 executor 配置，确保路径变更立即生效
    if let Ok(Some(config)) = request.db.get_executor_by_name(executor_type.as_str()).await {
        if config.enabled {
            let db_path = if config.path.is_empty() {
                executor_type.as_str()
            } else {
                &config.path
            };
            // 展开 ~ 为 home 目录，避免 tokio::process::Command 找不到文件
            let expanded_path = expand_tilde(db_path);

            // 检查缓存中的 executor 路径是否一致
            let need_refresh = match request.executor_registry.get(executor_type).await {
                Some(exec) => exec.executable_path() != expanded_path,
                None => true,
            };

            if need_refresh {
                request
                    .executor_registry
                    .register_by_name(executor_type.as_str(), &expanded_path)
                    .await;
            }
        }
    }

    if let Some(exec) = request.executor_registry.get(executor_type).await {
        return Ok(exec);
    }
    if let Some(exec) = request.executor_registry.get_default().await {
        return Ok(exec);
    }
    Err(reject_no_executor(
        &request.db,
        &request.task_manager,
        &request.tx,
        "",
        request.todo_id,
        todo.as_ref().map(|t| t.title.as_str()).unwrap_or(""),
        executor_type,
    )
    .await)
}

/// 展开路径中的 ~ 为用户 home 目录。
/// 如果 ~ 展开失败或无 ~ 前缀则返回原路径。
fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            let relative = path
                .trim_start_matches('~')
                .trim_start_matches(std::path::MAIN_SEPARATOR);
            return home.join(relative).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// 构造 argv：直接按 executor 规则拼。
fn build_executor_command_args(
    executor: &Arc<dyn CodeExecutor>,
    message: &str,
    resume_session_id: Option<&str>,
) -> Vec<String> {
    executor.command_args_with_session(
        message,
        resume_session_id,
        resume_session_id.is_some(),
    )
}

/// Update todo's executor to the one being used. 失败仅记日志，不阻断执行。
async fn persist_executor_choice(db: &Database, todo_id: i64, executor_str: &str) {
    if todo_id == 0 { return; } // 环节独立执行，不关联 todo
    if let Err(e) = db.update_todo_executor(todo_id, executor_str).await {
        tracing::error!("Failed to update todo executor: {}", e);
    }
}

/// 创建 execution_record 并把 stage 1 产物组装成 PreparedExecution。
///
/// record_id 是 stage 1 唯一需要数据库写的字段。失败时走 reject_create_record_failure
/// 把 todo 标回非 running 并清理 task，调用方拿到 ExecutionResult 直接返回给前端。
pub(crate) async fn create_run_execution_record(
    request: RunTodoExecutionRequest,
    task_state: super::types::TaskState,
    todo: Option<crate::models::Todo>,
    timeout_secs: u64,
    selected: SelectedExecutor,
) -> Result<super::types::PreparedExecution, ExecutionResult> {
    let command = build_command_string(&selected.executable_path, &selected.command_args);
    let record_id = match create_record_or_reject(&request, &task_state, &command, &selected).await {
        Ok(id) => id,
        Err(e) => return Err(e),
    };
    // todo_workspace_path 优先来自 todo；当 todo 不存在（loop 环节执行）时回退到 request.workspace_path，
    // 确保 worktree 创建失败后子进程 cwd 仍然是 loop 的 workspace。
    let todo_workspace_path = selected.todo_workspace_path.or_else(|| request.workspace_path.clone());
    Ok(super::types::PreparedExecution {
        request,
        task_guard: task_state.task_guard,
        cancel_rx: task_state.cancel_rx,
        task_id: task_state.task_id,
        command_args: selected.command_args,
        executable_path: selected.executable_path,
        executor: selected.executor,
        executor_str: selected.executor_str,
        record_id,
        todo,
        todo_workspace_path,
        timeout_secs,
    })
}

/// 拼出 execution_records.command 字段：executable + space + args.join(" ")
fn build_command_string(executable_path: &str, command_args: &[String]) -> String {
    format!("{} {}", executable_path, command_args.join(" "))
}

/// 创建 execution_record，失败时走 reject 路径返回 ExecutionResult。
///
/// DB 记录的 session_id 必须与 request.resume_session_id 保持一致：
///   - 首次执行（None）：写 None，真实 sid 由 stdout reader 从 Claude Code 的 system 事件解析后回写。
///   - resume（Some(sid)）：写原 sid。
///
/// 不能用 selected.session_id_for_executor——首次执行时它是后台合成的 UUID，
/// 直接写进 DB 会让 feishu_listener::decide_resume_session 拿合成 sid 触发 resume，
/// 把"Invalid session ID"错误从首次执行搬到第二次。
async fn create_record_or_reject(
    request: &RunTodoExecutionRequest,
    task_state: &super::types::TaskState,
    command: &str,
    selected: &SelectedExecutor,
) -> Result<i64, ExecutionResult> {
    match request
        .db
        .create_execution_record(NewExecutionRecord {
            todo_id: if request.todo_id == 0 { None } else { Some(request.todo_id) },
            command,
            executor: &selected.executor_str,
            trigger_type: &request.trigger_type,
            task_id: &task_state.task_id,
            session_id: request.resume_session_id.as_deref(),
            resume_message: request.resume_message.as_deref(),
            source_todo_id: request.source_todo_id,
            source_todo_title: request.source_todo_title.as_deref(),
            loop_step_execution_id: request.loop_step_execution_id,
            step_id: request.step_id,
        })
        .await
    {
        Ok(id) => Ok(id),
        Err(e) => Err(reject_create_record_failure(
            &request.db,
            &request.task_manager,
            &task_state.task_id,
            request.todo_id,
            e,
        )
        .await),
    }
}

/// Stage 1 步骤 1：在 `request` 上做 message 占位符替换，并返回替换后的 message。
///
/// 占位符替换只在编排阶段有效——executor 看到的 message 与 stage 2 写入
/// execution_record.command 的字符串一致。
pub(crate) fn substitute_message_placeholders(
    request: &super::RunTodoExecutionRequest,
) -> super::types::SubstitutedContext {
    let message = request
        .params
        .as_ref()
        .map(|params| crate::models::replace_placeholders(&request.message, params))
        .unwrap_or_else(|| request.message.clone());
    super::types::SubstitutedContext { message }
}

/// Stage 1 步骤 4a：如果 todo 已存在，校验并发限制。todo 为 None 时跳过。
pub(crate) async fn enforce_concurrency_limit(
    request: &super::RunTodoExecutionRequest,
    todo: Option<crate::models::Todo>,
    max_concurrent: u32,
    task_id: &str,
) -> Result<Option<crate::models::Todo>, super::ExecutionResult> {
    let Some(t) = todo else {
        return Ok(None);
    };
    let running_count =
        match count_active_running_for_todo(&request.task_manager, &request.db, request.todo_id)
            .await
        {
            Ok(n) => n,
            Err(()) => {
                return Err(super::ExecutionResult {
                    task_id: task_id.to_string(),
                    record_id: None,
                })
            }
        };
    if running_count >= max_concurrent as usize {
        return Err(reject_concurrency_limit(
            &request.task_manager,
            &request.tx,
            task_id,
            request.todo_id,
            &t.title,
            running_count,
            max_concurrent,
        )
        .await);
    }
    Ok(Some(t))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_executor_type_priority_chain() {
        // 显式有效值优先于 todo 存储值。
        assert_eq!(
            resolve_executor_type(Some("codex"), Some("claudecode")),
            ExecutorType::Codex
        );
        // 显式无效时降级到 todo 存储值。
        assert_eq!(
            resolve_executor_type(Some("bogus"), Some("hermes")),
            ExecutorType::Hermes
        );
        // todo 存储值也无效时回到默认。
        assert_eq!(
            resolve_executor_type(Some("bogus"), Some("also_bogus")),
            ExecutorType::default()
        );
        // 两者都缺失时回到默认。
        assert_eq!(resolve_executor_type(None, None), ExecutorType::default());
        // 只有 todo 存储值有效时使用它。
        assert_eq!(
            resolve_executor_type(None, Some("pi")),
            ExecutorType::Pi
        );
        // 只有显式值有效时使用它。
        assert_eq!(
            resolve_executor_type(Some("kimi"), None),
            ExecutorType::Kimi
        );
    }
}