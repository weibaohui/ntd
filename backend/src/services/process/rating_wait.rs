//! AI 评审门禁的评分等待（需求 047）。
//!
//! 046 删掉了门禁侧的评分 poll 等待，依赖「rating 在门禁评估前已就绪」的假设。
//! 但 `persist_completion_record`（写 record 终态）先于 `finalize_normal_completion`
//! （auto_review 写 rating），LoopRunner 一旦看到 record 终态就读 record，此时 rating
//! 可能还没写回，导致 `ai_criteria_review` 门禁拿到 None 误判 fail。
//!
//! 本模块在门禁评估处恢复「等评分」：rating 未就绪时 poll DB，由 `GateDefinition.timeout_secs`
//! 控制上限。不动 record 生命周期（重排 persist/finalize 会破坏 executor 终态信号语义）。

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::db::Database;
use crate::services::process::GateDefinition;

/// 默认评分等待上限（秒）：对齐 `auto_review` 自身的 300s 上限。
/// 超过仍未出分则放弃，让下游 ai_criteria_review 按 needs_review fail 推进，避免 loop 永久挂起。
const DEFAULT_RATING_WAIT_SECS: u64 = 300;

/// poll 间隔：与 `loop_runner::wait_for_step_finish` 一致，平衡响应速度与 DB 压力。
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// still-waiting 日志间隔：避免刷屏，又让运维知道仍在等待、不是卡死。
const LOG_TICK: Duration = Duration::from_secs(30);

/// gate_config 是否含至少一个 `ai_criteria_review` 门禁。
///
/// 复用 gate_evaluator 的解析路径（`serde_json` → `Vec<GateDefinition>`）。
/// 解析失败视为「无 AI 门禁」——gate_evaluator 后续会以 ParseError 显式报错，此处不重复报。
pub fn has_ai_criteria_review_gate(gate_config_json: &str) -> bool {
    let gates: Vec<GateDefinition> = match serde_json::from_str(gate_config_json) {
        Ok(gs) => gs,
        Err(_) => return false,
    };
    gates.iter().any(|g| g.gate_type == "ai_criteria_review")
}

/// 取所有 `ai_criteria_review` 门禁 `timeout_secs` 的最大者。
///
/// 多个 AI 门禁时取最长等待（最宽松的上限）；全为 None → None（调用方用默认 300s）。
pub fn resolve_rating_timeout(gate_config_json: &str) -> Option<i32> {
    let gates: Vec<GateDefinition> = serde_json::from_str(gate_config_json).ok()?;
    gates
        .iter()
        .filter(|g| g.gate_type == "ai_criteria_review")
        .filter_map(|g| g.timeout_secs)
        .max()
}

/// 轮询 `record.rating`，直到出现或超时。
///
/// # timeout 语义（与 `GateDefinition.timeout_secs` 注释一致）
/// - `None` → 默认 300s
/// - `Some(0)` → 一直等（`loop_runner::wait_for_step_finish` 的 24h 是最后兜底）
/// - `Some(N)` N>0 → 等 N 秒
///
/// 返回评分；超时或 record 不存在返回 None（让下游判 needs_review fail）。
pub async fn wait_for_rating(
    db: &Arc<Database>,
    record_id: i64,
    timeout_secs: Option<i32>,
) -> Option<i32> {
    // 计算等待上限：None 用默认；0 视为无限（用极大值近似，loop_runner 24h 兜底）。
    let wait_secs = match timeout_secs {
        None => DEFAULT_RATING_WAIT_SECS,
        Some(0) => u64::MAX / 2,
        Some(n) => n.max(0) as u64,
    };
    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    let mut last_log = Instant::now();

    tracing::info!(
        "rating_wait: 等待 record #{} 评分（上限 {}s）",
        record_id,
        wait_secs
    );

    loop {
        // 拿到评分立即返回（auto_review 已写回 rating）。
        if let Ok(Some(rec)) = db.get_execution_record(record_id).await {
            if let Some(r) = rec.rating {
                tracing::info!("rating_wait: record #{} 评分就绪 = {}", record_id, r);
                return Some(r);
            }
        }
        // 超时放弃，交给下游按「未评分」处理。
        if Instant::now() >= deadline {
            tracing::warn!(
                "rating_wait: 等待 record #{} 评分超时（{}s），按未评分处理",
                record_id,
                wait_secs
            );
            return None;
        }
        // 定期打 still-waiting，避免静默卡死让运维无日志可查。
        if last_log.elapsed() >= LOG_TICK {
            tracing::info!("rating_wait: 仍在等待 record #{} 评分...", record_id);
            last_log = Instant::now();
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// 解析含 ai_criteria_review 的 gate_config 应识别为需要等评分。
    #[test]
    fn test_has_ai_criteria_review_gate_true() {
        let cfg = r#"[{"name":"AI评审","type":"ai_criteria_review","min_score":80}]"#;
        assert!(has_ai_criteria_review_gate(cfg));
    }

    /// 只有 human_approval 的环节不需要等评分。
    #[test]
    fn test_has_ai_criteria_review_gate_false_for_human_only() {
        let cfg = r#"[{"name":"人工审批","type":"human_approval"}]"#;
        assert!(!has_ai_criteria_review_gate(cfg));
    }

    /// 空 gate_config 无需等评分。
    #[test]
    fn test_has_ai_criteria_review_gate_false_for_empty() {
        assert!(!has_ai_criteria_review_gate("[]"));
    }

    /// 非法 JSON 不报错，视为无需等评分（解析错误由 gate_evaluator 显式报）。
    #[test]
    fn test_has_ai_criteria_review_gate_invalid_json_is_false() {
        assert!(!has_ai_criteria_review_gate("not json"));
    }

    /// 多个 AI 门禁取最大 timeout_secs。
    #[test]
    fn test_resolve_rating_timeout_takes_max() {
        let cfg = r#"[
            {"name":"A","type":"ai_criteria_review","min_score":60,"timeout_secs":120},
            {"name":"B","type":"ai_criteria_review","min_score":80,"timeout_secs":300}
        ]"#;
        assert_eq!(resolve_rating_timeout(cfg), Some(300));
    }

    /// AI 门禁未配 timeout_secs → None（调用方用默认 300s）。
    #[test]
    fn test_resolve_rating_timeout_none_when_unset() {
        let cfg = r#"[{"name":"A","type":"ai_criteria_review","min_score":60}]"#;
        assert_eq!(resolve_rating_timeout(cfg), None);
    }
}
