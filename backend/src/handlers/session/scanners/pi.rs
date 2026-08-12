//! Pi 会话扫描器：scan_pi / get_pi_detail 及 JSONL 解析辅助族。
//! 096-W3-PR2 从 handlers/session.rs 逐字搬迁（Move Function），逻辑零改动。

use super::{home_dir, iter_jsonl_files, truncate_str};
use crate::handlers::session::{SessionDetail, SessionInfo, SessionMessage, SessionScanner};

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
/// 抽取单行 pi JSONL 并按 event_type 分派到对应 summary 更新函数。
/// 复用 build_pi_messages 里相同的 helper,保证两条扫描路径在 first_prompt、
/// model、tokens 等字段上的语义完全一致。
fn apply_pi_event(summary: &mut PiSessionSummary, event_type: &str, v: &serde_json::Value) {
    match event_type {
        "session" => apply_pi_session_event(summary, v),
        "model_change" if summary.model.is_none() => {
            summary.model = pi_model_from_change_event(v);
        }
        "message" => {
            // 跳过缺 message 字段的行(防御性,与原 match Some(m) 行为一致)
            let Some(msg) = v.get("message") else { return };
            summary.message_count += 1;
            // 首条 user message 的文本作为 first_prompt,后续不再覆盖
            if summary.first_prompt.is_none()
                && msg.get("role").and_then(|r| r.as_str()) == Some("user")
            {
                if let Some(text) = msg.get("content").and_then(extract_first_user_prompt_text) {
                    summary.first_prompt = Some(text);
                }
            }
            if let Some(usage) = msg.get("usage") {
                accumulate_pi_usage(summary, usage);
            }
            // 持续更新为最新一行 message 的 timestamp,符合旧实现语义
            if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
                summary.last_active_at = Some(ts.to_string());
            }
        }
        _ => {}
    }
}

fn summarize_pi_jsonl(content: &str) -> PiSessionSummary {
    let mut summary = PiSessionSummary::default();

    for line in content.lines() {
        if line.is_empty() { continue; }
        let Some((event_type, v)) = parse_pi_line(line) else { continue };
        apply_pi_event(&mut summary, &event_type, &v);
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

/// 把 SystemTime 转成「距 now 的秒数」;文件不可读 / mtime 不可取时返 u64::MAX。
/// 抽出是为了在 scan_pi / scan_X 三个地方复用同样的"now-relative age"计算,
/// 避免各自重写一遍 metadata + modified + chrono 转换链。
fn file_age_seconds(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            (chrono::Utc::now() - dt).num_seconds().max(0) as u64
        })
        .unwrap_or(u64::MAX)
}

/// pi session 文件名 = "<iso-ts>_<uuid>.jsonl",session_id 是后半 UUID 段。
/// 没有下划线分隔时退化为整个 stem,与 scan_pi 旧实现一致(防止编码缺失)。
fn pi_session_id_from_path(path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem().and_then(|s| s.to_str())?;
    match stem.rsplit_once('_') {
        Some((_ts, uuid)) => Some(uuid.to_string()),
        None => Some(stem.to_string()),
    }
}

/// 单个 pi JSONL 文件 -> SessionInfo。
/// 把 scan_pi 主循环里的 mtime / cwd 优先级 / last_active_at 优先级 / 0 字节跳过
/// 等逻辑收敛到一处;调用方只剩遍历 + 推入结果。
fn build_pi_session_info_from_file(
    fpath: &std::path::Path,
    decoded_cwd: &str,
) -> Option<SessionInfo> {
    let session_id = pi_session_id_from_path(fpath)?;
    let file_size = std::fs::metadata(fpath).map(|m| m.len()).unwrap_or(0);
    // 0 字节文件通常是断电/crash 残留,跳过后不会贡献空 session
    if file_size == 0 { return None; }

    let file_mtime = file_age_seconds(fpath);
    // mtime < 5min 视为 active;父目录 mtime 只在「文件增/删」时更新,
    // 不能反映文件内容修改,所以只看文件 mtime(原代码注释已说明此取舍)
    let is_active = file_mtime < PI_ACTIVE_WINDOW_SECONDS;

    let content = std::fs::read_to_string(fpath).ok()?;
    let summary = summarize_pi_jsonl(&content);

    // cwd 优先级:JSONL 首行 > 文件名反编码
    let cwd = summary.cwd.unwrap_or_else(|| decoded_cwd.to_string());
    // last_active_at 优先级:JSONL 最后事件时间戳 > 文件 mtime
    let last_active_at = summary
        .last_active_at
        .clone()
        .or_else(|| pi_mtime_to_rfc3339(fpath));

    Some(SessionInfo {
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
    })
}

