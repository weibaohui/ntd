use std::sync::Arc;
use parking_lot::Mutex;

use super::helpers;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use crate::models::ExecutionUsage;

/// AtomCode executor。
///
/// `BaseExecutor` 持有 path + model，
/// 额外保留 `has_done` 状态字段用于在 stderr 中检测到 "done" 事件。
// `BaseExecutor` 已经 derive Clone；`Arc<Mutex<...>>` 也派生 Clone（共享内部状态），
// 因此组合结构体可直接 derive Clone，与原手写 impl 语义等价。
#[derive(Clone)]
pub struct AtomcodeExecutor {
    base: BaseExecutor,
    /// 标记是否已收到 done 事件，用于 stderr 流处理
    has_done: Arc<Mutex<bool>>,
}

impl AtomcodeExecutor {
    pub fn new(path: String) -> Self {
        Self {
            base: BaseExecutor::new(path),
            has_done: Arc::new(Mutex::new(false)),
        }
    }
}

impl CodeExecutor for AtomcodeExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Atomcode
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "-v".to_string(),
            // headless/自动化模式下跳过交互式权限确认，与 ClaudeCodeExecutor 保持一致
            "--dangerously-skip-permissions".to_string(),
            "-p".to_string(),
            message.to_string(),
        ]
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = helpers::trimmed_non_empty(line)?;
        // 跳过 stderr 行混入 stdout 的情况（以 [ 开头的结构化事件已在 EventPipeline 处理）
        if trimmed.starts_with('[') {
            return None;
        }
        // atomcode 的 stdout 不解析为结构化事件，全部当作普通文本透传给前端。
        Some(helpers::text_entry(trimmed))
    }

    fn parse_stderr_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = helpers::trimmed_non_empty(line)?;

        // 跳过 streaming / headless 标记行：与原行为一致（不向前端展示）
        if trimmed.starts_with("[tool-streaming") || trimmed.starts_with("[headless]") || trimmed.starts_with("[tool-batch") {
            return None;
        }

        if trimmed.starts_with("[tokens]") {
            return self.parse_tokens_line(trimmed);
        }
        if trimmed.starts_with("[done]") {
            return self.parse_done_line(trimmed);
        }
        if trimmed.starts_with("[tool→") {
            let (tool_name, tool_input_json) = parse_atomcode_tool_call(trimmed);
            return Some(ParsedLogEntry {
                timestamp: crate::models::utc_timestamp(),
                log_type: "tool".to_string(),
                content: trimmed.to_string(),
                usage: None,
                tool_name,
                tool_input_json,
            });
        }
        if trimmed.starts_with("[tool←") {
            return Some(helpers::entry("tool", trimmed));
        }
        if trimmed.starts_with("[approval-denied]") {
            return Some(helpers::error_entry(trimmed));
        }
        None
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        super::default_final_result_with_think_stripping(logs)
    }

    fn get_model(&self) -> Option<String> {
        None
    }
}

