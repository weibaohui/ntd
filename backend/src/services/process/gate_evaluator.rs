//! 门禁编排引擎。
//!
//! 接收环节的门禁配置列表，依次执行每个门禁，将结果写入 `loop_step_execution_gates` 表，
//! 并返回汇总的通过/失败状态。

use std::sync::Arc;

use crate::db::entity::loop_step_artifacts;
use crate::db::entity::loop_step_execution_gates;
use crate::db::Database;
use crate::services::process::gates::{
    ai_criteria_review, human_approval, GateContext, GateResult,
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

    // 046：废弃门禁类型 artifact_present/script_check 在此提前拦截，
    // 统一并入 ai_criteria_review。此校验放在解析后、写 DB 前，
    // 保证废弃类型不会静默降级为 failed 记录，而是显式报错。
    for gate_def in &gates {
        if matches!(gate_def.gate_type.as_str(), "artifact_present" | "script_check") {
            return Err(crate::services::process::ProcessError::ParseError(format!(
                "已废弃的门禁类型「{}」：artifact_present/script_check 统一并入 ai_criteria_review",
                gate_def.gate_type
            )));
        }
    }

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

        // 复用前置创建的 pending 记录：create_loop_step_execution 已为每个门禁
        // 建好 pending 记录（供审批界面展示），此处若再新建会产生重复记录——
        // 审批只更新其中一条，残留的另一条会在 UI 上显示为「failed(等待人工审批)」。
        let gate_model = match find_pending_gate(db, step_execution_id, gate_def).await? {
            Some(m) => m,
            None => db
                .create_loop_step_execution_gate(
                    step_execution_id,
                    &gate_def.gate_type,
                    &gate_def.name,
                    &serde_json::to_string(gate_def).unwrap_or_default(),
                )
                .await
                .map_err(|e| crate::services::process::ProcessError::Db(Box::new(e)))?,
        };

        let result = evaluate_single_gate(&ctx, execution_rating).await;

        match result {
            Ok(gate_result) => {
                // human_approval 未通过是「等待人工」而非失败：持久化为 pending，
                // 避免 UI 把待审批门禁渲染成 failed（审批动作才会把它推进 passed/failed）。
                let status = if gate_result.passed {
                    "passed"
                } else if gate_def.gate_type == "human_approval" {
                    "pending"
                } else {
                    "failed"
                };
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

/// 在已有门禁记录中查找与定义匹配的 pending 记录（按 类型+名称）。
///
/// 只匹配 pending：已被评估/审批过的记录不复用，
/// 返工重跑时新 step_execution 有自己的记录集，互不干扰。
async fn find_pending_gate(
    db: &Arc<Database>,
    step_execution_id: i64,
    gate_def: &GateDefinition,
) -> Result<Option<loop_step_execution_gates::Model>, crate::services::process::ProcessError> {
    let existing = db
        .list_loop_step_execution_gates(step_execution_id)
        .await
        .map_err(|e| crate::services::process::ProcessError::Db(Box::new(e)))?;
    Ok(existing.into_iter().find(|g| {
        g.status == "pending" && g.gate_type == gate_def.gate_type && g.gate_name == gate_def.name
    }))
}

/// 执行单条门禁，根据 type 分派到具体实现。
///
/// `execution_rating` 是执行记录已有的评分，供 `ai_criteria_review` 使用。
async fn evaluate_single_gate(
    ctx: &GateContext<'_>,
    execution_rating: Option<i32>,
) -> Result<GateResult, crate::services::process::ProcessError> {
    match ctx.config.gate_type.as_str() {
        "human_approval" => {
            let result = human_approval::evaluate(ctx)?;
            Ok(result)
        }
        "ai_criteria_review" => {
            let result = ai_criteria_review::evaluate(ctx, execution_rating)?;
            Ok(result)
        }
        _ => Err(crate::services::process::ProcessError::ParseError(format!(
            "不支持的门禁类型「{}」（已废弃：artifact_present、script_check，统一并入 ai_criteria_review）",
            ctx.config.gate_type
        ))),
    }
}

// 044：infer_gates_from_fallback（从旧 min_rating/review_type 推断门禁）已下线——
// loop_steps 不再持有这两列，门禁配置完全以 gate_config 为权威来源。

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
mod tests {
    use super::*;

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

        // 046：artifact_present 已废弃，统一并入 ai_criteria_review。
        let gate_config = r#"[{"name":"AI评审","type":"ai_criteria_review","min_score":80}]"#;
        let artifacts: Vec<loop_step_artifacts::Model> = vec![];
        let skill_names: Vec<String> = vec![];

        // 无评分 → ai_criteria_review 门禁失败。
        let summary = evaluate_step_gates(
            &db, 1, gate_config, &artifacts, &skill_names,
            None, None, "/tmp", None,
        ).await.unwrap();

        assert!(!summary.all_passed);
        assert_eq!(summary.gate_records.len(), 1);
        assert_eq!(summary.gate_records[0].status, "failed");

        // 验证 DB 中确实有记录。
        let stored = db.list_loop_step_execution_gates(1).await.unwrap();
        assert_eq!(stored.len(), 1);
    }

    /// 046：废弃门禁类型 artifact_present/script_check 应返回错误。
    #[tokio::test]
    async fn test_evaluate_step_gates_rejects_deprecated_artifact_present() {
        use std::sync::Arc;
        let db = Arc::new(crate::db::Database::new(":memory:").await.unwrap());
        db.exec("INSERT INTO todos (id,title,prompt,status) VALUES (1,'t','p','pending')").await.unwrap();
        db.exec("INSERT INTO loops (id,name) VALUES (1,'l')").await.unwrap();
        db.exec("INSERT INTO loop_steps (id,loop_id,name,todo_id) VALUES (1,1,'s',1)").await.unwrap();
        db.exec("INSERT INTO loop_executions (id,loop_id,trigger_type,started_at,status) VALUES (1,1,'manual','2024-01-01','running')").await.unwrap();
        db.exec("INSERT INTO loop_step_executions (id,loop_execution_id,step_id,todo_id,status) VALUES (1,1,1,1,'running')").await.unwrap();

        let gate_config = r#"[{"name":"PRD存在","type":"artifact_present"}]"#;
        let result = evaluate_step_gates(
            &db, 1, gate_config, &[], &[], None, None, "/tmp", None,
        ).await;

        // 废弃类型应返回 ParseError，而非 silently pass。
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("artifact_present"), "错误信息应提及废弃类型: {err_msg}");
    }

    /// 046：废弃门禁类型 script_check 同样应返回错误。
    #[tokio::test]
    async fn test_evaluate_step_gates_rejects_deprecated_script_check() {
        use std::sync::Arc;
        let db = Arc::new(crate::db::Database::new(":memory:").await.unwrap());
        db.exec("INSERT INTO todos (id,title,prompt,status) VALUES (1,'t','p','pending')").await.unwrap();
        db.exec("INSERT INTO loops (id,name) VALUES (1,'l')").await.unwrap();
        db.exec("INSERT INTO loop_steps (id,loop_id,name,todo_id) VALUES (1,1,'s',1)").await.unwrap();
        db.exec("INSERT INTO loop_executions (id,loop_id,trigger_type,started_at,status) VALUES (1,1,'manual','2024-01-01','running')").await.unwrap();
        db.exec("INSERT INTO loop_step_executions (id,loop_execution_id,step_id,todo_id,status) VALUES (1,1,1,1,'running')").await.unwrap();

        let gate_config = r#"[{"name":"测试通过","type":"script_check","script":"run_tests.sh"}]"#;
        let result = evaluate_step_gates(
            &db, 1, gate_config, &[], &[], None, None, "/tmp", None,
        ).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("script_check"), "错误信息应提及废弃类型: {err_msg}");
    }

    /// 046：ai_criteria_review 有评分且 ≥ min_score → 门禁通过。
    #[tokio::test]
    async fn test_evaluate_step_gates_ai_review_passes_with_rating() {
        use std::sync::Arc;
        let db = Arc::new(crate::db::Database::new(":memory:").await.unwrap());
        db.exec("INSERT INTO todos (id,title,prompt,status) VALUES (1,'t','p','pending')").await.unwrap();
        db.exec("INSERT INTO loops (id,name) VALUES (1,'l')").await.unwrap();
        db.exec("INSERT INTO loop_steps (id,loop_id,name,todo_id) VALUES (1,1,'s',1)").await.unwrap();
        db.exec("INSERT INTO loop_executions (id,loop_id,trigger_type,started_at,status) VALUES (1,1,'manual','2024-01-01','running')").await.unwrap();
        db.exec("INSERT INTO loop_step_executions (id,loop_execution_id,step_id,todo_id,status) VALUES (1,1,1,1,'running')").await.unwrap();

        let gate_config = r#"[{"name":"AI评审","type":"ai_criteria_review","min_score":80}]"#;
        let summary = evaluate_step_gates(
            &db, 1, gate_config, &[], &[], None, None, "/tmp", Some(85),
        ).await.unwrap();

        assert!(summary.all_passed);
        assert_eq!(summary.gate_records.len(), 1);
        assert_eq!(summary.gate_records[0].status, "passed");
    }

    /// 回归：create_loop_step_execution 创建步骤执行时已为每个门禁建好 pending 记录，
    /// evaluate_step_gates 必须复用它而不是再建一条——否则审批只更新其中一条，
    /// 残留的另一条会在 UI 上显示为「failed(等待人工审批)」。
    #[tokio::test]
    async fn test_evaluate_step_gates_reuses_existing_pending_human_gate() {
        use std::sync::Arc;
        let db = Arc::new(crate::db::Database::new(":memory:").await.unwrap());
        // 插入 FK 依赖。
        db.exec("INSERT INTO todos (id,title,prompt,status) VALUES (1,'t','p','pending')").await.unwrap();
        db.exec("INSERT INTO loops (id,name) VALUES (1,'l')").await.unwrap();
        db.exec("INSERT INTO loop_steps (id,loop_id,name,todo_id) VALUES (1,1,'s',1)").await.unwrap();
        db.exec("INSERT INTO loop_executions (id,loop_id,trigger_type,started_at,status) VALUES (1,1,'manual','2024-01-01','running')").await.unwrap();
        db.exec("INSERT INTO loop_step_executions (id,loop_execution_id,step_id,todo_id,status) VALUES (1,1,1,1,'running')").await.unwrap();

        // 模拟 create_loop_step_execution 的前置创建：人工审批门禁已是 pending。
        let gate_config = r#"[{"name":"人工审批","type":"human_approval"}]"#;
        db.create_loop_step_execution_gate(1, "human_approval", "人工审批", gate_config).await.unwrap();

        let summary = evaluate_step_gates(
            &db, 1, gate_config, &[], &[], None, None, "/tmp", None,
        ).await.unwrap();

        // 等待人工是 paused 而非失败。
        assert!(!summary.all_passed);
        assert!(summary.has_pending_human);

        // 关键断言：复用前置记录，DB 中仍只有一条；
        // 且状态保持 pending（等待人工），不是 failed。
        let stored = db.list_loop_step_execution_gates(1).await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].status, "pending");
    }
}
