use std::sync::Arc;
use parking_lot::Mutex;

use super::helpers;
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use crate::models::TodoItem;

/// Hermes banner / 框线字符判定。
///
/// 框线字符 ╭ / │ / ╰ / ━ 与空格全部视作 banner，不向解析层透出。
/// 逻辑与原 `parse_output_line` 内联分支完全等价。
fn is_hermes_banner(trimmed: &str) -> bool {
    trimmed.starts_with('╭')
        || trimmed.starts_with('│')
        || trimmed.starts_with('╰')
        || trimmed.chars().all(|c| c == ' ' || c == '━' || c == '│' || c == '╰' || c == '╭')
}

/// Hermes executor。
///
/// `BaseExecutor` 持有 path + model，
/// Hermes 还有 4 个执行期专用状态：`has_done`、`session_id`、`tool_calls_count`，
/// 以及一个自定义的 `get_tool_calls_count()` override。
// `BaseExecutor` 已经 derive Clone；`Arc<Mutex<...>>` 也派生 Clone（共享内部状态），
// 因此组合结构体可直接 derive Clone，与原手写 impl 语义等价。
#[derive(Clone)]
pub struct HermesExecutor {
    base: BaseExecutor,
    _has_done: Arc<Mutex<bool>>,
    session_id: Arc<Mutex<Option<String>>>,
    tool_calls_count: Arc<Mutex<u64>>,
}

impl HermesExecutor {
    pub fn new(path: String) -> Self {
        Self {
            base: BaseExecutor::new(path),
            _has_done: Arc::new(Mutex::new(false)),
            session_id: Arc::new(Mutex::new(None)),
            tool_calls_count: Arc::new(Mutex::new(0)),
        }
    }

    /// 把 `Messages: X (Y user, Z tool calls)` 里的 Z 写入 tool_calls_count。
    ///
    /// 解析逻辑：先按 "tool calls" 切分取前半段，再 rsplit(',') 取最后一个数字段。
    /// 例如 `Messages: 4 (1 user, 2 tool calls)` → 取 " 2 " → 2。
    fn update_tool_calls_count(&self, trimmed: &str) {
        let Some(calls_part) = trimmed.split("tool calls").next() else { return };
        let Some(num_part) = calls_part.rsplit(',').next() else { return };
        if let Ok(count) = num_part.trim().parse::<u64>() {
            *self.tool_calls_count.lock() = count;
        }
    }

    /// 把 banner / box-drawing 行 / `┊` 状态指示符一起过滤掉；保留可解析行。
    fn is_skippable_line(trimmed: &str) -> bool {
        is_hermes_banner(trimmed) || trimmed.starts_with('┊')
    }

