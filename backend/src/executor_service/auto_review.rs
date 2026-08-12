//! 自动评审（auto-review）—— 同步派生一个评审 todo，给刚完成的那条执行记录打分。
//!
//! ## 统一评审路径（设计 034）
//!
//! 自设计 034 起，所有自动评审统一通过此模块执行。评审 prompt 三级回退：
//!
//! 1. **环节内联**（`loop_steps.review_prompt`）非空 → 用其作为模板正文
//! 2. **环路级**（`loops.review_template_id`）有值 → 查 `review_templates` 表
//! 3. **全局默认** → `ensure_default_review_template()` 行
//!
//! `maybe_run_auto_review` 是唯一入口，对 `loop_stage` 触发查出 step/loop 上下文
//! 传入 `run_auto_review_inner`，非环路 todo 只用默认模板。
//!
//! V15 之后评审模板是独立表（`review_templates`），不带 executor 字段。
//! 评审时新建一个 todo_type=2 的"评审实例" todo, prompt 用 caller 合成好的
//! `composed_prompt`, executor 继承自源 todo。
//!
//! 为避免与 `run_todo_execution` 的内部逻辑产生循环引用，这里用一个简化的
//! 同步路径：等 `run_todo_execution` 启动后创建的 record 进入终态，再解析 rating 回填。

use std::sync::OnceLock;

use tokio::sync::broadcast;

use crate::db::Database;
use crate::executor_service::ExecEvent;

use super::RunTodoExecutionRequest;

/// 哨兵 template_id，用于环节内联评审模板（无对应 review_templates 行）。
/// 不与真实模板冲突（真实模板 id 从 1 起），review_template_id 为逻辑引用非 FK。
///
/// `pub(crate)` 让端到端穿透测试可断言「环节内联用哨兵 id 0 归属评审实例」。
pub(crate) const INLINE_REVIEW_TEMPLATE_ID: i64 = 0;

/// 独立 runtime. 用于 run_auto_review 在原 todo 的 spawned task 内部同步运行
/// 自动评审逻辑, 避免与外层 spawned task 产生 Send / 嵌套 spawn 问题.
/// 返回 Option 而非直接 panic：runtime 构建失败时调用方跳过自动评审而非 crash。
fn review_runtime() -> Option<&'static tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    // OnceLock::get_or_init 闭包必须返回值，无法用 Result 传播；
    // 此处是进程启动时一次性初始化，失败意味着系统环境异常，panic 合理。
    #[allow(clippy::expect_used)]
    Some(RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("auto-review-runtime")
            .build()
            .expect("failed to build auto-review runtime")
    }))
}

/// 同步运行自动评审。在原 todo 执行完成、update_execution_record 写入 success/failed 后调用。
///
/// 参数:
///   - `deps`: 执行链路共享依赖五元组（096-W2-PR3 参数对象化，9 参塌缩为 5 参）
///   - `step_review_prompt`: 环节内联评审模板正文（空 = 未设，回退环路/默认）
///   - `loop_review_template_id`: 环路级评审模板 ID（本参数优先于全局默认）
///
/// 任何错误都只记 warn 日志，不影响原 todo 的完成响应。
///
/// 实现: 由于 `run_auto_review_inner` 内部需要 await `run_todo_execution`（后者会
/// 进一步 spawn）—— 整个 future 不是 Send —— 必须在独立 runtime 上 block_on。
pub(crate) async fn run_auto_review(
    deps: super::types::ExecutionDeps,
    todo_id: i64,
    record_id: i64,
    step_review_prompt: Option<String>,
    loop_review_template_id: Option<i64>,
) {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    // deps 整体 clone 进独立线程（Arc 字段廉价），tx 单独留一份给外层失败标记
    let deps_c = deps.clone();
    let tx_outer = deps.tx.clone();
    let runtime = match review_runtime() {
        Some(r) => r,
        None => {
            tracing::warn!("auto-review runtime unavailable, skipping review for todo #{}", todo_id);
            return;
        }
    };
    std::thread::spawn(move || {
        let result = runtime.block_on(run_auto_review_inner(
            deps_c,
            todo_id, record_id,
            step_review_prompt, loop_review_template_id,
        ));
        let _ = reply_tx.send(result);
    });
    match reply_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!(
                "auto-review for todo #{} record #{} failed: {}",
                todo_id, record_id, e
            );
            mark_review_failed(&deps.db, &tx_outer, record_id, todo_id).await;
        }
        Err(_) => {
            tracing::warn!(
                "auto-review thread dropped reply for todo #{} record #{}",
                todo_id, record_id
            );
            mark_review_failed(&deps.db, &tx_outer, record_id, todo_id).await;
        }
    }
}

