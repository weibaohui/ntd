//! Opencode 执行器的事件提取器实现
//!
//! Opencode 与 Kilo 使用完全相同的事件格式（hyphenated event types），
//! 因此直接复用 KiloExtractor 的逻辑。

use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;
use crate::execution_events::event::ExecutionEvent;

use crate::adapters::opencode_event::OpencodeAgentEvent;

/// Opencode 事件提取器
///
/// Opencode 与 Kilo 共享相同的 JSON 事件格式，
/// 因此实现逻辑与 KiloExtractor 基本一致。
#[derive(Debug, Clone)]
pub struct OpencodeExtractor {
    /// 元数据
    metadata: ExecutionMetadata,
    /// 步骤计数器
    step_index: u32,
}

impl OpencodeExtractor {
    /// 创建新的 Opencode 提取器
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("opencode".to_string()),
            step_index: 0,
        }
    }

    /// 从 OpencodeAgentEvent 提取事件
    fn extract_from_event(&mut self, event: &OpencodeAgentEvent) -> Vec<ExecutionEvent> {
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

impl EventExtractor for OpencodeExtractor {
    fn executor_name(&self) -> &str {
        "opencode"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // 尝试解析为 JSON
        if trimmed.starts_with('{') {
            match serde_json::from_str::<OpencodeAgentEvent>(trimmed) {
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

impl Default for OpencodeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// OpencodeAgentToolInput 的扩展方法，用于转换为完整的 JSON 值
trait OpencodeToolInputExt {
    fn to_full_json_value(&self) -> serde_json::Value;
}

impl OpencodeToolInputExt for crate::adapters::opencode_event::OpencodeAgentToolInput {
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
mod tests {
    use super::*;

    #[test]
    fn test_step_start() {
        let mut extractor = OpencodeExtractor::new();
        let json = r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_abc123"}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 2); // SessionStart + StepStart
        assert!(matches!(&events[0], ExecutionEvent::SessionStart { .. }));
        assert!(matches!(&events[1], ExecutionEvent::StepStart { .. }));
    }

    #[test]
    fn test_tool_use() {
        let mut extractor = OpencodeExtractor::new();
        let json = r#"{"type":"tool-use","timestamp":1700000000003,"part":{"type":"tool_use","tool":"bash","state":{"status":"running","input":{"description":"list files","command":"ls -la"},"output":null}}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ToolCall { name, .. } if name == "bash"));
    }

    #[test]
    fn test_empty_line() {
        let mut extractor = OpencodeExtractor::new();
        assert!(extractor.extract("").is_empty());
        assert!(extractor.extract("   ").is_empty());
    }

    #[test]
    fn test_non_json_line() {
        let mut extractor = OpencodeExtractor::new();
        let events = extractor.extract("plain text output");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Info { .. }));
    }

    #[test]
    fn test_step_index_increments() {
        let mut extractor = OpencodeExtractor::new();
        let json1 = r#"{"type":"step-start"}"#;
        let json2 = r#"{"type":"step-start"}"#;

        let events1 = extractor.extract(json1);
        let events2 = extractor.extract(json2);

        assert_eq!(extractor.step_index, 2);
        assert!(matches!(&events1[0], ExecutionEvent::StepStart { index: 0, .. }));
        assert!(matches!(&events2[0], ExecutionEvent::StepStart { index: 1, .. }));
    }
}
