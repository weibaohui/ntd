//! 黑板（Blackboard）Service 层。
//!
//! 实现黑板的 Wiki 化更新逻辑，复用现有的 Action/Todo 执行机制（run_todo_execution）。
//!
//! Wiki 化更新流程（两次 LLM 调用）：
//! 1. **分析阶段**：输入 = 新执行记录 + 现有页面摘要 → 输出 = 操作列表（create/update 哪些页面）
//! 2. **执行阶段**：输入 = 操作列表 + 待更新页面当前内容 → 输出 = 各页面新 Markdown 内容
//! 3. **后端落库**：upsert topic 页 → 自动生成 index 页 → 追加 log 条目
//!
//! 旧版单文件 `update_blackboard` 保留用于兼容，新增 `update_blackboard_wiki` 为 Wiki 模式入口。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::adapters::ExecutorRegistry;
use crate::config::Config;
use crate::db::Database;
use crate::db::blackboard_page::{PAGE_TYPE_INDEX, PAGE_TYPE_TOPIC};
use crate::executor_service::{ExecEvent, RunTodoExecutionRequest};
use crate::handlers::AppError;
use crate::task_manager::TaskManager;

/// 当前实现的固定 trigger_type：在 Finished 钩子中用于识别"自身"避免递归触发。
const TRIGGER_TYPE_BLACKBOARD: &str = "blackboard";

/// Wiki 模式：分析阶段的 action_key（第一次调用，决定改哪些页面）。
const ACTION_KEY_ANALYZE: &str = "wiki-analyze";

/// Wiki 模式：执行阶段的 action_key（第二次调用，写页面内容）。
const ACTION_KEY_EXECUTE: &str = "wiki-execute";

