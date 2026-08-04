use std::sync::Arc;
use parking_lot::Mutex;

use super::helpers;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use super::claude_protocol::{ClaudeMessage, ClaudeContentBlock};
use crate::models::utc_timestamp;

/// Codebuddy executor。
///
/// 与 ClaudeCode 结构对称（path + model），统一通过 `BaseExecutor` 共享状态。
/// `session_id` 缓存从 system 事件提取的真实会话 ID：首次执行由 CodeBuddy CLI
/// 自己生成并随 system 事件吐出，resume 时由 DB 读出经 `--resume` 传回 CLI。
// `BaseExecutor` 已经 `#[derive(Clone)]`，组合字段无需手写 Clone impl；
// session_id 用 Arc 包裹，克隆体共享同一份缓存，与 claude_code 保持一致。
#[derive(Clone)]
pub struct CodebuddyExecutor {
    base: BaseExecutor,
    session_id: Arc<Mutex<Option<String>>>,
}

impl CodebuddyExecutor {
    pub fn new(path: String) -> Self {
        Self {
            base: BaseExecutor::new(path),
            session_id: Arc::new(Mutex::new(None)),
        }
    }

    /// 处理 system 事件：把 model / session_id 写入缓存，content 显示 session init 摘要。
    ///
    /// session_id 必须在此缓存：它是「继续对话」的唯一凭据，resume 链路在
    /// EventPipeline 回退路径（`update_session_id_once`）依赖本缓存回写 DB。
    fn handle_system(&self, model: Option<&String>, session_id: Option<&String>, subtype: Option<&String>) -> Option<ParsedLogEntry> {
        if let Some(m) = model {
            *self.base.model.lock() = Some(m.clone());
        }
        // 缓存 session_id：供 get_session_id / extract_session_id 复用，
        // resume 场景下 CLI 重吐同一 sid，重复写入值相同，无需判重。
        if let Some(sid) = session_id {
            *self.session_id.lock() = Some(sid.clone());
        }
        Some(helpers::entry("system", format!("Session init: {:?}", session_id.or(subtype))))
    }

    /// 处理 assistant 事件：把所有 block 串成一个 assistant 条目，
    /// 记录第一个 ToolUse 的 name/input 给前端展示。
    fn handle_assistant(&self, message: &super::claude_protocol::ClaudeMessageContent) -> Option<ParsedLogEntry> {
        let mut parts: Vec<String> = Vec::new();
        let mut first_tool_name: Option<String> = None;
        let mut first_tool_input_json: Option<String> = None;
        for block in &message.content {
            append_assistant_block(block, &mut parts, &mut first_tool_name, &mut first_tool_input_json);
        }
        if parts.is_empty() {
            None
        } else {
            Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "assistant".to_string(),
                content: parts.join("\n"),
                usage: None,
                tool_name: first_tool_name,
                tool_input_json: first_tool_input_json,
            })
        }
    }

    /// 处理 user 事件：通常只携带 ToolResult block；无匹配返回 None。
    fn handle_user(&self, message: &super::claude_protocol::ClaudeMessageContent) -> Option<ParsedLogEntry> {
        let parts: Vec<String> = message
            .content
            .iter()
            .filter_map(user_block_part)
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(ParsedLogEntry {
                timestamp: utc_timestamp(),
                log_type: "user".to_string(),
                content: parts.join("\n"),
                usage: None,
                tool_name: None,
                tool_input_json: None,
            })
        }
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

/// assistant block → 文本片段收集；首次 ToolUse 会额外捕获 name + input_json。
fn append_assistant_block(
    block: &ClaudeContentBlock,
    parts: &mut Vec<String>,
    first_tool_name: &mut Option<String>,
    first_tool_input_json: &mut Option<String>,
) {
    match block {
        ClaudeContentBlock::Thinking { thinking: Some(t) } => {
            parts.push(format!("[thinking] {}", t.chars().take(200).collect::<String>()));
        }
        ClaudeContentBlock::Text { text: Some(t) } => parts.push(t.clone()),
        ClaudeContentBlock::ToolUse { name, input, .. } => {
            let input_str = serde_json::to_string(input).unwrap_or_default();
            parts.push(format!(
                "[tool] {}: {}",
                name.as_deref().unwrap_or(""),
                input_str.chars().take(100).collect::<String>()
            ));
            if first_tool_name.is_none() {
                *first_tool_name = name.clone();
                *first_tool_input_json = Some(input_str);
            }
        }
        ClaudeContentBlock::ToolResult { content, is_error, .. } => {
            let err_str = if is_error.unwrap_or(false) { "[error] " } else { "" };
            parts.push(format!(
                "{}{}",
                err_str,
                content.as_deref().unwrap_or("").chars().take(100).collect::<String>()
            ));
        }
        ClaudeContentBlock::Redacted { redacted } => {
            parts.push(format!("[redacted] {}", redacted.as_deref().unwrap_or("")));
        }
        _ => {}
    }
}

