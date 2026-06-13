//! MiMo executor adapter
//!
//! MiMo is Xiaomi's open-source AI coding CLI, compatible with Anthropic SDK protocol.
//! Supports session resumption, JSON streaming output, and Yolo mode.

use std::sync::Arc;
use parking_lot::Mutex;

use super::{CodeExecutor, ExecutorType, ParsedLogEntry, ExecutionUsage};
use super::mimo_event::MimoEvent;
use crate::models::utc_timestamp;

pub struct MimoExecutor {
    path: String,
    /// 从事件中提取的 model 名称
    model: Arc<Mutex<Option<String>>>,
    /// 累计 usage（从 step_finish 事件中提取）
    usage: Arc<Mutex<Option<ExecutionUsage>>>,
    /// 标记是否成功完成（MiMo 可能返回非零退出码但执行成功）
    has_successful_finish: Arc<Mutex<bool>>,
}

impl MimoExecutor {
    pub fn new(path: String) -> Self {
        Self {
            path,
            model: Arc::new(Mutex::new(None)),
            usage: Arc::new(Mutex::new(None)),
            has_successful_finish: Arc::new(Mutex::new(false)),
        }
    }
}

impl Clone for MimoExecutor {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            model: self.model.clone(),
            usage: self.usage.clone(),
            has_successful_finish: self.has_successful_finish.clone(),
        }
    }
}

