//! 数据库适配：事件 → execution_logs 映射
//!
//! 将 ExecutionEvent 转换为数据库兼容的格式。

use super::event::ExecutionEvent;
use crate::models::ExecutionUsage;

/// 数据库日志条目
///
/// 对应 execution_logs 表的结构。
#[derive(Debug, Clone)]
pub struct DbLogEntry {
    pub timestamp: String,
    pub log_type: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_input_json: Option<String>,
    pub usage: Option<ExecutionUsage>,
}

impl DbLogEntry {
    /// 从 ExecutionEvent 创建数据库日志条目
    pub fn from_event(event: &ExecutionEvent) -> Self {
        let timestamp = crate::models::utc_timestamp();

        match event {
            ExecutionEvent::ToolCall { id: _, name, input } => Self {
                timestamp,
                log_type: "tool_call".to_string(),
                content: name.clone(),
                tool_name: Some(name.clone()),
                tool_input_json: Some(input.to_string()),
                usage: None,
            },
            ExecutionEvent::ToolResult { call_id: _, output, is_error: _ } => Self {
                timestamp,
                log_type: "tool_result".to_string(),
                content: output.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::Thinking { content } => Self {
                timestamp,
                log_type: "thinking".to_string(),
                content: content.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::Result { summary } => Self {
                timestamp,
                log_type: "result".to_string(),
                content: summary.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::Assistant { content, thinking, message_id: _ } => Self {
                timestamp,
                log_type: "assistant".to_string(),
                content: if let Some(t) = thinking {
                    format!("{}\n\n[思考过程]\n{}", content, t)
                } else {
                    content.clone()
                },
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::User { content } => Self {
                timestamp,
                log_type: "user".to_string(),
                content: content.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::System { message } => Self {
                timestamp,
                log_type: "system".to_string(),
                content: message.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::Info { message } => Self {
                timestamp,
                log_type: "info".to_string(),
                content: message.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::Error { message } => Self {
                timestamp,
                log_type: "error".to_string(),
                content: message.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::StepStart { name, index } => Self {
                timestamp,
                log_type: "step_start".to_string(),
                content: format!("步骤 {}: {}", index, name),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::StepFinish { name, index } => Self {
                timestamp,
                log_type: "step_finish".to_string(),
                content: format!("完成步骤 {}: {}", index, name),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::Tokens {
                input,
                output,
                cache_read,
                cache_write,
            } => {
                let mut parts = vec![
                    format!("input_tokens: {}", input),
                    format!("output_tokens: {}", output),
                ];
                if let Some(cr) = cache_read {
                    parts.push(format!("cache_read_input_tokens: {}", cr));
                }
                if let Some(cw) = cache_write {
                    parts.push(format!("cache_creation_input_tokens: {}", cw));
                }
                Self {
                    timestamp,
                    log_type: "tokens".to_string(),
                    content: parts.join(", "),
                    tool_name: None,
                    tool_input_json: None,
                    usage: Some(ExecutionUsage {
                        input_tokens: *input,
                        output_tokens: *output,
                        cache_read_input_tokens: *cache_read,
                        cache_creation_input_tokens: *cache_write,
                        total_cost_usd: None,
                        duration_ms: None,
                    }),
                }
            }
            ExecutionEvent::SessionStart { session_id } => Self {
                timestamp,
                log_type: "session_start".to_string(),
                content: format!("session_id: {}", session_id),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::SessionEnd { session_id } => Self {
                timestamp,
                log_type: "session_end".to_string(),
                content: format!("session_id: {}", session_id),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::ModelSwitch { model } => Self {
                timestamp,
                log_type: "model_switch".to_string(),
                content: format!("model: {}", model),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::Cost { cost_usd } => Self {
                timestamp,
                log_type: "cost".to_string(),
                content: format!("total_cost_usd: {}", cost_usd),
                tool_name: None,
                tool_input_json: None,
                usage: Some(ExecutionUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    total_cost_usd: Some(*cost_usd),
                    duration_ms: None,
                }),
            },
            ExecutionEvent::Duration { duration_ms } => Self {
                timestamp,
                log_type: "duration".to_string(),
                content: format!("duration_ms: {}", duration_ms),
                tool_name: None,
                tool_input_json: None,
                usage: Some(ExecutionUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    total_cost_usd: None,
                    duration_ms: Some(*duration_ms),
                }),
            },
            ExecutionEvent::Progress { percent, message } => Self {
                timestamp,
                log_type: "progress".to_string(),
                content: if let Some(msg) = message {
                    format!("{}%: {}", percent, msg)
                } else {
                    format!("{}%", percent)
                },
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
        }
    }

    /// 转换为用于插入数据库的 JSON 字符串
    pub fn to_json(&self) -> String {
        let metadata = serde_json::json!({
            "tool_name": self.tool_name,
            "tool_input_json": self.tool_input_json,
        });
        serde_json::json!({
            "timestamp": self.timestamp,
            "type": self.log_type,
            "content": self.content,
            "metadata": metadata,
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call_to_db() {
        let event = ExecutionEvent::tool_call("123", "Bash", serde_json::json!({"command": "ls"}));
        let db_entry = DbLogEntry::from_event(&event);

        assert_eq!(db_entry.log_type, "tool_call");
        assert_eq!(db_entry.content, "Bash");
        assert_eq!(db_entry.tool_name, Some("Bash".to_string()));
    }

    #[test]
    fn test_tokens_to_db() {
        let event = ExecutionEvent::Tokens {
            input: 100,
            output: 200,
            cache_read: Some(50),
            cache_write: Some(10),
        };
        let db_entry = DbLogEntry::from_event(&event);

        assert_eq!(db_entry.log_type, "tokens");
        assert!(db_entry.usage.is_some());
        let usage = db_entry.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 200);
    }
}
