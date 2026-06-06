use std::sync::Arc;
use parking_lot::Mutex;

use super::{CodeExecutor, ExecutorType, ParsedLogEntry, ExecutionUsage};
use crate::models::utc_timestamp;

pub struct AtomcodeExecutor {
    path: String,
    usage: Arc<Mutex<Option<ExecutionUsage>>>,
    has_done: Arc<Mutex<bool>>,
}

impl AtomcodeExecutor {
    pub fn new(path: String) -> Self {
        Self {
            path,
            usage: Arc::new(Mutex::new(None)),
            has_done: Arc::new(Mutex::new(false)),
        }
    }
}

impl Clone for AtomcodeExecutor {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            usage: self.usage.clone(),
            has_done: self.has_done.clone(),
        }
    }
}

impl CodeExecutor for AtomcodeExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Atomcode
    }

    fn executable_path(&self) -> &str {
        &self.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "-v".to_string(),
            // headless/自动化模式下跳过交互式权限确认，与 ClaudeCodeExecutor 保持一致
            "--dangerously-skip-permissions".to_string(),
            "-p".to_string(),
            message.to_string(),
        ]
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(ParsedLogEntry {
            timestamp: utc_timestamp(),
            log_type: "text".to_string(),
            content: trimmed.to_string(),
            usage: None,
            tool_name: None,
            tool_input_json: None,
        })
    }

    fn parse_stderr_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = line.trim();

        // Skip streaming and headless markers
        if trimmed.starts_with("[tool-streaming") || trimmed.starts_with("[headless]") {
            return None;
        }

        // [tokens] prompt=11 completion=0
        if trimmed.starts_with("[tokens]") {
            let mut prompt_tokens = 0u64;
            let mut completion_tokens = 0u64;
            for part in trimmed.split_whitespace().skip(1) {
                if let Some((key, val)) = part.split_once('=') {
                    match key {
                        "prompt" => prompt_tokens = val.parse().unwrap_or(0),
                        "completion" => completion_tokens = val.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }

            let mut usage_guard = self.usage.lock();
            if let Some(ref mut usage) = *usage_guard {
                usage.input_tokens = prompt_tokens;
                usage.output_tokens = completion_tokens;
            } else {
                *usage_guard = Some(ExecutionUsage {
                    input_tokens: prompt_tokens,
                    output_tokens: completion_tokens,
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    total_cost_usd: None,
                    duration_ms: None,
                });
            }

            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "tokens".to_string(),
                content: trimmed.to_string(),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            });
        }

        // [done] 4.6s tokens=0 turns=1 tool_calls=0 [stopped=turn_limit]
        if trimmed.starts_with("[done]") {
            *self.has_done.lock() = true;

            let mut total_tokens = 0u64;
            let mut turns = 0u64;
            let mut tool_calls = 0u64;
            let mut duration_ms = None;

            for (i, part) in trimmed.split_whitespace().enumerate() {
                if i == 1 {
                    // e.g. "4.6s"
                    let s = part.trim_end_matches('s');
                    if let Ok(secs) = s.parse::<f64>() {
                        duration_ms = Some((secs * 1000.0) as u64);
                    }
                } else if let Some((key, val)) = part.split_once('=') {
                    match key {
                        "tokens" => total_tokens = val.parse().unwrap_or(0),
                        "turns" => turns = val.parse().unwrap_or(0),
                        "tool_calls" => tool_calls = val.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }

            let mut usage_guard = self.usage.lock();
            if let Some(ref mut usage) = *usage_guard {
                usage.duration_ms = duration_ms;
            } else if total_tokens > 0 {
                *usage_guard = Some(ExecutionUsage {
                    input_tokens: total_tokens,
                    output_tokens: 0,
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    total_cost_usd: None,
                    duration_ms,
                });
            }

            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "step_finish".to_string(),
                content: format!("Execution finished: {} turns, {} tool calls", turns, tool_calls),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            });
        }

        // [tool→ write_file args={...}]
        if trimmed.starts_with("[tool→") {
            let (tool_name, tool_input_json) = parse_atomcode_tool_call(trimmed);
            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "tool".to_string(),
                content: trimmed.to_string(),
                usage: None,
                tool_name,
                tool_input_json,
            });
        }

        // [tool← write_file OK 0ms] ...
        if trimmed.starts_with("[tool←") {
            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "tool".to_string(),
                content: trimmed.to_string(),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            });
        }

        // [approval-denied] tool=write_file reason=...
        if trimmed.starts_with("[approval-denied]") {
            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "error".to_string(),
                content: trimmed.to_string(),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            });
        }

        None
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        super::default_final_result_with_think_stripping(logs)
    }

    fn get_usage(&self, _logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> {
        self.usage.lock().clone()
    }

    fn get_model(&self) -> Option<String> {
        None
    }
}

