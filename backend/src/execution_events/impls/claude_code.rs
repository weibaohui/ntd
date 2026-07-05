//! Claude Code 执行器的事件提取器实现
//!
//! 解析 Claude Code 风格的 JSON lines 事件输出。

use crate::adapters::claude_protocol::{ClaudeMessage, ClaudeContentBlock};
use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Claude Code 事件提取器
///
/// 解析 Claude Code 的 JSON lines 格式输出。
#[derive(Debug, Clone)]
pub struct ClaudeCodeExtractor {
    /// 元数据
    metadata: ExecutionMetadata,
}

impl ClaudeCodeExtractor {
    /// 创建新的 Claude Code 提取器
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("claude_code".to_string()),
        }
    }

    /// 从 ClaudeMessage 提取事件
    fn extract_from_message(&mut self, msg: &ClaudeMessage) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        match msg {
            ClaudeMessage::System { subtype, session_id, model } => {
                // 提取 session_id
                if let Some(sid) = session_id {
                    if self.metadata.session_id.is_none() {
                        self.metadata.session_id = Some(sid.clone());
                        events.push(ExecutionEvent::SessionStart {
                            session_id: sid.clone(),
                        });
                    }
                }

                // 提取 model
                if let Some(m) = model {
                    if self.metadata.model.is_none() {
                        self.metadata.model = Some(m.clone());
                        events.push(ExecutionEvent::ModelSwitch {
                            model: m.clone(),
                        });
                    }
                }

                // System 消息
                if let Some(sub) = subtype {
                    events.push(ExecutionEvent::System {
                        message: format!("[{}]", sub),
                    });
                }
            }
            ClaudeMessage::Assistant { message, session_id, uuid, .. } => {
                // 提取 session_id
                if let Some(sid) = session_id {
                    if self.metadata.session_id.is_none() {
                        self.metadata.session_id = Some(sid.clone());
                        events.push(ExecutionEvent::SessionStart {
                            session_id: sid.clone(),
                        });
                    }
                }

                let mut texts: Vec<String> = Vec::new();
                let mut thinking_texts: Vec<String> = Vec::new();

                for block in &message.content {
                    match block {
                        ClaudeContentBlock::Thinking { thinking } => {
                            if let Some(text) = thinking {
                                if !text.is_empty() {
                                    thinking_texts.push(text.clone());
                                }
                            }
                        }
                        ClaudeContentBlock::Text { text } => {
                            if let Some(t) = text {
                                if !t.is_empty() {
                                    texts.push(t.clone());
                                }
                            }
                        }
                        ClaudeContentBlock::ToolUse { id, name, input } => {
                            events.push(ExecutionEvent::ToolCall {
                                id: id.clone().unwrap_or_default(),
                                name: name.clone().unwrap_or_default(),
                                input: input.clone(),
                            });
                        }
                        ClaudeContentBlock::ToolResult { tool_use_id, content, is_error } => {
                            events.push(ExecutionEvent::ToolResult {
                                call_id: tool_use_id.clone().unwrap_or_default(),
                                output: content.clone().unwrap_or_default(),
                                is_error: is_error.unwrap_or(false),
                            });
                        }
                        ClaudeContentBlock::Redacted { .. } => {
                            // 跳过已编辑的内容
                        }
                    }
                }

                // 思考事件
                if !thinking_texts.is_empty() {
                    events.push(ExecutionEvent::Thinking {
                        content: thinking_texts.join("\n"),
                    });
                }

                // 助手消息事件
                if !texts.is_empty() {
                    events.push(ExecutionEvent::Assistant {
                        content: texts.join("\n"),
                        thinking: if thinking_texts.is_empty() {
                            None
                        } else {
                            Some(thinking_texts.join("\n"))
                        },
                        message_id: uuid.clone(),
                    });
                }
            }
            ClaudeMessage::User { message, session_id, .. } => {
                // 提取 session_id
                if let Some(sid) = session_id {
                    if self.metadata.session_id.is_none() {
                        self.metadata.session_id = Some(sid.clone());
                        events.push(ExecutionEvent::SessionStart {
                            session_id: sid.clone(),
                        });
                    }
                }

                let texts: Vec<String> = message
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ClaudeContentBlock::Text { text } => text.clone(),
                        _ => None,
                    })
                    .filter(|t| !t.is_empty())
                    .collect();

                if !texts.is_empty() {
                    events.push(ExecutionEvent::User {
                        content: texts.join("\n"),
                    });
                }
            }
            ClaudeMessage::Result {
                subtype,
                is_error,
                duration_ms,
                result,
                total_cost_usd,
                usage,
                session_id,
            } => {
                // 提取 session_id
                if let Some(sid) = session_id {
                    if self.metadata.session_id.is_none() {
                        self.metadata.session_id = Some(sid.clone());
                        events.push(ExecutionEvent::SessionStart {
                            session_id: sid.clone(),
                        });
                    }
                }

                // usage
                if let Some(u) = usage {
                    events.push(ExecutionEvent::Tokens {
                        input: u.input_tokens,
                        output: u.output_tokens,
                        cache_read: u.cache_read_input_tokens,
                        cache_write: u.cache_creation_input_tokens,
                    });
                }

                // cost
                if let Some(cost) = total_cost_usd {
                    events.push(ExecutionEvent::Cost { cost_usd: *cost });
                }

                // duration
                if let Some(dur) = duration_ms {
                    events.push(ExecutionEvent::Duration { duration_ms: *dur });
                }

                // result
                if let Some(res) = result {
                    if !res.is_empty() {
                        events.push(ExecutionEvent::Result {
                            summary: res.clone(),
                        });
                    }
                }

                // 错误结果
                if *is_error {
                    if let Some(sub) = subtype {
                        events.push(ExecutionEvent::Error {
                            message: format!("[{}] 执行失败", sub),
                        });
                    }
                }
            }
        }

        events
    }
}

