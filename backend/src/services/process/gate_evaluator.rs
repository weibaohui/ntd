//! 门禁编排引擎。
//!
//! 接收环节的门禁配置列表，依次执行每个门禁，将结果写入 `loop_step_execution_gates` 表，
//! 并返回汇总的通过/失败状态。

use std::sync::Arc;

use crate::db::entity::loop_step_artifacts;
use crate::db::entity::loop_step_execution_gates;
use crate::db::Database;
use crate::services::process::gates::{
    ai_criteria_review, artifact_present, human_approval, script_check, GateContext, GateResult,
};
use crate::services::process::GateDefinition;

/// 全部门禁的评价汇总。
#[derive(Debug, Clone)]
pub struct GateSummary {
    /// 所有门禁是否全部通过。
    pub all_passed: bool,
    /// 是否有人工审批门禁在等待（paused）。
    pub has_pending_human: bool,
    /// 每条门禁的评价结果（已写入 DB 的含 id 版本）。
    pub gate_records: Vec<loop_step_execution_gates::Model>,
}

/// 编排某环节的全部门禁。
///
/// 1. 解析 `expected_gates` 为 `GateDefinition` 列表；
/// 2. 依次调用对应门禁 `evaluate` 函数；
/// 3. 每项门禁结果写入 `loop_step_execution_gates`表；
/// 4. 返回 `GateSummary`。
///
/// `step_execution_id` 是 `loop_step_executions.id`。
/// `gate_config_json` 是 `loop_steps.gate_config` 中的 JSON 数组。
/// `execution_rating` 是执行记录已有的评分（如有，供 ai_criteria_review 门禁直接使用）。
#[allow(clippy::too_many_arguments)]
pub async fn evaluate_step_gates(
    db: &Arc<Database>,
    step_execution_id: i64,
    gate_config_json: &str,
    artifacts: &[loop_step_artifacts::Model],
    skill_names: &[String],
    execution_result: Option<&str>,
    acceptance_criteria: Option<&str>,
    workspace_path: &str,
    execution_rating: Option<i32>,
) -> Result<GateSummary, crate::services::process::ProcessError> {
    let gates: Vec<GateDefinition> = serde_json::from_str(gate_config_json)
        .map_err(|e| crate::services::process::ProcessError::ParseError(format!(
            "解析 gate_config 失败: {}",
            e
        )))?;

    let mut all_passed = true;
    let mut has_pending_human = false;
    let mut gate_records = Vec::with_capacity(gates.len());

    for gate_def in &gates {
        let ctx = GateContext {
            step_execution_id,
            config: gate_def.clone(),
            skill_names,
            artifacts,
            execution_result,
            acceptance_criteria,
            workspace_path,
        };

        // 先创建 pending 记录，再评估，最后更新。
        let gate_model = db
            .create_loop_step_execution_gate(
                step_execution_id,
                &gate_def.gate_type,
                &gate_def.name,
                &serde_json::to_string(gate_def).unwrap_or_default(),
            )
            .await
            .map_err(|e| crate::services::process::ProcessError::Db(Box::new(e)))?;

        let result = evaluate_single_gate(&ctx, execution_rating).await;

        match result {
            Ok(gate_result) => {
                let status = if gate_result.passed { "passed" } else { "failed" };
                db.update_loop_step_execution_gate(
                    gate_model.id,
                    status,
                    gate_result.detail.as_deref(),
                    Some("ai"),
                )
                .await
                .map_err(|e| crate::services::process::ProcessError::Db(Box::new(e)))?;

                if !gate_result.passed {
                    all_passed = false;
                    if gate_def.gate_type == "human_approval" {
                        has_pending_human = true;
                    }
                }

                let mut updated = gate_model;
                updated.status = status.to_string();
                updated.result = gate_result.detail.clone();
                gate_records.push(updated);
            }
            Err(gate_err) => {
                // 评估失败视为门禁失败。
                let err_msg = gate_err.to_string();
                let status = "failed";
                db.update_loop_step_execution_gate(
                    gate_model.id,
                    status,
                    Some(&err_msg),
                    Some("system"),
                )
                .await
                .map_err(|e| crate::services::process::ProcessError::Db(Box::new(e)))?;

                all_passed = false;
                let mut updated = gate_model;
                updated.status = status.to_string();
                updated.result = Some(err_msg);
                gate_records.push(updated);
            }
        }
    }

    Ok(GateSummary {
        all_passed,
        has_pending_human,
        gate_records,
    })
}

