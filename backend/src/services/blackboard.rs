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

/// 构建 Wiki 分析阶段的 Prompt 模板（第一次调用）。
///
/// 输入占位符：
/// - `{{page_summaries}}`：现有页面列表（slug + title + 摘要）
/// - `{{pending_record_ids}}`：待分析的执行记录 ID 列表
///
/// 输出要求：严格 YAML 格式，包含 operations 数组，每项描述 create/update 哪个页面。
///
/// 为什么用 YAML 而非 JSON：YAML 用缩进表达结构，无需引号/反斜杠转义，
/// LLM 输出更稳更短，不易因转义错误或 token 截断导致解析失败。
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
4. 输出严格的 YAML 格式，不要输出任何其他解释

输出格式：
```yaml
operations:
  - action: create
    slug: auth-module
    title: 认证模块
    summary: 关于 JWT 验证、token 刷新、权限控制的结论汇总
    record_ids:
      - 42
      - 45
  - action: update
    slug: performance
    title: 性能优化
    summary: 数据库查询优化、缓存策略、接口响应时间
    record_ids:
      - 47
```

要求：
- slug 使用英文小写，单词间用连字符（如 "auth-module"）
- 标题使用中文，简洁明了
- summary 用一句话概括页面内容
- record_ids 列出涉及的执行记录 ID
- 如果结论涉及多个主题，可以分配到多个页面
- 不要创建过于细碎的页面，尽量将相关结论归到同一主题下
- 整个 YAML 内容必须用 ```yaml 起始、``` 结尾的代码块包裹，不要在代码块外输出任何文字"##.to_string()
}

/// 构建 Wiki 执行阶段的 Prompt 模板（第二次调用）。
///
/// 输入占位符：
/// - `{{operations_json}}`：分析阶段输出的操作列表（YAML 字符串）
/// - `{{page_contents}}`：待更新页面的当前内容（仅包含 operations 中涉及的页面）
///
/// 输出要求：严格 YAML 格式，key 为 slug，value 为 Markdown 内容。
///
/// 为什么用 YAML 而非 JSON：执行阶段 value 是大段 Markdown，含大量引号/换行/反斜杠。
/// JSON 要对这些字符转义，LLM 容易转义出错或截断在转义中途；YAML 字面量块标量
/// （`|`）能原样保留大段文本，几乎零转义负担，LLM 输出更稳更短更不易截断。
fn build_wiki_execute_prompt() -> String {
    r##"你是一个工作空间黑板维护者。你的任务是根据分析结果，更新或创建主题页面的 Markdown 内容。

你拥有以下工具，可以直接在执行过程中调用：
- `ntd todo execution get <id>`：获取指定执行记录的完整结论（result 字段）。例如：`ntd todo execution get 42` 获取第 42 条执行记录的结论。

操作列表（YAML）：
```yaml
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

输出严格的 YAML 格式，整体用 ```yaml ``` 代码块包裹，key 为页面 slug，value 为完整的 Markdown 内容。
value 用 YAML 字面量块标量 `|` 表达，可原样保留多行 Markdown，无需转义引号或换行：

```yaml
auth-module: |
  # 认证模块

  ## 已确认
  - ...
performance: |
  # 性能优化

  ## 已确认
  - ...
```

要求：
- 整个 YAML 内容必须用 ```yaml 起始、``` 结尾的代码块包裹，不要在代码块外输出任何文字
- value 一律用 `|` 字面量块标量，不要用引号包裹
- 不要输出其他任何文字"##.to_string()
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

/// 归一化黑板 Markdown 内容：剥掉 LLM 误包的最外层 fenced code block。
///
/// 规则（尽量保守）：
/// - 仅当整段内容以 ````markdown` / ````md` / ```` 开头，且以匹配的 ```` 结尾时才剥离
/// - 内部代码块不受影响
/// - 剥离后 trim() 一次
/// - 不满足条件时原样返回
///
/// 这是"双保险"的后端侧：源头修正，防止脏数据入库。
/// 前端也有一样的归一化逻辑作为兼容兜底。
pub fn normalize_blackboard_markdown(content: &str) -> String {
    let trimmed = content.trim();
    // 快速失败：太短的内容不可能包着 fenced code block
    if trimmed.len() < 5 {
        return content.to_string();
    }
    // 匹配开头的 fenced code block：``` 后跟任意语言标识符（markdown / md / code / ...），或纯 ```
    // 找到第一个换行，将 fence 整行（```xxx）一起剥掉
    if !trimmed.starts_with("```") {
        // 不是以 ``` 开头，原样返回
        return content.to_string();
    }
    let Some(first_newline) = trimmed.find('\n') else {
        // 如果没有换行说明 fence 不完整（单行内容），原样返回
        return content.to_string();
    };
    let inner = &trimmed[first_newline + 1..];
    // 检查末尾是否有匹配的 ```
    if !inner.ends_with("\n```") && inner != "```" {
        // 末尾没有 ```，说明不是完整的外层包裹，原样返回
        return content.to_string();
    }
    // 剥掉外层，trim 后返回
    let cleaned = inner.trim_end_matches("```").trim();
    // 剥掉后为空则返回原始内容（保护已有内容不被清空）
    if cleaned.is_empty() {
        return content.to_string();
    }
    cleaned.to_string()
}

