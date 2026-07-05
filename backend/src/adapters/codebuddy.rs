use super::helpers;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use super::claude_protocol::{ClaudeMessage, ClaudeContentBlock};
use crate::models::utc_timestamp;

/// Codebuddy executor。
///
/// 与 ClaudeCode 结构对称（path + model），统一通过 `BaseExecutor` 共享状态。
// `BaseExecutor` 已经 `#[derive(Clone)]`，组合字段无需手写 Clone impl。
#[derive(Clone)]
pub struct CodebuddyExecutor {
    base: BaseExecutor,
}

impl CodebuddyExecutor {
    pub fn new(path: String) -> Self {
        Self { base: BaseExecutor::new(path) }
    }

    /// 处理 system 事件：把 model 写入 base.state，content 显示 session init 摘要。
    fn handle_system(&self, model: Option<&String>, session_id: Option<&String>, subtype: Option<&String>) -> Option<ParsedLogEntry> {
        if let Some(m) = model {
            *self.base.model.lock() = Some(m.clone());
        }
        Some(helpers::entry("system", format!("Session init: {:?}", session_id.or(subtype))))
    }

    /// 处理 assistant 事件：把所有 block 串成一个 assistant 条目，
    /// 记录第一个 ToolUse 的 name/input 给前端展示。
    fn handle_assistant(&self, message: &super::claude_protocol::ClaudeMessageContent) -> Option<ParsedLogEntry> {
        let mut parts: Vec<String> = Vec::new();
        let mut first_tool_name: Option<String> = None;
        let mut first_tool_input_json: Option<String> = None;
        for block in &message.content {
            append_assistant_block(block, &mut parts, &mut first_tool_name, &mut first_tool_input_json);
        }
        if parts.is_empty() {
            None
        } else {
            Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "assistant".to_string(),
                content: parts.join("\n"),
                usage: None,
                tool_name: first_tool_name,
                tool_input_json: first_tool_input_json,
            })
        }
    }

    /// 处理 user 事件：通常只携带 ToolResult block；无匹配返回 None。
    fn handle_user(&self, message: &super::claude_protocol::ClaudeMessageContent) -> Option<ParsedLogEntry> {
        let parts: Vec<String> = message
            .content
            .iter()
            .filter_map(user_block_part)
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "user".to_string(),
                content: parts.join("\n"),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            })
        }
    }

    /// 处理 result 事件：组装 ExecutionUsage + final 文本/log_type。
    fn handle_result(
        &self,
        result: Option<&str>,
        is_error: bool,
        duration_ms: Option<u64>,
        total_cost_usd: Option<f64>,
        usage: Option<&crate::adapters::claude_protocol::ClaudeUsage>,
    ) -> Option<ParsedLogEntry> {
        let err_str = if is_error { "[error] " } else { "" };
        let result_str = result.unwrap_or_default();
        let usage = usage.map(|u| crate::models::ExecutionUsage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cache_read_input_tokens: u.cache_read_input_tokens,
            cache_creation_input_tokens: u.cache_creation_input_tokens,
            total_cost_usd,
            duration_ms,
        });
        Some(helpers::entry_with_usage(
            if is_error { "error" } else { "result" },
            format!("{}{}", err_str, result_str),
            usage,
        ))
    }
}

/// assistant block → 文本片段收集；首次 ToolUse 会额外捕获 name + input_json。
fn append_assistant_block(
    block: &ClaudeContentBlock,
    parts: &mut Vec<String>,
    first_tool_name: &mut Option<String>,
    first_tool_input_json: &mut Option<String>,
) {
    match block {
        ClaudeContentBlock::Thinking { thinking: Some(t) } => {
            parts.push(format!("[thinking] {}", t.chars().take(200).collect::<String>()));
        }
        ClaudeContentBlock::Text { text: Some(t) } => parts.push(t.clone()),
        ClaudeContentBlock::ToolUse { name, input, .. } => {
            let input_str = serde_json::to_string(input).unwrap_or_default();
            parts.push(format!(
                "[tool] {}: {}",
                name.as_deref().unwrap_or(""),
                input_str.chars().take(100).collect::<String>()
            ));
            if first_tool_name.is_none() {
                *first_tool_name = name.clone();
                *first_tool_input_json = Some(input_str);
            }
        }
        ClaudeContentBlock::ToolResult { content, is_error, .. } => {
            let err_str = if is_error.unwrap_or(false) { "[error] " } else { "" };
            parts.push(format!(
                "{}{}",
                err_str,
                content.as_deref().unwrap_or("").chars().take(100).collect::<String>()
            ));
        }
        ClaudeContentBlock::Redacted { redacted } => {
            parts.push(format!("[redacted] {}", redacted.as_deref().unwrap_or("")));
        }
        _ => {}
    }
}

