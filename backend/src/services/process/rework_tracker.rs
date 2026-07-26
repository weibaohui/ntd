//! 返工追踪器。
//!
//! M2 运行时：step 门禁失败后 goto 上游 → 视为返工 → `rework_count` 递增 → 超限则强拆。
//! 实际计数和写入由 PhaseDriver 协调，本模块只提供判定与错误类型。

/// 返工判定结果。
#[derive(Debug, Clone, PartialEq)]
pub enum ReworkDecision {
    /// 允许返工，新的 `rework_count` 值。
    Allowed(i32),
    /// 已达到最大返工次数，强制失败。
    MaxedOut {
        current_rework: i32,
        max_rework: i32,
    },
    /// 不需返工（正常流转）。
    NotRework,
}

/// 判断门禁失败后的跳转是否构成返工，并检查是否超限。
///
/// # 参数
/// - `prev_rework_count`：当前环节已执行的最近一次 step_execution 中的返工计数。
/// - `max_rework`：`loop_steps.max_rework`，环节允许的最大返工次数。
/// - `current_idx`：当前环节在 `all_steps` 中的索引。
/// - `target_idx`：`resolve_next` 返回的目标索引。
///
/// # 返回
/// 见 `ReworkDecision`。
pub fn evaluate_rework(
    prev_rework_count: i32,
    max_rework: i32,
    current_idx: usize,
    target_idx: Option<usize>,
) -> ReworkDecision {
    match target_idx {
        // 跳转到上游或同一环节 → 返工。
        Some(idx) if idx <= current_idx => {
            let new_rework = prev_rework_count + 1;
            if new_rework >= max_rework {
                ReworkDecision::MaxedOut {
                    current_rework: new_rework,
                    max_rework,
                }
            } else {
                ReworkDecision::Allowed(new_rework)
            }
        }
        // 跳转到下游或终止（None）→ 非返工。
        _ => ReworkDecision::NotRework,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn goto_upstream_allows_rework() {
        let result = evaluate_rework(0, 3, 2, Some(0));
        assert_eq!(result, ReworkDecision::Allowed(1));
    }

    #[test]
    fn goto_same_allows_rework() {
        let result = evaluate_rework(1, 3, 2, Some(2));
        assert_eq!(result, ReworkDecision::Allowed(2));
    }

    #[test]
    fn goto_downstream_not_rework() {
        let result = evaluate_rework(0, 3, 0, Some(2));
        assert_eq!(result, ReworkDecision::NotRework);
    }

    #[test]
    fn end_not_rework() {
        let result = evaluate_rework(0, 3, 0, None);
        assert_eq!(result, ReworkDecision::NotRework);
    }

    #[test]
    fn maxed_out_detected() {
        // prev=2, max=3, goto upstream then new=3 >= max=3 → MaxedOut
        let result = evaluate_rework(2, 3, 2, Some(0));
        assert_eq!(result, ReworkDecision::MaxedOut { current_rework: 3, max_rework: 3 });
    }

    #[test]
    fn at_limit_not_maxed() {
        // prev=1, max=3, goto upstream then new=2 < max=3 → Allowed
        let result = evaluate_rework(1, 3, 2, Some(0));
        assert_eq!(result, ReworkDecision::Allowed(2));
    }

    #[test]
    fn zero_prev_upstream_allows() {
        let result = evaluate_rework(0, 1, 1, Some(0));
        assert_eq!(result, ReworkDecision::MaxedOut { current_rework: 1, max_rework: 1 });
    }
}
