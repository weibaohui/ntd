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

    // ── 执行状态 ──────────────────────────────────────
    pub exit_code: Option<i32>,
    pub is_success: bool,
}

impl ExecutionMetadata {
    /// 创建新的元数据
    pub fn new(executor: impl Into<String>) -> Self {
        Self {
            executor: executor.into(),
            // 执行成功的初始值：true，但会被 exit_code 等后续状态覆盖
            is_success: true,
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
            ExecutionEvent::SessionEnd { session_id } => {
                // 如果之前没有设置 session_id，则设置
                if self.session_id.is_none() {
                    self.session_id = Some(session_id.clone());
                }
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

    /// 标记执行成功
    pub fn mark_success(&mut self) {
        self.is_success = true;
    }

    /// 标记执行失败
    pub fn mark_failed(&mut self) {
        self.is_success = false;
    }

    /// 设置退出码
    pub fn set_exit_code(&mut self, code: i32) {
        self.exit_code = Some(code);
        // 非零退出码视为失败
        if code != 0 {
            self.is_success = false;
        }
    }

    /// 设置结束时间
    pub fn set_finished_at(&mut self) {
        self.finished_at = Some(crate::models::utc_timestamp());
    }

    /// 获取总 token 数量
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    /// 获取总缓存 token 数量
    pub fn total_cache_tokens(&self) -> u64 {
        self.cache_read_tokens.saturating_add(self.cache_write_tokens)
    }

    /// 转换为数据库存储的 ExecutionUsage 格式
    pub fn to_usage(&self) -> crate::models::ExecutionUsage {
        crate::models::ExecutionUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_input_tokens: Some(self.cache_read_tokens),
            cache_creation_input_tokens: Some(self.cache_write_tokens),
            total_cost_usd: Some(self.cost_usd),
            duration_ms: Some(self.duration_ms),
        }
    }

    /// 获取摘要信息
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if self.input_tokens > 0 || self.output_tokens > 0 {
            parts.push(format!(
                "tokens: in={}, out={}",
                self.input_tokens, self.output_tokens
            ));
        }

        if self.cache_read_tokens > 0 || self.cache_write_tokens > 0 {
            parts.push(format!(
                "cache: read={}, write={}",
                self.cache_read_tokens, self.cache_write_tokens
            ));
        }

        if self.cost_usd > 0.0 {
            parts.push(format!("cost: ${:.4}", self.cost_usd));
        }

        if self.duration_ms > 0 {
            parts.push(format!("duration: {}ms", self.duration_ms));
        }

        if parts.is_empty() {
            "无统计信息".to_string()
        } else {
            parts.join(", ")
        }
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
    fn test_mark_success_failed() {
        let mut meta = ExecutionMetadata::new("test");
        assert!(meta.is_success); // 默认 true

        meta.mark_failed();
        assert!(!meta.is_success);

        meta.mark_success();
        assert!(meta.is_success);
    }

    #[test]
    fn test_set_exit_code() {
        let mut meta = ExecutionMetadata::new("test");

        meta.set_exit_code(0);
        assert!(meta.is_success);

        meta.set_exit_code(1);
        assert!(!meta.is_success);
        assert_eq!(meta.exit_code, Some(1));
    }

    #[test]
    fn test_total_tokens() {
        let mut meta = ExecutionMetadata::new("test");
        meta.input_tokens = 100;
        meta.output_tokens = 200;
        assert_eq!(meta.total_tokens(), 300);
    }
}
