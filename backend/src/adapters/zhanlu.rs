use std::sync::Arc;
use parking_lot::Mutex;

use super::helpers;
use super::zhanlu_event::ZhanluAgentEvent;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use crate::models::ExecutionUsage;
use crate::models::utc_timestamp;

/// Zhanlu executor。
///
/// 与 Opencode 完全等价：相同的命令行参数、相同的 JSON 输出格式、相同的退出码语义。
/// Issue #673 明确要求「行为跟 opencode 完全一致，执行命令也一致，返回输出 json 格式也一致」，
/// 但因为后续 Zhanlu 可能演化出独立行为，所以不复用 `opencode.rs`，而是完整复制一份。
///
/// `has_successful_finish` 与 Opencode 一样承担「非零退出码但有 finish 事件就算成功」的语义，
/// `session_id` 用于缓存从 JSON 事件中提取的 session_id。
#[derive(Clone)]
pub struct ZhanluExecutor {
    base: BaseExecutor,
    has_successful_finish: Arc<Mutex<bool>>,
    /// 缓存从 JSON 事件中提取的 session_id，支持跨行回退和 resume 时回写 DB。
    session_id: Arc<Mutex<Option<String>>>,
}

impl ZhanluExecutor {
    pub fn new(path: String) -> Self {
        Self {
            base: BaseExecutor::new(path),
            has_successful_finish: Arc::new(Mutex::new(false)),
            session_id: Arc::new(Mutex::new(None)),
        }
    }

    /// 更新 session_id 缓存（extract_session_id 和 parse_output_line 共用）。
    fn update_session_id_cache(&self, sid: Option<String>) {
        if let Some(ref s) = sid {
            *self.session_id.lock() = Some(s.clone());
        }
    }

    /// 把 Zhanlu 时间戳（毫秒）转换为 ISO 字符串；缺失时回退到 utc_timestamp。
    fn resolve_timestamp(ts: Option<u64>) -> String {
        ts.and_then(|ts| chrono::DateTime::from_timestamp_millis(ts as i64))
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
            .unwrap_or_else(utc_timestamp)
    }

    /// step_start / step-start: 重置 has_successful_finish + usage。
    fn handle_step_start(&self, timestamp: &str) -> Option<ParsedLogEntry> {
        *self.has_successful_finish.lock() = false;
        Some(helpers::with_timestamp(helpers::entry("step_start", "Step started"), timestamp))
    }

    /// tool_use / tool-use: bash 工具显示 description + output，其它工具显示 tool + description。
    fn handle_tool_use(
        &self,
        part: &super::zhanlu_event::ZhanluAgentPart,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
        let tool = part.tool.clone().unwrap_or_default();
        let status = part.state.as_ref().and_then(|s| s.status.clone()).unwrap_or_default();
        let description = part.state.as_ref().and_then(|s| s.input.as_ref().and_then(|i| i.description.clone())).unwrap_or_default();
        let input_json = part.state.as_ref()
            .and_then(|s| s.input.as_ref())
            .map(|i| i.to_full_json());

        // bash 特殊渲染：把命令输出也带上
        let content = if tool == "bash" {
            match &part.state.as_ref().and_then(|s| s.output.clone()) {
                Some(output) => format!("[{}] {}: {}", status, description, output),
                None => format!("[{}] {}", status, description),
            }
        } else {
            format!("[{}] Tool: {} - {}", status, tool, description)
        };

        Some(helpers::with_timestamp(
            helpers::entry_with_optional_tool("tool", content, Some(tool), input_json),
            timestamp,
        ))
    }

    /// text: 空文本返回 None，否则返回 text 日志。
    fn handle_text(
        &self,
        part: &super::zhanlu_event::ZhanluAgentPart,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
        let text = part.text.clone().unwrap_or_default();
        if text.is_empty() {
            return None;
        }
        Some(helpers::with_timestamp(helpers::text_entry(text), timestamp))
    }

    /// step_finish / step-finish: 标记 has_successful_finish，从 tokens 提取 usage。
    fn handle_step_finish(
        &self,
        event: &ZhanluAgentEvent,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
        // Mark as successfully finished — zhanlu returns non-zero exit code
        // even on successful execution, so we track success via the event stream.
        *self.has_successful_finish.lock() = true;
        // Store usage info if available
        let usage = if let Some(part) = &event.part {
            if let Some(tokens) = &part.tokens {
                Some(ExecutionUsage {
                    input_tokens: tokens.input,
                    output_tokens: tokens.output,
                    cache_read_input_tokens: if tokens.cache.read > 0 { Some(tokens.cache.read) } else { None },
                    cache_creation_input_tokens: if tokens.cache.write > 0 { Some(tokens.cache.write) } else { None },
                    total_cost_usd: part.cost,
                    duration_ms: None,
                })
            } else { None }
        } else { None };
        Some(helpers::with_timestamp(helpers::entry_with_usage("step_finish", "Step finished", usage), timestamp))
    }
}