/// 把 review status 标记为 failed 并 emit ReviewStatusChanged 事件。
async fn mark_review_failed(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    record_id: i64,
    todo_id: i64,
) {
    let _ = db
        .set_record_last_review_status(record_id, "failed")
        .await;
    let _ = tx.send(ExecEvent::ReviewStatusChanged {
        record_id,
        todo_id,
        review_status: "failed".to_string(),
    });
}

/// 把 review status 标记为 skipped 并 emit ReviewStatusChanged 事件。
async fn mark_review_skipped(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    record_id: i64,
    todo_id: i64,
) {
    let _ = db
        .set_record_last_review_status(record_id, "skipped")
        .await;
    let _ = tx.send(ExecEvent::ReviewStatusChanged {
        record_id,
        todo_id,
        review_status: "skipped".to_string(),
    });
}

/// auto_review 的内部实现：在独立 runtime 上同步跑评审实例并轮询终态。
///
/// 拆分为 7 步独立 helper，每个 ≤ 30 行；任一步提前返回前都正确清理状态。
async fn run_auto_review_inner(
    // 096-W2-PR3：五元组依赖已对象化（owned——本 future 被 move 进独立 runtime 线程）
    deps: super::types::ExecutionDeps,
    todo_id: i64,
    record_id: i64,
    // 设计 034：环节内联评审模板正文（空 = 未设，回退环路/默认）
    step_review_prompt: Option<String>,
    loop_review_template_id: Option<i64>,
) -> Result<(), String> {
    // 从 deps 解出与原签名同名的 owned 局部量（Arc::clone 廉价，仅原子计数自增），
    // 保持函数体零改动（093-B3 既有范式）；
    // executor_registry/task_manager/config 在 execute_review_instance 调用点收口后
    // 本函数体内不再直接使用，故不解包（避免死局部量）
    let db = deps.db.clone();
    let tx = deps.tx.clone();
    // 1) 加载原 todo + 校验是否需要跳过 review。
    let original = load_original_todo(&db, todo_id).await?;
    if !should_review_todo(&original) {
        mark_review_skipped(&db, &tx, record_id, todo_id).await;
        return Ok(());
    }

    // 2) 加载 source record + 校验 record 状态。
    let record = load_and_validate_record(&db, &tx, todo_id, record_id).await?;

    // 3) 解析评审模板（三级回退：环节内联 → 环路级 → 全局默认）。
    let resolved = resolve_review_template(
        &db,
        step_review_prompt.as_deref(),
        loop_review_template_id,
    )
    .await?;
    let (template_prompt, owning_template_id, owning_template_name) = resolved;

    // 4) 合并 prompt（截断原 output + 替换模板占位符）。
    let composed_prompt =
        compose_review_prompt(&original, &template_prompt, record.result.as_deref());

    // 5) 标记 pending。
    mark_review_pending(&db, &tx, record_id, todo_id).await;

    // 6) 执行评审实例（创建 todo_type=2 的评审实例 todo + 复用 run_todo_execution）。
    let review_record_id = match execute_review_instance(
        &deps,
        &original,
        owning_template_id,
        &owning_template_name,
        composed_prompt,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            mark_review_failed(&db, &tx, record_id, todo_id).await;
            return Err(e);
        }
    };

    // 7) 轮询评审实例 record 的终态 + 写回 rating。
    poll_review_to_terminal(&db, &tx, record_id, todo_id, review_record_id).await;
    Ok(())
}

/// 加载原 todo；找不到或 DB 错误时返回错误。
async fn load_original_todo(db: &Database, todo_id: i64) -> Result<crate::models::Todo, String> {
    match db.get_todo(todo_id).await {
        Ok(Some(t)) => Ok(t),
        Ok(None) => Err(format!("original todo #{} not found", todo_id)),
        Err(e) => Err(format!("load original todo: {}", e)),
    }
}

/// 是否应启动 review？仅当 todo_type=0 且 auto_review_enabled=true 才需要。
fn should_review_todo(todo: &crate::models::Todo) -> bool {
    todo.todo_type == 0 && todo.auto_review_enabled
}

