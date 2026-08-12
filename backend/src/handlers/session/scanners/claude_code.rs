//! Claude Code 会话扫描器：scan_claude_code / get_claude_detail 及私有辅助族。
//! 096-W3-PR2 从 handlers/session.rs 逐字搬迁（Move Function），逻辑零改动。

use std::path::PathBuf;

use super::{extract_text_content, home_dir, iter_jsonl_files, truncate_str, ParsedSessionLine};
use crate::handlers::session::{SessionDetail, SessionInfo, SessionMessage, SessionScanner, SubAgentInfo};

fn decode_project_path(encoded: &str) -> String {
    let s = encoded.strip_prefix('-').unwrap_or(encoded);
    format!("/{}", s.replace('-', "/"))
}

fn parse_claude_line_metadata(
    line: &str,
) -> Option<ParsedSessionLine> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let msg_type = v.get("type")?.as_str()?;
    match msg_type {
        "user" => {
            // user 行携带分支/版本/entrypoint 等会话级元信息,但无 model/usage;
            // 缺失的 model 与 token 字段显式置 None,避免与 assistant 行的值混在一起。
            let timestamp = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
            let git_branch = v.get("gitBranch").and_then(|b| b.as_str()).map(String::from);
            let version = v.get("version").and_then(|v| v.as_str()).map(String::from);
            let entrypoint = v.get("entrypoint").and_then(|e| e.as_str()).map(String::from);
            let prompt = v.get("message").and_then(|m| m.get("content")).map(extract_text_content);
            Some(ParsedSessionLine {
                timestamp,
                model: None,
                git_branch,
                version,
                entrypoint,
                prompt,
                input_tokens: None,
                output_tokens: None,
                role: "user".into(),
            })
        }
        "assistant" => {
            // assistant 行核心是 model + usage;分支/版本/entrypoint 在该行不存在,
            // 故统一 None,下游扫描时保留最早一次见到的值。
            let timestamp = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
            let msg = v.get("message")?;
            let model = msg.get("model").and_then(|m| m.as_str()).map(String::from);
            let usage = msg.get("usage");
            let input_tokens = usage.and_then(|u| u.get("input_tokens")).and_then(|t| t.as_u64());
            let output_tokens = usage.and_then(|u| u.get("output_tokens")).and_then(|t| t.as_u64());
            Some(ParsedSessionLine {
                timestamp,
                model,
                git_branch: None,
                version: None,
                entrypoint: None,
                prompt: None,
                input_tokens,
                output_tokens,
                role: "assistant".into(),
            })
        }
        "queue-operation" if v.get("operation").and_then(|o| o.as_str()) == Some("enqueue") => {
            // queue-enqueue 行的 content 直接位于顶层,不走 message.content;
            // role 归一化为 "queue" 是为了和 user/assistant 平级比较,msg_count 计数不计入。
            let timestamp = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
            let prompt = v.get("content").and_then(|c| c.as_str()).map(String::from);
            Some(ParsedSessionLine {
                timestamp,
                model: None,
                git_branch: None,
                version: None,
                entrypoint: None,
                prompt,
                input_tokens: None,
                output_tokens: None,
                role: "queue".into(),
            })
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

            // 内层 *.jsonl 枚举用 iter_jsonl_files 收敛 read_dir + 扩展名守卫。
            for (path, _name) in iter_jsonl_files(&project_entry.path()) {
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
                    if let Some(meta) = parse_claude_line_metadata(line) {
                        // 显式字段访问消除位置心智模型:
                        // ts 出现就更新 last_ts,首次见到的作为 first_ts;
                        // model/branch/version/entry 同理取"首次见到"的策略,
                        // 与原 9 元组解构行为保持一致。
                        if first_ts.is_none() { first_ts = meta.timestamp.clone(); }
                        if meta.timestamp.is_some() { last_ts = meta.timestamp.clone(); }
                        if meta.model.is_some() { model = meta.model; }
                        if meta.git_branch.is_some() { git_branch = meta.git_branch; }
                        if meta.version.is_some() { version = meta.version; }
                        if meta.entrypoint.is_some() { executor = meta.entrypoint; }
                        if first_prompt.is_none() && meta.prompt.is_some() { first_prompt = meta.prompt; }
                        if meta.role == "user" || meta.role == "assistant" { msg_count += 1; }
                        if let Some(i) = meta.input_tokens { total_in += i; }
                        if let Some(o) = meta.output_tokens { total_out += o; }
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

/// 覆盖 issue #608:Claude Code 日志行解析改用强类型结构体后的全部 3 条 return 分支
/// (user / assistant / queue-operation) 与 4 条 negative 分支(无 type / 未知 type /
/// queue 非 enqueue / 非 JSON),确保字段映射与原 9 元组完全等价。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod claude_line_meta_tests {
    use super::*;

    /// 解析器对空字符串、纯文本、缺 type 字段、空 JSON 一律返 None,
    /// 避免在主扫描循环里因"看起来像 JSON 但语义无关"的行污染结果。
    #[test]
    fn parse_claude_line_metadata_returns_none_for_garbage() {
        assert!(parse_claude_line_metadata("").is_none());
        assert!(parse_claude_line_metadata("not json").is_none());
        assert!(parse_claude_line_metadata("{}").is_none(), "no type => None");
        assert!(parse_claude_line_metadata("{\"foo\":1}").is_none(), "unknown shape => None");
    }

    /// user 行:把 gitBranch/version/entrypoint 显式映射到 git_branch/version/entrypoint,
    /// 把 message.content 文本提取到 prompt;model 与 token 一律 None。
    #[test]
    fn parse_claude_line_metadata_extracts_user_fields() {
        let line = r#"{"type":"user","timestamp":"2026-06-15T10:00:00Z","gitBranch":"feat/x","version":"1.2.3","entrypoint":"sdk-py","message":{"role":"user","content":"hello"}}"#;
        let meta = parse_claude_line_metadata(line).expect("user line should parse");
        assert_eq!(meta.timestamp.as_deref(), Some("2026-06-15T10:00:00Z"));
        assert_eq!(meta.git_branch.as_deref(), Some("feat/x"));
        assert_eq!(meta.version.as_deref(), Some("1.2.3"));
        assert_eq!(meta.entrypoint.as_deref(), Some("sdk-py"));
        assert_eq!(meta.prompt.as_deref(), Some("hello"));
        // user 行不应携带 model / token,显式 None 让下游判定时无需 Option<...>.is_none() 推断
        assert!(meta.model.is_none());
        assert!(meta.input_tokens.is_none());
        assert!(meta.output_tokens.is_none());
        assert_eq!(meta.role, "user");
    }

    /// assistant 行:model + usage.input_tokens/output_tokens 三个字段从 message.usage 抽取;
    /// 分支/版本/entrypoint 一律 None(该行本就不携带会话级元信息)。
    #[test]
    fn parse_claude_line_metadata_extracts_assistant_fields() {
        let line = r#"{"type":"assistant","timestamp":"2026-06-15T10:00:01Z","message":{"role":"assistant","model":"claude-opus-4","usage":{"input_tokens":15,"output_tokens":44}}}"#;
        let meta = parse_claude_line_metadata(line).expect("assistant line should parse");
        assert_eq!(meta.timestamp.as_deref(), Some("2026-06-15T10:00:01Z"));
        assert_eq!(meta.model.as_deref(), Some("claude-opus-4"));
        assert_eq!(meta.input_tokens, Some(15));
        assert_eq!(meta.output_tokens, Some(44));
        // 保持与原元组解构一致的 None —— 即使数据真缺,扫描器走"首次见到"分支不会错。
        assert!(meta.git_branch.is_none());
        assert!(meta.version.is_none());
        assert!(meta.entrypoint.is_none());
        assert!(meta.prompt.is_none());
        assert_eq!(meta.role, "assistant");
    }

    /// queue-operation 行只在 operation=="enqueue" 时被采纳,content 直接位于顶层。
    /// role 归一化为 "queue",与 user/assistant 平级比较;扫描器 msg_count 不计入。
    #[test]
    fn parse_claude_line_metadata_handles_queue_operation() {
        let enqueue = r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-06-15T10:00:02Z","content":"queued prompt"}"#;
        let meta = parse_claude_line_metadata(enqueue).expect("enqueue line should parse");
        assert_eq!(meta.prompt.as_deref(), Some("queued prompt"));
        assert_eq!(meta.role, "queue");
        assert!(meta.model.is_none());
        assert!(meta.input_tokens.is_none());

        // dequeue 不应被采纳(原行为就是 None,只采纳 enqueue)
        let dequeue = r#"{"type":"queue-operation","operation":"dequeue","content":"x"}"#;
        assert!(parse_claude_line_metadata(dequeue).is_none());
    }

    /// 结构体字段命名自解释:与原 9 元组 `(ts, model, branch, ver, entry, content, inp, out, role)`
    /// 的位置一一对应,但通过具名字段访问,后续维护不需要再对照位置。
    /// 本测试通过 `debug_assert_eq!` 锁定整体形状,作为防回归快照。
    #[test]
    fn parsed_session_line_field_shape_is_stable() {
        let line = r#"{"type":"user","timestamp":"t","gitBranch":"b","version":"v","entrypoint":"e","message":{"content":"c"}}"#;
        let meta = parse_claude_line_metadata(line).expect("user line should parse");
        // 字段顺序由 struct 定义决定,这里用结构体字面量锁定期望值,后续重构改字段会编译期失败。
        let expected = ParsedSessionLine {
            timestamp: Some("t".into()),
            model: None,
            git_branch: Some("b".into()),
            version: Some("v".into()),
            entrypoint: Some("e".into()),
            prompt: Some("c".into()),
            input_tokens: None,
            output_tokens: None,
            role: "user".into(),
        };
        assert_eq!(meta, expected);
    }

    /// 显式 message.content 数组形态:Claude Code 实际日志里 content 可能是
    /// `[{type:"text", text:"..."}]`,验证 extract_text_content 被透传到 prompt。
    #[test]
    fn parse_claude_line_metadata_user_content_array() {
        let line = r#"{"type":"user","message":{"content":[{"type":"text","text":"你好"},{"type":"text","text":"世界"}]}}"#;
        let meta = parse_claude_line_metadata(line).expect("user array content should parse");
        // extract_text_content 用 \n 拼接多 text 块
        assert_eq!(meta.prompt.as_deref(), Some("你好\n世界"));
    }

    /// queue 角色不产生 SessionMessage（msg_count 不计入队列操作）
    #[test]
    fn claude_meta_to_session_message_skips_queue_role() {
        let user = ParsedSessionLine {
            timestamp: Some("t".into()),
            model: None,
            git_branch: None,
            version: None,
            entrypoint: None,
            prompt: Some("hi".into()),
            input_tokens: None,
            output_tokens: None,
            role: "user".into(),
        };
        let m = claude_meta_to_session_message(&user).expect("user should produce a message");
        assert_eq!(m.role, "user");
        assert!(m.content_preview.contains("hi"));

        // queue 行不计入 messages
        let mut queue = user.clone();
        queue.role = "queue".into();
        assert!(claude_meta_to_session_message(&queue).is_none());
    }

    #[test]
    fn claude_accumulator_absorb_first_seen_and_latest_seen() {
        let mut acc = ClaudeAccumulator::default();
        let m1 = ParsedSessionLine {
            timestamp: Some("t1".into()),
            model: Some("claude-opus-4".into()),
            git_branch: None,
            version: None,
            entrypoint: Some("sdk".into()),
            prompt: Some("first".into()),
            input_tokens: Some(10),
            output_tokens: Some(20),
            role: "user".into(),
        };
        acc.absorb(&m1);
        assert_eq!(acc.first_ts.as_deref(), Some("t1"));
        assert_eq!(acc.last_ts.as_deref(), Some("t1"));
        assert_eq!(acc.model.as_deref(), Some("claude-opus-4"));
        assert_eq!(acc.first_prompt.as_deref(), Some("first"));
        assert_eq!(acc.msg_count, 1);
        assert_eq!(acc.total_in, 10);
        assert_eq!(acc.total_out, 20);

        // 第二行:model/branch 等空 → 不覆盖;但 last_ts / total 更新
        let m2 = ParsedSessionLine {
            timestamp: Some("t2".into()),
            model: None,
            git_branch: None,
            version: None,
            entrypoint: None,
            prompt: None,
            input_tokens: Some(5),
            output_tokens: Some(3),
            role: "assistant".into(),
        };
        acc.absorb(&m2);
        assert_eq!(acc.first_ts.as_deref(), Some("t1")); // 不变
        assert_eq!(acc.last_ts.as_deref(), Some("t2")); // 更新为最新
        assert_eq!(acc.model.as_deref(), Some("claude-opus-4")); // 不变
        assert_eq!(acc.msg_count, 2);
        assert_eq!(acc.total_in, 15);
        assert_eq!(acc.total_out, 23);
    }
}

/// 在 claude-code projects 目录下找 session_id 对应的 jsonl 文件,并反推项目路径。
/// 找不到返回 None,get_claude_detail 直接返回 None,与原语义一致。
fn find_claude_jsonl_for_session(session_id: &str) -> Option<(PathBuf, String)> {
    let projects_dir = home_dir().join(".claude/projects");
    let Ok(entries) = std::fs::read_dir(&projects_dir) else { return None };
    for entry in entries.flatten() {
        let candidate = entry.path().join(format!("{}.jsonl", session_id));
        if candidate.exists() {
            let project_path = decode_project_path(&entry.file_name().to_string_lossy());
            return Some((candidate, project_path));
        }
    }
    None
}

/// 从 subagents 目录下收集所有 *.meta.json 的 agent_type / description。
/// 抽离是因为原本 5 层 if let Some / exists / extension 嵌套读起来很痛苦。
fn collect_claude_subagents(session_jsonl_path: &std::path::Path) -> Vec<SubAgentInfo> {
    let session_dir = session_jsonl_path.with_extension("");
    let subagents_dir = session_dir.join("subagents");
    let Ok(entries) = std::fs::read_dir(&subagents_dir) else { return Vec::new() };

    entries
        .flatten()
        .filter_map(|entry| {
            let p = entry.path();
            // subagent 元数据约定: <agent-id>.meta.json
            if p.extension().and_then(|e| e.to_str()) != Some("json") { return None; }
            let name = p.file_stem().and_then(|n| n.to_str())?;
            if !name.ends_with(".meta") { return None; }
            let c = std::fs::read_to_string(&p).ok()?;
            let meta: serde_json::Value = serde_json::from_str(&c).ok()?;
            Some(SubAgentInfo {
                agent_type: meta.get("agentType").and_then(|t| t.as_str()).unwrap_or("unknown").to_string(),
                description: meta.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                message_count: 0,
            })
        })
        .collect()
}

/// 单行 ParsedSessionLine -> 可选的 SessionMessage 预览。
/// user / assistant 角色才有预览,queue 角色跳过 msg_count 计数但仍参与元信息。
fn claude_meta_to_session_message(meta: &ParsedSessionLine) -> Option<SessionMessage> {
    if meta.role != "user" && meta.role != "assistant" { return None; }
    let preview = meta.prompt.as_ref().map(|p| truncate_str(p, 500)).unwrap_or_default();
    Some(SessionMessage {
        role: meta.role.clone(),
        content_preview: preview,
        model: meta.model.clone(),
        input_tokens: meta.input_tokens,
        output_tokens: meta.output_tokens,
        timestamp: meta.timestamp.clone(),
        stop_reason: None,
    })
}

/// 把 jsonl 全文解析成 (claude 累计元信息, messages)。
/// 用 fold 累加 first/last/model/branch 等「首次见到 / 最新见到」字段,
/// 比手写 10 个 mutable Option 干净。
fn parse_claude_messages(content: &str) -> (ClaudeAccumulator, Vec<SessionMessage>) {
    let mut acc = ClaudeAccumulator::default();
    let mut messages = Vec::new();
    for line in content.lines() {
        let Some(meta) = parse_claude_line_metadata(line) else { continue };
        acc.absorb(&meta);
        if let Some(msg) = claude_meta_to_session_message(&meta) {
            messages.push(msg);
        }
    }
    (acc, messages)
}

/// Claude Code 会话级元信息累加器,封装「首次见到 / 最新见到」的更新策略。
/// 抽出来避免 parse_claude_messages 里 10 个 if Some() = ... 的样板。
#[derive(Default)]
struct ClaudeAccumulator {
    first_ts: Option<String>,
    last_ts: Option<String>,
    model: Option<String>,
    git_branch: Option<String>,
    version: Option<String>,
    executor: Option<String>,
    first_prompt: Option<String>,
    msg_count: u32,
    total_in: u64,
    total_out: u64,
}

impl ClaudeAccumulator {
    /// 把一行 ParsedSessionLine 累加进当前累计状态。
    fn absorb(&mut self, meta: &ParsedSessionLine) {
        if self.first_ts.is_none() { self.first_ts = meta.timestamp.clone(); }
        if meta.timestamp.is_some() { self.last_ts = meta.timestamp.clone(); }
        if meta.model.is_some() { self.model = meta.model.clone(); }
        if meta.git_branch.is_some() { self.git_branch = meta.git_branch.clone(); }
        if meta.version.is_some() { self.version = meta.version.clone(); }
        if meta.entrypoint.is_some() { self.executor = meta.entrypoint.clone(); }
        if self.first_prompt.is_none() && meta.prompt.is_some() {
            self.first_prompt = meta.prompt.clone();
        }
        if meta.role == "user" || meta.role == "assistant" {
            self.msg_count += 1;
        }
        if let Some(i) = meta.input_tokens { self.total_in += i; }
        if let Some(o) = meta.output_tokens { self.total_out += o; }
    }
}

fn get_claude_detail(session_id: &str) -> Option<SessionDetail> {
    let (path, project_path) = find_claude_jsonl_for_session(session_id)?;
    let content = std::fs::read_to_string(&path).ok()?;
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let active_set = collect_claude_active_sessions();

    let (acc, messages) = parse_claude_messages(&content);
    let subagents = collect_claude_subagents(&path);

    Some(SessionDetail {
        info: SessionInfo {
            session_id: session_id.to_string(),
            source: "claudecode".to_string(),
            project_path,
            status: if active_set.contains(session_id) { "active".into() } else { "completed".into() },
            executor: acc.executor.unwrap_or_else(|| "unknown".into()),
            model: acc.model.unwrap_or_else(|| "-".into()),
            git_branch: acc.git_branch,
            message_count: acc.msg_count,
            total_input_tokens: acc.total_in,
            total_output_tokens: acc.total_out,
            first_prompt: acc.first_prompt.map(|p| truncate_str(&p, 200)),
            created_at: acc.first_ts,
            last_active_at: acc.last_ts,
            file_size,
            version: acc.version,
            subagent_count: subagents.len() as u32,
        },
        messages,
        subagents,
    })
}

/// 扫描器的零大小标记 struct——实例仅承载类型信息，
/// I/O 与解析逻辑即本文件的 `scan_claude_code` / `get_claude_detail`（096-W3-PR2 收编后不再转发）。
pub struct ClaudeCodeScanner;

impl SessionScanner for ClaudeCodeScanner {
    fn name(&self) -> &'static str { "claudecode" }
    fn scan(&self, out: &mut Vec<SessionInfo>) { scan_claude_code(out); }
    fn get_detail(&self, session_id: &str) -> Option<SessionDetail> { get_claude_detail(session_id) }
}
