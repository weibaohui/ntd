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

/// 当前实现的固定 trigger_type：在 Finished 钩子中用于识别"自身"避免递归触发。
const TRIGGER_TYPE_BLACKBOARD: &str = "blackboard";

/// 查找或创建当前工作空间的黑板更新 Todo。
///
/// 黑板更新 Todo 的特征是 action_type="blackboard", action_key="update"。
/// 如果已存在则直接返回，否则自动创建一个新的 Todo（markdown 模板作为 prompt）。
/// 每个工作空间独立维护自己的黑板更新 Todo。
pub async fn find_or_create_blackboard_todo(
    db: &Database,
    prompt_template: &str,
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
    //    create_todo_with_extras 需要 workspace 路径信息，先查询工作空间
    let dir = db
        .get_project_directory_by_id(workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", workspace_id)))?;

    // 标题便于在 todo 列表中识别黑板更新 todo
    let title = format!("Blackboard: workspace_{}", workspace_id);
    // prompt 模板内含占位符，下游 start_todo_execution 之前再做替换
    let prompt = prompt_template.to_string();

    // create_todo_with_extras 不支持直接传 action_type/action_key，先建再改
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

    // 标记为黑板更新 todo：action_type/action_key 用于 find_or_create 的下次查找
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
/// 包含占位符 `{{current}}`、`{{pending_record_ids}}`，
/// 在执行前由 `replace_placeholders` 替换为实际值。
/// 模板要求 AI 通过 CLI 命令主动查询执行记录，再将结论整合到黑板。
fn build_blackboard_prompt() -> String {
    r#"你是一个工作空间知识库的维护者。你的任务是维护一个 Markdown 格式的"黑板"，记录工作空间中所有任务执行的结论和当前进展。

当前黑板内容：
```
{{current}}
```

待分析的执行记录 ID 列表：
{{pending_record_ids}}

请按以下步骤操作：
1. 对于列表中的每个 execution_record_id，使用 `ntd todo execution get <id>` 命令获取执行结论
2. 将各记录的结论整合到黑板中
3. 保持以下结构：
   - # 工作空间进展
   - ## 已确认
   - ## 新发现
   - ## 待解决问题
   - ## 矛盾/风险
   - ## 下一步建议
4. 每条结论标注来源，格式：(来源: [execution_record_{{id}}](ntd://execution/{{id}}))
5. 如果结论与已有结论矛盾，在"矛盾/风险"中标注
6. 如果结论提出了未解决的问题，在"待解决问题"中列出
7. 更新"下一步建议"
8. 保持 Markdown 格式，不要添加 HTML
9. 如果黑板为空，根据新结论创建初始结构

只输出更新后的黑板内容，不要输出任何解释。"#.to_string()
}


/// 读取指定工作空间的黑板内容；无记录时返回空字符串。
///
/// 隐藏 None/Some 差异，让上游不用每次都 .map().unwrap_or_default()。
async fn read_current_content(
    db: &Database,
    workspace_id: i64,
) -> Result<String, AppError> {
    // 首次访问可能为 None：未创建过黑板的工作空间返回空字符串作为初始值
    let board = db
        .get_blackboard(workspace_id)
        .await
        .map_err(|e| AppError::Internal(format!("读取黑板失败: {}", e)))?;
    Ok(board.map(|b| b.content).unwrap_or_default())
}

/// 构造带占位符替换的最终 prompt，并把原 params 一并返回（start_todo_execution 也需要）。
///
/// 使用调用方传入的 prompt_template（来自运行时 config），而非从 DB 加载。
/// 这样当用户修改 config 中的提示词后，下次执行会立即生效，无需重建 todo。
fn assemble_prompt(
    prompt_template: &str,
    current_content: String,
    pending_record_ids: Vec<i64>,
) -> (String, HashMap<String, String>) {
    // pending_record_ids 转为易读的列表字符串，如 "[1, 2, 3]"
    let ids_str = format!("{:?}", pending_record_ids);
    let mut params = HashMap::new();
    params.insert("current".to_string(), current_content);
    params.insert("pending_record_ids".to_string(), ids_str);
    // models::replace_placeholders 单遍替换 {{key}} -> params[key]
    let message = crate::models::replace_placeholders(prompt_template, &params);
    (message, params)
}

/// 启动 blackboard todo 执行并阻塞等待它的 Finished 事件，返回 LLM 产出的新内容。
///
/// 核心时序（不可调整）：
/// 1. tx.subscribe() 必须在 start_todo_execution 之前：极快完成的任务会在订阅前
///    发出 Finished 事件，导致函数永远阻塞等待。
/// 2. 等待阶段按 task_id 精确过滤：并发场景下其他任务的 Finished 会被忽略。
///
/// 返回的 String 是 LLM 输出的原始内容；调用方负责判空 + 写库。
async fn run_blackboard_execution(
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
    // 先订阅：broadcast 通道不会重发订阅前的事件，必须在 start 之前
    let mut rx = tx.subscribe();
    // 启动执行：trigger_type=blackboard 让 Finished 钩子识别"自身"避免递归
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
    // record_id 为 None 视为启动失败；task_id 同时 clone 出来用于事件匹配
    let task_id = result.task_id.clone();
    result.record_id.ok_or_else(|| {
        AppError::Internal("黑板更新任务启动失败".to_string())
    })?;
    // 阻塞等 Finished；返回 None 表示执行未产出结果（非错误）
    wait_for_finished(&mut rx, &task_id, workspace_id).await
}

/// 等待目标 task_id 对应的 Finished 事件，区分"完成有空内容/无内容/通道异常"。
async fn wait_for_finished(
    rx: &mut tokio::sync::broadcast::Receiver<ExecEvent>,
    task_id: &str,
    workspace_id: i64,
) -> Result<Option<String>, AppError> {
    loop {
        match rx.recv().await {
            // 命中：result 携带 LLM 产出（可能为空字符串）
            Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: Some(ref new_content),
                ..
            }) if *finished_task_id == task_id => {
                return Ok(Some(new_content.clone()));
            }
            // 命中但无 result：执行未产出结果，按"无内容"处理，不报错
            Ok(ExecEvent::Finished {
                task_id: ref finished_task_id,
                result: None,
                ..
            }) if *finished_task_id == task_id => {
                tracing::warn!(
                    "黑板执行未产出结果: workspace_id={}, task_id={}",
                    workspace_id,
                    task_id
                );
                return Ok(None);
            }
            // 其他任务的 Finished / Started / Output：忽略继续等
            Ok(_) => {}
            // 通道积压：跳过丢失的事件继续等（task_id 匹配会再次命中）
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("黑板更新事件通道积压，跳过 {} 个事件", n);
            }
            // 通道关闭：异常状态，应当报错让上游知晓
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err(AppError::Internal("事件通道已关闭".to_string()));
            }
        }
    }
}

