//! 黑板（Blackboard）Service 层。
//!
//! 纯文件存储方案：LLM 直接编辑 wiki 目录下的 Markdown 文件。
//!
//! 目录结构：
//! ~/.ntd/workspace/<workspace_id>/wiki/
//! ├── index.md      # 目录页（自动生成）
//! ├── log.md        # 执行日志（追加式）
//! └── topics/
//!     ├── auth-module.md
//!     └── performance.md
//!
//! 流程（单次 LLM 调用）：
//! 1. LLM 读取 wiki/ 目录下所有 topic 文件
//! 2. LLM 分析执行记录结论（调用 ntd todo execution get <id>）
//! 3. LLM 直接编辑文件：create 新文件 / update 现有文件
//! 4. 后端后处理：生成 index.md、追加 log.md

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::adapters::ExecutorRegistry;
use crate::config::Config;
use crate::db::Database;
use crate::executor_service::{ExecEvent, RunTodoExecutionRequest};
use crate::handlers::AppError;
use crate::task_manager::TaskManager;
use crate::wiki::{init_wiki_dir, list_topics, append_log_entry};

/// 当前实现的固定 trigger_type：在 Finished 钩子中用于识别"自身"避免递归触发。
const TRIGGER_TYPE_BLACKBOARD: &str = "blackboard";

/// Wiki 模式的 action_key（单阶段，直接编辑文件）。
const ACTION_KEY_WIKI: &str = "wiki-update";