/// 加载 source record 并校验状态。
///
/// record 不存在 / DB 错误 / record 未进入终态 / 已被评审过 都走 early return；
/// 前三种情况会触发 mark_review_skipped + 返回 Ok(())。
async fn load_and_validate_record(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    todo_id: i64,
    record_id: i64,
) -> Result<crate::models::ExecutionRecord, String> {
    use crate::models::ExecutionStatus;
    let record = match db.get_execution_record(record_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return Err(format!("record #{} not found", record_id)),
        Err(e) => return Err(format!("load record: {}", e)),
    };
    if !matches!(record.status, ExecutionStatus::Success | ExecutionStatus::Failed) {
        mark_review_skipped(db, tx, record_id, todo_id).await;
        return Err("skipped: record not in terminal state".to_string());
    }
    if record.last_review_status.as_deref() == Some("success") {
        // 避免重复评审；不算错误，直接返回。
        return Err("skipped: already reviewed".to_string());
    }
    Ok(record)
}

/// 获取评审模板：优先使用指定 template_id，否则用默认模板。
/// V15 之后模板是独立表 (review_templates) 里的行, 不带 executor 字段。
async fn ensure_review_template_by_id(db: &Database, template_id: Option<i64>) -> Result<crate::models::ReviewTemplate, String> {
    // 如果指定了模板 id，先尝试加载。
    if let Some(tid) = template_id {
        if let Some(t) = db.get_review_template(tid).await.map_err(|e| format!("load template: {}", e))? {
            return Ok(t);
        }
        tracing::warn!("review template #{} not found, falling back to default", tid);
    }
    // 回退到默认模板
    let default_id = db
        .ensure_default_review_template()
        .await
        .map_err(|e| format!("ensure default review template: {}", e))?;
    db.get_review_template(default_id)
        .await
        .map_err(|e| format!("reload template: {}", e))?
        .ok_or_else(|| "default reviewer template vanished".to_string())
}

/// 三级回退解析评审模板正文（设计 034）。
/// 顺序：step_review_prompt → loop 级 review_template_id → 全局默认。
///
/// 返回 (模板正文, 归属 template_id, 归属名称)。
/// 环节内联模板用哨兵 id `INLINE_REVIEW_TEMPLATE_ID`（0）归属评审实例 todo。
pub async fn resolve_review_template(
    db: &Database,
    step_review_prompt: Option<&str>,
    loop_review_template_id: Option<i64>,
) -> Result<(String /*prompt*/, i64 /*owning_id*/, String /*owning_name*/), String> {
    // 1) 环节内联评审模板：非空时直接作为正文，用哨兵 id 0 归属。
    if let Some(prompt) = step_review_prompt.filter(|s| !s.trim().is_empty()) {
        return Ok((prompt.to_string(), INLINE_REVIEW_TEMPLATE_ID, "环节内联评审".to_string()));
    }
    // 2) 回退到环路级/默认评审模板
    let t = ensure_review_template_by_id(db, loop_review_template_id).await?;
    Ok((t.prompt, t.id, t.name))
}

/// Step 4: 合并评审 prompt（截断原 output + 替换模板占位符）。
/// `template_prompt` 是解析后的模板正文（可以是默认模板 prompt 或环节内联 review_prompt）。
///
/// 占位符兼容：同时支持单大括号 `{original_prompt}`（历史模板）和双大括号
/// `{{original_prompt}}`（设计 029 引入），先试双大括号、未命中再试单大括号，
/// 确保存量 DB 行和新模板都能正确替换。
///
/// `pub(crate)` 让端到端穿透测试（installer_tests）可直接调用，
/// 复用同一段占位符替换逻辑，避免在测试里复制实现导致语义漂移。
pub(crate) fn compose_review_prompt(
    original: &crate::models::Todo,
    template_prompt: &str,
    original_output: Option<&str>,
) -> String {
    use crate::services::auto_review::MAX_OUTPUT_CHARS;
    let original_output = original_output.unwrap_or_default();
    let truncated: String = if original_output.chars().count() > MAX_OUTPUT_CHARS {
        let mut s: String = original_output.chars().take(MAX_OUTPUT_CHARS).collect();
        s.push_str("\n\n[...以下被截断...]");
        s
    } else {
        original_output.to_string()
    };
    let acceptance_criteria = original
        .acceptance_criteria
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("(无验收标准 —— 由评审师自行判断输出质量)");

    // 兼容历史单大括号和新双大括号两种占位符：
    // 先替换双大括号（新模板），再替换单大括号（存量模板）。
    // 若模板同时包含两种写法，双大括号先被替换后不会误伤单大括号版本。
    let prompt = original.prompt.clone();
    let max_chars_str = MAX_OUTPUT_CHARS.to_string();
    template_prompt
        .replace("{{original_prompt}}", &prompt)
        .replace("{{max_output_chars}}", &max_chars_str)
        .replace("{{original_output}}", &truncated)
        .replace("{{acceptance_criteria}}", acceptance_criteria)
        // 存量模板兼容：单大括号占位符（PR #945 之前的写法）
        .replace("{original_prompt}", &prompt)
        .replace("{max_output_chars}", &max_chars_str)
        .replace("{original_output}", &truncated)
        .replace("{acceptance_criteria}", acceptance_criteria)
}