impl AtomcodeExecutor {
    /// 解析 `[tokens] prompt=N completion=M` 行，更新 usage 并返回 tokens 日志条目。
    fn parse_tokens_line(&self, trimmed: &str) -> Option<ParsedLogEntry> {
        // 解析 key=value 形式，例如 [tokens] prompt=11 completion=0
        let mut prompt_tokens = 0u64;
        let mut completion_tokens = 0u64;
        for part in trimmed.split_whitespace().skip(1) {
            if let Some((key, val)) = part.split_once('=') {
                match key {
                    "prompt" => prompt_tokens = val.parse().unwrap_or(0),
                    "completion" => completion_tokens = val.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        let usage = if prompt_tokens > 0 || completion_tokens > 0 {
            Some(ExecutionUsage {
                input_tokens: prompt_tokens,
                output_tokens: completion_tokens,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
                total_cost_usd: None,
                duration_ms: None,
            })
        } else {
            None
        };

        Some(ParsedLogEntry {
            timestamp: crate::models::utc_timestamp(),
            log_type: "tokens".to_string(),
            content: trimmed.to_string(),
            usage,
            tool_name: None,
            tool_input_json: None,
        })
    }

    /// 解析 `[done] 4.6s tokens=N turns=N tool_calls=N ...` 行，更新 has_done 并返回 step_finish 日志。
    fn parse_done_line(&self, trimmed: &str) -> Option<ParsedLogEntry> {
        *self.has_done.lock() = true;
        let stats = parse_done_stats(trimmed);
        Some(helpers::entry(
            "step_finish",
            format!("Execution finished: {} turns, {} tool calls", stats.turns, stats.tool_calls),
        ))
    }
}

/// `[done]` 行解析出来的统计字段。
/// 用结构体集中收集 tokens / turns / tool_calls / duration，
/// 让 parse_done_line 与 update_usage_from_done 各自单一职责。
struct DoneStats {
    total_tokens: u64,
    turns: u64,
    tool_calls: u64,
    duration_ms: Option<u64>,
}

/// 从 `[done] 4.6s tokens=N turns=N tool_calls=N ...` 解析所有 key=value。
/// duration 来自第二个 token（第一个非 `[done]` 段），其它字段以 key=value 形式逐个匹配。
fn parse_done_stats(trimmed: &str) -> DoneStats {
    let mut stats = DoneStats { total_tokens: 0, turns: 0, tool_calls: 0, duration_ms: None };
    for (i, part) in trimmed.split_whitespace().enumerate() {
        if i == 1 {
            // 例如 "4.6s" → 4600 ms
            let s = part.trim_end_matches('s');
            if let Ok(secs) = s.parse::<f64>() {
                stats.duration_ms = Some((secs * 1000.0) as u64);
            }
        } else if let Some((key, val)) = part.split_once('=') {
            match key {
                "tokens" => stats.total_tokens = val.parse().unwrap_or(0),
                "turns" => stats.turns = val.parse().unwrap_or(0),
                "tool_calls" => stats.tool_calls = val.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    stats
}

/// Parse tool name and args JSON from atomcode stderr format: [tool→ name args={...}]
fn parse_atomcode_tool_call(line: &str) -> (Option<String>, Option<String>) {
    let trimmed = line.trim_start_matches("[tool→").trim_start_matches("[tool->").trim();
    let (name_part, args_part) = if let Some(idx) = trimmed.find(" args=") {
        (&trimmed[..idx], Some(trimmed[idx + 6..].trim()))
    } else if let Some(idx) = trimmed.find(" args:") {
        (&trimmed[..idx], Some(trimmed[idx + 6..].trim()))
    } else {
        (trimmed, None)
    };
    let name = name_part.split_whitespace().next().map(|s| s.to_string()).filter(|s| !s.is_empty());
    let args_json = args_part.and_then(|a| {
        let a = a.trim_matches(|c| c == '{' || c == '}');
        if a.is_empty() { None } else { Some(format!("{{{}}}", a)) }
    });
    (name, args_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ParsedLogEntry;

    #[test]
    fn test_parse_output_line_text() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_output_line("Hello world").unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_parse_output_line_empty() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.parse_output_line("").is_none());
        assert!(executor.parse_output_line("   ").is_none());
    }

    #[test]
    fn test_parse_stderr_line_tokens() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[tokens] prompt=11 completion=5").unwrap();
        assert_eq!(entry.log_type, "tokens");
        assert_eq!(entry.content, "[tokens] prompt=11 completion=5");
        assert_eq!(entry.usage.as_ref().unwrap().input_tokens, 11);
        assert_eq!(entry.usage.as_ref().unwrap().output_tokens, 5);
    }

    #[test]
    fn test_parse_stderr_line_done() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[done] 4.6s tokens=100 turns=2 tool_calls=1").unwrap();
        assert_eq!(entry.log_type, "step_finish");
        assert!(entry.content.contains("2 turns"));
        assert!(entry.content.contains("1 tool calls"));
    }

    #[test]
    fn test_parse_stderr_line_done_with_stopped() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[done] 4.9s tokens=0 turns=3 tool_calls=3 stopped=turn_limit").unwrap();
        assert_eq!(entry.log_type, "step_finish");
        assert!(entry.content.contains("3 turns"));
        assert!(entry.content.contains("3 tool calls"));
    }

    #[test]
    fn test_parse_stderr_line_tool_call() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[tool→ bash args={\"command\": \"ls\"}]").unwrap();
        assert_eq!(entry.log_type, "tool");
    }

    #[test]
    fn test_parse_stderr_line_tool_result() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[tool← bash OK 39ms] result here").unwrap();
        assert_eq!(entry.log_type, "tool");
    }

    #[test]
    fn test_parse_stderr_line_approval_denied() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let entry = executor.parse_stderr_line("[approval-denied] tool=write_file reason=outside dir").unwrap();
        assert_eq!(entry.log_type, "error");
    }

    #[test]
    fn test_parse_stderr_line_streaming_skipped() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.parse_stderr_line("[tool-streaming← write_file]").is_none());
    }

    #[test]
    fn test_parse_stderr_line_headless_skipped() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.parse_stderr_line("[headless] auto-approved bash: ...").is_none());
    }

    #[test]
    fn test_parse_stderr_line_unknown_falls_back() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.parse_stderr_line("some random stderr").is_none());
    }

    #[test]
    fn test_get_final_result_with_text() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  hello world  "),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello world".to_string()));
    }


    #[test]
    fn test_get_model_always_none() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.get_model().is_none());
    }

    #[test]
    fn test_command_args() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let args = executor.command_args("do something");
        assert_eq!(args, vec!["-v", "--dangerously-skip-permissions", "-p", "do something"]);
    }

    #[test]
    fn test_executor_type() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Atomcode);
    }
}