/// 查找或创建黑板 Wiki Todo（不更新 prompt）。
///
/// action_type="blackboard", action_key="wiki-update"。
/// 每个工作空间独立维护自己的 Wiki 更新 Todo。
///
/// prompt 的同步由 `apply_wiki_prompt_to_todo` 负责，与本函数解耦：
/// - 首次创建：用内置默认 prompt 兜底
/// - 已存在：原样返回 id，不触碰 prompt 字段
async fn find_or_create_wiki_todo(
    db: &Database,
    workspace_id: i64,
) -> Result<i64, AppError> {
    // 按 action_type + action_key + workspace_id 查找已有的 Todo
    if let Some(todo) = db
        .get_todo_by_action_type_and_key_and_workspace("blackboard", ACTION_KEY_WIKI, workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        return Ok(todo.id);
    }

    // 未找到，自动创建：用内置默认 prompt 兜底，后续由配置同步覆盖
    let dir = db
        .get_project_directory_by_id(workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", workspace_id)))?;

    let title = format!("Blackboard Wiki: workspace_{}", workspace_id);
    let prompt = build_wiki_prompt();

    let todo_id = db
        .create_todo_with_extras(
            &title,
            &prompt,
            None,
            None,
            false,
            workspace_id,
            &dir.path,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    db.update_todo_full(crate::db::TodoUpdate {
        id: todo_id,
        title: &title,
        prompt: &prompt,
        status: crate::models::TodoStatus::Pending,
        executor: None,
        scheduler_enabled: None,
        scheduler_config: None,
        scheduler_timezone: None,
        workspace_id: None,
        webhook_enabled: None,
        acceptance_criteria: None,
        auto_review_enabled: None,
        action_type: Some("blackboard"),
        action_key: Some(ACTION_KEY_WIKI),
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(todo_id)
}

/// 把数据库中的 wiki_prompt 配置同步到该 workspace 已存在的 Wiki Todo。
///
/// 规则：
/// - 配置非空：用配置值覆盖 todo.prompt
/// - 配置为空：用内置默认 prompt 覆盖 todo.prompt
/// - todo 不存在：跳过（执行时再创建）
///
/// 由配置保存接口调用，保证用户在前端改完提示词后，下次执行的 todo 就是最新值。
pub async fn apply_wiki_prompt_to_todo(
    db: &Database,
    workspace_id: i64,
) -> Result<(), AppError> {
    // 解析当前生效的 prompt：配置非空用配置值，否则用内置默认
    let prompt_template = resolve_effective_wiki_prompt(db, workspace_id).await?;
    update_todo_prompt_if_exists(db, &prompt_template, workspace_id).await
}

/// 解析当前生效的 wiki prompt：数据库非空用配置值，否则用内置默认。
async fn resolve_effective_wiki_prompt(
    db: &Database,
    workspace_id: i64,
) -> Result<String, AppError> {
    let prompt = db
        .get_blackboard_config(workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .and_then(|cfg| {
            if cfg.wiki_prompt.trim().is_empty() {
                None
            } else {
                Some(cfg.wiki_prompt)
            }
        });
    Ok(prompt.unwrap_or_else(build_wiki_prompt))
}

/// 仅更新已存在 todo 的 prompt，不存在则跳过。
async fn update_todo_prompt_if_exists(
    db: &Database,
    prompt_template: &str,
    workspace_id: i64,
) -> Result<(), AppError> {
    let Some(todo) = db
        .get_todo_by_action_type_and_key_and_workspace("blackboard", ACTION_KEY_WIKI, workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    else {
        // todo 尚未创建（用户从未触发过 wiki 更新）：无需同步
        return Ok(());
    };
    // prompt 未变化：跳过无意义写入
    if todo.prompt == prompt_template {
        return Ok(());
    }
    let title = format!("Blackboard Wiki: workspace_{}", workspace_id);
    db.update_todo_full(crate::db::TodoUpdate {
        id: todo.id,
        title: &title,
        prompt: prompt_template,
        status: crate::models::TodoStatus::Pending,
        executor: None,
        scheduler_enabled: None,
        scheduler_config: None,
        scheduler_timezone: None,
        workspace_id: None,
        webhook_enabled: None,
        acceptance_criteria: None,
        auto_review_enabled: None,
        action_type: Some("blackboard"),
        action_key: Some(ACTION_KEY_WIKI),
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(())
}

/// 构建 Wiki 更新的 Prompt 模板（单阶段）。
///
/// LLM 直接编辑文件，不需要输出 YAML/JSON。
fn build_wiki_prompt() -> String {
    r##"你是一个工作空间黑板维护者。你的任务是分析新的执行记录，更新 Wiki 页面。

你拥有以下工具，可以直接在执行过程中调用：
- `ls ~/.ntd/workspace/{{workspace_id}}/wiki/topics/`：列出现有主题页面
- `cat ~/.ntd/workspace/{{workspace_id}}/wiki/topics/<slug>.md`：读取页面内容
- `ntd todo execution get <id>`：获取指定执行记录的完整结论（result 字段）

待分析的执行记录 ID 列表：
{{pending_record_ids}}

请按以下步骤操作：
1. 列出现有主题页面，了解当前 Wiki 结构
2. 逐个调用 `ntd todo execution get <id>` 获取每条执行记录的结论
3. 分析每条结论涉及哪些主题领域
4. 对于新主题：创建 `~/.ntd/workspace/{{workspace_id}}/wiki/topics/<slug>.md`
5. 对于已有主题：编辑文件，追加/更新结论（保持已有内容）
6. 每个页面结构：
   - # 标题（中文）
   - ## 已确认
   - ## 新发现
   - ## 待解决问题
   - ## 矛盾/风险
   - ## 下一步建议
7. 每条结论标注来源，使用 `ntd todo execution get <record_id>` 返回结果中的 `todo_id` 和 `id` 字段，
   生成 app 内链接：(来源: [record_{record_id}](/?view=items&id={todo_id}&panel=post&record={record_id}))

完成后输出简短确认即可，无需输出 YAML/JSON。"##.to_string()
}

/// 启动 Wiki 更新执行并阻塞等待完成。
///
/// 返回 None 表示执行未产出结果（非错误）。
#[allow(clippy::too_many_arguments)] // 参数来自调用方的多个独立数据源，强行合并为 struct 会增加认知负担
async fn run_wiki_execution(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
    todo_id: i64,
    message: String,
    params: HashMap<String, String>,
) -> Result<Option<String>, AppError> {
    // 先订阅
    let mut rx = tx.subscribe();

    // 启动执行
    let result = crate::handlers::execution::start_todo_execution(RunTodoExecutionRequest {
        db: db.clone(),
        executor_registry,
        tx,
        task_manager,
        config,
        todo_id,
        message,
        req_executor: None,
        trigger_type: TRIGGER_TYPE_BLACKBOARD.to_string(),
        params: Some(params),
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
        workspace_id: Some(workspace_id),
    })
    .await?;

    let task_id = result.task_id.clone();
    result.record_id.ok_or_else(|| {
        AppError::Internal("Wiki 更新任务启动失败".to_string())
    })?;

    // 读取 per-workspace 超时配置：用户在黑板设置界面可调，DB 读失败回退默认 5 分钟
    let timeout_secs = resolve_wiki_timeout_secs(&db, workspace_id).await;

    // 等待 Finished
    wait_for_finished(&mut rx, &task_id, workspace_id, timeout_secs).await
}

/// 等待目标 task_id 对应的 Finished 事件。
///
/// 超时时长由调用方传入（来自 per-workspace 黑板配置 wiki_timeout_secs），
/// 防止事件丢失时永久阻塞；仅当 `success=true` 时才认为执行成功并返回结果。
async fn wait_for_finished(
    rx: &mut tokio::sync::broadcast::Receiver<ExecEvent>,
    task_id: &str,
    workspace_id: i64,
    timeout_secs: u64,
) -> Result<Option<String>, AppError> {
    // 用配置的超时时长，防止事件丢失时永久阻塞
    let timeout_duration = tokio::time::Duration::from_secs(timeout_secs);
    let deadline = tokio::time::Instant::now() + timeout_duration;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            // 超时：事件未在规定时间内到达
            Err(_) => {
                tracing::warn!(
                    "Wiki 执行超时: workspace_id={}, task_id={}, timeout_secs={}",
                    workspace_id,
                    task_id,
                    timeout_secs
                );
                return Err(AppError::Internal(format!(
                    "Wiki 执行超时（{}秒），task_id={}",
                    timeout_secs,
                    task_id
                )));
            }
            Ok(Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: Some(ref new_content),
                success: true,
                ..
            })) if *finished_task_id == task_id => {
                return Ok(Some(new_content.clone()));
            }
            Ok(Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: _,
                success: false,
                ..
            })) if *finished_task_id == task_id => {
                tracing::warn!(
                    "Wiki 执行失败: workspace_id={}, task_id={}",
                    workspace_id,
                    task_id
                );
                return Ok(None);
            }
            Ok(Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: None,
                success: true,
                ..
            })) if *finished_task_id == task_id => {
                tracing::warn!(
                    "Wiki 执行未产出结果: workspace_id={}, task_id={}",
                    workspace_id,
                    task_id
                );
                return Ok(None);
            }
            Ok(Ok(_)) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                tracing::warn!("Wiki 更新事件通道积压，跳过 {} 个事件", n);
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                return Err(AppError::Internal("事件通道已关闭".to_string()));
            }
        }
    }
}

