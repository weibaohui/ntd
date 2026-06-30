//! MiMo executor adapter
//!
//! MiMo is Xiaomi's open-source AI coding CLI, compatible with Anthropic SDK protocol.
//! Supports session resumption, JSON streaming output, and Yolo mode.
//!
//! ## 设计说明
//!
//! `MimoExecutor` 持有两类状态：
//! - **只读配置**：`path`（可执行文件路径），在构造时确定，执行期间不变。
//! - **执行期状态**：`usage` 和 `has_successful_finish`，通过 `Arc<Mutex<...>>` 在
//!   `parse_output_line`（写）和 `get_usage`/`check_success`（读）之间共享。
//!   这些字段只在单次执行的生命周期内被访问（一次执行对应一条 stdout 流），
//!   因此不存在跨执行并发修改的问题。
//!
//! ## JSON 事件格式
//!
//! MiMo 输出与 OpenCode 兼容的 JSONL 格式，事件类型包括：
//! `step_start`、`tool_use`、`text`、`reasoning`、`step_finish`。

use std::sync::Arc;
use parking_lot::Mutex;

use super::helpers;
use super::mimo_event::MimoEvent;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use crate::adapters::ExecutionUsage;
use crate::models::utc_timestamp;

/// MiMo executor。
///
/// `BaseExecutor` 持有 path + model + usage 三件套，
/// MiMo 还有自己额外的 `has_successful_finish` 状态用于「非零退出码但有 step_finish 就算成功」的语义。
// `BaseExecutor` 已经 derive Clone；`Arc<Mutex<...>>` 也派生 Clone（共享内部状态），
// 因此组合结构体可直接 derive Clone，与原手写 impl 语义等价。
#[derive(Clone)]
pub struct MimoExecutor {
    /// 共享基础状态
    base: BaseExecutor,
    /// 标记是否成功完成（MiMo 可能返回非零退出码但执行成功），
    /// 由 step_finish 写入，由 check_success 读取
    has_successful_finish: Arc<Mutex<bool>>,
}

impl MimoExecutor {
    pub fn new(path: String) -> Self {
        Self {
            base: BaseExecutor::new(path),
            has_successful_finish: Arc::new(Mutex::new(false)),
        }
    }

    /// 把 MiMo 时间戳（毫秒）转换为 ISO 字符串；缺失时回退到 utc_timestamp。
    fn resolve_timestamp(ts: Option<u64>) -> String {
        ts.and_then(|ts| chrono::DateTime::from_timestamp_millis(ts as i64))
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
            .unwrap_or_else(utc_timestamp)
    }

    /// step_start: 重置 has_successful_finish + usage（新 step 累计值重置）。
    fn handle_step_start(&self, timestamp: &str) -> Option<ParsedLogEntry> {
        *self.has_successful_finish.lock() = false;
        *self.base.usage.lock() = None;
        Some(helpers::with_timestamp(helpers::entry("step_start", "Step started"), timestamp))
    }

    /// tool_use: bash 工具显示 description + output，其它工具显示 tool + description。
    /// tool_input_json 序列化完整的 part.state（含 status / input / output），
    /// 前端 extractAgentCommands 依赖 state.status 判定命令成功/失败。
    fn handle_tool_use(
        &self,
        part: &super::mimo_event::MimoPart,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
        let tool = part.tool.clone().unwrap_or_default();
        let status = part.state.as_ref().and_then(|s| s.status.clone()).unwrap_or_default();
        // bash 工具显示描述文字而非原始命令，更适合用户阅读
        let description = part.state.as_ref()
            .and_then(|s| s.input.as_ref()?.description.clone())
            .unwrap_or_default();
        // 序列化完整的 part.state（含 status / input / output），
        // 而非仅 input；前端 extractAgentCommands 需要 state.status 判定成功/失败。
        let state_json = part.state.as_ref()
            .map(|s| serde_json::to_string(s).unwrap_or_default());

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
            helpers::entry_with_optional_tool("tool", content, Some(tool), state_json),
            timestamp,
        ))
    }

    /// text: 空文本返回 None，否则返回 text 日志。
    fn handle_text(
        &self,
        part: &super::mimo_event::MimoPart,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
        let text = part.text.clone().unwrap_or_default();
        if text.is_empty() {
            return None;
        }
        Some(helpers::with_timestamp(helpers::text_entry(text), timestamp))
    }

    /// reasoning: 思考过程，限制 500 字符避免占用过多显示空间。
    fn handle_reasoning(
        &self,
        part: &super::mimo_event::MimoPart,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
        let text = part.text.clone().unwrap_or_default();
        if text.is_empty() {
            return None;
        }
        let trimmed: String = text.chars().take(500).collect();
        Some(helpers::with_timestamp(helpers::entry("thinking", trimmed), timestamp))
    }

    /// step_finish: 标记 has_successful_finish，从 tokens 提取 usage。
    fn handle_step_finish(
        &self,
        event: &MimoEvent,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
        // 标记执行成功：即使退出码非零，只要收到 step_finish 事件即表示正常完成
        *self.has_successful_finish.lock() = true;
        // 从 step_finish 的 tokens 字段提取 usage
        if let Some(part) = &event.part {
            if let Some(tokens) = &part.tokens {
                *self.base.usage.lock() = Some(ExecutionUsage {
                    input_tokens: tokens.input,
                    output_tokens: tokens.output,
                    cache_read_input_tokens: if tokens.cache.read > 0 { Some(tokens.cache.read) } else { None },
                    cache_creation_input_tokens: if tokens.cache.write > 0 { Some(tokens.cache.write) } else { None },
                    total_cost_usd: part.cost,
                    duration_ms: None,
                });
            }
        }
        Some(helpers::with_timestamp(helpers::entry("step_finish", "Step finished"), timestamp))
    }
}