/// Parse tool name and args JSON from atomcode stderr format: [tool→ name args={...}]
fn parse_atomcode_tool_call(line: &str) -> (Option<String>, Option<String>) {
    let trimmed = line.trim_start_matches("[tool→").trim_start_matches("[tool->").trim();
    let (name_part, args_part) = if let Some(idx) = trimmed.find(" args=") {
        (&trimmed[..idx], Some(trimmed[idx + 6..].trim()))
    } else if let Some(idx) = trimmed.find(" args:") {
        (&trimmed[..idx], Some(trimmed[idx + 6..].trim()))
    } else {
        (trimmed, None)
    };
    let name = name_part.split_whitespace().next().map(|s| s.to_string()).filter(|s| !s.is_empty());
    let args_json = args_part.and_then(|a| {
        let a = a.trim_matches(|c| c == '{' || c == '}');
        if a.is_empty() { None } else { Some(format!("{{{}}}", a)) }
    });
    (name, args_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ParsedLogEntry;

    #[test]
    fn test_parse_output_line_text() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_output_line("Hello world").unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_parse_output_line_empty() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.parse_output_line("").is_none());
        assert!(executor.parse_output_line("   ").is_none());
    }

    #[test]
    fn test_parse_stderr_line_tokens() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[tokens] prompt=11 completion=5").unwrap();
        assert_eq!(entry.log_type, "tokens");
        assert_eq!(entry.content, "[tokens] prompt=11 completion=5");

        let usage = executor.get_usage(&[]).unwrap();
        assert_eq!(usage.input_tokens, 11);
        assert_eq!(usage.output_tokens, 5);
    }

    #[test]
    fn test_parse_stderr_line_done() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[done] 4.6s tokens=100 turns=2 tool_calls=1").unwrap();
        assert_eq!(entry.log_type, "step_finish");
        assert!(entry.content.contains("2 turns"));
        assert!(entry.content.contains("1 tool calls"));

        let usage = executor.get_usage(&[]).unwrap();
        assert_eq!(usage.duration_ms, Some(4600));
    }

    #[test]
    fn test_parse_stderr_line_done_with_stopped() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[done] 4.9s tokens=0 turns=3 tool_calls=3 stopped=turn_limit").unwrap();
        assert_eq!(entry.log_type, "step_finish");
        assert!(entry.content.contains("3 turns"));
        assert!(entry.content.contains("3 tool calls"));
    }

    #[test]
    fn test_parse_stderr_line_tool_call() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[tool→ bash args={\"command\": \"ls\"}]").unwrap();
        assert_eq!(entry.log_type, "tool");
    }

    #[test]
    fn test_parse_stderr_line_tool_result() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[tool← bash OK 39ms] result here").unwrap();
        assert_eq!(entry.log_type, "tool");
    }

    #[test]
    fn test_parse_stderr_line_approval_denied() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[approval-denied] tool=write_file reason=outside dir").unwrap();
        assert_eq!(entry.log_type, "error");
    }

    #[test]
    fn test_parse_stderr_line_streaming_skipped() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.parse_stderr_line("[tool-streaming← write_file]").is_none());
    }

    #[test]
    fn test_parse_stderr_line_headless_skipped() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.parse_stderr_line("[headless] auto-approved bash: ...").is_none());
    }

    #[test]
    fn test_parse_stderr_line_unknown_falls_back() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.parse_stderr_line("some random stderr").is_none());
    }

    #[test]
    fn test_get_final_result_with_text() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  hello world  "),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello world".to_string()));
    }

    #[test]
    fn test_get_usage_before_tokens() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.get_usage(&[]).is_none());
    }

    #[test]
    fn test_get_model_always_none() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.get_model().is_none());
    }

    #[test]
    fn test_command_args() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let args = executor.command_args("do something");
        assert_eq!(args, vec!["-v", "--dangerously-skip-permissions", "-p", "do something"]);
    }

    #[test]
    fn test_executor_type() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Atomcode);
    }
}
