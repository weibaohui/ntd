//! Kimi 会话扫描器：scan_kimi / get_kimi_detail。
//! 096-W3-PR2 从 handlers/session.rs 逐字搬迁（Move Function），逻辑零改动。

use super::{home_dir, truncate_str};
use crate::handlers::session::{SessionDetail, SessionInfo, SessionMessage, SessionScanner};

fn scan_kimi(sessions: &mut Vec<SessionInfo>) {
    let base = home_dir().join(".kimi/sessions");
    if !base.exists() { return; }

    // sessions/<project-hash>/<session-uuid>/context.jsonl
    if let Ok(project_dirs) = std::fs::read_dir(&base) {
        for project_dir in project_dirs.flatten() {
            if !project_dir.path().is_dir() { continue; }
            let project_hash = project_dir.file_name().to_string_lossy().to_string();

            if let Ok(session_dirs) = std::fs::read_dir(project_dir.path()) {
                for session_dir in session_dirs.flatten() {
                    if !session_dir.path().is_dir() { continue; }
                    let sid = session_dir.file_name().to_string_lossy().to_string();

                    let context_path = session_dir.path().join("context.jsonl");
                    let state_path = session_dir.path().join("state.json");

                    if !context_path.exists() { continue; }

                    // Get total file size of session directory
                    let file_size = std::fs::metadata(&context_path).map(|m| m.len()).unwrap_or(0);
                    let content = match std::fs::read_to_string(&context_path) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };

                    // Parse state.json for title
                    let state: Option<serde_json::Value> = std::fs::read_to_string(&state_path)
                        .ok()
                        .and_then(|s| serde_json::from_str(&s).ok());
                    let title = state.as_ref()
                        .and_then(|s| s.get("custom_title"))
                        .and_then(|t| t.as_str())
                        .map(String::from);

                    let mut first_ts: Option<String> = None;
                    let mut last_ts: Option<String> = None;
                    let mut model: Option<String> = None;
                    let mut first_prompt: Option<String> = None;
                    let mut msg_count: u32 = 0;

                    for line in content.lines() {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                            let role = v.get("role").and_then(|r| r.as_str()).unwrap_or("");

                            if role == "user" {
                                msg_count += 1;
                                let text = v.get("content").and_then(|c| c.as_str()).unwrap_or("");
                                if first_prompt.is_none() && !text.is_empty() {
                                    first_prompt = Some(truncate_str(text, 200));
                                }
                            } else if role == "assistant" {
                                msg_count += 1;
                                if model.is_none() {
                                    model = v.get("model").and_then(|m| m.as_str()).map(String::from);
                                }
                            }

                            if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
                                if first_ts.is_none() { first_ts = Some(ts.to_string()); }
                                last_ts = Some(ts.to_string());
                            }
                        }
                    }

                    sessions.push(SessionInfo {
                        session_id: sid,
                        source: "kimi".to_string(),
                        project_path: project_hash.clone(),
                        status: "completed".to_string(),
                        executor: "kimi".to_string(),
                        model: model.unwrap_or_else(|| "-".into()),
                        git_branch: None,
                        message_count: msg_count,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                        first_prompt: first_prompt.or(title),
                        created_at: first_ts,
                        last_active_at: last_ts,
                        file_size,
                        version: None,
                        subagent_count: 0,
                    });
                }
            }
        }
    }
}

fn get_kimi_detail(session_id: &str) -> Option<SessionDetail> {
    let base = home_dir().join(".kimi/sessions");
    if !base.exists() { return None; }

    if let Ok(project_dirs) = std::fs::read_dir(&base) {
        for project_dir in project_dirs.flatten().filter(|e| e.path().is_dir()) {
            let session_dir = project_dir.path().join(session_id);
            let context_path = session_dir.join("context.jsonl");
            if !context_path.exists() { continue; }

            let project_hash = project_dir.file_name().to_string_lossy().to_string();
            let content = std::fs::read_to_string(&context_path).ok()?;
            let file_size = std::fs::metadata(&context_path).map(|m| m.len()).unwrap_or(0);

            let state: Option<serde_json::Value> = std::fs::read_to_string(session_dir.join("state.json"))
                .ok().and_then(|s| serde_json::from_str(&s).ok());
            let title = state.as_ref().and_then(|s| s.get("custom_title")).and_then(|t| t.as_str()).map(String::from);

            let mut first_ts: Option<String> = None;
            let mut last_ts: Option<String> = None;
            let mut model: Option<String> = None;
            let mut first_prompt: Option<String> = None;
            let mut msg_count: u32 = 0;
            let mut messages: Vec<SessionMessage> = Vec::new();

            for line in content.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    let role = v.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();
                    if role == "user" || role == "assistant" {
                        msg_count += 1;
                        let text = v.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                        if first_prompt.is_none() && role == "user" && !text.is_empty() {
                            first_prompt = Some(truncate_str(&text, 200));
                        }
                        if model.is_none() && role == "assistant" {
                            model = v.get("model").and_then(|m| m.as_str()).map(String::from);
                        }
                        messages.push(SessionMessage {
                            role: role.clone(),
                            content_preview: truncate_str(&text, 500),
                            model: if role == "assistant" { model.clone() } else { None },
                            input_tokens: None,
                            output_tokens: None,
                            timestamp: v.get("timestamp").and_then(|t| t.as_str()).map(String::from),
                            stop_reason: None,
                        });
                    }
                    if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
                        if first_ts.is_none() { first_ts = Some(ts.to_string()); }
                        last_ts = Some(ts.to_string());
                    }
                }
            }

            return Some(SessionDetail {
                info: SessionInfo {
                    session_id: session_id.to_string(),
                    source: "kimi".to_string(),
                    project_path: project_hash,
                    status: "completed".to_string(),
                    executor: "kimi".to_string(),
                    model: model.unwrap_or_else(|| "-".into()),
                    git_branch: None,
                    message_count: msg_count,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    first_prompt: first_prompt.or(title),
                    created_at: first_ts,
                    last_active_at: last_ts,
                    file_size,
                    version: None,
                    subagent_count: 0,
                },
                messages,
                subagents: vec![],
            });
        }
    }
    None
}

/// 扫描器的零大小标记 struct——实例仅承载类型信息，
/// I/O 与解析逻辑即本文件的 `scan_kimi` / `get_kimi_detail`（096-W3-PR2 收编后不再转发）。
pub struct KimiScanner;

impl SessionScanner for KimiScanner {
    fn name(&self) -> &'static str { "kimi" }
    fn scan(&self, out: &mut Vec<SessionInfo>) { scan_kimi(out); }
    fn get_detail(&self, session_id: &str) -> Option<SessionDetail> { get_kimi_detail(session_id) }
}
