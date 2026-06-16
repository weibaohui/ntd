use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use super::claude_protocol::{ClaudeMessage, ClaudeContentBlock};
use crate::adapters::ExecutionUsage;
use crate::models::utc_timestamp;

/// ClaudeCode executor。
///
/// 使用 `BaseExecutor` 持有 path + model。
/// `usage` 字段虽然未被本 executor 直接使用（claude_code 的 usage 走
/// `super::get_usage_from_logs` 从 result 事件提取），但 BaseExecutor 仍然保留这个
/// `Arc<Mutex<Option<ExecutionUsage>>>` 字段，方便与其他 executor 行为保持一致。
// `BaseExecutor` 已经 `#[derive(Clone)]`，组合字段无需手写 Clone impl。
#[derive(Clone)]
pub struct ClaudeCodeExecutor {
    base: BaseExecutor,
}

impl ClaudeCodeExecutor {
    pub fn new(path: String) -> Self {
        Self { base: BaseExecutor::new(path) }
    }
}

impl CodeExecutor for ClaudeCodeExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Claudecode
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "--dangerously-skip-permissions".to_string(),
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            message.to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "--dangerously-skip-permissions".to_string(),
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];
        if let Some(sid) = session_id {
            if is_resume {
                args.push("--resume".to_string());
            } else {
                args.push("--session-id".to_string());
            }
            args.push(sid.to_string());
        }
        args.push("--verbose".to_string());
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        if line.is_empty() {
            return None;
        }

        // Try to parse as Claude NDJSON message
        if let Ok(msg) = serde_json::from_str::<ClaudeMessage>(line) {
            return match msg {
                ClaudeMessage::System { subtype, session_id, model } => {
                    // Store model if found
                    if let Some(m) = model {
                        *self.base.model.lock() = Some(m.clone());
                    }
                    Some(ParsedLogEntry {
                        timestamp: utc_timestamp(),
                        log_type: "system".to_string(),
                        content: format!("Session init: {:?}", session_id.or(subtype)),
                        usage: None,
            tool_name: None,
            tool_input_json: None,
                    })
                }
                ClaudeMessage::Assistant { message, .. } => {
                    // Check for tool_use first (for tool execution display)
                    for block in &message.content {
                        if let ClaudeContentBlock::ToolUse { name, input, .. } = block {
                            let input_str = serde_json::to_string(input).unwrap_or_default();
                            return Some(ParsedLogEntry {
                                timestamp: utc_timestamp(),
                                log_type: "tool_use".to_string(),
                                content: format!("调用工具: {} - {}", name.as_ref().unwrap_or(&"unknown".to_string()), input_str.chars().take(300).collect::<String>()),
                                usage: None,
                                tool_name: name.clone(),
                                tool_input_json: Some(serde_json::to_string(input).unwrap_or_default()),
                            });
                        }
                    }

                    // Check for thinking (for AI thinking display)
                        for block in &message.content {
                            if let ClaudeContentBlock::Thinking { thinking: Some(t) } = block {
                                return Some(ParsedLogEntry {
                                    timestamp: utc_timestamp(),
                                    log_type: "thinking".to_string(),
                                    content: t.chars().take(500).collect::<String>(),
                                    usage: None,
            tool_name: None,
            tool_input_json: None,
                                });
                            }
                        }

                    // Check for tool_result (from user message)
                    for block in &message.content {
                        if let ClaudeContentBlock::ToolResult { content, is_error, .. } = block {
                            let err_str = if is_error.unwrap_or(false) { "[错误] " } else { "" };
                            return Some(ParsedLogEntry {
                                timestamp: utc_timestamp(),
                                log_type: "tool_result".to_string(),
                                content: format!("{}{}", err_str, content.as_ref().unwrap_or(&String::new()).chars().take(300).collect::<String>()),
                                usage: None,
            tool_name: None,
            tool_input_json: None,
                            });
                        }
                    }

                    // Check for text content
                    let mut text_parts = Vec::new();
                    for block in &message.content {
                        if let ClaudeContentBlock::Text { text: Some(t) } = block {
                            text_parts.push(t.clone());
                        }
                    }
                    if !text_parts.is_empty() {
                        return Some(ParsedLogEntry {
                            timestamp: utc_timestamp(),
                            log_type: "assistant".to_string(),
                            content: text_parts.join("\n"),
                            usage: None,
            tool_name: None,
            tool_input_json: None,
                        });
                    }

                    // Check for redacted content
                    for block in &message.content {
                        if let ClaudeContentBlock::Redacted { redacted } = block {
                            return Some(ParsedLogEntry {
                                timestamp: utc_timestamp(),
                                log_type: "assistant".to_string(),
                                content: format!("[redacted] {}", redacted.as_ref().unwrap_or(&String::new())),
                                usage: None,
            tool_name: None,
            tool_input_json: None,
                            });
                        }
                    }

                    None
                }
                ClaudeMessage::User { message, .. } => {
                    // Check for tool_result in user message
                    for block in &message.content {
                        if let ClaudeContentBlock::ToolResult { content, is_error, .. } = block {
                            let err_str = if is_error.unwrap_or(false) { "[错误] " } else { "" };
                            return Some(ParsedLogEntry {
                                timestamp: utc_timestamp(),
                                log_type: "tool_result".to_string(),
                                content: format!("{}{}", err_str, content.as_ref().unwrap_or(&String::new()).chars().take(300).collect::<String>()),
                                usage: None,
            tool_name: None,
            tool_input_json: None,
                            });
                        }
                    }
                    None
                }
                ClaudeMessage::Result { result, is_error, duration_ms, total_cost_usd, usage, .. } => {
                    let err_str = if is_error { "[error] " } else { "" };
                    let result_str = result.unwrap_or_default();

                    // Build usage from Result fields
                    let usage = usage.map(|u| crate::models::ExecutionUsage {
                        input_tokens: u.input_tokens,
                        output_tokens: u.output_tokens,
                        cache_read_input_tokens: u.cache_read_input_tokens,
                        cache_creation_input_tokens: u.cache_creation_input_tokens,
                        total_cost_usd,
                        duration_ms,
                    });

                    Some(ParsedLogEntry {
                        timestamp: utc_timestamp(),
                        log_type: if is_error { "error".to_string() } else { "result".to_string() },
                        content: format!("{}{}", err_str, result_str),
                        usage,
                    tool_name: None,
                    tool_input_json: None,
                    })
                }
            };
        }

        // Fallback: treat as raw text
        Some(ParsedLogEntry {
            timestamp: utc_timestamp(),
            log_type: "text".to_string(),
            content: line.to_string(),
            usage: None,
            tool_name: None,
            tool_input_json: None,
        })
    }

    fn get_usage(&self, logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> {
        super::get_usage_from_logs(logs)
    }

    fn get_model(&self) -> Option<String> {
        self.base.model.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ParsedLogEntry;

    #[test]
    fn test_parse_output_line_system() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"system","model":"claude-3-5-sonnet"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "system");
        assert!(entry.content.contains("Session init"));
        assert_eq!(executor.get_model(), Some("claude-3-5-sonnet".to_string()));
    }

    #[test]
    fn test_parse_output_line_assistant_text() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "assistant");
        assert_eq!(entry.content, "hello");
    }

    #[test]
    fn test_parse_output_line_assistant_thinking() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"thinking..."}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "thinking");
        assert_eq!(entry.content, "thinking...");
    }

    #[test]
    fn test_parse_output_line_user_tool_result() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"result","is_error":false}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert_eq!(entry.content, "result");
    }

    #[test]
    fn test_parse_output_line_result_success() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
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
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"result","result":"error","is_error":true}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "error");
        assert_eq!(entry.content, "[error] error");
    }

    #[test]
    fn test_parse_output_line_empty_line() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = "";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_raw_text_fallback() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = "just text";
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "just text");
    }

    #[test]
    fn test_get_usage_after_result() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
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
        let usage = executor.get_usage(&logs).unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 20);
    }

    #[test]
    fn test_get_usage_no_result() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let logs: Vec<ParsedLogEntry> = vec![];
        assert!(executor.get_usage(&logs).is_none());
    }

    #[test]
    fn test_get_model_before_system() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        assert!(executor.get_model().is_none());
    }
}
