//! Codewhale executor adapter.
//!
//! Integrates with `codewhale exec --auto --output-format stream-json <prompt>`.
//! Output format is NDJSON with events: tool_use, tool_result, content, session_capture, metadata, done.
//!
//! Example output:
//!   {"type":"tool_use","name":"exec_shell","id":"call_xxx","input":{"command":"ls"}}
//!   {"type":"tool_result","id":"call_xxx","output":"...","status":"success"}
//!   {"type":"content","content":"Hello"}
//!   {"type":"session_capture","content":"uuid"}
//!   {"type":"metadata","meta":{"model":"deepseek-v4-pro","input_tokens":25182,"output_tokens":34,"session_id":"...","status":"completed"}}
//!   {"type":"done"}

use serde_json::Value;

use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use crate::adapters::ExecutionUsage;
use crate::models::utc_timestamp;

/// Codewhale executor implementation。
///
/// `#[derive(Clone)]` 由 `BaseExecutor`（已经 derive Clone）和自身所有 Arc<Mutex<...>> 字段共同保证安全。
#[derive(Clone)]
pub struct CodewhaleExecutor {
    /// 共享状态：path + model + usage。
    base: BaseExecutor,
}

impl CodewhaleExecutor {
    pub fn new(path: String) -> Self {
        Self { base: BaseExecutor::new(path) }
    }
}

