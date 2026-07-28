//! `human_approval` 门禁：需要人工审批评分才能通过。
//!
//! 此门禁在环节执行完成时设为 `pending` 状态，由外部审批 API 更新。
//! 不执行自动评审，只记录门禁记录并返回未通过（非失败，而是 pending）。
//!
//! 审批 API 会通过 `db.update_loop_step_execution_gate` 更新状态为 `passed` 或 `failed`，
//! 然后由 `LoopRunner::resume_loop_execution` 继续执行。

use super::{GateContext, GateError, GateResult};

/// 评估人工审批门禁：设置 pending，返回未通过（等待人工）。
pub fn evaluate(_ctx: &GateContext) -> Result<GateResult, GateError> {
    Ok(GateResult {
        gate_name: _ctx.config.name.clone(),
        gate_type: "human_approval".to_string(),
        passed: false,
        detail: Some("等待人工审批".to_string()),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::services::process::GateDefinition;

    use super::*;

    #[test]
    fn test_human_approval_returns_pending() {
        let ctx = GateContext {
            step_execution_id: 1,
            config: GateDefinition {
                name: "人工审批".to_string(),
                gate_type: "human_approval".to_string(),
                artifact: None,
                criteria_ref: None,
                min_score: None,
                script: None,
                timeout_secs: None,
            },
            skill_names: &[],
            artifacts: &[],
            execution_result: None,
            acceptance_criteria: None,
            workspace_path: "/tmp",
        };
        let result = evaluate(&ctx).unwrap();
        assert!(!result.passed);
        assert_eq!(result.detail.as_deref(), Some("等待人工审批"));
    }
}
