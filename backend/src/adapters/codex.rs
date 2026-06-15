use serde_json::Value;

use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use crate::adapters::ExecutionUsage;
use crate::models::utc_timestamp;

/// Codex executor。
///
/// Codex 的 `parse_stderr_line` 与默认实现不同：
///   - 默认：error 关键字 → "error" log_type；其他 → "stderr"
///   - Codex：error 关键字 → "stderr" log_type；其他 → "info"
///
/// 这种反向分类是历史行为，必须保留 override，不能直接删除该方法。
// `BaseExecutor` 已经 `#[derive(Clone)]`，组合字段无需手写 Clone impl。
#[derive(Clone)]
pub struct CodexExecutor {
    base: BaseExecutor,
}

impl CodexExecutor {
    pub fn new(path: String) -> Self {
        Self { base: BaseExecutor::new(path) }
    }
}

impl CodeExecutor for CodexExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Codex
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "exec".to_string(),
            "--json".to_string(),
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "--skip-git-repo-check".to_string(),
            message.to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, _session_id: Option<&str>, _is_resume: bool) -> Vec<String> {
        self.command_args(message)
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let json = serde_json::from_str::<Value>(trimmed).ok()?;

        // Determine the event type and event object to use
        // New format: {"type":"item.started","item":{...}} or {"type":"agent_message",...}
        // Old format: {"msg":{"type":"session_configured",...}} or {"id":"0","msg":{...}}
        let (event_type, event) = if let Some(msg) = json.get("msg") {
            // Old format with nested msg field
            let typ = msg.get("type").and_then(Value::as_str)?;
            (typ, msg.clone())
        } else if let Some(typ) = json.get("type").and_then(Value::as_str) {
            // New format with top-level type field
            (typ, json.clone())
        } else {
            return None;
        };

        // Handle item.started / item.completed - extract inner item from top-level json
        if event_type == "item.started" || event_type == "item.completed" {
            let item = json.get("item")?;
            let inner_type = item.get("type").and_then(Value::as_str)?;

            match (event_type, inner_type) {
                ("item.started", "command_execution") => {
                    let command = item
                        .get("command")
                        .cloned()
                        .and_then(|v| command_to_string(Some(v)))
                        .unwrap_or_default();
                    return Some(ParsedLogEntry {
                        timestamp: utc_timestamp(),
                        log_type: "tool_call".to_string(),
                        content: format!("Executing command: {}", command),
                        usage: None,
                        tool_name: Some("command_execution".to_string()),
                        tool_input_json: item.get("command").and_then(|v| serde_json::to_string(v).ok()),
                    });
                }
                ("item.completed", "command_execution") => {
                    let output = item
                        .get("aggregated_output")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let exit_code = item.get("exit_code").and_then(Value::as_i64);
                    let status = item.get("status").and_then(Value::as_str).unwrap_or("completed");

                    let mut content = format!("[{}] ", status);
                    if !output.is_empty() {
                        content.push_str(output);
                    }
                    if let Some(code) = exit_code {
                        content.push_str(&format!(" (exit={})", code));
                    }

                    return Some(ParsedLogEntry {
                        timestamp: utc_timestamp(),
                        log_type: "tool_result".to_string(),
                        content,
                        usage: None,
                        tool_name: Some("command_execution".to_string()),
                        tool_input_json: None,
                    });
                }
                ("item.completed", "agent_message") => {
                    let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                    if text.is_empty() {
                        return None;
                    }
                    return Some(ParsedLogEntry {
                        timestamp: utc_timestamp(),
                        log_type: "text".to_string(),
                        content: text.to_string(),
                        usage: None,
                        tool_name: None,
                        tool_input_json: None,
                    });
                }
                _ => return None,
            }
        }

        // Handle turn events
        if event_type == "turn.completed" || event_type == "turn.started" {
            let log_type = if event_type == "turn.completed" {
                // Try to parse usage from turn.completed (pass full json, not usage_obj)
                if let Some(usage) = parse_usage(&json) {
                    *self.base.usage.lock() = Some(usage.clone());
                    return Some(ParsedLogEntry {
                        timestamp: utc_timestamp(),
                        log_type: "tokens".to_string(),
                        content: format!(
                            "Tokens: input={}, output={}",
                            usage.input_tokens, usage.output_tokens
                        ),
                        usage: Some(usage),
                        tool_name: None,
                        tool_input_json: None,
                    });
                }
                "step_finish"
            } else {
                "system"
            };
            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: log_type.to_string(),
                content: format!("Codex {}", event_type.replace(['.', '_'], " ")),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            });
        }

        // Handle thread events
        if event_type == "thread.started" {
            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "system".to_string(),
                content: "Codex thread started".to_string(),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            });
        }

        // Store model if present
        if let Some(model) = first_string(&event, &["model", "model_slug"]) {
            *self.base.model.lock() = Some(model);
        }

        // Parse usage if present
        if let Some(usage) = parse_usage(&event) {
            *self.base.usage.lock() = Some(usage.clone());
            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "tokens".to_string(),
                content: format!(
                    "Tokens: input={}, output={}",
                    usage.input_tokens, usage.output_tokens
                ),
                usage: Some(usage),
                tool_name: None,
                tool_input_json: None,
            });
        }

        match event_type {
            "session_configured" | "task_started" => Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "system".to_string(),
                content: format!("Codex {}", event_type.replace('_', " ")),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            }),
            "agent_message" | "agent_message_delta" | "assistant_message" => {
                let text = first_string(&event, &["message", "delta", "text", "content"])?;
                if text.is_empty() {
                    return None;
                }
                Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "text".to_string(),
                    content: text,
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                })
            }
            "agent_reasoning" | "agent_reasoning_delta" | "reasoning" | "reasoning_delta" => {
                let text = first_string(&event, &["message", "delta", "text", "content"])?;
                if text.is_empty() {
                    return None;
                }
                Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "thinking".to_string(),
                    content: text,
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                })
            }
            "exec_command_begin" | "tool_call_begin" | "tool_call" => {
                let tool_name = first_string(&event, &["tool_name", "name"])
                    .unwrap_or_else(|| "exec".to_string());
                let command = command_to_string(event.get("command").cloned())
                    .or_else(|| first_string(&event, &["cmd", "arguments", "input"]))
                    .unwrap_or_default();
                Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "tool_call".to_string(),
                    content: if command.is_empty() {
                        format!("Calling tool: {}", tool_name)
                    } else {
                        format!("Calling tool: {} with args: {}", tool_name, command)
                    },
                    usage: None,
                    tool_name: Some(tool_name),
                    tool_input_json: event
                        .get("command")
                        .or_else(|| event.get("arguments"))
                        .and_then(|v| serde_json::to_string(v).ok()),
                })
            }
            "exec_command_end" | "tool_call_end" | "tool_result" => {
                let stdout =
                    first_string(&event, &["stdout", "output", "result", "aggregated_output"]).unwrap_or_default();
                let stderr = first_string(&event, &["stderr"]).unwrap_or_default();
                let exit_code = event.get("exit_code").and_then(Value::as_i64);
                let mut content = String::new();
                if let Some(code) = exit_code {
                    content.push_str(&format!("exit_code={}", code));
                }
                if !stdout.is_empty() {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(&stderr);
                }
                if content.is_empty() {
                    content = "Tool finished".to_string();
                }
                Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "tool_result".to_string(),
                    content,
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                })
            }
            "task_complete" => Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "step_finish".to_string(),
                content: "Codex finished".to_string(),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            }),
            "error" => Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "error".to_string(),
                content: first_string(&event, &["message", "error"])
                    .unwrap_or_else(|| event.to_string()),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            }),
            _ => None,
        }
    }

    fn parse_stderr_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        Some(ParsedLogEntry {
            timestamp: utc_timestamp(),
            log_type: if trimmed.to_lowercase().contains("error") {
                "stderr".to_string()
            } else {
                "info".to_string()
            },
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
        self.base.usage.lock().clone()
    }

    fn get_model(&self) -> Option<String> {
        self.base.model.lock().clone()
    }
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::to_string)
}

