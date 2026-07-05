//! Codewhale 执行器的事件提取器实现
//!
//! Codewhale 输出 NDJSON 格式，事件类型包括：
//! - tool_use: 工具调用（含 name / id / input.command）
//! - tool_result: 工具结果（含 id / output / status）
//! - content: 文本回复
//! - session_capture: 会话捕获（content 为 session_id）
//! - metadata: 元数据（含 model / input_tokens / output_tokens / session_id / status）
//! - done: 执行结束标记

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Codewhale 事件提取器
#[derive(Debug, Clone)]
pub struct CodewhaleExtractor {
    metadata: ExecutionMetadata,
}

impl CodewhaleExtractor {
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("codewhale".to_string()),
        }
    }

    /// 解析 NDJSON 行
    fn parse_json_line(&mut self, json: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();
        let event_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "session_capture" => {
                // 会话捕获：提取 session_id
                if let Some(sid) = json.get("content").and_then(|v| v.as_str()) {
                    if self.metadata.session_id.is_none() {
                        self.metadata.session_id = Some(sid.to_string());
                        events.push(ExecutionEvent::SessionStart {
                            session_id: sid.to_string(),
                        });
                    }
                }
            }
            "content" => {
                // 文本回复
                if let Some(text) = json.get("content").and_then(|v| v.as_str()) {
                    let trimmed = text.trim_end();
                    if !trimmed.is_empty() {
                        events.push(ExecutionEvent::Assistant {
                            content: trimmed.to_string(),
                            thinking: None,
                            message_id: None,
                        });
                    }
                }
            }
            "tool_use" => {
                // 工具调用
                let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                let input = json.get("input").cloned().unwrap_or(serde_json::json!({}));
                events.push(ExecutionEvent::ToolCall {
                    id: json.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    name: name.to_string(),
                    input,
                });
            }
            "tool_result" => {
                // 工具结果
                let output = json.get("output").and_then(|v| v.as_str()).unwrap_or_default();
                let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("completed");
                events.push(ExecutionEvent::ToolResult {
                    call_id: json.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    output: output.to_string(),
                    is_error: status == "error" || status == "failed",
                });
            }
            "metadata" => {
                // 元数据：提取 model / tokens / session_id
                if let Some(meta) = json.get("meta") {
                    if let Some(model) = meta.get("model").and_then(|v| v.as_str()) {
                        if self.metadata.model.is_none() {
                            self.metadata.model = Some(model.to_string());
                            events.push(ExecutionEvent::ModelSwitch {
                                model: model.to_string(),
                            });
                        }
                    }
                    // session_id（兜底，session_capture 可能未触发）
                    if let Some(sid) = meta.get("session_id").and_then(|v| v.as_str()) {
                        if self.metadata.session_id.is_none() {
                            self.metadata.session_id = Some(sid.to_string());
                            events.insert(0, ExecutionEvent::SessionStart {
                                session_id: sid.to_string(),
                            });
                        }
                    }
                    // Token 统计
                    let input = meta.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    let output = meta.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    if input > 0 || output > 0 {
                        events.push(ExecutionEvent::Tokens {
                            input,
                            output,
                            cache_read: None,
                            cache_write: None,
                        });
                    }
                    // 完成状态
                    if let Some(status) = meta.get("status").and_then(|v| v.as_str()) {
                        if status == "completed" || status == "success" {
                            self.metadata.mark_success();
                            self.metadata.set_finished_at();
                            events.push(ExecutionEvent::Result {
                                summary: "Task completed".to_string(),
                            });
                        } else if status == "error" || status == "failed" {
                            self.metadata.mark_failed();
                            self.metadata.set_finished_at();
                        }
                    }
                }
            }
            "done" => {
                // 执行结束标记，无事件
            }
            _ => {
                events.push(ExecutionEvent::Info {
                    message: serde_json::to_string(json).unwrap_or_default(),
                });
            }
        }

        events
    }
}

impl EventExtractor for CodewhaleExtractor {
    fn executor_name(&self) -> &str { "codewhale" }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() { return Vec::new(); }
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

impl Default for CodewhaleExtractor {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_session_capture() {
        let mut ext = CodewhaleExtractor::new();
        let events = ext.extract(r#"{"type":"session_capture","content":"ses_cw_001"}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::SessionStart { .. })));
        assert_eq!(ext.metadata().session_id.as_deref(), Some("ses_cw_001"));
    }

    #[test]
    fn test_content() {
        let mut ext = CodewhaleExtractor::new();
        let events = ext.extract(r#"{"type":"content","content":"Hello world"}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Assistant { content, .. } if content == "Hello world")));
    }

    #[test]
    fn test_tool_use() {
        let mut ext = CodewhaleExtractor::new();
        let events = ext.extract(r#"{"type":"tool_use","name":"exec_shell","id":"call_001","input":{"command":"ls -la"}}"#);
        match &events[0] {
            ExecutionEvent::ToolCall { name, input, .. } => {
                assert_eq!(name, "exec_shell");
                assert_eq!(input.get("command").and_then(|v| v.as_str()), Some("ls -la"));
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_tool_result() {
        let mut ext = CodewhaleExtractor::new();
        let events = ext.extract(r#"{"type":"tool_result","id":"call_001","output":"file.txt","status":"success"}"#);
        match &events[0] {
            ExecutionEvent::ToolResult { output, is_error, .. } => {
                assert_eq!(output, "file.txt");
                assert!(!is_error);
            }
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_metadata() {
        let mut ext = CodewhaleExtractor::new();
        let events = ext.extract(r#"{"type":"metadata","meta":{"model":"deepseek-v4-pro","input_tokens":100,"output_tokens":50,"session_id":"ses_1","status":"completed"}}"#);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { input: 100, output: 50, .. })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Result { .. })));
        assert_eq!(ext.metadata().model.as_deref(), Some("deepseek-v4-pro"));
    }

    #[test]
    fn test_empty_line() {
        let mut ext = CodewhaleExtractor::new();
        assert!(ext.extract("").is_empty());
    }
}
