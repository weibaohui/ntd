use std::sync::Arc;
use parking_lot::Mutex;

use super::{CodeExecutor, ExecutorType, ParsedLogEntry, ExecutionUsage};
use crate::models::{utc_timestamp, TodoItem};

pub struct HermesExecutor {
    path: String,
    usage: Arc<Mutex<Option<ExecutionUsage>>>,
    has_done: Arc<Mutex<bool>>,
    session_id: Arc<Mutex<Option<String>>>,
    tool_calls_count: Arc<Mutex<u64>>,
}

impl HermesExecutor {
    pub fn new(path: String) -> Self {
        Self {
            path,
            usage: Arc::new(Mutex::new(None)),
            has_done: Arc::new(Mutex::new(false)),
            session_id: Arc::new(Mutex::new(None)),
            tool_calls_count: Arc::new(Mutex::new(0)),
        }
    }
}

impl Clone for HermesExecutor {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            usage: self.usage.clone(),
            has_done: self.has_done.clone(),
            session_id: self.session_id.clone(),
            tool_calls_count: self.tool_calls_count.clone(),
        }
    }
}

impl CodeExecutor for HermesExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Hermes
    }

    fn executable_path(&self) -> &str {
        &self.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "chat".to_string(),
            "-q".to_string(),
            message.to_string(),
            "--yolo".to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        if is_resume {
            let mut args = vec!["chat".to_string()];
            args.push("-q".to_string());
            args.push(message.to_string());
            args.push("--resume".to_string());
            if let Some(sid) = session_id {
                args.push(sid.to_string());
            }
            args.push("--yolo".to_string());
            args
        } else {
            self.command_args(message)
        }
    }

    fn supports_resume(&self) -> bool {
        true
    }

    fn extract_session_id(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        // Format: "hermes --resume <session_id>"
        if let Some(rest) = trimmed.strip_prefix("hermes --resume ") {
            let sid = rest.trim().to_string();
            if !sid.is_empty() {
                return Some(sid);
            }
        }
        // Format: "hermes chat ... --resume <session_id> ..."
        if trimmed.starts_with("hermes chat ") {
            if let Some(after_hermes_chat) = trimmed.strip_prefix("hermes chat ") {
                if let Some(resume_pos) = after_hermes_chat.find("--resume ") {
                    let after_resume = &after_hermes_chat[resume_pos + 9..]; // 9 = len("--resume ")
                    let sid = after_resume.split_whitespace().next()?;
                    if !sid.is_empty() {
                        return Some(sid.to_string());
                    }
                }
            }
        }
        // Format: "Session: <id>"
        if let Some(rest) = trimmed.strip_prefix("Session:") {
            let sid = rest.trim().to_string();
            if !sid.is_empty() {
                return Some(sid);
            }
        }
        // Format: "session_id: <id>"
        if let Some(rest) = trimmed.strip_prefix("session_id:") {
            let sid = rest.trim().to_string();
            if !sid.is_empty() {
                return Some(sid);
            }
        }
        None
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Skip banner lines and special formatting
        if trimmed.starts_with("╭") || trimmed.starts_with("│") || trimmed.starts_with("╰") {
            return None;
        }

        // Parse session_id from output: "Session: <id>" or "session_id: <id>"
        if trimmed.starts_with("Session:") {
            let sid = trimmed.strip_prefix("Session:").unwrap_or("").trim().to_string();
            if !sid.is_empty() {
                *self.session_id.lock() = Some(sid);
            }
            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "info".to_string(),
                content: trimmed.to_string(),
                usage: None,
            tool_name: None,
            tool_input_json: None,
            });
        }
        if trimmed.starts_with("session_id:") {
            let sid = trimmed.strip_prefix("session_id:").unwrap_or("").trim().to_string();
            if !sid.is_empty() {
                *self.session_id.lock() = Some(sid);
            }
            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "info".to_string(),
                content: trimmed.to_string(),
                usage: None,
            tool_name: None,
            tool_input_json: None,
            });
        }

        // Skip status indicators
        if trimmed.starts_with("┊") {
            return None;
        }

        // Skip empty box characters
        if trimmed.chars().all(|c| c == ' ' || c == '━' || c == '│' || c == '╰' || c == '╭') {
            return None;
        }

        // Parse "Messages: X (Y user, Z tool calls)" to extract tool calls count
        if trimmed.starts_with("Messages:") {
            // Example: "Messages: 4 (1 user, 2 tool calls)"
            if let Some(calls_part) = trimmed.split("tool calls").next() {
                // calls_part = "Messages: 4 (1 user, 2 "
                if let Some(num_part) = calls_part.rsplit(',').next() {
                    // num_part = " 2 "
                    let num_str = num_part.trim();
                    if let Ok(count) = num_str.parse::<u64>() {
                        *self.tool_calls_count.lock() = count;
                    }
                }
            }
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
        if trimmed.is_empty() {
            return None;
        }

        // Classify stderr content by its nature - Hermes often outputs info to stderr
        let log_type = if trimmed.contains("error") || trimmed.contains("Error") || trimmed.contains("ERROR") || trimmed.contains("failed") || trimmed.contains("Failed") {
            "stderr".to_string()
        } else {
            "info".to_string()
        };

        Some(ParsedLogEntry {
            timestamp: utc_timestamp(),
            log_type,
            content: trimmed.to_string(),
            usage: None,
            tool_name: None,
            tool_input_json: None,
        })
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

    fn get_tool_calls_count(&self) -> Option<u64> {
        let count = *self.tool_calls_count.lock();
        if count > 0 {
            Some(count)
        } else {
            None
        }
    }

    fn post_execution_todo_progress(&self) -> Option<Vec<TodoItem>> {
        let sid = self.session_id.lock().clone()?;
        let home = dirs::home_dir()?;
        let session_path = home.join(".hermes").join("sessions").join(format!("session_{}.json", sid));

        let content = std::fs::read_to_string(&session_path).ok()?;
        let data: serde_json::Value = serde_json::from_str(&content).ok()?;

        let messages = data.get("messages")?.as_array()?;
        let mut latest_todos: Option<Vec<TodoItem>> = None;

        for msg in messages {
            if msg.get("role").and_then(|v| v.as_str()) != Some("tool") {
                continue;
            }
            let content = msg.get("content")?.as_str()?;
            let tool_result: serde_json::Value = serde_json::from_str(content).ok()?;
            let Some(todos) = tool_result.get("todos").and_then(|v| v.as_array()) else {
                continue;
            };
            let items: Vec<TodoItem> = todos
                .iter()
                .filter_map(|t| {
                    let content = t.get("content").or_else(|| t.get("title"))?.as_str()?;
                    if content.is_empty() {
                        return None;
                    }
                    let raw_status = t
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("pending");
                    let status = match raw_status.to_lowercase().as_str() {
                        "done" | "completed" | "complete" | "finished" => "completed".to_string(),
                        "in_progress" | "inprogress" | "in-progress" | "doing" | "active" => "in_progress".to_string(),
                        "cancelled" | "canceled" | "abort" | "aborted" => "cancelled".to_string(),
                        "failed" | "fail" | "error" => "failed".to_string(),
                        "running" => "running".to_string(),
                        _ => "pending".to_string(),
                    };
                    Some(TodoItem {
                        id: t.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        content: content.to_string(),
                        status,
                    })
                })
                .collect();
            if !items.is_empty() {
                latest_todos = Some(items);
            }
        }

        latest_todos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_line_text() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_output_line("Hello world").unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_parse_output_line_empty() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.parse_output_line("").is_none());
        assert!(executor.parse_output_line("   ").is_none());
    }

    #[test]
    fn test_parse_output_line_session_id() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_output_line("session_id: abc123").unwrap();
        assert_eq!(entry.log_type, "info");
        assert!(entry.content.contains("session_id"));
    }

    #[test]
    fn test_parse_output_line_banner() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.parse_output_line("╭─ Hermes ─────────────────────────────────").is_none());
        assert!(executor.parse_output_line("│ some text").is_none());
    }

    #[test]
    fn test_get_final_result_with_text() {
        let executor = HermesExecutor::new("hermes".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  hello world  "),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello world".to_string()));
    }

    #[test]
    fn test_get_usage_before_tokens() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.get_usage(&[]).is_none());
    }

    #[test]
    fn test_command_args() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args("do something");
        assert_eq!(args, vec!["chat", "-q", "do something", "--yolo"]);
    }

    #[test]
    fn test_executor_type() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Hermes);
    }

    #[test]
    fn test_supports_resume() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.supports_resume());
    }

    #[test]
    fn test_extract_session_id_resume_format() {
        let executor = HermesExecutor::new("hermes".to_string());
        let sid = executor.extract_session_id("hermes --resume 20260517_051220_95e4d6");
        assert_eq!(sid, Some("20260517_051220_95e4d6".to_string()));
    }

    #[test]
    fn test_extract_session_id_session_prefix() {
        let executor = HermesExecutor::new("hermes".to_string());
        let sid = executor.extract_session_id("Session: mysession123");
        assert_eq!(sid, Some("mysession123".to_string()));
    }

    #[test]
    fn test_extract_session_id_lowercase_prefix() {
        let executor = HermesExecutor::new("hermes".to_string());
        let sid = executor.extract_session_id("session_id: abc_xyz_789");
        assert_eq!(sid, Some("abc_xyz_789".to_string()));
    }

    #[test]
    fn test_extract_session_id_no_match() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.extract_session_id("Hello world").is_none());
        assert!(executor.extract_session_id("").is_none());
        assert!(executor.extract_session_id("hermes chat -q test").is_none());
    }

    #[test]
    fn test_command_args_with_session_new() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args_with_session("do something", Some("task_id"), false);
        assert_eq!(args, vec!["chat", "-q", "do something", "--yolo"]);
    }

    #[test]
    fn test_command_args_with_session_resume() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args_with_session("continue", Some("session_123"), true);
        assert_eq!(args, vec!["chat", "-q", "continue", "--resume", "session_123", "--yolo"]);
    }

    #[test]
    fn test_command_args_with_session_resume_none() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args_with_session("continue", None, true);
        assert_eq!(args, vec!["chat", "-q", "continue", "--resume", "--yolo"]);
    }
}
