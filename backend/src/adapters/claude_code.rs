use std::sync::Arc;
use parking_lot::Mutex;

use super::helpers;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use super::claude_protocol::{ClaudeMessage, ClaudeContentBlock};
use crate::models::utc_timestamp;

/// ClaudeCode executor。
///
/// 使用 `BaseExecutor` 持有 path + model。
/// `usage` 字段虽然未被本 executor 直接使用（claude_code 的 usage 走
/// `super::get_usage_from_logs` 从 result 事件提取），但 BaseExecutor 仍然保留这个
/// `Arc<Mutex<Option<ExecutionUsage>>>` 字段，方便与其他 executor 行为保持一致。
///
/// `session_id` 用于保存从 Claude Code system 事件中提取的真实 session_id，
/// 首次执行时由 Claude Code 自己生成，resume 时从 DB 读取。
// `BaseExecutor` 已经 `#[derive(Clone)]`，组合字段无需手写 Clone impl。
#[derive(Clone)]
pub struct ClaudeCodeExecutor {
    base: BaseExecutor,
    session_id: Arc<Mutex<Option<String>>>,
}

impl ClaudeCodeExecutor {
    pub fn new(path: String) -> Self {
        Self {
            base: BaseExecutor::new(path),
            session_id: Arc::new(Mutex::new(None)),
        }
    }

    /// 处理 system 事件：仅保留副作用（缓存 model / session_id），不产生日志条目。
    ///
    /// Claude Code 在一次执行里会连续吐出多条 system 事件——首条带 session_id/model，
    /// 后续重复携带相同 session_id。这些 "Session init" 摘要对用户无价值且会刷屏
    /// （实测一次执行能刷出十几条），因此既不入库也不推前端，只缓存 model / session_id
    /// 供 usage 统计与 resume 使用。这是 Claude Code 专有的精简策略。
    ///
    /// 注意：实时日志流优先走 EventPipeline（已丢弃 System 事件），但重复 system 行
    /// 在 pipeline 里只产出被丢弃的 System 事件 → results 为空 → 回退到 parse_output_line，
    /// 因此这里返回 None 是堵住回退路径刷屏的最后一道关。
    fn handle_system(&self, model: Option<&String>, session_id: Option<&String>) {
        // 缓存 model：completion 阶段 get_model / get_model_from_logs 会读取。
        if let Some(m) = model {
            *self.base.model.lock() = Some(m.clone());
        }
        // 缓存 session_id：用于后续回写 DB 和 resume 时传递给 Claude Code CLI。
        if let Some(sid) = session_id {
            *self.session_id.lock() = Some(sid.clone());
        }
    }

    /// 处理 assistant 事件：优先 tool_use → thinking → tool_result → text → redacted 顺序匹配。
    /// 第一个匹配的 block 直接返回对应日志条目，text block 会 join 所有 text 段。
    fn handle_assistant(&self, message: &super::claude_protocol::ClaudeMessageContent) -> Option<ParsedLogEntry> {
        // 收集 text blocks 用于 fallback；与原行为一致：所有 text 用 \n 连接
        let mut text_parts: Vec<&str> = Vec::new();
        for block in &message.content {
            match block {
                ClaudeContentBlock::ToolUse { name, input, .. } => return Some(tool_use_entry(name.as_ref(), input)),
                ClaudeContentBlock::Thinking { thinking: Some(t) } => return Some(thinking_entry(t)),
                ClaudeContentBlock::ToolResult { content, is_error, .. } => {
                    return Some(tool_result_entry(content.as_deref(), *is_error));
                }
                ClaudeContentBlock::Text { text: Some(t) } => text_parts.push(t.as_str()),
                ClaudeContentBlock::Redacted { redacted } => return Some(redacted_entry(redacted.as_deref())),
                _ => {}
            }
        }
        // text fallback：把所有 text blocks 用 \n 拼成单个 assistant 条目
        if !text_parts.is_empty() {
            return Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "assistant".to_string(),
                content: text_parts.join("\n"),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            });
        }
        None
    }

    /// 处理 user 事件：通常只携带 tool_result block；无匹配返回 None。
    fn handle_user(&self, message: &super::claude_protocol::ClaudeMessageContent) -> Option<ParsedLogEntry> {
        for block in &message.content {
            if let ClaudeContentBlock::ToolResult { content, is_error, .. } = block {
                return Some(tool_result_entry(content.as_deref(), *is_error));
            }
        }
        None
    }

    /// 处理 result 事件：组装 ExecutionUsage + final 文本/log_type。
    fn handle_result(
        &self,
        result: Option<&str>,
        is_error: bool,
        duration_ms: Option<u64>,
        total_cost_usd: Option<f64>,
        usage: Option<&crate::adapters::claude_protocol::ClaudeUsage>,
    ) -> Option<ParsedLogEntry> {
        let err_str = if is_error { "[error] " } else { "" };
        let result_str = result.unwrap_or_default();
        let usage = usage.map(|u| crate::models::ExecutionUsage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cache_read_input_tokens: u.cache_read_input_tokens,
            cache_creation_input_tokens: u.cache_creation_input_tokens,
            total_cost_usd,
            duration_ms,
        });
        Some(helpers::entry_with_usage(
            if is_error { "error" } else { "result" },
            format!("{}{}", err_str, result_str),
            usage,
        ))
    }
}

