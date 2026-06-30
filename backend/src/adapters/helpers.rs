//! 适配器层共享的解析与构造辅助函数。
//!
//! 原本散落在 12 个适配器 `parse_output_line` / `parse_stderr_line`
//! 里的样板代码（trim + 空行检查 + JSON 解析 + ParsedLogEntry 构造）
//! 在这里集中维护一次，新增适配器只需调用这些 helper 即可。
//!
//! 设计目标：
//! - **不改变行为**：所有 helper 与原内联代码 100% 等价（trim 规则、空行判定、JSON 错误处理）。
//! - **不强加 trait 方法**：保持当前 `CodeExecutor` 的 trait 形状不变（避免一次性的大改动）。
//! - **可单测**：每个 helper 都有独立单元测试覆盖空行、空白、非 JSON、JSON 错误等分支。

use crate::models::ExecutionUsage;
use crate::models::{utc_timestamp, ParsedLogEntry};
use serde_json::Value;

/// 解析一行输出：trim → 空行返回 None → JSON 解析失败返回 None。
///
/// 用于「只接受 JSON 行」的适配器（mimo / mobilecoder / opencode / codewhale / codex 等）。
/// 返回 `None` 等价于原代码里的 `serde_json::from_str::<Value>(...).ok()?`。
pub fn parse_json_line(line: &str) -> Option<Value> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str::<Value>(trimmed).ok()
}

/// trim 一行后返回原文；空行返回 None。供 `parse_stderr_line` 等场景复用。
pub fn trimmed_non_empty(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// 构造纯文本类型的 `ParsedLogEntry`（log_type="text"，无 usage/tool 字段）。
///
/// 与原代码里手写的 `ParsedLogEntry { timestamp: utc_timestamp(), log_type: "text".to_string(), content, .. }` 完全等价。
pub fn text_entry(content: impl Into<String>) -> ParsedLogEntry {
    ParsedLogEntry::new("text", content)
}

/// 构造 `info` 类型的 `ParsedLogEntry`。
pub fn info_entry(content: impl Into<String>) -> ParsedLogEntry {
    ParsedLogEntry::new("info", content)
}

/// 构造 `error` 类型的 `ParsedLogEntry`。
pub fn error_entry(content: impl Into<String>) -> ParsedLogEntry {
    ParsedLogEntry::new("error", content)
}

/// 构造 `tokens` 类型的 `ParsedLogEntry`，附带 usage 字段。
///
/// 用于 mimo / opencode / codex 等从事件流里提取 token 用量的场景。
pub fn tokens_entry(content: impl Into<String>, usage: ExecutionUsage) -> ParsedLogEntry {
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: "tokens".to_string(),
        content: content.into(),
        usage: Some(usage),
        tool_name: None,
        tool_input_json: None,
    }
}

/// 构造带 `tool_name` + `tool_input_json` 的工具调用条目（log_type 由调用方指定）。
///
/// 用于 claude_code / codebuddy / kimi / codex 等需要记录工具调用的场景。
pub fn tool_call_entry(
    log_type: &str,
    content: impl Into<String>,
    tool_name: impl Into<String>,
    tool_input_json: Option<String>,
) -> ParsedLogEntry {
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: log_type.to_string(),
        content: content.into(),
        usage: None,
        tool_name: Some(tool_name.into()),
        tool_input_json,
    }
}

/// 构造自定义 log_type + content + 可选 tool_name/tool_input_json 的条目。
///
/// 用于 `system` / `assistant` / `step_finish` / `thinking` 等带额外字段的场景。
pub fn entry_with_optional_tool(
    log_type: &str,
    content: impl Into<String>,
    tool_name: Option<String>,
    tool_input_json: Option<String>,
) -> ParsedLogEntry {
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: log_type.to_string(),
        content: content.into(),
        usage: None,
        tool_name,
        tool_input_json,
    }
}

/// 构造自定义 log_type + content + usage 的条目（无 tool 字段）。
pub fn entry_with_usage(
    log_type: &str,
    content: impl Into<String>,
    usage: Option<ExecutionUsage>,
) -> ParsedLogEntry {
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: log_type.to_string(),
        content: content.into(),
        usage,
        tool_name: None,
        tool_input_json: None,
    }
}

/// 构造自定义 log_type + content 的条目（既无 usage 也无 tool 字段）。
///
/// 用于 `system` / `assistant` / `step_start` / `step_finish` / `thinking` 等场景的最小条目。
pub fn entry(log_type: &str, content: impl Into<String>) -> ParsedLogEntry {
    ParsedLogEntry::new(log_type, content)
}