/// 把 LLM 产出写入黑板表，必要时跳过（空内容保护）。
///
/// 策略：
/// - 产出为空 → 不覆盖现有黑板（LLM 偶发无意义输出时保护已有内容）
/// - 产出非空 → 走 upsert，一次往返完成"创建/更新"判定 + 写入
async fn save_blackboard(
    db: &Database,
    workspace_id: i64,
    new_content: Option<String>,
) -> Result<(), AppError> {
    // None = 执行未产出结果：没有可写内容，按"无变化"处理
    let Some(new_content) = new_content else {
        return Ok(());
    };
    // 空内容保护：避免 LLM 偶发返回 "" 覆盖已有黑板
    if new_content.trim().is_empty() {
        tracing::warn!(
            "黑板更新结果为空，跳过写入: workspace_id={}",
            workspace_id
        );
        return Ok(());
    }
    // upsert：记录不存在时创建，已存在时覆盖 content/updated_at，保留 created_at
    db.upsert_blackboard_content(workspace_id, &new_content)
        .await
        .map_err(|e| AppError::Internal(format!("更新黑板失败: {}", e)))?;
    tracing::info!("黑板更新成功: workspace_id={}", workspace_id);
    Ok(())
}

/// 更新黑板内容。
///
/// 核心逻辑：
/// 1. 读取当前黑板内容
/// 2. 查找或创建 blackboard update Todo
/// 3. 构造 Prompt + 启动执行
/// 4. 阻塞等待 Finished 事件
/// 5. upsert 写回黑板
///
/// 黑板更新失败不会影响源任务的执行流程，只在日志中记录 warn。
pub async fn update_blackboard(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
    pending_record_ids: Vec<i64>,
) -> Result<(), AppError> {
    // 1+2: 读当前内容、找或建 todo；任一失败直接返回
    let current_content = read_current_content(&db, workspace_id).await?;
    // 从 per-workspace 配置中提取提示词模板；若为空则使用内置默认
    let prompt_template = {
        match db.get_blackboard_config(workspace_id).await {
            Ok(Some(cfg)) if !cfg.update_prompt.is_empty() => cfg.update_prompt,
            _ => build_blackboard_prompt(),
        }
    };
    let (todo_id, _) = find_or_create_blackboard_todo(&db, &prompt_template, workspace_id).await?;
    // 3: 组装 prompt：使用当前运行时 config 中的提示词模板
    let (message, params) = assemble_prompt(
        &prompt_template,
        current_content,
        pending_record_ids,
    );
    // 4: 启动执行 + 等待 Finished
    let new_content = run_blackboard_execution(
        db.clone(),
        executor_registry,
        tx,
        task_manager,
        config,
        workspace_id,
        todo_id,
        message,
        params,
    )
    .await?;
    // 5: 写回黑板（带空内容保护 + upsert）
    save_blackboard(&db, workspace_id, new_content).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 测试用：建一个 workspace，返回 id。
    async fn make_workspace(db: &Database) -> i64 {
        db.create_project_directory("/tmp/test-blackboard-svc", None, false, false)
            .await
            .expect("create workspace must succeed")
    }

    /// 验证 build_blackboard_prompt 含所有必要占位符。
    /// 缺占位符会导致 assemble_prompt 替换后 prompt 不完整，LLM 行为会偏离预期。
    #[test]
    fn test_prompt_contains_required_placeholders() {
        let prompt = build_blackboard_prompt();
        assert!(prompt.contains("{{current}}"));
        assert!(prompt.contains("{{pending_record_ids}}"));
    }

    /// 验证 assemble_prompt 正确替换占位符，且 pending_record_ids 透传。
    /// 防止模型 prompt 模板被误改后占位符替换断裂（导致 LLM 收到 "{{pending_record_ids}}" 字面量）。
    #[test]
    fn test_assemble_prompt_replaces_all_placeholders() {
        let prompt_template = build_blackboard_prompt();
        let (message, params) = assemble_prompt(
            &prompt_template,
            "已有黑板".to_string(),
            vec![1, 2, 3],
        );
        // 原始占位符应全部被替换
        assert!(!message.contains("{{current}}"));
        assert!(!message.contains("{{pending_record_ids}}"));
        // 替换值应透传到 message
        assert!(message.contains("已有黑板"));
        assert!(message.contains("[1, 2, 3]"));
        // params 也应原样返回，供 start_todo_execution 使用
        assert_eq!(params.get("pending_record_ids"), Some(&"[1, 2, 3]".to_string()));
    }

    /// 验证 find_or_create 第二次调用返回 (same_id, false)，避免重复创建。
    /// 黑板更新 todo 重复创建会让数据库里出现多个 action_type=blackboard 记录，
    /// 后续 update_blackboard 不知道该用哪个。
    #[tokio::test]
    async fn test_find_or_create_is_idempotent() {
        let db = Database::new(":memory:").await.unwrap();
        let ws_id = make_workspace(&db).await;
        let prompt_template = build_blackboard_prompt();
        // 第一次：新建
        let (id1, created1) = find_or_create_blackboard_todo(&db, &prompt_template, ws_id).await.unwrap();
        assert!(created1, "首次调用应返回 created=true");
        // 第二次：复用
        let (id2, created2) = find_or_create_blackboard_todo(&db, &prompt_template, ws_id).await.unwrap();
        assert!(!created2, "第二次调用应返回 created=false");
        assert_eq!(id1, id2, "应返回同一 todo id");
    }

    /// 验证不同 workspace 各自有独立的 blackboard todo。
    /// 工作空间隔离是黑板的关键约束：跨 workspace 复用 todo 会导致 prompt 串味。
    /// 注意：两个 workspace 的 path 必须不同（project_directories.path 是 UNIQUE），
    /// 这里用 ws_id 后缀保证唯一。
    #[tokio::test]
    async fn test_find_or_create_scoped_per_workspace() {
        let db = Database::new(":memory:").await.unwrap();
        let prompt_template = build_blackboard_prompt();
        let ws1 = db
            .create_project_directory("/tmp/test-blackboard-svc-1", None, false, false)
            .await
            .unwrap();
        let ws2 = db
            .create_project_directory("/tmp/test-blackboard-svc-2", None, false, false)
            .await
            .unwrap();
        let (id1, _) = find_or_create_blackboard_todo(&db, &prompt_template, ws1).await.unwrap();
        let (id2, _) = find_or_create_blackboard_todo(&db, &prompt_template, ws2).await.unwrap();
        assert_ne!(id1, id2, "不同 workspace 应当各自有独立 todo");
    }

    /// 验证 find_or_create 在 workspace 不存在时返回 BadRequest。
    /// 用过宽松的 Internal 错误会掩盖调用方错误，BadRequest 更合适。
    #[tokio::test]
    async fn test_find_or_create_missing_workspace_returns_bad_request() {
        let db = Database::new(":memory:").await.unwrap();
        let prompt_template = build_blackboard_prompt();
        let result = find_or_create_blackboard_todo(&db, &prompt_template, 9999).await;
        match result {
            Err(AppError::BadRequest(_)) => {}
            other => panic!("expected BadRequest, got: {:?}", other),
        }
    }
}
