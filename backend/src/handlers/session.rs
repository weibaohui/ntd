use axum::{
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::{AppError, AppState};
use crate::models::ApiResponse;

// ─── Request / Response types ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    pub page: Option<u64>,
    pub page_size: Option<u64>,
    pub status: Option<String>,    // "active" | "completed"
    pub source: Option<String>,    // filter by tool source: "claude-code", "codex", "hermes", etc.
    pub executor: Option<String>,  // filter by entrypoint
    pub project: Option<String>,   // filter by project path (partial match)
    pub search: Option<String>,    // search in first prompt
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub source: String,            // "claude-code" | "codex" | "hermes" | "kimi" | "atomcode" | "cc-connect"
    pub project_path: String,
    pub status: String,
    pub executor: String,
    pub model: String,
    pub git_branch: Option<String>,
    pub message_count: u32,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub first_prompt: Option<String>,
    pub created_at: Option<String>,
    pub last_active_at: Option<String>,
    pub file_size: u64,
    pub version: Option<String>,
    pub subagent_count: u32,
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionInfo>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

#[derive(Debug, Serialize)]
pub struct SessionStats {
    pub total_sessions: u64,
    pub active_sessions: u64,
    pub today_sessions: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub by_source: HashMap<String, u64>,
    pub by_executor: HashMap<String, u64>,
    pub by_project: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMessage {
    pub role: String,
    pub content_preview: String,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub timestamp: Option<String>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubAgentInfo {
    pub agent_type: String,
    pub description: String,
    pub message_count: u32,
}

#[derive(Debug, Serialize)]
pub struct SessionDetail {
    pub info: SessionInfo,
    pub messages: Vec<SessionMessage>,
    pub subagents: Vec<SubAgentInfo>,
}

// ─── Helpers ──────────────────────────────────────────────

/// Parsed metadata from a Claude Code session log line.
pub type ClaudeLineMeta = (
    Option<String>, Option<String>, Option<String>, Option<String>,
    Option<String>, Option<String>, Option<u64>, Option<u64>, String,
);

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("no home directory")
}

/// Truncate a string to at most `max_len` chars, appending "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{}...", truncated)
    }
}

/// Extract text content from a JSON value that may be a string or array of content blocks.
fn extract_text_content(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            let mut texts = Vec::new();
            for block in blocks {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    texts.push(text.to_string());
                }
            }
            texts.join("\n")
        }
        _ => String::new(),
    }
}

// ─── Claude Code Scanner ──────────────────────────────────

fn decode_project_path(encoded: &str) -> String {
    let s = encoded.strip_prefix('-').unwrap_or(encoded);
    format!("/{}", s.replace('-', "/"))
}

fn parse_claude_line_metadata(
    line: &str,
) -> Option<ClaudeLineMeta> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let msg_type = v.get("type")?.as_str()?;
    match msg_type {
        "user" => {
            let ts = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
            let branch = v.get("gitBranch").and_then(|b| b.as_str()).map(String::from);
            let ver = v.get("version").and_then(|v| v.as_str()).map(String::from);
            let entry = v.get("entrypoint").and_then(|e| e.as_str()).map(String::from);
            let content = v.get("message").and_then(|m| m.get("content")).map(extract_text_content);
            Some((ts, None, branch, ver, entry, content, None, None, "user".into()))
        }
        "assistant" => {
            let ts = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
            let msg = v.get("message")?;
            let model = msg.get("model").and_then(|m| m.as_str()).map(String::from);
            let usage = msg.get("usage");
            let input_tokens = usage.and_then(|u| u.get("input_tokens")).and_then(|t| t.as_u64());
            let output_tokens = usage.and_then(|u| u.get("output_tokens")).and_then(|t| t.as_u64());
            Some((ts, model, None, None, None, None, input_tokens, output_tokens, "assistant".into()))
        }
        "queue-operation" if v.get("operation").and_then(|o| o.as_str()) == Some("enqueue") => {
            let ts = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
            let content = v.get("content").and_then(|c| c.as_str()).map(String::from);
            Some((ts, None, None, None, None, content, None, None, "queue".into()))
        }
        _ => None,
    }
}

fn collect_claude_active_sessions() -> std::collections::HashSet<String> {
    let dir = home_dir().join(".claude/sessions");
    let mut active = std::collections::HashSet::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(sid) = v.get("sessionId").and_then(|s| s.as_str()) {
                        active.insert(sid.to_string());
                    }
                }
            }
        }
    }
    active
}

fn scan_claude_code(sessions: &mut Vec<SessionInfo>) {
    let active_set = collect_claude_active_sessions();
    let projects_dir = home_dir().join(".claude/projects");
    if !projects_dir.exists() { return; }

    if let Ok(project_entries) = std::fs::read_dir(&projects_dir) {
        for project_entry in project_entries.flatten() {
            let project_name = project_entry.file_name().to_string_lossy().to_string();
            if !project_entry.path().is_dir() || project_name.starts_with('.') || project_name == "memory" {
                continue;
            }
            let project_path = decode_project_path(&project_name);

            if let Ok(session_entries) = std::fs::read_dir(project_entry.path()) {
                for session_entry in session_entries.flatten() {
                    let path = session_entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }

                    let session_id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let file_content = std::fs::read_to_string(&path).unwrap_or_default();

                    let mut first_ts: Option<String> = None;
                    let mut last_ts: Option<String> = None;
                    let mut model: Option<String> = None;
                    let mut git_branch: Option<String> = None;
                    let mut version: Option<String> = None;
                    let mut executor: Option<String> = None;
                    let mut first_prompt: Option<String> = None;
                    let mut msg_count: u32 = 0;
                    let mut total_in: u64 = 0;
                    let mut total_out: u64 = 0;

                    for line in file_content.lines() {
                        if let Some((ts, mdl, branch, ver, entry, prompt, inp, out, role)) =
                            parse_claude_line_metadata(line)
                        {
                            if first_ts.is_none() { first_ts = ts.clone(); }
                            if ts.is_some() { last_ts = ts; }
                            if mdl.is_some() { model = mdl; }
                            if branch.is_some() { git_branch = branch; }
                            if ver.is_some() { version = ver; }
                            if entry.is_some() { executor = entry; }
                            if first_prompt.is_none() && prompt.is_some() { first_prompt = prompt; }
                            if role == "user" || role == "assistant" { msg_count += 1; }
                            if let Some(i) = inp { total_in += i; }
                            if let Some(o) = out { total_out += o; }
                        }
                    }

                    sessions.push(SessionInfo {
                        session_id: session_id.clone(),
                        source: "claudecode".to_string(),
                        project_path: project_path.clone(),
                        status: if active_set.contains(&session_id) { "active".into() } else { "completed".into() },
                        executor: executor.unwrap_or_else(|| "unknown".into()),
                        model: model.unwrap_or_else(|| "-".into()),
                        git_branch,
                        message_count: msg_count,
                        total_input_tokens: total_in,
                        total_output_tokens: total_out,
                        first_prompt: first_prompt.map(|p| truncate_str(&p, 200)),
                        created_at: first_ts,
                        last_active_at: last_ts,
                        file_size,
                        version,
                        subagent_count: 0,
                    });
                }
            }
        }
    }
}