/// user block → 文本片段（只关心 ToolResult）；其它 block 跳过。
fn user_block_part(block: &ClaudeContentBlock) -> Option<String> {
    if let ClaudeContentBlock::ToolResult { content, is_error, .. } = block {
        let err_str = if is_error.unwrap_or(false) { "[error] " } else { "" };
        Some(format!("{}{}", err_str, content.as_deref().unwrap_or("")))
    } else {
        None
    }
}

impl CodeExecutor for CodebuddyExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Codebuddy
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--permission-mode".to_string(),
            "bypassPermissions".to_string(),
            message.to_string(),
        ]
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        if line.is_empty() {
            return None;
        }
        if let Ok(msg) = serde_json::from_str::<ClaudeMessage>(line) {
            return match msg {
                ClaudeMessage::System { subtype, session_id, model } => {
                    self.handle_system(model.as_ref(), session_id.as_ref(), subtype.as_ref())
                }
                ClaudeMessage::Assistant { message, .. } => self.handle_assistant(&message),
                ClaudeMessage::User { message, .. } => self.handle_user(&message),
                ClaudeMessage::Result { result, is_error, duration_ms, total_cost_usd, usage, .. } => {
                    self.handle_result(result.as_deref(), is_error, duration_ms, total_cost_usd, usage.as_ref())
                }
            };
        }
        Some(helpers::text_entry(line))
    }

    fn get_model(&self) -> Option<String> {
        self.base.model.lock().clone()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use crate::executor_service::completion::get_usage_from_tokens_logs;
    use crate::models::{ExecutionUsage, ParsedLogEntry};

    #[test]
    fn test_parse_output_line_system() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"system","model":"claude-3-5-sonnet"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "system");
        assert!(entry.content.contains("Session init"));
        assert_eq!(executor.get_model(), Some("claude-3-5-sonnet".to_string()));
    }

    #[test]
    fn test_parse_output_line_assistant_text() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "assistant");
        assert_eq!(entry.content, "hello");
    }

    #[test]
    fn test_parse_output_line_assistant_thinking() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"thinking..."}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "assistant");
        assert!(entry.content.starts_with("[thinking]"));
        assert!(entry.content.contains("thinking..."));
    }

    #[test]
    fn test_parse_output_line_user_tool_result() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"result","is_error":false}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "user");
        assert_eq!(entry.content, "result");
    }

    #[test]
    fn test_parse_output_line_result_success() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"result","result":"final","is_error":false,"duration_ms":100,"total_cost_usd":0.001,"usage":{"input_tokens":10,"output_tokens":20}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "result");
        assert_eq!(entry.content, "final");
        assert!(entry.usage.is_some());
        let usage = entry.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 20);
        assert_eq!(usage.duration_ms, Some(100));
        assert_eq!(usage.total_cost_usd, Some(0.001));
    }

    #[test]
    fn test_parse_output_line_result_error() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"result","result":"error","is_error":true}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "error");
        assert_eq!(entry.content, "[error] error");
    }

    #[test]
    fn test_parse_output_line_empty_line() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = "";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_raw_text_fallback() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = "just text";
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "just text");
    }

    #[test]
    fn test_usage_from_tokens_logs() {
        let logs = vec![
            ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "result".to_string(),
                content: "final".to_string(),
                usage: Some(ExecutionUsage {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    total_cost_usd: Some(0.001),
                    duration_ms: Some(100),
                }),
            tool_name: None,
            tool_input_json: None,
            },
        ];
        let usage = get_usage_from_tokens_logs(&logs);
        assert!(usage.is_none(), "result type should not match tokens type");
    }

    #[test]
    fn test_usage_from_tokens_logs_no_logs() {
        let logs: Vec<ParsedLogEntry> = vec![];
        assert!(get_usage_from_tokens_logs(&logs).is_none());
    }

    #[test]
    fn test_get_model_before_system() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        assert!(executor.get_model().is_none());
    }
}
