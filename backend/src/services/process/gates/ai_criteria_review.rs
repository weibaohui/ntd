//! `ai_criteria_review` 门禁：检查执行记录评分（rating）是否 ≥ min_score 阈值。
//!
//! 设计 034 后，评审已由统一路径在 `finalize_normal_completion` 中完成，评分在
//! PhaseDriver 进入门禁评估前已在 DB。此门禁只做**快速检查**——比对已有评分与阈值。
//! 无评分时视为门禁未通过（`needs_review` 标记保留供日志排查，不再触发新评审）。
//!
//! 配置格式：
//! ```json
//! {"criteria_ref": "phase.acceptance_criteria", "min_score": 80}
//! ```

use super::{GateContext, GateResult};

/// AI 评审结果。
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewStatus {
    /// 已有评分，通过。
    Passed { score: i32 },
    /// 已有评分，未通过。
    Failed { score: i32, min_score: i32 },
    /// 需要触发 auto-review（无评分）。
    NeedsReview,
}

/// 检查 `ai_criteria_review` 门禁状态（不触发 auto-review）。
///
/// # 返回
/// - `GateResult`：如果已有评分，直接返回通过/失败；
/// - `needs_review: true` 标记在 detail 中供 PhaseDriver 识别。
pub fn evaluate(
    ctx: &GateContext,
    existing_rating: Option<i32>,
) -> Result<GateResult, super::GateError> {
    let min_score = ctx.config.min_score.unwrap_or(0);

    match existing_rating {
        Some(score) => {
            let passed = score >= min_score;
            let detail = if passed {
                format!("AI 评审通过（评分 {}，阈值 {}）", score, min_score)
            } else {
                format!(
                    "AI 评审未通过（评分 {}，阈值 {}）",
                    score, min_score
                )
            };

            Ok(GateResult {
                gate_name: ctx.config.name.clone(),
                gate_type: "ai_criteria_review".to_string(),
                passed,
                detail: Some(detail),
            })
        }
        None => {
            // 无评分时通知 PhaseDriver 需要触发 auto-review。
            let skill_context = if ctx.skill_names.is_empty() {
                String::new()
            } else {
                format!(
                    "（需按 skill 自检清单评审：{}）",
                    ctx.skill_names.join(", ")
                )
            };

            Ok(GateResult {
                gate_name: ctx.config.name.clone(),
                gate_type: "ai_criteria_review".to_string(),
                passed: false,
                detail: Some(format!("needs_review: 需触发 auto-review{}", skill_context)),
            })
        }
    }
}

// 设计 034：build_review_prompt 已移除（全仓库零调用），评审 prompt 统一由
// auto_review.rs::resolve_review_template + compose_review_prompt 处理。

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use crate::services::process::GateDefinition;

    use super::*;

    #[test]
    fn test_rating_above_threshold_passes() {
        let ctx = GateContext {
            step_execution_id: 1,
            config: GateDefinition {
                name: "AI 评审".to_string(),
                gate_type: "ai_criteria_review".to_string(),
                artifact: None,
                criteria_ref: None,
                min_score: Some(80),
                script: None,
                timeout_secs: None,
            },
            skill_names: &[],
            artifacts: &[],
            execution_result: Some("output"),
            acceptance_criteria: None,
            workspace_path: "/tmp",
        };
        let result = evaluate(&ctx, Some(85)).unwrap();
        assert!(result.passed);
    }

    #[test]
    fn test_rating_below_threshold_fails() {
        let ctx = GateContext {
            step_execution_id: 1,
            config: GateDefinition {
                name: "AI 评审".to_string(),
                gate_type: "ai_criteria_review".to_string(),
                artifact: None,
                criteria_ref: None,
                min_score: Some(80),
                script: None,
                timeout_secs: None,
            },
            skill_names: &[],
            artifacts: &[],
            execution_result: Some("output"),
            acceptance_criteria: None,
            workspace_path: "/tmp",
        };
        let result = evaluate(&ctx, Some(65)).unwrap();
        assert!(!result.passed);
    }

    #[test]
    fn test_no_rating_returns_needs_review() {
        let ctx = GateContext {
            step_execution_id: 1,
            config: GateDefinition {
                name: "AI 评审".to_string(),
                gate_type: "ai_criteria_review".to_string(),
                artifact: None,
                criteria_ref: None,
                min_score: Some(80),
                script: None,
                timeout_secs: None,
            },
            skill_names: &["write-prd".to_string()],
            artifacts: &[],
            execution_result: Some("output"),
            acceptance_criteria: None,
            workspace_path: "/tmp",
        };
        let result = evaluate(&ctx, None).unwrap();
        assert!(!result.passed);
        assert!(result.detail.as_deref().unwrap().contains("needs_review"));
        assert!(result.detail.as_deref().unwrap().contains("write-prd"));
    }
}