/// 执行单条门禁，根据 type 分派到具体实现。
///
/// `execution_rating` 是执行记录已有的评分，供 `ai_criteria_review` 使用。
async fn evaluate_single_gate(
    ctx: &GateContext<'_>,
    execution_rating: Option<i32>,
) -> Result<GateResult, crate::services::process::ProcessError> {
    match ctx.config.gate_type.as_str() {
        "artifact_present" => {
            let result = artifact_present::evaluate(ctx)?;
            Ok(result)
        }
        "human_approval" => {
            let result = human_approval::evaluate(ctx)?;
            Ok(result)
        }
        "script_check" => {
            let result = script_check::evaluate(ctx).await?;
            Ok(result)
        }
        "ai_criteria_review" => {
            let result = ai_criteria_review::evaluate(ctx, execution_rating)?;
            Ok(result)
        }
        _ => Ok(GateResult {
            gate_name: ctx.config.name.clone(),
            gate_type: ctx.config.gate_type.clone(),
            passed: true,
            detail: Some(format!("未知门禁类型「{}」，默认通过", ctx.config.gate_type)),
        }),
    }
}

/// 从旧 `min_rating` 与 `review_type` 隐式生成门禁配置 JSON 数组。
///
/// 当 `gate_config` 为空或 `[]` 时有此降级：
/// - `min_rating` 有值 → 生成 `ai_criteria_review` 门禁；
/// - `review_type == "human"` → 生成 `human_approval` 门禁。
pub fn infer_gates_from_fallback(
    gate_config_json: &str,
    min_rating: Option<i32>,
    review_type: &str,
) -> String {
    // 如果已有显式门禁，直接返回。
    if let Ok(gates) = serde_json::from_str::<Vec<GateDefinition>>(gate_config_json) {
        if !gates.is_empty() {
            return gate_config_json.to_string();
        }
    }

    let mut inferred = Vec::new();

    if review_type == "human" {
        inferred.push(GateDefinition {
            name: "人工审批".to_string(),
            gate_type: "human_approval".to_string(),
            artifact: None,
            criteria_ref: None,
            min_score: None,
            script: None,
        });
    }

    if let Some(rating) = min_rating {
        inferred.push(GateDefinition {
            name: format!("AI 评审（min_rating={}）", rating),
            gate_type: "ai_criteria_review".to_string(),
            artifact: None,
            criteria_ref: None,
            min_score: Some(rating),
            script: None,
        });
    }

    serde_json::to_string(&inferred).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_gates_from_fallback_empty_keeps_existing() {
        let result = infer_gates_from_fallback(r#"[{"name":"Test","type":"artifact_present"}]"#, Some(80), "ai");
        assert!(result.contains("artifact_present"));
    }

    #[test]
    fn test_infer_gates_from_fallback_min_rating() {
        let result = infer_gates_from_fallback("[]", Some(70), "ai");
        assert!(result.contains("ai_criteria_review"));
        assert!(!result.contains("human_approval"));
    }

    #[test]
    fn test_infer_gates_from_fallback_human_review() {
        let result = infer_gates_from_fallback("[]", None, "human");
        assert!(result.contains("human_approval"));
        assert!(!result.contains("ai_criteria_review"));
    }

    #[test]
    fn test_infer_gates_from_fallback_both() {
        let result = infer_gates_from_fallback("[]", Some(60), "human");
        assert!(result.contains("ai_criteria_review"));
        assert!(result.contains("human_approval"));
    }

    /// 模拟完整 evaluate_step_gates 流程：创建 DB 门禁记录并验证。
    #[tokio::test]
    async fn test_evaluate_step_gates_creates_records() {
        use std::sync::Arc;
        let db = Arc::new(crate::db::Database::new(":memory:").await.unwrap());
        // 插入 FK 依赖。
        db.exec("INSERT INTO todos (id,title,prompt,status) VALUES (1,'t','p','pending')").await.unwrap();
        db.exec("INSERT INTO loops (id,name) VALUES (1,'l')").await.unwrap();
        db.exec("INSERT INTO loop_steps (id,loop_id,name,todo_id) VALUES (1,1,'s',1)").await.unwrap();
        db.exec("INSERT INTO loop_executions (id,loop_id,trigger_type,started_at,status) VALUES (1,1,'manual','2024-01-01','running')").await.unwrap();
        db.exec("INSERT INTO loop_step_executions (id,loop_execution_id,step_id,todo_id,status) VALUES (1,1,1,1,'running')").await.unwrap();

        let gate_config = r#"[{"name":"PRD存在","type":"artifact_present"}]"#;
        let artifacts: Vec<loop_step_artifacts::Model> = vec![];
        let skill_names: Vec<String> = vec![];

        let summary = evaluate_step_gates(
            &db, 1, gate_config, &artifacts, &skill_names,
            None, None, "/tmp", None,
        ).await.unwrap();

        // artifact_present 无产物 → 门禁失败。
        assert!(!summary.all_passed);
        assert_eq!(summary.gate_records.len(), 1);
        assert_eq!(summary.gate_records[0].status, "failed");

        // 验证 DB 中确实有记录。
        let stored = db.list_loop_step_execution_gates(1).await.unwrap();
        assert_eq!(stored.len(), 1);
    }
}
