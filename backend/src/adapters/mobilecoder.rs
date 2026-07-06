use std::sync::Arc;
use parking_lot::Mutex;

use super::helpers;
use super::mobilecoder_event::MobilecoderAgentEvent;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use crate::models::ExecutionUsage;
use crate::models::utc_timestamp;

/// MobileCoder executor。
///
/// `BaseExecutor` 已经 `#[derive(Clone)]`，`Arc<Mutex<...>>` 也派生 Clone。
#[derive(Clone)]
pub struct MobilecoderExecutor {
    base: BaseExecutor,
    /// 缓存从 JSON 事件中提取的 session_id，支持跨行回退和 resume 时回写 DB。
    session_id: Arc<Mutex<Option<String>>>,
}

impl MobilecoderExecutor {
    pub fn new(path: String) -> Self {
        Self {
            base: BaseExecutor::new(path),
            session_id: Arc::new(Mutex::new(None)),
        }
    }

    /// 更新 session_id 缓存（extract_session_id 和 parse_output_line 共用）。
    fn update_session_id_cache(&self, sid: Option<String>) {
        if let Some(ref s) = sid {
            *self.session_id.lock() = Some(s.clone());
        }
    }

    /// 解析 MobileCoder 的 timestamp 字段：优先 ISO 8601，再回退到数字（毫秒/秒），
    /// 最后落到 utc_timestamp()。逻辑与原内联分支完全等价。
    fn resolve_timestamp(ts: Option<&super::mobilecoder_event::MobilecoderTimestamp>) -> String {
        let Some(ts) = ts else { return utc_timestamp() };
        let raw = &ts.0;
        // Try ISO 8601 first (new version format)
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
            return dt.with_timezone(&chrono::Utc)
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string();
        }
        // Try as numeric string (milliseconds or seconds)
        if let Ok(ts_f) = raw.parse::<f64>() {
            let ts_ms = if ts_f > 1e12 { ts_f as i64 } else { (ts_f * 1000.0) as i64 };
            if let Some(dt) = chrono::DateTime::from_timestamp_millis(ts_ms) {
                return dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
            }
        }
        utc_timestamp()
    }

    /// step_start: 重置 usage（新一轮 step 累计值重置），返回 step_start 日志。
    fn handle_step_start(&self, timestamp: &str) -> Option<ParsedLogEntry> {
        Some(helpers::with_timestamp(
            helpers::entry_with_optional_tool("step_start", "Step started", None, None),
            timestamp,
        ))
    }

    /// tool_use: bash 工具显示 description + output；其它工具显示 tool + description。
    fn handle_tool_use(
        &self,
        part: &super::mobilecoder_event::MobilecoderAgentPart,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
        let tool = part.tool.clone().unwrap_or_default();
        let status = part.state.as_ref().and_then(|s| s.status.clone()).unwrap_or_default();
        let description = part.state.as_ref().and_then(|s| s.input.as_ref().and_then(|i| i.description.clone())).unwrap_or_default();
        let input_json = part.state.as_ref()
            .and_then(|s| s.input.as_ref())
            .map(|i| i.to_full_json());

        // bash 特殊渲染：把命令输出也带上，其它工具只显示描述
        let content = if tool == "bash" {
            match &part.state.as_ref().and_then(|s| s.output.clone()) {
                Some(output) => format!("[{}] {}: {}", status, description, output),
                None => format!("[{}] {}", status, description),
            }
        } else {
            format!("[{}] Tool: {} - {}", status, tool, description)
        };

        // 空工具名不上报 tool_name 字段，避免前端拿到空字符串
        let tool_name = if tool.trim().is_empty() { None } else { Some(tool) };
        Some(helpers::with_timestamp(
            helpers::entry_with_optional_tool("tool", content, tool_name, input_json),
            timestamp,
        ))
    }

    /// text: 空文本返回 None（前端不渲染空消息），否则返回 text 日志。
    fn handle_text(
        &self,
        part: &super::mobilecoder_event::MobilecoderAgentPart,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
        let text = part.text.clone().unwrap_or_default();
        if text.is_empty() {
            return None;
        }
        Some(helpers::with_timestamp(helpers::text_entry(text), timestamp))
    }

    /// step_finish: 从 part.tokens 提取 usage，返回 step_finish 日志。
    fn handle_step_finish(
        &self,
        event: &MobilecoderAgentEvent,
        timestamp: &str,
    ) -> Option<ParsedLogEntry> {
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

impl CodeExecutor for MobilecoderExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Mobilecoder
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
            message.to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        if is_resume {
            if let Some(sid) = session_id {
                args.push("-s".to_string());
                args.push(sid.to_string());
            }
        }
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    fn extract_session_id(&self, line: &str) -> Option<String> {
        let event: MobilecoderAgentEvent = serde_json::from_str(line).ok()?;
        let sid = event.session_id.or_else(|| event.part.as_ref()?.session_id.clone());
        self.update_session_id_cache(sid.clone());
        sid.or_else(|| self.session_id.lock().clone())
    }

    fn get_session_id(&self) -> Option<String> {
        self.session_id.lock().clone()
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let event: MobilecoderAgentEvent = serde_json::from_str(line).ok()?;
        // 缓存 session_id：优先取事件顶层字段，再取 part 内字段
        let sid = event.session_id.clone().or_else(|| event.part.as_ref()?.session_id.clone());
        self.update_session_id_cache(sid);
        let timestamp = Self::resolve_timestamp(event.timestamp.as_ref());

        match event.event_type.as_str() {
            "step_start" => self.handle_step_start(&timestamp),
            "tool_use" => self.handle_tool_use(event.part.as_ref()?, &timestamp),
            "text" => self.handle_text(event.part.as_ref()?, &timestamp),
            "step_finish" => self.handle_step_finish(&event, &timestamp),
            _ => None,
        }
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        super::default_final_result_with_think_stripping(logs)
    }

    fn get_model(&self) -> Option<String> {
        // MobileCoder doesn't provide model info
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ParsedLogEntry;

    #[test]
    fn test_parse_output_line_step_start() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert_eq!(entry.content, "Step started");
    }

    #[test]
    fn test_parse_output_line_tool_use_bash() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = r#"{"type":"tool_use","timestamp":1700000000000,"part":{"type":"tool_use","tool":"bash","state":{"status":"success","input":{"description":"list files"},"output":"file.txt"}}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool");
        assert!(entry.content.contains("success"), "content should contain status: {}", entry.content);
        assert!(entry.content.contains("list files"), "content should contain description: {}", entry.content);
        assert!(entry.content.contains("file.txt"), "content should contain output: {}", entry.content);
    }

    #[test]
    fn test_parse_output_line_text() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"part":{"type":"text","text":"hello world"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "hello world");
    }


    #[test]
    fn test_parse_output_line_unknown_type() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = r#"{"type":"unknown","timestamp":1700000000000}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_invalid_json() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = "not json";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_parse_output_line_empty_text() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"part":{"type":"text","text":""}}"#;
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_get_final_result_with_text() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  hello world  "),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello world".to_string()));
    }

    #[test]
    fn test_get_final_result_fallback_to_stderr() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let logs = vec![
            ParsedLogEntry::new("stderr", "error output"),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("error output".to_string()));
    }

    #[test]
    fn test_get_final_result_empty_logs() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let logs: Vec<ParsedLogEntry> = vec![];
        assert!(executor.get_final_result(&logs).is_none());
    }


    #[test]
    fn test_get_model_always_none() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        assert!(executor.get_model().is_none());
    }

    #[test]
    fn test_parse_output_line_with_iso_timestamp() {
        // New version: ISO 8601 string timestamp
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = r#"{"type":"step_start","timestamp":"2026-05-12T06:08:58.721Z","content":"Step started"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert_eq!(entry.content, "Step started");
        assert_eq!(entry.timestamp, "2026-05-12T06:08:58.721Z");
    }

    #[test]
    fn test_parse_output_line_with_number_timestamp_milliseconds() {
        // Milliseconds format (13+ digits) should still work
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000,"content":"Step started"}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
        assert!(entry.timestamp.starts_with("2023-"));
    }

    #[test]
    fn test_extract_session_id_caches_result() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000,"sessionID":"mob_sess_abc"}"#;
        let sid = executor.extract_session_id(line);
        assert_eq!(sid, Some("mob_sess_abc".to_string()));
        // 再次调用仍能获取缓存值
        assert_eq!(executor.extract_session_id(r#"{"type":"text"}"#), Some("mob_sess_abc".to_string()));
    }

    #[test]
    fn test_get_session_id_returns_cached() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let line = r#"{"type":"step_start","sessionID":"mob_session_xyz"}"#;
        let _ = executor.parse_output_line(line);
        assert_eq!(executor.get_session_id(), Some("mob_session_xyz".to_string()));
    }

    #[test]
    fn test_get_session_id_before_any_event() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        assert_eq!(executor.get_session_id(), None);
    }
}