/// 把单个项目目录下所有 *.jsonl 解析后追加到 sessions。
/// 抽离是为了让 scan_pi 主循环只剩「遍历项目目录」一层 for,
/// 嵌套深度压到 2 层。
fn collect_pi_sessions_in_project(project_dir: &std::path::Path, sessions: &mut Vec<SessionInfo>) {
    // 文件名格式: --Users-weibh-projects-rust-nothing-todo--(首尾各一个 -)
    let encoded = project_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let decoded_cwd = decode_pi_cwd(&encoded);
    for (fpath, _name) in iter_jsonl_files(project_dir) {
        if let Some(info) = build_pi_session_info_from_file(&fpath, &decoded_cwd) {
            sessions.push(info);
        }
    }
}

fn scan_pi(sessions: &mut Vec<SessionInfo>) {
    let root = home_dir().join(".pi/agent/sessions");
    if !root.exists() { return; }
    let Ok(project_dirs) = std::fs::read_dir(&root) else { return };

    // 外层循环遍历项目目录,内层全部委托给 helper,scan_pi 嵌套深度 ≤2 层。
    for project_entry in project_dirs.flatten().filter(|e| e.path().is_dir()) {
        collect_pi_sessions_in_project(&project_entry.path(), sessions);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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

/// 从 session JSONL 里抽取消息预览/usage/stop_reason 等用于详情面板。
///
/// 优先复用 scan_pi 用的 summarize 逻辑，但详情页面要 messages 数组而不仅是
/// 计数，所以这里另外走一遍 file。
/// 从 session 事件里抽取 cwd / created_at / version,使用「首次见到」策略。
/// 抽离是为了让 build_pi_messages 主循环压到 ≤30 行,同时复用给 summarize_pi_jsonl。
fn apply_pi_session_event(summary: &mut PiSessionSummary, v: &serde_json::Value) {
    if summary.cwd.is_none() {
        summary.cwd = v.get("cwd").and_then(|c| c.as_str()).map(String::from);
    }
    if summary.created_at.is_none() {
        summary.created_at = v.get("timestamp").and_then(|t| t.as_str()).map(String::from);
    }
    if summary.version.is_none() {
        summary.version = v.get("version").and_then(|v| v.as_u64()).map(|n| n.to_string());
    }
}

/// 从 model_change 事件里构造 "<provider>/<modelId>" 形式的 model 字符串。
/// 仅在 summary.model 尚未设置时生效——首条 model_change 决定整 session 的模型。
fn pi_model_from_change_event(v: &serde_json::Value) -> Option<String> {
    let model_id = v.get("modelId").and_then(|m| m.as_str()).map(String::from);
    let provider = v.get("provider").and_then(|p| p.as_str()).map(String::from);
    match (provider, model_id) {
        (Some(p), Some(m)) => Some(format!("{}/{}", p, m)),
        (_, Some(m)) => Some(m),
        (Some(p), None) => Some(p),
        _ => None,
    }
}

/// 把 message.content 数组里的 text / toolCall / toolResult 拼接成预览。
/// 单层 for + match,比原来嵌套的 and_then + if let 链更易读且避免 4 层嵌套。
fn format_pi_message_preview(content: &serde_json::Value) -> String {
    let Some(arr) = content.as_array() else { return String::new() };
    let mut pieces: Vec<String> = Vec::new();
    for block in arr {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    pieces.push(t.to_string());
                }
            }
            Some("toolCall") => {
                // 工具调用只展示 name,完整 arguments 可能很大不适合预览
                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                pieces.push(format!("[toolCall: {}]", name));
            }
            Some("toolResult") => {
                // 工具输出可能巨大,预览里只占位
                pieces.push("[toolResult]".to_string());
            }
            _ => {}
        }
    }
    pieces.join("\n")
}