// ─── Codex CLI Scanner ────────────────────────────────────

fn scan_codex(sessions: &mut Vec<SessionInfo>) {
    let base = home_dir().join(".codex/sessions");
    if !base.exists() { return; }

    // Walk date-organized directories: sessions/YYYY/MM/DD/rollout-*.jsonl
    if let Ok(years) = std::fs::read_dir(&base) {
        for year_entry in years.flatten() {
            if !year_entry.path().is_dir() { continue; }
            if let Ok(months) = std::fs::read_dir(year_entry.path()) {
                for month_entry in months.flatten() {
                    if !month_entry.path().is_dir() { continue; }
                    if let Ok(days) = std::fs::read_dir(month_entry.path()) {
                        for day_entry in days.flatten() {
                            let day_path = day_entry.path();
                            if !day_path.is_dir() { continue; }
                            // Iterate files inside the day directory
                            if let Ok(files) = std::fs::read_dir(&day_path) {
                                for file_entry in files.flatten() {
                                    let path = file_entry.path();
                                    if !path.is_file() { continue; }
                                    let name = file_entry.file_name().to_string_lossy().to_string();
                                    if !name.starts_with("rollout-") || !name.ends_with(".jsonl") { continue; }

                            let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            let content = match std::fs::read_to_string(&path) {
                                Ok(c) => c,
                                Err(_) => continue,
                            };

                            let mut session_id = String::new();
                            let mut project_path = String::new();
                            let mut version = String::new();
                            let mut model = String::new();
                            let mut first_ts: Option<String> = None;
                            let mut last_ts: Option<String> = None;
                            let mut first_prompt: Option<String> = None;
                            let mut msg_count: u32 = 0;

                            for line in content.lines() {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                                    let ts = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
                                    let line_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

                                    match line_type {
                                        "session_meta" => {
                                            if let Some(payload) = v.get("payload") {
                                                session_id = payload.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                                                project_path = payload.get("cwd").and_then(|c| c.as_str()).unwrap_or("").to_string();
                                                version = payload.get("cli_version").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                model = payload.get("model_provider").and_then(|m| m.as_str()).unwrap_or("openai").to_string();
                                            }
                                            first_ts = ts.clone();
                                        }
                                        "event_msg" => {
                                            if let Some(payload) = v.get("payload") {
                                                let event_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                                if event_type == "message" {
                                                    let role = payload.get("message").and_then(|m| m.get("role")).and_then(|r| r.as_str()).unwrap_or("");
                                                    if role == "user" {
                                                        let text = payload.get("message")
                                                            .and_then(|m| m.get("content"))
                                                            .and_then(|c| c.as_str())
                                                            .unwrap_or("");
                                                        if first_prompt.is_none() && !text.is_empty() {
                                                            first_prompt = Some(truncate_str(text, 200));
                                                        }
                                                    }
                                                    msg_count += 1;
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                    if ts.is_some() { last_ts = ts; }
                                }
                            }

                            if session_id.is_empty() {
                                session_id = name.trim_end_matches(".jsonl").to_string();
                            }

                            sessions.push(SessionInfo {
                                session_id,
                                source: "codex".to_string(),
                                project_path,
                                status: "completed".to_string(),
                                executor: "codex".to_string(),
                                model,
                                git_branch: None,
                                message_count: msg_count,
                                total_input_tokens: 0,
                                total_output_tokens: 0,
                                first_prompt,
                                created_at: first_ts,
                                last_active_at: last_ts,
                                file_size,
                                version: if version.is_empty() { None } else { Some(version) },
                                subagent_count: 0,
                            });
                            } // end for file_entry
                            } // end if let Ok(files)
                        } // end for day_entry
                    } // end if let Ok(days)
                } // end for month
            } // end if let Ok(months)
        } // end for year
    } // end if let Ok(years)
}

// ─── Hermes Scanner ───────────────────────────────────────

fn scan_hermes(sessions: &mut Vec<SessionInfo>) {
    let dir = home_dir().join(".hermes/sessions");
    if !dir.exists() { return; }

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }

            let name = entry.file_name().to_string_lossy().to_string();
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
}

// ─── Kimi Code Scanner ────────────────────────────────────

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

// ─── AtomCode Scanner ─────────────────────────────────────

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



// ─── Pi CLI Scanner ────────────────────────────────────────────

/// 把 pi 的项目目录编码还原成绝对路径。
///
/// pi 把 cwd 里的 `/` 替换为 `-`，并在头尾各加一个 `-`：
/// `/Users/weibh/projects/rust/nothing-todo` → `--Users-weibh-projects-rust-nothing-todo--`。
/// 反向就是去掉首尾的 `-` 再把 `-` 还原成 `/`。
fn decode_pi_cwd(encoded: &str) -> String {
    encoded.trim_matches('-').replace('-', "/")
}

/// JSONL 单行容错解析：只关心 `type` 字段、嵌套 `message` 字段和顶层 `cwd`/`id`/`timestamp`。
///
/// pi 的事件结构由 `backend/src/adapters/pi_event.rs` 描述；这里用的是 serde_json::Value
/// 偷懒解析（容错性比强类型更好），因为 scan 阶段不需要完整类型校验。
fn parse_pi_line(line: &str) -> Option<(String, serde_json::Value)> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let event_type = v.get("type")?.as_str()?.to_string();
    Some((event_type, v))
}

/// 从 pi session JSONL 文件里统计消息数 / tokens / 首个 user prompt / 最终 model。
///
/// 完整 JSONL 解析代价较大（实测单个 session 最高 1.5MB、772 行），但 JSONL 解析一次到
/// `serde_json::Value` 比逐字段 char-level 解析要快且安全。该函数只走一遍文件。
fn summarize_pi_jsonl(content: &str) -> PiSessionSummary {
    let mut summary = PiSessionSummary::default();
    let mut found_user_prompt = false;

    for line in content.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((event_type, v)) = parse_pi_line(line) else {
            continue;
        };

        // 第一行 (event_type == "session") 优先拿 cwd / id / timestamp，跳过 content 解析
        if event_type == "session" {
            if summary.cwd.is_none() {
                summary.cwd = v.get("cwd").and_then(|c| c.as_str()).map(String::from);
            }
            if summary.created_at.is_none() {
                summary.created_at = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
            }
            if summary.version.is_none() {
                summary.version = v.get("version").and_then(|v| v.as_u64()).map(|n| n.to_string());
            }
            continue;
        }

        // model_change 事件：provider + modelId。只在未设置时记录（第一条 model_change 决定 session 模型）
        if event_type == "model_change" {
            if summary.model.is_none() {
                let model_id = v.get("modelId").and_then(|m| m.as_str()).map(String::from);
                let provider = v.get("provider").and_then(|p| p.as_str()).map(String::from);
                summary.model = match (provider, model_id) {
                    (Some(p), Some(m)) => Some(format!("{}/{}", p, m)),
                    (_, Some(m)) => Some(m),
                    (Some(p), None) => Some(p),
                    _ => None,
                };
            }
            continue;
        }

        if event_type == "message" {
            summary.message_count += 1;
            let msg = match v.get("message") {
                Some(m) => m,
                None => continue,
            };

            // 首个 user prompt 提取
            if !found_user_prompt {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if role == "user" {
                    if let Some(text) = msg.get("content").and_then(|c| c.as_array()).and_then(|arr| {
                        arr.iter().find_map(|c| c.get("text").and_then(|t| t.as_str()))
                    }) {
                        summary.first_prompt = Some(text.to_string());
                        found_user_prompt = true;
                    }
                }
            }

            // tokens 累加：pi 的 usage 里 input 是不含缓存读写的净 input，
            // cacheRead/cacheWrite 单独记录；本 scan 把 cache 命中量计入 input
            // 等价物，避免反复读缓存的 session 在面板上只看到 output 高、input 低。
            if let Some(usage) = msg.get("usage") {
                let i = usage.get("input").and_then(|n| n.as_u64()).unwrap_or(0);
                let cr = usage.get("cacheRead").and_then(|n| n.as_u64()).unwrap_or(0);
                let cw = usage.get("cacheWrite").and_then(|n| n.as_u64()).unwrap_or(0);
                summary.total_input_tokens += i + cr + cw;
                if let Some(o) = usage.get("output").and_then(|n| n.as_u64()) {
                    summary.total_output_tokens += o;
                }
            }

            // 取 message 行的时间戳作为 last_active_at（持续更新为最新）
            if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
                summary.last_active_at = Some(ts.to_string());
            }
        }
    }
    summary
}

