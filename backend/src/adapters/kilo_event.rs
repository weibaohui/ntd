//! Kilo-specific event parsing.
//!
//! Kilo 复用了 OpenCode 的事件格式（hyphenated event types，例如 step-start、tool-use），
//! 并额外使用 camelCase 字段名（如 sessionID）。
//!
//! 行为与 Opencode/Zhanlu 完全一致：相同的命令参数、相同的 JSON 输出格式、
//! 相同的退出码语义（非零但含 step_finish 事件时视为成功）。
//! 所以这里只把类型名从 Opencode 前缀重命名为 Kilo，serde 结构和字段映射保持完全一致。

use std::collections::HashMap;
use serde::Deserialize;

/// Kilo agent event with hyphenated type names (与 OpenCode 完全相同的 JSON 结构)
#[derive(Debug, Clone, Deserialize)]
pub struct KiloAgentEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub timestamp: Option<u64>,
    #[serde(default, rename = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub part: Option<KiloAgentPart>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KiloAgentPart {
    #[serde(rename = "type")]
    pub part_type: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub state: Option<KiloAgentToolState>,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tokens: Option<KiloAgentTokens>,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KiloAgentToolState {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub input: Option<KiloAgentToolInput>,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KiloAgentToolInput {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl KiloAgentToolInput {
    pub fn to_full_json(&self) -> String {
        let mut map = serde_json::Map::new();
        if let Some(ref cmd) = self.command {
            map.insert("command".into(), serde_json::Value::String(cmd.clone()));
        }
        if let Some(ref desc) = self.description {
            map.insert("description".into(), serde_json::Value::String(desc.clone()));
        }
        for (k, v) in &self.extra {
            map.insert(k.clone(), v.clone());
        }
        serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KiloAgentTokens {
    pub total: u64,
    pub input: u64,
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub cache: KiloAgentCacheTokens,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct KiloAgentCacheTokens {
    #[serde(default)]
    pub read: u64,
    #[serde(default)]
    pub write: u64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    // ── KiloAgentEvent deserialization ──────────────────────────────────

    #[test]
    fn test_kilo_agent_event_basic_step_start() {
        let json = r#"{"type":"step_start","timestamp":1700000000000}"#;
        let event: KiloAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "step_start");
        assert_eq!(event.timestamp, Some(1700000000000u64));
        assert!(event.session_id.is_none());
        assert!(event.part.is_none());
    }

    #[test]
    fn test_kilo_agent_event_hyphenated_step_start() {
        let json = r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_abc123"}"#;
        let event: KiloAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "step-start");
        assert_eq!(event.timestamp, Some(1777471473403u64));
        assert_eq!(event.session_id, Some("ses_abc123".to_string()));
        assert!(event.part.is_none());
    }

    #[test]
    fn test_kilo_agent_event_missing_timestamp_defaults_to_none() {
        // timestamp is #[serde(default)] so missing → None
        let json = r#"{"type":"unknown"}"#;
        let event: KiloAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "unknown");
        assert!(event.timestamp.is_none());
    }

    #[test]
    fn test_kilo_agent_event_with_text_part() {
        let json = r#"{"type":"text","timestamp":1700000000001,"sessionID":"ses_xyz","part":{"type":"text","text":"hello kilo"}}"#;
        let event: KiloAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "text");
        assert_eq!(event.session_id, Some("ses_xyz".to_string()));
        let part = event.part.unwrap();
        assert_eq!(part.text, Some("hello kilo".to_string()));
        assert_eq!(part.part_type, Some("text".to_string()));
    }

    #[test]
    fn test_kilo_agent_event_with_step_finish_part() {
        let json = r#"{"type":"step-finish","timestamp":1700000000002,"part":{"type":"step-finish","reason":"stop","tokens":{"total":200,"input":150,"output":50,"reasoning":0,"cache":{"read":10,"write":5}},"cost":0.0025}}"#;
        let event: KiloAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "step-finish");
        let part = event.part.unwrap();
        assert_eq!(part.reason, Some("stop".to_string()));
        assert_eq!(part.cost, Some(0.0025));
        let tokens = part.tokens.unwrap();
        assert_eq!(tokens.total, 200);
        assert_eq!(tokens.input, 150);
        assert_eq!(tokens.output, 50);
        assert_eq!(tokens.reasoning, 0);
        assert_eq!(tokens.cache.read, 10);
        assert_eq!(tokens.cache.write, 5);
    }

    #[test]
    fn test_kilo_agent_event_with_tool_use_part() {
        let json = r#"{"type":"tool-use","timestamp":1700000000003,"part":{"type":"tool_use","tool":"bash","state":{"status":"running","input":{"description":"list files","command":"ls -la"},"output":null}}}"#;
        let event: KiloAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "tool-use");
        let part = event.part.unwrap();
        assert_eq!(part.tool, Some("bash".to_string()));
        let state = part.state.unwrap();
        assert_eq!(state.status, Some("running".to_string()));
        assert!(state.output.is_none());
        let input = state.input.unwrap();
        assert_eq!(input.description, Some("list files".to_string()));
        assert_eq!(input.command, Some("ls -la".to_string()));
    }

    #[test]
    fn test_kilo_agent_event_invalid_json_fails() {
        let result: Result<KiloAgentEvent, _> = serde_json::from_str("not json at all");
        assert!(result.is_err());
    }

    // ── KiloAgentPart deserialization ────────────────────────────────────

    #[test]
    fn test_kilo_agent_part_all_optional_fields_default_to_none() {
        // Only type field is required (but actually part_type is Option<String>)
        let json = r#"{"type":"text"}"#;
        let part: KiloAgentPart = serde_json::from_str(json).unwrap();
        assert_eq!(part.part_type, Some("text".to_string()));
        assert!(part.id.is_none());
        assert!(part.text.is_none());
        assert!(part.tool.is_none());
        assert!(part.call_id.is_none());
        assert!(part.state.is_none());
        assert!(part.message_id.is_none());
        assert!(part.session_id.is_none());
        assert!(part.tokens.is_none());
        assert!(part.cost.is_none());
        assert!(part.reason.is_none());
    }

    #[test]
    fn test_kilo_agent_part_with_session_id_in_part() {
        let json = r#"{"type":"step-finish","session_id":"ses_from_part"}"#;
        let part: KiloAgentPart = serde_json::from_str(json).unwrap();
        assert_eq!(part.session_id, Some("ses_from_part".to_string()));
    }

    // ── KiloAgentToolState deserialization ───────────────────────────────

    #[test]
    fn test_kilo_agent_tool_state_complete() {
        let json = r#"{"status":"completed","input":{"description":"echo test","command":"echo test"},"output":"test\n"}"#;
        let state: KiloAgentToolState = serde_json::from_str(json).unwrap();
        assert_eq!(state.status, Some("completed".to_string()));
        assert_eq!(state.output, Some("test\n".to_string()));
        let input = state.input.unwrap();
        assert_eq!(input.description, Some("echo test".to_string()));
        assert_eq!(input.command, Some("echo test".to_string()));
    }

    #[test]
    fn test_kilo_agent_tool_state_all_optional() {
        let json = r#"{}"#;
        let state: KiloAgentToolState = serde_json::from_str(json).unwrap();
        assert!(state.status.is_none());
        assert!(state.input.is_none());
        assert!(state.output.is_none());
    }

    // ── KiloAgentToolInput.to_full_json() ────────────────────────────────

    #[test]
    fn test_kilo_agent_tool_input_to_full_json_with_command_and_description() {
        let json = r#"{"command":"ls -la","description":"list all files"}"#;
        let input: KiloAgentToolInput = serde_json::from_str(json).unwrap();
        let result = input.to_full_json();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["command"], "ls -la");
        assert_eq!(parsed["description"], "list all files");
    }

    #[test]
    fn test_kilo_agent_tool_input_to_full_json_with_extra_fields() {
        let json = r#"{"description":"write file","path":"/tmp/test.txt","content":"hello"}"#;
        let input: KiloAgentToolInput = serde_json::from_str(json).unwrap();
        let result = input.to_full_json();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["description"], "write file");
        // Extra fields are included via flatten
        assert_eq!(parsed["path"], "/tmp/test.txt");
        assert_eq!(parsed["content"], "hello");
    }

    #[test]
    fn test_kilo_agent_tool_input_to_full_json_empty_input() {
        // No command, no description, no extra fields
        let json = r#"{}"#;
        let input: KiloAgentToolInput = serde_json::from_str(json).unwrap();
        let result = input.to_full_json();
        // Should produce a valid (possibly empty) JSON object
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn test_kilo_agent_tool_input_to_full_json_only_command() {
        let json = r#"{"command":"git status"}"#;
        let input: KiloAgentToolInput = serde_json::from_str(json).unwrap();
        let result = input.to_full_json();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["command"], "git status");
        assert!(parsed.get("description").is_none() || parsed["description"].is_null());
    }

    #[test]
    fn test_kilo_agent_tool_input_to_full_json_returns_valid_json_string() {
        let json = r#"{"description":"test","command":"pwd"}"#;
        let input: KiloAgentToolInput = serde_json::from_str(json).unwrap();
        let result = input.to_full_json();
        // Must be non-empty and parseable
        assert!(!result.is_empty());
        assert!(serde_json::from_str::<serde_json::Value>(&result).is_ok());
    }

    // ── KiloAgentTokens deserialization ──────────────────────────────────

    #[test]
    fn test_kilo_agent_tokens_all_fields() {
        let json = r#"{"total":14155,"input":13862,"output":293,"reasoning":100,"cache":{"read":500,"write":200}}"#;
        let tokens: KiloAgentTokens = serde_json::from_str(json).unwrap();
        assert_eq!(tokens.total, 14155);
        assert_eq!(tokens.input, 13862);
        assert_eq!(tokens.output, 293);
        assert_eq!(tokens.reasoning, 100);
        assert_eq!(tokens.cache.read, 500);
        assert_eq!(tokens.cache.write, 200);
    }

    #[test]
    fn test_kilo_agent_tokens_reasoning_defaults_to_zero() {
        let json = r#"{"total":100,"input":70,"output":30}"#;
        let tokens: KiloAgentTokens = serde_json::from_str(json).unwrap();
        assert_eq!(tokens.reasoning, 0);
    }

    #[test]
    fn test_kilo_agent_tokens_cache_defaults_to_zero() {
        let json = r#"{"total":100,"input":70,"output":30}"#;
        let tokens: KiloAgentTokens = serde_json::from_str(json).unwrap();
        // cache field has Default, so read/write both 0
        assert_eq!(tokens.cache.read, 0);
        assert_eq!(tokens.cache.write, 0);
    }

    #[test]
    fn test_kilo_agent_tokens_zero_cost_step_finish() {
        // Real Kilo output: cost=0 when using free tier
        let json = r#"{"total":14155,"input":13862,"output":293,"reasoning":0,"cache":{"write":0,"read":0}}"#;
        let tokens: KiloAgentTokens = serde_json::from_str(json).unwrap();
        assert_eq!(tokens.total, 14155);
        assert_eq!(tokens.cache.write, 0);
        assert_eq!(tokens.cache.read, 0);
    }

    // ── KiloAgentCacheTokens defaults ────────────────────────────────────

    #[test]
    fn test_kilo_agent_cache_tokens_default_is_zero() {
        let cache = KiloAgentCacheTokens::default();
        assert_eq!(cache.read, 0);
        assert_eq!(cache.write, 0);
    }

    #[test]
    fn test_kilo_agent_cache_tokens_partial_deserialization() {
        // Only write provided, read should default to 0
        let json = r#"{"write":42}"#;
        let cache: KiloAgentCacheTokens = serde_json::from_str(json).unwrap();
        assert_eq!(cache.read, 0);
        assert_eq!(cache.write, 42);
    }

    // ── Round-trip / integration deserialization ─────────────────────────

    #[test]
    fn test_kilo_agent_event_real_step_finish_json() {
        // Based on actual Kilo CLI output captured during testing
        let json = r#"{"type":"step-finish","timestamp":1777471505168,"sessionID":"ses_xxx","part":{"type":"step-finish","reason":"stop","tokens":{"total":14155,"input":13862,"output":293,"reasoning":0,"cache":{"write":0,"read":0}},"cost":0}}"#;
        let event: KiloAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "step-finish");
        assert_eq!(event.session_id, Some("ses_xxx".to_string()));
        let part = event.part.unwrap();
        assert_eq!(part.reason, Some("stop".to_string()));
        // cost: 0 (integer in JSON) should deserialize to Some(0.0)
        assert_eq!(part.cost, Some(0.0));
        let tokens = part.tokens.unwrap();
        assert_eq!(tokens.total, 14155);
        assert_eq!(tokens.input, 13862);
        assert_eq!(tokens.output, 293);
    }

    #[test]
    fn test_kilo_agent_event_real_text_json() {
        let json = r#"{"type":"text","timestamp":1777471505165,"sessionID":"ses_xxx","part":{"type":"text","text":"Hello, this is a test response from Kilo"}}"#;
        let event: KiloAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "text");
        let part = event.part.unwrap();
        assert_eq!(part.text, Some("Hello, this is a test response from Kilo".to_string()));
    }
}
