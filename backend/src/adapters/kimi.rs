use super::helpers;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};

/// Kimi executor。
///
/// 内部使用 `BaseExecutor` 持有共享状态（path + model + usage），
/// Kimi 自身不维护额外的执行期状态，因此 `BaseExecutor` 的所有字段默认即可。
// `BaseExecutor` 已经 `#[derive(Clone)]`，组合字段无需手写 Clone impl。
#[derive(Clone)]
pub struct KimiExecutor {
    base: BaseExecutor,
}

impl KimiExecutor {
    pub fn new(path: String) -> Self {
        Self { base: BaseExecutor::new(path) }
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

    /// 收集 content[] 中的 text/think，按 text > thinking 优先级返回。
    fn parse_assistant_content(&self, json: &serde_json::Value) -> Option<ParsedLogEntry> {
        let items = json.get("content")?.as_array()?;
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

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let json = helpers::parse_json_line(line)?;
        // 非对象（Kimi 始终输出 JSON 对象）一律忽略，行为与原实现一致
        let role = json.get("role").and_then(|v| v.as_str())?;
        match role {
            "assistant" => self.parse_assistant(&json),
            "tool" => self.parse_tool_result(&json),
            // role="meta" 包含 session.resume_hint 等元事件，跳过（resume 提示由 parse_stderr_line 统一处理）
            "meta" => None,
            _ => None,
        }
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

    fn get_usage(&self, _logs: &[ParsedLogEntry]) -> Option<crate::adapters::ExecutionUsage> {
        self.base.usage.lock().clone()
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
    fn test_command_args_with_session() {
        let executor = KimiExecutor::new("kimi".to_string());
        let args = executor.command_args_with_session("continue task", Some("abc123"), false);
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
    fn test_parse_output_line_skip_resume() {
        let executor = KimiExecutor::new("kimi".to_string());
        let line = "To resume this session: kimi -r abc123";
        assert!(executor.parse_output_line(line).is_none());
    }
}
