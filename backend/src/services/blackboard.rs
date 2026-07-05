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

/// 查找或创建黑板 Wiki Todo。
///
/// action_type="blackboard", action_key="wiki-update"。
/// 每个工作空间独立维护自己的 Wiki 更新 Todo。
async fn find_or_create_wiki_todo(
    db: &Database,
    prompt_template: &str,
    workspace_id: i64,
) -> Result<i64, AppError> {
    // 按 action_type + action_key + workspace_id 查找已有的 Todo
    if let Some(todo) = db
        .get_todo_by_action_type_and_key_and_workspace("blackboard", ACTION_KEY_WIKI, workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        // 如果 prompt 模板有变化，更新已有的 Todo
        if todo.prompt != prompt_template {
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
        }
        return Ok(todo.id);
    }

    // 未找到，自动创建
    let dir = db
        .get_project_directory_by_id(workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", workspace_id)))?;

    let title = format!("Blackboard Wiki: workspace_{}", workspace_id);
    let prompt = prompt_template.to_string();

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
async fn wait_for_finished(
    rx: &mut tokio::sync::broadcast::Receiver<ExecEvent>,
    task_id: &str,
    workspace_id: i64,
) -> Result<Option<String>, AppError> {
    loop {
        match rx.recv().await {
            Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: Some(ref new_content),
                ..
            }) if *finished_task_id == task_id => {
                return Ok(Some(new_content.clone()));
            }
            Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: None,
                ..
            }) if *finished_task_id == task_id => {
                tracing::warn!(
                    "Wiki 执行未产出结果: workspace_id={}, task_id={}",
                    workspace_id,
                    task_id
                );
                return Ok(None);
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("Wiki 更新事件通道积压，跳过 {} 个事件", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err(AppError::Internal("事件通道已关闭".to_string()));
            }
        }
    }
}

/// Wiki 更新入口：单阶段调用，LLM 直接编辑文件。
///
/// 流程：
/// 1. 确保 wiki 目录存在
/// 2. 构造 prompt（含 workspace_id 和 record_ids）
/// 3. 单次 LLM 调用
/// 4. 后处理：生成 index.md、追加 log.md
pub async fn update_blackboard_wiki(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
    pending_record_ids: Vec<i64>,
) -> Result<(), AppError> {
    // 1. 确保 wiki 目录存在
    init_wiki_dir(workspace_id).map_err(|e| {
        AppError::Internal(format!("初始化 wiki 目录失败: {:?}", e))
    })?;

    // 2. 构造 prompt
    let prompt_template = build_wiki_prompt();
    let ids_str = format!("{:?}", pending_record_ids);
    let mut params = HashMap::new();
    params.insert("workspace_id".to_string(), workspace_id.to_string());
    params.insert("pending_record_ids".to_string(), ids_str);
    let message = crate::models::replace_placeholders(&prompt_template, &params);

    // 3. 查找或创建 todo
    let todo_id = find_or_create_wiki_todo(&db, &prompt_template, workspace_id).await?;

    // 4. 启动执行
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

    // 5. 后处理：生成 index、追加 log
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