//! AtomCode 会话扫描器：scan_atomcode / get_atomcode_detail。
//! 096-W3-PR2 从 handlers/session.rs 逐字搬迁（Move Function），逻辑零改动。

use super::{home_dir, truncate_str};
use crate::handlers::session::{SessionDetail, SessionInfo, SessionMessage, SessionScanner};

fn scan_atomcode(sessions: &mut Vec<SessionInfo>) {
    let base = home_dir().join(".atomcode/sessions");
    if !base.exists() { return; }

    // sessions/<project-hash>/<session-uuid>.json
    if let Ok(project_dirs) = std::fs::read_dir(&base) {
        for project_dir in project_dirs.flatten() {
            if !project_dir.path().is_dir() { continue; }

            if let Ok(files) = std::fs::read_dir(project_dir.path()) {
                for file in files.flatten() {
                    let path = file.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }

                    let content = match std::fs::read_to_string(&path) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let v: serde_json::Value = match serde_json::from_str(&content) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let session_id = v.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                    let working_dir = v.get("working_dir").and_then(|w| w.as_str()).unwrap_or("").to_string();
                    let created_at = v.get("created_at").and_then(|t| t.as_u64());
                    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

                    let messages = v.get("messages").and_then(|m| m.as_array());
                    let msg_count = messages.map(|m| m.len()).unwrap_or(0) as u32;
                    let first_prompt = messages.and_then(|msgs| {
                        msgs.first().and_then(|m| m.get("content").and_then(|c| c.get("Text")).and_then(|t| t.as_str()))
                            .map(|t| truncate_str(t, 200))
                    });

                    let created_str = created_at.map(|ts| {
                        chrono::DateTime::from_timestamp(ts as i64, 0)
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_default()
                    });

                    sessions.push(SessionInfo {
                        session_id: if session_id.is_empty() { path.file_stem().unwrap_or_default().to_string_lossy().to_string() } else { session_id },
                        source: "atomcode".to_string(),
                        project_path: working_dir,
                        status: "completed".to_string(),
                        executor: "atomcode".to_string(),
                        model: "-".to_string(),
                        git_branch: None,
                        message_count: msg_count,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                        first_prompt,
                        created_at: created_str.clone(),
                        last_active_at: created_str,
                        file_size,
                        version: None,
                        subagent_count: 0,
                    });
                }
            }
        }
    }
}

fn get_atomcode_detail(session_id: &str) -> Option<SessionDetail> {
    let base = home_dir().join(".atomcode/sessions");
    if !base.exists() { return None; }

    if let Ok(project_dirs) = std::fs::read_dir(&base) {
        for project_dir in project_dirs.flatten().filter(|e| e.path().is_dir()) {
            if let Ok(files) = std::fs::read_dir(project_dir.path()) {
                for file in files.flatten() {
                    let path = file.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
                    let content = std::fs::read_to_string(&path).ok()?;
                    let v: serde_json::Value = serde_json::from_str(&content).ok()?;

                    let sid = v.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    if sid != session_id { continue; }

                    let working_dir = v.get("working_dir").and_then(|w| w.as_str()).unwrap_or("").to_string();
                    let created_at_ts = v.get("created_at").and_then(|t| t.as_u64());
                    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let msgs_arr = v.get("messages").and_then(|m| m.as_array());
                    let msg_count = msgs_arr.map(|m| m.len()).unwrap_or(0) as u32;

                    let created_str = created_at_ts.and_then(|ts| {
                        chrono::DateTime::from_timestamp(ts as i64, 0).map(|dt| dt.to_rfc3339())
                    });

                    let mut messages: Vec<SessionMessage> = Vec::new();
                    let mut first_prompt: Option<String> = None;

                    if let Some(msgs) = msgs_arr {
                        for msg in msgs {
                            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();
                            let text = msg.get("content").and_then(|c| c.get("Text")).and_then(|t| t.as_str()).unwrap_or("").to_string();
                            if first_prompt.is_none() && role == "User" && !text.is_empty() {
                                first_prompt = Some(truncate_str(&text, 200));
                            }
                            messages.push(SessionMessage {
                                role: if role == "User" { "user".into() } else { "assistant".into() },
                                content_preview: truncate_str(&text, 500),
                                model: None,
                                input_tokens: None,
                                output_tokens: None,
                                timestamp: created_str.clone(),
                                stop_reason: None,
                            });
                        }
                    }

                    return Some(SessionDetail {
                        info: SessionInfo {
                            session_id: session_id.to_string(),
                            source: "atomcode".to_string(),
                            project_path: working_dir,
                            status: "completed".to_string(),
                            executor: "atomcode".to_string(),
                            model: "-".to_string(),
                            git_branch: None,
                            message_count: msg_count,
                            total_input_tokens: 0,
                            total_output_tokens: 0,
                            first_prompt,
                            created_at: created_str.clone(),
                            last_active_at: created_str,
                            file_size,
                            version: None,
                            subagent_count: 0,
                        },
                        messages,
                        subagents: vec![],
                    });
                }
            }
        }
    }
    None
}

/// 扫描器的零大小标记 struct——实例仅承载类型信息，
/// I/O 与解析逻辑即本文件的 `scan_atomcode` / `get_atomcode_detail`（096-W3-PR2 收编后不再转发）。
pub struct AtomCodeScanner;

impl SessionScanner for AtomCodeScanner {
    fn name(&self) -> &'static str { "atomcode" }
    fn scan(&self, out: &mut Vec<SessionInfo>) { scan_atomcode(out); }
    fn get_detail(&self, session_id: &str) -> Option<SessionDetail> { get_atomcode_detail(session_id) }
}