/// 从 message.content 数组里取第一条 text block 作为 user prompt。
/// find_map 把嵌套的 Option 链压平,只用一层表达式表达。
fn extract_first_user_prompt_text(content: &serde_json::Value) -> Option<String> {
    content.as_array().and_then(|arr| {
        arr.iter().find_map(|c| c.get("text").and_then(|t| t.as_str()).map(String::from))
    })
}

/// 把 pi 的 usage 字段累加到 summary(pi 把 cache 命中量计入 input 等价物)。
/// cacheRead/cacheWrite 不存在时按 0 处理,符合 scan_pi 旧实现。
fn accumulate_pi_usage(summary: &mut PiSessionSummary, usage: &serde_json::Value) {
    let i = usage.get("input").and_then(|n| n.as_u64()).unwrap_or(0);
    let cr = usage.get("cacheRead").and_then(|n| n.as_u64()).unwrap_or(0);
    let cw = usage.get("cacheWrite").and_then(|n| n.as_u64()).unwrap_or(0);
    summary.total_input_tokens += i + cr + cw;
    if let Some(o) = usage.get("output").and_then(|n| n.as_u64()) {
        summary.total_output_tokens += o;
    }
}

/// 从 message 事件构造一条 SessionMessage 预览;同时累加 summary.message_count、
/// first_prompt、tokens、last_active_at 等可由本消息推导的字段。
/// 抽出来是为了把 build_pi_messages 主循环压到 ≤30 行。
fn build_pi_session_message(msg: &serde_json::Value, envelope_ts: Option<&str>) -> SessionMessage {
    let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();
    let preview = msg.get("content").map(format_pi_message_preview).unwrap_or_default();
    let input_tokens = msg.get("usage").and_then(|u| u.get("input")).and_then(|n| n.as_u64());
    let output_tokens = msg.get("usage").and_then(|u| u.get("output")).and_then(|n| n.as_u64());
    let stop_reason = msg.get("stopReason").and_then(|s| s.as_str()).map(String::from);
    let model = msg.get("model").and_then(|m| m.as_str()).map(String::from);
    let timestamp = envelope_ts.map(String::from);
    SessionMessage {
        role,
        content_preview: truncate_str(&preview, 500),
        model,
        input_tokens,
        output_tokens,
        timestamp,
        stop_reason,
    }
}

/// 处理一条 message 事件,既更新 summary 也产出对应的 SessionMessage 预览。
/// 抽离是为了把 build_pi_messages 主循环的 match 臂嵌套压平到 2 层。
fn process_pi_message_event(
    v: &serde_json::Value,
    summary: &mut PiSessionSummary,
    out: &mut Vec<SessionMessage>,
) {
    let Some(msg) = v.get("message") else { return };
    let envelope_ts = v.get("timestamp").and_then(|t| t.as_str());
    apply_pi_event(summary, "message", v);
    out.push(build_pi_session_message(msg, envelope_ts));
}

fn build_pi_messages(content: &str) -> (Vec<SessionMessage>, PiSessionSummary) {
    let mut messages: Vec<SessionMessage> = Vec::new();
    let mut summary = PiSessionSummary::default();

    for line in content.lines() {
        if line.is_empty() { continue; }
        let Some((event_type, v)) = parse_pi_line(line) else { continue };

        // 复用 summarize 的派发逻辑,保证两条路径在同一 JSONL 上的
        // summary 字段一致;唯一差异是 build_pi_messages 额外收集每条 message 的预览。
        match event_type.as_str() {
            "message" => process_pi_message_event(&v, &mut summary, &mut messages),
            _ => apply_pi_event(&mut summary, event_type.as_str(), &v),
        }
    }

    (messages, summary)
}

