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
use crate::wiki::{init_wiki_dir, list_topics, regenerate_index, append_log_entry};

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

    // 等待 Finished
    wait_for_finished(&mut rx, &task_id, workspace_id).await
}

/// 等待目标 task_id 对应的 Finished 事件。
///
/// 使用 5 分钟超时防止无限等待；仅当 `success=true` 时才认为执行成功并返回结果。
async fn wait_for_finished(
    rx: &mut tokio::sync::broadcast::Receiver<ExecEvent>,
    task_id: &str,
    workspace_id: i64,
) -> Result<Option<String>, AppError> {
    // 5 分钟超时，防止事件丢失时永久阻塞
    let timeout_duration = tokio::time::Duration::from_secs(5 * 60);
    let deadline = tokio::time::Instant::now() + timeout_duration;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            // 超时：事件未在规定时间内到达
            Err(_) => {
                tracing::warn!(
                    "Wiki 执行超时: workspace_id={}, task_id={}, timeout_secs=300",
                    workspace_id,
                    task_id
                );
                return Err(AppError::Internal(format!(
                    "Wiki 执行超时（5 分钟），task_id={}",
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
                // 执行失败（LLM 返回了 Finished 但 success=false）
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

    // 6. 后处理：生成 index、追加 log
    // 无论 LLM 是否输出，都尝试生成 index 和 log
    let topics = list_topics(workspace_id).map_err(|e| {
        AppError::Internal(format!("列出 topic 失败: {:?}", e))
    })?;

    // 生成 index.md
    regenerate_index(workspace_id).map_err(|e| {
        AppError::Internal(format!("生成 index 失败: {:?}", e))
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