/// Wiki 模式：查找或创建指定 action_key 的黑板 Todo。
///
/// 每个阶段（analyze/execute）各有一个独立的 Todo，prompt 模板不同。
/// action_type 固定为 "blackboard"，action_key 区分阶段。
async fn find_or_create_wiki_todo(
    db: &Database,
    prompt_template: &str,
    workspace_id: i64,
    action_key: &str,
) -> Result<i64, AppError> {
    if let Some(todo) = db
        .get_todo_by_action_type_and_key_and_workspace("blackboard", action_key, workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        // wiki 提示词模板可能随版本升级变化，每次运行时无条件同步最新模板
        db.update_todo_full(crate::db::TodoUpdate {
            id: todo.id,
            title: &todo.title,
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
            action_key: Some(action_key),
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        return Ok(todo.id);
    }

    let dir = db
        .get_project_directory_by_id(workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", workspace_id)))?;

    let title = format!("Blackboard Wiki: {} - workspace_{}", action_key, workspace_id);
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
        action_key: Some(action_key),
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(todo_id)
}

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
/// ⚠️ 注意：此为后端内置默认提示词，前端 `BlackboardPage.tsx` 中也有一份
/// `DEFAULT_BLACKBOARD_UPDATE_PROMPT` 与之对应，修改时需同步更新两处。
///
/// 包含占位符 `{{current}}`、`{{pending_record_ids}}`，
/// 在执行前由 `replace_placeholders` 替换为实际值。
/// 模板要求 AI 通过 CLI 命令主动查询执行记录，再将结论整合到黑板。
fn build_blackboard_prompt() -> String {
    r#"你是一个工作空间索引的维护者。你的任务是维护一个 Markdown 格式的"黑板"，记录工作空间中所有任务执行的结论和当前进展。

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
4. 每条结论标注来源。使用 `ntd todo execution get <record_id>` 返回结果中的 `todo_id` 和 `id` 字段，
   生成 app 内链接，格式：(来源: [record_{record_id}](/?view=items&id={todo_id}&panel=post&record={record_id}))
5. 如果结论与已有结论矛盾，在"矛盾/风险"中标注
6. 如果结论提出了未解决的问题，在"待解决问题"中列出
7. 更新"下一步建议"
8. 保持 Markdown 格式，不要添加 HTML
9. 如果黑板为空，根据新结论创建初始结构

只输出更新后的黑板内容，不要输出任何解释。"#.to_string()
}

/// 构建 Wiki 分析阶段的 Prompt 模板（第一次调用）。
///
/// 输入占位符：
/// - `{{page_summaries}}`：现有页面列表（slug + title + 摘要）
/// - `{{pending_record_ids}}`：待分析的执行记录 ID 列表
///
/// 输出要求：严格 JSON 格式，包含 operations 数组，每项描述 create/update 哪个页面。
fn build_wiki_analyze_prompt() -> String {
    r##"你是一个工作空间黑板维护者。你的任务是分析新的执行记录，决定它们应该归到哪些主题页面。

你拥有以下工具，可以直接在执行过程中调用：
- `ntd todo execution get <id>`：获取指定执行记录的完整结论（result 字段）。例如：`ntd todo execution get 42` 获取第 42 条执行记录的结论。

现有主题页面列表（slug | 标题 | 摘要）：
```
{{page_summaries}}
```

待分析的执行记录 ID 列表：
{{pending_record_ids}}

请按以下步骤操作：
1. 逐个调用 `ntd todo execution get <id>` 获取每条执行记录的完整结论
2. 仔细分析每条结论涉及哪些主题领域
3. 根据分析结果决定：是创建新页面还是更新现有页面
4. 输出严格的 JSON 格式，不要输出任何其他解释

输出格式：
```json
{
  "operations": [
    {
      "action": "create",
      "slug": "auth-module",
      "title": "认证模块",
      "summary": "关于 JWT 验证、token 刷新、权限控制的结论汇总",
      "record_ids": [42, 45]
    },
    {
      "action": "update",
      "slug": "performance",
      "title": "性能优化",
      "summary": "数据库查询优化、缓存策略、接口响应时间",
      "record_ids": [47]
    }
  ]
}
```

要求：
- slug 使用英文小写，单词间用连字符（如 "auth-module"）
- 标题使用中文，简洁明了
- summary 用一句话概括页面内容
- record_ids 列出涉及的执行记录 ID
- 如果结论涉及多个主题，可以分配到多个页面
- 不要创建过于细碎的页面，尽量将相关结论归到同一主题下
- 只输出 JSON，不要输出其他任何文字"##.to_string()
}

/// 构建 Wiki 执行阶段的 Prompt 模板（第二次调用）。
///
/// 输入占位符：
/// - `{{operations_json}}`：分析阶段输出的操作列表 JSON
/// - `{{page_contents}}`：待更新页面的当前内容（仅包含 operations 中涉及的页面）
///
/// 输出要求：严格 JSON 格式，key 为 slug，value 为新的 Markdown 内容。
fn build_wiki_execute_prompt() -> String {
    r##"你是一个工作空间黑板维护者。你的任务是根据分析结果，更新或创建主题页面的 Markdown 内容。

你拥有以下工具，可以直接在执行过程中调用：
- `ntd todo execution get <id>`：获取指定执行记录的完整结论（result 字段）。例如：`ntd todo execution get 42` 获取第 42 条执行记录的结论。

操作列表（JSON）：
```json
{{operations_json}}
```

待更新页面的当前内容（仅列出需要 update 的页面，create 的页面无当前内容）：
```
{{page_contents}}
```

请按以下要求操作：
1. 对于 action="create" 的页面：调用 `ntd todo execution get <id>` 获取 record_ids 中每条执行记录的结论，根据这些结论创建新的主题页面
2. 对于 action="update" 的页面：调用 `ntd todo execution get <id>` 获取 record_ids 中每条执行记录的结论，将新结论整合到现有页面中，保持已有内容，补充新信息
3. 每个页面使用以下结构：
   - # 页面标题
   - ## 已确认
   - ## 新发现
   - ## 待解决问题
   - ## 矛盾/风险
   - ## 下一步建议
4. 每条结论标注来源。使用 `ntd todo execution get <record_id>` 返回结果中的 `todo_id` 和 `id` 字段，
   生成 app 内链接，格式：(来源: [record_{record_id}](/?view=items&id={todo_id}&panel=post&record={record_id}))
5. 如果新结论与已有结论矛盾，在"矛盾/风险"中标注
6. 保持 Markdown 格式，不要添加 HTML

输出严格的 JSON 格式，key 为页面 slug，value 为完整的 Markdown 内容：
```json
{
  "auth-module": "# 认证模块\n\n## 已确认\n- ...",
  "performance": "# 性能优化\n\n## 已确认\n- ..."
}
```

只输出 JSON，不要输出其他任何文字。"##.to_string()
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
        .map_err(|e| AppError::Internal(format!("读取黑板失败: {:?}", e)))?;
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
        .map_err(|e| AppError::Internal(format!("更新黑板失败: {:?}", e)))?;
    tracing::info!("黑板更新成功: workspace_id={}", workspace_id);
    Ok(())
}

/// 生成 index 页面 Markdown：列出所有 topic 页面的链接和摘要。
///
/// index 页由后端自动生成，保证内容与实际页面列表 100% 一致，
/// 不让 LLM 写 index 避免遗漏或格式不一致。
async fn regenerate_index_page(db: &Database, workspace_id: i64) -> Result<(), AppError> {
    let pages = db.list_topic_pages(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("查询主题页面列表失败: {:?}", e))
    })?;

    let mut md = String::from("# 工作空间索引

");
    md.push_str("## 主题页面

");

    if pages.is_empty() {
        md.push_str("_暂无主题页面，等待任务执行结论自动生成。_
");
    } else {
        for p in &pages {
            let refs: Vec<i64> = serde_json::from_str(&p.source_refs).unwrap_or_default();
            md.push_str(&format!(
                "- **{}** — {}（{} 条来源）
",
                p.title,
                p.summary,
                refs.len()
            ));
        }
    }

    md.push_str(&format!(
        "
---
*页面总数：{} | 最后更新：自动生成*
",
        pages.len()
    ));

    db.upsert_blackboard_page(
        workspace_id,
        PAGE_TYPE_INDEX,
        "index",
        "索引",
        "页面索引",
        &md,
        &[],
    ).await.map_err(|e| AppError::Internal(format!("更新 index 页面失败: {:?}", e)))?;

    Ok(())
}

/// 追加一条 log 记录到 log 页面（纯追加，永不修改旧条目）。
///
/// log 页记录每次摄入的时间、涉及页面、来源记录，便于追溯。
async fn append_log_entry(
    db: &Database,
    workspace_id: i64,
    record_ids: &[i64],
    operations: &[serde_json::Value],
) -> Result<(), AppError> {
    use chrono::Local;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();

    let mut entry = format!("## [{}] 摄入 | 执行记录 #{}

", now, record_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", #"));

    entry.push_str("涉及页面：
");
    for op in operations {
        let action = op.get("action").and_then(|v| v.as_str()).unwrap_or("unknown");
        let slug = op.get("slug").and_then(|v| v.as_str()).unwrap_or("?");
        let title = op.get("title").and_then(|v| v.as_str()).unwrap_or("?");
        let action_cn = match action {
            "create" => "新建",
            "update" => "更新",
            _ => action,
        };
        entry.push_str(&format!("- {}（{}）— {}
", title, action_cn, slug));
    }
    entry.push_str("\n");

    db.append_log_entry(workspace_id, &entry)
        .await.map_err(|e| AppError::Internal(format!("追加日志失败: {:?}", e)))?;

    Ok(())
}

/// 第一次 LLM 调用：分析阶段 — 决定创建/更新哪些页面。
///
/// 输入：现有页面摘要 + 待分析 record_ids
/// 输出：operations JSON 数组
async fn run_analyze_phase(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
    pending_record_ids: Vec<i64>,
) -> Result<Vec<serde_json::Value>, AppError> {
    // 1. 获取现有主题页面的摘要列表
    let pages = db.list_topic_pages(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("分析阶段：查询页面列表失败: {:?}", e))
    })?;

    let mut page_summaries = String::new();
    for p in &pages {
        page_summaries.push_str(&format!(
            "{} | {} | {}
",
            p.slug, p.title, p.summary
        ));
    }
    if page_summaries.is_empty() {
        page_summaries = "(暂无主题页面)
".to_string();
    }

    // 2. 构造分析阶段 prompt
    let prompt_template = build_wiki_analyze_prompt();
    let ids_str = format!("{:?}", pending_record_ids);
    let mut params = HashMap::new();
    params.insert("page_summaries".to_string(), page_summaries);
    params.insert("pending_record_ids".to_string(), ids_str);
    let message = crate::models::replace_placeholders(&prompt_template, &params);

    // 3. 查找或创建 analyze 阶段的 Todo
    let action_key_analyze = format!("{}-{}", ACTION_KEY_ANALYZE, workspace_id);
    let todo_id = find_or_create_wiki_todo(&db, &prompt_template, workspace_id, &action_key_analyze).await?;

    // 4. 启动执行，等待 Finished
    let result = run_blackboard_execution(
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

    // 5. 解析输出为 JSON operations 数组
    let raw = result.ok_or_else(|| AppError::Internal("分析阶段未产出结果".to_string()))?;
    let parsed = extract_json_from_output(&raw)?;
    let ops = parsed.get("operations")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Internal("分析阶段输出缺少 operations 数组".to_string()))?;

    Ok(ops.clone())
}

/// 第二次 LLM 调用：执行阶段 — 写页面内容。
///
/// 输入：operations + 待更新页面的当前内容
/// 输出：{slug: markdown_content} JSON 对象
async fn run_execute_phase(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
    operations: &[serde_json::Value],
) -> Result<serde_json::Map<String, serde_json::Value>, AppError> {
    // 1. 收集所有需要读取当前内容的页面（action=update 的）
    let mut update_slugs = Vec::new();
    for op in operations {
        if let Some("update") = op.get("action").and_then(|v| v.as_str()) {
            if let Some(slug) = op.get("slug").and_then(|v| v.as_str()) {
                update_slugs.push(slug.to_string());
            }
        }
    }

    // 2. 读取这些页面的当前内容
    let mut page_contents_str = String::new();
    for slug in &update_slugs {
        if let Some(page) = db.get_blackboard_page(workspace_id, slug).await.map_err(|e| {
            AppError::Internal(format!("执行阶段：读取页面 {:?} 失败: {:?}", slug, e))
        })? {
            page_contents_str.push_str(&format!("=== {} ===
{}

", slug, page.content));
        }
    }
    if page_contents_str.is_empty() {
        page_contents_str = "(无需要更新的现有页面，全部为新建)
".to_string();
    }

    // 3. 构造执行阶段 prompt
    let prompt_template = build_wiki_execute_prompt();
    let ops_json = serde_json::to_string_pretty(operations).unwrap_or_default();
    let mut params = HashMap::new();
    params.insert("operations_json".to_string(), ops_json);
    params.insert("page_contents".to_string(), page_contents_str);
    let message = crate::models::replace_placeholders(&prompt_template, &params);

    // 4. 查找或创建 execute 阶段的 Todo
    let action_key_execute = format!("{}-{}", ACTION_KEY_EXECUTE, workspace_id);
    let todo_id = find_or_create_wiki_todo(&db, &prompt_template, workspace_id, &action_key_execute).await?;

    // 5. 启动执行，等待 Finished
    let result = run_blackboard_execution(
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

    // 6. 解析输出为 {slug: content} JSON 对象
    let raw = result.ok_or_else(|| AppError::Internal("执行阶段未产出结果".to_string()))?;
    let parsed = extract_json_from_output(&raw)?;
    let map = parsed.as_object()
        .ok_or_else(|| AppError::Internal("执行阶段输出不是 JSON 对象".to_string()))?;

    Ok(map.clone())
}

/// 从 LLM 输出中提取 JSON（兼容被 ```json 代码块包裹的情况）。
///
/// LLM 有时会在 JSON 外加 markdown 代码块标记，需要剥掉才能解析。
fn extract_json_from_output(raw: &str) -> Result<serde_json::Value, AppError> {
    let trimmed = raw.trim();
    // 尝试直接解析
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(v);
    }
    // 尝试剥掉 ```json ... ``` 包裹
    if let Some(start) = trimmed.find("```json") {
        let rest = &trimmed[start + 7..];
        if let Some(end) = rest.find("```") {
            let inner = rest[..end].trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(inner) {
                return Ok(v);
            }
        }
    }
    // 尝试剥掉 ``` ... ``` 包裹（无 json 标记）
    if let Some(start) = trimmed.find("```") {
        let rest = &trimmed[start + 3..];
        if let Some(end) = rest.find("```") {
            let inner = rest[..end].trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(inner) {
                return Ok(v);
            }
        }
    }
    Err(AppError::Internal(format!("无法解析 LLM 输出为 JSON: {:?}", &trimmed[..trimmed.len().min(200)])))
}

/// Wiki 模式：更新黑板（两次 LLM 调用 + index/log 自动生成）。
///
/// 流程：
/// 1. 分析阶段 → 决定 create/update 哪些页面
/// 2. 执行阶段 → 写页面内容
/// 3. 落库 → upsert topic 页 + 生成 index + 追加 log
///
/// 失败时记录 warn 但不向上抛出，避免影响源任务。
pub async fn update_blackboard_wiki(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
    pending_record_ids: Vec<i64>,
) -> Result<(), AppError> {
    // Phase 1: 分析
    let operations = run_analyze_phase(
        db.clone(),
        executor_registry.clone(),
        tx.clone(),
        task_manager.clone(),
        config.clone(),
        workspace_id,
        pending_record_ids.clone(),
    ).await.map_err(|e| {
        AppError::Internal(format!("Wiki 分析阶段失败: {:?}", e))
    })?;

    if operations.is_empty() {
        tracing::info!("Wiki 分析结果为空操作，跳过执行阶段: workspace_id={}", workspace_id);
        return Ok(());
    }

    // Phase 2: 执行
    let page_contents = run_execute_phase(
        db.clone(),
        executor_registry,
        tx,
        task_manager,
        config,
        workspace_id,
        &operations,
    ).await.map_err(|e| {
        AppError::Internal(format!("Wiki 执行阶段失败: {:?}", e))
    })?;

    // Phase 3: 落库 — 逐个 upsert topic 页面
    for op in &operations {
        let slug = op.get("slug").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Internal("操作缺少 slug".to_string()))?;
        let title = op.get("title").and_then(|v| v.as_str()).unwrap_or(slug);
        let summary = op.get("summary").and_then(|v| v.as_str()).unwrap_or("");
        let record_ids: Vec<i64> = op.get("record_ids")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();

        let content = page_contents.get(slug)
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Internal(format!("执行阶段未产出页面 {:?} 的内容", slug)))?;

        db.upsert_blackboard_page(
            workspace_id,
            PAGE_TYPE_TOPIC,
            slug,
            title,
            summary,
            content,
            &record_ids,
        ).await.map_err(|e| AppError::Internal(format!("写入页面 {:?} 失败: {:?}", slug, e)))?;

        tracing::info!("Wiki 页面已更新: workspace_id={}, slug={}", workspace_id, slug);
    }

    // Phase 4: 自动生成 index 页面
    regenerate_index_page(&db, workspace_id).await?;

    // Phase 5: 追加 log 条目
    append_log_entry(&db, workspace_id, &pending_record_ids, &operations).await?;

    // Phase 6: 清空 pending 队列
    // 清空 pending 队列：take_pending_record_ids 会取出并清空
    let _ = db.take_pending_record_ids(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("清空 pending 队列失败: {:?}", e))
    })?;

    tracing::info!("Wiki 黑板更新完成: workspace_id={}, pages={}", workspace_id, operations.len());
    Ok(())
}

/// 更新黑板内容（旧版单文件模式，保留兼容）。
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
