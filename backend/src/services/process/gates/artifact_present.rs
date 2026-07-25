//! `artifact_present` 门禁：检查指定名称的产物是否已在 `loop_step_artifacts` 中捕获。
//!
//! 配置格式：
//! ```json
//! {"artifact": "PRD"}
//! ```

use super::{GateContext, GateError, GateResult};

/// 评估产物存在性门禁。
pub fn evaluate(ctx: &GateContext) -> Result<GateResult, GateError> {
    let artifact_name = ctx
        .config
        .artifact
        .as_deref()
        .unwrap_or(&ctx.config.name);

    let found = ctx.artifacts.iter().any(|a| a.name == artifact_name);
    let passed = found;

    let detail = if passed {
        Some(format!("产物「{}」已捕获", artifact_name))
    } else {
        Some(format!("产物「{}」未捕获", artifact_name))
    };

    Ok(GateResult {
        gate_name: ctx.config.name.clone(),
        gate_type: "artifact_present".to_string(),
        passed,
        detail,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::db::entity::loop_step_artifacts;
    use crate::services::process::GateDefinition;

    use super::*;

    fn make_artifacts(names: &[&str]) -> Vec<loop_step_artifacts::Model> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| loop_step_artifacts::Model {
                id: (i + 1) as i64,
                loop_step_execution_id: 1,
                name: name.to_string(),
                artifact_type: "file".to_string(),
                locator: String::new(),
                content_text: None,
                captured_at: String::new(),
                captured_by: None,
            })
            .collect()
    }

    #[test]
    fn test_artifact_present_found() {
        let artifacts = make_artifacts(&["PRD", "Design"]);
        let ctx = GateContext {
            step_execution_id: 1,
            config: GateDefinition {
                name: "Check PRD".to_string(),
                gate_type: "artifact_present".to_string(),
                artifact: Some("PRD".to_string()),
                criteria_ref: None,
                min_score: None,
                script: None,
            },
            skill_names: &[],
            artifacts: &artifacts,
            execution_result: None,
            acceptance_criteria: None,
            workspace_path: "/tmp",
        };
        let result = evaluate(&ctx).unwrap();
        assert!(result.passed);
        assert!(result.detail.unwrap().contains("PRD"));
    }

    #[test]
    fn test_artifact_present_missing() {
        let artifacts = make_artifacts(&["Design"]);
        let ctx = GateContext {
            step_execution_id: 1,
            config: GateDefinition {
                name: "Check PRD".to_string(),
                gate_type: "artifact_present".to_string(),
                artifact: Some("PRD".to_string()),
                criteria_ref: None,
                min_score: None,
                script: None,
            },
            skill_names: &[],
            artifacts: &artifacts,
            execution_result: None,
            acceptance_criteria: None,
            workspace_path: "/tmp",
        };
        let result = evaluate(&ctx).unwrap();
        assert!(!result.passed);
        assert!(result.detail.unwrap().contains("未捕获"));
    }
}