/// tool_use block：log_type="tool_use"，content 含工具名 + 截断 300 字符的 input。
fn tool_use_entry(name: Option<&String>, input: &serde_json::Value) -> ParsedLogEntry {
    let name_str = name.cloned().unwrap_or_else(|| "unknown".to_string());
    let input_str = serde_json::to_string(input).unwrap_or_default();
    let content = format!("调用工具: {} - {}", name_str, input_str.chars().take(300).collect::<String>());
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: "tool_use".to_string(),
        content,
        usage: None,
        tool_name: Some(name_str),
        tool_input_json: Some(input_str),
    }
}

/// thinking block：log_type="thinking"，content 截断 500 字符。
fn thinking_entry(t: &str) -> ParsedLogEntry {
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: "thinking".to_string(),
        content: t.chars().take(500).collect::<String>(),
        usage: None,
        tool_name: None,
        tool_input_json: None,
    }
}

/// tool_result block（assistant 或 user 上下文）：log_type="tool_result"，
/// is_error=true 时前缀 "[错误] "，content 截断 300 字符。
fn tool_result_entry(content: Option<&str>, is_error: Option<bool>) -> ParsedLogEntry {
    let err_str = if is_error.unwrap_or(false) { "[错误] " } else { "" };
    let body = content.unwrap_or("").chars().take(300).collect::<String>();
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: "tool_result".to_string(),
        content: format!("{}{}", err_str, body),
        usage: None,
        tool_name: None,
        tool_input_json: None,
    }
}

/// redacted block：log_type="assistant"，content 前缀 "[redacted] "。
fn redacted_entry(redacted: Option<&str>) -> ParsedLogEntry {
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: "assistant".to_string(),
        content: format!("[redacted] {}", redacted.unwrap_or("")),
        usage: None,
        tool_name: None,
        tool_input_json: None,
    }
}