#[derive(Default)]
struct PiSessionSummary {
    cwd: Option<String>,
    created_at: Option<String>,
    last_active_at: Option<String>,
    version: Option<String>,
    model: Option<String>,
    first_prompt: Option<String>,
    message_count: u32,
    total_input_tokens: u64,
    total_output_tokens: u64,
}

/// 把 session JSONL 文件的 mtime 转成 RFC3339 字符串。
fn pi_mtime_to_rfc3339(path: &std::path::Path) -> Option<String> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Utc> = mtime.into();
    Some(dt.to_rfc3339())
}

/// pi 的 session 按项目目录存储，没有独立 active 索引。
/// 启发式：mtime < ACTIVE_WINDOW_SECONDS 视为 active。
///
/// 5 分钟窗口是个粗略估计：pi 在持续对话时几乎每秒都会 fsync JSONL；超过 5 分钟没
/// 写入通常意味着用户切换/退出。短于 5 分钟的瞬时静默（如网络抖动）会被误判为
/// active，代价仅是列表多一条 "active"，可以接受。
const PI_ACTIVE_WINDOW_SECONDS: u64 = 5 * 60;

fn scan_pi(sessions: &mut Vec<SessionInfo>) {
    let root = home_dir().join(".pi/agent/sessions");
    if !root.exists() { return; }

    let project_dirs = match std::fs::read_dir(&root) {
        Ok(it) => it,
        Err(_) => return,
    };

    for project_entry in project_dirs.flatten() {
        let path = project_entry.path();
        if !path.is_dir() { continue; }

        // 文件名格式: --Users-weibh-projects-rust-nothing-todo--（首尾各一个 -）
        let encoded = project_entry.file_name().to_string_lossy().to_string();
        let decoded_cwd = decode_pi_cwd(&encoded);
        let dir_mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                let secs = (chrono::Utc::now() - dt).num_seconds().max(0) as u64;
                Some(secs)
            })
            .unwrap_or(u64::MAX);

        let files = match std::fs::read_dir(&path) {
            Ok(it) => it,
            Err(_) => continue,
        };
        for file in files.flatten() {
            let fpath = file.path();
            if fpath.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }

            // 文件名格式: <iso-ts>_<uuid>.jsonl
            // 例如: 2026-06-11T01-44-54-108Z_019eb45a-8e5c-7cf4-95f9-787b5a83b0fa.jsonl
            let stem = match fpath.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let session_id = match stem.rsplit_once('_') {
                Some((_ts, uuid)) => uuid.to_string(),
                None => stem.clone(),
            };

            let file_size = std::fs::metadata(&fpath).map(|m| m.len()).unwrap_or(0);
            let file_mtime = std::fs::metadata(&fpath)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    let secs = (chrono::Utc::now() - dt).num_seconds().max(0) as u64;
                    Some(secs)
                })
                .unwrap_or(u64::MAX);

            // mtime 检查：仅看文件 mtime。父目录 mtime 只在「文件增/删/重命名」时更新，
            // 不反映文件内容修改；用「同项目下其他老 session 在新 session 创建后被错标为 active」。
            // 父目录 mtime 信息保留在 dir_mtime 变量中供以后需要独立状态时使用。
            let _ = dir_mtime;
            let is_active = file_mtime < PI_ACTIVE_WINDOW_SECONDS;

            // 跳过过期的 0 字节文件
            if file_size == 0 {
                continue;
            }

            // 解析整个文件：取 cwd / model / tokens / first_prompt 等。
            // mtime 启发式已经能给出 active 标记，因此我们只走一遍文件
            let content = match std::fs::read_to_string(&fpath) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let summary = summarize_pi_jsonl(&content);

            // cwd 优先级：JSONL 首行 > 文件名反编码
            let cwd = summary.cwd.unwrap_or_else(|| decoded_cwd.clone());
            // last_active_at 优先级：JSONL 最后事件时间戳 > 文件 mtime
            let last_active_at = summary
                .last_active_at
                .clone()
                .or_else(|| pi_mtime_to_rfc3339(&fpath));

            sessions.push(SessionInfo {
                session_id,
                source: "pi".to_string(),
                project_path: cwd,
                status: if is_active { "active".into() } else { "completed".into() },
                executor: "pi".to_string(),
                model: summary.model.unwrap_or_else(|| "-".into()),
                git_branch: None, // pi 不跟踪 git 分支
                message_count: summary.message_count,
                total_input_tokens: summary.total_input_tokens,
                total_output_tokens: summary.total_output_tokens,
                first_prompt: summary.first_prompt.map(|p| truncate_str(&p, 200)),
                created_at: summary.created_at,
                last_active_at,
                file_size,
                version: summary.version,
                subagent_count: 0,
            });
        }
    }
}

