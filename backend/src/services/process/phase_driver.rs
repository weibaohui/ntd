//! 阶段驱动器 —— 环节执行完成后的总控单元。
//!
//! 职责：协调产物捕获、门禁评价、流转解析、返工统计、阶段状态维护，
//! 实现从「原始执行记录」到「gate_passed / next_idx / rework_count」的完整链路。
//!
//! 设计意图：把 LoopRunner 的「4h ~ 4l」内联逻辑替换为一次 `PhaseDriver::execute_step` 调用。

use std::sync::Arc;
use std::collections::HashMap;

use sea_orm::{ActiveValue, ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};

use crate::db::Database;
use crate::db::entity::{loop_step_executions, loop_steps, loop_step_artifacts};
use crate::models::ExecutionRecord;
use crate::services::process::artifact_capture::{self, ArtifactSpec};
use crate::services::process::gate_evaluator::{self, GateSummary};
use crate::services::process::rework_tracker;
use crate::services::process::transition_resolver;
use crate::services::process::GateDefinition;

/// 步骤执行结果，供 LoopRunner 直接消费。
#[derive(Debug)]
pub struct StepOutcome {
    /// 所有门禁是否通过（仅 `human_approval` 返回 false 但非失败，见 `paused`）。
    pub gate_passed: bool,
    /// AI 评审得分。
    pub rating: Option<i32>,
    /// 门禁失败原因。
    pub error_message: Option<String>,
    /// 下一步环节索引；`None` 表示终止。
    pub next_idx: Option<usize>,
    /// 是否因 `human_approval` 暂停等待人工审批。
    pub paused: bool,
    /// 本次执行后的返工计数。
    pub rework_count: i32,
    /// 已捕获的产物模型列表。
    pub artifacts: Vec<loop_step_artifacts::Model>,
    /// 门禁评价结果。
    pub gate_summary: Option<GateSummary>,
}

