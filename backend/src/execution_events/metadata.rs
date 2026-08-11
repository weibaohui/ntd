//! 执行元数据结构定义
//!
//! 集中管理跨事件的上下文信息：session_id、token、cost 等。

use serde::{Deserialize, Serialize};

use super::event::ExecutionEvent;

/// 执行元数据：跨事件的上下文信息
///
/// # 设计原则
/// - 累积模式：字段初始为默认值，随着事件流逐步填充
/// - 用于汇总统计和下游格式化
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    // ── 标识信息 ──────────────────────────────────────
    /// 会话 ID
    pub session_id: Option<String>,
    /// 使用的模型
    pub model: Option<String>,
    /// 执行器类型
    pub executor: String,

    // ── Token 统计（累积） ─────────────────────────────
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,

    // ── 成本与耗时 ────────────────────────────────────
    pub cost_usd: f64,
    pub duration_ms: u64,

    // ── 时间戳 ─────────────────────────────────────────
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

impl ExecutionMetadata {
    /// 创建新的元数据
    pub fn new(executor: impl Into<String>) -> Self {
        Self {
            executor: executor.into(),
            ..Default::default()
        }
    }

    /// 从事件累积更新元数据
    pub fn update_from(&mut self, event: &ExecutionEvent) {
        match event {
            ExecutionEvent::Tokens {
                input,
                output,
                cache_read,
                cache_write,
            } => {
                self.input_tokens = *input;
                self.output_tokens = *output;
                if let Some(cr) = cache_read {
                    self.cache_read_tokens = *cr;
                }
                if let Some(cw) = cache_write {
                    self.cache_write_tokens = *cw;
                }
            }
            ExecutionEvent::SessionStart { session_id } => {
                self.session_id = Some(session_id.clone());
            }
            ExecutionEvent::SessionEnd { session_id } if self.session_id.is_none() => {
                // 仅在之前未设置 session_id 时才回填（SessionStart 优先）
                self.session_id = Some(session_id.clone());
            }
            ExecutionEvent::ModelSwitch { model } => {
                self.model = Some(model.clone());
            }
            ExecutionEvent::Cost { cost_usd } => {
                self.cost_usd = *cost_usd;
            }
            ExecutionEvent::Duration { duration_ms } => {
                self.duration_ms = *duration_ms;
            }
            ExecutionEvent::Progress { percent, message } => {
                tracing::debug!("执行进度: {}% - {:?}", percent, message);
            }
            // 仅在首次 StepStart 时记录开始时间；guard 不满足时落入 `_ => {}`，与原 if 跳过语义一致。
            ExecutionEvent::StepStart { .. } if self.started_at.is_none() => {
                self.started_at = Some(crate::models::utc_timestamp());
            }
            _ => {}
        }
    }

    /// 设置结束时间
    pub fn set_finished_at(&mut self) {
        self.finished_at = Some(crate::models::utc_timestamp());
    }

    /// session 首现认领（096-W2：收敛 impls 层 15 处逐字同构的「首现即置元数据并发
    /// SessionStart」模式）。
    ///
    /// 语义：首次见到 session_id 时记录到元数据并产出 `SessionStart` 事件；
    /// 后续再次出现返回 `None`（同会话的后续行不再重复发事件，先到先赢）。
    /// 调用方通常写作 `events.extend(self.metadata.claim_session(sid));`。
    pub fn claim_session(&mut self, session_id: &str) -> Option<ExecutionEvent> {
        if self.session_id.is_none() {
            self.session_id = Some(session_id.to_string());
            Some(ExecutionEvent::SessionStart {
                session_id: session_id.to_string(),
            })
        } else {
            None
        }
    }

    /// 获取总 token 数量
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let meta = ExecutionMetadata::new("claude_code");
        assert_eq!(meta.executor, "claude_code");
        assert!(meta.session_id.is_none());
        assert_eq!(meta.input_tokens, 0);
    }

    #[test]
    fn test_update_from_tokens() {
        let mut meta = ExecutionMetadata::new("test");
        let event = ExecutionEvent::Tokens {
            input: 100,
            output: 200,
            cache_read: Some(50),
            cache_write: Some(10),
        };
        meta.update_from(&event);

        assert_eq!(meta.input_tokens, 100);
        assert_eq!(meta.output_tokens, 200);
        assert_eq!(meta.cache_read_tokens, 50);
        assert_eq!(meta.cache_write_tokens, 10);
    }

    #[test]
    fn test_total_tokens() {
        let mut meta = ExecutionMetadata::new("test");
        meta.input_tokens = 100;
        meta.output_tokens = 200;
        assert_eq!(meta.total_tokens(), 300);
    }

    /// claim_session：首次调用置元数据并产出 SessionStart；重复调用幂等返回 None（先到先赢）。
    #[test]
    fn test_claim_session_first_wins_and_idempotent() {
        let mut meta = ExecutionMetadata::new("test");
        // 首次：返回 SessionStart 且元数据落位
        let first = meta.claim_session("ses_a");
        assert!(
            matches!(first, Some(ExecutionEvent::SessionStart { ref session_id }) if session_id == "ses_a"),
            "首次认领应产出 SessionStart"
        );
        assert_eq!(meta.session_id.as_deref(), Some("ses_a"));
        // 再次（同 id 或不同 id）：均返回 None，元数据不被覆盖
        assert!(meta.claim_session("ses_a").is_none());
        assert!(meta.claim_session("ses_b").is_none());
        assert_eq!(meta.session_id.as_deref(), Some("ses_a"), "先到先赢，不被后续覆盖");
    }
}