impl CodeExecutor for MimoExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Mimo
    }

    fn executable_path(&self) -> &str {
        &self.path
    }

    /// 基本命令参数：单次执行，使用 JSON 格式输出，开启 Yolo 模式跳过权限确认
    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--dangerously-skip-permissions".to_string(),
            message.to_string(),
        ]
    }

    /// 带 session 的命令参数
    /// MiMo 使用 `-s <session_id>` 续接指定 session，`-c` 续接最近 session
    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        if is_resume {
            // 恢复模式：续接最近 session（-c）或指定 session（-s）
            if let Some(sid) = session_id {
                args.push("-s".to_string());
                args.push(sid.to_string());
            } else {
                args.push("-c".to_string());
            }
        }
        args.push("--dangerously-skip-permissions".to_string());
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    /// 从 step_start 事件中提取 session_id
    fn extract_session_id(&self, line: &str) -> Option<String> {
        let event: MimoEvent = serde_json::from_str(line).ok()?;
        event.session_id.or_else(|| event.part.as_ref()?.session_id.clone())
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let event: MimoEvent = serde_json::from_str(line).ok()?;

        let timestamp = event.timestamp
            .and_then(|ts| chrono::DateTime::from_timestamp_millis(ts as i64))
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
            .unwrap_or_else(utc_timestamp);

        match event.event_type.as_str() {
            "step_start" => {
                // 重置完成状态
                *self.has_successful_finish.lock() = false;
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
                let description = part.state.as_ref()
                    .and_then(|s| s.input.as_ref()?.description.clone())
                    .unwrap_or_default();
                let input_json = part.state.as_ref()
                    .and_then(|s| s.input.as_ref())
                    .map(|i| i.to_full_json());

                // 从 state.output 中提取命令输出（用于 bash 工具）
                let content = if tool == "bash" {
                    if let Some(output) = &part.state.as_ref().and_then(|s| s.output.clone()) {
                        format!("[{}] {}: {}", status, description, output)
                    } else {
                        format!("[{}] {}", status, description)
                    }
                } else {
                    format!("[{}] Tool: {} - {}", status, tool, description)
                };

                Some(ParsedLogEntry {
                    timestamp,
                    log_type: "tool".to_string(),
                    content,
                    usage: None,
                    tool_name: Some(tool),
                    tool_input_json: input_json,
                })
            }
            "text" => {
                let part = event.part?;
                let text = part.text.clone().unwrap_or_default();
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
            "reasoning" => {
                // 思考过程（需要 --thinking 参数才会输出）
                let part = event.part?;
                let text = part.text.clone().unwrap_or_default();
                if text.is_empty() {
                    return None;
                }
                Some(ParsedLogEntry {
                    timestamp,
                    log_type: "thinking".to_string(),
                    content: text.chars().take(500).collect(),
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                })
            }
            "step_finish" => {
                // 标记为成功完成
                *self.has_successful_finish.lock() = true;

                // 提取 usage 信息
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
        // 收集所有 text 类型的日志条目作为最终结果
        let texts: Vec<String> = logs.iter()
            .filter(|l| l.log_type == "text")
            .map(|l| l.content.clone())
            .filter(|t| !t.trim().is_empty())
            .collect();

        if !texts.is_empty() {
            Some(texts.join("\n\n"))
        } else {
            None
        }
    }

    fn get_usage(&self, _logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> {
        self.usage.lock().clone()
    }

    fn get_model(&self) -> Option<String> {
        self.model.lock().clone()
    }

    /// MiMo 可能返回非零退出码（如被信号打断），但只要收到了 step_finish 事件就算成功
    fn check_success(&self, exit_code: i32) -> bool {
        if exit_code == 0 {
            return true;
        }
        *self.has_successful_finish.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_line_step_start() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000,"sessionID":"ses_xxx"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert_eq!(entry.content, "Step started");
    }

    #[test]
    fn test_parse_output_line_tool_use_bash() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"tool_use","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"tool","tool":"bash","state":{"status":"completed","input":{"command":"echo hello","description":"Print hello"},"output":"hello\n"}}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool");
        assert!(entry.content.contains("completed"));
        // description (Print hello) is shown for bash tools, not the raw command
        assert!(entry.content.contains("Print hello"));
        assert!(entry.content.contains("hello"));
    }

    #[test]
    fn test_parse_output_line_text() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"text","text":"Hello world"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_parse_output_line_step_finish_stores_usage() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"step_finish","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"step_finish","tokens":{"total":32086,"input":29146,"output":36,"reasoning":24,"cache":{"write":0,"read":2880}},"cost":0}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_finish");

        let usage = executor.get_usage(&[]).unwrap();
        assert_eq!(usage.input_tokens, 29146);
        assert_eq!(usage.output_tokens, 36);
        assert_eq!(usage.cache_read_input_tokens, Some(2880));
        assert_eq!(usage.total_cost_usd, Some(0.0));
    }

    #[test]
    fn test_parse_output_line_reasoning() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"reasoning","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"reasoning","text":"thinking..."}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "thinking");
        assert_eq!(entry.content, "thinking...");
    }

    #[test]
    fn test_parse_output_line_unknown_type() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"unknown","timestamp":1700000000000}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_invalid_json() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = "not json";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_empty_text() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"text","text":""}}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_extract_session_id() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000,"sessionID":"ses_1419497e9ffePCoFjjKuVNZnte"}"#;
        let sid = executor.extract_session_id(line);
        assert_eq!(sid, Some("ses_1419497e9ffePCoFjjKuVNZnte".to_string()));
    }

    #[test]
    fn test_extract_session_id_from_part() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"tool_use","timestamp":1700000000000,"part":{"type":"tool","sessionID":"ses_part_sid"}}"#;
        let sid = executor.extract_session_id(line);
        assert_eq!(sid, Some("ses_part_sid".to_string()));
    }

    #[test]
    fn test_get_final_result_joins_texts() {
        let executor = MimoExecutor::new("mimo".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "hello"),
            ParsedLogEntry::new("text", "world"),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello\n\nworld".to_string()));
    }

    #[test]
    fn test_get_final_result_empty() {
        let executor = MimoExecutor::new("mimo".to_string());
        let logs: Vec<ParsedLogEntry> = vec![];
        assert!(executor.get_final_result(&logs).is_none());
    }

    #[test]
    fn test_check_success_exit_code_zero() {
        let executor = MimoExecutor::new("mimo".to_string());
        assert!(executor.check_success(0));
    }

    #[test]
    fn test_check_success_non_zero_without_step_finish() {
        let executor = MimoExecutor::new("mimo".to_string());
        assert!(!executor.check_success(144));
    }

    #[test]
    fn test_check_success_non_zero_with_step_finish() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"step_finish","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"step_finish","tokens":{"total":100,"input":50,"output":50,"reasoning":0,"cache":{"read":0,"write":0}},"cost":0}}"#;
        let _ = executor.parse_output_line(line);
        assert!(executor.check_success(144));
    }

    #[test]
    fn test_command_args_basic() {
        let executor = MimoExecutor::new("mimo".to_string());
        let args = executor.command_args("hello world");
        assert_eq!(args, vec!["run", "--format", "json", "--dangerously-skip-permissions", "hello world"]);
    }

    #[test]
    fn test_command_args_with_session_resume() {
        let executor = MimoExecutor::new("mimo".to_string());
        let args = executor.command_args_with_session("hello", Some("ses_xxx"), true);
        assert!(args.contains(&"-s".to_string()));
        assert!(args.contains(&"ses_xxx".to_string()));
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn test_command_args_with_session_continue() {
        let executor = MimoExecutor::new("mimo".to_string());
        // is_resume=true 但没有 session_id，应该用 -c
        let args = executor.command_args_with_session("hello", None, true);
        assert!(args.contains(&"-c".to_string()));
    }

    #[test]
    fn test_command_args_with_session_new() {
        let executor = MimoExecutor::new("mimo".to_string());
        // is_resume=false，新 session，不带 -c 或 -s
        let args = executor.command_args_with_session("hello", Some("ses_xxx"), false);
        assert!(!args.contains(&"-c".to_string()));
        assert!(!args.contains(&"-s".to_string()));
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
    }
}
