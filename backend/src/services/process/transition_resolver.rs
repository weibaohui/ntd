//! 环节流转解析器 —— 根据门禁结果与环节配置决定下一步操作。
//!
//! 规则：
//! - 门禁全通过 → 执行 `on_success` 策略；
//! - 有门禁失败 → 执行 `on_rating_fail` 策略（安装时已做 on_gate_fail fallback）；
//! - 策略 `next` → 推进到下一索引；
//! - 策略为环节 id（跳转目标）→ 使用 `success_goto_step_id` 或 `fail_goto_step_id`（需求 037：裸 id 即跳转，已清除 `goto:` 前缀）；
//! - 策略 `end` / `break` → 终止（返回 None）；
//! - 策略 `skip` → 推进到下一索引（同 next）。

use std::collections::HashMap;

use crate::db::entity::loop_steps;

/// 解析下一步索引。
///
/// # 参数
/// - `step`：当前环节模型。
/// - `gates_passed`：所有门禁是否通过。
/// - `step_id_to_idx`：所有环节 ID 到 order_index 的映射。
/// - `current_idx`：当前环节在 `all_steps` 中的索引。
///
/// # 返回
/// `Some(idx)` 下一步索引；`None` 表示终止（end/break）。
pub fn resolve_next(
    step: &loop_steps::Model,
    gates_passed: bool,
    step_id_to_idx: &HashMap<i64, usize>,
    current_idx: usize,
) -> Option<usize> {
    let policy = if gates_passed {
        &step.on_success
    } else {
        &step.on_rating_fail
    };

    resolve_by_policy(policy, step, gates_passed, step_id_to_idx, current_idx)
}

/// 按策略字符串解析下一步索引。
///
/// 从 `resolve_next` 拆分独立函数，方便单独测试且保持父函数 ≤30 行。
fn resolve_by_policy(
    policy: &str,
    step: &loop_steps::Model,
    gates_passed: bool,
    step_id_to_idx: &HashMap<i64, usize>,
    current_idx: usize,
) -> Option<usize> {
    // 流转策略规则（需求 037：已清除 goto: 前缀）：
    // - 保留字 next/skip → 下一索引；end/break → 终止；
    // - 其他值（裸环节 id）= 跳转目标，用安装时解析好的 success_goto_step_id/fail_goto_step_id 流转。
    match policy {
        "next" => Some(current_idx + 1),
        "skip" => Some(current_idx + 1),
        "end" | "break" => None,
        _ => {
            // 非保留字即跳转目标。数字目标 id 由 installer.resolve_goto 在安装时写入。
            // 找不到（旧数据/异常）时 fallback 到 next，保证流转不卡死。
            let target = if gates_passed {
                step.success_goto_step_id
            } else {
                step.fail_goto_step_id
            };
            match target.and_then(|id| step_id_to_idx.get(&id).copied()) {
                Some(idx) => {
                    tracing::info!("transition: goto step #{} (idx={})", step.id, idx);
                    Some(idx)
                }
                None => {
                    tracing::warn!(
                        "transition: goto target for step #{} not found, falling back to next",
                        step.id
                    );
                    Some(current_idx + 1)
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
mod tests {
    use super::*;

    /// 构造测试用的 loop_steps 模型，只设置与流转相关的字段。
    fn make_step(
        id: i64,
        on_success: &str,
        on_rating_fail: &str,
        success_goto: Option<i64>,
        fail_goto: Option<i64>,
    ) -> loop_steps::Model {
        loop_steps::Model {
            id,
            loop_id: 1,
            name: format!("step_{}", id),
            description: String::new(),
            order_index: 0,
            todo_id: 100 + id,
            // 044：run_mode/skip_on_source_failed/min_rating/unrated_policy 列已下线
            on_success: on_success.to_string(),
            success_goto_step_id: success_goto,
            on_rating_fail: on_rating_fail.to_string(),
            fail_goto_step_id: fail_goto,
            phase_id: None,
            expected_artifacts: "[]".to_string(),
            step_template_refs: "[]".to_string(),
            gate_config: "[]".to_string(),
            max_rework: 3,
            skill_names: "[]".to_string(),
            expert_name: None,
            review_prompt: None,
            enabled: 1,
            created_at: None,
        }
    }

    fn idx_map(steps: &[loop_steps::Model]) -> HashMap<i64, usize> {
        steps.iter().enumerate().map(|(i, s)| (s.id, i)).collect()
    }

    // ── gates_passed: 走 on_success ──

    #[test]
    fn success_next_returns_plus_one() {
        let steps = vec![
            make_step(1, "next", "break", None, None),
            make_step(2, "next", "break", None, None),
        ];
        let map = idx_map(&steps);
        assert_eq!(resolve_next(&steps[0], true, &map, 0), Some(1));
    }

    #[test]
    fn success_end_returns_none() {
        let steps = vec![make_step(1, "end", "break", None, None)];
        let map = idx_map(&steps);
        assert_eq!(resolve_next(&steps[0], true, &map, 0), None);
    }

    // ── gates_failed: 走 on_rating_fail ──

    #[test]
    fn fail_break_returns_none() {
        let steps = vec![make_step(1, "next", "break", None, None)];
        let map = idx_map(&steps);
        assert_eq!(resolve_next(&steps[0], false, &map, 0), None);
    }

    #[test]
    fn fail_skip_returns_plus_one() {
        let steps = vec![
            make_step(1, "next", "skip", None, None),
            make_step(2, "next", "break", None, None),
        ];
        let map = idx_map(&steps);
        assert_eq!(resolve_next(&steps[0], false, &map, 0), Some(1));
    }

    // ── goto ──

    #[test]
    fn success_goto_found_returns_target() {
        let steps = vec![
            make_step(1, "step-3", "break", Some(3), None),
            make_step(2, "next", "break", None, None),
            make_step(3, "next", "break", None, None),
        ];
        let map = idx_map(&steps);
        assert_eq!(resolve_next(&steps[0], true, &map, 0), Some(2));
    }

    #[test]
    fn fail_goto_found_returns_target() {
        let steps = vec![
            make_step(1, "next", "step-3", None, Some(3)),
            make_step(2, "next", "break", None, None),
            make_step(3, "next", "break", None, None),
        ];
        let map = idx_map(&steps);
        // gates failed → 走 on_rating_fail=step-3（裸环节 id 跳转）→ fail_goto_step_id=3 → idx=2
        assert_eq!(resolve_next(&steps[0], false, &map, 0), Some(2));
    }

    #[test]
    fn success_goto_missing_falls_back_to_next() {
        let steps = vec![
            make_step(1, "step-3", "break", Some(999), None),
            make_step(2, "next", "break", None, None),
        ];
        let map = idx_map(&steps);
        assert_eq!(resolve_next(&steps[0], true, &map, 0), Some(1));
    }

}
