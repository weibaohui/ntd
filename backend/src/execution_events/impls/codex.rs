//! Codex 执行器的事件提取器实现
//!
//! Codex 使用独特的 JSON 格式：
//! - item.started / item.completed
//! - turn.completed
//! - typed events: agent_message, agent_reasoning, tool_call, tool_result, error, etc.

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// Codex 事件提取器
///
/// 解析 Codex 的 JSON 格式输出。
#[derive(Debug, Clone)]
pub struct CodexExtractor {
    /// 元数据
    metadata: ExecutionMetadata,
    /// 步骤计数器
    step_index: u32,
}

impl CodexExtractor {
    /// 创建新的 Codex 提取器
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("codex".to_string()),
            step_index: 0,
        }
    }

    /// 提取 (event_type, event_value) 二元组
    fn extract_event_type(json: &serde_json::Value) -> Option<(&str, &serde_json::Value)> {
        // 优先从顶层 type 字段提取
        if let Some(typ) = json.get("type").and_then(|v| v.as_str()) {
            return Some((typ, json));
        }
        // 回退到 msg.type
        if let Some(msg) = json.get("msg") {
            if let Some(typ) = msg.get("type").and_then(|v| v.as_str()) {
                return Some((typ, msg));
            }
        }
        None
    }

    /// 解析一行 JSON
    fn parse_json_line(&mut self, json: &serde_json::Value) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        let Some((event_type, event_value)) = Self::extract_event_type(json) else {
            return vec![ExecutionEvent::Info {
                message: serde_json::to_string(json).unwrap_or_default(),
            }];
        };

        match event_type {
            // Item 事件
            "item.started" => {
                if let Some(item) = event_value.get("item") {
                    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if item_type == "command_execution" {
                        let idx = self.step_index;
                        self.step_index += 1;
                        events.push(ExecutionEvent::StepStart {
                            name: format!("command_{}", idx),
                            index: idx,
                        });
                    }
                }
            }
            "item.completed" => {
                if let Some(item) = event_value.get("item") {
                    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if item_type == "command_execution" {
                        let idx = self.step_index.saturating_sub(1);
                        events.push(ExecutionEvent::StepFinish {
                            name: format!("command_{}", idx),
                            index: idx,
                        });
                    } else if item_type == "agent_message" {
                        // 提取 agent_message 文本
                        if let Some(text) = item.get("message").and_then(|v| v.as_str()) {
                            events.push(ExecutionEvent::Result {
                                summary: text.to_string(),
                            });
                        }
                    } else if item_type == "collab_tool_call" {
                        // codex 派生子 agent 走 collab_tool_call(spawn_agent)；
                        // 归一成 ToolCall，让 agent_progress 能识别（item.completed 才带 prompt + receiver）。
                        if let Some(ev) = collab_spawn_agent_tool_call(item) {
                            events.push(ev);
                        }
                    }
                }
            }

            // Thread 事件：thread.started 是当前版本 Codex 输出会话 ID 的唯一位置
            // （docs/samples/codex/output.txt 实证），thread_id 即 `codex exec resume`
            // 的会话凭据。claim_session 幂等判重保证只记首次，与旧格式 session_configured 路径先到先赢。
            "thread.started" => {
                if let Some(tid) = json.get("thread_id").and_then(|v| v.as_str()) {
                    events.extend(self.metadata.claim_session(tid));
                }
            }

            // Turn 事件
            "turn.completed" => {
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

                let idx = self.step_index.saturating_sub(1);
                events.push(ExecutionEvent::StepFinish {
                    name: format!("turn_{}", idx),
                    index: idx,
                });
            }
            "turn.started" => {
                let idx = self.step_index;
                self.step_index += 1;
                events.push(ExecutionEvent::StepStart {
                    name: format!("turn_{}", idx),
                    index: idx,
                });
            }

            // Typed 事件
            "agent_message" | "agent_message_delta" | "assistant_message" => {
                // 提取文本
                let text = json
                    .get("message")
                    .or_else(|| json.get("delta"))
                    .or_else(|| json.get("text"))
                    .or_else(|| json.get("content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !text.is_empty() {
                    events.push(ExecutionEvent::Assistant {
                        content: text.to_string(),
                        thinking: None,
                        message_id: None,
                    });
                }
            }
            "agent_reasoning" | "agent_reasoning_delta" | "reasoning" | "reasoning_delta" => {
                let text = json
                    .get("message")
                    .or_else(|| json.get("delta"))
                    .or_else(|| json.get("text"))
                    .or_else(|| json.get("content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !text.is_empty() {
                    events.push(ExecutionEvent::Thinking {
                        content: text.to_string(),
                    });
                }
            }
            "exec_command_begin" | "tool_call_begin" | "tool_call" => {
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
            "exec_command_end" | "tool_call_end" | "tool_result" => {
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
                    call_id: json.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    output,
                    is_error: false,
                });
            }
            "task_complete" => {
                events.push(ExecutionEvent::Result {
                    summary: json.get("result").and_then(|v| v.as_str()).unwrap_or("Task completed").to_string(),
                });
            }
            "error" => {
                let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                events.push(ExecutionEvent::Error {
                    message: msg.to_string(),
                });
            }
            "session_configured" | "task_started" => {
                // session 首现认领（claim_session 幂等，先到先赢）
                if let Some(sid) = json.get("session_id").and_then(|v| v.as_str()) {
                    events.extend(self.metadata.claim_session(sid));
                }

                // 提取 model
                if let Some(model) = json.get("model").or_else(|| json.get("model_slug")).and_then(|v| v.as_str()) {
                    if self.metadata.model.is_none() {
                        self.metadata.model = Some(model.to_string());
                        events.push(ExecutionEvent::ModelSwitch {
                            model: model.to_string(),
                        });
                    }
                }
            }
            _ => {
                // 未知类型，保留原始 JSON
                events.push(ExecutionEvent::Info {
                    message: format!("[{}]", event_type),
                });
            }
        }

        events
    }
}