/// 在 project_dir 下找文件名匹配的 pi JSONL 并解析为 SessionDetail。
/// 抽离出"找文件 + 比对 session_id + 解析文件"三件事,get_pi_detail 主循环只剩
/// "遍历项目目录 + 用 helper"。
fn find_pi_detail_in_project(
    project_dir: &std::path::Path,
    session_id: &str,
) -> Option<SessionDetail> {
    // 在项目目录里按文件名 UUID 后缀找匹配的 jsonl
    let path = iter_jsonl_files(project_dir)
        .into_iter()
        .map(|(p, _)| p)
        .find(|p| pi_session_id_from_path(p).as_deref() == Some(session_id))?;
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let content = std::fs::read_to_string(&path).ok()?;
    let (messages, summary) = build_pi_messages(&content);

    // cwd 优先级:JSONL 首行 > 文件名反编码(同 scan_pi 策略)
    let project_path = summary.cwd.clone().unwrap_or_else(|| {
        let encoded = project_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        decode_pi_cwd(&encoded)
    });
    let last_active_at = summary.last_active_at.or_else(|| pi_mtime_to_rfc3339(&path));
    let model = summary.model.unwrap_or_else(|| "-".into());
    let first_prompt = summary.first_prompt.map(|p| truncate_str(&p, 200));
    let created_at = summary.created_at;
    let version = summary.version;

    Some(SessionDetail {
        info: SessionInfo {
            session_id: session_id.to_string(),
            source: "pi".to_string(),
            project_path,
            status: "completed".to_string(),
            executor: "pi".to_string(),
            model,
            git_branch: None,
            message_count: summary.message_count,
            total_input_tokens: summary.total_input_tokens,
            total_output_tokens: summary.total_output_tokens,
            first_prompt,
            created_at,
            last_active_at,
            file_size,
            version,
            subagent_count: 0,
        },
        messages,
        subagents: vec![],
    })
}

fn get_pi_detail(session_id: &str) -> Option<SessionDetail> {
    let root = home_dir().join(".pi/agent/sessions");
    if !root.exists() { return None; }
    let project_dirs = std::fs::read_dir(&root).ok()?;

    // 主循环只剩"遍历项目目录 + 委托给 helper",内层 if let Some 链全消失
    for project_dir in project_dirs.flatten().filter(|e| e.path().is_dir()) {
        if let Some(detail) = find_pi_detail_in_project(&project_dir.path(), session_id) {
            return Some(detail);
        }
    }
    None
}

