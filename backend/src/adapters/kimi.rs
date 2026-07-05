use std::sync::Arc;
use parking_lot::Mutex;

use super::helpers;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};

/// Kimi executor。
///
/// 内部使用 `BaseExecutor` 持有共享状态（path + model），
/// 额外的 `session_id` 用于缓存从 meta 事件中提取的 session_id，
/// 供 `extract_session_id` / `get_session_id` 回退使用。
// `BaseExecutor` 已经 `#[derive(Clone)]`，`Arc<Mutex<...>>` 也派生 Clone。
#[derive(Clone)]
pub struct KimiExecutor {
    base: BaseExecutor,
    /// 缓存从 meta 事件中提取的 session_id，支持跨行回退和 resume 时回写 DB。
    session_id: Arc<Mutex<Option<String>>>,
}

impl KimiExecutor {
    pub fn new(path: String) -> Self {
        Self {
            base: BaseExecutor::new(path),
            session_id: Arc::new(Mutex::new(None)),
        }
    }

    /// 解析 assistant 角色：优先 tool_calls（首个匹配即返回），否则 text / thinking。
    fn parse_assistant(&self, json: &serde_json::Value) -> Option<ParsedLogEntry> {
        if let Some(entry) = self.parse_assistant_tool_call(json) {
            return Some(entry);
        }
        self.parse_assistant_content(json)
    }

    /// 提取 assistant.tool_calls[0].function 作为 tool_call 日志。
    fn parse_assistant_tool_call(&self, json: &serde_json::Value) -> Option<ParsedLogEntry> {
        let calls = json.get("tool_calls")?.as_array()?;
        for call in calls {
            let func = call.get("function")?;
            let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
            let args = func.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
            return Some(helpers::tool_call_entry(
                "tool_call",
                format!("Calling tool: {} with args: {}", name, args),
                name,
                Some(args.to_string()),
            ));
        }
        None
    }

    /// 收集 content 中的 text/think，按 text > thinking 优先级返回。
    ///
    /// content 可以是两种格式：
    /// - 字符串：`"content":"Hello world"`
    /// - 对象数组：`"content":[{"type":"text","text":"Hello"}]`
    fn parse_assistant_content(&self, json: &serde_json::Value) -> Option<ParsedLogEntry> {
        let content_val = json.get("content")?;
        // 字符串格式：直接作为 text
        if let Some(s) = content_val.as_str() {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            return Some(helpers::text_entry(trimmed.to_string()));
        }
        // 数组格式：遍历 text/think
        let items = content_val.as_array()?;
        let mut text: Option<String> = None;
        let mut think: Option<String> = None;
        for item in items {
            match item.get("type").and_then(|v| v.as_str()) {
                Some("text") => {
                    if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                        text = Some(t.to_string());
                    }
                }
                Some("think") => {
                    if let Some(t) = item.get("think").and_then(|v| v.as_str()) {
                        think = Some(t.to_string());
                    }
                }
                _ => {}
            }
        }
        text.map(|t| helpers::text_entry(t))
            .or_else(|| think.map(|t| helpers::entry("thinking", t)))
    }

    /// 解析 tool 角色的 content[0].text 作为 tool_result。
    fn parse_tool_result(&self, json: &serde_json::Value) -> Option<ParsedLogEntry> {
        let items = json.get("content")?.as_array()?;
        for item in items {
            if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    return Some(helpers::entry("tool_result", text));
                }
            }
        }
        None
    }
}

