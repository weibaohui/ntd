//! Kilo 执行器的事件提取器实现
//!
//! Kilo 复用了 OpenCode 的事件格式（hyphenated event types，例如 step-start、tool-use），
//! 并额外使用 camelCase 字段名（如 sessionID）。

use crate::adapters::kilo_event::KiloAgentEvent;
use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Kilo 事件提取器
///
/// 解析 Kilo/OpenCode 风格的 JSON 事件输出。
#[derive(Debug, Clone)]
pub struct KiloExtractor {
    /// 元数据
    metadata: ExecutionMetadata,
    /// 步骤计数器
    step_index: u32,
}

impl KiloExtractor {
    /// 创建新的 Kilo 提取器
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("kilo".to_string()),
            step_index: 0,
        }
    }

    /// 从 KiloAgentEvent 提取事件
    fn extract_from_event(&mut self, event: &KiloAgentEvent) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        // 提取 session_id（如果首次出现则生成 SessionStart）
        if let Some(session_id) = &event.session_id {
            if self.metadata.session_id.is_none() {
                self.metadata.session_id = Some(session_id.clone());
                events.push(ExecutionEvent::SessionStart {
                    session_id: session_id.clone(),
                });
            }
        }

        // 根据事件类型处理
        let event_type = &event.event_type;
        let part = event.part.as_ref();

        match event_type.as_str() {
            "step-start" | "step_start" => {
                let idx = self.step_index;
                self.step_index += 1;
                events.push(ExecutionEvent::StepStart {
                    name: format!("step_{}", idx),
                    index: idx,
                });
            }
            "step-finish" | "step_finish" => {
                let idx = self.step_index.saturating_sub(1);
                events.push(ExecutionEvent::StepFinish {
                    name: format!("step_{}", idx),
                    index: idx,
                });

                // 提取 tokens 和 cost
                if let Some(part) = part {
                    if let Some(tokens) = &part.tokens {
                        events.push(ExecutionEvent::Tokens {
                            input: tokens.input,
                            output: tokens.output,
                            cache_read: Some(tokens.cache.read),
                            cache_write: Some(tokens.cache.write),
                        });
                    }
                    if let Some(cost) = part.cost {
                        events.push(ExecutionEvent::Cost { cost_usd: cost });
                    }
                }
            }
            "text" | "agent" => {
                if let Some(part) = part {
                    // reasoning / thinking
                    if let Some(reason) = &part.reason {
                        if !reason.is_empty() {
                            events.push(ExecutionEvent::Thinking {
                                content: reason.clone(),
                            });
                        }
                    }

                    // 文本消息
                    if let Some(text) = &part.text {
                        if !text.is_empty() {
                            events.push(ExecutionEvent::Assistant {
                                content: text.clone(),
                                thinking: None,
                                message_id: part.message_id.clone(),
                            });
                        }
                    }

                    // 工具调用（如果有 tool 字段）
                    if let Some(tool_name) = &part.tool {
                        let call_id = part.call_id.clone().unwrap_or_default();
                        let input_json = part
                            .state
                            .as_ref()
                            .and_then(|s| s.input.as_ref())
                            .map(|i| i.to_full_json_value())
                            .unwrap_or(serde_json::json!({}));

                        events.push(ExecutionEvent::ToolCall {
                            id: call_id,
                            name: tool_name.clone(),
                            input: input_json,
                        });
                    }
                }
            }
            "tool-use" | "tool_use" => {
                if let Some(part) = part {
                    if let Some(tool_name) = &part.tool {
                        let call_id = part.call_id.clone().unwrap_or_default();
                        let input_json = part
                            .state
                            .as_ref()
                            .and_then(|s| s.input.as_ref())
                            .map(|i| i.to_full_json_value())
                            .unwrap_or(serde_json::json!({}));

                        events.push(ExecutionEvent::ToolCall {
                            id: call_id,
                            name: tool_name.clone(),
                            input: input_json,
                        });
                    }
                }
            }
            "tool-result" | "tool_result" => {
                if let Some(part) = part {
                    let call_id = part.call_id.clone().unwrap_or_default();
                    let output = part
                        .state
                        .as_ref()
                        .and_then(|s| s.output.clone())
                        .unwrap_or_default();

                    events.push(ExecutionEvent::ToolResult {
                        call_id,
                        output,
                        is_error: false,
                    });
                }
            }
            "result" | "finish" => {
                if let Some(part) = part {
                    // 最终结果文本
                    if let Some(text) = &part.text {
                        events.push(ExecutionEvent::Result {
                            summary: text.clone(),
                        });
                    }

                    // tokens
                    if let Some(tokens) = &part.tokens {
                        events.push(ExecutionEvent::Tokens {
                            input: tokens.input,
                            output: tokens.output,
                            cache_read: Some(tokens.cache.read),
                            cache_write: Some(tokens.cache.write),
                        });
                    }

                    if let Some(cost) = part.cost {
                        events.push(ExecutionEvent::Cost { cost_usd: cost });
                    }
                }
            }
            _ => {
                // 未知类型，作为 info 处理
                events.push(ExecutionEvent::Info {
                    message: format!("[{}]", event_type),
                });
            }
        }

        events
    }
}

