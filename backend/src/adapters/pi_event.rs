//! pi JSONL 事件类型定义
//!
//! pi 在 `--mode json` 时输出的 JSONL 事件格式

use serde::Deserialize;

/// pi JSONL 输出的事件信封，所有事件都有 type 字段
#[derive(Debug, Clone, Deserialize)]
pub struct PiEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    // message 事件
    #[serde(default)]
    pub message: Option<PiMessage>,
    // message_update 中的 assistant 事件（包含 thinking_delta / text_delta）
    #[serde(default, rename = "assistantMessageEvent")]
    pub assistant_message_event: Option<PiAssistantMessageEvent>,
    // tool_execution 事件
    #[serde(default)]
    pub tool_execution: Option<PiToolExecution>,
    // agent 事件
    #[serde(default)]
    pub agent: Option<PiAgent>,
    // turn 事件
    #[serde(default)]
    pub turn: Option<PiTurn>,
    // queue_update 事件
    #[serde(default)]
    pub queue_update: Option<PiQueueUpdate>,
    // compaction 事件
    #[serde(default)]
    pub compaction: Option<PiCompaction>,
}

/// 消息内容（message_start / message_update / message_end）
#[derive(Debug, Clone, Deserialize)]
pub struct PiMessage {
    #[serde(rename = "type")]
    pub message_type: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Vec<PiContentBlock>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    // pi 输出的 token 用量信息，位于 message 对象中
    // message_start 和 message_end 都会包含，但 message_end 才有真实值
    #[serde(default)]
    pub usage: Option<PiUsage>,
}

/// 内容块（text / toolCall / toolResult / thinking / redacted）
///
/// pi 输出 type 字段为 camelCase（如 "toolCall"），但也兼容 snake_case。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum PiContentBlock {
    #[serde(rename = "text")]
    Text { text: Option<String> },
    /// 支持 "toolCall"（pi camelCase）和 "tool_call"（snake_case）
    #[serde(rename = "toolCall", alias = "tool_call")]
    ToolCall {
        id: Option<String>,
        name: Option<String>,
        /// pi 输出字段名为 "arguments"，兼容 "input"
        #[serde(default, alias = "arguments")]
        input: serde_json::Value,
    },
    /// 支持 "toolResult"（pi camelCase）和 "tool_result"（snake_case）
    #[serde(rename = "toolResult", alias = "tool_result")]
    ToolResult {
        /// pi 输出字段名为 "toolCallId"
        #[serde(default, alias = "toolCallId")]
        tool_call_id: Option<String>,
        /// pi 输出 content 可能为字符串或数组，利用 serde_json::Value 兼容
        #[serde(default)]
        content: Option<serde_json::Value>,
        /// pi 输出字段名为 "isError"
        #[serde(default, alias = "isError")]
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: Option<String> },
    #[serde(rename = "redacted")]
    Redacted { redacted: Option<String> },
}

/// assistantMessageEvent 中的内容（message_update 时包含 thinking_delta / text_delta）
///
/// 注意：text_end 时 assistantMessageEvent 也包含 usage 字段（此时是真实值），
/// 而 text_delta / thinking_delta 阶段的 usage 为 0。
#[derive(Debug, Clone, Deserialize)]
pub struct PiAssistantMessageEvent {
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    #[serde(default)]
    pub content_index: Option<u32>,
    #[serde(default)]
    pub delta: Option<String>,
    #[serde(default)]
    pub partial: Option<PiAssistantMessagePartial>,
    #[serde(default)]
    pub model: Option<String>,
    // assistantMessageEvent 级别的 usage 字段（出现在 text_end 中）
    #[serde(default)]
    pub usage: Option<PiUsage>,
}

/// partial 内容（包含完整的 thinking 或 text）
#[derive(Debug, Clone, Deserialize)]
pub struct PiAssistantMessagePartial {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Vec<PiContentBlock>,
    #[serde(default)]
    pub model: Option<String>,
    /// text_end 中 partial 也携带 usage（与 message.usage 一致）
    #[serde(default)]
    pub usage: Option<PiUsage>,
}

