//! 统一的事件类型枚举定义
//!
//! 完全替代 ParsedLogEntry，使用强类型枚举替代字符串化的 log_type。

use serde::{Deserialize, Serialize};

/// 统一的事件类型枚举
///
/// # 设计原则
/// - 使用 #[serde(tag = "type")] 实现 JSON 中的 "type" 字段自动序列化
/// - 每个变体都是独立的语义单元
/// - 向后兼容：最终会映射到 execution_logs.log_type 的已知值
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ExecutionEvent {
    // ── 消息类型 ────────────────────────────────────────

    /// 助手消息
    Assistant {
        content: String,
        thinking: Option<String>,
        message_id: Option<String>,
    },

    /// 思考过程（从 <thinking> 标签或 thinking 事件提取）
    Thinking { content: String },

    /// 工具调用发起
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// 工具调用结果
    ToolResult {
        call_id: String,
        output: String,
        is_error: bool,
    },

    /// 最终结果/总结
    Result { summary: String },

    /// 用户消息
    User { content: String },

    /// 系统消息
    System { message: String },

    /// 普通信息/日志
    Info { message: String },

    /// 错误消息
    Error { message: String },

    // ── 执行阶段 ───────────────────────────────────────────

    /// 执行步骤开始
    StepStart { name: String, index: u32 },

    /// 执行步骤完成
    StepFinish { name: String, index: u32 },

    // ── 元数据事件 ─────────────────────────────────────────

    /// Token 统计
    Tokens {
        input: u64,
        output: u64,
        cache_read: Option<u64>,
        cache_write: Option<u64>,
    },

    /// 会话开始
    SessionStart { session_id: String },

    /// 会话结束
    SessionEnd { session_id: String },

    /// 模型切换
    ModelSwitch { model: String },

    /// 成本报告
    Cost { cost_usd: f64 },

    /// 耗时报告
    Duration { duration_ms: u64 },

    /// 进度更新
    Progress { percent: u8, message: Option<String> },
}

