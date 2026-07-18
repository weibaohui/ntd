//! Codex 执行器的事件提取器实现
//!
//! Codex 使用独特的 JSON 格式：
//! - item.started / item.completed
//! - turn.completed
//! - typed events: agent_message, agent_reasoning, tool_call, tool_result, error, etc.

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Codex 事件提取器
///
/// 解析 Codex 的 JSON 格式输出。
#[derive(Debug, Clone)]
pub struct CodexExtractor {
    /// 元数据
    metadata: ExecutionMetadata,
    /// 步骤计数器
    step_index: u32,
}

impl CodexExtractor {
    /// 创建新的 Codex 提取器
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("codex".to_string()),
            step_index: 0,
        }
    }

    /// 提取 (event_type, event_value) 二元组
    fn extract_event_type(json: &serde_json::Value) -> Option<(&str, &serde_json::Value)> {
        // 优先从顶层 type 字段提取
        if let Some(typ) = json.get("type").and_then(|v| v.as_str()) {
            return Some((typ, json));
        }
        // 回退到 msg.type
        if let Some(msg) = json.get("msg") {
            if let Some(typ) = msg.get("type").and_then(|v| v.as_str()) {
                return Some((typ, msg));
            }
        }
        None
    }

    /// 解析一行 JSON
    fn parse_json_line(&mut self, json: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        let Some((event_type, event_value)) = Self::extract_event_type(json) else {
            return vec![ExecutionEvent::Info {
                message: serde_json::to_string(json).unwrap_or_default(),
            }];
        };

        match event_type {
            // Item 事件
            "item.started" => {
                if let Some(item) = event_value.get("item") {
                    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if item_type == "command_execution" {
                        let idx = self.step_index;
                        self.step_index += 1;
                        events.push(ExecutionEvent::StepStart {
                            name: format!("command_{}", idx),
                            index: idx,
                        });
                    }
                }
            }
            "item.completed" => {
                if let Some(item) = event_value.get("item") {
                    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if item_type == "command_execution" {
                        let idx = self.step_index.saturating_sub(1);
                        events.push(ExecutionEvent::StepFinish {
                            name: format!("command_{}", idx),
                            index: idx,
                        });
                    } else if item_type == "agent_message" {
                        // 提取 agent_message 文本
                        if let Some(text) = item.get("message").and_then(|v| v.as_str()) {
                            events.push(ExecutionEvent::Result {
                                summary: text.to_string(),
                            });
                        }
                    } else if item_type == "collab_tool_call" {
                        // codex 派生子 agent 走 collab_tool_call(spawn_agent)；
                        // 归一成 ToolCall，让 agent_progress 能识别（item.completed 才带 prompt + receiver）。
                        if let Some(ev) = collab_spawn_agent_tool_call(item) {
                            events.push(ev);
                        }
                    }
                }
            }

            // Turn 事件
            "turn.completed" => {
                // 提取 usage
                if let Some(usage) = json.get("usage") {
                    let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    if input > 0 || output > 0 {
                        events.push(ExecutionEvent::Tokens {
                            input,
                            output,
                            cache_read: None,
                            cache_write: None,
                        });
                    }
                }

                let idx = self.step_index.saturating_sub(1);
                events.push(ExecutionEvent::StepFinish {
                    name: format!("turn_{}", idx),
                    index: idx,
                });
            }
            "turn.started" => {
                let idx = self.step_index;
                self.step_index += 1;
                events.push(ExecutionEvent::StepStart {
                    name: format!("turn_{}", idx),
                    index: idx,
                });
            }

            // Typed 事件
            "agent_message" | "agent_message_delta" | "assistant_message" => {
                // 提取文本
                let text = json
                    .get("message")
                    .or_else(|| json.get("delta"))
                    .or_else(|| json.get("text"))
                    .or_else(|| json.get("content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !text.is_empty() {
                    events.push(ExecutionEvent::Assistant {
                        content: text.to_string(),
                        thinking: None,
                        message_id: None,
                    });
                }
            }
            "agent_reasoning" | "agent_reasoning_delta" | "reasoning" | "reasoning_delta" => {
                let text = json
                    .get("message")
                    .or_else(|| json.get("delta"))
                    .or_else(|| json.get("text"))
                    .or_else(|| json.get("content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !text.is_empty() {
                    events.push(ExecutionEvent::Thinking {
                        content: text.to_string(),
                    });
                }
            }
            "exec_command_begin" | "tool_call_begin" | "tool_call" => {
                let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("bash");
                let input = json.get("arguments")
                    .or_else(|| json.get("input"))
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                events.push(ExecutionEvent::ToolCall {
                    id: json.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    name: name.to_string(),
                    input,
                });
            }
            "exec_command_end" | "tool_call_end" | "tool_result" => {
                let output = json.get("output")
                    .or_else(|| json.get("result"))
                    .map(|v| {
                        if let Some(s) = v.as_str() {
                            s.to_string()
                        } else {
                            serde_json::to_string(v).unwrap_or_default()
                        }
                    })
                    .unwrap_or_default();
                events.push(ExecutionEvent::ToolResult {
                    call_id: json.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    output,
                    is_error: false,
                });
            }
            "task_complete" => {
                events.push(ExecutionEvent::Result {
                    summary: json.get("result").and_then(|v| v.as_str()).unwrap_or("Task completed").to_string(),
                });
            }
            "error" => {
                let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                events.push(ExecutionEvent::Error {
                    message: msg.to_string(),
                });
            }
            "session_configured" | "task_started" => {
                // 提取 session_id
                if let Some(sid) = json.get("session_id").and_then(|v| v.as_str()) {
                    if self.metadata.session_id.is_none() {
                        self.metadata.session_id = Some(sid.to_string());
                        events.push(ExecutionEvent::SessionStart {
                            session_id: sid.to_string(),
                        });
                    }
                }

                // 提取 model
                if let Some(model) = json.get("model").or_else(|| json.get("model_slug")).and_then(|v| v.as_str()) {
                    if self.metadata.model.is_none() {
                        self.metadata.model = Some(model.to_string());
                        events.push(ExecutionEvent::ModelSwitch {
                            model: model.to_string(),
                        });
                    }
                }
            }
            _ => {
                // 未知类型，保留原始 JSON
                events.push(ExecutionEvent::Info {
                    message: format!("[{}]", event_type),
                });
            }
        }

        events
    }
}

/// codex 的 collab_tool_call(tool=spawn_agent) → ToolCall，让多 agent 提取器识别派生的子 agent。
///
/// 仅 item.completed 调用（此时才带 prompt 与 receiver_thread_ids）；tool 非 spawn_agent 返回 None。
fn collab_spawn_agent_tool_call(item: &serde_json::Value) -> Option<ExecutionEvent> {
    let tool = item.get("tool").and_then(|v| v.as_str()).unwrap_or("");
    if tool != "spawn_agent" {
        return None;
    }
    let input = serde_json::json!({
        "prompt": item.get("prompt").and_then(|v| v.as_str()).unwrap_or(""),
        "receiver_thread_ids": item.get("receiver_thread_ids").cloned().unwrap_or(serde_json::json!([])),
    });
    Some(ExecutionEvent::ToolCall {
        id: item.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        name: "spawn_agent".to_string(),
        input,
    })
}

impl EventExtractor for CodexExtractor {
    fn executor_name(&self) -> &str {
        "codex"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        if trimmed.starts_with('{') {
            match serde_json::from_str::<serde_json::Value>(trimmed) {
                Ok(json) => self.parse_json_line(&json),
                Err(_) => {
                    vec![ExecutionEvent::Info {
                        message: trimmed.to_string(),
                    }]
                }
            }
        } else {
            vec![ExecutionEvent::Info {
                message: trimmed.to_string(),
            }]
        }
    }

    fn extract_stderr(&mut self, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Codex 特殊处理：stderr 的 error 不一定是 error 类型，统一作为 Info 上报
        Some(ExecutionEvent::Info {
            message: trimmed.to_string(),
        })
    }

    fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
        &mut self.metadata
    }
}

impl Default for CodexExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call() {
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"tool_call","id":"call_1","name":"bash","arguments":"{\"command\":\"ls\"}"}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ToolCall { name, .. } if name == "bash"));
    }

    #[test]
    fn test_reasoning() {
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"agent_reasoning","content":"Thinking about the problem..."}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Thinking { .. }));
    }

    #[test]
    fn test_item_started() {
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"item.started","item":{"type":"command_execution","id":"cmd_1"}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::StepStart { index: 0, .. }));
    }

    #[test]
    fn test_usage() {
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"turn.completed","usage":{"input_tokens":200,"output_tokens":100}}"#;
        let events = extractor.extract(json);

        assert!(events.len() >= 2); // Tokens + StepFinish
        assert!(matches!(&events[0], ExecutionEvent::Tokens { input: 200, output: 100, .. }));
    }

    #[test]
    fn test_empty_line() {
        let mut extractor = CodexExtractor::new();
        assert!(extractor.extract("").is_empty());
    }
}
