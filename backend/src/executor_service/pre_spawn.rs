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
            feishu_receive_id_type: None,
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
            feishu_receive_id_type: None,
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
            feishu_receive_id_type: None,
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
    let (executor, executor_config) =
        resolve_executor_instance(request, todo, executor_type).await?;
    // 解析三层优先级模型并同步注入：req_model > todo.model > executor.default_model > None。
    // 关键：set_exec_model 与紧随的 build_executor_command_args 之间不能有 .await——
    // registry 的 executor 实例是 per-type 单例（Arc 共享），若两个 todo 并发用同一执行器，
    // 中途 await 会让另一执行的 set_exec_model 覆盖本执行的 model。同步注入+构建保证原子。
    // executor_config 复用 resolve_executor_instance 已查到的配置（含 default_model），避免二次查询。
    let todo_model = todo.as_ref().and_then(|t| t.model.clone());
    let default_model = executor_config.and_then(|c| c.default_model);
    let model = resolve_exec_model(
        request.req_model.as_deref(),
        todo_model.as_deref(),
        default_model.as_deref(),
    );
    executor.set_exec_model(model);
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
) -> Result<(Arc<dyn CodeExecutor>, Option<crate::models::ExecutorConfig>), ExecutionResult> {
    // 每次执行前从 DB 重新读取 executor 配置，确保路径变更立即生效。
    // config 仅当 enabled 且 registry 返回 executor 时一并返回给调用方，
    // 用于读取 default_model，避免紧随其后的 model 解析再次查询。
    // disabled 的执行器不返回 config，其 default_model 不应影响后续回退路径的模型选择。
    let mut config: Option<crate::models::ExecutorConfig> = None;
    if let Ok(Some(cfg)) = request.db.get_executor_by_name(executor_type.as_str()).await {
        if cfg.enabled {
            let db_path = if cfg.path.is_empty() {
                executor_type.as_str()
            } else {
                &cfg.path
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
            // 配置仅在 enabled 且成功获取 executor 后返回。
            // disabled 时不设 config，让调用方的 model 解析只看到 default fallback 的配置。
            config = Some(cfg);
        }
    }

    if let Some(exec) = request.executor_registry.get(executor_type).await {
        return Ok((exec, config));
    }
    if let Some(exec) = request.executor_registry.get_default().await {
        return Ok((exec, config));
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

/// 三层优先级解析最终使用的模型：
/// `req_model`(显式指定) > `todo_model`(任务级) > `executor_default_model`(执行器级) > None。
/// 空串或全空格视为未指定并继续回退，避免把空白字符串当模型名传给执行器。
/// 非空模型值也会 trim 前后空格，容忍用户误输入。
/// None 表示不传 --model，由执行器配置文件决定（保持升级前行为）。
fn resolve_exec_model(
    req_model: Option<&str>,
    todo_model: Option<&str>,
    executor_default_model: Option<&str>,
) -> Option<String> {
    // 每层先 map trim 去掉前后空格，再过滤空串、回退到下一层。
    // 不能先 or 再统一 filter——Some("") 是 Some，会阻断 or 链，让空串无法回退到下一层。
    // trim 在 filter 之前：既要过滤 "   " 这样的纯空格输入，也要把 "  gpt-4  " 归一化为 "gpt-4"。
    req_model
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| todo_model.map(str::trim).filter(|s| !s.is_empty()))
        .or_else(|| executor_default_model.map(str::trim).filter(|s| !s.is_empty()))
        .map(|s| s.to_string())
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

/// 注入工作空间级共识 prompt（需求 022）。
///
/// 读取 `workspace_settings.system_prompt`，若非空则拼接到 message 最前面，
/// 用 Markdown 水平分割线 `\n---\n` 与原 message 分隔。这样 workspace 下的
/// 所有 todo 执行时都共享同一份前置上下文（产物目录、认证信息、基本文件路径
/// 等），达成 workspace 维度的共享、遵守、共识。
///
/// 降级策略（任一命中即静默返回原 message，不阻断 todo 执行）：
/// - `workspace_id` 为 None
/// - `get_workspace_settings` DB 查询失败
/// - workspace_settings 行不存在
/// - system_prompt 为 None 或空串
///
/// 安全：本函数不在日志中打印 prompt 内容（可能含认证信息），仅在 DB 查询
/// 失败时 warn 一句不含 prompt 的消息。
pub(crate) async fn inject_workspace_prompt(
    db: &Database,
    workspace_id: Option<i64>,
    message: &str,
) -> String {
    // workspace_id 缺失（如独立环节执行）→ 跳过注入
    let Some(wid) = workspace_id else {
        return message.to_string();
    };
    // 读取失败静默回退，不让 workspace settings 故障阻断 todo 执行
    let settings = match crate::db::workspace_setting::get_workspace_settings(db, wid).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "读取 workspace {} settings 失败，跳过 prompt 注入: {}",
                wid,
                e
            );
            return message.to_string();
        }
    };
    // 行不存在 / system_prompt 为 None 或空串 → 原样返回
    let Some(prompt) = settings.and_then(|s| s.system_prompt) else {
        return message.to_string();
    };
    if prompt.is_empty() {
        return message.to_string();
    }
    // 拼接：workspace 共识 + 分隔线 + 原任务
    format!("{}\n---\n{}", prompt, message)
}