impl ExecutionEvent {
    /// 转换为数据库兼容的 log_type 字符串
    pub fn to_log_type(&self) -> &'static str {
        match self {
            ExecutionEvent::Assistant { .. } => "assistant",
            ExecutionEvent::Thinking { .. } => "thinking",
            ExecutionEvent::ToolCall { .. } => "tool_call",
            ExecutionEvent::ToolResult { .. } => "tool_result",
            ExecutionEvent::Result { .. } => "result",
            ExecutionEvent::User { .. } => "user",
            ExecutionEvent::System { .. } => "system",
            ExecutionEvent::Info { .. } => "info",
            ExecutionEvent::Error { .. } => "error",
            ExecutionEvent::StepStart { .. } => "step_start",
            ExecutionEvent::StepFinish { .. } => "step_finish",
            ExecutionEvent::Tokens { .. } => "tokens",
            ExecutionEvent::SessionStart { .. } => "session_start",
            ExecutionEvent::SessionEnd { .. } => "session_end",
            ExecutionEvent::ModelSwitch { .. } => "model_switch",
            ExecutionEvent::Cost { .. } => "cost",
            ExecutionEvent::Duration { .. } => "duration",
            ExecutionEvent::Progress { .. } => "progress",
        }
    }

    /// 是否为需要前端特殊渲染的交互类型
    pub fn is_interactive(&self) -> bool {
        matches!(
            self,
            ExecutionEvent::ToolCall { .. }
                | ExecutionEvent::ToolResult { .. }
                | ExecutionEvent::Thinking { .. }
        )
    }

    /// 093-B2：WS Output 广播过滤——「哪些事件推给执行面板」的唯一事实源。
    ///
    /// 原 `log_capture::parse_and_broadcast` 的 18 分支 match 规则上移至此；
    /// SessionEnd 刻意返回 false：逐行路径不转发，由 pipeline.finalize() 统一产出
    /// （避免重复转发）。Info 的内容规则（纯 JSON 行/空串不打扰面板）封装在本方法内。
    pub fn should_broadcast(&self) -> bool {
        match self {
            // 纯 JSON 行（{ 开头）或空串是协议噪声，不作为 info 转发
            ExecutionEvent::Info { message } => {
                !message.starts_with('{') && !message.is_empty()
            }
            ExecutionEvent::Error { .. }
            | ExecutionEvent::Thinking { .. }
            | ExecutionEvent::ToolCall { .. }
            | ExecutionEvent::ToolResult { .. }
            | ExecutionEvent::Assistant { .. }
            | ExecutionEvent::Result { .. }
            | ExecutionEvent::SessionStart { .. }
            | ExecutionEvent::Tokens { .. }
            | ExecutionEvent::Cost { .. }
            | ExecutionEvent::Duration { .. }
            | ExecutionEvent::StepStart { .. }
            | ExecutionEvent::StepFinish { .. }
            // ModelSwitch 需转发到 DB，否则 completion 阶段 get_model_from_logs 找不到模型
            | ExecutionEvent::ModelSwitch { .. } => true,
            ExecutionEvent::SessionEnd { .. }
            | ExecutionEvent::Progress { .. }
            | ExecutionEvent::User { .. }
            | ExecutionEvent::System { .. } => false,
        }
    }

    /// 093-B2：飞书私聊直推收集过滤——「哪些事件值得打扰用户」的唯一事实源。
    ///
    /// 原 `message_debounce::parse_for_direct_stream` 的分支规则上移至此：
    /// 只保留思考/工具交互/助手回复/最终结论等对私聊用户有阅读价值的内容，
    /// 内部状态（session/step/tokens/cost 等）一律不打扰。
    pub fn is_direct_stream_worthy(&self) -> bool {
        match self {
            // 与 should_broadcast 相同的内容规则：协议噪声不进入收集
            ExecutionEvent::Info { message } => {
                !message.starts_with('{') && !message.is_empty()
            }
            ExecutionEvent::Thinking { .. }
            | ExecutionEvent::ToolCall { .. }
            | ExecutionEvent::ToolResult { .. }
            | ExecutionEvent::Assistant { .. }
            | ExecutionEvent::Result { .. } => true,
            _ => false,
        }
    }

    /// 是否为需要显示在对话视图的消息类型
    pub fn is_message(&self) -> bool {
        matches!(
            self,
            ExecutionEvent::Assistant { .. }
                | ExecutionEvent::User { .. }
                | ExecutionEvent::System { .. }
        )
    }

    /// 提取事件的主要内容（用于日志展示）
    pub fn content_preview(&self) -> String {
        match self {
            ExecutionEvent::Assistant { content, .. } => content.clone(),
            ExecutionEvent::Thinking { content } => content.chars().take(200).collect(),
            ExecutionEvent::ToolCall { name, .. } => name.clone(),
            ExecutionEvent::ToolResult { output, .. } => output.chars().take(200).collect(),
            ExecutionEvent::Result { summary } => summary.chars().take(500).collect(),
            ExecutionEvent::User { content } => content.clone(),
            ExecutionEvent::System { message } => message.clone(),
            ExecutionEvent::Info { message } => message.clone(),
            ExecutionEvent::Error { message } => message.clone(),
            ExecutionEvent::StepStart { name, .. } => format!("开始: {}", name),
            ExecutionEvent::StepFinish { name, .. } => format!("完成: {}", name),
            ExecutionEvent::Tokens { input, output, .. } => {
                format!("tokens: in={}, out={}", input, output)
            }
            ExecutionEvent::SessionStart { session_id } => format!("会话: {}", session_id),
            ExecutionEvent::SessionEnd { session_id } => format!("会话结束: {}", session_id),
            ExecutionEvent::ModelSwitch { model } => format!("模型: {}", model),
            ExecutionEvent::Cost { cost_usd } => format!("成本: ${:.4}", cost_usd),
            ExecutionEvent::Duration { duration_ms } => format!("耗时: {}ms", duration_ms),
            ExecutionEvent::Progress { percent, message } => {
                if let Some(msg) = message {
                    format!("{}% - {}", percent, msg)
                } else {
                    format!("进度: {}%", percent)
                }
            }
        }
    }

    /// 从事件内容创建 Info 事件
    pub fn info(message: impl Into<String>) -> Self {
        ExecutionEvent::Info {
            message: message.into(),
        }
    }

    /// 从事件内容创建 Error 事件
    pub fn error(message: impl Into<String>) -> Self {
        ExecutionEvent::Error {
            message: message.into(),
        }
    }

    /// 创建助手消息事件
    pub fn assistant(content: impl Into<String>) -> Self {
        ExecutionEvent::Assistant {
            content: content.into(),
            thinking: None,
            message_id: None,
        }
    }

    /// 创建思考事件
    pub fn thinking(content: impl Into<String>) -> Self {
        ExecutionEvent::Thinking {
            content: content.into(),
        }
    }

    /// 创建用户消息事件
    pub fn user(content: impl Into<String>) -> Self {
        ExecutionEvent::User {
            content: content.into(),
        }
    }

    /// 创建系统消息事件
    pub fn system(message: impl Into<String>) -> Self {
        ExecutionEvent::System {
            message: message.into(),
        }
    }

    /// 创建工具调用事件
    pub fn tool_call(id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        ExecutionEvent::ToolCall {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    /// 创建工具结果事件
    pub fn tool_result(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        ExecutionEvent::ToolResult {
            call_id: call_id.into(),
            output: output.into(),
            is_error: false,
        }
    }

    /// 创建最终结果事件
    pub fn result(summary: impl Into<String>) -> Self {
        ExecutionEvent::Result {
            summary: summary.into(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    /// 093-B2：两个过滤判定的全枚举断言（18 个变体 × 2 维）。
    /// 该表即原 parse_and_broadcast / parse_for_direct_stream 两处 match 的逐分支对照，
    /// 防未来新增事件变体时过滤语义漂移。
    #[test]
    fn test_broadcast_and_direct_stream_matrix() {
        // (事件, should_broadcast, is_direct_stream_worthy)
        let cases: Vec<(ExecutionEvent, bool, bool)> = vec![
            (ExecutionEvent::assistant("a"), true, true),
            (ExecutionEvent::thinking("t"), true, true),
            (ExecutionEvent::tool_call("i", "n", serde_json::json!({})), true, true),
            (ExecutionEvent::tool_result("i", "o"), true, true),
            (ExecutionEvent::result("r"), true, true),
            (ExecutionEvent::error("e"), true, false),
            (ExecutionEvent::SessionStart { session_id: "s".into() }, true, false),
            (ExecutionEvent::Tokens { input: 1, output: 1, cache_read: None, cache_write: None }, true, false),
            (ExecutionEvent::Cost { cost_usd: 0.1 }, true, false),
            (ExecutionEvent::Duration { duration_ms: 1 }, true, false),
            (ExecutionEvent::StepStart { name: "s".into(), index: 0 }, true, false),
            (ExecutionEvent::StepFinish { name: "s".into(), index: 0 }, true, false),
            // ModelSwitch 广播（DB 需要）但不打扰私聊用户
            (ExecutionEvent::ModelSwitch { model: "m".into() }, true, false),
            // SessionEnd 逐行路径不转发（由 finalize 统一产出）
            (ExecutionEvent::SessionEnd { session_id: "s".into() }, false, false),
            (ExecutionEvent::Progress { percent: 1, message: None }, false, false),
            (ExecutionEvent::user("u"), false, false),
            (ExecutionEvent::system("s"), false, false),
        ];
        for (event, broadcast, direct) in cases {
            assert_eq!(event.should_broadcast(), broadcast, "should_broadcast 判定不符: {event:?}");
            assert_eq!(event.is_direct_stream_worthy(), direct, "is_direct_stream_worthy 判定不符: {event:?}");
        }
    }

    /// Info 的内容规则：纯 JSON 行 / 空串为协议噪声，两维都不转发；普通文本两维都收
    #[test]
    fn test_info_content_rules() {
        let json_line = ExecutionEvent::info("{\"type\":\"x\"}");
        assert!(!json_line.should_broadcast());
        assert!(!json_line.is_direct_stream_worthy());
        let empty = ExecutionEvent::info("");
        assert!(!empty.should_broadcast());
        assert!(!empty.is_direct_stream_worthy());
        let plain = ExecutionEvent::info("普通日志");
        assert!(plain.should_broadcast());
        assert!(plain.is_direct_stream_worthy());
    }

    #[test]
    fn test_to_log_type() {
        assert_eq!(ExecutionEvent::thinking("test").to_log_type(), "thinking");
        assert_eq!(
            ExecutionEvent::tool_call("1", "bash", serde_json::json!({})).to_log_type(),
            "tool_call"
        );
        assert_eq!(ExecutionEvent::info("hello").to_log_type(), "info");
        assert_eq!(ExecutionEvent::error("oops").to_log_type(), "error");
    }

    #[test]
    fn test_is_interactive() {
        assert!(ExecutionEvent::thinking("test").is_interactive());
        assert!(ExecutionEvent::tool_call("1", "bash", serde_json::json!({})).is_interactive());
        assert!(!ExecutionEvent::info("hello").is_interactive());
    }

    #[test]
    fn test_is_message() {
        assert!(ExecutionEvent::assistant("hello").is_message());
        assert!(ExecutionEvent::user("hi").is_message());
        assert!(ExecutionEvent::system(" booting").is_message());
        assert!(!ExecutionEvent::thinking("test").is_message());
    }

    #[test]
    fn test_content_preview() {
        let long_text = "a".repeat(600);

        // Thinking: 200 字符
        let event = ExecutionEvent::Thinking { content: long_text.clone() };
        assert_eq!(event.content_preview().len(), 200);

        // Result: 500 字符
        let event = ExecutionEvent::Result { summary: long_text };
        assert_eq!(event.content_preview().len(), 500);
    }
}