impl CodeExecutor for ZhanluExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Zhanlu
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--dangerously-skip-permissions".to_string(),
            message.to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        // Resume mode: use -s to specify existing session
        if is_resume {
            if let Some(sid) = session_id {
                args.push("-s".to_string());
                args.push(sid.to_string());
            }
        }
        args.push("--dangerously-skip-permissions".to_string());
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    fn extract_session_id(&self, line: &str) -> Option<String> {
        let event: ZhanluAgentEvent = serde_json::from_str(line).ok()?;
        let sid = event.session_id.or_else(|| event.part.as_ref()?.session_id.clone());
        self.update_session_id_cache(sid.clone());
        sid.or_else(|| self.session_id.lock().clone())
    }

    fn get_session_id(&self) -> Option<String> {
        self.session_id.lock().clone()
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let event: ZhanluAgentEvent = serde_json::from_str(line).ok()?;
        // 缓存 session_id：优先取事件顶层字段，再取 part 内字段
        let sid = event.session_id.clone().or_else(|| event.part.as_ref()?.session_id.clone());
        self.update_session_id_cache(sid);
        let timestamp = Self::resolve_timestamp(event.timestamp);

        match event.event_type.as_str() {
            "step_start" | "step-start" => self.handle_step_start(&timestamp),
            "tool_use" | "tool-use" => self.handle_tool_use(event.part.as_ref()?, &timestamp),
            "text" => self.handle_text(event.part.as_ref()?, &timestamp),
            "step_finish" | "step-finish" => self.handle_step_finish(&event, &timestamp),
            _ => None,
        }
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        super::default_final_result_with_think_stripping(logs)
    }

    fn get_model(&self) -> Option<String> {
        self.base.model.lock().clone()
    }

    fn check_success(&self, exit_code: i32) -> bool {
        if exit_code == 0 {
            return true;
        }
        // zhanlu returns non-zero exit codes (e.g. 144) even on successful execution,
        // similar to opencode. Trust the presence of a step_finish event in the output stream.
        *self.has_successful_finish.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ParsedLogEntry;

    #[test]
    fn test_parse_output_line_step_start() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert_eq!(entry.content, "Step started");
    }

    #[test]
    fn test_parse_output_line_tool_use_bash() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"tool_use","timestamp":1700000000000,"part":{"type":"tool_use","tool":"bash","state":{"status":"success","input":{"description":"list files"},"output":"file.txt"}}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool");
        assert!(entry.content.contains("success"), "content should contain status: {}", entry.content);
        assert!(entry.content.contains("list files"), "content should contain description: {}", entry.content);
        assert!(entry.content.contains("file.txt"), "content should contain output: {}", entry.content);
    }

    #[test]
    fn test_parse_output_line_text() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"part":{"type":"text","text":"hello world"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "hello world");
    }


    #[test]
    fn test_parse_output_line_unknown_type() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"unknown","timestamp":1700000000000}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_invalid_json() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = "not json";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_empty_text() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"part":{"type":"text","text":""}}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_get_final_result_with_text() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  hello world  "),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello world".to_string()));
    }

    #[test]
    fn test_get_final_result_fallback_to_stderr() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let logs = vec![
            ParsedLogEntry::new("stderr", "error output"),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("error output".to_string()));
    }

    #[test]
    fn test_get_final_result_empty_logs() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let logs: Vec<ParsedLogEntry> = vec![];
        assert!(executor.get_final_result(&logs).is_none());
    }


    #[test]
    fn test_get_model_always_none() {
        let executor = ZhanluExecutor::new("zl".to_string());
        assert!(executor.get_model().is_none());
    }

    #[test]
    fn test_check_success_exit_code_zero() {
        let executor = ZhanluExecutor::new("zl".to_string());
        assert!(executor.check_success(0));
    }

    #[test]
    fn test_check_success_non_zero_without_step_finish() {
        let executor = ZhanluExecutor::new("zl".to_string());
        assert!(!executor.check_success(144));
        assert!(!executor.check_success(1));
    }

    #[test]
    fn test_check_success_non_zero_with_step_finish() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"step_finish","timestamp":1700000000000,"part":{"type":"step_finish","tokens":{"total":100,"input":50,"output":50,"cache":{"read":10,"write":5}},"cost":0.001}}"#;
        let _ = executor.parse_output_line(line);
        assert!(executor.check_success(144), "should succeed when step_finish was parsed even with non-zero exit code");
    }

    // Tests for the hyphenated type names (e.g., step-start, tool-use)
    #[test]
    fn test_parse_output_line_step_start_hyphenated() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"step-start","timestamp":1700000000000,"sessionID":"ses_xxx"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert_eq!(entry.content, "Step started");
    }

    #[test]
    fn test_parse_output_line_tool_use_hyphenated() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"tool-use","timestamp":1700000000000,"part":{"type":"tool","tool":"bash","state":{"status":"completed","input":{"description":"list files"},"output":"file.txt"}}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool");
        assert!(entry.content.contains("completed"), "content should contain status: {}", entry.content);
    }

    #[test]
    fn test_parse_output_line_step_finish_hyphenated() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"step-finish","timestamp":1700000000000,"part":{"type":"step-finish","reason":"stop","tokens":{"total":100,"input":50,"output":50,"reasoning":0,"cache":{"read":10,"write":5}},"cost":0.001}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_finish");
        assert_eq!(entry.content, "Step finished");
    }

    #[test]
    fn test_check_success_with_step_finish_hyphenated() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"step-finish","timestamp":1700000000000,"part":{"type":"step-finish","tokens":{"total":100,"input":50,"output":50,"cache":{"read":10,"write":5}},"cost":0.001}}"#;
        let _ = executor.parse_output_line(line);
        assert!(executor.check_success(144), "should succeed when step-finish was parsed even with non-zero exit code");
    }

    #[test]
    fn test_parse_actual_zhanlu_json_format() {
        // Test with actual zhanlu output format (hyphenated types, sessionID)
        // 这是与 opencode 完全一致的 JSON 结构——Issue #673 要求 zhanlu 输出格式
        // 与 opencode 完全相同，所以这里使用的字段名/类型名都按 opencode 风格。
        let executor = ZhanluExecutor::new("zl".to_string());

        // Step start
        let line = r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_xxx"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");

        // Text output
        let line = r#"{"type":"text","timestamp":1777471505165,"sessionID":"ses_xxx","part":{"type":"text","text":"Hello, this is a test response"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello, this is a test response");

        // Step finish
        let line = r#"{"type":"step-finish","timestamp":1777471505168,"sessionID":"ses_xxx","part":{"type":"step-finish","reason":"stop","tokens":{"total":14155,"input":13862,"output":293,"reasoning":0,"cache":{"write":0,"read":0}},"cost":0}}"#;
        let _ = executor.parse_output_line(line);

        // Verify final result extraction
        let logs = vec![
            ParsedLogEntry::new("step_start", "Step started"),
            ParsedLogEntry::new("text", "Hello, this is a test response"),
            ParsedLogEntry::new("step_finish", "Step finished"),
        ];
        let result = executor.get_final_result(&logs);
        assert_eq!(result, Some("Hello, this is a test response".to_string()));
    }

    #[test]
    fn test_extract_session_id_caches_result() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"step-start","sessionID":"zl_sess_abc"}"#;
        let sid = executor.extract_session_id(line);
        assert_eq!(sid, Some("zl_sess_abc".to_string()));
        // 再次调用仍能获取缓存值
        assert_eq!(executor.extract_session_id(r#"{"type":"text"}"#), Some("zl_sess_abc".to_string()));
    }

    #[test]
    fn test_get_session_id_returns_cached() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"step-start","sessionID":"zl_session_xyz"}"#;
        let _ = executor.parse_output_line(line);
        assert_eq!(executor.get_session_id(), Some("zl_session_xyz".to_string()));
    }

    #[test]
    fn test_get_session_id_before_any_event() {
        let executor = ZhanluExecutor::new("zl".to_string());
        assert_eq!(executor.get_session_id(), None);
    }

    /// 验证 zhanlu 的 executor_type 与 ExecutorType 枚举保持一致，
    /// 防止以后修改 ExecutorType::Zhanlu 名字时忘记同步此处。
    #[test]
    fn test_executor_type_is_zhanlu() {
        let executor = ZhanluExecutor::new("zl".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Zhanlu);
        assert_eq!(ExecutorType::Zhanlu.as_str(), "zhanlu");
    }

    /// zhanlu 与 opencode 行为一致：相同的命令行参数结构，确保 Issue #673
    /// 「执行命令也一致」要求被满足。
    #[test]
    fn test_command_args_match_opencode_shape() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let args = executor.command_args("hello");
        assert_eq!(args[0], "run");
        assert_eq!(args[1], "--format");
        assert_eq!(args[2], "json");
        assert_eq!(args[3], "--dangerously-skip-permissions");
        assert_eq!(args[4], "hello");
    }
}