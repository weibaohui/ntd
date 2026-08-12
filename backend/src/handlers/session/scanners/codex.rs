//! Codex 会话扫描器：scan_codex / get_codex_detail 及私有辅助族。
//! 096-W3-PR2 从 handlers/session.rs 逐字搬迁（Move Function），逻辑零改动。

use super::{home_dir, truncate_str};
use crate::handlers::session::{SessionDetail, SessionInfo, SessionMessage, SessionScanner};

/// codex 在 sessions/YYYY/MM/DD/ 下按日期组织 JSONL 文件。
/// 用闭包+?扁平化原本嵌套 7+ 层的 read_dir 链,每层只负责"是否进入下一层目录"。
fn iter_codex_rollout_files(base: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(years) = std::fs::read_dir(base) else { return Vec::new() };
    let mut out = Vec::new();
    // 用 flat_map 把三层 read_dir + is_dir 守卫压平成一次迭代;
    // 任一层不可读或非目录,该项被过滤,继续走下一项。
    // .flatten() 在每一层消费 Result<DirEntry, io::Error>,遇到错误跳过该项。
    for day_entry in years
        .flatten()
        .filter(|e| e.path().is_dir())
        .flat_map(|y| std::fs::read_dir(y.path()).ok().into_iter().flatten())
        .flatten()
        .filter(|e| e.path().is_dir())
        .flat_map(|m| std::fs::read_dir(m.path()).ok().into_iter().flatten())
        .flatten()
        .filter(|e| e.path().is_dir())
        .flat_map(|d| std::fs::read_dir(d.path()).ok().into_iter().flatten())
        .flatten()
    {
        let path = day_entry.path();
        if !path.is_file() { continue; }
        let name = day_entry.file_name().to_string_lossy().to_string();
        // codex 的命名约定:rollout-<timestamp>-<uuid>.jsonl
        if name.starts_with("rollout-") && name.ends_with(".jsonl") {
            out.push(path);
        }
    }
    out
}

/// 解析单行 session_meta,得到 id/cwd/version/model_provider 等会话级字段。
/// 返回 None 表示该行不是 session_meta 或缺失 payload。
fn parse_codex_session_meta(line: &serde_json::Value) -> Option<CodexSessionMeta> {
    if line.get("type").and_then(|t| t.as_str()) != Some("session_meta") { return None; }
    let payload = line.get("payload")?;
    Some(CodexSessionMeta {
        session_id: payload.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
        project_path: payload.get("cwd").and_then(|c| c.as_str()).unwrap_or("").to_string(),
        version: payload.get("cli_version").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        model: payload.get("model_provider").and_then(|m| m.as_str()).unwrap_or("openai").to_string(),
    })
}

/// codex session_meta 解析出的会话级元数据。
/// 用具名字段代替原嵌套 `.get(...).and_then(...)` 链,降低嵌套层级并自解释。
struct CodexSessionMeta {
    session_id: String,
    project_path: String,
    version: String,
    model: String,
}

/// 解析 event_msg 行的 message 事件,抽取 user 的首个文本作为 first_prompt。
/// 返回 Some(text) 表示该行是 user 消息;返回 None 表示无关事件或角色非 user。
/// 计数由调用方根据返回值是否 Some 来决定,避免内嵌 if 嵌套。
fn parse_codex_user_prompt(line: &serde_json::Value) -> Option<String> {
    if line.get("type").and_then(|t| t.as_str()) != Some("event_msg") { return None; }
    let payload = line.get("payload")?;
    if payload.get("type").and_then(|t| t.as_str()) != Some("message") { return None; }
    let msg = payload.get("message")?;
    if msg.get("role").and_then(|r| r.as_str()) != Some("user") { return None; }
    msg.get("content").and_then(|c| c.as_str()).filter(|t| !t.is_empty()).map(String::from)
}

/// event_msg:message 类型的消息计数,user/assistant 都计入 msg_count。
/// 拆分自内联 `if event_type == "message"` 嵌套。
fn is_codex_message_event(line: &serde_json::Value) -> bool {
    if line.get("type").and_then(|t| t.as_str()) != Some("event_msg") { return false; }
    line.get("payload")
        .and_then(|p| p.get("type"))
        .and_then(|t| t.as_str())
        == Some("message")
}