/// Step 5: 标记 review pending + emit event。
async fn mark_review_pending(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    record_id: i64,
    todo_id: i64,
) {
    let _ = db.set_record_last_review_status(record_id, "pending").await;
    let _ = db.set_record_last_reviewed_at(record_id).await;
    let _ = tx.send(ExecEvent::ReviewStatusChanged {
        record_id,
        todo_id,
        review_status: "pending".to_string(),
    });
}

/// Step 6: 同步执行评审实例 —— 创建一个 todo_type=2 的评审实例 todo,
/// 再用 [`super::run_todo_execution`] 跑它。
///
/// V15 之后评审模板独立成表 (不带 executor), 评审实例的 executor
/// 继承自源 todo (review_instance.executor = original.executor)。
///
/// 参数 `owning_template_id`：环节内联模板用 `INLINE_REVIEW_TEMPLATE_ID`（0），
/// 查表模板用 `review_templates.id`。`owning_template_name` 供创建时写标题。
async fn execute_review_instance(
    // 096-W2-PR3：五元组依赖已对象化（借用现场按引用接参）
    deps: &super::types::ExecutionDeps,
    original: &crate::models::Todo,
    owning_template_id: i64,
    owning_template_name: &str,
    composed_prompt: String,
) -> Result<i64, String> {
    // 从 deps 解出与原签名同名的 owned 局部量（Arc::clone 廉价，仅原子计数自增），
    // 保持函数体零改动（093-B3 既有范式）
    let db = deps.db.clone();
    let executor_registry = deps.executor_registry.clone();
    let tx = deps.tx.clone();
    let task_manager = deps.task_manager.clone();
    let config = deps.config.clone();
    // 复用策略：同一 review_template 全局共享一条评审实例 todo,
    // 避免「每次评审都新建 todo」把 todos 表刷成同一评审 N 份。
    // - 已有 → 重置 prompt/executor/status（保留 id 和 execution_records 关联）
    // - 没有 → 新建
    let ws = original.workspace_id.unwrap_or(0);
    let review_todo_id = match db
        .find_review_instance_by_template(owning_template_id)
        .await
        .map_err(|e| format!("find review instance: {}", e))?
    {
        Some(existing) => {
            db.reset_review_instance_for_reuse(
                existing.id,
                &composed_prompt,
                original.executor.as_deref(),
                ws,
            )
            .await
            .map_err(|e| format!("reset review instance: {}", e))?;
            existing.id
        }
        None => {
            db.create_review_instance_todo(
                original.id,
                owning_template_id,
                owning_template_name,
                composed_prompt.clone(),
                original.executor.clone(),
                ws,
            )
            .await
            .map_err(|e| format!("create review instance todo: {}", e))?
        }
    };

    let request = RunTodoExecutionRequest {
        db: db.clone(),
        executor_registry: executor_registry.clone(),
        tx: tx.clone(),
        task_manager: task_manager.clone(),
        config: config.clone(),
        blackboard_debouncer: deps.blackboard_debouncer.clone(),
        todo_id: review_todo_id,
        message: composed_prompt,
        req_executor: original.executor.clone(),
        req_model: None,
        trigger_type: "auto_review".to_string(),
        params: None,
        resume_session_id: None,
        resume_message: None,
        source_todo_id: Some(original.id),
        source_todo_title: Some(original.title.clone()),
        loop_step_execution_id: None,
        step_id: None,
        feishu_bot_id: None,
        feishu_receive_id: None,
            feishu_receive_id_type: None,
        workspace_path: None,
        workspace_id: original.workspace_id, // v89: 评审 record 归属原 todo 的 ws，不经间接关联
        // auto_review 创建的评审 todo 不继承原 todo 的 expert_name，
        // 传 None 跳过专家上下文注入，避免给评审 instance 错误加载原 todo 的专家
        expert_manager: None,
    };
    let exec_result = super::run_todo_execution(request).await;
    exec_result
        .record_id
        .ok_or_else(|| "review execution produced no record (rejected?)".to_string())
}

