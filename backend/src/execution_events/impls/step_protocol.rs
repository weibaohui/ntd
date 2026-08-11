//! Step 协议族提取器的统一实现（096-W2 治理产物）。
//!
//! kilo / opencode 两家执行器输出完全相同的 JSONL 协议（step-start / tool-use / text /
//! step-finish ...），事件结构体在 093-B1 已收敛为 `StepAgentEvent`，但提取器实现仍各存
//! 一份逐字相同的代码（约 170 行 ×2）。本模块是唯一事实源；`kilo.rs` / `opencode.rs`
//! 降级为类型别名壳，保持既有引用路径不抖。
//!
//! 与原两份实现的有意差异（仅一处，093-B1 既定语义对齐）：
//! 工具输入序列化从「extra 允许覆盖 command/description」的旧 ext trait 语义，切换为
//! `StepAgentToolInput::to_full_json_value` 的防覆盖语义（extra 同名键跳过）。

use crate::adapters::step_event::StepAgentEvent;
use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Step 协议族事件提取器（kilo/opencode 共用）。
///
/// 唯一差异点是执行器名（写入 metadata.executor 并由 `executor_name()` 返回），
/// 构造时以参数注入。
#[derive(Debug, Clone)]
pub struct StepProtocolExtractor {
    /// 元数据
    metadata: ExecutionMetadata,
    /// 步骤计数器
    step_index: u32,
}

impl StepProtocolExtractor {
    /// 创建提取器；`executor_name` 是该实例的唯一身份差异（如 "kilo" / "opencode"）。
    pub fn new(executor_name: impl Into<String>) -> Self {
        Self {
            metadata: ExecutionMetadata::new(executor_name.into()),
            step_index: 0,
        }
    }

