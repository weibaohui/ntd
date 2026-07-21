use serde_json::Value;

use super::helpers;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use crate::models::ExecutionUsage;
use crate::models::utc_timestamp;

/// Codex executor。
///
/// Codex 的 `parse_stderr_line` 与默认实现不同：
///   - 默认：error 关键字 → "error" log_type；其他 → "stderr"
///   - Codex：error 关键字 → "stderr" log_type；其他 → "info"
///
/// 这种反向分类是历史行为，必须保留 override，不能直接删除该方法。
// `BaseExecutor` 已经 `#[derive(Clone)]`，组合字段无需手写 Clone impl。
#[derive(Clone)]
pub struct CodexExecutor {
    base: BaseExecutor,
}

impl CodexExecutor {
    pub fn new(path: String) -> Self {
        Self { base: BaseExecutor::new(path) }
    }

    /// item.started / item.completed：从顶层 json.item 提取 inner_type，再分发。
    fn handle_item_event(&self, event_type: &str, json: &Value) -> Option<ParsedLogEntry> {
        let item = json.get("item")?;
        let inner_type = item.get("type").and_then(Value::as_str)?;
        match (event_type, inner_type) {
            ("item.started", "command_execution") => Some(item_command_started(item)),
            ("item.completed", "command_execution") => Some(item_command_completed(item)),
            ("item.completed", "agent_message") => item_agent_message_text(item),
            _ => None,
        }
    }

    /// turn.completed / turn.started：comleted 优先尝试从 json 提取 usage；
    /// 若有 usage 返回 tokens 条目，否则回退为 step_finish 摘要。
    fn handle_turn_event(&self, event_type: &str, json: &Value) -> Option<ParsedLogEntry> {
        if event_type == "turn.completed" {
            if let Some(usage) = parse_usage(json) {
                return Some(self.tokens_entry_from_usage(usage));
            }
        }
        let log_type = if event_type == "turn.completed" { "step_finish" } else { "system" };
        Some(helpers::entry(
            log_type,
            format!("Codex {}", event_type.replace(['.', '_'], " ")),
        ))
    }

    /// 通用：把 model 写入 base.model（如果 event 内有 model / model_slug）。
    fn store_model_if_present(&self, event: &Value) {
        if let Some(model) = first_string(event, &["model", "model_slug"]) {
            *self.base.model.lock() = Some(model);
        }
    }

    /// 返回 tokens 日志条目（usage 通过 ParsedLogEntry.usage 字段传递）。
    fn tokens_entry_from_usage(&self, usage: ExecutionUsage) -> ParsedLogEntry {
        helpers::tokens_entry(
            format!("Tokens: input={}, output={}", usage.input_tokens, usage.output_tokens),
            usage,
        )
    }

    /// 通用 typed event 分发（agent_message / reasoning / tool_call / tool_result / ...）
    fn handle_typed_event(&self, event_type: &str, event: &Value) -> Option<ParsedLogEntry> {
        match event_type {
            "session_configured" | "task_started" => Some(helpers::entry(
                "system",
                format!("Codex {}", event_type.replace('_', " ")),
            )),
            "agent_message" | "agent_message_delta" | "assistant_message" => {
                typed_text_entry(event, "text", &["message", "delta", "text", "content"])
            }
            "agent_reasoning" | "agent_reasoning_delta" | "reasoning" | "reasoning_delta" => {
                typed_text_entry(event, "thinking", &["message", "delta", "text", "content"])
            }
            "exec_command_begin" | "tool_call_begin" | "tool_call" => Some(typed_tool_call(event)),
            "exec_command_end" | "tool_call_end" | "tool_result" => Some(typed_tool_result(event)),
            "task_complete" => Some(helpers::entry("step_finish", "Codex finished")),
            "error" => Some(typed_error(event)),
            _ => None,
        }
    }
}

/// 提取 (event_type, event) 二元组：
/// 优先新格式（顶层 type 字段），再回退到旧格式（msg.type 字段）。
/// 返回 None 时调用方应忽略该行。
fn extract_codex_event(json: &Value) -> Option<(&str, Value)> {
    if let Some(msg) = json.get("msg") {
        let typ = msg.get("type").and_then(Value::as_str)?;
        Some((typ, msg.clone()))
    } else if let Some(typ) = json.get("type").and_then(Value::as_str) {
        Some((typ, json.clone()))
    } else {
        None
    }
}

