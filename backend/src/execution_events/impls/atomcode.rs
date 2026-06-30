//! AtomCode 执行器的事件提取器实现
//!
//! AtomCode 的输出比较特殊——不是 JSON 格式。
//! stdout 是纯文本（AI 回复），stderr 包含以 `[xxx]` 前缀标记的结构化事件。
//!
//! 事件类型：
//! - `[tokens] prompt=N completion=M` → Tokens 事件
//! - `[done] <duration> tokens=N turns=N tool_calls=N` → StepFinish + 元数据更新
//! - `[tool→ <name> args={<json>}]` → ToolCall 事件
//! - `[tool← <name> ...]` → ToolResult 事件
//! - `[engine v2] new stack active (model xxx)` → ModelSwitch 事件
//! - `[thinking] <text>` → 思考内容（多行累积为一块直到非 [thinking] 行）

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// AtomCode 事件提取器
#[derive(Debug, Clone)]
pub struct AtomcodeExtractor {
    metadata: ExecutionMetadata,
    /// 用于追踪 tool_call_id，匹配 tool→ 和 tool←
    pending_tool_id: Option<String>,
    /// 思考块缓冲：多行 [thinking] 累积为一块，非 [thinking] 行触发 flush
    pending_thinking: Vec<String>,
}

impl AtomcodeExtractor {
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("atomcode".to_string()),
            pending_tool_id: None,
            pending_thinking: Vec::new(),
        }
    }

    /// 解析 stderr 中的结构化事件行（以 `[xxx]` 开头）
    fn parse_stderr_line(&mut self, trimmed: &str) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        // 思考块处理：多行 [thinking] 累积为一块
        if trimmed.starts_with("[thinking]") {
            let content = trimmed["[thinking]".len()..].trim();
            self.pending_thinking.push(content.to_string());
            return events;
        }

        // 非 thinking 行，先 flush 之前缓冲的思考块
        self.flush_thinking(&mut events);

        // 跳过流式/headless 标记
        if trimmed.starts_with("[tool-streaming") || trimmed.starts_with("[headless]") {
            return events;
        }

        // 引擎信息行: [engine v2] new stack active (model deepseek-v4-flash)
        // atomcode 的 model 信息在 stderr 首行括号中
        if trimmed.starts_with("[engine") {
            if let Some(pos) = trimmed.find("(model ") {
                let after = &trimmed[pos + 7..]; // 跳过 "(model "
                let model = after.trim_end_matches(')').trim();
                if !model.is_empty() {
                    if self.metadata.model.is_none() {
                        self.metadata.model = Some(model.to_string());
                        events.push(ExecutionEvent::ModelSwitch {
                            model: model.to_string(),
                        });
                    }
                }
            }
            return events;
        }

        if trimmed.starts_with("[tokens]") {
            // 解析 token 统计: [tokens] prompt=11 completion=5
            let mut prompt_tokens = 0u64;
            let mut completion_tokens = 0u64;
            for part in trimmed.split_whitespace().skip(1) {
                if let Some((key, val)) = part.split_once('=') {
                    match key {
                        "prompt" => prompt_tokens = val.parse().unwrap_or(0),
                        "completion" => completion_tokens = val.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
            events.push(ExecutionEvent::Tokens {
                input: prompt_tokens,
                output: completion_tokens,
                cache_read: None,
                cache_write: None,
            });
        } else if trimmed.starts_with("[done]") {
            // 解析完成事件: [done] 4.6s tokens=100 turns=2 tool_calls=1
            let mut duration_ms = None;
            let mut total_tokens = 0u64;
            for (i, part) in trimmed.split_whitespace().enumerate() {
                if i == 1 {
                    // "4.6s" → 4600 ms
                    let s = part.trim_end_matches('s');
                    if let Ok(secs) = s.parse::<f64>() {
                        duration_ms = Some((secs * 1000.0) as u64);
                    }
                } else if let Some((key, val)) = part.split_once('=') {
                    if key == "tokens" {
                        total_tokens = val.parse().unwrap_or(0);
                    }
                }
            }

            if let Some(ms) = duration_ms {
                self.metadata.duration_ms = ms;
            }
            if total_tokens > 0 {
                self.metadata.input_tokens = total_tokens;
            }
            self.metadata.set_finished_at();

            events.push(ExecutionEvent::StepFinish {
                name: "execution".to_string(),
                index: 0,
            });
        } else if trimmed.starts_with("[tool→") || trimmed.starts_with("[tool->") {
            // 工具调用发起: [tool→ bash args={"command": "ls"}]
            let content = trimmed
                .trim_start_matches("[tool→")
                .trim_start_matches("[tool->")
                .trim();

            let (name, args_json) = if let Some(idx) = content.find(" args=") {
                let name = content[..idx].trim();
                // 去掉末尾的 ]，因为拼接行格式为 [tool→ name args={...}]
                let raw_args = content[idx + 6..].trim().trim_end_matches(']');
                let parsed = if raw_args.starts_with('{') {
                    serde_json::from_str(raw_args).ok()
                } else {
                    None
                };
                (name.to_string(), parsed.unwrap_or(serde_json::json!({})))
            } else {
                (content.to_string(), serde_json::json!({}))
            };

            let tool_id = format!("tool_{}", self.metadata.session_id.as_deref().unwrap_or("0"));
            self.pending_tool_id = Some(tool_id.clone());

            events.push(ExecutionEvent::ToolCall {
                id: tool_id,
                name,
                input: args_json,
            });
        } else if trimmed.starts_with("[tool←") || trimmed.starts_with("[tool<-") {
            // 工具调用返回: [tool← bash ...]
            let tool_id = self.pending_tool_id.take().unwrap_or_default();
            let content = trimmed
                .trim_start_matches("[tool←")
                .trim_start_matches("[tool<-")
                .trim();

            events.push(ExecutionEvent::ToolResult {
                call_id: tool_id,
                output: content.to_string(),
                is_error: false,
            });
        } else if trimmed.starts_with("[approval-denied]") {
            events.push(ExecutionEvent::Error {
                message: trimmed.to_string(),
            });
        }

        events
    }

    /// 将缓冲的思考行合并为一个 Thinking 事件，然后清空缓冲
    fn flush_thinking(&mut self, events: &mut Vec<ExecutionEvent>) {
        if self.pending_thinking.is_empty() {
            return;
        }
        let content = self.pending_thinking.join("\n");
        self.pending_thinking.clear();
        if !content.trim().is_empty() {
            events.push(ExecutionEvent::Thinking { content });
        }
    }

    /// 解析 stdout 中的纯文本（AI 回复）
    fn parse_stdout_line(&mut self, trimmed: &str) -> Vec<ExecutionEvent> {
        if trimmed.is_empty() {
            return Vec::new();
        }
        vec![ExecutionEvent::Assistant {
            content: trimmed.to_string(),
            thinking: None,
            message_id: None,
        }]
    }
}

impl EventExtractor for AtomcodeExtractor {
    fn executor_name(&self) -> &str {
        "atomcode"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // 检查是否是 stderr 结构化行（以 `[` 开头）
        if trimmed.starts_with('[') {
            return self.parse_stderr_line(trimmed);
        }

        // stdout 纯文本行前，先 flush 之前缓冲的思考块
        let mut events = Vec::new();
        self.flush_thinking(&mut events);
        events.extend(self.parse_stdout_line(trimmed));
        events
    }

    fn extract_stderr(&mut self, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // stderr 中的结构化事件
        if trimmed.starts_with('[') {
            let events = self.parse_stderr_line(trimmed);
            // 只取第一个事件（通常只有一个）
            return events.into_iter().next();
        }

        // 非结构化 stderr：关键字分类
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

impl Default for AtomcodeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokens_line() {
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract("[tokens] prompt=120 completion=45");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::Tokens { input, output, .. } => {
                assert_eq!(*input, 120);
                assert_eq!(*output, 45);
            }
            _ => panic!("Expected Tokens event"),
        }
    }

    #[test]
    fn test_done_line() {
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract("[done] 4.6s tokens=100 turns=2 tool_calls=1");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::StepFinish { .. }));
        assert_eq!(ext.metadata().duration_ms, 4600);
    }

    #[test]
    fn test_tool_call() {
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract(r#"[tool→ bash args={"command": "ls -la"}]"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::ToolCall { name, input, .. } => {
                assert_eq!(name, "bash");
                assert_eq!(input.get("command").and_then(|v| v.as_str()), Some("ls -la"));
            }
            _ => panic!("Expected ToolCall event"),
        }
    }

    #[test]
    fn test_stdout_text() {
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract("Hello, this is a text response");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Assistant { content, .. } if content == "Hello, this is a text response"));
    }

    #[test]
    fn test_empty_line() {
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("").is_empty());
        assert!(ext.extract("   ").is_empty());
    }

    #[test]
    fn test_streaming_marker_skipped() {
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("[tool-streaming...]").is_empty());
        assert!(ext.extract("[headless]").is_empty());
    }

    #[test]
    fn test_engine_model_extraction() {
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract("[engine v2] new stack active (model deepseek-v4-flash)");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ModelSwitch { model } if model == "deepseek-v4-flash"));
        assert_eq!(ext.metadata().model.as_deref(), Some("deepseek-v4-flash"));
    }

    #[test]
    fn test_engine_model_only_once() {
        // 重复出现时不生成重复事件
        let mut ext = AtomcodeExtractor::new();
        let events1 = ext.extract("[engine v2] new stack active (model deepseek-v4-flash)");
        assert_eq!(events1.len(), 1);
        let events2 = ext.extract("[engine v2] new stack active (model other-model)");
        assert_eq!(events2.len(), 0); // 已设置，不再生成
        assert_eq!(ext.metadata().model.as_deref(), Some("deepseek-v4-flash"));
    }

    #[test]
    fn test_thinking_accumulation() {
        // 多行 thinking 累积，直到非 thinking 行才输出
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("[thinking] line 1").is_empty());
        assert!(ext.extract("[thinking] line 2").is_empty());
        assert!(ext.extract("[thinking] line 3").is_empty());

        let events = ext.extract("[done] 1s turns=1 tool_calls=0");
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Thinking { content } if content == "line 1\nline 2\nline 3")));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::StepFinish { .. })));
    }

    #[test]
    fn test_thinking_flushed_by_stdout() {
        // stdout 纯文本行也能触发 thinking flush
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("[thinking] thinking text").is_empty());

        let events = ext.extract("plain response");
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Thinking { .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Assistant { .. })));
    }

    #[test]
    fn test_thinking_reset_between_blocks() {
        // 两块 thinking 之间被非 thinking 行隔开，每块独立输出
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("[thinking] block 1").is_empty());
        let events1 = ext.extract("[done] 1s turns=1 tool_calls=0");
        assert!(events1.iter().any(|e| matches!(e, ExecutionEvent::Thinking { content } if content == "block 1")));

        assert!(ext.extract("[thinking] block 2").is_empty());
        let events2 = ext.extract("[done] 1s turns=1 tool_calls=0");
        assert!(events2.iter().any(|e| matches!(e, ExecutionEvent::Thinking { content } if content == "block 2")));
    }
}