impl EventExtractor for KiloExtractor {
    fn executor_name(&self) -> &str {
        "kilo"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // 尝试解析为 JSON
        if trimmed.starts_with('{') {
            match serde_json::from_str::<KiloAgentEvent>(trimmed) {
                Ok(event) => self.extract_from_event(&event),
                Err(_) => {
                    // JSON 解析失败，作为普通 info
                    vec![ExecutionEvent::Info {
                        message: trimmed.to_string(),
                    }]
                }
            }
        } else {
            // 非 JSON 行，作为 info
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

impl Default for KiloExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// KiloAgentToolInput 的扩展方法，用于转换为完整的 JSON 值
trait KiloToolInputExt {
    fn to_full_json_value(&self) -> serde_json::Value;
}

impl KiloToolInputExt for crate::adapters::kilo_event::KiloAgentToolInput {
    fn to_full_json_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(ref cmd) = self.command {
            map.insert("command".into(), serde_json::Value::String(cmd.clone()));
        }
        if let Some(ref desc) = self.description {
            map.insert("description".into(), serde_json::Value::String(desc.clone()));
        }
        for (k, v) in &self.extra {
            map.insert(k.clone(), v.clone());
        }
        serde_json::Value::Object(map)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_step_start() {
        let mut extractor = KiloExtractor::new();
        let json = r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_abc123"}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 2); // SessionStart + StepStart
        assert!(matches!(&events[0], ExecutionEvent::SessionStart { .. }));
        assert!(matches!(&events[1], ExecutionEvent::StepStart { .. }));
    }

    #[test]
    fn test_text_message() {
        let mut extractor = KiloExtractor::new();
        let json = r#"{"type":"text","timestamp":1700000000001,"sessionID":"ses_xyz","part":{"type":"text","text":"hello kilo"}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 2); // SessionStart + Assistant
        assert!(matches!(&events[1], ExecutionEvent::Assistant { content, .. } if content == "hello kilo"));
    }

    #[test]
    fn test_tool_use() {
        let mut extractor = KiloExtractor::new();
        let json = r#"{"type":"tool-use","timestamp":1700000000003,"part":{"type":"tool_use","tool":"bash","state":{"status":"running","input":{"description":"list files","command":"ls -la"},"output":null}}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ToolCall { name, .. } if name == "bash"));
    }

    #[test]
    fn test_step_finish_with_tokens() {
        let mut extractor = KiloExtractor::new();
        let json = r#"{"type":"step-finish","timestamp":1700000000002,"part":{"type":"step-finish","reason":"stop","tokens":{"total":200,"input":150,"output":50,"reasoning":0,"cache":{"read":10,"write":5}},"cost":0.0025}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 3); // StepFinish + Tokens + Cost
        assert!(matches!(&events[0], ExecutionEvent::StepFinish { .. }));
        assert!(matches!(&events[1], ExecutionEvent::Tokens { .. }));
        assert!(matches!(&events[2], ExecutionEvent::Cost { .. }));
    }

    #[test]
    fn test_empty_line() {
        let mut extractor = KiloExtractor::new();
        assert!(extractor.extract("").is_empty());
        assert!(extractor.extract("   ").is_empty());
    }

    #[test]
    fn test_non_json_line() {
        let mut extractor = KiloExtractor::new();
        let events = extractor.extract("plain text output");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Info { .. }));
    }

    #[test]
    fn test_step_index_increments() {
        let mut extractor = KiloExtractor::new();
        let json1 = r#"{"type":"step-start"}"#;
        let json2 = r#"{"type":"step-start"}"#;

        let events1 = extractor.extract(json1);
        let events2 = extractor.extract(json2);

        assert_eq!(extractor.step_index, 2);
        assert!(matches!(&events1[0], ExecutionEvent::StepStart { index: 0, .. }));
        assert!(matches!(&events2[0], ExecutionEvent::StepStart { index: 1, .. }));
    }
}
