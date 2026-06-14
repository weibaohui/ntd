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

/// 内容块（text / tool_call / tool_result / thinking）
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum PiContentBlock {
    #[serde(rename = "text")]
    Text { text: Option<String> },
    #[serde(rename = "tool_call")]
    ToolCall { id: Option<String>, name: Option<String>, input: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult { tool_call_id: Option<String>, content: Option<String>, is_error: Option<bool> },
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