/// 注入专家上下文：如果 todo 关联了专家，将专家角色定义和技能信息拼接到 message 前面。
///
/// 失败时静默返回原 message，不阻断执行——专家 prompt 注入是增强项，
/// 不应该让专家索引读取失败导致整个 todo 执行失败。
///
/// 注入格式：
/// ```text
/// # 专家角色定义
/// {agent_md_content}
///
/// # 可用技能
/// {skill_name}: {skill_description}
/// ...
///
/// # 任务
/// {original_message}
/// ```
pub(crate) async fn inject_expert_context(
    request: &super::RunTodoExecutionRequest,
    todo: &Option<crate::models::Todo>,
    message: &str,
) -> String {
    // 任一前置条件缺失都静默回退到原 message——专家注入是增强项，不应阻断执行。
    let Some(expert_name) = todo.as_ref().and_then(|t| t.expert_name.as_deref()) else {
        return message.to_string();
    };
    let Some(expert_manager) = &request.expert_manager else {
        return message.to_string();
    };
    let Some(metadata) = expert_manager.get_expert_by_name(expert_name) else {
        tracing::warn!("未找到专家 '{}'，跳过专家上下文注入", expert_name);
        return message.to_string();
    };
    // 解析主理 agent：team 用 lead_agent、agent 用 agent_name（resolve_agent_name 统一），
    // 并按 (expert_name, agent_name) 复合键查找，避免不同专家同名 agent 互窜。
    let Some(agent_name) = metadata.resolve_agent_name() else {
        tracing::warn!(
            "专家 '{}' 没有可用 agent（agent_name/lead_agent 都为空）",
            expert_name
        );
        return message.to_string();
    };
    let Ok(agent_md) = expert_manager.get_agent_md_content(expert_name, agent_name) else {
        tracing::warn!(
            "未找到专家 '{}' 的 Agent '{}' MD 内容，跳过注入",
            expert_name,
            agent_name
        );
        return message.to_string();
    };
    let skills_text = build_expert_skills_text(expert_manager, expert_name);
    build_expert_prompt(&agent_md, &skills_text, message)
}

/// 拼接专家技能列表文本：复用 loader 模块的 build_skills_context，支持中文描述优先。
///
/// 抽出来单独成函数是为了让 `inject_expert_context` 保持在 30 行内。
fn build_expert_skills_text(
    expert_manager: &crate::expert::ExpertIndexManager,
    expert_name: &str,
) -> String {
    // 复用 loader::build_skills_context，它已实现中文优先回退逻辑。
    crate::expert::build_skills_context(&expert_manager.get_expert_skills(expert_name))
}