impl CodeExecutor for MimoExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Mimo
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    /// 基本命令参数：单次执行，使用 JSON 格式输出。
    ///
    /// ## 为什么默认启用 --dangerously-skip-permissions
    ///
    /// ntd 的设计目标是「无人值守自动化执行」，与 Claude Code / OpenCode 保持一致。
    /// 用户明确选择 MiMo 作为执行器，即表示接受自动批准权限的行为。
    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--dangerously-skip-permissions".to_string(),
            message.to_string(),
        ]
    }

    /// 带 session 的命令参数。
    ///
    /// MiMo 使用 `-s <session_id>` 续接指定 session，`-c` 续接最近 session。
    /// `is_resume=true` 时优先使用 `-s`（精确恢复），未提供 session_id 时降级为 `-c`。
    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        if is_resume {
            // 恢复模式：优先用 -s 精确恢复指定 session，否则用 -c 续接最近 session
            if let Some(sid) = session_id {
                args.push("-s".to_string());
                args.push(sid.to_string());
            } else {
                args.push("-c".to_string());
            }
        }
        // 与 command_args 保持一致，默认启用 Yolo 模式
        args.push("--dangerously-skip-permissions".to_string());
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    /// 从 step_start 事件中提取 session_id
    fn extract_session_id(&self, line: &str) -> Option<String> {
        let event: MimoEvent = serde_json::from_str(line).ok()?;
        event.session_id.or_else(|| event.part.as_ref()?.session_id.clone())
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let event: MimoEvent = serde_json::from_str(line).ok()?;
        let timestamp = Self::resolve_timestamp(event.timestamp);

        match event.event_type.as_str() {
            "step_start" => self.handle_step_start(&timestamp),
            "tool_use" => self.handle_tool_use(event.part.as_ref()?, &timestamp),
            "text" => self.handle_text(event.part.as_ref()?, &timestamp),
            "reasoning" => self.handle_reasoning(event.part.as_ref()?, &timestamp),
            "step_finish" => self.handle_step_finish(&event, &timestamp),
            _ => None,
        }
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        // MimoExtractor 产生 Assistant 事件（log_type="assistant"），pipeline.finalize()
        // 将最后一个 Assistant 提升为 Result 事件（log_type="result"）。使用通用提取逻辑
        // 即可覆盖 result -> text -> stderr 的完整 fallback 链。
        super::default_final_result_with_think_stripping(logs)
    }

    fn get_usage(&self, _logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> {
        self.base.usage.lock().clone()
    }

    fn get_model(&self) -> Option<String> {
        // MiMo 的 JSON 输出中不包含模型名称字段，始终返回 None
        None
    }

    /// MiMo 可能返回非零退出码（如被信号打断），但只要收到了 step_finish 事件就算成功。
    /// 这是因为 MiMo 内部使用小米模型，某些错误（如模型响应超时）会导致非零退出码，
    /// 但实际任务已经完成，此时以 step_finish 事件为准。
    fn check_success(&self, exit_code: i32) -> bool {
        if exit_code == 0 {
            return true;
        }
        *self.has_successful_finish.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_line_step_start() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000,"sessionID":"ses_xxx"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert_eq!(entry.content, "Step started");
    }

    #[test]
    fn test_parse_output_line_tool_use_bash() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"tool_use","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"tool","tool":"bash","state":{"status":"completed","input":{"command":"echo hello","description":"Print hello"},"output":"hello\n"}}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool");
        assert!(entry.content.contains("completed"));
        // bash 工具显示描述文字而非原始命令
        assert!(entry.content.contains("Print hello"));
        assert!(entry.content.contains("hello"));
    }

    #[test]
    fn test_parse_output_line_text() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"text","text":"Hello world"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_parse_output_line_step_finish_stores_usage() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"step_finish","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"step_finish","tokens":{"total":32086,"input":29146,"output":36,"reasoning":24,"cache":{"write":0,"read":2880}},"cost":0}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_finish");

        let usage = executor.get_usage(&[]).unwrap();
        assert_eq!(usage.input_tokens, 29146);
        assert_eq!(usage.output_tokens, 36);
        assert_eq!(usage.cache_read_input_tokens, Some(2880));
        assert_eq!(usage.total_cost_usd, Some(0.0));
    }

    #[test]
    fn test_parse_output_line_reasoning() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"reasoning","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"reasoning","text":"thinking..."}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "thinking");
        assert_eq!(entry.content, "thinking...");
    }

    #[test]
    fn test_parse_output_line_unknown_type() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"unknown","timestamp":1700000000000}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_invalid_json() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = "not json";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_empty_text() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"text","text":""}}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_extract_session_id() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000,"sessionID":"ses_1419497e9ffePCoFjjKuVNZnte"}"#;
        let sid = executor.extract_session_id(line);
        assert_eq!(sid, Some("ses_1419497e9ffePCoFjjKuVNZnte".to_string()));
    }

    #[test]
    fn test_extract_session_id_from_part() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"tool_use","timestamp":1700000000000,"part":{"type":"tool","sessionID":"ses_part_sid"}}"#;
        let sid = executor.extract_session_id(line);
        assert_eq!(sid, Some("ses_part_sid".to_string()));
    }

    #[test]
    fn test_get_final_result_joins_texts() {
        let executor = MimoExecutor::new("mimo".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "hello"),
            ParsedLogEntry::new("text", "world"),
        ];
        // text 类型作为 fallback 被 shared helper 支持
        assert_eq!(executor.get_final_result(&logs), Some("hello\n\nworld".to_string()));
    }

    #[test]
    fn test_get_final_result_result_type_takes_priority_over_text() {
        // pipeline.finalize() 将最后一个 Assistant 提升为 result 类型；
        // get_final_result 应优先取 result 而非 text
        let executor = MimoExecutor::new("mimo".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "fallback text"),
            ParsedLogEntry::new("assistant", "assistant conclusion"),
            ParsedLogEntry::new("result", "final conclusion from pipeline"),
        ];
        assert_eq!(
            executor.get_final_result(&logs),
            Some("final conclusion from pipeline".to_string())
        );
    }

    #[test]
    fn test_get_final_result_empty() {
        let executor = MimoExecutor::new("mimo".to_string());
        let logs: Vec<ParsedLogEntry> = vec![];
        assert!(executor.get_final_result(&logs).is_none());
    }

    #[test]
    fn test_check_success_exit_code_zero() {
        let executor = MimoExecutor::new("mimo".to_string());
        assert!(executor.check_success(0));
    }

    #[test]
    fn test_check_success_non_zero_without_step_finish() {
        let executor = MimoExecutor::new("mimo".to_string());
        assert!(!executor.check_success(144));
    }

    #[test]
    fn test_check_success_non_zero_with_step_finish() {
        let executor = MimoExecutor::new("mimo".to_string());
        let line = r#"{"type":"step_finish","timestamp":1700000000000,"sessionID":"ses_xxx","part":{"type":"step_finish","tokens":{"total":100,"input":50,"output":50,"reasoning":0,"cache":{"read":0,"write":0}},"cost":0}}"#;
        let _ = executor.parse_output_line(line);
        assert!(executor.check_success(144));
    }

    #[test]
    fn test_command_args_basic() {
        let executor = MimoExecutor::new("mimo".to_string());
        let args = executor.command_args("hello world");
        assert_eq!(args, vec!["run", "--format", "json", "--dangerously-skip-permissions", "hello world"]);
    }

    #[test]
    fn test_command_args_with_session_resume() {
        let executor = MimoExecutor::new("mimo".to_string());
        let args = executor.command_args_with_session("hello", Some("ses_xxx"), true);
        assert!(args.contains(&"-s".to_string()));
        assert!(args.contains(&"ses_xxx".to_string()));
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn test_command_args_with_session_continue() {
        let executor = MimoExecutor::new("mimo".to_string());
        // is_resume=true 但没有 session_id，应该用 -c
        let args = executor.command_args_with_session("hello", None, true);
        assert!(args.contains(&"-c".to_string()));
    }

    #[test]
    fn test_command_args_with_session_new() {
        let executor = MimoExecutor::new("mimo".to_string());
        // is_resume=false，新 session，不带 -c 或 -s
        let args = executor.command_args_with_session("hello", Some("ses_xxx"), false);
        assert!(!args.contains(&"-c".to_string()));
        assert!(!args.contains(&"-s".to_string()));
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn test_get_model_always_none() {
        // MiMo JSON 输出不包含模型名称字段
        let executor = MimoExecutor::new("mimo".to_string());
        assert!(executor.get_model().is_none());
    }
}
