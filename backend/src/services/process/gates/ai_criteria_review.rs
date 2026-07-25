//! `ai_criteria_review` 门禁：AI 按验收标准 + skill 自检清单评审产物。
//!
//! 此门禁有两种执行模式：
//! 1. **快速检查**：执行记录已有评分（rating）时，直接比对 min_score 阈值；
//! 2. **完整评审**：无评分时返回 `needs_review` 标记，由 PhaseDriver 触发 auto-review。
//!
//! 完整评审复用现有的 `auto_review` runtime（DEFAULT_REVIEWER_PROMPT + parse_rating_from_result），
//! 在 PhaseDriver 中通过 LoopRunnerCtx 调用 `run_todo_execution` 执行。
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

/// 从 auto-review 执行结果和 skill 自检清单构建评审 prompt。
///
/// 在 PhaseDriver 中触发 auto-review 时使用此函数拼接 prompt。
pub fn build_review_prompt(
    step_prompt: &str,
    output: &str,
    acceptance_criteria: &str,
    skill_self_check_list: Option<&str>,
) -> String {
    use crate::services::auto_review::MAX_OUTPUT_CHARS;

    let truncated: String = if output.chars().count() > MAX_OUTPUT_CHARS {
        let mut s: String = output.chars().take(MAX_OUTPUT_CHARS).collect();
        s.push_str("\n\n[...以下被截断...]");
        s
    } else {
        output.to_string()
    };

    let mut prompt = crate::services::auto_review::DEFAULT_REVIEWER_PROMPT
        .to_string()
        .replace("{original_prompt}", step_prompt)
        .replace("{original_output}", &truncated)
        .replace("{acceptance_criteria}", acceptance_criteria);
    // 注入 skill 自检清单。
    if let Some(checklist) = skill_self_check_list.filter(|s| !s.trim().is_empty()) {
        prompt = prompt.replace(
            "{acceptance_criteria}",
            &format!(
                "{}\n\n# Skill 自检清单\n{}",
                acceptance_criteria, checklist
            ),
        );
    }

    prompt
}

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