/// Wiki 更新入口：单阶段调用，LLM 直接编辑文件。
///
/// 流程：
/// 1. 空队列直接返回（无需处理）
/// 2. 确保 wiki 目录存在
/// 3. 构造 prompt（含 workspace_id 和 record_ids）
/// 4. 单次 LLM 调用
/// 5. 后处理：生成 index.md、追加 log.md
pub async fn update_blackboard_wiki(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
    pending_record_ids: Vec<i64>,
) -> Result<(), AppError> {
    // 1. 空队列直接返回，避免无意义地启动 LLM 调用
    if pending_record_ids.is_empty() {
        tracing::debug!("Wiki 更新跳过: pending_record_ids 为空, workspace_id={}", workspace_id);
        return Ok(());
    }

    // 2. 确保 wiki 目录存在
    init_wiki_dir(workspace_id).map_err(|e| {
        AppError::Internal(format!("初始化 wiki 目录失败: {:?}", e))
    })?;

    // 3. 查找或创建 todo（prompt 由配置保存接口同步，执行时不触碰）
    let todo_id = find_or_create_wiki_todo(&db, workspace_id).await?;

    // 4. 用 todo.prompt 作为执行 message：这是真相源
    //    - 用户在配置页保存提示词 → apply_wiki_prompt_to_todo 同步到 todo.prompt
    //    - 未配置 → 创建时已用内置默认兜底
    let prompt_template = db
        .get_todo(todo_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .map(|t| t.prompt)
        .unwrap_or_else(build_wiki_prompt);
    let ids_str = format!("{:?}", pending_record_ids);
    let mut params = HashMap::new();
    params.insert("workspace_id".to_string(), workspace_id.to_string());
    params.insert("pending_record_ids".to_string(), ids_str);
    let message = crate::models::replace_placeholders(&prompt_template, &params);

    // 5. 启动执行
    let _result = run_wiki_execution(
        db.clone(),
        executor_registry,
        tx,
        task_manager,
        config,
        workspace_id,
        todo_id,
        message,
        params,
    ).await?;

    // 6. 后处理：追加 log
    // 无论 LLM 是否输出，都尝试追加 log
    let topics = list_topics(workspace_id).map_err(|e| {
        AppError::Internal(format!("列出 topic 失败: {:?}", e))
    })?;

    // 追加 log.md
    if !pending_record_ids.is_empty() {
        append_log_entry(workspace_id, &pending_record_ids, &topics).map_err(|e| {
            AppError::Internal(format!("追加 log 失败: {:?}", e))
        })?;
    }

    // 清空已处理的记录：只删除本次处理的 ID，保留期间新到达的记录
    db.remove_specific_pending_record_ids(workspace_id, &pending_record_ids)
        .await
        .map_err(|e| AppError::Internal(format!("清空 pending 队列失败: {}", e)))?;

    tracing::info!(
        "Wiki 更新完成: workspace_id={}, topics={}, records={}",
        workspace_id,
        topics.len(),
        pending_record_ids.len()
    );

    Ok(())
}

/// Wiki 对话响应结构（对应 handler 的 WikiChatResponse）。
#[derive(Debug, serde::Serialize)]
pub struct WikiChatResponse {
    pub content: String,
    pub task_id: String,
    /// 是否执行成功（退出码为 0）
    pub success: bool,
    /// 执行时长（秒）
    pub duration_secs: i64,
}

/// 执行一次 Wiki 对话：spawn 执行器在 wiki 目录运行，返回解析后的结果文本。
///
/// 设计参考：feishu_listener 的「executor 默认响应」模式（message_debounce.rs 的
/// handle_default_response_executor），把触发源从飞书消息换成 HTTP 请求，
/// 把 cwd 从 project_directories.path 换成 wiki 目录，把结果回送方式从
/// ExecEvent::DirectCardMessage 改成直接返回值。
///
/// 不创建 Todo、不创建 execution_record、不持久化聊天历史、非流式一次性返回。
pub async fn chat_with_wiki(
    db: &Arc<Database>,
    executor_registry: &Arc<ExecutorRegistry>,
    tx: &tokio::sync::broadcast::Sender<crate::executor_service::events::ExecEvent>,
    workspace_id: i64,
    message: &str,
    override_executor: Option<&str>,
) -> Result<WikiChatResponse, AppError> {
    use crate::executor_service::events::ExecEvent;

    // 1. 确定执行器名称：优先用请求里指定的，其次用黑板配置，最后缺省 claudecode
    let exec_name = resolve_chat_executor(db, workspace_id, override_executor).await?;

    // 2. 解析执行器类型字符串为 ExecutorType 枚举
    let exec_type = crate::adapters::parse_executor_type(&exec_name)
        .ok_or_else(|| AppError::BadRequest(format!("未知的执行器类型: {}", exec_name)))?;

    // 3. 从注册中心获取执行器实例
    let executor = executor_registry
        .get(exec_type)
        .await
        .ok_or_else(|| AppError::BadRequest(format!("执行器未安装: {}", exec_name)))?;

    // 4. 获取该执行器的 session（用于保持连续对话）
    let session_id = db
        .get_wiki_chat_session(workspace_id, &exec_name)
        .await
        .map_err(|e| AppError::Internal(format!("读取 wiki chat session 失败: {}", e)))?
        .flatten();

    // 5. 确保 wiki 目录存在（首次访问时自动创建）
    init_wiki_dir(workspace_id).map_err(|e| {
        AppError::Internal(format!("初始化 wiki 目录失败: {:?}", e))
    })?;
    let wiki_dir = crate::wiki::wiki_dir(workspace_id).map_err(|e| {
        AppError::Internal(format!("获取 wiki 目录失败: {:?}", e))
    })?;

    // 6. 生成任务 ID，先发送 Started 事件（前端可立即显示"执行中"）
    let task_id = format!("wiki-chat-{}", uuid::Uuid::new_v4());
    tracing::info!(
        "wiki chat: task_id={}, workspace_id={}, executor={}, message_len={}, session_id={:?}",
        task_id,
        workspace_id,
        exec_name,
        message.len(),
        session_id
    );
    let _ = tx.send(ExecEvent::WikiChatStarted {
        task_id: task_id.clone(),
        workspace_id,
        executor: exec_name.clone(),
        message: message.to_string(),
    });

    // 7. spawn 执行器子进程，流式读取 stdout，逐行推送 WikiChatOutput 事件
    //    超时读 per-workspace 配置：用户在黑板设置界面可调，DB 读失败回退默认 5 分钟
    let timeout_secs = resolve_wiki_timeout_secs(db, workspace_id).await;
    let started_at = std::time::Instant::now();
    let spawn_result = spawn_executor_for_chat_streaming(
        &executor,
        message,
        &wiki_dir,
        &task_id,
        workspace_id,
        tx,
        session_id.clone(),
        timeout_secs,
    )
    .await;

    let duration_secs = started_at.elapsed().as_secs() as i64;

    match spawn_result {
        Ok((logs, stdout_raw, stderr_raw, success)) => {
            // 7. 从日志中提取最终结果文本
            let result_text =
                crate::executor_service::completion::get_final_result_from_logs(&logs)
                    .unwrap_or_else(|| {
                        if success {
                            stdout_raw.clone()
                        } else {
                            format!("执行失败\n\n输出：\n{}\n\n错误：\n{}", stdout_raw, stderr_raw)
                        }
                    });

            // 8. 发送 Finished 事件
            let _ = tx.send(ExecEvent::WikiChatFinished {
                task_id: task_id.clone(),
                workspace_id,
                success,
                result: Some(result_text.clone()),
                duration_secs,
            });

            // 9. 成功时从日志中提取 session_id 并持久化到数据库
            if success {
                if let Some(new_session_id) = extract_session_from_logs(&executor, &logs) {
                    tracing::info!(
                        "wiki chat: extracted session_id={} for executor={}, saving to DB",
                        new_session_id,
                        exec_name
                    );
                    if let Err(e) = db.set_wiki_chat_session(workspace_id, &exec_name, Some(new_session_id)).await {
                        tracing::warn!("保存 wiki chat session 失败: {:?}", e);
                    }
                }
            }

            Ok(WikiChatResponse {
                content: result_text,
                task_id,
                success,
                duration_secs,
            })
        }
        Err(e) => {
            // 出错也发 Finished 事件，让前端知道结束了
            let err_msg = match &e {
                AppError::Internal(msg) => msg.clone(),
                AppError::BadRequest(msg) => msg.clone(),
                AppError::NotFound => "资源不存在".to_string(),
            };
            let _ = tx.send(ExecEvent::WikiChatFinished {
                task_id: task_id.clone(),
                workspace_id,
                success: false,
                result: Some(err_msg),
                duration_secs,
            });
            Err(e)
        }
    }
}

/// 从执行日志中提取 session_id。
///
/// 流程：
/// 1. 先尝试从日志内容中提取（parse_output_session_id）
/// 2. 如果没有，尝试执行器内部缓存的 session_id（get_session_id）
///
/// 不同执行器暴露 session_id 的方式不同：
/// - Claude Code: stdout JSONL 行含 session_id
/// - Hermès: `session_id: <sid>` 行
/// - Pi: `{"type":"session","id":"<sid>"}` 行（通过 get_session_id 获取缓存值）
///
/// 返回 None 表示执行器不支持 session 或首次执行。
fn extract_session_from_logs(
    executor: &Arc<dyn crate::adapters::CodeExecutor>,
    logs: &[crate::models::ParsedLogEntry],
) -> Option<String> {
    // 1. 优先从日志内容提取
    for entry in logs {
        if let Some(sid) = executor.extract_session_id(&entry.content) {
            return Some(sid);
        }
    }
    // 2. 回退到执行器内部缓存的 session_id（Pi 等执行器在 parse_output_line 时缓存）
    executor.get_session_id()
}

/// 解析本次对话使用的执行器名称。
///
/// 优先级：override_executor（请求指定） > 黑板配置 wiki_chat_executor > 默认 "claudecode"。
/// 空字符串视为未配置，回退到默认值。
async fn resolve_chat_executor(
    db: &Arc<Database>,
    workspace_id: i64,
    override_executor: Option<&str>,
) -> Result<String, AppError> {
    // 请求里明确指定了 → 直接用
    if let Some(name) = override_executor {
        if !name.is_empty() {
            return Ok(name.to_string());
        }
    }
    // 读黑板配置
    let cfg = db
        .get_blackboard_config(workspace_id)
        .await
        .map_err(|e| AppError::Internal(format!("查询黑板配置失败: {}", e)))?;
    if let Some(c) = cfg {
        if let Some(name) = c.wiki_chat_executor {
            if !name.is_empty() {
                return Ok(name);
            }
        }
    }
    // 都没有 → 默认 claudecode
    Ok("claudecode".to_string())
}

/// 从 per-workspace 黑板配置读取 Wiki 执行超时秒数。
///
/// DB 读取失败或记录缺失时回退默认值（5 分钟），与历史写死行为一致，
/// 避免配置读取异常影响 Wiki 任务的可用性。返回值已被 db 层钳制到合法区间，
/// 这里只做防御性兜底：负值/0 视为未配置回退默认。
async fn resolve_wiki_timeout_secs(db: &Arc<Database>, workspace_id: i64) -> u64 {
    // 读黑板配置，失败不致命——回退默认值保证 Wiki 任务能继续跑
    match db.get_blackboard_config(workspace_id).await {
        Ok(Some(cfg)) if cfg.wiki_timeout_secs > 0 => cfg.wiki_timeout_secs as u64,
        // 配置缺失/读到 0/负值 → 回退默认 5 分钟
        _ => {
            tracing::debug!(
                "wiki timeout 未配置或非法，回退默认值: workspace_id={}",
                workspace_id
            );
            crate::db::blackboard::DEFAULT_WIKI_TIMEOUT_SECS as u64
        }
    }
}

/// spawn 执行器子进程，流式读取 stdout，逐行解析并推送 WikiChatOutput 事件。
///
/// 与 handle_default_response_executor 的 spawn 逻辑保持一致：
/// - stdout 用 piped 收集日志
/// - stderr 丢弃（避免污染结果解析）
/// - stdin 用 piped（部分执行器需要预写 payload）
/// - cwd 设为 wiki 目录
///
/// 返回：(解析出的日志条目列表, 原始 stdout 文本, 原始 stderr 文本, 是否成功退出)
/// 同时通过 broadcast channel 实时推送每一行日志，前端 WebSocket 可收到。
///
/// 超时保护由调用方传入（来自 per-workspace 配置 wiki_timeout_secs），
/// 超时后 kill 子进程并返回错误。
///
/// 参数较多但均为生成子进程所必需的上下文，拆 struct 反而割裂调用点可读性，
/// 故与项目内其他 spawn 类函数一致允许 too_many_arguments。
#[allow(clippy::too_many_arguments)]
async fn spawn_executor_for_chat_streaming(
    executor: &std::sync::Arc<dyn crate::adapters::CodeExecutor>,
    message: &str,
    cwd: &std::path::Path,
    task_id: &str,
    workspace_id: i64,
    tx: &tokio::sync::broadcast::Sender<crate::executor_service::events::ExecEvent>,
    session_id: Option<String>,
    timeout_secs: u64,
) -> Result<(Vec<crate::models::ParsedLogEntry>, String, String, bool), AppError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use crate::executor_service::events::ExecEvent;

    let program = executor.executable_path();
    // 使用带 session 的命令参数（执行器不支持时自动忽略）
    let command_args = executor.command_args_with_session(message, session_id.as_deref(), session_id.is_some());

    tracing::info!(
        "wiki chat spawn: task_id={}, {} {:?} (cwd={:?})",
        task_id,
        program,
        command_args,
        cwd
    );

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(&command_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::piped())
        .current_dir(cwd);

    let mut child = cmd.spawn().map_err(|e| {
        AppError::Internal(format!("启动执行器失败: {}", e))
    })?;

    // 处理 stdin：无论是否有 payload，都需要 take() 并 drop 以确保 stdin 关闭，
    // 避免 CLI 进程挂起等待 EOF。
    if let Some(mut stdin) = child.stdin.take() {
        if let Some(payload) = executor.stdin_payload() {
            stdin
                .write_all(payload.as_bytes())
                .await
                .map_err(|e| AppError::Internal(format!("写入 stdin payload 失败: {}", e)))?;
            stdin
                .flush()
                .await
                .map_err(|e| AppError::Internal(format!("flush stdin 失败: {}", e)))?;
        }
        drop(stdin);
    }

    // 取出 stdout 和 stderr，用 BufReader 逐行读取
    let stdout = child.stdout.take().ok_or_else(|| {
        AppError::Internal("无法获取执行器 stdout".to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        AppError::Internal("无法获取执行器 stderr".to_string())
    })?;
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    // 收集解析后的日志和原始 stdout/stderr
    let mut parsed_logs: Vec<crate::models::ParsedLogEntry> = Vec::new();
    let mut raw_stdout_lines: Vec<String> = Vec::new();
    let mut raw_stderr_lines: Vec<String> = Vec::new();

    // 超时保护：用 per-workspace 配置值，超时后 kill 子进程避免僵尸进程
    let timeout_fut = tokio::time::sleep(std::time::Duration::from_secs(timeout_secs));
    tokio::pin!(timeout_fut);

    loop {
        tokio::select! {
            // 读取下一行 stdout
            line_result = stdout_reader.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        raw_stdout_lines.push(line.clone());
                        // 解析成 ParsedLogEntry，成功则推送事件
                        if let Some(entry) = executor.parse_output_line(&line) {
                            // 推送 WikiChatOutput 事件（前端通过 WebSocket 收到）
                            let _ = tx.send(ExecEvent::WikiChatOutput {
                                task_id: task_id.to_string(),
                                workspace_id,
                                entry: entry.clone(),
                            });
                            parsed_logs.push(entry);
                        }
                    }
                    // stdout 读完了，等待子进程退出并返回结果
                    Ok(None) => {
                        let status = child.wait().await.map_err(|e| {
                            AppError::Internal(format!("等待执行器退出失败: {}", e))
                        })?;
                        let stdout_raw = raw_stdout_lines.join("\n");
                        let stderr_raw = raw_stderr_lines.join("\n");
                        let success = status.success();
                        tracing::info!(
                            "wiki chat executor finished: task_id={}, exit_code={:?}, stdout_len={}, stderr_len={}",
                            task_id,
                            status.code(),
                            stdout_raw.len(),
                            stderr_raw.len()
                        );
                        if !stderr_raw.is_empty() {
                            tracing::warn!(
                                "wiki chat executor stderr: task_id={}, stderr={}",
                                task_id,
                                stderr_raw
                            );
                        }
                        return Ok((parsed_logs, stdout_raw, stderr_raw, success));
                    }
                    Err(e) => {
                        let stderr_raw = raw_stderr_lines.join("\n");
                        return Err(AppError::Internal(format!(
                            "读取执行器 stdout 失败: {}\nstderr: {}",
                            e, stderr_raw
                        )));
                    }
                }
            }
            // 读取下一行 stderr
            line_result = stderr_reader.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        let line_clone = line.clone();
                        raw_stderr_lines.push(line);
                        tracing::debug!(
                            "wiki chat executor stderr: task_id={}, line={}",
                            task_id,
                            line_clone
                        );
                    }
                    Ok(None) => {
                        // stderr 读完了，继续等待 stdout
                    }
                    Err(e) => {
                        tracing::warn!(
                            "wiki chat executor stderr read error: task_id={}, error={}",
                            task_id,
                            e
                        );
                    }
                }
            }
            // 超时
            _ = &mut timeout_fut => {
                tracing::error!(
                    "wiki chat executor timed out after {}s, killing child: task_id={}",
                    timeout_secs,
                    task_id
                );
                let _ = child.kill().await;
                let stderr_raw = raw_stderr_lines.join("\n");
                return Err(AppError::Internal(format!(
                    "执行器超时（{}秒），请稍后重试或简化问题\nstderr: {}",
                    timeout_secs, stderr_raw
                )));
            }
        }
    }
}