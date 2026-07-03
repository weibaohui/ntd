//! 黑板（Blackboard）Service 层。
//!
//! 实现黑板的更新逻辑，复用现有的 Action/Todo 执行机制（run_todo_execution）。
//! 核心流程：读取当前黑板 → 构造 Prompt → 通过 Action Todo 调用 LLM → 更新黑板。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::adapters::ExecutorRegistry;
use crate::config::Config;
use crate::db::Database;
use crate::executor_service::{ExecEvent, RunTodoExecutionRequest};
use crate::handlers::AppError;
use crate::task_manager::TaskManager;

/// 查找或创建当前工作空间的黑板更新 Todo。
///
/// 黑板更新 Todo 的特征是 action_type="blackboard", action_key="update"。
/// 如果已存在则直接返回，否则自动创建一个新的 Todo（markdown 模板作为 prompt）。
/// 每个工作空间独立维护自己的黑板更新 Todo。
pub async fn find_or_create_blackboard_todo(
    db: &Database,
    workspace_id: i64,
) -> Result<(i64, bool), AppError> {
    // 1. 按 action_type + action_key + workspace_id 查找已有的 Todo
    //    作用域限定在当前工作空间，确保每个 workspace 有独立的黑板更新 Todo
    if let Some(todo) = db
        .get_todo_by_action_type_and_key_and_workspace("blackboard", "update", workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        return Ok((todo.id, false));
    }

    // 2. 未找到，自动创建
    // 先获取工作空间的路径信息（create_todo_with_extras 需要）
    let dir = db
        .get_project_directory_by_id(workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", workspace_id)))?;

    let title = format!("Blackboard: workspace_{}", workspace_id);
    let prompt = build_blackboard_prompt();

    let todo_id = db
        .create_todo_with_extras(
            &title,
            &prompt,
            None,   // executor: 使用默认
            None,   // acceptance_criteria
            false,  // webhook_enabled
            workspace_id,
            &dir.path,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 更新 action_type 和 action_key，标记为黑板更新 Todo
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
        action_key: Some("update"),
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok((todo_id, true))
}

/// 构建黑板更新的 Prompt 模板。
///
/// 包含占位符 `{{current}}`、`{{conclusion}}`、`{{todo_id}}`、`{{todo_title}}`，
/// 在执行前由 `replace_placeholders` 替换为实际值。
/// 模板要求 LLM 按固定 Markdown 结构输出，便于前端直接渲染。
fn build_blackboard_prompt() -> String {
    r#"你是一个工作空间知识库的维护者。你的任务是维护一个 Markdown 格式的"黑板"，记录工作空间中所有任务执行的结论和当前进展。

当前黑板内容：
```
{{current}}
```

新任务结论：
- 任务 ID: {{todo_id}}
- 任务标题: {{todo_title}}
- 执行结论: {{conclusion}}

请更新黑板内容，要求：
1. 将新结论整合到黑板中
2. 保持以下结构：
   - # 工作空间进展
   - ## 已确认
   - ## 新发现
   - ## 待解决问题
   - ## 矛盾/风险
   - ## 下一步建议
3. 每条结论标注来源，格式：(来源: [todo_{{todo_id}}](ntd://todo/{{todo_id}}))
4. 如果新结论与已有结论矛盾，在"矛盾/风险"中标注
5. 如果新结论提出了未解决的问题，在"待解决问题"中列出
6. 更新"下一步建议"
7. 保持 Markdown 格式，不要添加 HTML
8. 如果黑板为空，根据新结论创建初始结构

只输出更新后的黑板内容，不要输出任何解释。"#.to_string()
}

/// 更新黑板内容。
///
/// 核心逻辑：
/// 1. 读取当前黑板内容（来自 blackboards 表）
/// 2. 查找或创建 blackboard update Todo（action_type="blackboard", action_key="update"）
/// 3. 用 `replace_placeholders` 替换 Prompt 中的占位符
/// 4. 调用 `run_todo_execution` 启动执行（复用现有的 LLM 执行机制）
/// 5. 订阅 broadcast channel 等待 `Finished` 事件
/// 6. 提取 `result` 并更新 blackboards 表
///
/// 黑板更新失败不会影响源任务的执行流程，只在日志中记录 warn。
pub async fn update_blackboard(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
    conclusion: &str,
    source_todo_id: i64,
    source_todo_title: &str,
) -> Result<(), AppError> {
    // 1. 读取当前黑板内容
    //    首次使用时可能为 None，此时使用空字符串作为初始内容
    let current = db.get_blackboard(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("读取黑板失败: {}", e))
    })?;
    let current_content = current
        .map(|b| b.content)
        .unwrap_or_default();

    // 2. 查找或创建 blackboard update todo
    //    每个工作空间使用独立的 blackboard todo，避免跨空间混淆
    let (todo_id, _) = find_or_create_blackboard_todo(&db, workspace_id).await?;

    // 3. 构造 prompt（复用 Action 的占位符替换机制）
    //    models::replace_placeholders 将 {{key}} 占位符替换为 params 中的值
    let prompt = build_blackboard_prompt();
    let mut params = HashMap::new();
    params.insert("current".to_string(), current_content);
    params.insert("conclusion".to_string(), conclusion.to_string());
    params.insert("todo_id".to_string(), source_todo_id.to_string());
    params.insert("todo_title".to_string(), source_todo_title.to_string());
    let message = crate::models::replace_placeholders(&prompt, &params);

    // 4. 先订阅 broadcast channel（必须在启动执行之前订阅，否则极速完成的任务
    //    会导致 Finished 事件在 subscribe 之前发出而被丢失，函数无限等待）
    let mut rx = tx.subscribe();

    // 5. 启动执行（复用 run_todo_execution）
    //    使用 trigger_type="blackboard" 标记这是黑板更新任务，
    //    这样 Finished 事件 Hook 中可以跳过再次触发黑板更新（避免无限循环）
    let result = crate::handlers::execution::start_todo_execution(
        RunTodoExecutionRequest {
            db: db.clone(),
            executor_registry,
            tx,
            task_manager,
            config,
            todo_id,
            message,
            req_executor: None,
            trigger_type: "blackboard".to_string(),
            params: Some(params),
            resume_session_id: None,
            resume_message: None,
            source_todo_id: Some(source_todo_id),
            source_todo_title: Some(source_todo_title.to_string()),
            feishu_bot_id: None,
            feishu_receive_id: None,
            loop_step_execution_id: None,
            step_id: None,
            workspace_path: None,
            workspace_id: Some(workspace_id),
        },
    )
    .await?;

    // 获取 task_id 用于后续匹配 Finished 事件（ExecEvent::Finished 没有 record_id 字段，
    // 用全局唯一的 task_id 区分多次执行，避免并发时匹配到错误的完成事件）
    let task_id = result.task_id.clone();
    result.record_id.ok_or_else(|| {
        AppError::Internal("黑板更新任务启动失败".to_string())
    })?;

    // 6. 等待 Finished 事件
    //    用 task_id 匹配对应的完成事件，忽略其他事件的干扰
    loop {
        match rx.recv().await {
            Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: Some(ref new_content),
                ..
            }) if *finished_task_id == task_id => {
                let new_content = new_content.clone();
                // 如果 LLM 返回空内容，跳过更新，保护已有黑板内容不被覆盖
                if new_content.trim().is_empty() {
                    tracing::warn!(
                        "黑板更新结果为空，跳过更新: workspace_id={}, source_todo_id={}, task_id={}",
                        workspace_id,
                        source_todo_id,
                        task_id
                    );
                    return Ok(());
                }
                // 6. 更新黑板内容到数据库
                //    先确保黑板记录存在（首次更新时自动创建）
                if db.get_blackboard(workspace_id).await.map_err(|e| {
                    AppError::Internal(format!("查询黑板失败: {}", e))
                })?.is_none() {
                    db.create_blackboard(workspace_id).await.map_err(|e| {
                        AppError::Internal(format!("创建黑板失败: {}", e))
                    })?;
                }

                db.update_blackboard_content(workspace_id, &new_content)
                    .await
                    .map_err(|e| AppError::Internal(format!("更新黑板失败: {}", e)))?;

                tracing::info!(
                    "黑板更新成功: workspace_id={}, source_todo_id={}, task_id={}",
                    workspace_id,
                    source_todo_id,
                    task_id
                );
                return Ok(());
            }
            // Finished 事件中 result 为 None 说明执行未产出结果，
            // 跳过但不报错，避免阻塞后续黑板更新
            Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: None,
                ..
            }) if *finished_task_id == task_id => {
                tracing::warn!(
                    "黑板更新执行未产出结果: workspace_id={}, source_todo_id={}, task_id={}",
                    workspace_id,
                    source_todo_id,
                    task_id
                );
                return Ok(());
            }
            Ok(_) => {
                // 其他事件（Output/Started 等），忽略继续等待
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                // 通道积压：发生 Lagged 时跳过丢失的事件，继续等待
                tracing::warn!("黑板更新事件通道积压，跳过 {} 个事件", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                // 通道关闭：不应发生，如果通道关闭则返回错误
                return Err(AppError::Internal("事件通道已关闭".to_string()));
            }
        }
    }
}

