//! pi 执行器 adapter
//!
//! 支持 --continue 恢复会话、--session 指定 session-id、--mode json JSONL 输出

use std::sync::Arc;
use parking_lot::Mutex;

use super::{CodeExecutor, ExecutorType, ParsedLogEntry, ExecutionUsage};
use super::pi_event::PiEvent;
use crate::models::utc_timestamp;

pub struct PiExecutor {
    path: String,
    model: Arc<Mutex<Option<String>>>,
    /// 从 session 事件中提取的 session id
    session_id: Arc<Mutex<Option<String>>>,
}

impl PiExecutor {
    pub fn new(path: String) -> Self {
        Self {
            path,
            model: Arc::new(Mutex::new(None)),
            session_id: Arc::new(Mutex::new(None)),
        }
    }
}

impl Clone for PiExecutor {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            model: self.model.clone(),
            session_id: self.session_id.clone(),
        }
    }
}

impl CodeExecutor for PiExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Pi
    }

    fn executable_path(&self) -> &str {
        &self.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "-p".to_string(),
            "--mode".to_string(),
            "json".to_string(),
            message.to_string(),
        ]
    }

    /// 支持 session 的命令参数
    /// pi 使用 --continue 恢复，--session <id> 指定已有 session
    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "-p".to_string(),
            "--mode".to_string(),
            "json".to_string(),
        ];
        if let Some(sid) = session_id {
            if is_resume {
                // 恢复模式：继续之前的 session
                args.push("--continue".to_string());
                args.push(format!("--session {}", sid));
            } else {
                // 新 session 但指定 id（目前 pi 不支持创建指定 id 的 session，
                // 只能用 --continue 恢复，这里 fallback 到 --continue）
                args.push("--continue".to_string());
                args.push(format!("--session {}", sid));
            }
        }
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    /// 从 session 事件中提取 session id
    fn extract_session_id(&self, line: &str) -> Option<String> {
        if line.is_empty() {
            return None;
        }
        if let Ok(event) = serde_json::from_str::<PiEvent>(line) {
            if event.event_type == "session" {
                if let Some(id) = event.id {
                    *self.session_id.lock() = Some(id.clone());
                    return Some(id);
                }
            }
        }
        None
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        if line.is_empty() {
            return None;
        }

        // 尝试解析为 pi JSONL 事件
        if let Ok(event) = serde_json::from_str::<PiEvent>(line) {
            return match event.event_type.as_str() {
                "session" => {
                    // Session 初始化事件
                    if let Some(id) = &event.id {
                        *self.session_id.lock() = Some(id.clone());
                    }
                    Some(ParsedLogEntry {
                        timestamp: utc_timestamp(),
                        log_type: "system".to_string(),
                        content: format!("Session: {:?}", event.id),
                        usage: None,
                        tool_name: None,
                        tool_input_json: None,
                    })
                }
                "message_start" | "message_update" | "message_end" => {
                    // 解析消息内容
                    if let Some(msg) = event.message {
                        for block in msg.content {
                            match block {
                                super::pi_event::PiContentBlock::Text { text } => {
                                    if let Some(t) = text {
                                        let trimmed = t.trim();
                                        if !trimmed.is_empty() {
                                            return Some(ParsedLogEntry {
                                                timestamp: utc_timestamp(),
                                                log_type: "assistant".to_string(),
                                                content: trimmed.to_string(),
                                                usage: None,
                                                tool_name: None,
                                                tool_input_json: None,
                                            });
                                        }
                                    }
                                }
                                super::pi_event::PiContentBlock::ToolCall { id: _, name, input } => {
                                    let name_str = name.unwrap_or_else(|| "unknown".to_string());
                                    let input_str = serde_json::to_string(&input).unwrap_or_default();
                                    return Some(ParsedLogEntry {
                                        timestamp: utc_timestamp(),
                                        log_type: "tool_use".to_string(),
                                        content: format!("调用工具: {} - {}", name_str, input_str.chars().take(300).collect::<String>()),
                                        usage: None,
                                        tool_name: Some(name_str),
                                        tool_input_json: Some(input_str),
                                    });
                                }
                                super::pi_event::PiContentBlock::ToolResult { tool_call_id: _, content, is_error } => {
                                    let err_str = if is_error.unwrap_or(false) { "[错误] " } else { "" };
                                    return Some(ParsedLogEntry {
                                        timestamp: utc_timestamp(),
                                        log_type: "tool_result".to_string(),
                                        content: format!("{}{}", err_str, content.unwrap_or_default().chars().take(300).collect::<String>()),
                                        usage: None,
                                        tool_name: None,
                                        tool_input_json: None,
                                    });
                                }
                                super::pi_event::PiContentBlock::Thinking { thinking } => {
                                    if let Some(t) = thinking {
                                        let trimmed = t.trim();
                                        if !trimmed.is_empty() {
                                            return Some(ParsedLogEntry {
                                                timestamp: utc_timestamp(),
                                                log_type: "thinking".to_string(),
                                                content: trimmed.chars().take(500).collect(),
                                                usage: None,
                                                tool_name: None,
                                                tool_input_json: None,
                                            });
                                        }
                                    }
                                }
                                super::pi_event::PiContentBlock::Redacted { redacted } => {
                                    return Some(ParsedLogEntry {
                                        timestamp: utc_timestamp(),
                                        log_type: "assistant".to_string(),
                                        content: format!("[redacted] {}", redacted.unwrap_or_default()),
                                        usage: None,
                                        tool_name: None,
                                        tool_input_json: None,
                                    });
                                }
                            }
                        }
                    }
                    None
                }
                "tool_execution_start" => {
                    if let Some(te) = event.tool_execution {
                        let name = te.name.unwrap_or_else(|| "unknown".to_string());
                        let input_str = te.input.as_ref().map(|i| serde_json::to_string(i).unwrap_or_default()).unwrap_or_default();
                        return Some(ParsedLogEntry {
                            timestamp: utc_timestamp(),
                            log_type: "tool_use".to_string(),
                            content: format!("开始工具: {}", name),
                            usage: None,
                            tool_name: Some(name),
                            tool_input_json: Some(input_str),
                        });
                    }
                    None
                }
                "tool_execution_end" => {
                    if let Some(te) = event.tool_execution {
                        let name = te.name.unwrap_or_else(|| "unknown".to_string());
                        let output = te.output.unwrap_or_default();
                        return Some(ParsedLogEntry {
                            timestamp: utc_timestamp(),
                            log_type: "tool_result".to_string(),
                            content: format!("{}: {}", name, output.chars().take(300).collect::<String>()),
                            usage: None,
                            tool_name: Some(name),
                            tool_input_json: None,
                        });
                    }
                    None
                }
                "agent_start" => Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "system".to_string(),
                    content: "Agent started".to_string(),
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                }),
                "agent_end" => Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "system".to_string(),
                    content: "Agent finished".to_string(),
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                }),
                "compaction_start" => Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "system".to_string(),
                    content: "Compacting session...".to_string(),
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                }),
                "compaction_end" => Some(ParsedLogEntry {
                    timestamp: utc_timestamp(),
                    log_type: "system".to_string(),
                    content: "Compaction finished".to_string(),
                    usage: None,
                    tool_name: None,
                    tool_input_json: None,
                }),
                // 忽略其他事件类型
                _ => None,
            };
        }

        // 非 JSON 行当作普通文本处理
        Some(ParsedLogEntry {
            timestamp: utc_timestamp(),
            log_type: "text".to_string(),
            content: line.to_string(),
            usage: None,
            tool_name: None,
            tool_input_json: None,
        })
    }

    fn check_success(&self, exit_code: i32) -> bool {
        exit_code == 0
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        // 收集所有 assistant 类型的文本
        let texts: Vec<String> = logs.iter()
            .filter(|l| l.log_type == "assistant")
            .map(|l| l.content.clone())
            .filter(|t| !t.trim().is_empty())
            .collect();

        if !texts.is_empty() {
            Some(texts.join("\n\n"))
        } else {
            // fallback 到最后一条文本
            logs.iter()
                .rev()
                .find(|l| l.log_type == "text")
                .map(|l| l.content.clone())
        }
    }

    fn get_usage(&self, _logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> {
        // pi 目前不在 JSONL 中输出 usage 信息
        None
    }

    fn get_model(&self) -> Option<String> {
        self.model.lock().clone()
    }
}