/// item.started + command_execution：tool_call 日志。
fn item_command_started(item: &Value) -> ParsedLogEntry {
    let command = item
        .get("command")
        .cloned()
        .and_then(|v| command_to_string(Some(v)))
        .unwrap_or_default();
    let tool_input_json = item.get("command").and_then(|v| serde_json::to_string(v).ok());
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: "tool_call".to_string(),
        content: format!("Executing command: {}", command),
        usage: None,
        tool_name: Some("command_execution".to_string()),
        tool_input_json,
    }
}

/// item.completed + command_execution：tool_result 日志（含 status / output / exit_code）。
fn item_command_completed(item: &Value) -> ParsedLogEntry {
    let output = item.get("aggregated_output").and_then(Value::as_str).unwrap_or_default();
    let exit_code = item.get("exit_code").and_then(Value::as_i64);
    let status = item.get("status").and_then(Value::as_str).unwrap_or("completed");

    let mut content = format!("[{}] ", status);
    if !output.is_empty() {
        content.push_str(output);
    }
    if let Some(code) = exit_code {
        content.push_str(&format!(" (exit={})", code));
    }

    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: "tool_result".to_string(),
        content,
        usage: None,
        tool_name: Some("command_execution".to_string()),
        tool_input_json: None,
    }
}

/// item.completed + agent_message：text 日志；空文本返回 None。
fn item_agent_message_text(item: &Value) -> Option<ParsedLogEntry> {
    let text = item.get("text").and_then(Value::as_str)?;
    if text.is_empty() {
        return None;
    }
    Some(helpers::text_entry(text))
}

/// typed event：text / thinking 日志的统一入口。
/// 缺失文本或空文本时返回 None（前端不渲染空消息）。
fn typed_text_entry(event: &Value, log_type: &str, keys: &[&str]) -> Option<ParsedLogEntry> {
    let text = first_string(event, keys)?;
    if text.is_empty() {
        return None;
    }
    Some(helpers::entry(log_type, text))
}

/// typed event：exec_command_begin / tool_call_begin / tool_call。
fn typed_tool_call(event: &Value) -> ParsedLogEntry {
    let tool_name = first_string(event, &["tool_name", "name"]).unwrap_or_else(|| "exec".to_string());
    let command = command_to_string(event.get("command").cloned())
        .or_else(|| first_string(event, &["cmd", "arguments", "input"]))
        .unwrap_or_default();
    let tool_input_json = event
        .get("command")
        .or_else(|| event.get("arguments"))
        .and_then(|v| serde_json::to_string(v).ok());
    let content = if command.is_empty() {
        format!("Calling tool: {}", tool_name)
    } else {
        format!("Calling tool: {} with args: {}", tool_name, command)
    };
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: "tool_call".to_string(),
        content,
        usage: None,
        tool_name: Some(tool_name),
        tool_input_json,
    }
}

/// typed event：exec_command_end / tool_call_end / tool_result。
/// 拼接 exit_code / stdout / stderr；全部缺失时回退为 "Tool finished"。
fn typed_tool_result(event: &Value) -> ParsedLogEntry {
    let stdout = first_string(event, &["stdout", "output", "result", "aggregated_output"]).unwrap_or_default();
    let stderr = first_string(event, &["stderr"]).unwrap_or_default();
    let exit_code = event.get("exit_code").and_then(Value::as_i64);
    let mut content = String::new();
    if let Some(code) = exit_code {
        content.push_str(&format!("exit_code={}", code));
    }
    if !stdout.is_empty() {
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&stderr);
    }
    if content.is_empty() {
        content = "Tool finished".to_string();
    }
    ParsedLogEntry {
        timestamp: utc_timestamp(),
        log_type: "tool_result".to_string(),
        content,
        usage: None,
        tool_name: None,
        tool_input_json: None,
    }
}

/// typed event：error 日志，优先取 message / error 字段，缺失时 fallback 到整个 JSON 序列化。
fn typed_error(event: &Value) -> ParsedLogEntry {
    let content = first_string(event, &["message", "error"]).unwrap_or_else(|| event.to_string());
    helpers::error_entry(content)
}