    /// 提取 "Session: <id>" / "session_id: <id>" 中的 id（已 trim）；无匹配返回 None。
    fn extract_session_prefix(trimmed: &str) -> Option<&str> {
        trimmed
            .strip_prefix("Session:")
            .or_else(|| trimmed.strip_prefix("session_id:"))
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

impl CodeExecutor for HermesExecutor {
    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Hermes
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        vec![
            "chat".to_string(),
            "-q".to_string(),
            message.to_string(),
            "--yolo".to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        if is_resume {
            let mut args = vec!["chat".to_string()];
            args.push("-q".to_string());
            args.push(message.to_string());
            args.push("--resume".to_string());
            if let Some(sid) = session_id {
                args.push(sid.to_string());
            }
            args.push("--yolo".to_string());
            args
        } else {
            self.command_args(message)
        }
    }

    fn supports_resume(&self) -> bool {
        true
    }

    fn extract_session_id(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        // Format: "hermes --resume <session_id>"
        if let Some(rest) = trimmed.strip_prefix("hermes --resume ") {
            let sid = rest.trim().to_string();
            if !sid.is_empty() {
                return Some(sid);
            }
        }
        // Format: "hermes chat ... --resume <session_id> ..."
        // Use rfind to match the last --resume (which is the command argument, not user message content)
        if trimmed.starts_with("hermes chat ") {
            if let Some(after_hermes_chat) = trimmed.strip_prefix("hermes chat ") {
                if let Some(resume_pos) = after_hermes_chat.rfind("--resume ") {
                    let after_resume = &after_hermes_chat[resume_pos + 9..]; // 9 = len("--resume ")
                    let sid = after_resume.split_whitespace().next()?;
                    if !sid.is_empty() {
                        return Some(sid.to_string());
                    }
                }
            }
        }
        // Format: "Session: <id>" / "session_id: <id>" — 两种前缀统一处理
        Self::extract_session_prefix(trimmed).map(str::to_string)
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = helpers::trimmed_non_empty(line)?;
        // 跳过 banner / box-drawing 行：与原行为一致（不向前端展示）
        if Self::is_skippable_line(trimmed) {
            return None;
        }

        // Session: <id> / session_id: <id> — 提取并存入 session_id
        if let Some(sid) = Self::extract_session_prefix(trimmed) {
            *self.session_id.lock() = Some(sid.to_string());
            return Some(helpers::info_entry(trimmed));
        }

        // "Messages: X (Y user, Z tool calls)" — 提取 Z 到 tool_calls_count
        if trimmed.starts_with("Messages:") {
            self.update_tool_calls_count(trimmed);
        }

        Some(helpers::text_entry(trimmed))
    }

    fn parse_stderr_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let trimmed = helpers::trimmed_non_empty(line)?;
        // Hermes 把 info 行也写到 stderr，按关键字分类：error/failed → "stderr"，其余 → "info"
        let log_type = if trimmed.contains("error") || trimmed.contains("Error") || trimmed.contains("ERROR") || trimmed.contains("failed") || trimmed.contains("Failed") {
            "stderr"
        } else {
            "info"
        };
        Some(helpers::entry(log_type, trimmed))
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        super::default_final_result_with_think_stripping(logs)
    }

    fn get_model(&self) -> Option<String> {
        None
    }

    fn get_tool_calls_count(&self) -> Option<u64> {
        let count = *self.tool_calls_count.lock();
        if count > 0 {
            Some(count)
        } else {
            None
        }
    }

    fn post_execution_todo_progress(&self) -> Option<Vec<TodoItem>> {
        let sid = self.session_id.lock().clone()?;
        let home = dirs::home_dir()?;
        let session_path = home.join(".hermes").join("sessions").join(format!("session_{}.json", sid));

        let content = std::fs::read_to_string(&session_path).ok()?;
        let data: serde_json::Value = serde_json::from_str(&content).ok()?;

        let messages = data.get("messages")?.as_array()?;
        // 同一 session 末尾可能存在多条 tool 消息，逐条解析，保留最新非空列表
        let mut latest_todos: Option<Vec<TodoItem>> = None;
        for msg in messages {
            if msg.get("role").and_then(|v| v.as_str()) != Some("tool") {
                continue;
            }
            if let Some(items) = parse_hermes_todo_items(msg) {
                latest_todos = Some(items);
            }
        }
        latest_todos
    }
}

/// 从单条 tool 消息中提取 todos；解析失败或无 todos 时返回 None。
fn parse_hermes_todo_items(msg: &serde_json::Value) -> Option<Vec<TodoItem>> {
    let content = msg.get("content")?.as_str()?;
    let tool_result: serde_json::Value = serde_json::from_str(content).ok()?;
    let todos = tool_result.get("todos").and_then(|v| v.as_array())?;
    let items: Vec<TodoItem> = todos.iter().filter_map(map_hermes_todo).collect();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

/// 把单条 todo JSON 转成 TodoItem；缺失关键字段时返回 None。
fn map_hermes_todo(t: &serde_json::Value) -> Option<TodoItem> {
    let content = t.get("content").or_else(|| t.get("title"))?.as_str()?;
    if content.is_empty() {
        return None;
    }
    let raw_status = t.get("status").and_then(|v| v.as_str()).unwrap_or("pending");
    let status = match raw_status.to_lowercase().as_str() {
        "done" | "completed" | "complete" | "finished" => "completed".to_string(),
        "in_progress" | "inprogress" | "in-progress" | "doing" | "active" => "in_progress".to_string(),
        "cancelled" | "canceled" | "abort" | "aborted" => "cancelled".to_string(),
        "failed" | "fail" | "error" => "failed".to_string(),
        "running" => "running".to_string(),
        _ => "pending".to_string(),
    };
    Some(TodoItem {
        id: t.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        content: content.to_string(),
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_line_text() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_output_line("Hello world").unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_parse_output_line_empty() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.parse_output_line("").is_none());
        assert!(executor.parse_output_line("   ").is_none());
    }

    #[test]
    fn test_parse_output_line_session_id() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_output_line("session_id: abc123").unwrap();
        assert_eq!(entry.log_type, "info");
        assert!(entry.content.contains("session_id"));
    }

    #[test]
    fn test_parse_output_line_banner() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.parse_output_line("╭─ Hermes ─────────────────────────────────").is_none());
        assert!(executor.parse_output_line("│ some text").is_none());
    }

    #[test]
    fn test_get_final_result_with_text() {
        let executor = HermesExecutor::new("hermes".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  hello world  "),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello world".to_string()));
    }

    #[test]
    fn test_command_args() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args("do something");
        assert_eq!(args, vec!["chat", "-q", "do something", "--yolo"]);
    }

    #[test]
    fn test_executor_type() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Hermes);
    }

    #[test]
    fn test_supports_resume() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.supports_resume());
    }

    #[test]
    fn test_extract_session_id_resume_format() {
        let executor = HermesExecutor::new("hermes".to_string());
        let sid = executor.extract_session_id("hermes --resume 20260517_051220_95e4d6");
        assert_eq!(sid, Some("20260517_051220_95e4d6".to_string()));
    }

    #[test]
    fn test_extract_session_id_session_prefix() {
        let executor = HermesExecutor::new("hermes".to_string());
        let sid = executor.extract_session_id("Session: mysession123");
        assert_eq!(sid, Some("mysession123".to_string()));
    }

    #[test]
    fn test_extract_session_id_lowercase_prefix() {
        let executor = HermesExecutor::new("hermes".to_string());
        let sid = executor.extract_session_id("session_id: abc_xyz_789");
        assert_eq!(sid, Some("abc_xyz_789".to_string()));
    }

    #[test]
    fn test_extract_session_id_no_match() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.extract_session_id("Hello world").is_none());
        assert!(executor.extract_session_id("").is_none());
        assert!(executor.extract_session_id("hermes chat -q test").is_none());
    }

    #[test]
    fn test_command_args_with_session_new() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args_with_session("do something", Some("task_id"), false);
        assert_eq!(args, vec!["chat", "-q", "do something", "--yolo"]);
    }

    #[test]
    fn test_command_args_with_session_resume() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args_with_session("continue", Some("session_123"), true);
        assert_eq!(args, vec!["chat", "-q", "continue", "--resume", "session_123", "--yolo"]);
    }

    #[test]
    fn test_command_args_with_session_resume_none() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args_with_session("continue", None, true);
        assert_eq!(args, vec!["chat", "-q", "continue", "--resume", "--yolo"]);
    }

    #[test]
    fn test_extract_session_prefix_both_forms() {
        // 抽出 static helper 后的单元测试，覆盖 Session: / session_id: / 非匹配三种分支
        assert_eq!(HermesExecutor::extract_session_prefix("Session: abc"), Some("abc"));
        assert_eq!(HermesExecutor::extract_session_prefix("session_id: xyz"), Some("xyz"));
        assert_eq!(HermesExecutor::extract_session_prefix("Hello"), None);
        assert_eq!(HermesExecutor::extract_session_prefix("Session: "), None);
    }

    #[test]
    fn test_is_hermes_banner_variants() {
        assert!(is_hermes_banner("╭─────"));
        assert!(is_hermes_banner("│ text"));
        assert!(is_hermes_banner("╰─────"));
        assert!(is_hermes_banner("━━━━━"));
        assert!(is_hermes_banner("   "));
        assert!(!is_hermes_banner("normal text"));
    }

    #[test]
    fn test_update_tool_calls_count() {
        let executor = HermesExecutor::new("hermes".to_string());
        executor.update_tool_calls_count("Messages: 4 (1 user, 2 tool calls)");
        assert_eq!(executor.get_tool_calls_count(), Some(2));
    }

    #[test]
    fn test_update_tool_calls_count_invalid() {
        // 不匹配时不更新（保留 0 → get_tool_calls_count 返回 None）
        let executor = HermesExecutor::new("hermes".to_string());
        executor.update_tool_calls_count("not a messages line");
        assert_eq!(executor.get_tool_calls_count(), None);
    }

    #[test]
    fn test_map_hermes_todo_normalizes_status() {
        let t = serde_json::json!({
            "content": "do thing",
            "status": "Done"
        });
        let item = map_hermes_todo(&t).unwrap();
        assert_eq!(item.content, "do thing");
        assert_eq!(item.status, "completed");
    }

    #[test]
    fn test_map_hermes_todo_uses_title_fallback() {
        // content 缺失时 fallback 到 title
        let t = serde_json::json!({"title": "from title"});
        let item = map_hermes_todo(&t).unwrap();
        assert_eq!(item.content, "from title");
        assert_eq!(item.status, "pending");
    }

    #[test]
    fn test_map_hermes_todo_empty_content_returns_none() {
        let t = serde_json::json!({"content": ""});
        assert!(map_hermes_todo(&t).is_none());
    }

    #[test]
    fn test_map_hermes_todo_missing_content_returns_none() {
        let t = serde_json::json!({"status": "done"});
        assert!(map_hermes_todo(&t).is_none());
    }
}
