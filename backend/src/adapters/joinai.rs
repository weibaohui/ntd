use std::sync::Arc;
use parking_lot::Mutex;

use super::{CodeExecutor, ExecutorType, ParsedLogEntry, ExecutionUsage};
use super::joinai_event::JoinaiAgentEvent;
use crate::models::utc_timestamp;

pub struct JoinaiExecutor {
    path: String,
    usage: Arc<Mutex<Option<ExecutionUsage>>>,
}

impl JoinaiExecutor {
    pub fn new(path: String) -> Self {
        Self { path, usage: Arc::new(Mutex::new(None)) }
    }
}

impl Clone for JoinaiExecutor {
    fn clone(&self) -> Self {
        Self { path: self.path.clone(), usage: self.usage.clone() }
    }
}

impl CodeExecutor for JoinaiExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Joinai
    }

    fn executable_path(&self) -> &str {
        &self.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "run".to_string(),
            "--agent".to_string(),
            "yolo".to_string(),
            "--format".to_string(),
            "json".to_string(),
            message.to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--agent".to_string(),
            "yolo".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        if is_resume {
            if let Some(sid) = session_id {
                args.push("-s".to_string());
                args.push(sid.to_string());
            }
        }
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    fn extract_session_id(&self, line: &str) -> Option<String> {
        let event: JoinaiAgentEvent = serde_json::from_str(line).ok()?;
        event.session_id.or_else(|| event.part.as_ref()?.session_id.clone())
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let event: JoinaiAgentEvent = serde_json::from_str(line).ok()?;

        let timestamp = event.timestamp
            .map(|ts| {
                let raw = ts.0;
                // Try ISO 8601 first (new version format)
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&raw) {
                    return dt.with_timezone(&chrono::Utc)
                        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                        .to_string();
                }
                // Try as numeric string (milliseconds or seconds)
                if let Ok(ts_f) = raw.parse::<f64>() {
                    let ts_ms = if ts_f > 1e12 { ts_f as i64 } else { (ts_f * 1000.0) as i64 };
                    if let Some(dt) = chrono::DateTime::from_timestamp_millis(ts_ms) {
                        return dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                    }
                }
                utc_timestamp()
            })
            .unwrap_or_else(utc_timestamp);

        match event.event_type.as_str() {
            "step_start" => {
                *self.usage.lock() = None;
                Some(ParsedLogEntry {
                    timestamp,
                    log_type: "step_start".to_string(),
                    content: "Step started".to_string(),
                    usage: None,
            tool_name: None,
            tool_input_json: None,
                })
            }
            "tool_use" => {
                let part = event.part?;
                let tool = part.tool.unwrap_or_default();
                let status = part.state.as_ref().and_then(|s| s.status.clone()).unwrap_or_default();
                let description = part.state.as_ref().and_then(|s| s.input.as_ref().and_then(|i| i.description.clone())).unwrap_or_default();
                let input_json = part.state.as_ref()
                    .and_then(|s| s.input.as_ref())
                    .map(|i| i.to_full_json());

                let content = if tool == "bash" {
                    if let Some(output) = &part.state.as_ref().and_then(|s| s.output.clone()) {
                        format!("[{}] {}: {}", status, description, output)
                    } else {
                        format!("[{}] {}", status, description)
                    }
                } else {
                    format!("[{}] Tool: {} - {}", status, tool, description)
                };

                let tool_name = if tool.trim().is_empty() {
                    None
                } else {
                    Some(tool)
                };

                Some(ParsedLogEntry {
                    timestamp,
                    log_type: "tool".to_string(),
                    content,
                    usage: None,
                    tool_name,
                    tool_input_json: input_json,
                })
            }
            "text" => {
                let text = event.part?.text.unwrap_or_default();
                if text.is_empty() {
                    return None;
                }
                Some(ParsedLogEntry {
                    timestamp,
                    log_type: "text".to_string(),
                    content: text,
                    usage: None,
            tool_name: None,
            tool_input_json: None,
                })
            }
            "step_finish" => {
                // Store usage info if available
                if let Some(part) = &event.part {
                    if let Some(tokens) = &part.tokens {
                        let usage = ExecutionUsage {
                            input_tokens: tokens.input,
                            output_tokens: tokens.output,
                            cache_read_input_tokens: if tokens.cache.read > 0 { Some(tokens.cache.read) } else { None },
                            cache_creation_input_tokens: if tokens.cache.write > 0 { Some(tokens.cache.write) } else { None },
                            total_cost_usd: part.cost,
                            duration_ms: None,
                        };
                        *self.usage.lock() = Some(usage);
                    }
                }
                Some(ParsedLogEntry {
                    timestamp,
                    log_type: "step_finish".to_string(),
                    content: "Step finished".to_string(),
                    usage: None,
            tool_name: None,
            tool_input_json: None,
                })
            }
            _ => None,
        }
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        super::default_final_result_with_think_stripping(logs)
    }

    fn get_usage(&self, _logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> {
        self.usage.lock().clone()
    }

    fn get_model(&self) -> Option<String> {
        // Joinai doesn't provide model info
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ParsedLogEntry;

    #[test]
    fn test_parse_output_line_step_start() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert_eq!(entry.content, "Step started");
    }

    #[test]
    fn test_parse_output_line_tool_use_bash() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"tool_use","timestamp":1700000000000,"part":{"type":"tool_use","tool":"bash","state":{"status":"success","input":{"description":"list files"},"output":"file.txt"}}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool");
        assert!(entry.content.contains("success"), "content should contain status: {}", entry.content);
        assert!(entry.content.contains("list files"), "content should contain description: {}", entry.content);
        assert!(entry.content.contains("file.txt"), "content should contain output: {}", entry.content);
    }

    #[test]
    fn test_parse_output_line_text() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"part":{"type":"text","text":"hello world"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "hello world");
    }

    #[test]
    fn test_parse_output_line_step_finish_stores_usage() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"step_finish","timestamp":1700000000000,"part":{"type":"step_finish","tokens":{"total":100,"input":50,"output":50,"cache":{"read":10,"write":5}},"cost":0.001}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_finish");
        assert_eq!(entry.content, "Step finished");

        let usage = executor.get_usage(&[]).unwrap();
        assert_eq!(usage.input_tokens, 50);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_input_tokens, Some(10));
        assert_eq!(usage.cache_creation_input_tokens, Some(5));
        assert_eq!(usage.total_cost_usd, Some(0.001));
    }

    #[test]
    fn test_parse_output_line_unknown_type() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"unknown","timestamp":1700000000000}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_invalid_json() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = "not json";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_empty_text() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"part":{"type":"text","text":""}}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_get_final_result_with_text() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  hello world  "),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello world".to_string()));
    }

    #[test]
    fn test_get_final_result_fallback_to_stderr() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let logs = vec![
            ParsedLogEntry::new("stderr", "error output"),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("error output".to_string()));
    }

    #[test]
    fn test_get_final_result_empty_logs() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let logs: Vec<ParsedLogEntry> = vec![];
        assert!(executor.get_final_result(&logs).is_none());
    }

    #[test]
    fn test_get_usage_before_step_finish() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        assert!(executor.get_usage(&[]).is_none());
    }

    #[test]
    fn test_get_model_always_none() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        assert!(executor.get_model().is_none());
    }

    #[test]
    fn test_parse_output_line_with_iso_timestamp() {
        // New version: ISO 8601 string timestamp
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"step_start","timestamp":"2026-05-12T06:08:58.721Z","content":"Step started"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert_eq!(entry.content, "Step started");
        assert_eq!(entry.timestamp, "2026-05-12T06:08:58.721Z");
    }

    #[test]
    fn test_parse_output_line_with_number_timestamp_milliseconds() {
        // Milliseconds format (13+ digits) should still work
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000,"content":"Step started"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert!(entry.timestamp.starts_with("2023-"));
    }
}