/// 汇总单文件扫描结果,只在调用方 flat_map 的回调里组装 SessionInfo。
/// 抽到这里是为把 scan_codex 主循环压到 ≤30 行。
fn build_codex_session_info(_path: &std::path::Path, content: &str, file_size: u64) -> Option<SessionInfo> {
    let mut session_id = String::new();
    let mut project_path = String::new();
    let mut version = String::new();
    let mut model = String::new();
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    let mut msg_count: u32 = 0;

    // 逐行解析:用 ? / filter 替代内嵌 if let Some() 链,最深嵌套降到 2 层。
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let ts = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);

        if let Some(meta) = parse_codex_session_meta(&v) {
            session_id = meta.session_id;
            project_path = meta.project_path;
            version = meta.version;
            model = meta.model;
            first_ts = ts.clone();
        }
        if is_codex_message_event(&v) {
            msg_count += 1;
            if let Some(text) = parse_codex_user_prompt(&v) {
                if first_prompt.is_none() {
                    first_prompt = Some(truncate_str(&text, 200));
                }
            }
        }
        if ts.is_some() { last_ts = ts; }
    }

    if session_id.is_empty() { return None; }
    Some(SessionInfo {
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
    })
}

fn scan_codex(sessions: &mut Vec<SessionInfo>) {
    let base = home_dir().join(".codex/sessions");
    if !base.exists() { return; }

    // 三层目录遍历 + rollout 前缀过滤收敛到一次调用,
    // 原本的 7+ 层 if let Ok / is_dir / starts_with 嵌套全部消失。
    for path in iter_codex_rollout_files(&base) {
        let Ok(content) = std::fs::read_to_string(&path) else { continue };
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        // 单文件解析收敛到一个函数,主循环只剩"遍历 + 推入结果"。
        if let Some(info) = build_codex_session_info(&path, &content, file_size) {
            sessions.push(info);
        }
    }
}

/// 读取 codex rollout 文件首行,得到 session_meta 中的 id。
/// 用作 get_codex_detail 的快路径匹配,避免对每个候选文件做全量 JSON 解析。
fn codex_session_id_from_first_line(content: &str) -> Option<String> {
    let first = serde_json::from_str::<serde_json::Value>(content.lines().next()?).ok()?;
    first.get("payload")
        .and_then(|p| p.get("id"))
        .and_then(|i| i.as_str())
        .map(String::from)
}

/// codex 单文件 -> SessionDetail。
/// 把 get_codex_detail 主循环里的 5+ 层 if let Ok / is_dir 嵌套全部收敛到此处;
/// 命中条件:首行 session_meta.id 匹配 OR 文件名包含 session_id(双保险)。
fn build_codex_session_detail(path: &std::path::Path, session_id: &str) -> Option<SessionDetail> {
    let content = std::fs::read_to_string(path).ok()?;
    let sid = codex_session_id_from_first_line(&content).unwrap_or_default();
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if sid != session_id && !name.contains(session_id) { return None; }

    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut project_path = String::new();
    let mut version = String::new();
    let mut model = String::new();
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    let mut msg_count: u32 = 0;
    let mut messages: Vec<SessionMessage> = Vec::new();

    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let ts = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);

        if let Some(meta) = parse_codex_session_meta(&v) {
            project_path = meta.project_path;
            version = meta.version;
            model = meta.model;
            first_ts = ts.clone();
        }
        if is_codex_message_event(&v) {
            if let Some(text) = parse_codex_user_prompt(&v) {
                if first_prompt.is_none() {
                    first_prompt = Some(truncate_str(&text, 200));
                }
            }
            // codex 的 event_msg:message 不区分 user/assistant,只记录文本,
            // 这里统一构造一条 SessionMessage 推入 messages。
            let role = v.get("payload")
                .and_then(|p| p.get("message"))
                .and_then(|m| m.get("role"))
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            let text = v.get("payload")
                .and_then(|p| p.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
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
        if ts.is_some() { last_ts = ts; }
    }

    Some(SessionDetail {
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
    })
}