/// 执行一次步骤的完整工艺驱动逻辑。
///
/// # 参数
/// - `db`：数据库访问。
/// - `execution_record`：本次步骤的 `execution_record`（含有 `result` 文本）。
/// - `loop_execution_id`: `loop_executions.id`。
/// - `step`: 当前环节定义。
/// - `step_exec`: 当前环节的执行记录（`loop_step_executions.Model`）。
/// - `all_steps`: 本 loop 的所有环节列表。
/// - `current_idx`: 当前环节在 `all_steps` 中的索引。
/// - `workspace_path`: 工作空间路径。
///
/// # 流程
/// 1. 解析 `expected_artifacts` → `ArtifactCapture`；
/// 2. 解析 `gate_config`（兼容 `min_rating` / `review_type` 降级）→ `GateEvaluator`；
/// 3. 根据门禁结果 → `TransitionResolver` 决定 `next_idx`；
/// 4. 如果是返工 → `ReworkTracker` 计算 `rework_count`；
/// 5. 更新 `loop_step_executions`（status / rating / error_message / rework_count）；
/// 6. 更新 `loop_phase_executions`；
/// 7. 返回 `StepOutcome`。
#[allow(clippy::too_many_arguments)]
pub async fn execute_step(
    db: &Arc<Database>,
    execution_record: Option<&ExecutionRecord>,
    loop_execution_id: i64,
    step: &loop_steps::Model,
    step_exec: &loop_step_executions::Model,
    all_steps: &[loop_steps::Model],
    current_idx: usize,
    workspace_path: &str,
) -> Result<StepOutcome, crate::services::process::ProcessError> {
    // 1. 产物捕获。
    let specs = parse_expected_artifacts(step);
    let captured = artifact_capture::capture_step_artifacts(
        db,
        step_exec.id,
        workspace_path,
        &specs,
        execution_record,
        step_exec.execution_record_id.map(|id| id.to_string()).as_deref(),
    )
    .await?;

    // 2. 门禁评价。
    let skill_names: Vec<String> =
        serde_json::from_str(&step.skill_names).unwrap_or_default();
    // acceptance_criteria 在 todos 表中，PhaseDriver 不做两层解析；
    // ai_criteria_review 门禁使用 gate_config 中配置的 criteria_ref 引用阶段或 todo 的验收标准。
    let acceptance_criteria: Option<&str> = None;

    // 兼容旧字段 min_rating / review_type。
    let effective_gate_config = gate_evaluator::infer_gates_from_fallback(
        &step.gate_config,
        step.min_rating,
        &step.review_type,
    );
    let execution_result = execution_record
        .and_then(|r| r.result.as_deref());

    let execution_rating = execution_record.and_then(|r| r.rating);

    // 如果当前无评分，检查 ai_criteria_review 门禁是否配置了等待超时。
    // 不填/0 = 一直等到出分；正数 = 最多等 N 秒。
    let execution_rating = if execution_rating.is_none() {
        let timeout = serde_json::from_str::<Vec<GateDefinition>>(&effective_gate_config)
            .ok()
            .and_then(|gates| {
                gates.into_iter()
                    .find(|g| g.gate_type == "ai_criteria_review")
                    .and_then(|g| g.timeout_secs)
            })
            .filter(|&t| t > 0);
        if let Some(timeout_secs) = timeout {
            // 有限超时：最多等 N 秒
            let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs as u64);
            poll_rating(db, execution_record.as_ref().map(|r| r.id), deadline).await
        } else {
            // 不设超时：一直等到出分（无限等待，实际由 loop_execution 生命周期兜底）
            let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(86400);
            poll_rating(db, execution_record.as_ref().map(|r| r.id), deadline).await
        }
    } else {
        execution_rating
    };

    let gate_summary = gate_evaluator::evaluate_step_gates(
        db,
        step_exec.id,
        &effective_gate_config,
        &captured,
        &skill_names,
        execution_result,
        acceptance_criteria,
        workspace_path,
        execution_rating,
    )
    .await?;

    // 3. 流转解析。
    let step_id_to_idx: HashMap<i64, usize> = all_steps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id, i))
        .collect();
    let gates_passed = gate_summary.all_passed && !gate_summary.has_pending_human;

    let next_idx = transition_resolver::resolve_next(
        step,
        gates_passed,
        &step_id_to_idx,
        current_idx,
    );

    // 4. 返工统计。
    let rework_decision = rework_tracker::evaluate_rework(
        step_exec.rework_count,
        step.max_rework,
        current_idx,
        next_idx,
    );

    let (actual_next_idx, final_rework_count, _paused, error_message) = match rework_decision {
        rework_tracker::ReworkDecision::MaxedOut { current_rework, max_rework } => {
            // 返工超限：强制失败，不跳转。
            db.set_step_execution_rework_count(step_exec.id, current_rework)
                .await?;
            (
                None, // 终止
                current_rework,
                false,
                Some(format!(
                    "返工次数 {} 已达到上限 {}，工艺终止",
                    current_rework, max_rework
                )),
            )
        }
        rework_tracker::ReworkDecision::Allowed(new_count) => {
            if new_count > step_exec.rework_count {
                db.set_step_execution_rework_count(step_exec.id, new_count)
                    .await?;
            }
            (next_idx, new_count, false, None)
        }
        rework_tracker::ReworkDecision::NotRework => {
            (next_idx, step_exec.rework_count, false, None)
        }
    };

    // 5. 确定最终状态。
    let human_pending = gate_summary.has_pending_human;
    let final_status = if human_pending {
        "pending_approval"
    } else if gates_passed {
        "success"
    } else {
        "failed"
    };

    let rating = gate_summary
        .gate_records
        .iter()
        .find(|g| g.gate_type == "ai_criteria_review" && g.status == "passed")
        .and_then(|_| {
            // 从 gate result detail 中尝试解析评分。
            parse_rating_from_detail(&gate_summary)
        });

    // 6. 更新 step_execution（复用已有的 finish 方法，但 rework_count 已单独更新）。
    db.finish_step_execution(
        step_exec.id,
        final_status,
        step_exec.execution_record_id,
        error_message.as_deref(),
        rating,
        None, // conclusion 由 LoopRunner 提取
    )
    .await?;

    // 7. 更新 phase 生命周期。
    update_phase_execution(db, loop_execution_id, step, gates_passed).await?;

    Ok(StepOutcome {
        gate_passed: gates_passed,
        rating,
        error_message,
        next_idx: actual_next_idx,
        paused: human_pending,
        rework_count: final_rework_count,
        artifacts: captured,
        gate_summary: Some(gate_summary),
    })
}

