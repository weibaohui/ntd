//! Hermes 执行器的事件提取器实现
//!
//! Hermes 使用特殊格式：
//! - banner 过滤：╭ │ ╰ ━ 等字符
//! - session_id 提取：Session: <id> / session_id: <id>
//! - 文本和工具调用通过特殊格式输出

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Hermes 事件提取器
///
/// 解析 Hermes 的特殊格式输出。
#[derive(Debug, Clone)]
pub struct HermesExtractor {
    /// 元数据
    metadata: ExecutionMetadata,
    /// 是否已看到完成标记
    has_done: bool,
}

impl HermesExtractor {
    /// 创建新的 Hermes 提取器
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("hermes".to_string()),
            has_done: false,
        }
    }

    /// 判断是否为 Hermes banner 行（需要过滤）
    fn is_banner_line(trimmed: &str) -> bool {
        trimmed.starts_with('╭')
            || trimmed.starts_with('│')
            || trimmed.starts_with('╰')
            || trimmed.chars().all(|c| c == ' ' || c == '━' || c == '│' || c == '╰' || c == '╭')
    }

    /// 判断是否为需要跳过的行（banner 或状态指示符）
    fn is_skippable_line(trimmed: &str) -> bool {
        Self::is_banner_line(trimmed) || trimmed.starts_with('┊')
    }

    /// 提取 session_id
    fn extract_session_prefix(trimmed: &str) -> Option<&str> {
        trimmed
            .strip_prefix("Session:")
            .or_else(|| trimmed.strip_prefix("session_id:"))
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// 解析一行
    fn parse_line(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() || Self::is_skippable_line(trimmed) {
            return Vec::new();
        }

        // 尝试解析 JSON
        if trimmed.starts_with('{') {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return self.parse_json_line(&json);
            }
        }

        // 提取 session_id
        if let Some(sid) = Self::extract_session_prefix(trimmed) {
            if self.metadata.session_id.is_none() {
                self.metadata.session_id = Some(sid.to_string());
                return vec![ExecutionEvent::SessionStart {
                    session_id: sid.to_string(),
                }];
            }
        }

        // 检查是否包含完成标记
        if trimmed.contains("done") || trimmed.contains("complete") {
            self.has_done = true;
        }

        // 检查是否包含工具调用
        if trimmed.starts_with("Calling tool:") || trimmed.contains("Executing command") {
            // 提取工具名称
            if let Some(name) = trimmed.split_whitespace().nth(2) {
                let name = name.trim_matches(|c: char| c == ':' || c == '`');
                return vec![ExecutionEvent::ToolCall {
                    id: format!("tool_{}", self.metadata.session_id.as_deref().unwrap_or("0")),
                    name: name.to_string(),
                    input: serde_json::json!({}),
                }];
            }
        }

        // 检查是否包含思考标记
        if trimmed.contains("[thinking]") || trimmed.contains("reasoning") {
            let content = trimmed
                .replace("[thinking]", "")
                .replace("[/thinking]", "")
                .replace("reasoning", "")
                .trim()
                .to_string();
            if !content.is_empty() {
                return vec![ExecutionEvent::Thinking { content }];
            }
        }

        // 其他行作为 info
        vec![ExecutionEvent::Info {
            message: trimmed.to_string(),
        }]
    }

    /// 解析 JSON 行
    fn parse_json_line(&mut self, json: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        // 提取 event_type
        let event_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "assistant" | "message" => {
                // 提取文本
                if let Some(text) = json.get("text").or_else(|| json.get("content")).and_then(|v| v.as_str()) {
                    events.push(ExecutionEvent::Assistant {
                        content: text.to_string(),
                        thinking: None,
                        message_id: json.get("id").and_then(|v| v.as_str()).map(String::from),
                    });
                }

                // 提取 reasoning
                if let Some(reasoning) = json.get("reasoning").or_else(|| json.get("thinking")).and_then(|v| v.as_str()) {
                    events.push(ExecutionEvent::Thinking {
                        content: reasoning.to_string(),
                    });
                }
            }
            "tool_call" | "tool_use" => {
                let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("bash");
                let input = json.get("arguments")
                    .or_else(|| json.get("input"))
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                events.push(ExecutionEvent::ToolCall {
                    id: json.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    name: name.to_string(),
                    input,
                });
            }
            "tool_result" | "tool" => {
                let output = json.get("output")
                    .or_else(|| json.get("result"))
                    .map(|v| {
                        if let Some(s) = v.as_str() {
                            s.to_string()
                        } else {
                            serde_json::to_string(v).unwrap_or_default()
                        }
                    })
                    .unwrap_or_default();
                events.push(ExecutionEvent::ToolResult {
                    call_id: json.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    output,
                    is_error: json.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false),
                });
            }
            "finish" | "complete" | "done" => {
                self.has_done = true;
                events.push(ExecutionEvent::Result {
                    summary: json.get("message").or_else(|| json.get("summary")).and_then(|v| v.as_str()).unwrap_or("Task completed").to_string(),
                });

                // 提取 usage
                if let Some(usage) = json.get("usage") {
                    let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    if input > 0 || output > 0 {
                        events.push(ExecutionEvent::Tokens {
                            input,
                            output,
                            cache_read: None,
                            cache_write: None,
                        });
                    }
                }
            }
            "error" => {
                let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                events.push(ExecutionEvent::Error {
                    message: msg.to_string(),
                });
            }
            _ => {
                // 未知类型，保留为 info
                events.push(ExecutionEvent::Info {
                    message: serde_json::to_string(json).unwrap_or_default(),
                });
            }
        }

        events
    }
}

impl EventExtractor for HermesExtractor {
    fn executor_name(&self) -> &str {
        "hermes"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        self.parse_line(line)
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

impl Default for HermesExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_banner_filter() {
        let _extractor = HermesExtractor::new();
        assert!(HermesExtractor::is_banner_line("╭─────"));
        assert!(HermesExtractor::is_banner_line("│ content"));
        assert!(HermesExtractor::is_banner_line("╰─────"));
        assert!(!HermesExtractor::is_banner_line("normal text"));
    }

    #[test]
    fn test_session_extraction() {
        assert_eq!(
            HermesExtractor::extract_session_prefix("Session: abc123"),
            Some("abc123")
        );
        assert_eq!(
            HermesExtractor::extract_session_prefix("session_id: xyz789"),
            Some("xyz789")
        );
    }

    #[test]
    fn test_normal_text() {
        let mut extractor = HermesExtractor::new();
        let events = extractor.extract("Hello, this is a normal message");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Info { .. }));
    }

    #[test]
    fn test_session_prefix() {
        let mut extractor = HermesExtractor::new();
        let events = extractor.extract("Session: test-session-001");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::SessionStart { session_id } if session_id == "test-session-001"));
    }

    #[test]
    fn test_json_message() {
        let mut extractor = HermesExtractor::new();
        let json = r#"{"type":"assistant","text":"Hello from Hermes"}"#;
        let events = extractor.extract(json);
        assert!(matches!(&events[0], ExecutionEvent::Assistant { content, .. } if content == "Hello from Hermes"));
    }

    #[test]
    fn test_empty_line() {
        let mut extractor = HermesExtractor::new();
        assert!(extractor.extract("").is_empty());
        assert!(extractor.extract("   ").is_empty());
    }
}
