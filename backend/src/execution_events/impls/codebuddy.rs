//! Codebuddy 执行器的事件提取器实现
//!
//! Codebuddy 使用 Claude Protocol 的 JSON 格式（与 ClaudeCode 完全一致）：
//! - system: 系统消息（含 session_id / model）
//! - assistant: 助手回复（content[] 含 text / thinking / tool_use）
//! - user: 用户消息（content[] 含 tool_result）
//! - result: 执行结果（含 result / duration_ms / total_cost_usd / usage）

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Codebuddy 事件提取器
#[derive(Debug, Clone)]
pub struct CodebuddyExtractor {
    metadata: ExecutionMetadata,
}

impl CodebuddyExtractor {
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("codebuddy".to_string()),
        }
    }

    fn parse_json_line(&mut self, json: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();
        let event_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "system" => {
                // 系统消息：提取 session_id 和 model
                if let Some(sid) = json.get("session_id").and_then(|v| v.as_str()) {
                    if self.metadata.session_id.is_none() {
                        self.metadata.session_id = Some(sid.to_string());
                        events.push(ExecutionEvent::SessionStart {
                            session_id: sid.to_string(),
                        });
                    }
                }
                if let Some(model) = json.get("model").and_then(|v| v.as_str()) {
                    if self.metadata.model.is_none() {
                        self.metadata.model = Some(model.to_string());
                        events.push(ExecutionEvent::ModelSwitch {
                            model: model.to_string(),
                        });
                    }
                }
            }
            "assistant" => {
                // 助手消息：从 message.content[] 中提取 thinking / text / tool_use
                if let Some(msg) = json.get("message") {
                    if let Some(content) = msg.get("content").and_then(|v| v.as_array()) {
                        for block in content {
                            let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            match block_type {
                                "thinking" => {
                                    if let Some(t) = block.get("thinking").and_then(|v| v.as_str()) {
                                        let trimmed = t.trim();
                                        if !trimmed.is_empty() {
                                            events.push(ExecutionEvent::Thinking {
                                                content: trimmed.to_string(),
                                            });
                                        }
                                    }
                                }
                                "text" => {
                                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                                        let trimmed = t.trim();
                                        if !trimmed.is_empty() {
                                            events.push(ExecutionEvent::Assistant {
                                                content: trimmed.to_string(),
                                                thinking: None,
                                                message_id: msg.get("id").and_then(|v| v.as_str()).map(String::from),
                                            });
                                        }
                                    }
                                }
                                "tool_use" | "toolUse" => {
                                    let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                                    let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                                    events.push(ExecutionEvent::ToolCall {
                                        id: block.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                        name: name.to_string(),
                                        input,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            "user" => {
                // 用户消息：从 message.content[] 中提取 tool_result
                if let Some(msg) = json.get("message") {
                    if let Some(content) = msg.get("content").and_then(|v| v.as_array()) {
                        for block in content {
                            let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            if block_type == "tool_result" || block_type == "toolResult" {
                                // tool_result 的 content 可能是字符串或数组
                                // 例如: "content":"plain text" 或 "content":[{"type":"text","text":"..."}]
                                let output = block.get("content")
                                    .and_then(|v| {
                                        v.as_str().map(String::from).or_else(|| {
                                            // 数组格式：从所有 text block 中拼接
                                            v.as_array().map(|arr| {
                                                arr.iter()
                                                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                                                    .collect::<Vec<_>>()
                                                    .join("\n")
                                            })
                                        })
                                    })
                                    .unwrap_or_default();
                                events.push(ExecutionEvent::ToolResult {
                                    call_id: block.get("tool_use_id").or_else(|| block.get("toolUseId")).and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                    output,
                                    is_error: block.get("is_error").or_else(|| block.get("isError")).and_then(|v| v.as_bool()).unwrap_or(false),
                                });
                            }
                        }
                    }
                }
            }
            "result" => {
                // 执行结果
                if let Some(r) = json.get("result").and_then(|v| v.as_str()) {
                    events.push(ExecutionEvent::Result {
                        summary: r.to_string(),
                    });
                }
                // 耗时
                if let Some(dur) = json.get("duration_ms").and_then(|v| v.as_u64()) {
                    if dur > 0 {
                        self.metadata.duration_ms = dur;
                        events.push(ExecutionEvent::Duration { duration_ms: dur });
                    }
                }
                // 成本
                if let Some(cost) = json.get("total_cost_usd").and_then(|v| v.as_f64()) {
                    if cost > 0.0 {
                        events.push(ExecutionEvent::Cost { cost_usd: cost });
                    }
                }
                // Token 统计
                if let Some(usage) = json.get("usage") {
                    let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    if input > 0 || output > 0 {
                        events.push(ExecutionEvent::Tokens {
                            input,
                            output,
                            cache_read: usage.get("cache_read_input_tokens").or_else(|| usage.get("cacheReadInputTokens")).and_then(|v| v.as_u64()),
                            cache_write: usage.get("cache_creation_input_tokens").or_else(|| usage.get("cacheCreationInputTokens")).and_then(|v| v.as_u64()),
                        });
                    }
                }
                if json.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false) {
                    self.metadata.mark_failed();
                } else {
                    self.metadata.mark_success();
                }
                self.metadata.set_finished_at();
            }
            _ => {}
        }

        events
    }
}

impl EventExtractor for CodebuddyExtractor {
    fn executor_name(&self) -> &str { "codebuddy" }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() { return Vec::new(); }
        if trimmed.starts_with('{') {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return self.parse_json_line(&json);
            }
        }
        vec![ExecutionEvent::Info { message: trimmed.to_string() }]
    }

    fn extract_stderr(&mut self, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() { return None; }
        if trimmed.to_lowercase().contains("error") {
            Some(ExecutionEvent::Error { message: trimmed.to_string() })
        } else {
            Some(ExecutionEvent::Info { message: trimmed.to_string() })
        }
    }

    fn metadata(&self) -> &ExecutionMetadata { &self.metadata }
    fn metadata_mut(&mut self) -> &mut ExecutionMetadata { &mut self.metadata }
}

impl Default for CodebuddyExtractor {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_system_event() {
        let mut ext = CodebuddyExtractor::new();
        let events = ext.extract(r#"{"type":"system","subtype":"initialized","session_id":"ses_cb_1","model":"claude-sonnet-4"}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::SessionStart { .. })));
        assert_eq!(ext.metadata().model.as_deref(), Some("claude-sonnet-4"));
    }

    #[test]
    fn test_assistant_with_thinking_and_text() {
        let mut ext = CodebuddyExtractor::new();
        let json = r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"thinking","thinking":"Let me analyze..."},{"type":"text","text":"Here's the plan."}]}}"#;
        let events = ext.extract(json);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Thinking { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Assistant { content, .. } if content == "Here's the plan.")));
    }

    #[test]
    fn test_assistant_with_tool_use() {
        let mut ext = CodebuddyExtractor::new();
        let json = r#"{"type":"assistant","message":{"id":"msg_2","type":"message","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls -la"}}]}}"#;
        let events = ext.extract(json);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::ToolCall { name, .. } if name == "Bash")));
    }

    #[test]
    fn test_user_with_tool_result() {
        let mut ext = CodebuddyExtractor::new();
        let json = r#"{"type":"user","message":{"id":"msg_3","type":"message","role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"file.txt","is_error":false}]}}"#;
        let events = ext.extract(json);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::ToolResult { output, .. } if output == "file.txt")));
    }

    #[test]
    fn test_result() {
        let mut ext = CodebuddyExtractor::new();
        let json = r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":2500,"result":"Task done.","total_cost_usd":0.02,"usage":{"input_tokens":800,"output_tokens":400,"cache_read_input_tokens":100,"cache_creation_input_tokens":50}}"#;
        let events = ext.extract(json);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Result { summary, .. } if summary == "Task done.")));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { input: 800, output: 400, .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Cost { cost_usd: 0.02 })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Duration { duration_ms: 2500 })));
    }

    #[test]
    fn test_empty_line() {
        let mut ext = CodebuddyExtractor::new();
        assert!(ext.extract("").is_empty());
    }
}