#[cfg(test)]
mod pi_scan_tests {
    use super::*;

    #[test]
    fn decode_pi_cwd_works() {
        // 基本替换：/→-，头尾加 -
        assert_eq!(decode_pi_cwd("--Users-weibh--"), "Users/weibh");
        assert_eq!(decode_pi_cwd("--tmp--"), "tmp");
        // 没有首尾的 -：原样替换（表示这不是 pi 编码过的）
        assert_eq!(decode_pi_cwd("foo"), "foo");
        // ⚠️ 已知歧义：项目名中的连字符会被错误还原为 /。
        // 例如真实路径 `/Users/weibh/projects/rust/nothing-todo` 被 pi 编码为
        // `--Users-weibh-projects-rust-nothing-todo--`，但我们反解码出的是
        // `Users/weibh/projects/rust/nothing/todo`。这是 pi 编码策略本身的歧义：
        // 它无法区分「路径分隔符」与「合法目录名里的 -」。
        // 调用方（scan_pi）实际优先用 JSONL 首行的 `cwd` 字段，filename
        // 解码结果仅在 cwd 缺失时作为 hint 使用。
        assert_eq!(
            decode_pi_cwd("--Users-weibh-projects-rust-nothing-todo--"),
            "Users/weibh/projects/rust/nothing/todo"
        );
    }

    #[test]
    fn parse_pi_line_handles_garbage() {
        assert!(parse_pi_line("").is_none());
        assert!(parse_pi_line("not json").is_none());
        assert!(parse_pi_line("{}").is_none()); // no type
    }