/// 把 Agent MD、技能列表、原 message 拼成最终 prompt。
///
/// 三段式结构：专家角色定义 → 可用技能（由 build_skills_context 生成） → 任务。
/// skills_text 为空时不添加技能段落，避免无谓标题。
fn build_expert_prompt(agent_md: &str, skills_text: &str, original_message: &str) -> String {
    if skills_text.is_empty() {
        format!("# 专家角色定义\n{}\n\n# 任务\n{}", agent_md, original_message)
    } else {
        format!(
            "# 专家角色定义\n{}\n\n{}\n\n# 任务\n{}",
            agent_md, skills_text, original_message
        )
    }
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

    /// 三层优先级：req_model > todo_model > executor_default_model。
    #[test]
    fn test_resolve_exec_model_prefers_req_model() {
        assert_eq!(
            resolve_exec_model(Some("opus"), Some("sonnet"), Some("haiku")),
            Some("opus".to_string())
        );
    }

    #[test]
    fn test_resolve_exec_model_falls_back_to_todo_model() {
        assert_eq!(
            resolve_exec_model(None, Some("sonnet"), Some("haiku")),
            Some("sonnet".to_string())
        );
    }

    #[test]
    fn test_resolve_exec_model_falls_back_to_executor_default() {
        assert_eq!(
            resolve_exec_model(None, None, Some("haiku")),
            Some("haiku".to_string())
        );
    }

    /// 三层全空 → None（不传 --model，向后兼容）。
    #[test]
    fn test_resolve_exec_model_none_when_all_absent() {
        assert_eq!(resolve_exec_model(None, None, None), None);
    }

    /// 空串视为未指定，继续向下一层回退；避免把空字符串当模型名传给执行器。
    #[test]
    fn test_resolve_exec_model_treats_empty_string_as_absent() {
        // req 为空串 → 回退到 todo_model。
        assert_eq!(
            resolve_exec_model(Some(""), Some("sonnet"), None),
            Some("sonnet".to_string())
        );
        // 三层全为空串 → None。
        assert_eq!(resolve_exec_model(Some(""), Some(""), Some("")), None);
    }

    /// 全空格视为未指定，trim 后回退；同时验证 "  gpt-4  " 会被 trim 为 "gpt-4"。
    #[test]
    fn test_resolve_exec_model_trims_whitespace_only() {
        // req 为全空格 → 回退到 todo_model。
        assert_eq!(
            resolve_exec_model(Some("   "), Some("sonnet"), None),
            Some("sonnet".to_string())
        );
        // 三层全空格 → None。
        assert_eq!(
            resolve_exec_model(Some("  "), Some(" "), Some("   ")),
            None
        );
        // req 为 "  gpt-4  "  → trim 后保留 "gpt-4"，不向 todo 回退。
        assert_eq!(
            resolve_exec_model(Some("  gpt-4  "), Some("sonnet"), None),
            Some("gpt-4".to_string())
        );
    }

    /// `build_expert_prompt` 把 Agent MD、技能列表、原 message 拼成三段式 prompt。
    /// 有技能时保留技能段落（由 build_skills_context 生成，含标题行）。
    #[test]
    fn test_build_expert_prompt_three_sections() {
        let agent_md = "你是一个 Rust 专家";
        let skills = "## 可用技能\n你可以使用以下技能来辅助完成任务：\n- **code-review**: 代码评审技能\n";
        let original = "请帮我写一个函数";
        let result = build_expert_prompt(agent_md, skills, original);
        // 三段标题按顺序出现，且原 message 在末尾
        assert!(result.contains("# 专家角色定义\n你是一个 Rust 专家"));
        assert!(result.contains("## 可用技能"));
        assert!(result.contains("# 任务\n请帮我写一个函数"));
    }

    /// `build_expert_prompt` 技能列表为空时省略技能段落。
    #[test]
    fn test_build_expert_prompt_empty_skills_omits_section() {
        let result = build_expert_prompt("agent", "", "do something");
        // 空技能时不出现技能段落，直接从角色定义跳到任务
        assert!(!result.contains("可用技能"));
        assert!(result.contains("# 专家角色定义\nagent"));
        assert!(result.contains("# 任务\ndo something"));
    }

    /// `build_expert_skills_text` 复用 loader::build_skills_context，
    /// 返回 Markdown 格式的技能列表（含标题行和项目符号）。
    #[test]
    fn test_build_expert_skills_text_formats_each_skill() {
        use crate::expert::{ExpertIndexManager, SkillMetadata};
        let manager = ExpertIndexManager::new();
        // 准备两个 skill 的元数据并更新到索引
        let skills = vec![
            SkillMetadata {
                skill_name: "code-review".to_string(),
                skill_dir: "/tmp/skills/code-review".to_string(),
                skill_md_path: "/tmp/skills/code-review/SKILL.md".to_string(),
                yaml_name: None,
                yaml_description: Some("代码评审".to_string()),
                yaml_description_zh: None,
                yaml_description_en: None,
                yaml_version: None,
                yaml_allowed_tools: vec![],
                yaml_emoji: None,
            },
            SkillMetadata {
                skill_name: "test-gen".to_string(),
                skill_dir: "/tmp/skills/test-gen".to_string(),
                skill_md_path: "/tmp/skills/test-gen/SKILL.md".to_string(),
                yaml_name: None,
                // description 为 None 时回退到 "(无描述)"
                yaml_description: None,
                yaml_description_zh: None,
                yaml_description_en: None,
                yaml_version: None,
                yaml_allowed_tools: vec![],
                yaml_emoji: None,
            },
        ];
        // 借用一个最小 ExpertMetadata 把 skill 绑到 "test-expert"
        let expert = make_minimal_expert_metadata("test-expert", Some("test-agent"), None);
        manager.update_index(&expert, &[], &skills);
        let text = build_expert_skills_text(&manager, "test-expert");
        // build_skills_context 输出 Markdown 格式：标题 + 项目符号列表
        assert!(text.contains("## 可用技能"));
        // build_skills_context 将技能名渲染为 Markdown 链接指向 SKILL.md 路径
        assert!(text.contains("- **[code-review]("));
        assert!(text.contains("**: 代码评审"));
        assert!(text.contains("- **[test-gen]("));
        assert!(text.contains("**: (无描述)"));
    }

    /// `build_expert_skills_text` 查询不存在的专家时返回空串（get_expert_skills 容错）。
    #[test]
    fn test_build_expert_skills_text_unknown_expert_returns_empty() {
        use crate::expert::ExpertIndexManager;
        let manager = ExpertIndexManager::new();
        let text = build_expert_skills_text(&manager, "non-existent-expert");
        assert!(text.is_empty());
    }

    /// `inject_expert_context` 在 todo 为 None 时直接返回原 message。
    #[tokio::test]
    async fn test_inject_expert_context_no_todo_returns_original() {
        let request = make_test_request(None).await;
        let original = "请帮我写代码";
        let result = inject_expert_context(&request, &None, original).await;
        assert_eq!(result, original);
    }

    /// `inject_expert_context` 在 todo 有 expert_name 但 request.expert_manager
    /// 为 None 时（系统内部 todo 路径）返回原 message。
    #[tokio::test]
    async fn test_inject_expert_context_no_manager_returns_original() {
        let request = make_test_request(None).await;
        let todo = make_todo_with_expert(Some("rust-expert"));
        let original = "请帮我写代码";
        let result = inject_expert_context(&request, &Some(todo), original).await;
        assert_eq!(result, original);
    }

    /// `inject_expert_context` 在 todo 没有 expert_name 时返回原 message。
    #[tokio::test]
    async fn test_inject_expert_context_no_expert_name_returns_original() {
        let manager = Arc::new(crate::expert::ExpertIndexManager::new());
        let request = make_test_request(Some(manager)).await;
        let todo = make_todo_with_expert(None);
        let original = "请帮我写代码";
        let result = inject_expert_context(&request, &Some(todo), original).await;
        assert_eq!(result, original);
    }

    /// `inject_expert_context` 在 expert_manager 中找不到对应专家时返回原 message。
    #[tokio::test]
    async fn test_inject_expert_context_unknown_expert_returns_original() {
        let manager = Arc::new(crate::expert::ExpertIndexManager::new());
        let request = make_test_request(Some(manager)).await;
        let todo = make_todo_with_expert(Some("non-existent"));
        let original = "请帮我写代码";
        let result = inject_expert_context(&request, &Some(todo), original).await;
        assert_eq!(result, original);
    }

    /// `inject_expert_context` 在专家存在但 Agent MD 文件无法读取时返回原 message。
    /// 通过指向不存在的 md_file_path 模拟读取失败。
    #[tokio::test]
    async fn test_inject_expert_context_agent_md_unreadable_returns_original() {
        let manager = Arc::new(crate::expert::ExpertIndexManager::new());
        // 注册一个 agent，但 md_file_path 指向不存在的文件
        let agent_file = crate::expert::AgentFileMetadata {
            agent_name: "test-agent".to_string(),
            md_file_path: "/non/existent/path/agent.md".to_string(),
            yaml_name: None,
            yaml_description: None,
            yaml_color: None,
            yaml_emoji: None,
            yaml_vibe: None,
        };
        let expert = make_minimal_expert_metadata("test-expert", Some("test-agent"), None);
        manager.update_index(&expert, &[agent_file], &[]);
        let request = make_test_request(Some(manager)).await;
        let todo = make_todo_with_expert(Some("test-expert"));
        let original = "请帮我写代码";
        let result = inject_expert_context(&request, &Some(todo), original).await;
        assert_eq!(result, original);
    }

    /// `inject_expert_context` 正常路径：专家存在 + Agent MD 可读 + 有技能
    /// → 返回带三段式结构的 prompt，原 message 被拼到末尾。
    #[tokio::test]
    async fn test_inject_expert_context_happy_path_injects_prompt() {
        use std::io::Write;
        // 准备临时 Agent MD 文件
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push(format!(
            "ntd_test_agent_md_{}.md",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut f = std::fs::File::create(&tmp_path).unwrap();
        // 写入角色定义内容
        writeln!(f, "你是一个 Rust 专家").unwrap();
        let md_path = tmp_path.to_string_lossy().to_string();

        let manager = Arc::new(crate::expert::ExpertIndexManager::new());
        let agent_file = crate::expert::AgentFileMetadata {
            agent_name: "rust-agent".to_string(),
            md_file_path: md_path.clone(),
            yaml_name: None,
            yaml_description: None,
            yaml_color: None,
            yaml_emoji: None,
            yaml_vibe: None,
        };
        let skill = crate::expert::SkillMetadata {
            skill_name: "code-review".to_string(),
            skill_dir: "/tmp".to_string(),
            skill_md_path: "/tmp/SKILL.md".to_string(),
            yaml_name: None,
            yaml_description: Some("代码评审".to_string()),
            yaml_description_zh: None,
            yaml_description_en: None,
            yaml_version: None,
            yaml_allowed_tools: vec![],
            yaml_emoji: None,
        };
        let expert = make_minimal_expert_metadata("rust-expert", Some("rust-agent"), None);
        manager.update_index(&expert, &[agent_file], &[skill]);

        let request = make_test_request(Some(manager)).await;
        let todo = make_todo_with_expert(Some("rust-expert"));
        let original = "请帮我写代码";
        let result = inject_expert_context(&request, &Some(todo), original).await;
        // 验证三段式结构都存在（build_skills_context 生成 Markdown 格式技能列表）
        assert!(result.starts_with("# 专家角色定义\n"));
        assert!(result.contains("你是一个 Rust 专家"));
        assert!(result.contains("## 可用技能"));
        // build_skills_context 将技能名渲染为 Markdown 链接指向 SKILL.md 路径
        assert!(result.contains("- **[code-review]("));
        assert!(result.contains("**: 代码评审"));
        assert!(result.contains("# 任务\n请帮我写代码"));
        // 清理临时文件
        let _ = std::fs::remove_file(&md_path);
    }

    /// `inject_expert_context` 对 team 类型专家（lead_agent 有值、agent_name 为 None）也能注入。
    /// 回归测试：修复前 inject 只读 agent_name，team 专家因 agent_name 为 None 被静默跳过。
    /// 注意：inject 只依据 resolve_agent_name（lead_agent 优先）解析主理 agent，不检查 expert_type，
    /// 因此这里用默认的 Agent 类型即可复现 team 的字段分布（agent_name=None + lead_agent=Some）。
    #[tokio::test]
    async fn test_inject_expert_context_team_expert_uses_lead_agent() {
        use std::io::Write;
        // 准备 lead agent 的 MD 文件：team 的主理 agent 是 lead_agent，不是 agent_name
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push(format!(
            "ntd_test_team_lead_md_{}.md",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut f = std::fs::File::create(&tmp_path).unwrap();
        writeln!(f, "你是团队负责人").unwrap();
        let md_path = tmp_path.to_string_lossy().to_string();

        let manager = Arc::new(crate::expert::ExpertIndexManager::new());
        // 把 lead agent 注册到索引（agent_name 字段作为 key 指向 lead 的 MD 文件）
        let agent_file = crate::expert::AgentFileMetadata {
            agent_name: "team-lead".to_string(),
            md_file_path: md_path.clone(),
            yaml_name: None,
            yaml_description: None,
            yaml_color: None,
            yaml_emoji: None,
            yaml_vibe: None,
        };
        // team 字段分布：agent_name=None、lead_agent=Some —— 修复前的失败场景
        let expert = make_minimal_expert_metadata("my-team", None, Some("team-lead"));
        manager.update_index(&expert, &[agent_file], &[]);

        let request = make_test_request(Some(manager)).await;
        let todo = make_todo_with_expert(Some("my-team"));
        let original = "请帮我写代码";
        let result = inject_expert_context(&request, &Some(todo), original).await;
        // 修复后应注入 lead_agent 的角色定义，而非静默返回原 message
        assert!(
            result.starts_with("# 专家角色定义\n"),
            "team 专家应注入 lead_agent 的 MD，实际结果: {}",
            result
        );
        assert!(result.contains("你是团队负责人"));
        assert!(result.contains("# 任务\n请帮我写代码"));
        // 清理临时文件
        let _ = std::fs::remove_file(&md_path);
    }

    /// 构造最小可用的 ExpertMetadata 供测试使用。
    /// 只填必要字段，其余用空值/None，避免每个测试都重复 22 个字段。
    fn make_minimal_expert_metadata(
        name: &str,
        agent_name: Option<&str>,
        lead_agent: Option<&str>,
    ) -> crate::expert::ExpertMetadata {
        use crate::expert::{ExpertMetadata, ExpertSource, ExpertType};
        ExpertMetadata {
            name: name.to_string(),
            expert_type: ExpertType::Agent,
            version: "0.0.1-test".to_string(),
            source: ExpertSource::System,
            display_name_zh: None,
            display_name_en: None,
            profession_zh: None,
            profession_en: None,
            description_zh: None,
            description_en: None,
            avatar_path: None,
            category_id: None,
            definition_dir: "/tmp".to_string(),
            plugin_json_path: "/tmp/plugin.json".to_string(),
            agent_name: agent_name.map(|s| s.to_string()),
            lead_agent: lead_agent.map(|s| s.to_string()),
            member_agents: vec![],
            members: vec![],
            skills: vec![],
            default_init_prompt_zh: None,
            default_init_prompt_en: None,
            tags: vec![],
            loaded_at: "test".to_string(),
            is_active: true,
        }
    }

    /// 构造一个带 expert_name 的 Todo（仅填必要字段）。
    fn make_todo_with_expert(expert_name: Option<&str>) -> crate::models::Todo {
        use crate::models::{Todo, TodoStatus};
        Todo {
            id: 1,
            title: "测试 todo".to_string(),
            prompt: "test".to_string(),
            status: TodoStatus::Pending,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            tag_ids: vec![],
            executor: None,
            model: None,
            scheduler_enabled: false,
            scheduler_config: None,
            scheduler_timezone: None,
            scheduler_next_run_at: None,
            task_id: None,
            workspace_path: None,
            workspace_id: None,
            webhook_enabled: false,
            acceptance_criteria: None,
            todo_type: 0,
            parent_todo_id: None,
            review_template_id: None,
            auto_review_enabled: false,
            action_type: None,
            action_key: None,
            archived_at: None,
            expert_name: expert_name.map(|s| s.to_string()),
        }
    }

    /// 构造一个测试用的 RunTodoExecutionRequest，expert_manager 可选。
    /// 其他字段用最小占位值——inject_expert_context 只关心 expert_manager 字段。
    /// 注意：调用方必须在 tokio runtime 上下文中（用 #[tokio::test]）。
    async fn make_test_request(
        expert_manager: Option<Arc<crate::expert::ExpertIndexManager>>,
    ) -> RunTodoExecutionRequest {
        use crate::adapters::ExecutorRegistry;
        use crate::config::Config;
        use crate::db::Database;
        use crate::task_manager::TaskManager;
        use std::sync::RwLock;
        // 用内存 DB 占位——inject_expert_context 不会触碰 DB，但 struct 字段必须 owned
        let db = Arc::new(Database::new(":memory:").await.unwrap());
        RunTodoExecutionRequest {
            db,
            executor_registry: Arc::new(ExecutorRegistry::default()),
            tx: broadcast::channel(1).0,
            task_manager: Arc::new(TaskManager::default()),
            config: Arc::new(RwLock::new(Config::default())),
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
            expert_manager,
        }
    }

    /// 注入函数：workspace_id 为 None 时原样返回（独立环节执行场景）。
    #[tokio::test]
    async fn test_inject_workspace_prompt_none_workspace_id() {
        let db = Arc::new(Database::new(":memory:").await.unwrap());
        let result = inject_workspace_prompt(&db, None, "做某事").await;
        assert_eq!(result, "做某事");
    }

    /// 注入函数：workspace_settings 行不存在时原样返回。
    #[tokio::test]
    async fn test_inject_workspace_prompt_no_settings() {
        let db = Arc::new(Database::new(":memory:").await.unwrap());
        // workspace 999 未配置 settings
        let result = inject_workspace_prompt(&db, Some(999), "做某事").await;
        assert_eq!(result, "做某事");
    }

    /// 注入函数：system_prompt 为空串时跳过拼接。
    #[tokio::test]
    async fn test_inject_workspace_prompt_empty_prompt() {
        let db = Arc::new(Database::new(":memory:").await.unwrap());
        // 写入空 prompt
        crate::db::workspace_setting::upsert_workspace_settings(
            &db, 1, None, None, None, None, Some(String::new()),
        )
        .await
        .unwrap();
        let result = inject_workspace_prompt(&db, Some(1), "做某事").await;
        assert_eq!(result, "做某事");
    }

    /// 注入函数：system_prompt 为 None 时跳过拼接。
    #[tokio::test]
    async fn test_inject_workspace_prompt_null_prompt() {
        let db = Arc::new(Database::new(":memory:").await.unwrap());
        // 创建时显式传 None → system_prompt 列为 NULL
        crate::db::workspace_setting::upsert_workspace_settings(
            &db, 1, Some("todo".to_string()), None, None, None, None,
        )
        .await
        .unwrap();
        let result = inject_workspace_prompt(&db, Some(1), "做某事").await;
        assert_eq!(result, "做某事");
    }

    /// 注入函数：正常场景，workspace 共识 prompt 拼到 message 前。
    #[tokio::test]
    async fn test_inject_workspace_prompt_normal_inject() {
        let db = Arc::new(Database::new(":memory:").await.unwrap());
        let prompt = "## 工作空间共识\n- 产物目录：./target";
        crate::db::workspace_setting::upsert_workspace_settings(
            &db, 1, None, None, None, None, Some(prompt.to_string()),
        )
        .await
        .unwrap();
        let result = inject_workspace_prompt(&db, Some(1), "做某事").await;
        assert_eq!(result, format!("{}\n---\n做某事", prompt));
    }
}