impl CodeExecutor for KimiExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Kimi
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "--output-format".to_string(),
            "stream-json".to_string(),
            "-p".to_string(),
            message.to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, _is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];
        if let Some(sid) = session_id {
            args.push("-r".to_string());
            args.push(sid.to_string());
        }
        args.push("-p".to_string());
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    /// 从 meta 事件的 JSON 行中提取 session_id，优先取当前行，否则回退到缓存值。
    fn extract_session_id(&self, line: &str) -> Option<String> {
        if let Some(json) = helpers::parse_json_line(line) {
            if json.get("role").and_then(|v| v.as_str()) == Some("meta") {
                if let Some(sid) = json.get("session_id").and_then(|v| v.as_str()) {
                    *self.session_id.lock() = Some(sid.to_string());
                    return Some(sid.to_string());
                }
            }
        }
        self.session_id.lock().clone()
    }

    fn get_session_id(&self) -> Option<String> {
        self.session_id.lock().clone()
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        // 先尝试解析 JSON 行（标准 NDJSON 格式）
        if let Some(json) = helpers::parse_json_line(line) {
            let role = json.get("role").and_then(|v| v.as_str())?;
            // role="meta" 事件中可能携带 session_id，缓存到 executor 状态中
            // （与 Claude Code/Hermès/Pi 的模式一致：提取后供后续回写 DB）。
            if role == "meta" {
                if let Some(sid) = json.get("session_id").and_then(|v| v.as_str()) {
                    *self.session_id.lock() = Some(sid.to_string());
                }
                return None;
            }
            return match role {
                "assistant" => self.parse_assistant(&json),
                "tool" => self.parse_tool_result(&json),
                _ => None,
            };
        }

        // 非 JSON 行：kimi 有时会在 NDJSON 之前直接输出纯文本结果
        // （例如命令执行的原样输出：date/whoami/ping 的结果）。
        // 回退到 text 类型条目，确保非 JSON 行不被静默丢弃。
        helpers::trimmed_non_empty(line).map(|t| helpers::text_entry(t))
    }

    fn parse_stderr_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = helpers::trimmed_non_empty(line)?;
        // 跳过 resume 提示行（不属于 stderr 内容）
        if trimmed.starts_with("To resume this session:") {
            return None;
        }

        // Classify stderr content by its nature
        let log_type = if trimmed.starts_with("[tool-streaming") {
            "tool"
        } else if trimmed.contains("error") || trimmed.contains("Error") || trimmed.contains("ERROR") || trimmed.contains("failed") || trimmed.contains("Failed") {
            "stderr"
        } else {
            "info"
        };
        Some(helpers::entry(log_type, trimmed))
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        let texts: Vec<String> = logs.iter()
            .filter(|l| l.log_type == "text")
            .map(|l| l.content.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        if texts.is_empty() {
            None
        } else {
            Some(texts.join("\n\n"))
        }
    }

    fn get_model(&self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_args() {
        let executor = KimiExecutor::new("kimi".to_string());
        let args = executor.command_args("do something");
        assert_eq!(args, vec!["--output-format", "stream-json", "-p", "do something"]);
    }

    #[test]
    fn test_command_args_only_prompt() {
        let executor = KimiExecutor::new("kimi".to_string());
        let args = executor.command_args("hello");
        assert_eq!(args, vec!["--output-format", "stream-json", "-p", "hello"]);
    }

    #[test]
    fn test_command_args_with_session() {
        let executor = KimiExecutor::new("kimi".to_string());
        let args = executor.command_args_with_session("continue task", Some("abc123"), false);
        assert_eq!(args, vec!["--output-format", "stream-json", "-r", "abc123", "-p", "continue task"]);
    }

    #[test]
    fn test_command_args_with_session_resume() {
        let executor = KimiExecutor::new("kimi".to_string());
        // 与普通 session 调用参数一致：kimi -r <session_id> -p <message>
        let args = executor.command_args_with_session("continue task", Some("abc123"), true);
        assert_eq!(args, vec!["--output-format", "stream-json", "-r", "abc123", "-p", "continue task"]);
    }

    #[test]
    fn test_executor_type() {
        let executor = KimiExecutor::new("kimi".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Kimi);
    }

    #[test]
    fn test_parse_output_line_assistant_text() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"assistant","content":[{"type":"text","text":"Hello world"}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_parse_output_line_tool_call_request() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"assistant","content":[],"tool_calls":[{"type":"function","id":"call_1","function":{"name":"Shell","arguments":"{\"command\":\"date\"}"}}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert!(entry.content.contains("Shell"));
    }

    #[test]
    fn test_parse_output_line_tool_result() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"tool","content":[{"type":"text","text":"Tue Apr 28 07:59:16 PDT 2026"}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert_eq!(entry.content, "Tue Apr 28 07:59:16 PDT 2026");
    }

    #[test]
    fn test_parse_output_line_non_json_fallback_text() {
        let executor = KimiExecutor::new("kimi".to_string());
        // 非 JSON 行回退为 text 类型，不再被静默丢弃
        let entry = executor.parse_output_line("Tue Jun 30 12:01:21 CST 2026").unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Tue Jun 30 12:01:21 CST 2026");
    }

    #[test]
    fn test_parse_output_line_non_json_empty_returns_none() {
        let executor = KimiExecutor::new("kimi".to_string());
        assert!(executor.parse_output_line("").is_none());
        assert!(executor.parse_output_line("   ").is_none());
    }

    #[test]
    fn test_parse_output_line_meta_ignored() {
        let executor = KimiExecutor::new("kimi".to_string());
        // role="meta" 的 JSON 行（resume_hint）仍然跳过
        let json = r#"{"role":"meta","type":"session.resume_hint","session_id":"abc","command":"kimi -r abc","content":"To resume this session: kimi -r abc"}"#;
        assert!(executor.parse_output_line(json).is_none());
    }

    #[test]
    fn test_parse_output_line_string_content() {
        let executor = KimiExecutor::new("kimi".to_string());
        // 实际 kimi 有时 content 为字符串而非数组
        let json = r#"{"role":"assistant","content":"我来执行这两个命令。"}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "我来执行这两个命令。");
    }

    #[test]
    fn test_parse_output_line_string_content_with_tool_calls() {
        let executor = KimiExecutor::new("kimi".to_string());
        // 实际 kimi 同时有字符串 content 和 tool_calls
        let json = r#"{"role":"assistant","content":"我来执行这两个命令。\n","tool_calls":[{"type":"function","id":"call_1","function":{"name":"TodoList","arguments":"{\"todos\":[{\"title\":\"task1\"}]}"}}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert!(entry.content.contains("TodoList"));
        assert!(entry.tool_input_json.is_some());
    }

    #[test]
    fn test_extract_session_id_from_meta_event() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"meta","type":"session.resume_hint","session_id":"kimi_abc123","command":"kimi -r kimi_abc123","content":"To resume this session: kimi -r kimi_abc123"}"#;
        // 通过 extract_session_id 提取
        assert_eq!(executor.extract_session_id(json), Some("kimi_abc123".to_string()));
    }

    #[test]
    fn test_extract_session_id_falls_back_to_cached() {
        let executor = KimiExecutor::new("kimi".to_string());
        let meta = r#"{"role":"meta","type":"session.resume_hint","session_id":"kimi_session_xyz"}"#;
        // 先通过 parse_output_line 缓存
        let _ = executor.parse_output_line(meta);
        // 后续任意行都能回退到缓存值
        assert_eq!(executor.extract_session_id(r#"{"role":"assistant","content":"hello"}"#), Some("kimi_session_xyz".to_string()));
    }

    #[test]
    fn test_extract_session_id_no_session_yet() {
        let executor = KimiExecutor::new("kimi".to_string());
        // meta 事件未到达时返回 None
        assert_eq!(executor.extract_session_id(r#"{"role":"assistant","content":"hello"}"#), None);
    }

    #[test]
    fn test_get_session_id_returns_cached() {
        let executor = KimiExecutor::new("kimi".to_string());
        let meta = r#"{"role":"meta","session_id":"kimi_cached_sid"}"#;
        let _ = executor.parse_output_line(meta);
        assert_eq!(executor.get_session_id(), Some("kimi_cached_sid".to_string()));
    }

    #[test]
    fn test_get_session_id_before_meta_returns_none() {
        let executor = KimiExecutor::new("kimi".to_string());
        assert_eq!(executor.get_session_id(), None);
    }
}