impl CodeExecutor for CodewhaleExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Codewhale
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    /// Build command args for a fresh execution (no session).
    /// Uses --auto to enable tool calls and --output-format stream-json for NDJSON output.
    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "exec".to_string(),
            "--auto".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            message.to_string(),
        ]
    }

    /// Build command args with optional session support.
    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "exec".to_string(),
            "--auto".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];
        if is_resume {
            if let Some(sid) = session_id {
                args.push("--session-id".to_string());
                args.push(sid.to_string());
            }
        }
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    /// Extract session_id from session_capture event.
    fn extract_session_id(&self, line: &str) -> Option<String> {
        let json = serde_json::from_str::<Value>(line).ok()?;
        if json.get("type")?.as_str()? == "session_capture" {
            json.get("content")?.as_str().map(String::from)
        } else {
            None
        }
    }

    /// Parse a single NDJSON line from codewhale stream-json output.
    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let json = serde_json::from_str::<Value>(trimmed).ok()?;
        let event_type = json.get("type")?.as_str()?;

        match event_type {
            "tool_use" => {
                // {"type":"tool_use","name":"exec_shell","id":"call_xxx","input":{"command":"ls"}}
                let name = json.get("name").and_then(Value::as_str).unwrap_or("unknown");
                let input = json.get("input");
                let command = input
                    .and_then(|v| v.get("command"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let input_json = input.map(|v| v.to_string());

                Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "tool_call".to_string(),
                    content: format!("Calling tool: {} with args: {}", name, command),
                    usage: None,
                    tool_name: Some(name.to_string()),
                    tool_input_json: input_json,
                })
            }
            "tool_result" => {
                // {"type":"tool_result","id":"call_xxx","output":"...","status":"success"}
                let output = json.get("output").and_then(Value::as_str).unwrap_or_default();
                let status = json.get("status").and_then(Value::as_str).unwrap_or("completed");
                let id = json.get("id").and_then(Value::as_str).unwrap_or_default();

                Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "tool_result".to_string(),
                    content: if output.is_empty() {
                        format!("[{}] (id: {})", status, id)
                    } else {
                        format!("[{}] {}", status, output)
                    },
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                })
            }
            "content" => {
                // {"type":"content","content":"Hello"}
                // Trim trailing whitespace/newlines to prevent extra line breaks in final result
                let content = json.get("content").and_then(Value::as_str).unwrap_or_default().trim_end();
                if content.is_empty() {
                    return None;
                }
                Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "text".to_string(),
                    content: content.to_string(),
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                })
            }
            "metadata" => {
                // {"type":"metadata","meta":{"model":"deepseek-v4-pro","input_tokens":25182,"output_tokens":34,"session_id":"...","status":"completed"}}
                let meta = json.get("meta")?;

                // Extract and store model
                if let Some(model) = meta.get("model").and_then(Value::as_str) {
                    *self.base.model.lock() = Some(model.to_string());
                }

                // Extract and store usage
                let input_tokens = meta.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output_tokens = meta.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

                if input_tokens > 0 || output_tokens > 0 {
                    let usage = ExecutionUsage {
                        input_tokens,
                        output_tokens,
                        cache_read_input_tokens: None,
                        cache_creation_input_tokens: None,
                        total_cost_usd: None,
                        duration_ms: None,
                    };
                    *self.base.usage.lock() = Some(usage);
                }

                let status = meta.get("status").and_then(Value::as_str).unwrap_or("completed");
                Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "tokens".to_string(),
                    content: format!(
                        "Tokens: input={}, output={}, status={}",
                        input_tokens, output_tokens, status
                    ),
                    usage: self.base.usage.lock().clone(),
                    tool_name: None,
                    tool_input_json: None,
                })
            }
            "done" => {
                // {"type":"done"} - terminal event, ignore
                None
            }
            _ => None,
        }
    }

    // parse_stderr_line 委托给 BaseExecutor 的默认关键字分类逻辑，
    // 根据是否包含 "error" 决定 log_type（error / stderr）。
    fn parse_stderr_line(&self, line: &str) -> Option<ParsedLogEntry> {
        BaseExecutor::default_parse_stderr_line(line)
    }

    // check_success 走 CodeExecutor 默认实现（委托给 BaseExecutor），
    // 与本文件以前的 in-class 实现完全等价。去掉重复 override 是 PR #536 的核心目标。

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        // CodeWhale streams text as small chunks (individual characters or words).
        // To preserve word boundaries and spacing between chunks, we must:
        // 1) Concatenate all raw chunks first (without per-chunk trimming)
        // 2) Then strip <think> tags from the full concatenated text once
        // 3) Finally normalize whitespace on the result
        // This prevents "  Hello  " + "  World  " from becoming "HelloWorld"
        // (the old approach trimmed each chunk separately, losing inter-chunk spaces).
        let raw_chunks: Vec<&str> = logs
            .iter()
            .filter(|l| l.log_type == "text")
            .map(|l| l.content.as_str())
            .collect();

        if !raw_chunks.is_empty() {
            // Concatenate all chunks with their original spacing preserved
            let concatenated = raw_chunks.join("");
            // Strip think tags from the full text (this also trims outer whitespace)
            let cleaned = super::strip_think_tags(&concatenated);
            // Normalize whitespace:
            // - Split by newlines to preserve multi-line structure
            // - collapse all whitespace within each line to single spaces (split_whitespace + join)
            // - filter empty lines after trimming
            let normalized = cleaned
                .split('\n')
                .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            // Only return non-empty results after normalization
            if !normalized.is_empty() {
                Some(normalized)
            } else {
                None
            }
        } else {
            // Fallback: last stderr entry when no text chunks exist
            logs.iter()
                .rev()
                .find(|l| l.log_type == "stderr")
                .map(|l| l.content.clone())
        }
    }

    fn get_usage(&self, _logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> {
        self.base.usage.lock().clone()
    }

    fn get_model(&self) -> Option<String> {
        self.base.model.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_args() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let args = executor.command_args("say hello");
        assert_eq!(
            args,
            vec![
                "exec",
                "--auto",
                "--output-format",
                "stream-json",
                "say hello"
            ]
        );
    }

    #[test]
    fn test_command_args_with_session() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let args = executor.command_args_with_session("say hello", Some("session-123"), true);
        assert_eq!(
            args,
            vec![
                "exec",
                "--auto",
                "--output-format",
                "stream-json",
                "--session-id",
                "session-123",
                "say hello"
            ]
        );
    }

    #[test]
    fn test_executor_type() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Codewhale);
    }

    #[test]
    fn test_parse_tool_use() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = r#"{"type":"tool_use","name":"exec_shell","id":"call_001","input":{"command":"ls -la"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert_eq!(entry.tool_name, Some("exec_shell".to_string()));
        assert!(entry.content.contains("ls -la"));
    }

    #[test]
    fn test_parse_tool_result() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = r#"{"type":"tool_result","id":"call_001","output":"file1.txt\nfile2.txt","status":"success"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert!(entry.content.contains("success"));
        assert!(entry.content.contains("file1.txt"));
    }

    #[test]
    fn test_parse_content() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = r#"{"type":"content","content":"Hello world"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_parse_content_empty() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = r#"{"type":"content","content":""}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_metadata() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = r#"{"type":"metadata","meta":{"model":"deepseek-v4-pro","input_tokens":25182,"output_tokens":34,"session_id":"abc123","status":"completed"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tokens");

        let usage = executor.get_usage(&[]).unwrap();
        assert_eq!(usage.input_tokens, 25182);
        assert_eq!(usage.output_tokens, 34);

        let model = executor.get_model().unwrap();
        assert_eq!(model, "deepseek-v4-pro");
    }

    #[test]
    fn test_parse_done() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = r#"{"type":"done"}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_unknown_type() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = r#"{"type":"unknown","data":"test"}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_invalid_json() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        assert!(executor.parse_output_line("not json").is_none());
    }

    #[test]
    fn test_extract_session_id() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = r#"{"type":"session_capture","content":"e5b5853f-31a1-4717-88b3-8e8838157586"}"#;
        let sid = executor.extract_session_id(line).unwrap();
        assert_eq!(sid, "e5b5853f-31a1-4717-88b3-8e8838157586");
    }

    #[test]
    fn test_extract_session_id_not_capture() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = r#"{"type":"content","content":"hello"}"#;
        assert!(executor.extract_session_id(line).is_none());
    }

    #[test]
    fn test_get_final_result_with_text() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  Hello  "),
            ParsedLogEntry::new("text", "  World  "),
        ];
        // CodeWhale streams text as small chunks; get_final_result must preserve
        // word boundaries across chunks:
        // 1) Concatenate all raw chunks first (without per-chunk trimming)
        // 2) Then strip <think> tags from the full concatenated text once
        // 3) Finally normalize whitespace on the result
        // This prevents inter-chunk spaces from being lost.
        // "  Hello  " + "  World  " → "Hello World"
        assert_eq!(executor.get_final_result(&logs), Some("Hello World".to_string()));
    }

    #[test]
    fn test_get_final_result_with_think_tags() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "<think>thinking过程</think>Hello"),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("Hello".to_string()));
    }

    #[test]
    fn test_get_final_result_fallback_to_stderr() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let logs = vec![
            ParsedLogEntry::new("stderr", "error output"),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("error output".to_string()));
    }

    #[test]
    fn test_supports_resume() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        assert!(executor.supports_resume());
    }

    #[test]
    fn test_parse_stderr_error() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = "ERROR: something went wrong";
        let entry = executor.parse_stderr_line(line).unwrap();
        assert_eq!(entry.log_type, "error");
        assert_eq!(entry.content, "ERROR: something went wrong");
    }

    #[test]
    fn test_parse_stderr_info() {
        let executor = CodewhaleExecutor::new("codewhale".to_string());
        let line = "Just some info";
        let entry = executor.parse_stderr_line(line).unwrap();
        assert_eq!(entry.log_type, "stderr");
        assert_eq!(entry.content, "Just some info");
    }
}