/// 覆盖 issue #637 重构后新增的 helper:
/// - file_age_seconds / pi_session_id_from_path / format_pi_message_preview
/// - accumulate_pi_usage / apply_pi_session_event / pi_model_from_change_event
/// - parse_codex_session_meta / parse_codex_user_prompt / is_codex_message_event
/// - claude_meta_to_session_message / ClaudeAccumulator::absorb
/// - codex_session_id_from_first_line
/// 这些都是 issue 抽取出来的纯函数,值得单测覆盖以防回归。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod refactor_helpers_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn file_age_seconds_handles_missing_and_existing() {
        // 不存在的路径 → u64::MAX(防止减法下溢,与旧代码 .unwrap_or(u64::MAX) 一致)
        assert_eq!(
            file_age_seconds(std::path::Path::new("/__no_such__/__ntd__")),
            u64::MAX
        );
        // 真实文件 → 应该是有限且 < u64::MAX
        let tmp = std::env::temp_dir().join("__ntd_file_age__");
        std::fs::write(&tmp, b"x").unwrap();
        let age = file_age_seconds(&tmp);
        assert!(age < u64::MAX, "real file should yield finite age, got {age}");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn pi_session_id_from_path_splits_on_underscore() {
        let p = std::path::Path::new("/x/2026-06-11T01-44-54-108Z_019eb45a-8e5c-7cf4-95f9-787b5a83b0fa.jsonl");
        assert_eq!(
            pi_session_id_from_path(p).as_deref(),
            Some("019eb45a-8e5c-7cf4-95f9-787b5a83b0fa")
        );
        // 没有下划线时退化为整个 stem
        let p2 = std::path::Path::new("/x/justname.jsonl");
        assert_eq!(pi_session_id_from_path(p2).as_deref(), Some("justname"));
    }

    #[test]
    fn format_pi_message_preview_concatenates_text_toolcall_toolresult() {
        let v = json!([
            {"type": "text", "text": "hello"},
            {"type": "toolCall", "name": "bash"},
            {"type": "toolResult"},
            {"type": "unknown", "x": 1},
        ]);
        let preview = format_pi_message_preview(&v);
        assert!(preview.contains("hello"), "preview should include text body: {preview}");
        assert!(preview.contains("[toolCall: bash]"), "preview should label tool calls: {preview}");
        assert!(preview.contains("[toolResult]"), "preview should placeholder tool results: {preview}");
        // 顺序: text 在前,toolCall 在中,toolResult 在后
        assert!(preview.find("hello").unwrap() < preview.find("[toolCall: bash]").unwrap());
        assert!(preview.find("[toolCall: bash]").unwrap() < preview.find("[toolResult]").unwrap());
        // 非数组内容 → 空字符串
        assert_eq!(format_pi_message_preview(&json!("plain")), "");
        assert_eq!(format_pi_message_preview(&json!(null)), "");
    }

    #[test]
    fn accumulate_pi_usage_counts_input_and_cache_and_output() {
        let mut summary = PiSessionSummary::default();
        let usage = json!({
            "input": 10,
            "cacheRead": 100,
            "cacheWrite": 50,
            "output": 7,
        });
        accumulate_pi_usage(&mut summary, &usage);
        // 10 + 100 + 50 = 160
        assert_eq!(summary.total_input_tokens, 160);
        assert_eq!(summary.total_output_tokens, 7);
        // 累加:再喂一条只含 cacheWrite
        accumulate_pi_usage(&mut summary, &json!({"input": 0, "cacheWrite": 30, "output": 0}));
        assert_eq!(summary.total_input_tokens, 190);
        // 缺字段时按 0 处理,不 panic
        accumulate_pi_usage(&mut summary, &json!({}));
        assert_eq!(summary.total_input_tokens, 190);
    }

    #[test]
    fn apply_pi_session_event_uses_first_seen_strategy() {
        let mut s = PiSessionSummary::default();
        apply_pi_session_event(&mut s, &json!({"cwd": "/x", "timestamp": "t1", "version": 3}));
        assert_eq!(s.cwd.as_deref(), Some("/x"));
        assert_eq!(s.created_at.as_deref(), Some("t1"));
        assert_eq!(s.version.as_deref(), Some("3"));
        // 再次设置不同值,不覆盖
        apply_pi_session_event(&mut s, &json!({"cwd": "/y", "timestamp": "t2", "version": 5}));
        assert_eq!(s.cwd.as_deref(), Some("/x"));
        assert_eq!(s.created_at.as_deref(), Some("t1"));
        assert_eq!(s.version.as_deref(), Some("3"));
    }

    #[test]
    fn pi_model_from_change_event_formats_provider_and_model() {
        let v = json!({"provider": "anthropic", "modelId": "claude-opus-4"});
        assert_eq!(pi_model_from_change_event(&v).as_deref(), Some("anthropic/claude-opus-4"));
        // 缺 provider
        let v2 = json!({"modelId": "gpt-4"});
        assert_eq!(pi_model_from_change_event(&v2).as_deref(), Some("gpt-4"));
        // 缺 modelId
        let v3 = json!({"provider": "anthropic"});
        assert_eq!(pi_model_from_change_event(&v3).as_deref(), Some("anthropic"));
        // 全缺
        assert!(pi_model_from_change_event(&json!({})).is_none());
    }
}

/// 扫描器的零大小标记 struct——实例仅承载类型信息，
/// I/O 与解析逻辑即本文件的 `scan_pi` / `get_pi_detail`（096-W3-PR2 收编后不再转发）。
pub struct PiScanner;

impl SessionScanner for PiScanner {
    fn name(&self) -> &'static str { "pi" }
    fn scan(&self, out: &mut Vec<SessionInfo>) { scan_pi(out); }
    fn get_detail(&self, session_id: &str) -> Option<SessionDetail> { get_pi_detail(session_id) }
}