impl CodeExecutor for CodexExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Codex
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        let mut args = vec!["exec".to_string()];
        // 注入模型（codex 接受 -m <id>，如 o3）。
        if let Some(m) = self.base.model.lock().clone() {
            args.push("-m".to_string());
            args.push(m);
        }
        args.push("--json".to_string());
        args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
        args.push("--skip-git-repo-check".to_string());
        args.push(message.to_string());
        args
    }

    /// 执行前注入期望模型，写入 base.model，供 command_args 拼 -m。
    fn set_exec_model(&self, model: Option<String>) {
        *self.base.model.lock() = model;
    }

    fn command_args_with_session(&self, message: &str, _session_id: Option<&str>, _is_resume: bool) -> Vec<String> {
        self.command_args(message)
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let json = helpers::parse_json_line(line)?;

        // 拆分 event_type / event 上下文：
        // - 新格式：{"type":"...","item":{...}}（type 在顶层）
        // - 老格式：{"msg":{"type":"..."}}（type 在 msg 内）
        let (event_type, event) = extract_codex_event(&json)?;

        // item.* 事件：嵌套 item 内的 inner type
        if event_type == "item.started" || event_type == "item.completed" {
            return self.handle_item_event(event_type, &json);
        }
        // turn.* 事件：优先 usage，否则 step_finish
        if event_type == "turn.completed" || event_type == "turn.started" {
            return self.handle_turn_event(event_type, &json);
        }
        // thread.started 单独路径
        if event_type == "thread.started" {
            return Some(helpers::entry("system", "Codex thread started"));
        }
        // 通用路径：先存 model，再尝试 usage，再按 event_type 分发
        self.store_model_if_present(&event);
        if let Some(usage) = parse_usage(&event) {
            return Some(self.tokens_entry_from_usage(usage));
        }
        self.handle_typed_event(event_type, &event)
    }

    fn parse_stderr_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        Some(ParsedLogEntry {
            timestamp: utc_timestamp(),
            log_type: if trimmed.to_lowercase().contains("error") {
                "stderr".to_string()
            } else {
                "info".to_string()
            },
            content: trimmed.to_string(),
            usage: None,
            tool_name: None,
            tool_input_json: None,
        })
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        super::default_final_result_with_think_stripping(logs)
    }

    fn get_model(&self) -> Option<String> {
        self.base.model.lock().clone()
    }
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::to_string)
}

fn command_to_string(value: Option<Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        other => Some(other.to_string()),
    }
}

fn parse_usage(event: &Value) -> Option<ExecutionUsage> {
    let usage = event
        .get("usage")
        .or_else(|| event.get("token_usage"))
        .or_else(|| event.pointer("/info/total_token_usage"))?;

    let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

    if input_tokens == 0 && output_tokens == 0 {
        return None;
    }

    Some(ExecutionUsage {
        input_tokens,
        output_tokens,
        cache_read_input_tokens: usage.get("cached_input_tokens").and_then(|v| v.as_u64()),
        cache_creation_input_tokens: usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()),
        total_cost_usd: usage.get("total_cost_usd").and_then(|v| v.as_f64()),
        duration_ms: event.get("duration_ms").and_then(|v| v.as_u64()),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_command_args() {
        let executor = CodexExecutor::new("codex".to_string());
        let args = executor.command_args("say hello");
        assert_eq!(
            args,
            vec![
                "exec",
                "--json",
                "--dangerously-bypass-approvals-and-sandbox",
                "--skip-git-repo-check",
                "say hello"
            ]
        );
    }

    #[test]
    fn test_executor_type() {
        let executor = CodexExecutor::new("codex".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Codex);
    }

    #[test]
    fn test_parse_thread_started() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"thread.started","thread_id":"abc123"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "system");
        assert_eq!(entry.content, "Codex thread started");
    }

    #[test]
    fn test_parse_turn_started() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"turn.started"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "system");
        assert_eq!(entry.content, "Codex turn started");
    }

    #[test]
    fn test_parse_item_started_command_execution() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"/bin/zsh -lc date"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert_eq!(entry.tool_name, Some("command_execution".to_string()));
        assert!(entry.content.contains("Executing command:"));
        assert!(entry.content.contains("/bin/zsh"));
    }

    #[test]
    fn test_parse_item_completed_command_execution() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"item.completed","item":{"id":"item_0","type":"command_execution","command":"/bin/zsh -lc date","aggregated_output":"Fri May  1 03:33:39 PDT 2026\n","exit_code":0,"status":"completed"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert_eq!(entry.tool_name, Some("command_execution".to_string()));
        assert!(entry.content.contains("[completed]"));
        assert!(entry.content.contains("Fri May"));
        assert!(entry.content.contains("exit=0"));
    }

    #[test]
    fn test_parse_item_completed_agent_message() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"这是AI的回复"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "这是AI的回复");
    }

    #[test]
    fn test_parse_turn_completed_with_usage() {
        let executor = CodexExecutor::new("codex".to_string());
        let line = r#"{"type":"turn.completed","usage":{"input_tokens":46503,"cached_input_tokens":45824,"output_tokens":90,"reasoning_output_tokens":0}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tokens");
        let usage = entry.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, 46503);
        assert_eq!(usage.output_tokens, 90);
        assert_eq!(usage.cache_read_input_tokens, Some(45824));
    }
}
