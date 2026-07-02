//! 自动评审（auto-review）—— 同步派生一个评审 todo，给刚完成的那条执行记录打分。
//!
//! 调用点: `finalize_normal_completion` 在 `update_execution_record`（写终态）之后。
//! 仅当:
//!   - 源 todo 是 normal 类型 (todo_type=0)
//!   - `auto_review_enabled=true`
//!   - 源 record 进入了 success 或 failed 终态
//!   - `source_execution_record_id` 尚未被设置（避免重复评审同一条记录）
//! 才启动评审。
//!
//! V15 之后评审模板是独立表（`review_templates`），不带 executor 字段。
//! 评审时新建一个 todo_type=2 的"评审实例" todo, prompt 用 caller 合成好的
//! `composed_prompt`, executor 继承自源 todo。
//!
//! 为避免与 `run_todo_execution` 的内部逻辑产生循环引用，这里用一个简化的
//! 同步路径：等 `run_todo_execution` 启动后创建的 record 进入终态，再解析 rating 回填。

use std::sync::{Arc, OnceLock};

use tokio::sync::broadcast;

use crate::db::Database;
use crate::executor_service::ExecEvent;
use crate::task_manager::TaskManager;

use super::RunTodoExecutionRequest;

/// 独立 runtime. 用于 run_auto_review 在原 todo 的 spawned task 内部同步运行
/// 自动评审逻辑, 避免与外层 spawned task 产生 Send / 嵌套 spawn 问题.
fn review_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("auto-review-runtime")
            .build()
            .expect("failed to build auto-review runtime")
    })
}

/// 同步运行自动评审。在原 todo 执行完成、update_execution_record 写入 success/failed 后调用。
///
/// 参数: (db, todo, record_id, executor_registry, tx, task_manager, config).
/// 任何错误都只记 warn 日志，不影响原 todo 的完成响应。
///
/// 实现: 由于 `run_auto_review_inner` 内部需要 await `run_todo_execution`（后者会
/// 进一步 spawn）—— 整个 future 不是 Send —— 必须在独立 runtime 上 block_on.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_auto_review(
    db: Arc<Database>,
    executor_registry: Arc<crate::adapters::ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
    todo_id: i64,
    record_id: i64,
) {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let db_c = db.clone();
    let er_c = executor_registry.clone();
    let tx_c = tx.clone();
    let tx_outer = tx.clone();
    let tm_c = task_manager.clone();
    let cfg_c = config.clone();
    let runtime = review_runtime();
    std::thread::spawn(move || {
        let result = runtime.block_on(run_auto_review_inner(
            db_c, er_c, tx_c, tm_c, cfg_c, todo_id, record_id,
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
            mark_review_failed(&db, &tx_outer, record_id, todo_id).await;
        }
        Err(_) => {
            tracing::warn!(
                "auto-review thread dropped reply for todo #{} record #{}",
                todo_id, record_id
            );
            mark_review_failed(&db, &tx_outer, record_id, todo_id).await;
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
#[allow(clippy::too_many_arguments)]
async fn run_auto_review_inner(
    db: Arc<Database>,
    executor_registry: Arc<crate::adapters::ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<crate::config::Config>>,
    todo_id: i64,
    record_id: i64,
) -> Result<(), String> {
    // 1) 加载原 todo + 校验是否需要跳过 review。
    let original = load_original_todo(&db, todo_id).await?;
    if !should_review_todo(&original) {
        mark_review_skipped(&db, &tx, record_id, todo_id).await;
        return Ok(());
    }

    // 2) 加载 source record + 校验 record 状态。
    let record = load_and_validate_record(&db, &tx, todo_id, record_id).await?;

    // 3) 准备评审模板（review_templates 表行，不带 executor 字段）。
    let template = ensure_review_template(&db.clone()).await?;

    // 4) 合并 prompt（截断原 output + 替换模板占位符）。
    let composed_prompt =
        compose_review_prompt(&original, &template, record.result.as_deref());

    // 5) 标记 pending。
    mark_review_pending(&db, &tx, record_id, todo_id).await;

    // 6) 执行评审实例（创建 todo_type=2 的评审实例 todo + 复用 run_todo_execution）。
    let review_record_id = match execute_review_instance(
        &db,
        &executor_registry,
        &tx,
        &task_manager,
        &config,
        &original,
        &template,
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

/// 准备评审模板：从 review_templates 表拿默认模板（确保存在 + reload 拿到最新内容）。
/// V15 之后模板是独立表, 没有 executor 字段；executor 由 caller 从源 todo 继承。
async fn ensure_review_template(db: &Arc<Database>) -> Result<crate::models::ReviewTemplate, String> {
    let template_id = db
        .ensure_default_review_template()
        .await
        .map_err(|e| format!("ensure default review template: {}", e))?;
    db.get_review_template(template_id)
        .await
        .map_err(|e| format!("reload template: {}", e))?
        .ok_or_else(|| "reviewer template vanished".to_string())
}

/// Step 4: 合并评审 prompt（截断原 output + 替换模板占位符）。
fn compose_review_prompt(
    original: &crate::models::Todo,
    template: &crate::models::ReviewTemplate,
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
    template
        .prompt
        .replace("{original_prompt}", &original.prompt)
        .replace("{max_output_chars}", &MAX_OUTPUT_CHARS.to_string())
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
#[allow(clippy::too_many_arguments)]
async fn execute_review_instance(
    db: &Arc<Database>,
    executor_registry: &Arc<crate::adapters::ExecutorRegistry>,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &Arc<TaskManager>,
    config: &Arc<std::sync::RwLock<crate::config::Config>>,
    original: &crate::models::Todo,
    template: &crate::models::ReviewTemplate,
    composed_prompt: String,
) -> Result<i64, String> {
    // 复用策略:同一 review_template 全局共享一条评审实例 todo,
    // 避免「每次评审都新建 todo」把 todos 表刷成同一评审 N 份。
    // - 已有 → 重置 prompt/executor/status(保留 id 和 execution_records 关联)
    // - 没有 → 新建
    let review_todo_id = match db
        .find_review_instance_by_template(template.id)
        .await
        .map_err(|e| format!("find review instance: {}", e))?
    {
        Some(existing) => {
            db.reset_review_instance_for_reuse(
                existing.id,
                &composed_prompt,
                original.executor.as_deref(),
            )
            .await
            .map_err(|e| format!("reset review instance: {}", e))?;
            existing.id
        }
        None => db
            .create_review_instance_todo(
                original.id,
                template.id,
                &template.name,
                composed_prompt.clone(),
                original.executor.clone(),
            )
            .await
            .map_err(|e| format!("create review instance todo: {}", e))?,
    };

    let request = RunTodoExecutionRequest {
        db: db.clone(),
        executor_registry: executor_registry.clone(),
        tx: tx.clone(),
        task_manager: task_manager.clone(),
        config: config.clone(),
        todo_id: review_todo_id,
        message: composed_prompt,
        req_executor: original.executor.clone(),
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
        workspace_path: None,
        workspace_id: None,
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