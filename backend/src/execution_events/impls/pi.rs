//! Pi 执行器的事件提取器实现
//!
//! Pi 使用 JSONL 流格式输出，包含多种事件类型：
//! - message_update: 流式增量更新（thinking_delta / text_delta / toolcall_delta / thinking_end / text_end / toolcall_end）
//! - message_end: 完整消息（含 content[] + usage）
//! - turn_end: 完整回合（同 message_end + toolResults[]）
//! - session: 会话信息
//! - model_change: 模型切换
//! - message_start: 消息开始信号

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Pi 事件提取器
///
/// 解析 Pi 的 JSONL 流格式输出，提取结构化事件。
/// 格式示例见 `backend/src/adapters/pi_event.rs` 和数据库 execution_logs。
#[derive(Debug, Clone)]
pub struct PiExtractor {
    /// 元数据
    metadata: ExecutionMetadata,
    /// 用于累积追踪 tool_results 和 message_end 之间的对应关系
    pending_tool_calls: Vec<String>,
}

impl PiExtractor {
    /// 创建新的 Pi 提取器
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("pi".to_string()),
            pending_tool_calls: Vec::new(),
        }
    }

    /// 从 content[] 数组中提取 thinking / text / toolCall 事件
    fn extract_content_blocks(&mut self, content: &[serde_json::Value]) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        for block in content {
            // 每个 block 有 "type" 字段标识内容类型
            let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match block_type {
                "thinking" => {
                    // 思考块：thinking 字段包含思考内容
                    if let Some(thinking) = block.get("thinking").and_then(|v| v.as_str()) {
                        let trimmed = thinking.trim();
                        if !trimmed.is_empty() {
                            events.push(ExecutionEvent::Thinking {
                                content: trimmed.to_string(),
                            });
                        }
                    }
                }
                "text" => {
                    // 文本块：text 字段包含文本内容
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
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
                "toolCall" | "tool_call" => {
                    // 工具调用块：提取 id / name / arguments
                    let id = block.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                    let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("bash");
                    let input = block.get("arguments")
                        .or_else(|| block.get("input"))
                        .cloned()
                        .unwrap_or(serde_json::json!({}));

                    events.push(ExecutionEvent::ToolCall {
                        id: id.to_string(),
                        name: name.to_string(),
                        input,
                    });
                    // 记录 pending tool call id，供后续 tool_result 匹配
                    if !id.is_empty() {
                        self.pending_tool_calls.push(id.to_string());
                    }
                }
                _ => {
                    // 未知内容块类型，作为 info 保留
                    if let Some(text) = block.as_str() {
                        if !text.trim().is_empty() {
                            events.push(ExecutionEvent::Info {
                                message: text.to_string(),
                            });
                        }
                    }
                }
            }
        }

        events
    }

    /// 从工具结果数组中提取 ToolResult 事件
    fn extract_tool_results(&mut self, results: &[serde_json::Value]) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        for result in results {
            let tool_call_id = result.get("toolCallId")
                .or_else(|| result.get("tool_call_id"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            // 从 content[] 中提取文本输出
            let output = if let Some(content) = result.get("content").and_then(|v| v.as_array()) {
                content.iter()
                    .filter_map(|block| block.get("text").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                result.get("output").and_then(|v| v.as_str()).unwrap_or_default().to_string()
            };

            let is_error = result.get("isError")
                .or_else(|| result.get("is_error"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            events.push(ExecutionEvent::ToolResult {
                call_id: tool_call_id.to_string(),
                output,
                is_error,
            });
        }

        events
    }

    /// 从 message 对象中提取 usage 统计
    fn extract_usage_from_message(&mut self, msg: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        // 提取 usage 对象
        if let Some(usage) = msg.get("usage") {
            events.extend(self.extract_usage_event(usage));

            // 提取 cost
            if let Some(cost) = usage.get("cost") {
                if let Some(total) = cost.get("total").and_then(|v| v.as_f64()) {
                    if total > 0.0 {
                        events.push(ExecutionEvent::Cost { cost_usd: total });
                    }
                }
            }
        }

        events
    }

    /// 从 usage JSON 对象中提取 Tokens 事件
    fn extract_usage_event(&self, usage: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();
        let input = usage.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
        let output = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_read = usage.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_write = usage.get("cacheWrite").and_then(|v| v.as_u64()).unwrap_or(0);

        // input 和 output 均为 0 时跳过中间状态的占位数据，但保留 cache 数据
        if input > 0 || output > 0 || cache_read > 0 || cache_write > 0 {
            events.push(ExecutionEvent::Tokens {
                input,
                output,
                cache_read: usage.get("cacheRead").and_then(|v| v.as_u64()),
                cache_write: usage.get("cacheWrite").and_then(|v| v.as_u64()),
            });
        }
        events
    }

    /// 从 message 对象中提取 model 并更新 metadata
    fn extract_model_from_message(&mut self, msg: &serde_json::Value) {
        if self.metadata.model.is_none() {
            // 尝试从 message 顶层取 model
            if let Some(model) = msg.get("model").and_then(|v| v.as_str()) {
                if !model.is_empty() {
                    self.metadata.model = Some(model.to_string());
                }
            }
        }
    }

    /// 从 message 中提取 stopReason 并生成对应事件
    fn extract_stop_reason(msg: &serde_json::Value) -> Option<ExecutionEvent> {
        let stop_reason = msg.get("stopReason").and_then(|v| v.as_str())?;
        match stop_reason {
            "end_turn" | "stop" => Some(ExecutionEvent::Result {
                summary: "Task completed".to_string(),
            }),
            "toolUse" => None, // 工具调用不需要额外事件
            _ => Some(ExecutionEvent::Info {
                message: format!("Stopped: {}", stop_reason),
            }),
        }
    }

    /// 解析一行 JSON 事件
    fn parse_json_line(&mut self, json: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        // 提取顶层事件类型
        let event_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "session" => {
                // 会话事件：提取 session_id
                if let Some(sid) = json.get("id").and_then(|v| v.as_str()) {
                    if self.metadata.session_id.is_none() {
                        self.metadata.session_id = Some(sid.to_string());
                        events.push(ExecutionEvent::SessionStart {
                            session_id: sid.to_string(),
                        });
                    }
                }
            }
            "model_change" => {
                // 模型切换事件
                if let Some(model) = json.get("model").and_then(|v| v.as_str()) {
                    self.metadata.model = Some(model.to_string());
                    events.push(ExecutionEvent::ModelSwitch {
                        model: model.to_string(),
                    });
                }
            }
            "message_start" => {
                // 消息开始信号：重置 pending 状态
                self.pending_tool_calls.clear();
            }
            "agent_start" | "turn_start" => {
                // agent/turn 开始信号：无事件，仅状态标记
            }
            "message_update" => {
                // 增量消息更新：检查 assistantMessageEvent 的子类型
                if let Some(ame) = json.get("assistantMessageEvent") {
                    let sub_type = ame.get("type").and_then(|v| v.as_str()).unwrap_or("");

                    match sub_type {
                        "thinking_end" => {
                            // thinking 结束：从 content 或 delta 提取完整思考内容
                            if let Some(content) = ame.get("content").and_then(|v| v.as_str()) {
                                let trimmed = content.trim();
                                if !trimmed.is_empty() {
                                    events.push(ExecutionEvent::Thinking {
                                        content: trimmed.to_string(),
                                    });
                                }
                            }
                        }
                        "text_end" => {
                            // 文本结束：从 content 或 delta 提取完整文本
                            if let Some(content) = ame.get("content").and_then(|v| v.as_str()) {
                                let trimmed = content.trim();
                                if !trimmed.is_empty() {
                                    events.push(ExecutionEvent::Assistant {
                                        content: trimmed.to_string(),
                                        thinking: None,
                                        message_id: None,
                                    });
                                }
                            }

                            // text_end 可能携带 usage（message_end 未触发时兜底）
                            // 实际数据中 usage 在 partial 下，而非 assistantMessageEvent 顶层
                            if let Some(usage) = ame.get("usage")
                                .or_else(|| ame.get("partial").and_then(|p| p.get("usage")))
                            {
                                events.extend(self.extract_usage_event(usage));
                            }
                        }
                        "toolcall_end" => {
                            // 工具调用结束：从 toolCall 对象提取完整信息
                            if let Some(tc) = ame.get("toolCall") {
                                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                                let name = tc.get("name").and_then(|v| v.as_str()).unwrap_or("bash");
                                let input = tc.get("arguments")
                                    .or_else(|| tc.get("input"))
                                    .cloned()
                                    .unwrap_or(serde_json::json!({}));

                                events.push(ExecutionEvent::ToolCall {
                                    id: id.to_string(),
                                    name: name.to_string(),
                                    input,
                                });
                                if !id.is_empty() {
                                    self.pending_tool_calls.push(id.to_string());
                                }
                            }

                            // toolcall_end 可能携带 usage（在 partial 中）
                            if let Some(usage) = ame.get("partial").and_then(|p| p.get("usage")) {
                                events.extend(self.extract_usage_event(usage));
                            }
                        }
                        "toolcall_delta" | "toolcall_start" | "thinking_start" | "thinking_delta" | "text_delta" => {
                            // 增量或开始信号，还未完成，跳过
                        }
                        _ => {
                            // 未知子类型，作为 info 保留
                            if let Some(delta) = ame.get("delta").and_then(|v| v.as_str()) {
                                if !delta.trim().is_empty() {
                                    events.push(ExecutionEvent::Info {
                                        message: delta.to_string(),
                                    });
                                }
                            }
                        }
                    }

                    // 从 assistantMessageEvent 的 partial 中提取 model（兜底）
                    if self.metadata.model.is_none() {
                        if let Some(partial) = ame.get("partial") {
                            self.extract_model_from_message(partial);
                        }
                    }
                }

                // 从 message 字段提取 model（顶层 message 包含完整消息状态）
                if let Some(msg) = json.get("message") {
                    self.extract_model_from_message(msg);
                }
            }
            "message_end" => {
                // 完整消息结束：从 message 字段提取所有内容
                if let Some(msg) = json.get("message") {
                    self.extract_model_from_message(msg);

                    // 提取 content[] 中的事件
                    if let Some(content) = msg.get("content").and_then(|v| v.as_array()) {
                        events.extend(self.extract_content_blocks(content));
                    }

                    // 提取 usage
                    events.extend(self.extract_usage_from_message(msg));

                    // 提取 stopReason
                    if let Some(stop_event) = Self::extract_stop_reason(msg) {
                        events.push(stop_event);
                    }
                }
            }
            "turn_end" => {
                // 完整回合结束：同 message_end + toolResults
                if let Some(msg) = json.get("message") {
                    self.extract_model_from_message(msg);

                    // 提取 content[] 中的事件
                    if let Some(content) = msg.get("content").and_then(|v| v.as_array()) {
                        events.extend(self.extract_content_blocks(content));
                    }

                    // 提取 usage
                    events.extend(self.extract_usage_from_message(msg));

                    // 提取 stopReason
                    if let Some(stop_event) = Self::extract_stop_reason(msg) {
                        events.push(stop_event);
                    }
                }

                // 提取 toolResults
                if let Some(results) = json.get("toolResults").and_then(|v| v.as_array()) {
                    events.extend(self.extract_tool_results(results));
                }
            }
            "agent_end" => {
                // agent 结束事件：提取最终助手消息的内容和用量
                if let Some(messages) = json.get("messages").and_then(|v| v.as_array()) {
                    // 找到最后一条 assistant 消息
                    for msg in messages.iter().rev() {
                        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
                        if role != "assistant" {
                            continue;
                        }

                        // 提取最终结果文本
                        if let Some(content) = msg.get("content").and_then(|v| v.as_array()) {
                            for item in content {
                                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                                    let trimmed = text.trim();
                                    if !trimmed.is_empty() {
                                        events.push(ExecutionEvent::Result {
                                            summary: trimmed.to_string(),
                                        });
                                        break;
                                    }
                                }
                            }
                        }

                        // 提取最终 usage
                        events.extend(self.extract_usage_from_message(msg));
                        break;
                    }
                }
            }
            "error" => {
                // 错误事件
                let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                events.push(ExecutionEvent::Error {
                    message: msg.to_string(),
                });
            }
            _ => {
                // 未知事件类型，作为 info 保留
                if let Some(msg) = json.get("message").and_then(|v| v.as_str()) {
                    events.push(ExecutionEvent::Info {
                        message: msg.to_string(),
                    });
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

impl EventExtractor for PiExtractor {
    fn executor_name(&self) -> &str {
        "pi"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // 尝试解析 JSON
        if trimmed.starts_with('{') {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return self.parse_json_line(&json);
            }
        }

        // 非 JSON 行作为普通 info
        vec![ExecutionEvent::Info {
            message: trimmed.to_string(),
        }]
    }

    fn extract_stderr(&mut self, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // 检查是否包含 error 关键字
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

impl Default for PiExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：toolcall_end 事件提取工具调用
    #[test]
    fn test_toolcall_end() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"message_update","assistantMessageEvent":{"type":"toolcall_end","contentIndex":2,"toolCall":{"type":"toolCall","id":"call_123","name":"bash","arguments":{"command":"ls -la"}}}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::ToolCall { id, name, input } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "bash");
                assert_eq!(input.get("command").and_then(|v| v.as_str()), Some("ls -la"));
            }
            _ => panic!("Expected ToolCall event"),
        }
    }

    /// 测试：thinking_end 事件提取思考内容
    #[test]
    fn test_thinking_end() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"message_update","assistantMessageEvent":{"type":"thinking_end","contentIndex":0,"content":"Let me analyze this step by step."}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::Thinking { content } => {
                assert!(content.contains("Let me analyze this"));
            }
            _ => panic!("Expected Thinking event"),
        }
    }

    /// 测试：text_end 事件提取助手消息
    #[test]
    fn test_text_end() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_end","contentIndex":1,"content":"Here is the result."}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::Assistant { content, .. } => {
                assert_eq!(content, "Here is the result.");
            }
            _ => panic!("Expected Assistant event"),
        }
    }

    /// 测试：message_end 事件提取完整消息
    #[test]
    fn test_message_end() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"message_end","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Let me think..."},{"type":"text","text":"Here is the answer."}],"model":"claude-sonnet-4","usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"totalTokens":150,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0.003}},"stopReason":"end_turn"}}"#;
        let events = extractor.extract(json);

        // 应该包含 Thinking + Assistant + Tokens + Cost + Result
        assert!(events.len() >= 4);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Thinking { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Assistant { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Result { .. })));

        // 验证元数据
        assert_eq!(extractor.metadata().model.as_deref(), Some("claude-sonnet-4"));
    }

    /// 测试：turn_end 事件提取完整回合（含 tool results）
    #[test]
    fn test_turn_end_with_tool_results() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"turn_end","message":{"role":"assistant","content":[{"type":"thinking","thinking":"I need to run a command."},{"type":"toolCall","id":"call_456","name":"bash","arguments":{"command":"echo hello"}},{"type":"text","text":"Command executed."}],"usage":{"input":200,"output":100,"totalTokens":300,"cost":{"total":0.005}},"stopReason":"toolUse"},"toolResults":[{"role":"toolResult","toolCallId":"call_456","toolName":"bash","content":[{"type":"text","text":"hello\n"}],"isError":false}]}"#;
        let events = extractor.extract(json);

        // 应该包含 Thinking + ToolCall + Assistant + Tokens + Cost + ToolResult
        assert!(events.len() >= 5);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Thinking { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::ToolCall { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Assistant { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::ToolResult { .. })));
    }

    /// 测试：session 事件
    #[test]
    fn test_session_event() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"session","id":"ses_pi_789"}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::SessionStart { session_id } => {
                assert_eq!(session_id, "ses_pi_789");
            }
            _ => panic!("Expected SessionStart event"),
        }
        assert_eq!(extractor.metadata().session_id.as_deref(), Some("ses_pi_789"));
    }

    /// 测试：空行不产生事件
    #[test]
    fn test_empty_line() {
        let mut extractor = PiExtractor::new();
        assert!(extractor.extract("").is_empty());
        assert!(extractor.extract("   ").is_empty());
    }

    /// 测试：model_change 事件
    #[test]
    fn test_model_change() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"model_change","model":"gpt-4"}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::ModelSwitch { model } => {
                assert_eq!(model, "gpt-4");
            }
            _ => panic!("Expected ModelSwitch event"),
        }
        assert_eq!(extractor.metadata().model.as_deref(), Some("gpt-4"));
    }

    /// 测试：message_start 不产生事件
    #[test]
    fn test_message_start_no_events() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"message_start"}"#;
        let events = extractor.extract(json);
        assert!(events.is_empty());
    }

    /// 测试：toolcall_delta 增量跳过
    #[test]
    fn test_toolcall_delta_skipped() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"message_update","assistantMessageEvent":{"type":"toolcall_delta","delta":"{\"command\": \"echo"}}"#;
        let events = extractor.extract(json);
        assert!(events.is_empty());
    }

    /// 测试：非 JSON 行作为 info
    #[test]
    fn test_non_json_line() {
        let mut extractor = PiExtractor::new();
        let events = extractor.extract("Some plain text output");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Info { .. }));
    }

    /// 测试：message_update 中 text_end 携带 usage（兜底路径）
    #[test]
    fn test_text_end_with_usage() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_end","content":"Result.","usage":{"input":50,"output":30}}}"#;
        let events = extractor.extract(json);

        // Assistant + Tokens
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ExecutionEvent::Assistant { .. }));
        assert!(matches!(&events[1], ExecutionEvent::Tokens { input: 50, output: 30, .. }));
    }

    /// 测试：text_end 从 partial.usage 提取用量（实际 PI 数据路径）
    #[test]
    fn test_text_end_with_partial_usage() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_end","content":"Done.","partial":{"role":"assistant","usage":{"input":0,"output":21,"cacheRead":10246,"cacheWrite":0,"totalTokens":10267,"cost":{"total":0.0}}}}}"#;
        let events = extractor.extract(json);

        // Assistant + Tokens
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ExecutionEvent::Assistant { .. }));
        assert!(matches!(&events[1], ExecutionEvent::Tokens { input: 0, output: 21, cache_read: Some(10246), .. }));
    }

    /// 测试：toolcall_end 从 partial.usage 提取用量
    #[test]
    fn test_toolcall_end_with_partial_usage() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"message_update","assistantMessageEvent":{"type":"toolcall_end","toolCall":{"id":"call_1","name":"bash","arguments":{"command":"date"}},"partial":{"role":"assistant","usage":{"input":0,"output":38,"cacheRead":10092,"cacheWrite":7,"totalTokens":10137,"cost":{"total":0.0}}}}}"#;
        let events = extractor.extract(json);

        // ToolCall + Tokens
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ExecutionEvent::ToolCall { .. }));
        assert!(matches!(&events[1], ExecutionEvent::Tokens { input: 0, output: 38, cache_read: Some(10092), cache_write: Some(7), .. }));
    }

    /// 测试：agent_end 提取最终文本和用量
    #[test]
    fn test_agent_end_with_usage() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"agent_end","messages":[
            {"role":"user","content":[{"type":"text","text":"hello"}]},
            {"role":"assistant","content":[{"type":"thinking","thinking":"Let me think"},{"type":"text","text":"Here is the answer."}],"usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"totalTokens":150,"cost":{"total":0.003}}}
        ],"willRetry":false}"#;
        let events = extractor.extract(json);

        // Result + Tokens + Cost
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Result { summary } if summary == "Here is the answer.")));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { input: 100, output: 50, cache_read: Some(10), cache_write: Some(5), .. })));
    }

    /// 测试：agent_end 提取 cache-only 用量（input=0, output=0, 但有 cache）
    #[test]
    fn test_agent_end_with_cache_only_usage() {
        let mut extractor = PiExtractor::new();
        let json = r#"{"type":"agent_end","messages":[
            {"role":"user","content":[{"type":"text","text":"hello"}]},
            {"role":"assistant","content":[{"type":"text","text":"OK"}],"usage":{"input":0,"output":0,"cacheRead":10092,"cacheWrite":7,"totalTokens":10099,"cost":{"total":0.0}}}
        ],"willRetry":false}"#;
        let events = extractor.extract(json);

        // 即使 input=0,output=0，有 cache 数据也应提取 Tokens
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { input: 0, output: 0, cache_read: Some(10092), cache_write: Some(7), .. })));
    }
}
