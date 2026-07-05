//! Kimi 执行器的事件提取器实现
//!
//! Kimi 使用 JSON 格式输出：
//! - role: "assistant" -> 可能包含 tool_calls[] 和 content[]
//! - role: "tool" -> tool result
//! - content[]: text / think 类型

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Kimi 事件提取器
///
/// 解析 Kimi 的 JSON 格式输出。
#[derive(Debug, Clone)]
pub struct KimiExtractor {
    /// 元数据
    metadata: ExecutionMetadata,
}

impl KimiExtractor {
    /// 创建新的 Kimi 提取器
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("kimi".to_string()),
        }
    }

    /// 解析一行 JSON
    fn parse_json_line(&mut self, json: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        // 提取 role
        let role = json.get("role").and_then(|v| v.as_str()).unwrap_or("");

        match role {
            "assistant" => {
                // 检查是否有 tool_calls
                if let Some(calls) = json.get("tool_calls").and_then(|v| v.as_array()) {
                    for call in calls {
                        if let Some(func) = call.get("function") {
                            let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                            let args = func.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
                            events.push(ExecutionEvent::ToolCall {
                                id: call.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                name: name.to_string(),
                                input: serde_json::from_str(args).unwrap_or(serde_json::json!({})),
                            });
                        }
                    }
                }

                // 解析 content[]
                if let Some(content) = json.get("content") {
                    // 字符串格式：直接作为助手消息
                    if let Some(s) = content.as_str() {
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            events.push(ExecutionEvent::Assistant {
                                content: trimmed.to_string(),
                                thinking: None,
                                message_id: None,
                            });
                        }
                    // 数组格式：遍历 text/think
                    } else if let Some(items) = content.as_array() {
                        let mut texts: Vec<String> = Vec::new();
                        let mut thinking: Option<String> = None;

                        for item in items {
                            match item.get("type").and_then(|v| v.as_str()) {
                                Some("text") => {
                                    if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                                        texts.push(text.to_string());
                                    }
                                }
                                Some("think") => {
                                    if let Some(text) = item.get("think").and_then(|v| v.as_str()) {
                                        thinking = Some(text.to_string());
                                    }
                                }
                                _ => {}
                            }
                        }

                        // 发送思考事件
                        if let Some(ref t) = thinking {
                            events.push(ExecutionEvent::Thinking {
                                content: t.clone(),
                            });
                        }

                        // 发送助手消息
                        if !texts.is_empty() {
                            events.push(ExecutionEvent::Assistant {
                                content: texts.join("\n"),
                                thinking,
                                message_id: None,
                            });
                        }
                    }
                }
            }
            "tool" => {
                // Tool result
                if let Some(content) = json.get("content").and_then(|v| v.as_array()) {
                    for item in content {
                        if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                                events.push(ExecutionEvent::ToolResult {
                                    call_id: json.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                    output: text.to_string(),
                                    is_error: false,
                                });
                            }
                        }
                    }
                }
            }
            "system" => {
                events.push(ExecutionEvent::System {
                    message: json.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                });
            }
            _ => {}
        }

        // 提取 session_id
        if let Some(sid) = json.get("session_id").and_then(|v| v.as_str()) {
            if self.metadata.session_id.is_none() {
                self.metadata.session_id = Some(sid.to_string());
                events.insert(0, ExecutionEvent::SessionStart {
                    session_id: sid.to_string(),
                });
            }
        }

        // 提取 usage
        if let Some(usage) = json.get("usage") {
            let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            if input > 0 || output > 0 {
                events.push(ExecutionEvent::Tokens {
                    input,
                    output,
                    cache_read: usage.get("cache_read").and_then(|v| v.as_u64()),
                    cache_write: usage.get("cache_write").and_then(|v| v.as_u64()),
                });
            }
        }

        // 提取 cost
        if let Some(cost) = json.get("cost").and_then(|v| v.as_f64()) {
            if cost > 0.0 {
                events.push(ExecutionEvent::Cost { cost_usd: cost });
            }
        }

        events
    }
}

impl EventExtractor for KimiExtractor {
    fn executor_name(&self) -> &str {
        "kimi"
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

        if trimmed.to_lowercase().contains("error") {
            Some(ExecutionEvent::Error {
                message: trimmed.to_string(),
            })
        } else {
            Some(ExecutionEvent::Info {
                message: trimmed.to_string(),
            })
        }
    }

    fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
        &mut self.metadata
    }
}

impl Default for KimiExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_assistant_with_tool_call() {
        let mut extractor = KimiExtractor::new();
        let json = r#"{"role":"assistant","tool_calls":[{"id":"call_1","function":{"name":"bash","arguments":"{\"command\":\"ls\"}"}}],"content":[{"type":"text","text":"Running command..."}]}"#;
        let events = extractor.extract(json);

        assert!(events.len() >= 2);
        assert!(matches!(&events[0], ExecutionEvent::ToolCall { name, .. } if name == "bash"));
        assert!(matches!(&events[1], ExecutionEvent::Assistant { .. }));
    }

    #[test]
    fn test_assistant_with_thinking() {
        let mut extractor = KimiExtractor::new();
        let json = r#"{"role":"assistant","content":[{"type":"think","think":"Let me think about this..."},{"type":"text","text":"Here is my answer."}]}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ExecutionEvent::Thinking { .. }));
        assert!(matches!(&events[1], ExecutionEvent::Assistant { content, thinking: Some(_), .. } if content == "Here is my answer."));
    }

    #[test]
    fn test_tool_result() {
        let mut extractor = KimiExtractor::new();
        let json = r#"{"role":"tool","tool_call_id":"call_1","content":[{"type":"text","text":"file1.txt\nfile2.txt"}]}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ToolResult { call_id, .. } if call_id == "call_1"));
    }

    #[test]
    fn test_usage() {
        let mut extractor = KimiExtractor::new();
        let json = r#"{"role":"assistant","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":100,"output_tokens":50}}"#;
        let events = extractor.extract(json);

        assert!(events.len() >= 2); // Assistant + Tokens
        let tokens = events.iter().find(|e| matches!(e, ExecutionEvent::Tokens { .. }));
        assert!(tokens.is_some());
    }

    #[test]
    fn test_empty_line() {
        let mut extractor = KimiExtractor::new();
        assert!(extractor.extract("").is_empty());
    }
}
