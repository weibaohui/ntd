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
        "codebuddy" | "opencode" | "mobilecoder" => None, // no session storage found
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


