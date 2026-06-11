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

/// 工具执行事件（tool_execution_start / tool_execution_update / tool_execution_end）
#[derive(Debug, Clone, Deserialize)]
pub struct PiToolExecution {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub tool_type: Option<String>,
    pub name: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
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