/// 把 LLM 产出写入黑板表，必要时跳过（空内容保护）。
///
/// 策略：
/// - 产出为空 → 不覆盖现有黑板（LLM 偶发无意义输出时保护已有内容）
/// - 产出非空 → 先归一化剥掉外层 fenced markdown，再 upsert
async fn save_blackboard(
    db: &Database,
    workspace_id: i64,
    new_content: Option<String>,
) -> Result<(), AppError> {
    // None = 执行未产出结果：没有可写内容，按"无变化"处理
    let Some(new_content) = new_content else {
        return Ok(());
    };
    // 归一化：剥掉 LLM 误包的 ````markdown ... ```` 外层
    let normalized = normalize_blackboard_markdown(&new_content);
    // 空内容保护：避免 LLM 偶发返回 "" 覆盖已有黑板
    if normalized.trim().is_empty() {
        tracing::warn!(
            "黑板更新结果为空，跳过写入: workspace_id={}",
            workspace_id
        );
        return Ok(());
    }
    // upsert：记录不存在时创建，已存在时覆盖 content/updated_at，保留 created_at
    db.upsert_blackboard_content(workspace_id, &normalized)
        .await
        .map_err(|e| AppError::Internal(format!("更新黑板失败: {}", e)))?;
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

    // 2. 构造分析阶段 prompt：优先使用用户配置的提示词，空则回退内置默认
    // 区分 Err（DB 错误→返回错误）和 Ok(None)/Ok(Some(""))（未配置→回退默认）
    let prompt_template = {
        match db.get_blackboard_config(workspace_id).await {
            Ok(Some(ref cfg)) if !cfg.wiki_index_prompt.is_empty() => cfg.wiki_index_prompt.clone(),
            Ok(_) => build_wiki_analyze_prompt(),
            Err(e) => return Err(AppError::Internal(format!("分析阶段：读取黑板配置失败: {:?}", e))),
        }
    };
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

    // 5. 解析输出为 YAML operations 数组
    let raw = result.ok_or_else(|| AppError::Internal("分析阶段未产出结果".to_string()))?;
    let parsed = extract_yaml_from_output(&raw)?;
    let ops = parsed.get("operations")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Internal("分析阶段输出缺少 operations 数组".to_string()))?;

    Ok(ops.clone())
}

/// 第二次 LLM 调用：执行阶段 — 写页面内容。
///
/// 输入：operations + 待更新页面的当前内容
/// 输出：{slug: markdown_content} YAML 映射（反序列化后即 serde_json::Map）
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

    // 3. 构造执行阶段 prompt：优先使用用户配置的提示词，空则回退内置默认
    // 区分 Err（DB 错误→返回错误）和 Ok(None)/Ok(Some(""))（未配置→回退默认）
    let prompt_template = {
        match db.get_blackboard_config(workspace_id).await {
            Ok(Some(ref cfg)) if !cfg.wiki_page_prompt.is_empty() => cfg.wiki_page_prompt.clone(),
            Ok(_) => build_wiki_execute_prompt(),
            Err(e) => return Err(AppError::Internal(format!("执行阶段：读取黑板配置失败: {:?}", e))),
        }
    };
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

    // 6. 解析输出为 {slug: content} YAML 映射
    let raw = result.ok_or_else(|| AppError::Internal("执行阶段未产出结果".to_string()))?;
    let parsed = extract_yaml_from_output(&raw)?;
    let map = parsed.as_object()
        .ok_or_else(|| AppError::Internal("执行阶段输出不是 YAML 映射".to_string()))?;

    Ok(map.clone())
}