/// 手动刷新黑板：重新执行 blackboard update todo。
///
/// 与 `update_blackboard` 不同，手动刷新只是纯粹地要求 LLM 根据当前黑板内容
/// 重新组织生成（没有新的结论输入）。效果相当于让 LLM"重新检视"现有内容。
///
/// 工作方式：
/// 1. 查找 blackboard update todo
/// 2. 以当前黑板内容 + "手动刷新"作为输入执行该 todo
/// 3. 等待 Finished 事件并更新黑板
pub async fn refresh_blackboard(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
) -> Result<(), AppError> {
    // 查找黑板的当前内容，如果没有内容则无需刷新
    let current = db.get_blackboard(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("读取黑板失败: {}", e))
    })?;
    let current_content = current
        .map(|b| b.content)
        .unwrap_or_default();

    if current_content.is_empty() {
        return Err(AppError::BadRequest("黑板暂无内容，无需刷新".to_string()));
    }

    // 查找或创建 blackboard update todo
    let (todo_id, _) = find_or_create_blackboard_todo(&db, workspace_id).await?;

    // 构造 prompt：使用当前黑板内容 + 手动刷新标记
    let prompt = build_blackboard_prompt();
    let mut params = HashMap::new();
    params.insert("current".to_string(), current_content);
    params.insert("conclusion".to_string(), "手动刷新：无新结论，请重新组织现有内容".to_string());
    params.insert("todo_id".to_string(), "0".to_string());
    params.insert("todo_title".to_string(), "手动刷新黑板".to_string());
    let message = crate::models::replace_placeholders(&prompt, &params);

    // 先订阅 broadcast channel，再启动执行，避免错过 Finished 事件
    let mut rx = tx.subscribe();

    // 启动执行
    let result = crate::handlers::execution::start_todo_execution(
        RunTodoExecutionRequest {
            db: db.clone(),
            executor_registry,
            tx,
            task_manager,
            config,
            todo_id,
            message,
            req_executor: None,
            trigger_type: "blackboard".to_string(),
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
        },
    )
    .await?;

    let task_id = result.task_id.clone();
    result.record_id.ok_or_else(|| {
        AppError::Internal("黑板刷新任务启动失败".to_string())
    })?;

    // 等待 Finished 事件
    loop {
        match rx.recv().await {
            Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: Some(ref new_content),
                ..
            }) if *finished_task_id == task_id => {
                let new_content = new_content.clone();
                // 如果 LLM 返回空内容，跳过更新，保护已有黑板内容不被覆盖
                if new_content.trim().is_empty() {
                    tracing::warn!(
                        "黑板刷新结果为空，跳过更新: workspace_id={}, task_id={}",
                        workspace_id,
                        task_id
                    );
                    return Ok(());
                }
                db.update_blackboard_content(workspace_id, &new_content)
                    .await
                    .map_err(|e| AppError::Internal(format!("更新黑板失败: {}", e)))?;

                tracing::info!(
                    "黑板刷新成功: workspace_id={}, task_id={}",
                    workspace_id,
                    task_id
                );
                return Ok(());
            }
            // Finished 事件中 result 为 None 说明执行未产出结果
            Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: None,
                ..
            }) if *finished_task_id == task_id => {
                tracing::warn!(
                    "黑板刷新执行未产出结果: workspace_id={}, task_id={}",
                    workspace_id,
                    task_id
                );
                return Ok(());
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("黑板刷新事件通道积压，跳过 {} 个事件", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err(AppError::Internal("事件通道已关闭".to_string()));
            }
        }
    }
}