/// 解析 `loop_steps.expected_artifacts` JSON 为 `ArtifactSpec` 列表。
fn parse_expected_artifacts(step: &loop_steps::Model) -> Vec<ArtifactSpec> {
    serde_json::from_str::<Vec<crate::services::process::ExpectedArtifact>>(&step.expected_artifacts)
        .map(|specs| {
            specs
                .iter()
                .map(ArtifactSpec::from_expected)
                .collect()
        })
        .unwrap_or_default()
}

/// 尝试从门禁汇总中提取 AI 评审评分。
fn parse_rating_from_detail(summary: &GateSummary) -> Option<i32> {
    for gate in &summary.gate_records {
        if gate.gate_type == "ai_criteria_review" {
            if let Some(ref detail) = gate.result {
                // 复用 auto_review 的评分解析函数（它解析 RATING: N 格式）。
                return crate::services::auto_review::parse_rating_from_result(Some(detail));
            }
        }
    }
    None
}

/// 轮询等待执行记录的评分出现。
/// 每 500ms 查一次 DB，直到 deadline 或出分。
async fn poll_rating(
    db: &Arc<Database>,
    record_id: Option<i64>,
    deadline: tokio::time::Instant,
) -> Option<i32> {
    let record_id = record_id?;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if let Ok(Some(rec)) = db.get_execution_record(record_id).await {
            if let Some(rating) = rec.rating {
                return Some(rating);
            }
        }
    }
    None
}

/// 更新阶段生命周期：当步骤进入新 phase 或完成后，更新 `loop_phase_executions`。
async fn update_phase_execution(
    db: &Arc<Database>,
    loop_execution_id: i64,
    step: &loop_steps::Model,
    gates_passed: bool,
) -> Result<(), sea_orm::DbErr> {
    let Some(phase_id) = step.phase_id else {
        return Ok(());
    };

    // 检查是否已有 phase execution 记录。
    let existing = loop_phase_executions_for_execution(db, loop_execution_id, phase_id).await?;
    if let Some(pex) = existing {
        // 已有记录：如果步骤完成且阶段状态仍为 running → 更新为 success（或 failed）。
        if pex.status == "running" {
            let new_status = if gates_passed { "success" } else { "failed" };
            update_phase_status(db, pex.id, new_status).await?;
        }
    } else {
        // 新阶段：创建 running 记录。
        let now = crate::models::utc_timestamp();
        let am = crate::db::entity::loop_phase_executions::ActiveModel {
            loop_execution_id: ActiveValue::Set(loop_execution_id),
            phase_id: ActiveValue::Set(phase_id),
            status: ActiveValue::Set("running".to_string()),
            started_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&db.conn).await?;
    }

    Ok(())
}

/// 查询某次 phase 的执行记录。
async fn loop_phase_executions_for_execution(
    db: &Arc<Database>,
    loop_execution_id: i64,
    phase_id: i64,
) -> Result<Option<crate::db::entity::loop_phase_executions::Model>, sea_orm::DbErr> {
    crate::db::entity::loop_phase_executions::Entity::find()
        .filter(
            crate::db::entity::loop_phase_executions::Column::LoopExecutionId.eq(loop_execution_id),
        )
        .filter(
            crate::db::entity::loop_phase_executions::Column::PhaseId.eq(phase_id),
        )
        .one(&db.conn)
        .await
}

/// 更新 phase execution 状态。
async fn update_phase_status(
    db: &Arc<Database>,
    pex_id: i64,
    status: &str,
) -> Result<(), sea_orm::DbErr> {
    use sea_orm::ActiveModelTrait;
    let existing = crate::db::entity::loop_phase_executions::Entity::find_by_id(pex_id)
        .one(&db.conn)
        .await?;
    if let Some(c) = existing {
        let mut am: crate::db::entity::loop_phase_executions::ActiveModel = c.into();
        am.status = sea_orm::ActiveValue::Set(status.to_string());
        am.finished_at = sea_orm::ActiveValue::Set(Some(crate::models::utc_timestamp()));
        am.update(&db.conn).await?;
    }
    Ok(())
}


