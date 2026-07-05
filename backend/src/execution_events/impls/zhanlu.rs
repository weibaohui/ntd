//! Zhanlu 执行器的事件提取器实现
//!
//! Zhanlu 输出 JSONL 格式，使用连字符分隔的事件类型名（与 Opencode 完全一致）：
//! - step-start: 步骤开始
//! - text: 文本回复
//! - reasoning: 思考过程
//! - tool-use: 工具调用（含 state.status / input / output）
//! - step-finish: 步骤完成（含 tokens / cost）

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Zhanlu 事件提取器
#[derive(Debug, Clone)]
pub struct ZhanluExtractor {
    metadata: ExecutionMetadata,
    /// 步骤计数器
    step_index: u32,
}

impl ZhanluExtractor {
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("zhanlu".to_string()),
            step_index: 0,
        }
    }

    /// 解析 JSON 事件行
    fn parse_json_line(&mut self, json: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

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
            "step-start" | "step_start" => {
                let idx = self.step_index;
                self.step_index += 1;
                events.push(ExecutionEvent::StepStart {
                    name: "step".to_string(),
                    index: idx,
                });
            }
            "text" => {
                if let Some(part) = json.get("part") {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            events.push(ExecutionEvent::Assistant {
                                content: trimmed.to_string(),
                                thinking: None,
                                message_id: None,
                            });
                        }
                    }
                }
            }
            "reasoning" => {
                if let Some(part) = json.get("part") {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            events.push(ExecutionEvent::Thinking {
                                content: truncated(trimmed, 500),
                            });
                        }
                    }
                }
            }
            "tool-use" | "tool_use" => {
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

                    // 工具结果（如果有 output）
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
            "step-finish" | "step_finish" => {
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

                let idx = self.step_index.saturating_sub(1);
                events.push(ExecutionEvent::StepFinish {
                    name: "step".to_string(),
                    index: idx,
                });
            }
            _ => {
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

impl EventExtractor for ZhanluExtractor {
    fn executor_name(&self) -> &str { "zhanlu" }

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

impl Default for ZhanluExtractor {
    fn default() -> Self { Self::new() }
}

/// 截断字符串到指定长度
fn truncated(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_step_start_hyphenated() {
        let mut ext = ZhanluExtractor::new();
        let events = ext.extract(r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_zl1"}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::SessionStart { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::StepStart { .. })));
    }

    #[test]
    fn test_step_start_underscore() {
        let mut ext = ZhanluExtractor::new();
        let events = ext.extract(r#"{"type":"step_start","timestamp":1700000000000,"sessionID":"ses_zl2"}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::StepStart { .. })));
    }

    #[test]
    fn test_text() {
        let mut ext = ZhanluExtractor::new();
        let events = ext.extract(r#"{"type":"text","part":{"type":"text","text":"Hello, this is a test"}}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Assistant { content, .. } if content == "Hello, this is a test")));
    }

    #[test]
    fn test_reasoning() {
        let mut ext = ZhanluExtractor::new();
        let events = ext.extract(r#"{"type":"reasoning","part":{"type":"reasoning","text":"Let me analyze..."}}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Thinking { .. })));
    }

    #[test]
    fn test_tool_use() {
        let mut ext = ZhanluExtractor::new();
        let events = ext.extract(r#"{"type":"tool-use","part":{"type":"tool_use","tool":"bash","state":{"status":"running","input":{"command":"ls"}}}}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::ToolCall { name, .. } if name == "bash")));
    }

    #[test]
    fn test_step_finish() {
        let mut ext = ZhanluExtractor::new();
        let events = ext.extract(r#"{"type":"step-finish","part":{"type":"step-finish","reason":"stop","tokens":{"total":100,"input":50,"output":50,"cache":{"read":10,"write":5}},"cost":0.001}}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { input: 50, output: 50, .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Cost { cost_usd: 0.001 })));
    }

    #[test]
    fn test_empty_line() {
        let mut ext = ZhanluExtractor::new();
        assert!(ext.extract("").is_empty());
    }
}