/// user block → 文本片段（只关心 ToolResult）；其它 block 跳过。
fn user_block_part(block: &ClaudeContentBlock) -> Option<String> {
    if let ClaudeContentBlock::ToolResult { content, is_error, .. } = block {
        let err_str = if is_error.unwrap_or(false) { "[error] " } else { "" };
        Some(format!("{}{}", err_str, content.as_deref().unwrap_or("")))
    } else {
        None
    }
}

impl CodeExecutor for CodebuddyExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Codebuddy
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--permission-mode".to_string(),
            "bypassPermissions".to_string(),
            message.to_string(),
        ]
    }

    /// 带 session 的 argv 构造：resume 时在 stream-json 之后插入 `--resume <sid>`。
    ///
    /// 设计取舍（与 claude_code 对齐）：
    /// - 首次执行不传 `--session-id`，让 CLI 自生成真实 sid，再由 system 事件回收；
    ///   若外部强行指定 sid 可能与 CLI 内部生成规则冲突。
    /// - `is_resume=true` 但 sid 为 None 时静默降级为新会话：handler 层
    ///   （`resolve_resume_session_id`）已拦截 None 并返回 400，此处只是防御。
    /// - `--resume` 放在 stream-json 之后、`--verbose` 之前，与 claude_code 的 argv
    ///   布局保持一致，降低维护者对照两个适配器时的认知成本。
    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];
        if is_resume {
            if let Some(sid) = session_id {
                args.push("--resume".to_string());
                args.push(sid.to_string());
            }
        }
        args.push("--verbose".to_string());
        args.push("--permission-mode".to_string());
        args.push("bypassPermissions".to_string());
        args.push(message.to_string());
        args
    }

    /// CodeBuddy CLI 原生支持 `-r/--resume <sessionId>`（已在设计文档验证），
    /// 声明可恢复后 handler 层才会放行 resume 请求。
    fn supports_resume(&self) -> bool {
        true
    }

    /// 从 stdout 行提取 session_id：命中 system 事件则更新缓存并返回，
    /// 否则回退到已缓存值（handle_system 在 parse_output_line 路径写入）。
    /// EventPipeline 正常路径用不到本方法，它是 pipeline 无事件产出时的兜底。
    fn extract_session_id(&self, line: &str) -> Option<String> {
        if !line.is_empty() {
            // 两层匹配合并为一层：外层 JSON 解析，内层直接命中带 session_id 的 System 变体。
            if let Ok(ClaudeMessage::System { session_id: Some(sid), .. }) = serde_json::from_str::<ClaudeMessage>(line) {
                *self.session_id.lock() = Some(sid.clone());
                return Some(sid);
            }
        }
        self.session_id.lock().clone()
    }

    fn get_session_id(&self) -> Option<String> {
        self.session_id.lock().clone()
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        if line.is_empty() {
            return None;
        }
        if let Ok(msg) = serde_json::from_str::<ClaudeMessage>(line) {
            return match msg {
                ClaudeMessage::System { subtype, session_id, model } => {
                    self.handle_system(model.as_ref(), session_id.as_ref(), subtype.as_ref())
                }
                ClaudeMessage::Assistant { message, .. } => self.handle_assistant(&message),
                ClaudeMessage::User { message, .. } => self.handle_user(&message),
                ClaudeMessage::Result { result, is_error, duration_ms, total_cost_usd, usage, .. } => {
                    self.handle_result(result.as_deref(), is_error, duration_ms, total_cost_usd, usage.as_ref())
                }
            };
        }
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

    #[test]
    fn test_parse_output_line_system() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"system","model":"claude-3-5-sonnet"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "system");
        assert!(entry.content.contains("Session init"));
        assert_eq!(executor.get_model(), Some("claude-3-5-sonnet".to_string()));
    }

    #[test]
    fn test_parse_output_line_assistant_text() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "assistant");
        assert_eq!(entry.content, "hello");
    }

    #[test]
    fn test_parse_output_line_assistant_thinking() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"thinking..."}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "assistant");
        assert!(entry.content.starts_with("[thinking]"));
        assert!(entry.content.contains("thinking..."));
    }

    #[test]
    fn test_parse_output_line_user_tool_result() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"result","is_error":false}]}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "user");
        assert_eq!(entry.content, "result");
    }

    #[test]
    fn test_parse_output_line_result_success() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
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
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"result","result":"error","is_error":true}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "error");
        assert_eq!(entry.content, "[error] error");
    }

    #[test]
    fn test_parse_output_line_empty_line() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = "";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_raw_text_fallback() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
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
        assert!(usage.is_none(), "result type should not match tokens type");
    }

    #[test]
    fn test_usage_from_tokens_logs_no_logs() {
        let logs: Vec<ParsedLogEntry> = vec![];
        assert!(get_usage_from_tokens_logs(&logs).is_none());
    }

    #[test]
    fn test_get_model_before_system() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        assert!(executor.get_model().is_none());
    }

    // ====================== resume（继续对话）能力测试 ======================
    // 对应需求 058 R1：session_id 缓存 + supports_resume + command_args_with_session。

    #[test]
    fn test_supports_resume_true() {
        // CodeBuddy CLI 原生支持 --resume <sessionId>，必须为 true 才能通过 handler 层校验
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        assert!(executor.supports_resume());
    }

    #[test]
    fn test_command_args_with_session_resume_includes_resume_flag() {
        // resume 场景：argv 必须携带 `--resume <sid>`，且位于 stream-json 之后、--verbose 之前
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let args = executor.command_args_with_session("continue please", Some("sess_cb_1"), true);
        let pos_resume = args.iter().position(|a| a == "--resume").expect("should contain --resume");
        assert_eq!(args[pos_resume + 1], "sess_cb_1");
        let pos_format = args.iter().position(|a| a == "stream-json").unwrap();
        let pos_verbose = args.iter().position(|a| a == "--verbose").unwrap();
        assert!(pos_format < pos_resume && pos_resume < pos_verbose);
        // message 仍是最后一个参数，避免被 CLI 当作 flag 值
        assert_eq!(args.last().unwrap(), "continue please");
    }

    #[test]
    fn test_command_args_with_session_new_execution_ignores_sid() {
        // 新执行（is_resume=false）即使带了 sid 也不传 --resume / --session-id：
        // 首次执行让 CLI 自生成真实 sid，避免外部指定与内部生成规则冲突
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let args = executor.command_args_with_session("hello", Some("sess_cb_2"), false);
        assert!(!args.iter().any(|a| a == "--resume" || a == "--session-id"));
        assert!(!args.iter().any(|a| a == "sess_cb_2"));
    }

    #[test]
    fn test_command_args_with_session_resume_without_sid_degrades() {
        // 防御：is_resume=true 但 sid 为 None 时静默降级为新会话
        // （handler 层已拦截 None 返回 400，正常不会走到这里）
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let args = executor.command_args_with_session("hello", None, true);
        assert!(!args.iter().any(|a| a == "--resume"));
    }

    #[test]
    fn test_extract_session_id_from_system_line() {
        // system 事件携带 session_id：应返回并写入缓存
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"system","subtype":"init","session_id":"37814b2c-c93e-44ca-8462-bd7fc8d8105c","model":"m"}"#;
        assert_eq!(
            executor.extract_session_id(line),
            Some("37814b2c-c93e-44ca-8462-bd7fc8d8105c".to_string())
        );
        assert_eq!(
            executor.get_session_id(),
            Some("37814b2c-c93e-44ca-8462-bd7fc8d8105c".to_string())
        );
    }

    #[test]
    fn test_extract_session_id_fallback_to_cached() {
        // 非 system 行（如 assistant）不含 sid：回退到 handle_system 已缓存的值。
        // 这覆盖 log_capture 回退路径——pipeline 无事件产出时仍能把 sid 回写 DB。
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let sys = r#"{"type":"system","session_id":"sess_cached","model":"m"}"#;
        let _ = executor.parse_output_line(sys);
        let assistant = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}"#;
        assert_eq!(
            executor.extract_session_id(assistant),
            Some("sess_cached".to_string())
        );
    }

    #[test]
    fn test_extract_session_id_empty_line_before_system() {
        // system 事件未到达前：空行返回 None，不应误报一个不存在的 sid
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        assert_eq!(executor.extract_session_id(""), None);
    }

    #[test]
    fn test_get_session_id_before_system() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        assert!(executor.get_session_id().is_none());
    }

    #[test]
    fn test_parse_output_line_system_caches_session_id() {
        // parse_output_line → handle_system 的副作用：session_id 被缓存，
        // 同时 system 条目照常产生（维持现状的日志展示行为）
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let line = r#"{"type":"system","session_id":"sess_parse","model":"claude-3-5-sonnet"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "system");
        assert_eq!(executor.get_session_id(), Some("sess_parse".to_string()));
        assert_eq!(executor.get_model(), Some("claude-3-5-sonnet".to_string()));
    }
}
