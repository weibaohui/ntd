//! MobileCoder 执行器的事件提取器实现
//!
//! MobileCoder 输出 JSONL 格式，使用下划线分隔的事件类型名：
//! - step_start: 步骤开始
//! - tool_use: 工具调用（含 state.status / input / output）
//! - text: 文本回复
//! - step_finish: 步骤完成（含 tokens / cost）

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// MobileCoder 事件提取器
#[derive(Debug, Clone)]
pub struct MobilecoderExtractor {
    metadata: ExecutionMetadata,
}

impl MobilecoderExtractor {
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("mobilecoder".to_string()),
        }
    }

    /// 解析一行 JSON 事件
    fn parse_json_line(&mut self, json: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        // 顶层事件类型
        let event_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

        // 提取 sessionID
        if let Some(sid) = json.get("sessionID").and_then(|v| v.as_str()) {
            if self.metadata.session_id.is_none() {
                self.metadata.session_id = Some(sid.to_string());
                events.push(ExecutionEvent::SessionStart {
                    session_id: sid.to_string(),
                });
            }
        }

        match event_type {
            "step_start" => {
                // 记录开始时间
                events.push(ExecutionEvent::StepStart {
                    name: "step".to_string(),
                    index: 0,
                });
            }
            "tool_use" => {
                // 工具调用：从 part 中提取工具名和参数
                if let Some(part) = json.get("part") {
                    let tool = part.get("tool").and_then(|v| v.as_str()).unwrap_or("bash");
                    let input = part.get("state")
                        .and_then(|s| s.get("input"))
                        .cloned()
                        .unwrap_or(serde_json::json!({}));

                    events.push(ExecutionEvent::ToolCall {
                        id: part.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        name: tool.to_string(),
                        input,
                    });

                    // 检查是否有输出（工具结果）
                    if let Some(output) = part.get("state")
                        .and_then(|s| s.get("output"))
                        .and_then(|v| v.as_str())
                    {
                        if !output.is_empty() {
                            events.push(ExecutionEvent::ToolResult {
                                call_id: String::new(),
                                output: output.to_string(),
                                is_error: part.get("state")
                                    .and_then(|s| s.get("status"))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s == "error" || s == "failed")
                                    .unwrap_or(false),
                            });
                        }
                    }
                }
            }
            "text" => {
                // 文本回复
                if let Some(part) = json.get("part") {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            events.push(ExecutionEvent::Assistant {
                                content: trimmed.to_string(),
                                thinking: None,
                                message_id: part.get("message_id").and_then(|v| v.as_str()).map(String::from),
                            });
                        }
                    }
                }
            }
            "step_finish" => {
                // 步骤完成：提取 tokens / cost
                if let Some(part) = json.get("part") {
                    // Token 统计
                    if let Some(tokens) = part.get("tokens") {
                        events.push(ExecutionEvent::Tokens {
                            input: tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0),
                            output: tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
                            cache_read: tokens.get("cache").and_then(|c| c.get("read")).and_then(|v| v.as_u64()),
                            cache_write: tokens.get("cache").and_then(|c| c.get("write")).and_then(|v| v.as_u64()),
                        });
                    }

                    // 成本
                    if let Some(cost) = part.get("cost").and_then(|v| v.as_f64()) {
                        if cost > 0.0 {
                            events.push(ExecutionEvent::Cost { cost_usd: cost });
                        }
                    }
                }

                events.push(ExecutionEvent::StepFinish {
                    name: "step".to_string(),
                    index: 0,
                });
            }
            _ => {
                // 未知类型作为 info
                if let Some(s) = json.as_str() {
                    events.push(ExecutionEvent::Info { message: s.to_string() });
                } else {
                    events.push(ExecutionEvent::Info {
                        message: serde_json::to_string(json).unwrap_or_default(),
                    });
                }
            }
        }

        events
    }
}

impl EventExtractor for MobilecoderExtractor {
    fn executor_name(&self) -> &str {
        "mobilecoder"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        if trimmed.starts_with('{') {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return self.parse_json_line(&json);
            }
        }

        vec![ExecutionEvent::Info {
            message: trimmed.to_string(),
        }]
    }

    fn extract_stderr(&mut self, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.to_lowercase().contains("error") {
            Some(ExecutionEvent::Error { message: trimmed.to_string() })
        } else {
            Some(ExecutionEvent::Info { message: trimmed.to_string() })
        }
    }

    fn metadata(&self) -> &ExecutionMetadata { &self.metadata }
    fn metadata_mut(&mut self) -> &mut ExecutionMetadata { &mut self.metadata }
}

impl Default for MobilecoderExtractor {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_step_start() {
        let mut ext = MobilecoderExtractor::new();
        let events = ext.extract(r#"{"type":"step_start","timestamp":1700000000000,"sessionID":"ses_m1"}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::SessionStart { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::StepStart { .. })));
        assert_eq!(ext.metadata().session_id.as_deref(), Some("ses_m1"));
    }

    #[test]
    fn test_tool_use_with_output() {
        let mut ext = MobilecoderExtractor::new();
        let line = r#"{"type":"tool_use","part":{"type":"tool_use","tool":"bash","state":{"status":"success","input":{"command":"ls"},"output":"file.txt"}}}"#;
        let events = ext.extract(line);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::ToolCall { name, .. } if name == "bash")));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::ToolResult { .. })));
    }

    #[test]
    fn test_text() {
        let mut ext = MobilecoderExtractor::new();
        let events = ext.extract(r#"{"type":"text","part":{"type":"text","text":"Hello world"}}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Assistant { content, .. } if content == "Hello world")));
    }

    #[test]
    fn test_step_finish_with_tokens() {
        let mut ext = MobilecoderExtractor::new();
        let line = r#"{"type":"step_finish","part":{"type":"step_finish","tokens":{"total":100,"input":50,"output":50,"cache":{"read":10,"write":5}},"cost":0.001}}"#;
        let events = ext.extract(line);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { input: 50, output: 50, .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Cost { cost_usd: 0.001 })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::StepFinish { .. })));
    }

    #[test]
    fn test_empty_line() {
        let mut ext = MobilecoderExtractor::new();
        assert!(ext.extract("").is_empty());
    }
}