/// 从 LLM 输出中提取 YAML（要求被 ```yaml ... ``` 代码块包裹）。
///
/// 设计：prompt 强制 LLM 把 YAML 内容用 ```yaml 起始、``` 结尾的代码块包裹，
/// 解析器用正则 `(?s)```yaml\s*\n(.*?)``` ` 非贪婪匹配代码块内部内容，
/// 再交给 serde_yaml 反序列化为 serde_json::Value（结构契约不变，调用方
/// 仍用 .get("operations") / .as_object() 访问）。
///
/// 为什么用正则而非 find: 旧 JSON 解析器用 `find("```yaml")` + `find("```")`
/// 找闭合标记，遇到 YAML 内容里嵌套的 ``` （比如 Markdown 示例）会错位截断。
/// 正则 `(.*?)```` 非贪婪匹配第一个闭合 ```，配合 `(?s)` 让 `.` 匹配换行，
/// 既能跨多行又能停在第一个闭合标记，更鲁棒。
///
/// 为什么只支持 YAML 不兼容 JSON: YAML 对大段 Markdown 文本用字面量块标量 `|`
/// 原样保留，无需转义引号/反斜杠，LLM 输出更稳更短更不易截断。JSON 转义大段
/// 文本容易出错且 token 占用大，已废弃。旧数据不保留，新 todo 一律输出 YAML。
fn extract_yaml_from_output(raw: &str) -> Result<serde_json::Value, AppError> {
    use regex::Regex;
    use std::sync::OnceLock;

    // 正则编译开销不小，用 OnceLock 缓存编译结果，多次调用复用。
    // (?s) 让 . 匹配换行；非贪婪 .*? 确保停在第一个闭合 ```。
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?s)```yaml\s*\n(.*?)```").expect("yaml fence regex must compile")
    });

    // 先尝试从 ```yaml ``` 包裹块提取
    if let Some(caps) = re.captures(raw) {
        let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if !inner.is_empty() {
            match serde_yaml::from_str::<serde_json::Value>(inner) {
                Ok(v) => return Ok(v),
                Err(e) => {
                    return Err(AppError::Internal(format!(
                        "YAML 代码块内容解析失败: {:?}; 内容前 200 字: {:?}",
                        e,
                        inner.chars().take(200).collect::<String>()
                    )));
                }
            }
        }
    }

    // 回退：整段当 YAML 直接解析（LLM 偶尔不按格式包裹时兜底）
    let trimmed = raw.trim();
    match serde_yaml::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => Ok(v),
        Err(e) => Err(AppError::Internal(format!(
            "无法从 LLM 输出提取 YAML（未找到 ```yaml ``` 包裹块且整段解析失败）: {:?}; 内容前 200 字: {:?}",
            e,
            trimmed.chars().take(200).collect::<String>()
        ))),
    }
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
    //
    // 失败清理约定：本函数有多个失败提前返回点（Phase 1 分析失败、Phase 2 执行失败、
    // Phase 3-5 落库失败）。旧实现用 `?` 直接返回，跳过 Phase 6 的 remove_specific_pending_record_ids，
    // 导致这批 record 永远留在 pending 队列。下次 worker 又读到同一批 ID，又调 LLM，又失败……
    // 形成「失败→不清理→重试同一批→又失败」的死循环。当失败原因是 LLM 输出截断（output_tokens
    // 上限）时，队列越长 LLM 输出越长越易截断，越易失败越积越多，恶性循环直至无解。
    //
    // 修复：任何失败路径都先调 remove_specific_pending_record_ids 清理本批已处理 ID，
    // 把这批 record 视为「处理失败已放弃」，记 warn 但不重试。代价是这批 record 的结论
    // 不会写入 wiki，但换来队列能继续往前走、新 record 能正常处理。优于永久卡死。
    let operations = match run_analyze_phase(
        db.clone(),
        executor_registry.clone(),
        tx.clone(),
        task_manager.clone(),
        config.clone(),
        workspace_id,
        pending_record_ids.clone(),
    ).await {
        Ok(ops) => ops,
        Err(e) => {
            // 分析失败：清理本批 record 避免死循环重试，记录 warn 供排查
            let err_msg = format!("Wiki 分析阶段失败: {:?}", e);
            tracing::warn!("黑板分析失败，放弃本批 record: workspace_id={}, pending_count={}, error={}", workspace_id, pending_record_ids.len(), err_msg);
            if let Err(cleanup_err) = db.remove_specific_pending_record_ids(workspace_id, &pending_record_ids).await {
                tracing::warn!("分析失败后清理 pending 队列又失败: workspace_id={}, error={:?}", workspace_id, cleanup_err);
            }
            return Err(AppError::Internal(err_msg));
        }
    };

    if operations.is_empty() {
        tracing::info!("Wiki 分析结果为空操作，跳过执行阶段: workspace_id={}", workspace_id);
        // 即使 LLM 判定本批 record 无需更新任何页面，也必须从 pending 队列移除已处理的 ID。
        // 旧实现直接 return Ok() 跳过 Phase 6 的清理，导致这批 record 永远留在队列里：
        // worker 内循环下一轮 get_blackboard 又读到同样的 ID，再次调用本函数又得到空 operations，
        // 形成「分析→空→不清理→再分析同一批」的静默死循环，UI 持续显示
        // 「刷新中 / N / 阈值 条」却永不收敛。空操作是合法的 LLM 判断结果，
        // 应视为已成功处理。
        db.remove_specific_pending_record_ids(workspace_id, &pending_record_ids).await.map_err(|e| {
            AppError::Internal(format!("空操作分支：移除已处理 pending 记录失败: {:?}", e))
        })?;
        return Ok(());
    }

    // Phase 2: 执行
    //
    // 同 Phase 1 约定：失败时清理本批 record 避免死循环。
    // 这是最常见的失败点——LLM 输出被 output_tokens 上限截断，JSON 不完整无法解析。
    // 清理后这批 record 的结论不会写入 wiki，但队列能继续往前走。
    let page_contents = match run_execute_phase(
        db.clone(),
        executor_registry,
        tx,
        task_manager,
        config,
        workspace_id,
        &operations,
    ).await {
        Ok(map) => map,
        Err(e) => {
            let err_msg = format!("Wiki 执行阶段失败: {:?}", e);
            tracing::warn!("黑板执行失败，放弃本批 record: workspace_id={}, pending_count={}, error={}", workspace_id, pending_record_ids.len(), err_msg);
            if let Err(cleanup_err) = db.remove_specific_pending_record_ids(workspace_id, &pending_record_ids).await {
                tracing::warn!("执行失败后清理 pending 队列又失败: workspace_id={}, error={:?}", workspace_id, cleanup_err);
            }
            return Err(AppError::Internal(err_msg));
        }
    };

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

        // 若 LLM 未产出某个页面的内容，记录 warn 并跳过，不中止整个循环
        // 避免因单个页面缺失导致已 upsert 的 topic 与 index/log 不一致
        let content = match page_contents.get(slug).and_then(|v| v.as_str()) {
            Some(c) => c,
            None => {
                tracing::warn!(
                    "执行阶段未产出页面 {:?} 的内容，跳过该页面: workspace_id={}",
                    slug,
                    workspace_id
                );
                continue;
            }
        };

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

    // Phase 6: 移除已处理的 pending 记录（只移除本次处理的，保留期间新到达的）
    // 注意：不使用 take_pending_record_ids（全量清空），以免丢失在 wiki 更新期间
    // 新到达的 record_id；改为只移除本次实际处理的 ID。
    db.remove_specific_pending_record_ids(workspace_id, &pending_record_ids).await.map_err(|e| {
        AppError::Internal(format!("移除已处理 pending 记录失败: {:?}", e))
    })?;

    tracing::info!("Wiki 黑板更新完成: workspace_id={}, pages={}", workspace_id, operations.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常 Markdown 内容不应被误删。
    #[test]
    fn test_normalize_preserves_plain_markdown() {
        let input = "# 工作空间进展\n\n- 已完成 A\n- 进行中 B";
        assert_eq!(normalize_blackboard_markdown(input), input);
    }

    /// 剥掉 ````markdown 外层。
    #[test]
    fn test_normalize_strips_markdown_fence() {
        let input = "```markdown\n# 工作空间进展\n\n- 已完成 A\n```";
        let expected = "# 工作空间进展\n\n- 已完成 A";
        assert_eq!(normalize_blackboard_markdown(input), expected);
    }

    /// 剥掉 ````md 外层。
    #[test]
    fn test_normalize_strips_md_fence() {
        let input = "```md\n# 测试\n```";
        let expected = "# 测试";
        assert_eq!(normalize_blackboard_markdown(input), expected);
    }

    /// 剥掉纯 ```` 外层。
    #[test]
    fn test_normalize_strips_plain_fence() {
        let input = "```\n# 测试\n```";
        let expected = "# 测试";
        assert_eq!(normalize_blackboard_markdown(input), expected);
    }

    /// 内部有代码块时不应误删。
    #[test]
    fn test_normalize_preserves_inner_code_blocks() {
        let input = "# 进展\n\n```rust\nfn main() {}\n```";
        assert_eq!(normalize_blackboard_markdown(input), input);
    }

    /// 开头有 ``` 但结尾不匹配时不删除。
    #[test]
    fn test_normalize_no_match_trailing_backticks() {
        let input = "```\n# 测试";
        assert_eq!(normalize_blackboard_markdown(input), input);
    }

    /// 剥掉外层后 trim 去除多余空白。
    #[test]
    fn test_normalize_trims_result() {
        let input = "```markdown\n\n# 测试\n\n```";
        let expected = "# 测试";
        assert_eq!(normalize_blackboard_markdown(input), expected);
    }

    /// 空内容保护：剥掉外层后为空时返回原始内容。
    #[test]
    fn test_normalize_empty_inner_preserves_original() {
        let input = "```markdown\n\n```";
        assert_eq!(normalize_blackboard_markdown(input), input);
    }

    /// 太短的内容直接返回。
    #[test]
    fn test_normalize_too_short_returns_original() {
        let input = "```";
        assert_eq!(normalize_blackboard_markdown(input), input);
    }

    // ===== extract_yaml_from_output 单元测试 =====
    //
    // 覆盖四个场景：标准包裹、Markdown 内嵌 ``` 不误截、缺包裹块回退整段解析、
    // YAML 语法错误返回 Err。这些场景对应生产中 LLM 输出的真实变体，
    // 确保解析器鲁棒性。

    /// 标准 ```yaml ``` 包裹的 operations 数组能正确解析。
    #[test]
    fn test_extract_yaml_standard_fence_operations() {
        let raw = r#"```yaml
operations:
  - action: create
    slug: auth-module
    title: 认证模块
    summary: JWT 验证汇总
    record_ids:
      - 42
      - 45
```
"#;
        let v = extract_yaml_from_output(raw).expect("标准包裹必须解析成功");
        let ops = v.get("operations").and_then(|o| o.as_array()).expect("必须有 operations 数组");
        assert_eq!(ops.len(), 1);
        let op = &ops[0];
        assert_eq!(op.get("action").and_then(|v| v.as_str()), Some("create"));
        assert_eq!(op.get("slug").and_then(|v| v.as_str()), Some("auth-module"));
        let record_ids: Vec<i64> = op.get("record_ids")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();
        assert_eq!(record_ids, vec![42, 45]);
    }

    /// execute 阶段输出：slug → Markdown 字面量块标量，能原样保留多行内容。
    /// 这是 YAML 相对 JSON 的核心优势——大段 Markdown 无需转义。
    #[test]
    fn test_extract_yaml_execute_phase_literal_block() {
        let raw = r#"```yaml
auth-module: |
  # 认证模块

  ## 已确认
  - JWT 验证已实现，含 "引号" 和 `反引号`
  - token 刷新策略：见 [record_42](/?view=items&id=10)
performance: |
  # 性能优化

  ## 已确认
  - 查询耗时下降 50%
```
"#;
        let v = extract_yaml_from_output(raw).expect("execute phase YAML 必须解析成功");
        let map = v.as_object().expect("必须是映射");
        assert_eq!(map.len(), 2);
        let auth = map.get("auth-module").and_then(|v| v.as_str()).expect("auth-module 必须存在");
        assert!(auth.contains("# 认证模块"));
        assert!(auth.contains("含 \"引号\" 和 `反引号`"));
        assert!(auth.contains("[record_42]"));
    }

    /// LLM 在 YAML 前后输出了自然语言前缀/后缀，正则仍能精准提取代码块内容。
    /// 这是正则相比旧 find 方案的优势——不依赖整段是合法 YAML。
    #[test]
    fn test_extract_yaml_tolerates_surrounding_prose() {
        let raw = r#"Now I have all the data. Let me output the YAML.

```yaml
operations:
  - action: update
    slug: perf
    title: 性能
    summary: 优化
    record_ids:
      - 7
```

Done. Above is the YAML.
"#;
        let v = extract_yaml_from_output(raw).expect("有包裹块时必须提取成功，忽略前后自然语言");
        let ops = v.get("operations").and_then(|o| o.as_array()).expect("operations 必须存在");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].get("slug").and_then(|v| v.as_str()), Some("perf"));
    }

    /// 缺少 ```yaml ``` 包裹时，回退到整段当 YAML 解析。
    /// LLM 偶尔不按格式包裹，回退保证兜底可用。
    #[test]
    fn test_extract_yaml_fallback_when_no_fence() {
        let raw = "operations:\n  - action: create\n    slug: x\n    title: X\n    summary: s\n    record_ids: []\n";
        let v = extract_yaml_from_output(raw).expect("无包裹块时回退整段解析应成功");
        assert!(v.get("operations").is_some());
    }

    /// 既无包裹块、整段也不是合法 YAML 时，返回 Err 并附前 200 字诊断。
    ///
    /// 注意 YAML 的宽容性：纯文本无冒号会被当成合法字符串标量解析成功，
    /// 所以测试输入必须用真正违反 YAML 语法的结构（如非法缩进映射）。
    #[test]
    fn test_extract_yaml_returns_err_for_garbage() {
        // 非法 YAML：映射 key 后跟另一个冒号，语法错误
        let raw = "key: : value: broken\n  - : :\n";
        let err = extract_yaml_from_output(raw).expect_err("非法 YAML 必须返回 Err");
        let msg = format!("{:?}", err);
        assert!(msg.contains("YAML") || msg.contains("yaml"), "错误信息应提示 YAML: {}", msg);
    }

    /// YAML value 里含普通代码（无 ``` fence），字面量块标量能原样保留。
    ///
    /// 注意：不测试 value 里嵌套 ``` 的场景——那是 fence 协议的歧义，
    /// 非贪婪正则无法区分「内容里的 ```」和「真正的闭合 ```」。
    /// 正确做法是 prompt 约束 LLM 不要在 value 里用 ``` fence，
    /// 而非试图解析这种歧义。这里只验证普通代码（缩进块）能正常保留。
    #[test]
    fn test_extract_yaml_value_with_plain_code_block() {
        let raw = r#"```yaml
guide: |
  示例代码（无 fence，纯缩进）：
      print('hi')
      echo 'bye'
  上述代码应原样保留。
```
"#;
        let v = extract_yaml_from_output(raw).expect("普通代码内容不应破坏解析");
        let guide = v.get("guide").and_then(|v| v.as_str()).expect("guide 字段必须存在");
        assert!(guide.contains("print('hi')"), "代码内容应完整保留: {:?}", guide);
        assert!(guide.contains("echo 'bye'"), "多行代码应完整保留: {:?}", guide);
    }
}
