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

        // session 首现认领（claim_session 幂等：仅首次产出 SessionStart，先到先赢）
        if let Some(sid) = json.get("sessionID").and_then(|v| v.as_str()) {
            events.extend(self.metadata.claim_session(sid));
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
                    // zhanlu 不产出 message_id（None 为既有行为），共享 text 提取逻辑
                    events.extend(super::step_json::extract_assistant_text(part, None));
                }
            }
            "reasoning" => {
                if let Some(part) = json.get("part") {
                    events.extend(super::step_json::extract_thinking(part));
                }
            }
            "tool-use" | "tool_use" => {
                if let Some(part) = json.get("part") {
                    // id 取法是各家唯一差异（zhanlu 仅 part.id），工具对提取逻辑共享
                    let call_id = part
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    events.extend(super::step_json::extract_tool_pair(part, &call_id));
                }
            }
            "step-finish" | "step_finish" => {
                if let Some(part) = json.get("part") {
                    // tokens/cost 提取（三家逐字同构部分，已收敛）
                    events.extend(super::step_json::extract_tokens_cost(part));
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

    fn metadata(&self) -> &ExecutionMetadata { &self.metadata }
    fn metadata_mut(&mut self) -> &mut ExecutionMetadata { &mut self.metadata }
}

impl Default for ZhanluExtractor {
    fn default() -> Self { Self::new() }
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