/// Step 7: 轮询评审实例 record 的终态，解析 rating 写回 source record。
async fn poll_review_to_terminal(
    db: &Database,
    tx: &broadcast::Sender<ExecEvent>,
    record_id: i64,
    todo_id: i64,
    review_record_id: i64,
) {
    use crate::models::ExecutionStatus;
    use crate::services::auto_review::parse_rating_from_result;

    let max_wait = std::time::Duration::from_secs(300);
    let poll = std::time::Duration::from_millis(500);
    let start = std::time::Instant::now();
    let final_review = loop {
        if start.elapsed() > max_wait {
            tracing::warn!("auto-review record #{} timed out", review_record_id);
            let _ = db
                .set_record_last_review_status(record_id, "failed")
                .await;
            let _ = tx.send(ExecEvent::ReviewStatusChanged {
                record_id,
                todo_id,
                review_status: "failed".to_string(),
            });
            return;
        }
        if let Ok(Some(rec)) = db.get_execution_record(review_record_id).await {
            if !matches!(rec.status, ExecutionStatus::Running) {
                break rec;
            }
        }
        tokio::time::sleep(poll).await;
    };

    let review_status_str = match final_review.status {
        ExecutionStatus::Success => "success",
        ExecutionStatus::Failed => "failed",
        _ => "interrupted",
    };
    let rating = parse_rating_from_result(final_review.result.as_deref());
    if let Some(r) = rating {
        let _ = db.update_execution_record_rating(record_id, Some(r)).await;
    }
    let _ = db
        .link_review_to_source(review_record_id, record_id, review_status_str)
        .await;
    let _ = db
        .set_record_last_review_status(record_id, review_status_str)
        .await;
    let _ = tx.send(ExecEvent::ReviewStatusChanged {
        record_id,
        todo_id,
        review_status: review_status_str.to_string(),
    });
    tracing::info!(
        "auto-review done: original_todo=#{} record=#{} review_record=#{} status={} rating={:?}",
        todo_id, record_id, review_record_id, review_status_str, rating
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::compose_review_prompt;
    use crate::models::Todo;

    fn sample_todo(prompt: &str, criteria: Option<&str>) -> Todo {
        Todo {
            id: 1,
            title: "test".to_string(),
            prompt: prompt.to_string(),
            status: crate::models::TodoStatus::Pending,
            created_at: Default::default(),
            updated_at: Default::default(),
            tag_ids: Vec::new(),
            executor: None,
            scheduler_enabled: false,
            scheduler_config: None,
            scheduler_timezone: None,
            scheduler_next_run_at: None,
            task_id: None,
            workspace_path: None,
            workspace_id: None,
            webhook_enabled: false,
            acceptance_criteria: criteria.map(String::from),
            todo_type: 0,
            parent_todo_id: None,
            review_template_id: None,
            auto_review_enabled: true,
            action_type: None,
            action_key: None,
            archived_at: None,
            expert_name: None,
            model: None,
            skills: Vec::new(),
        }
    }

    #[test]
    fn test_compose_double_braces_replaces_all() {
        // 新模板：双大括号占位符
        let template = "任务: {{original_prompt}}\n截断: {{max_output_chars}}\n输出: {{original_output}}\n标准: {{acceptance_criteria}}";
        let todo = sample_todo("do something", Some("must work"));
        let result = compose_review_prompt(&todo, template, Some("actual output"));
        assert!(result.contains("任务: do something"));
        assert!(result.contains("截断: 8000"));
        assert!(result.contains("输出: actual output"));
        assert!(result.contains("标准: must work"));
    }

    #[test]
    fn test_compose_single_braces_replaces_all() {
        // 存量模板：单大括号占位符（BUG-002 复现场景）
        let template = "任务: {original_prompt}\n截断: {max_output_chars}\n输出: {original_output}\n标准: {acceptance_criteria}";
        let todo = sample_todo("do something", Some("must work"));
        let result = compose_review_prompt(&todo, template, Some("actual output"));
        assert!(result.contains("任务: do something"));
        assert!(result.contains("截断: 8000"));
        assert!(result.contains("输出: actual output"));
        assert!(result.contains("标准: must work"));
    }

    #[test]
    fn test_compose_no_output_uses_default() {
        let template = "{{original_output}}";
        let todo = sample_todo("task", None);
        let result = compose_review_prompt(&todo, template, None);
        assert_eq!(result, "");
    }

    #[test]
    fn test_compose_no_criteria_uses_fallback() {
        let template = "{{acceptance_criteria}}";
        let todo = sample_todo("task", None);
        let result = compose_review_prompt(&todo, template, Some("output"));
        assert!(result.contains("无验收标准"));
    }

    #[test]
    fn test_compose_mixed_braces_both_replaced() {
        // 极端情况：模板混用两种占位符（不应发生，但兼容）
        let template = "{{original_prompt}} + {acceptance_criteria}";
        let todo = sample_todo("task", Some("must work"));
        let result = compose_review_prompt(&todo, template, Some("out"));
        assert!(result.contains("task"));
        assert!(result.contains("must work"));
    }
}