impl EventExtractor for ClaudeCodeExtractor {
    fn executor_name(&self) -> &str {
        "claude_code"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // 尝试解析为 JSON
        if trimmed.starts_with('{') {
            match serde_json::from_str::<ClaudeMessage>(trimmed) {
                Ok(msg) => self.extract_from_message(&msg),
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

impl Default for ClaudeCodeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_assistant_message_with_text() {
        let mut extractor = ClaudeCodeExtractor::new();
        let json = r#"{"type":"assistant","message":{"id":"msg_123","type":"message","role":"assistant","content":[{"type":"text","text":"Hello, I can help you with that."}]}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Assistant { content, .. } if content == "Hello, I can help you with that."));
    }

    #[test]
    fn test_assistant_with_thinking() {
        let mut extractor = ClaudeCodeExtractor::new();
        let json = r#"{"type":"assistant","message":{"id":"msg_456","type":"message","role":"assistant","content":[{"type":"thinking","thinking":"Let me think about this problem..."},{"type":"text","text":"Here is the solution."}]}}"#;
        let events = extractor.extract(json);

        // 应该有 Thinking + Assistant 两个事件
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ExecutionEvent::Thinking { .. }));
        assert!(matches!(&events[1], ExecutionEvent::Assistant { content, thinking: Some(_), .. } if content == "Here is the solution."));
    }

    #[test]
    fn test_tool_use() {
        let mut extractor = ClaudeCodeExtractor::new();
        let json = r#"{"type":"assistant","message":{"id":"msg_789","type":"message","role":"assistant","content":[{"type":"tool_use","id":"toolu_123","name":"Bash","input":{"command":"ls -la"}}]}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ToolCall { name, .. } if name == "Bash"));
    }

    #[test]
    fn test_result_with_usage() {
        let mut extractor = ClaudeCodeExtractor::new();
        let json = r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":1500,"result":"Task completed successfully.","total_cost_usd":0.015,"usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 4); // Tokens + Cost + Duration + Result
        assert!(matches!(&events[0], ExecutionEvent::Tokens { .. }));
        assert!(matches!(&events[1], ExecutionEvent::Cost { .. }));
        assert!(matches!(&events[2], ExecutionEvent::Duration { .. }));
        assert!(matches!(&events[3], ExecutionEvent::Result { .. }));
    }

    #[test]
    fn test_empty_line() {
        let mut extractor = ClaudeCodeExtractor::new();
        assert!(extractor.extract("").is_empty());
        assert!(extractor.extract("   ").is_empty());
    }

    #[test]
    fn test_session_id_from_system() {
        let mut extractor = ClaudeCodeExtractor::new();
        let json = r#"{"type":"system","subtype":"initialized","session_id":"ses_abc123","model":"claude-sonnet-4-20250514"}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 3); // SessionStart + ModelSwitch + System
        assert!(matches!(&events[0], ExecutionEvent::SessionStart { session_id } if session_id == "ses_abc123"));
        assert!(matches!(&events[1], ExecutionEvent::ModelSwitch { model } if model == "claude-sonnet-4-20250514"));
    }
}
