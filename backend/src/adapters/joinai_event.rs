//! JoinAI-specific event parsing.
//!
//! JoinAI uses underscore-separated event types (e.g., step_start, tool_use) and
//! follows a different naming convention.

use std::collections::HashMap;
use serde::Deserialize;

/// Flexible timestamp that can be deserialized from both numbers and strings.
#[derive(Debug, Clone)]
pub struct JoinAITimestamp(pub String);

impl<'de> Deserialize<'de> for JoinAITimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct JoinAITimestampVisitor;
        impl<'de> serde::de::Visitor<'de> for JoinAITimestampVisitor {
            type Value = JoinAITimestamp;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number (u64) or a string")
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<JoinAITimestamp, E> {
                Ok(JoinAITimestamp(v.to_string()))
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<JoinAITimestamp, E> {
                Ok(JoinAITimestamp(v.to_string()))
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<JoinAITimestamp, E> {
                Ok(JoinAITimestamp(v.to_string()))
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<JoinAITimestamp, E> {
                Ok(JoinAITimestamp(v))
            }
        }
        deserializer.deserialize_any(JoinAITimestampVisitor)
    }
}

/// JoinAI agent event with underscore-separated type names
#[derive(Debug, Clone, Deserialize)]
pub struct JoinaiAgentEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub timestamp: Option<JoinAITimestamp>,
    #[serde(default, rename = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub part: Option<JoinaiAgentPart>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JoinaiAgentPart {
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
    pub state: Option<JoinaiAgentToolState>,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default, rename = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tokens: Option<JoinaiAgentTokens>,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JoinaiAgentToolState {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub input: Option<JoinaiAgentToolInput>,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JoinaiAgentToolInput {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl JoinaiAgentToolInput {
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
pub struct JoinaiAgentTokens {
    pub total: u64,
    pub input: u64,
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub cache: JoinaiAgentCacheTokens,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct JoinaiAgentCacheTokens {
    #[serde(default)]
    pub read: u64,
    #[serde(default)]
    pub write: u64,
}