/// codex 的 collab_tool_call(tool=spawn_agent) → ToolCall，让多 agent 提取器识别派生的子 agent。
///
/// 仅 item.completed 调用（此时才带 prompt 与 receiver_thread_ids）；tool 非 spawn_agent 返回 None。
fn collab_spawn_agent_tool_call(item: &serde_json::Value) -> Option<ExecutionEvent> {
    let tool = item.get("tool").and_then(|v| v.as_str()).unwrap_or("");
    if tool != "spawn_agent" {
        return None;
    }
    let input = serde_json::json!({
        "prompt": item.get("prompt").and_then(|v| v.as_str()).unwrap_or(""),
        "receiver_thread_ids": item.get("receiver_thread_ids").cloned().unwrap_or(serde_json::json!([])),
    });
    Some(ExecutionEvent::ToolCall {
        id: item.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        name: "spawn_agent".to_string(),
        input,
    })
}

impl EventExtractor for CodexExtractor {
    fn executor_name(&self) -> &str {
        "codex"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        if trimmed.starts_with('{') {
            match serde_json::from_str::<serde_json::Value>(trimmed) {
                Ok(json) => self.parse_json_line(&json),
                Err(_) => {
                    vec![ExecutionEvent::Info {
                        message: trimmed.to_string(),
                    }]
                }
            }
        } else {
            vec![ExecutionEvent::Info {
                message: trimmed.to_string(),
            }]
        }
    }

    // 096-W1：本 override 与 trait 默认实现**有意不同**，勿删——
    // 默认实现按 "error" 关键字分流 Error/Info，Codex 统一 Info（误报率高）。
    fn extract_stderr(&mut self, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Codex 特殊处理：stderr 的 error 不一定是 error 类型，统一作为 Info 上报
        Some(ExecutionEvent::Info {
            message: trimmed.to_string(),
        })
    }

    fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
        &mut self.metadata
    }
}