fn get_codex_detail(session_id: &str) -> Option<SessionDetail> {
    let base = home_dir().join(".codex/sessions");
    if !base.exists() { return None; }
    // 复用 iter_codex_rollout_files 把三层目录遍历压平,主循环只剩"找匹配 + 返回"。
    for path in iter_codex_rollout_files(&base) {
        if let Some(detail) = build_codex_session_detail(&path, session_id) {
            return Some(detail);
        }
    }
    None
}

/// 扫描器的零大小标记 struct——实例仅承载类型信息，
/// I/O 与解析逻辑即本文件的 `scan_codex` / `get_codex_detail`（096-W3-PR2 收编后不再转发）。
pub struct CodexScanner;

impl SessionScanner for CodexScanner {
    fn name(&self) -> &'static str { "codex" }
    fn scan(&self, out: &mut Vec<SessionInfo>) { scan_codex(out); }
    fn get_detail(&self, session_id: &str) -> Option<SessionDetail> { get_codex_detail(session_id) }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_codex_session_meta_returns_none_for_non_meta() {
        assert!(parse_codex_session_meta(&json!({"type": "event_msg"})).is_none());
        let meta = parse_codex_session_meta(&json!({
            "type": "session_meta",
            "payload": {"id": "abc", "cwd": "/w", "cli_version": "0.1", "model_provider": "oai"},
        })).expect("session_meta should parse");
        assert_eq!(meta.session_id, "abc");
        assert_eq!(meta.project_path, "/w");
        assert_eq!(meta.version, "0.1");
        assert_eq!(meta.model, "oai");
    }

    #[test]
    fn parse_codex_user_prompt_filters_by_role_and_non_empty_text() {
        // 非 event_msg
        assert!(parse_codex_user_prompt(&json!({"type": "session_meta"})).is_none());
        // event_msg 但非 message
        assert!(parse_codex_user_prompt(&json!({"type": "event_msg", "payload": {"type": "x"}})).is_none());
        // event_msg:message 但非 user
        assert!(parse_codex_user_prompt(&json!({
            "type": "event_msg",
            "payload": {"type": "message", "message": {"role": "assistant", "content": "hi"}}
        })).is_none());
        // user 但空文本 → None(filter 排除)
        assert!(parse_codex_user_prompt(&json!({
            "type": "event_msg",
            "payload": {"type": "message", "message": {"role": "user", "content": ""}}
        })).is_none());
        // 命中
        let got = parse_codex_user_prompt(&json!({
            "type": "event_msg",
            "payload": {"type": "message", "message": {"role": "user", "content": "hello"}}
        })).expect("should return Some");
        assert_eq!(got, "hello");
    }

    #[test]
    fn is_codex_message_event_only_true_for_event_msg_type_message() {
        assert!(!is_codex_message_event(&json!({"type": "session_meta"})));
        assert!(!is_codex_message_event(&json!({"type": "event_msg", "payload": {"type": "x"}})));
        assert!(is_codex_message_event(&json!({"type": "event_msg", "payload": {"type": "message"}})));
    }


    #[test]
    fn codex_session_id_from_first_line_reads_payload_id() {
        let content = "{\"type\":\"session_meta\",\"payload\":{\"id\":\"abc-123\",\"cwd\":\"/x\"}}\n{\"type\":\"event_msg\"}";
        assert_eq!(
            codex_session_id_from_first_line(content).as_deref(),
            Some("abc-123")
        );
        // 缺 payload.id → None
        assert!(codex_session_id_from_first_line("{}").is_none());
        // 非 JSON → None
        assert!(codex_session_id_from_first_line("not json").is_none());
        // 空内容 → None
        assert!(codex_session_id_from_first_line("").is_none());
    }

    #[test]
    fn iter_codex_rollout_files_returns_empty_for_missing_root() {
        let p = std::path::Path::new("/__no_such__/__ntd__/codex_root");
        assert!(iter_codex_rollout_files(p).is_empty());
}
}