fn command_to_string(value: Option<Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        other => Some(other.to_string()),
    }
}

fn parse_usage(event: &Value) -> Option<ExecutionUsage> {
    let usage = event
        .get("usage")
        .or_else(|| event.get("token_usage"))
        .or_else(|| event.pointer("/info/total_token_usage"))?;

    let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

    if input_tokens == 0 && output_tokens == 0 {
        return None;
    }

    Some(ExecutionUsage {
        input_tokens,
        output_tokens,
        cache_read_input_tokens: usage.get("cached_input_tokens").and_then(|v| v.as_u64()),
        cache_creation_input_tokens: usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()),
        total_cost_usd: usage.get("total_cost_usd").and_then(|v| v.as_f64()),
        duration_ms: event.get("duration_ms").and_then(|v| v.as_u64()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_args() {
        let executor = CodexExecutor::new("codex".to_string());
        let args = executor.command_args("say hello");
        assert_eq!(
            args,
            vec![
                "exec",
                "--json",
                "--dangerously-bypass-approvals-and-sandbox",
                "--skip-git-repo-check",
                "say hello"
            ]
        );
    }

    #[test]
    fn test_executor_type() {
        let executor = CodexExecutor::new("codex".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Codex);
    }

    #[test]
    fn test_parse_thread_started() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"thread.started","thread_id":"abc123"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "system");
        assert_eq!(entry.content, "Codex thread started");
    }

    #[test]
    fn test_parse_turn_started() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"turn.started"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "system");
        assert_eq!(entry.content, "Codex turn started");
    }

    #[test]
    fn test_parse_item_started_command_execution() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"/bin/zsh -lc date"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert_eq!(entry.tool_name, Some("command_execution".to_string()));
        assert!(entry.content.contains("Executing command:"));
        assert!(entry.content.contains("/bin/zsh"));
    }

    #[test]
    fn test_parse_item_completed_command_execution() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"item.completed","item":{"id":"item_0","type":"command_execution","command":"/bin/zsh -lc date","aggregated_output":"Fri May  1 03:33:39 PDT 2026\n","exit_code":0,"status":"completed"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert_eq!(entry.tool_name, Some("command_execution".to_string()));
        assert!(entry.content.contains("[completed]"));
        assert!(entry.content.contains("Fri May"));
        assert!(entry.content.contains("exit=0"));
    }

    #[test]
    fn test_parse_item_completed_agent_message() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"这是AI的回复"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "这是AI的回复");
    }

    #[test]
    fn test_parse_turn_completed_with_usage() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"turn.completed","usage":{"input_tokens":46503,"cached_input_tokens":45824,"output_tokens":90,"reasoning_output_tokens":0}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tokens");
        let usage = executor.get_usage(&[]).unwrap();
        assert_eq!(usage.input_tokens, 46503);
        assert_eq!(usage.output_tokens, 90);
        assert_eq!(usage.cache_read_input_tokens, Some(45824));
    }
}