impl Default for CodexExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call() {
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"tool_call","id":"call_1","name":"bash","arguments":"{\"command\":\"ls\"}"}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ToolCall { name, .. } if name == "bash"));
    }

    #[test]
    fn test_reasoning() {
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"agent_reasoning","content":"Thinking about the problem..."}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Thinking { .. }));
    }

    #[test]
    fn test_item_started() {
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"item.started","item":{"type":"command_execution","id":"cmd_1"}}"#;
        let events = extractor.extract(json);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::StepStart { index: 0, .. }));
    }

    #[test]
    fn test_usage() {
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"turn.completed","usage":{"input_tokens":200,"output_tokens":100}}"#;
        let events = extractor.extract(json);

        assert!(events.len() >= 2); // Tokens + StepFinish
        assert!(matches!(&events[0], ExecutionEvent::Tokens { input: 200, output: 100, .. }));
    }

    #[test]
    fn test_empty_line() {
        let mut extractor = CodexExtractor::new();
        assert!(extractor.extract("").is_empty());
    }

    // ====================== 059 R1：thread_id 入库测试 ======================

    #[test]
    fn test_thread_started_extracts_session_id() {
        // 真实输出格式（docs/samples/codex/output.txt）：thread.started 携 thread_id，
        // 必须产出 SessionStart 并写入 metadata，否则 execution_records.session_id 恒为 NULL
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"thread.started","thread_id":"019f13f6-4be4-74f1-8b77-74fe3878091c"}"#;
        let events = extractor.extract(json);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::SessionStart { session_id } if session_id == "019f13f6-4be4-74f1-8b77-74fe3878091c")));
        assert_eq!(extractor.metadata().session_id.as_deref(), Some("019f13f6-4be4-74f1-8b77-74fe3878091c"));
    }

    #[test]
    fn test_thread_started_dedup_repeated_events() {
        // resume 后 CLI 会重吐 thread.started：判重保证只记一次，避免刷屏与重复回写
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"thread.started","thread_id":"tid_1"}"#;
        let first = extractor.extract(json);
        let second = extractor.extract(json);
        assert_eq!(first.iter().filter(|e| matches!(e, ExecutionEvent::SessionStart { .. })).count(), 1);
        assert!(second.iter().all(|e| !matches!(e, ExecutionEvent::SessionStart { .. })));
    }

    #[test]
    fn test_legacy_session_configured_still_extracts() {
        // 旧格式回归保护：早期 codex 用 session_configured + session_id，不能因新增分支失效
        let mut extractor = CodexExtractor::new();
        let json = r#"{"type":"session_configured","session_id":"legacy_sid_1","model":"o3"}"#;
        let events = extractor.extract(json);
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::SessionStart { session_id } if session_id == "legacy_sid_1")));
    }

    #[test]
    fn test_thread_started_without_thread_id_no_session() {
        // 缺 thread_id 字段时不应误提取，metadata 保持 None
        let mut extractor = CodexExtractor::new();
        let events = extractor.extract(r#"{"type":"thread.started"}"#);
        assert!(events.iter().all(|e| !matches!(e, ExecutionEvent::SessionStart { .. })));
        assert!(extractor.metadata().session_id.is_none());
    }

    // ====================== 096-W1：extract_stderr override 回归保护 ======================

    // 钉住 Codex extract_stderr 的差异化行为：含 error 关键字的 stderr 行统一判为 Info。
    // trait 默认实现（extractor.rs:33-48）会将其分流为 Error 事件，Codex 因 stderr 误报率高
    // 而 override 为恒 Info。若有人误删 override 回退默认实现，本测试会失败。
    #[test]
    fn test_extract_stderr_error_keyword_returns_info() {
        let mut extractor = CodexExtractor::new();
        // 含 "error" 关键字——默认实现会返回 Error，Codex override 必须返回 Info
        let event = extractor.extract_stderr("Error: compilation failed");
        match event.as_ref() {
            Some(ExecutionEvent::Info { message }) => assert_eq!(message.as_str(), "Error: compilation failed"),
            // 命中此分支说明 override 被删、回退成了默认的 error 关键字分流
            Some(ExecutionEvent::Error { .. }) => {
                panic!("含 error 关键字的 stderr 不应判为 Error（Codex override 统一 Info）")
            }
            None => panic!("预期 Info 事件，实际返回 None"),
            Some(_) => panic!("预期 Info 事件，实际返回其他事件类型"),
        }
    }

    // 空行/纯空白行不应产出事件，避免 stderr 空行噪声污染事件流
    #[test]
    fn test_extract_stderr_empty_line_returns_none() {
        let mut extractor = CodexExtractor::new();
        assert!(extractor.extract_stderr("").is_none());
        assert!(extractor.extract_stderr("   \t  ").is_none());
    }
}