impl CodeExecutor for ClaudeCodeExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Claudecode
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "--dangerously-skip-permissions".to_string(),
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            message.to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec!["--dangerously-skip-permissions".to_string()];
        // 执行前若注入了期望模型（来自 todo.model / executor.default_model），插 --model。
        // 紧跟首个 flag 之后、-p 之前：--model 是全局 flag，位置不影响解析，靠前更清晰。
        if let Some(m) = self.base.model.lock().clone() {
            args.push("--model".to_string());
            args.push(m);
        }
        args.push("-p".to_string());
        args.push("--output-format".to_string());
        args.push("stream-json".to_string());
        // 首次执行时不传 --session-id，让 Claude Code 自己生成 session_id，
        // 然后从 system 事件中提取真实 session_id 回写 DB。
        // resume 时传 --resume <session_id>，恢复之前的会话。
        if is_resume {
            if let Some(sid) = session_id {
                args.push("--resume".to_string());
                args.push(sid.to_string());
            }
        }
        args.push("--verbose".to_string());
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    /// 执行前注入期望模型，写入 base.model，供 command_args_with_session 拼 --model。
    /// 与 get_model 共用 base.model 字段：注入的是「期望值」，执行中会被输出事件覆盖为「真实值」。
    fn set_exec_model(&self, model: Option<String>) {
        *self.base.model.lock() = model;
    }

    /// 从 stdout 中提取 session_id。
    /// Claude Code 的 session_id 在 system 事件的 session_id 字段中输出。
    /// 提取成功后保存到 executor 状态，供后续回写 DB 使用。
    /// 若 line 为空或不含 session_id，则返回之前已缓存的值（handle_system 写入的）。
    fn extract_session_id(&self, line: &str) -> Option<String> {
        // 尝试从当前行解析新的 session_id
        if !line.is_empty() {
            // 把两层 if-let 合并为一层：外层解析 JSON，内层直接匹配 System { session_id: Some }
            if let Ok(ClaudeMessage::System { session_id: Some(sid), .. }) = serde_json::from_str::<ClaudeMessage>(line) {
                *self.session_id.lock() = Some(sid.clone());
                return Some(sid);
            }
        }
        // 回退：返回之前缓存的 session_id（由 handle_system 写入）
        self.session_id.lock().clone()
    }

    fn get_session_id(&self) -> Option<String> {
        self.session_id.lock().clone()
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        if line.is_empty() {
            return None;
        }
        // Try to parse as Claude NDJSON message
        if let Ok(msg) = serde_json::from_str::<ClaudeMessage>(line) {
            return match msg {
                // subtype（如 init）仅 Claude Code 内部使用，无需展示；system 事件整体
                // 不产生日志条目，避免 "Session init" 刷屏（详见 handle_system 注释）。
                ClaudeMessage::System { session_id, model, .. } => {
                    self.handle_system(model.as_ref(), session_id.as_ref());
                    None
                }
                ClaudeMessage::Assistant { message, .. } => self.handle_assistant(&message),
                ClaudeMessage::User { message, .. } => self.handle_user(&message),
                ClaudeMessage::Result { result, is_error, duration_ms, total_cost_usd, usage, .. } => {
                    self.handle_result(result.as_deref(), is_error, duration_ms, total_cost_usd, usage.as_ref())
                }
            };
        }
        // Fallback: treat as raw text
        Some(helpers::text_entry(line))
    }

    fn get_model(&self) -> Option<String> {
        self.base.model.lock().clone()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use crate::executor_service::completion::get_usage_from_tokens_logs;
    use crate::models::{ExecutionUsage, ParsedLogEntry};

    /// set_exec_model 注入模型后，command_args_with_session 应拼出 `--model <value>`；
    /// 未注入时不含 --model（保持升级前行为，向后兼容）。
    #[test]
    fn test_command_args_injects_model_when_set() {
        let exec = ClaudeCodeExecutor::new("claude".to_string());
        // 未注入模型：argv 不应含 --model。
        let args_none = exec.command_args_with_session("hello", None, false);
        assert!(!args_none.iter().any(|a| a == "--model"));
        // 注入后：argv 含 "--model" 紧跟模型名。
        exec.set_exec_model(Some("glm-5.2".to_string()));
        let args = exec.command_args_with_session("hello", None, false);
        let model_value = args.windows(2).find(|w| w[0] == "--model").map(|w| w[1].clone());
        assert_eq!(model_value.as_deref(), Some("glm-5.2"));
    }

    #[test]
    fn test_parse_output_line_system() {
        // system 事件不产生日志条目（避免 "Session init" 刷屏），但副作用仍生效：model 被缓存。
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"system","model":"claude-3-5-sonnet"}"#;
        assert!(executor.parse_output_line(line).is_none());
        assert_eq!(executor.get_model(), Some("claude-3-5-sonnet".to_string()));
    }

    #[test]
    fn test_parse_output_line_system_repeated_produces_no_entry() {
        // Claude Code 会连续吐多条 system 事件：每条都不应产生日志条目，
        // 验证 "Session init" 不会刷屏，同时 session_id / model 缓存仍生效。
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let sys = r#"{"type":"system","subtype":"init","session_id":"sess_rep","model":"m"}"#;
        assert!(executor.parse_output_line(sys).is_none());
        // 重复一次：仍不产生条目，且不破坏已缓存的 session_id / model。
        assert!(executor.parse_output_line(sys).is_none());
        assert_eq!(executor.get_session_id(), Some("sess_rep".to_string()));
        assert_eq!(executor.get_model(), Some("m".to_string()));
    }

    #[test]
    fn test_parse_output_line_assistant_text() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "assistant");
        assert_eq!(entry.content, "hello");
    }

    #[test]
    fn test_parse_output_line_assistant_thinking() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"thinking..."}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "thinking");
        assert_eq!(entry.content, "thinking...");
    }

    #[test]
    fn test_parse_output_line_user_tool_result() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"result","is_error":false}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert_eq!(entry.content, "result");
    }

    #[test]
    fn test_parse_output_line_result_success() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"result","result":"final","is_error":false,"duration_ms":100,"total_cost_usd":0.001,"usage":{"input_tokens":10,"output_tokens":20}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "result");
        assert_eq!(entry.content, "final");
        assert!(entry.usage.is_some());
        let usage = entry.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 20);
        assert_eq!(usage.duration_ms, Some(100));
        assert_eq!(usage.total_cost_usd, Some(0.001));
    }

    #[test]
    fn test_parse_output_line_result_error() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"result","result":"error","is_error":true}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "error");
        assert_eq!(entry.content, "[error] error");
    }

    #[test]
    fn test_parse_output_line_empty_line() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = "";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_raw_text_fallback() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = "just text";
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "just text");
    }

    #[test]
    fn test_usage_from_tokens_logs() {
        let logs = vec![
            ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "result".to_string(),
                content: "final".to_string(),
                usage: Some(ExecutionUsage {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    total_cost_usd: Some(0.001),
                    duration_ms: Some(100),
                }),
            tool_name: None,
            tool_input_json: None,
            },
        ];
        let usage = get_usage_from_tokens_logs(&logs);
        assert!(usage.is_none(), "should be None: result type, not tokens type");
    }

    #[test]
    fn test_usage_from_tokens_logs_no_logs() {
        let logs: Vec<ParsedLogEntry> = vec![];
        assert!(get_usage_from_tokens_logs(&logs).is_none());
    }

    #[test]
    fn test_get_model_before_system() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        assert!(executor.get_model().is_none());
    }

    #[test]
    fn test_extract_session_id_before_system() {
        // system 事件未到达时返回 None
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#;
        assert_eq!(executor.extract_session_id(line), None);
    }

    #[test]
    fn test_extract_session_id_from_system() {
        // system 事件携带 session_id，handle_system 写入后 extract_session_id 能拿到
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"system","session_id":"sess_abc123","model":"claude-3-5-sonnet"}"#;
        // 走 parse_output_line 触发 handle_system 写入
        let _ = executor.parse_output_line(line);
        assert_eq!(
            executor.extract_session_id(""),
            Some("sess_abc123".to_string())
        );
    }

    #[test]
    fn test_extract_session_id_system_without_sid() {
        // system 事件没有 session_id 字段时保持 None
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let line = r#"{"type":"system","model":"claude-3-5-sonnet"}"#;
        let _ = executor.parse_output_line(line);
        assert_eq!(executor.extract_session_id(""), None);
    }

    #[test]
    fn test_extract_session_id_empty_line() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        assert_eq!(executor.extract_session_id(""), None);
    }

    #[test]
    fn test_extract_session_id_is_stable_across_lines() {
        // 第一次解析到 system 写入 sid 后，后续任意行（哪怕是 assistant）继续返回同一 sid
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let sys = r#"{"type":"system","session_id":"sess_xyz","model":"m"}"#;
        let _ = executor.parse_output_line(sys);
        let assistant = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}"#;
        let _ = executor.parse_output_line(assistant);
        assert_eq!(
            executor.extract_session_id(assistant),
            Some("sess_xyz".to_string())
        );
    }
}
