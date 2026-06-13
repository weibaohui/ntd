//! MiMo-specific event parsing.
//!
//! MiMo uses the same event structure as OpenCode (compatible with Anthropic SDK protocol),
//! with underscore-separated type names (e.g., step_start, tool_use).

use std::collections::HashMap;
use serde::Deserialize;

/// MiMo agent event with underscore-separated type names (same as OpenCode)
#[derive(Debug, Clone, Deserialize)]
pub struct MimoEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub timestamp: Option<u64>,
    #[serde(default, rename = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub part: Option<MimoPart>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MimoPart {
    #[serde(rename = "type")]
    pub part_type: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default, rename = "callID")]
    pub call_id: Option<String>,
    #[serde(default)]
    pub state: Option<MimoToolState>,
    #[serde(default, rename = "messageID")]
    pub message_id: Option<String>,
    #[serde(default, rename = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tokens: Option<MimoTokens>,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MimoToolState {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub input: Option<MimoToolInput>,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MimoToolInput {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl MimoToolInput {
    /// 序列化为完整 JSON 字符串。
    /// 结构化字段（command/description）优先级最高；extra 中与它们同名的键会被跳过，
    /// 避免 extra 覆盖核心语义。
    pub fn to_full_json(&self) -> String {
        let mut map = serde_json::Map::new();
        if let Some(ref cmd) = self.command {
            map.insert("command".into(), serde_json::Value::String(cmd.clone()));
        }
        if let Some(ref desc) = self.description {
            map.insert("description".into(), serde_json::Value::String(desc.clone()));
        }
        // extra 只补充结构化字段中不存在的键，防止同名键覆盖语义
        for (k, v) in &self.extra {
            if k == "command" || k == "description" {
                continue;
            }
            map.insert(k.clone(), v.clone());
        }
        serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MimoTokens {
    pub total: u64,
    pub input: u64,
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub cache: MimoCacheTokens,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MimoCacheTokens {
    #[serde(default)]
    pub read: u64,
    #[serde(default)]
    pub write: u64,
}