/// 工具执行事件（tool_execution_start / tool_execution_update / tool_execution_end）
#[derive(Debug, Clone, Deserialize)]
pub struct PiToolExecution {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub tool_type: Option<String>,
    #[serde(default, rename = "toolCallId")]
    pub tool_call_id: Option<String>,
    #[serde(default, rename = "toolName")]
    pub tool_name: Option<String>,
    #[serde(default, rename = "args")]
    pub args: Option<serde_json::Value>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// agent 事件（agent_start / agent_end）
#[derive(Debug, Clone, Deserialize)]
pub struct PiAgent {
    pub name: Option<String>,
}

/// turn 事件（turn_start / turn_end）
#[derive(Debug, Clone, Deserialize)]
pub struct PiTurn {
    pub turn_number: Option<u32>,
}

/// queue_update 事件
#[derive(Debug, Clone, Deserialize)]
pub struct PiQueueUpdate {
    pub messages_queued: Option<u32>,
}

/// compaction 事件（compaction_start / compaction_end）
#[derive(Debug, Clone, Deserialize)]
pub struct PiCompaction {
    pub reason: Option<String>,
}

/// pi 输出的 token 用量信息
///
/// 来源于 pi JSONL 中 message 对象的 usage 字段。
/// pi 使用驼峰命名（cacheRead, cacheWrite, totalTokens）。
/// 格式示例：{"input":3705,"output":139,"cacheRead":0,"cacheWrite":0,"totalTokens":3844,"cost":{...}}
#[derive(Debug, Clone, Deserialize)]
pub struct PiUsage {
    pub input: u64,
    pub output: u64,
    #[serde(default, rename = "cacheRead")]
    pub cache_read: Option<u64>,
    #[serde(default, rename = "cacheWrite")]
    pub cache_write: Option<u64>,
    #[serde(default, rename = "totalTokens")]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub cost: Option<PiCost>,
}

/// pi 输出的费用信息，位于 usage.cost 中
/// pi 使用驼峰命名。
#[derive(Debug, Clone, Deserialize)]
pub struct PiCost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default, rename = "cacheRead")]
    pub cache_read: Option<f64>,
    #[serde(default, rename = "cacheWrite")]
    pub cache_write: Option<f64>,
    #[serde(default)]
    pub total: f64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    /// PiContentBlock ToolCall 变体：type 字段可能是 "toolCall"（camelCase，pi 实际输出）
    #[test]
    fn test_pi_content_block_toolcall_camelcase() {
        // pi 实际输出：type=toolCall, arguments=command
        let json = r#"{"type":"toolCall","id":"call_123","name":"bash","arguments":{"command":"date"}}"#;
        let result = serde_json::from_str::<PiContentBlock>(json);
        assert!(result.is_ok(), "Failed to parse toolCall: {:?}", result.err());
        if let Ok(block) = result {
            match block {
                PiContentBlock::ToolCall { id, name, input } => {
                    assert_eq!(id.as_deref(), Some("call_123"));
                    assert_eq!(name.as_deref(), Some("bash"));
                    // pi 输出用 "arguments"，反序列化后存入 input
                    assert_eq!(input.get("command").and_then(|v| v.as_str()), Some("date"));
                }
                other => panic!("Expected ToolCall, got {:?}", std::mem::discriminant(&other)),
            }
        }
    }

    /// PiContentBlock ToolCall 变体：type 是 "tool_call"（snake_case，向后兼容）
    #[test]
    fn test_pi_content_block_toolcall_snakecase() {
        let json = r#"{"type":"tool_call","id":"call_456","name":"bash","input":{"command":"ls"}}"#;
        let result = serde_json::from_str::<PiContentBlock>(json);
        assert!(result.is_ok(), "Failed to parse tool_call: {:?}", result.err());
    }

    /// PiContentBlock ToolResult 变体：pi 实际输出 camelCase 字段名
    #[test]
    fn test_pi_content_block_toolresult_camelcase() {
        // pi 实际输出：type=toolResult, toolCallId=xxx, isError=false
        let json = r#"{"type":"toolResult","toolCallId":"call_123","content":"hello","isError":false}"#;
        let result = serde_json::from_str::<PiContentBlock>(json);
        assert!(result.is_ok(), "Failed to parse toolResult: {:?}", result.err());
    }
}