    /// 从 StepAgentEvent 提取事件（kilo/opencode 原 extract_from_event 的逐字合并版）。
    fn extract_from_event(&mut self, event: &StepAgentEvent) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        // session 首现认领（claim_session 幂等：仅首次产出 SessionStart，先到先赢）
        if let Some(sid) = &event.session_id {
            events.extend(self.metadata.claim_session(sid));
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
                        events.push(Self::tool_call_event(part, tool_name));
                    }
                }
            }
            "tool-use" | "tool_use" => {
                if let Some(part) = part {
                    if let Some(tool_name) = &part.tool {
                        events.push(Self::tool_call_event(part, tool_name));
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

    /// 从 part 构建 ToolCall 事件（text/agent 与 tool-use 两分支共用）。
    /// input 经 `to_full_json_value` 防覆盖序列化：extra 中的 command/description
    /// 同名键不覆盖结构化字段（093-B1 既定语义）。
    fn tool_call_event(
        part: &crate::adapters::step_event::StepAgentPart,
        tool_name: &str,
    ) -> ExecutionEvent {
        let call_id = part.call_id.clone().unwrap_or_default();
        let input_json = part
            .state
            .as_ref()
            .and_then(|s| s.input.as_ref())
            .map(|i| i.to_full_json_value())
            .unwrap_or(serde_json::json!({}));

        ExecutionEvent::ToolCall {
            id: call_id,
            name: tool_name.to_string(),
            input: input_json,
        }
    }
}

impl EventExtractor for StepProtocolExtractor {
    fn executor_name(&self) -> &str {
        &self.metadata.executor
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // 尝试解析为 JSON
        if trimmed.starts_with('{') {
            match serde_json::from_str::<StepAgentEvent>(trimmed) {
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

    fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
        &mut self.metadata
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    // 测试集由原 kilo.rs（7 项）与 opencode.rs（5 项）合并去重而来——
    // 两家测试样本本就同构，合并后作为协议行为锚点统一守护。

    #[test]
    fn test_step_start() {
        let mut extractor = StepProtocolExtractor::new("kilo");
        let json = r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_abc123"}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 2); // SessionStart + StepStart
        assert!(matches!(&events[0], ExecutionEvent::SessionStart { .. }));
        assert!(matches!(&events[1], ExecutionEvent::StepStart { .. }));
    }

    #[test]
    fn test_text_message() {
        let mut extractor = StepProtocolExtractor::new("kilo");
        let json = r#"{"type":"text","timestamp":1700000000001,"sessionID":"ses_xyz","part":{"type":"text","text":"hello kilo"}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 2); // SessionStart + Assistant
        assert!(matches!(&events[1], ExecutionEvent::Assistant { content, .. } if content == "hello kilo"));
    }

    #[test]
    fn test_tool_use() {
        let mut extractor = StepProtocolExtractor::new("kilo");
        let json = r#"{"type":"tool-use","timestamp":1700000000003,"part":{"type":"tool_use","tool":"bash","state":{"status":"running","input":{"description":"list files","command":"ls -la"},"output":null}}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ToolCall { name, input, .. } if name == "bash" && input["command"] == "ls -la"));
    }

    #[test]
    fn test_tool_result() {
        let mut extractor = StepProtocolExtractor::new("kilo");
        let json = r#"{"type":"tool-result","part":{"type":"tool_result","call_id":"c1","state":{"status":"success","output":"done"}}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ToolResult { call_id, output, is_error: false } if call_id == "c1" && output == "done"));
    }

    #[test]
    fn test_step_finish_with_tokens() {
        let mut extractor = StepProtocolExtractor::new("kilo");
        let json = r#"{"type":"step-finish","timestamp":1700000000002,"part":{"type":"step-finish","reason":"stop","tokens":{"total":200,"input":150,"output":50,"reasoning":0,"cache":{"read":10,"write":5}},"cost":0.0025}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 3); // StepFinish + Tokens + Cost
        assert!(matches!(&events[0], ExecutionEvent::StepFinish { .. }));
        assert!(matches!(&events[1], ExecutionEvent::Tokens { .. }));
        assert!(matches!(&events[2], ExecutionEvent::Cost { .. }));
    }

    #[test]
    fn test_result_finish() {
        let mut extractor = StepProtocolExtractor::new("kilo");
        let json = r#"{"type":"result","part":{"type":"result","text":"最终答案","tokens":{"total":10,"input":6,"output":4}}}"#;
        let events = extractor.extract(json);

        assert!(matches!(&events[0], ExecutionEvent::Result { summary } if summary == "最终答案"));
        assert!(matches!(&events[1], ExecutionEvent::Tokens { input: 6, output: 4, .. }));
    }

    #[test]
    fn test_empty_line() {
        let mut extractor = StepProtocolExtractor::new("kilo");
        assert!(extractor.extract("").is_empty());
        assert!(extractor.extract("   ").is_empty());
    }

    #[test]
    fn test_non_json_line() {
        let mut extractor = StepProtocolExtractor::new("kilo");
        let events = extractor.extract("plain text output");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Info { .. }));
    }

    #[test]
    fn test_step_index_increments() {
        let mut extractor = StepProtocolExtractor::new("kilo");
        let json1 = r#"{"type":"step-start"}"#;
        let json2 = r#"{"type":"step-start"}"#;

        let events1 = extractor.extract(json1);
        let events2 = extractor.extract(json2);

        assert_eq!(extractor.step_index, 2);
        assert!(matches!(&events1[0], ExecutionEvent::StepStart { index: 0, .. }));
        assert!(matches!(&events2[0], ExecutionEvent::StepStart { index: 1, .. }));
    }

    /// 执行器名是唯一身份差异：构造参数同时写入 metadata.executor 与 executor_name()
    #[test]
    fn test_executor_name_comes_from_constructor() {
        let kilo = StepProtocolExtractor::new("kilo");
        let opencode = StepProtocolExtractor::new("opencode");
        assert_eq!(kilo.executor_name(), "kilo");
        assert_eq!(opencode.executor_name(), "opencode");
        assert_eq!(kilo.metadata().executor, "kilo");
        assert_eq!(opencode.metadata().executor, "opencode");
    }

    /// 工具输入序列化的防覆盖语义（093-B1 既定对齐项）：
    /// extra 中的 command/description 同名键不得覆盖结构化字段
    #[test]
    fn test_tool_input_extra_does_not_override_core_keys() {
        let json = r#"{"type":"tool-use","part":{"type":"tool_use","tool":"bash","state":{"input":{"command":"ls","description":"x","extra_k":"v"}}}}"#;
        // 手工构造 extra 同名键的场景：直接走 to_full_json_value 路径（serde flatten 无法注入同名键）
        let mut extra = std::collections::HashMap::new();
        extra.insert("command".to_string(), serde_json::Value::String("evil".to_string()));
        let input = crate::adapters::step_event::StepAgentToolInput {
            command: Some("ls".to_string()),
            description: None,
            extra,
        };
        let v = input.to_full_json_value();
        assert_eq!(v["command"], "ls", "extra 中的 command 不得覆盖结构化字段");

        // 协议路径 smoke：工具事件正常产出
        let mut extractor = StepProtocolExtractor::new("opencode");
        let events = extractor.extract(json);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::ToolCall { .. })));
    }
}