    #[test]
    fn summarize_pi_jsonl_extracts_tokens_and_first_prompt() {
        let content = "\
{\"type\":\"session\",\"version\":3,\"id\":\"019eb48c-a6c0-79b1-88ae-44ec6a1bf9bd\",\"timestamp\":\"2026-06-11T02:39:37.152Z\",\"cwd\":\"/Users/weibh/projects/nothing-todo\"}
{\"type\":\"model_change\",\"id\":\"4500ec8e\",\"timestamp\":\"2026-06-11T02:39:37.175Z\",\"provider\":\"anthropic\",\"modelId\":\"claude-opus-4\"}
{\"type\":\"message\",\"timestamp\":\"2026-06-11T02:39:39.498Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"你好\"}]}}
{\"type\":\"message\",\"timestamp\":\"2026-06-11T02:39:50.086Z\",\"message\":{\"role\":\"assistant\",\"content\":[],\"model\":\"claude-opus-4\",\"usage\":{\"input\":15,\"output\":44,\"cacheRead\":2585,\"cacheWrite\":0,\"totalTokens\":2644},\"stopReason\":\"toolUse\"}}
{\"type\":\"message\",\"timestamp\":\"2026-06-11T02:40:01.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"done\"}],\"usage\":{\"input\":1,\"output\":2,\"totalTokens\":3}}}
";
        let s = summarize_pi_jsonl(content);
        assert_eq!(s.cwd.as_deref(), Some("/Users/weibh/projects/nothing-todo"));
        assert_eq!(s.version.as_deref(), Some("3"));
        assert_eq!(s.model.as_deref(), Some("anthropic/claude-opus-4"));
        assert_eq!(s.message_count, 3);
        // msg2: input=15 + cacheRead=2585 + cacheWrite=0 = 2600
        // msg3: input=1（无 cache）
        // total input = 2601（cache 计入 input 等价物）
        assert_eq!(s.total_input_tokens, 2601);
        assert_eq!(s.total_output_tokens, 46);
        assert_eq!(s.first_prompt.as_deref(), Some("你好"));
        assert_eq!(s.last_active_at.as_deref(), Some("2026-06-11T02:40:01.000Z"));
    }

    /// 扫真实本地 ~/.pi/agent/sessions/，验证 scan_pi 端到端可用。
    /// 需要本地安装并使用过 pi。默认 #[ignore] 不参与 CI，仅手动验证：
    ///   cargo test --lib -- --ignored scan_pi_against_real_local_data
    #[test]
    #[ignore]
    fn scan_pi_against_real_local_data() {
        let mut sessions = Vec::new();
        scan_pi(&mut sessions);
        assert!(
            !sessions.is_empty(),
            "expected scan_pi to find at least one session under ~/.pi/agent/sessions/"
        );
        // 拿一个 session_id 走 get_pi_detail，验证 C1 不再返 404
        let first = &sessions[0];
        let detail = get_pi_detail(&first.session_id)
            .expect("C1: get_pi_detail should return Some for a session scan_pi found");
        assert_eq!(detail.info.source, "pi");
        assert_eq!(detail.info.session_id, first.session_id);
        assert!(
            !detail.messages.is_empty(),
            "expected get_pi_detail to populate messages"
        );
        for s in &sessions {
            println!(
                "id={} cwd={} status={} model={} msgs={} in={} out={} size={}",
                s.session_id,
                s.project_path,
                s.status,
                s.model,
                s.message_count,
                s.total_input_tokens,
                s.total_output_tokens,
                s.file_size
            );
            assert!(s.source == "pi");
            assert!(!s.session_id.is_empty());
        }
    }

    /// 验证 cacheRead/cacheWrite 被计入 total_input_tokens（H3-A 修法）。
    /// 场景：一条 message 只含 cacheRead（0 input/0 output），另一条只含 cacheWrite。
    #[test]
    fn summarize_pi_jsonl_handles_cache_tokens() {
        let content = "\
{\"type\":\"session\",\"id\":\"aaa\",\"timestamp\":\"2026-06-11T02:00:00Z\",\"cwd\":\"/x\"}
{\"type\":\"message\",\"timestamp\":\"2026-06-11T02:00:01Z\",\"message\":{\"role\":\"assistant\",\"content\":[],\"usage\":{\"input\":0,\"output\":10,\"cacheRead\":100,\"cacheWrite\":0,\"totalTokens\":110}}}
{\"type\":\"message\",\"timestamp\":\"2026-06-11T02:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[],\"usage\":{\"input\":0,\"output\":20,\"cacheRead\":0,\"cacheWrite\":50,\"totalTokens\":70}}}
";
        let s = summarize_pi_jsonl(content);
        // msg1: input=0 + cacheRead=100 + cacheWrite=0 = 100
        // msg2: input=0 + cacheRead=0 + cacheWrite=50 = 50
        assert_eq!(s.total_input_tokens, 150);
        assert_eq!(s.total_output_tokens, 30);
    }

    /// 验证 build_pi_messages 输出预览包含 text 与 toolCall 拼接、role 正确、stop_reason 提取。
    #[test]
    fn build_pi_messages_extracts_text_and_tool_calls() {
        let content = "\
{\"type\":\"session\",\"id\":\"aaa\",\"timestamp\":\"2026-06-11T02:00:00Z\",\"cwd\":\"/x\"}
{\"type\":\"message\",\"timestamp\":\"2026-06-11T02:00:01Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"看一下\"}]}}
{\"type\":\"message\",\"timestamp\":\"2026-06-11T02:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"好的。\"},{\"type\":\"toolCall\",\"id\":\"c1\",\"name\":\"bash\",\"arguments\":{\"command\":\"ls\"}}],\"model\":\"claude-opus-4\",\"usage\":{\"input\":5,\"output\":3,\"cacheRead\":0,\"cacheWrite\":0,\"totalTokens\":8},\"stopReason\":\"toolUse\"}}
";
        let (msgs, summary) = build_pi_messages(content);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert!(msgs[0].content_preview.contains("看一下"));
        assert_eq!(msgs[1].role, "assistant");
        assert!(msgs[1].content_preview.contains("好的"));
        assert!(msgs[1].content_preview.contains("[toolCall: bash]"));
        assert_eq!(msgs[1].stop_reason.as_deref(), Some("toolUse"));
        assert_eq!(msgs[1].model.as_deref(), Some("claude-opus-4"));
        assert_eq!(msgs[1].input_tokens, Some(5));
        assert_eq!(msgs[1].output_tokens, Some(3));
        // first_prompt 取自首条 user
        assert_eq!(summary.first_prompt.as_deref(), Some("看一下"));
    }
}

// ─── Unified scan ─────────────────────────────────────────

/// Map executor name to scanner function and default source name.
/// Only executors listed here will be scanned.
fn get_scanner(name: &str) -> Option<fn(&mut Vec<SessionInfo>)> {
    match name {
        "claudecode" => Some(scan_claude_code),
        "codex" => Some(scan_codex),
        "hermes" => Some(scan_hermes),
        "kimi" => Some(scan_kimi),
        "atomcode" => Some(scan_atomcode),
        "pi" => Some(scan_pi),
        "codebuddy" | "opencode" | "mobilecoder" | "mimo" => None, // no session storage found
        _ => None,
    }
}

fn scan_for_executors(enabled_executors: &[crate::models::ExecutorConfig]) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();

    for exec in enabled_executors {
        // Check if session_dir is configured and exists
        if !exec.session_dir.is_empty() {
            let expanded = exec.session_dir.replace('~', &dirs::home_dir().unwrap_or_default().to_string_lossy());
            if !std::path::Path::new(&expanded).exists() {
                continue;
            }
        }

        if let Some(scanner) = get_scanner(&exec.name) {
            scanner(&mut sessions);
        }
    }

    // Sort by last_active_at descending
    sessions.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    sessions
}

// ─── Handlers ─────────────────────────────────────────────

pub async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<ApiResponse<SessionListResponse>, AppError> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    let executors = state.db.get_enabled_executors().await.map_err(|e| AppError::Internal(e.to_string()))?;

    let result = tokio::task::spawn_blocking(move || {
        let mut sessions = scan_for_executors(&executors);

        // Apply filters
        if let Some(ref status) = query.status {
            sessions.retain(|s| &s.status == status);
        }
        if let Some(ref source) = query.source {
            sessions.retain(|s| s.source == *source);
        }
        if let Some(ref executor) = query.executor {
            sessions.retain(|s| s.executor == *executor);
        }
        if let Some(ref project) = query.project {
            sessions.retain(|s| s.project_path.contains(project));
        }
        if let Some(ref search) = query.search {
            let search_lower = search.to_lowercase();
            sessions.retain(|s| {
                s.first_prompt.as_ref()
                    .map(|p| p.to_lowercase().contains(&search_lower))
                    .unwrap_or(false)
            });
        }

        let total = sessions.len() as u64;
        let start = ((page - 1) * page_size) as usize;
        let end = (start + page_size as usize).min(sessions.len());
        let page_data = if start < sessions.len() {
            sessions[start..end].to_vec()
        } else {
            Vec::new()
        };

        SessionListResponse { sessions: page_data, total, page, page_size }
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(result))
}

pub async fn get_session_stats(
    State(state): State<AppState>,
) -> Result<ApiResponse<SessionStats>, AppError> {
    let executors = state.db.get_enabled_executors().await.map_err(|e| AppError::Internal(e.to_string()))?;

    let stats = tokio::task::spawn_blocking(move || {
        let sessions = scan_for_executors(&executors);
        let now = chrono::Utc::now();
        let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();

        let mut by_source: HashMap<String, u64> = HashMap::new();
        let mut by_executor: HashMap<String, u64> = HashMap::new();
        let mut by_project: HashMap<String, u64> = HashMap::new();
        let mut active_count = 0u64;
        let mut today_count = 0u64;
        let mut total_in = 0u64;
        let mut total_out = 0u64;

        for s in &sessions {
            *by_source.entry(s.source.clone()).or_insert(0) += 1;
            *by_executor.entry(s.executor.clone()).or_insert(0) += 1;
            *by_project.entry(s.project_path.clone()).or_insert(0) += 1;
            total_in += s.total_input_tokens;
            total_out += s.total_output_tokens;
            if s.status == "active" { active_count += 1; }
            if let Some(ref created) = s.created_at {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(created) {
                    if dt.naive_utc() >= today_start { today_count += 1; }
                }
            }
        }

        SessionStats {
            total_sessions: sessions.len() as u64,
            active_sessions: active_count,
            today_sessions: today_count,
            total_input_tokens: total_in,
            total_output_tokens: total_out,
            by_source,
            by_executor,
            by_project,
        }
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(stats))
}

pub async fn get_session_detail(
    State(_state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<ApiResponse<SessionDetail>, AppError> {
    let detail = tokio::task::spawn_blocking(move || {
        // Try each source to find the session

        // Claude Code
        if let Some(d) = get_claude_detail(&session_id) { return Some(d); }
        // Codex
        if let Some(d) = get_codex_detail(&session_id) { return Some(d); }
        // Hermes
        if let Some(d) = get_hermes_detail(&session_id) { return Some(d); }
        // Kimi
        if let Some(d) = get_kimi_detail(&session_id) { return Some(d); }
        // AtomCode
        if let Some(d) = get_atomcode_detail(&session_id) { return Some(d); }
        // Pi
        if let Some(d) = get_pi_detail(&session_id) { return Some(d); }
        None
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    match detail {
        Some(d) => Ok(ApiResponse::ok(d)),
        None => Err(AppError::NotFound),
    }
}

pub async fn delete_session(
    State(_state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    tokio::task::spawn_blocking(move || {
        // Try to delete from each source
        let claude_dir = home_dir().join(".claude/projects");
        if let Ok(entries) = std::fs::read_dir(&claude_dir) {
            for entry in entries.flatten() {
                let jsonl = entry.path().join(format!("{}.jsonl", session_id));
                if jsonl.exists() {
                    let _ = std::fs::remove_file(&jsonl);
                    let dir = jsonl.with_extension("");
                    if dir.is_dir() { let _ = std::fs::remove_dir_all(&dir); }
                    return true;
                }
            }
        }
        false
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(()))
}

// ─── Detail getters per source ────────────────────────────

fn get_claude_detail(session_id: &str) -> Option<SessionDetail> {
    let projects_dir = home_dir().join(".claude/projects");
    let mut jsonl_path: Option<PathBuf> = None;
    let mut project_path = String::new();

    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        for entry in entries.flatten() {
            let candidate = entry.path().join(format!("{}.jsonl", session_id));
            if candidate.exists() {
                jsonl_path = Some(candidate);
                project_path = decode_project_path(&entry.file_name().to_string_lossy());
                break;
            }
        }
    }
    let path = jsonl_path?;
    let content = std::fs::read_to_string(&path).ok()?;
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let active_set = collect_claude_active_sessions();

    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;
    let mut model: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut version: Option<String> = None;
    let mut executor: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    let mut msg_count: u32 = 0;
    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    let mut messages: Vec<SessionMessage> = Vec::new();

    for line in content.lines() {
        if let Some((ts, mdl, branch, ver, entry, prompt, inp, out, role)) =
            parse_claude_line_metadata(line)
        {
            if first_ts.is_none() { first_ts = ts.clone(); }
            if ts.is_some() { last_ts = ts.clone(); }
            if mdl.is_some() { model = mdl.clone(); }
            if branch.is_some() { git_branch = branch; }
            if ver.is_some() { version = ver; }
            if entry.is_some() { executor = entry; }
            if first_prompt.is_none() && prompt.is_some() { first_prompt = prompt.clone(); }
            if role == "user" || role == "assistant" {
                msg_count += 1;
                let preview = prompt.map(|p| truncate_str(&p, 500)).unwrap_or_default();
                messages.push(SessionMessage {
                    role: role.clone(),
                    content_preview: preview,
                    model: mdl.clone(),
                    input_tokens: inp,
                    output_tokens: out,
                    timestamp: ts,
                    stop_reason: None,
                });
            }
            if let Some(i) = inp { total_in += i; }
            if let Some(o) = out { total_out += o; }
        }
    }

    // Subagents
    let session_dir = path.with_extension("");
    let subagents_dir = session_dir.join("subagents");
    let mut subagents: Vec<SubAgentInfo> = Vec::new();
    if subagents_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&subagents_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Some(name) = p.file_stem().and_then(|n| n.to_str()) {
                        if name.ends_with(".meta") {
                            if let Ok(c) = std::fs::read_to_string(&p) {
                                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&c) {
                                    subagents.push(SubAgentInfo {
                                        agent_type: meta.get("agentType").and_then(|t| t.as_str()).unwrap_or("unknown").to_string(),
                                        description: meta.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                                        message_count: 0,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Some(SessionDetail {
        info: SessionInfo {
            session_id: session_id.to_string(),
            source: "claudecode".to_string(),
            project_path,
            status: if active_set.contains(session_id) { "active".into() } else { "completed".into() },
            executor: executor.unwrap_or_else(|| "unknown".into()),
            model: model.unwrap_or_else(|| "-".into()),
            git_branch,
            message_count: msg_count,
            total_input_tokens: total_in,
            total_output_tokens: total_out,
            first_prompt: first_prompt.map(|p| truncate_str(&p, 200)),
            created_at: first_ts,
            last_active_at: last_ts,
            file_size,
            version,
            subagent_count: subagents.len() as u32,
        },
        messages,
        subagents,
    })
}

fn get_codex_detail(session_id: &str) -> Option<SessionDetail> {
    let base = home_dir().join(".codex/sessions");
    if !base.exists() { return None; }

    // Walk to find matching rollout file
    if let Ok(y1) = std::fs::read_dir(&base) {
        for y in y1.flatten().filter(|e| e.path().is_dir()) {
            if let Ok(m1) = std::fs::read_dir(y.path()) {
                for m in m1.flatten().filter(|e| e.path().is_dir()) {
                    if let Ok(d1) = std::fs::read_dir(m.path()) {
                        for f in d1.flatten() {
                            let p = f.path();
                            if !p.is_file() { continue; }
                            let name = f.file_name().to_string_lossy().to_string();
                            if !name.starts_with("rollout-") || !name.ends_with(".jsonl") { continue; }

                            // Check if session_id matches the id in session_meta
                            let content = std::fs::read_to_string(&p).ok()?;
                            let first_line = content.lines().next()?;
                            let first: serde_json::Value = serde_json::from_str(first_line).ok()?;
                            let sid = first.get("payload").and_then(|p| p.get("id")).and_then(|i| i.as_str()).unwrap_or("");
                            if sid != session_id && !name.contains(session_id) { continue; }

                            let file_size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                            let mut project_path = String::new();
                            let mut version = String::new();
                            let mut model = String::new();
                            let mut first_ts: Option<String> = None;
                            let mut last_ts: Option<String> = None;
                            let mut first_prompt: Option<String> = None;
                            let mut msg_count: u32 = 0;
                            let mut messages: Vec<SessionMessage> = Vec::new();

                            for line in content.lines() {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                                    let ts = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
                                    let line_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

                                    match line_type {
                                        "session_meta" => {
                                            if let Some(payload) = v.get("payload") {
                                                project_path = payload.get("cwd").and_then(|c| c.as_str()).unwrap_or("").to_string();
                                                version = payload.get("cli_version").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                model = payload.get("model_provider").and_then(|m| m.as_str()).unwrap_or("openai").to_string();
                                            }
                                            first_ts = ts.clone();
                                        }
                                        "event_msg" => {
                                            if let Some(payload) = v.get("payload") {
                                                let event_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                                if event_type == "message" {
                                                    let role = payload.get("message").and_then(|m| m.get("role")).and_then(|r| r.as_str()).unwrap_or("").to_string();
                                                    let text = payload.get("message")
                                                        .and_then(|m| m.get("content"))
                                                        .and_then(|c| c.as_str())
                                                        .unwrap_or("").to_string();
                                                    if first_prompt.is_none() && role == "user" && !text.is_empty() {
                                                        first_prompt = Some(truncate_str(&text, 200));
                                                    }
                                                    msg_count += 1;
                                                    messages.push(SessionMessage {
                                                        role: role.clone(),
                                                        content_preview: truncate_str(&text, 500),
                                                        model: if role == "assistant" { Some(model.clone()) } else { None },
                                                        input_tokens: None,
                                                        output_tokens: None,
                                                        timestamp: ts.clone(),
                                                        stop_reason: None,
                                                    });
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                    if ts.is_some() { last_ts = ts; }
                                }
                            }

                            return Some(SessionDetail {
                                info: SessionInfo {
                                    session_id: session_id.to_string(),
                                    source: "codex".to_string(),
                                    project_path,
                                    status: "completed".to_string(),
                                    executor: "codex".to_string(),
                                    model,
                                    git_branch: None,
                                    message_count: msg_count,
                                    total_input_tokens: 0,
                                    total_output_tokens: 0,
                                    first_prompt,
                                    created_at: first_ts,
                                    last_active_at: last_ts,
                                    file_size,
                                    version: if version.is_empty() { None } else { Some(version) },
                                    subagent_count: 0,
                                },
                                messages,
                                subagents: vec![],
                            });
                        }
                    }
                }
            }
        }
    }
    None
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

// ─── Pi Detail ──────────────────────────────────────────────

/// 从 session JSONL 里抽取消息预览/usage/stop_reason 等用于详情面板。
///
/// 优先复用 scan_pi 用的 summarize 逻辑，但详情页面要 messages 数组而不仅是
/// 计数，所以这里另外走一遍 file。
fn build_pi_messages(content: &str) -> (Vec<SessionMessage>, PiSessionSummary) {
    let mut messages: Vec<SessionMessage> = Vec::new();
    let mut summary = PiSessionSummary::default();

    for line in content.lines() {
        if line.is_empty() { continue; }
        let Some((event_type, v)) = parse_pi_line(line) else { continue; };

        if event_type == "session" {
            if summary.cwd.is_none() {
                summary.cwd = v.get("cwd").and_then(|c| c.as_str()).map(String::from);
            }
            if summary.created_at.is_none() {
                summary.created_at = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
            }
            if summary.version.is_none() {
                summary.version = v.get("version").and_then(|v| v.as_u64()).map(|n| n.to_string());
            }
            continue;
        }

        if event_type == "model_change" {
            if summary.model.is_none() {
                let model_id = v.get("modelId").and_then(|m| m.as_str()).map(String::from);
                let provider = v.get("provider").and_then(|p| p.as_str()).map(String::from);
                summary.model = match (provider, model_id) {
                    (Some(p), Some(m)) => Some(format!("{}/{}", p, m)),
                    (_, Some(m)) => Some(m),
                    (Some(p), None) => Some(p),
                    _ => None,
                };
            }
            continue;
        }

        if event_type != "message" { continue; }
        let Some(msg) = v.get("message") else { continue; };
        summary.message_count += 1;

        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();
        // 从 content 数组里拼接 text / toolCall 名作为预览
        let preview = msg.get("content")
            .and_then(|c| c.as_array())
            .map(|arr| {
                let mut pieces: Vec<String> = Vec::new();
                for block in arr {
                    match block.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                                pieces.push(t.to_string());
                            }
                        }
                        Some("toolCall") => {
                            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                            pieces.push(format!("[toolCall: {}]", name));
                        }
                        Some("toolResult") => {
                            // 预览里仅占位，避免巨大工具输出烒友预览
                            pieces.push("[toolResult]".to_string());
                        }
                        _ => {}
                    }
                }
                pieces.join("\n")
            })
            .unwrap_or_default();

        let input_tokens = msg.get("usage").and_then(|u| u.get("input")).and_then(|n| n.as_u64());
        let output_tokens = msg.get("usage").and_then(|u| u.get("output")).and_then(|n| n.as_u64());
        let stop_reason = msg.get("stopReason").and_then(|s| s.as_str()).map(String::from);
        let model = msg.get("model").and_then(|m| m.as_str()).map(String::from);
        let timestamp = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);

        // first_prompt 提取（与 scan_pi 逻辑一致）
        if summary.first_prompt.is_none() && role == "user" {
            if let Some(text) = msg.get("content").and_then(|c| c.as_array()).and_then(|arr| {
                arr.iter().find_map(|c| c.get("text").and_then(|t| t.as_str()))
            }) {
                summary.first_prompt = Some(text.to_string());
            }
        }

        // tokens 累加（与 scan_pi 一致：cache 计入 input）
        if let Some(usage) = msg.get("usage") {
            let i = usage.get("input").and_then(|n| n.as_u64()).unwrap_or(0);
            let cr = usage.get("cacheRead").and_then(|n| n.as_u64()).unwrap_or(0);
            let cw = usage.get("cacheWrite").and_then(|n| n.as_u64()).unwrap_or(0);
            summary.total_input_tokens += i + cr + cw;
            if let Some(o) = usage.get("output").and_then(|n| n.as_u64()) {
                summary.total_output_tokens += o;
            }
        }
        if let Some(ts) = &timestamp {
            summary.last_active_at = Some(ts.clone());
        }

        messages.push(SessionMessage {
            role,
            content_preview: truncate_str(&preview, 500),
            model,
            input_tokens,
            output_tokens,
            timestamp,
            stop_reason,
        });
    }

    (messages, summary)
}

fn get_pi_detail(session_id: &str) -> Option<SessionDetail> {
    let root = home_dir().join(".pi/agent/sessions");
    if !root.exists() { return None; }

    let project_dirs = std::fs::read_dir(&root).ok()?;
    for project_dir in project_dirs.flatten().filter(|e| e.path().is_dir()) {
        let files = match std::fs::read_dir(project_dir.path()) {
            Ok(it) => it,
            Err(_) => continue,
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            // 文件名 = <iso-ts>_<uuid>.jsonl，session_id 是后半 UUID
            let parsed_sid = match stem.rsplit_once('_') {
                Some((_ts, uuid)) => uuid.to_string(),
                None => stem.clone(),
            };
            if parsed_sid != session_id { continue; }

            let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let content = std::fs::read_to_string(&path).ok()?;
            let (messages, summary) = build_pi_messages(&content);

            let project_path = summary.cwd.clone().unwrap_or_else(|| {
                let encoded = project_dir.file_name().to_string_lossy().to_string();
                decode_pi_cwd(&encoded)
            });
            let last_active_at = summary
                .last_active_at
                .clone()
                .or_else(|| pi_mtime_to_rfc3339(&path));

            return Some(SessionDetail {
                info: SessionInfo {
                    session_id: session_id.to_string(),
                    source: "pi".to_string(),
                    project_path,
                    status: "completed".to_string(),
                    executor: "pi".to_string(),
                    model: summary.model.clone().unwrap_or_else(|| "-".into()),
                    git_branch: None,
                    message_count: summary.message_count,
                    total_input_tokens: summary.total_input_tokens,
                    total_output_tokens: summary.total_output_tokens,
                    first_prompt: summary.first_prompt.clone().map(|p| truncate_str(&p, 200)),
                    created_at: summary.created_at.clone(),
                    last_active_at,
                    file_size,
                    version: summary.version.clone(),
                    subagent_count: 0,
                },
                messages,
                subagents: vec![],
            });
        }
    }
    None
}


