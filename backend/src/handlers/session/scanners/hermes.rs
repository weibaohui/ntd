//! Hermes 会话扫描器：scan_hermes / get_hermes_detail。
//! 096-W3-PR2 从 handlers/session.rs 逐字搬迁（Move Function），逻辑零改动。

use super::{home_dir, iter_jsonl_files, truncate_str};
use crate::handlers::session::{SessionDetail, SessionInfo, SessionMessage, SessionScanner};

fn scan_hermes(sessions: &mut Vec<SessionInfo>) {
    let dir = home_dir().join(".hermes/sessions");
    if !dir.exists() { return; }

    // hermes 直接把所有 *.jsonl 平铺在 sessions 目录,无 project 层级,
    // 是 iter_jsonl_files 抽象最贴合的场景——`read_dir` + 扩展名守卫
    // 全部由 helper 承担,这里只关心解析逻辑。
    for (path, name) in iter_jsonl_files(&dir) {
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut first_ts: Option<String> = None;
        let mut last_ts: Option<String> = None;
        let mut model: Option<String> = None;
        let mut first_prompt: Option<String> = None;
        let mut msg_count: u32 = 0;
        let mut project_path = String::new();

        for line in content.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                let role = v.get("role").and_then(|r| r.as_str()).unwrap_or("");

                if role == "session_meta" {
                    first_ts = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
                } else if role == "user" {
                    msg_count += 1;
                    let text = v.get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("");
                    if first_prompt.is_none() && !text.is_empty() {
                        first_prompt = Some(truncate_str(text, 200));
                    }
                    // Try to get cwd from tool calls or context
                    if project_path.is_empty() {
                        if let Some(tool_calls) = v.get("tool_calls") {
                            for tc in tool_calls.as_array().unwrap_or(&vec![]) {
                                if let Some(inp) = tc.get("function").and_then(|f| f.get("arguments")) {
                                    if let Some(cwd) = inp.get("cwd").and_then(|c| c.as_str()) {
                                        project_path = cwd.to_string();
                                    }
                                }
                            }
                        }
                    }
                } else if role == "assistant" {
                    msg_count += 1;
                    if model.is_none() {
                        model = v.get("model").and_then(|m| m.as_str()).map(String::from);
                    }
                }

                if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
                    last_ts = Some(ts.to_string());
                }
            }
        }

        let session_id = name.trim_end_matches(".jsonl").to_string();

        sessions.push(SessionInfo {
            session_id,
            source: "hermes".to_string(),
            project_path,
            status: "completed".to_string(),
            executor: "hermes".to_string(),
            model: model.unwrap_or_else(|| "-".into()),
            git_branch: None,
            message_count: msg_count,
            total_input_tokens: 0,
            total_output_tokens: 0,
            first_prompt,
            created_at: first_ts,
            last_active_at: last_ts,
            file_size,
            version: None,
            subagent_count: 0,
        });
    }
}

fn get_hermes_detail(session_id: &str) -> Option<SessionDetail> {
    let path = home_dir().join(".hermes/sessions").join(format!("{}.jsonl", session_id));
    if !path.exists() { return None; }

    let content = std::fs::read_to_string(&path).ok()?;
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;
    let mut model: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    let mut msg_count: u32 = 0;
    let mut messages: Vec<SessionMessage> = Vec::new();
    let project_path = String::new();

    for line in content.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let role = v.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();
            if role == "session_meta" {
                first_ts = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
            } else if role == "user" || role == "assistant" {
                msg_count += 1;
                let text = v.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                if first_prompt.is_none() && role == "user" && !text.is_empty() {
                    first_prompt = Some(truncate_str(&text, 200));
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
                last_ts = Some(ts.to_string());
            }
            if model.is_none() && role == "assistant" {
                model = v.get("model").and_then(|m| m.as_str()).map(String::from);
            }
        }
    }

    Some(SessionDetail {
        info: SessionInfo {
            session_id: session_id.to_string(),
            source: "hermes".to_string(),
            project_path,
            status: "completed".to_string(),
            executor: "hermes".to_string(),
            model: model.unwrap_or_else(|| "-".into()),
            git_branch: None,
            message_count: msg_count,
            total_input_tokens: 0,
            total_output_tokens: 0,
            first_prompt,
            created_at: first_ts,
            last_active_at: last_ts,
            file_size,
            version: None,
            subagent_count: 0,
        },
        messages,
        subagents: vec![],
    })
}

/// 扫描器的零大小标记 struct——实例仅承载类型信息，
/// I/O 与解析逻辑即本文件的 `scan_hermes` / `get_hermes_detail`（096-W3-PR2 收编后不再转发）。
pub struct HermesScanner;

impl SessionScanner for HermesScanner {
    fn name(&self) -> &'static str { "hermes" }
    fn scan(&self, out: &mut Vec<SessionInfo>) { scan_hermes(out); }
    fn get_detail(&self, session_id: &str) -> Option<SessionDetail> { get_hermes_detail(session_id) }
}