/// 替换 ParsedLogEntry 的 timestamp 字段，保留其他字段不变。
///
/// 用于 mobilecoder / mimo / opencode 这类需要从事件里提取 timestamp 字段的场景。
pub fn with_timestamp(mut entry: ParsedLogEntry, timestamp: impl Into<String>) -> ParsedLogEntry {
    entry.timestamp = timestamp.into();
    entry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_line_empty_returns_none() {
        assert!(parse_json_line("").is_none());
        assert!(parse_json_line("   ").is_none());
        assert!(parse_json_line("\t\n").is_none());
    }

    #[test]
    fn test_parse_json_line_invalid_returns_none() {
        assert!(parse_json_line("not json").is_none());
        assert!(parse_json_line("{").is_none());
    }

    #[test]
    fn test_parse_json_line_valid_object() {
        let v = parse_json_line(r#"{"type":"text"}"#).unwrap();
        assert_eq!(v["type"], "text");
    }

    #[test]
    fn test_parse_json_line_trims_whitespace() {
        let v = parse_json_line("  {\"k\":\"v\"}\n").unwrap();
        assert_eq!(v["k"], "v");
    }

    #[test]
    fn test_trimmed_non_empty_empty_returns_none() {
        assert!(trimmed_non_empty("").is_none());
        assert!(trimmed_non_empty("  \t").is_none());
    }

    #[test]
    fn test_trimmed_non_empty_returns_trimmed() {
        assert_eq!(trimmed_non_empty("  hello  "), Some("hello"));
        assert_eq!(trimmed_non_empty("hi"), Some("hi"));
    }

    #[test]
    fn test_text_entry_defaults() {
        let e = text_entry("hello");
        assert_eq!(e.log_type, "text");
        assert_eq!(e.content, "hello");
        assert!(e.usage.is_none());
        assert!(e.tool_name.is_none());
        assert!(e.tool_input_json.is_none());
        assert!(!e.timestamp.is_empty());
    }

    #[test]
    fn test_info_entry_uses_info_type() {
        let e = info_entry("status update");
        assert_eq!(e.log_type, "info");
        assert_eq!(e.content, "status update");
    }

    #[test]
    fn test_error_entry_uses_error_type() {
        let e = error_entry("bad thing");
        assert_eq!(e.log_type, "error");
        assert_eq!(e.content, "bad thing");
    }

    #[test]
    fn test_tokens_entry_attaches_usage() {
        let usage = ExecutionUsage {
            input_tokens: 10,
            output_tokens: 20,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            total_cost_usd: None,
            duration_ms: None,
        };
        let e = tokens_entry("Tokens: input=10, output=20", usage);
        assert_eq!(e.log_type, "tokens");
        assert_eq!(e.usage.unwrap().input_tokens, 10);
    }

    #[test]
    fn test_tool_call_entry_fills_tool_fields() {
        let e = tool_call_entry("tool_use", "calling bash", "bash", Some(r#"{"cmd":"ls"}"#.to_string()));
        assert_eq!(e.log_type, "tool_use");
        assert_eq!(e.tool_name.as_deref(), Some("bash"));
        assert_eq!(e.tool_input_json.as_deref(), Some(r#"{"cmd":"ls"}"#));
    }

    #[test]
    fn test_entry_with_optional_tool_none_fields() {
        let e = entry_with_optional_tool("system", "session init", None, None);
        assert_eq!(e.log_type, "system");
        assert!(e.tool_name.is_none());
        assert!(e.tool_input_json.is_none());
    }

    #[test]
    fn test_entry_with_usage_none_passes_through() {
        let e = entry_with_usage("result", "done", None);
        assert!(e.usage.is_none());
        assert_eq!(e.log_type, "result");
    }

    #[test]
    fn test_entry_basic_no_tool_no_usage() {
        let e = entry("assistant", "hi");
        assert_eq!(e.log_type, "assistant");
        assert_eq!(e.content, "hi");
        assert!(e.usage.is_none());
        assert!(e.tool_name.is_none());
    }

    #[test]
    fn test_with_timestamp_replaces_only_timestamp() {
        let original = text_entry("hello");
        let updated = with_timestamp(original, "2026-05-12T06:08:58.721Z");
        assert_eq!(updated.timestamp, "2026-05-12T06:08:58.721Z");
        assert_eq!(updated.content, "hello");
        assert_eq!(updated.log_type, "text");
    }

    #[test]
    fn test_with_timestamp_preserves_tool_fields() {
        let original = tool_call_entry("tool_use", "calling bash", "bash", Some("{}".to_string()));
        let updated = with_timestamp(original, "2026-05-12T06:08:58.721Z");
        assert_eq!(updated.tool_name.as_deref(), Some("bash"));
        assert_eq!(updated.tool_input_json.as_deref(), Some("{}"));
    }
}
