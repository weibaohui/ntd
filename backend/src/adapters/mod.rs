use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::{ExecutorType, ParsedLogEntry, ExecutionUsage, TodoItem};

/// Unified executor definition - single source of truth for all executor metadata.
pub struct ExecutorDef {
    /// Internal name used in database and registry (e.g., "claudecode")
    pub name: &'static str,
    /// ExecutorType enum variant for this executor
    pub executor_type: ExecutorType,
    /// Binary name to execute (e.g., "claude")
    pub binary_name: &'static str,
    /// Display name for UI (e.g., "Claude Code")
    pub display_name: &'static str,
    /// Default binary path
    pub default_path: &'static str,
    /// Session directory (can be empty)
    pub session_dir: &'static str,
    /// Aliases that can be used to refer to this executor
    pub aliases: &'static [&'static str],
}

impl ExecutorDef {
    /// Check if this executor matches the given name (name or alias)
    pub fn matches(&self, name: &str) -> bool {
        let name = name.trim().to_lowercase();
        self.name == name || self.aliases.iter().any(|&a| a == name)
    }
}

/// All supported executors - single source of truth
pub static EXECUTORS: &[ExecutorDef] = &[
    ExecutorDef {
        name: "claudecode",
        executor_type: ExecutorType::Claudecode,
        binary_name: "claude",
        display_name: "Claude Code",
        default_path: "claude",
        session_dir: "~/.claude",
        aliases: &["claude", "claude_code"],
    },
    ExecutorDef {
        name: "codebuddy",
        executor_type: ExecutorType::Codebuddy,
        binary_name: "codebuddy",
        display_name: "CodeBuddy",
        default_path: "codebuddy",
        session_dir: "~/.codebuddy",
        aliases: &["cbc"],
    },
    ExecutorDef {
        name: "opencode",
        executor_type: ExecutorType::Opencode,
        binary_name: "opencode",
        display_name: "Opencode",
        default_path: "opencode",
        session_dir: "~/.opencode",
        aliases: &[],
    },
    ExecutorDef {
        name: "atomcode",
        executor_type: ExecutorType::Atomcode,
        binary_name: "atomcode",
        display_name: "AtomCode",
        default_path: "atomcode",
        session_dir: "~/.atomcode",
        aliases: &["atom"],
    },
    ExecutorDef {
        name: "hermes",
        executor_type: ExecutorType::Hermes,
        binary_name: "hermes",
        display_name: "Hermes",
        default_path: "hermes",
        session_dir: "~/.hermes",
        aliases: &[],
    },
    ExecutorDef {
        name: "kimi",
        executor_type: ExecutorType::Kimi,
        binary_name: "kimi",
        display_name: "Kimi",
        default_path: "kimi",
        session_dir: "~/.kimi",
        aliases: &[],
    },
    ExecutorDef {
        name: "mobilecoder",
        executor_type: ExecutorType::Mobilecoder,
        binary_name: "mobile",
        display_name: "MobileCoder",
        default_path: "mobile",
        session_dir: "~/.mobile-coder",
        aliases: &[],
    },
    ExecutorDef {
        name: "codex",
        executor_type: ExecutorType::Codex,
        binary_name: "codex",
        display_name: "Codex",
        default_path: "codex",
        session_dir: "~/.codex",
        aliases: &[],
    },
    ExecutorDef {
        name: "codewhale",
        executor_type: ExecutorType::Codewhale,
        binary_name: "codewhale",
        display_name: "CodeWhale",
        default_path: "codewhale",
        session_dir: "~/.codewhale",
        aliases: &[],
    },
    ExecutorDef {
        name: "pi",
        executor_type: ExecutorType::Pi,
        binary_name: "pi",
        display_name: "Pi",
        default_path: "pi",
        session_dir: "~/.pi",
        aliases: &[],
    },
];

/// 支持继续对话的执行器集合（与前端 RESUMABLE_EXECUTORS 保持一致）
pub const RESUMABLE_EXECUTORS: &[&str] = &["claudecode", "kimi", "opencode", "mobilecoder", "hermes", "codewhale", "pi"];

/// 默认执行器
pub const DEFAULT_EXECUTOR: &str = "claudecode";

/// Find executor definition by name or alias
pub fn find_executor(name: &str) -> Option<&'static ExecutorDef> {
    EXECUTORS.iter().find(|e| e.matches(name))
}

/// Parse executor string (with aliases) into `ExecutorType`.
/// Returns `None` for unrecognized names.
pub fn parse_executor_type(executor: &str) -> Option<ExecutorType> {
    find_executor(executor).map(|e| e.executor_type)
}

/// Strip `<think>...</think>` tags from content.
pub fn strip_think_tags(content: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    static THINK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"<think>[\s\S]*?</think>").unwrap()
    });
    THINK_RE.replace_all(content, "").trim().to_string()
}

/// Default `get_final_result` for executors that use text+stderr logs with think-tag stripping.
/// Collects all "text" log entries (with think tags stripped), falling back to last "stderr" log.
pub fn default_final_result_with_think_stripping(logs: &[ParsedLogEntry]) -> Option<String> {
    let texts: Vec<String> = logs.iter()
        .filter(|l| l.log_type == "text")
        .map(|l| strip_think_tags(&l.content))
        .filter(|t| !t.trim().is_empty())
        .collect();

    if !texts.is_empty() {
        Some(texts.join("\n\n"))
    } else {
        logs.iter()
            .rev()
            .find(|l| l.log_type == "stderr")
            .map(|l| l.content.clone())
    }
}

/// Extract usage from the last "result" log entry (used by claude_code, codebuddy).
pub fn get_usage_from_logs(logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> {
    logs.iter().rev().find(|l| l.log_type == "result")?.usage.clone()
}

pub mod mobilecoder;
pub mod mobilecoder_event;
pub mod claude_protocol;
pub mod agent_event;
pub mod claude_code;
pub mod codebuddy;
pub mod opencode;
pub mod opencode_event;
pub mod atomcode;
pub mod hermes;
pub mod kimi;
pub mod codex;
pub mod codewhale;
pub mod pi;
pub mod pi_event;

#[async_trait]
pub trait CodeExecutor: Send + Sync {
    /// 返回执行器类型
    fn executor_type(&self) -> ExecutorType;

    /// 返回可执行文件路径
    fn executable_path(&self) -> &str;

    /// 返回命令参数
    fn command_args(&self, message: &str) -> Vec<String>;

    /// 返回带 session 的命令参数（默认实现忽略 session）
    /// `is_resume` 为 true 时表示恢复已有会话，false 表示新执行并指定 session_id
    fn command_args_with_session(&self, message: &str, _session_id: Option<&str>, _is_resume: bool) -> Vec<String> {
        self.command_args(message)
    }

    /// 该执行器是否支持通过 session_id 恢复对话
    fn supports_resume(&self) -> bool {
        false
    }

    /// 从输出行中提取 session_id（用于执行过程中实时更新数据库）
    fn extract_session_id(&self, _line: &str) -> Option<String> {
        None
    }

    /// 解析输出行，返回解析后的日志条目
    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry>;

    /// 解析 stderr 行，返回解析后的日志条目。返回 None 表示作为普通 stderr 处理。
    fn parse_stderr_line(&self, _line: &str) -> Option<ParsedLogEntry> {
        None
    }

    /// 是否解析成功（检查退出码）
    fn check_success(&self, exit_code: i32) -> bool {
        exit_code == 0
    }

    /// 从日志列表中提取最终结果
    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        logs.iter()
            .rev()
            .find(|l| l.log_type == "result" || l.log_type == "text")
            .map(|l| l.content.clone())
    }

    /// 从日志列表中提取 usage 信息
    fn get_usage(&self, logs: &[ParsedLogEntry]) -> Option<ExecutionUsage>;
    fn get_model(&self) -> Option<String>;

    /// 执行完成后从外部数据源提取 todo 进度（用于无法从 stdout 获取工具调用的执行器）
    fn post_execution_todo_progress(&self) -> Option<Vec<TodoItem>> {
        None
    }

    /// 获取工具调用次数（用于从输出摘要中提取的执行器，如 hermes）
    fn get_tool_calls_count(&self) -> Option<u64> {
        None
    }
}

/// 代码执行器注册表
pub struct ExecutorRegistry {
    executors: Arc<RwLock<HashMap<ExecutorType, Arc<dyn CodeExecutor>>>>,
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self {
            executors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register<E: CodeExecutor + 'static>(&self, executor: E) {
        let executor_type = executor.executor_type();
        self.executors.write().await.insert(executor_type, Arc::new(executor));
    }

    pub async fn get(&self, executor_type: ExecutorType) -> Option<Arc<dyn CodeExecutor>> {
        self.executors.read().await.get(&executor_type).cloned()
    }

    pub async fn get_default(&self) -> Option<Arc<dyn CodeExecutor>> {
        self.get(ExecutorType::Claudecode).await
    }

    pub async fn list_executors(&self) -> Vec<ExecutorType> {
        self.executors.read().await.keys().copied().collect()
    }

    pub async fn unregister(&self, executor_type: ExecutorType) {
        self.executors.write().await.remove(&executor_type);
    }

    /// Create an executor instance by name and path.
    pub fn create_executor(name: &str, path: &str) -> Option<Arc<dyn CodeExecutor>> {
        let exec = find_executor(name)?;
        let executor: Arc<dyn CodeExecutor> = match exec.name {
            "claudecode" => Arc::new(claude_code::ClaudeCodeExecutor::new(path.to_string())),
            "mobilecoder" => Arc::new(mobilecoder::MobilecoderExecutor::new(path.to_string())),
            "codebuddy" => Arc::new(codebuddy::CodebuddyExecutor::new(path.to_string())),
            "opencode" => Arc::new(opencode::OpencodeExecutor::new(path.to_string())),
            "atomcode" => Arc::new(atomcode::AtomcodeExecutor::new(path.to_string())),
            "hermes" => Arc::new(hermes::HermesExecutor::new(path.to_string())),
            "kimi" => Arc::new(kimi::KimiExecutor::new(path.to_string())),
            "codex" => Arc::new(codex::CodexExecutor::new(path.to_string())),
            "codewhale" => Arc::new(codewhale::CodewhaleExecutor::new(path.to_string())),
            "pi" => Arc::new(pi::PiExecutor::new(path.to_string())),
            _ => return None,
        };
        Some(executor)
    }

    /// Register an executor by name and path (convenience method).
    pub async fn register_by_name(&self, name: &str, path: &str) -> bool {
        if let Some(executor) = Self::create_executor(name, path) {
            let executor_type = executor.executor_type();
            self.executors.write().await.insert(executor_type, executor);
            true
        } else {
            false
        }
    }
}

impl Default for ExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ParsedLogEntry, ExecutionUsage};

    #[test]
    fn test_parse_executor_type_claudecode() {
        assert_eq!(parse_executor_type("claudecode"), Some(ExecutorType::Claudecode));
        assert_eq!(parse_executor_type("claude"), Some(ExecutorType::Claudecode));
    }

    #[test]
    fn test_parse_executor_type_codebuddy() {
        assert_eq!(parse_executor_type("codebuddy"), Some(ExecutorType::Codebuddy));
        assert_eq!(parse_executor_type("cbc"), Some(ExecutorType::Codebuddy));
    }

    #[test]
    fn test_parse_executor_type_opencode() {
        assert_eq!(parse_executor_type("opencode"), Some(ExecutorType::Opencode));
    }

    #[test]
    fn test_parse_executor_type_atomcode() {
        assert_eq!(parse_executor_type("atomcode"), Some(ExecutorType::Atomcode));
        assert_eq!(parse_executor_type("atom"), Some(ExecutorType::Atomcode));
        assert_eq!(parse_executor_type("ATOMCODE"), Some(ExecutorType::Atomcode));
    }

    #[test]
    fn test_parse_executor_type_mobilecoder() {
        assert_eq!(parse_executor_type("mobilecoder"), Some(ExecutorType::Mobilecoder));
    }

    #[test]
    fn test_parse_executor_type_codex() {
        assert_eq!(parse_executor_type("codex"), Some(ExecutorType::Codex));
        assert_eq!(parse_executor_type("CODEX"), Some(ExecutorType::Codex));
    }

    #[test]
    fn test_parse_executor_type_codewhale() {
        assert_eq!(parse_executor_type("codewhale"), Some(ExecutorType::Codewhale));
        assert_eq!(parse_executor_type("CODEWHALE"), Some(ExecutorType::Codewhale));
    }

    #[test]
    fn test_parse_executor_type_unknown() {
        assert_eq!(parse_executor_type("unknown"), None);
        assert_eq!(parse_executor_type(""), None);
        assert_eq!(parse_executor_type("typo_executor"), None);
    }

    #[test]
    fn test_parse_executor_type_case_insensitive() {
        assert_eq!(parse_executor_type("Claude"), Some(ExecutorType::Claudecode));
        assert_eq!(parse_executor_type("CLAUDE"), Some(ExecutorType::Claudecode));
        assert_eq!(parse_executor_type("CodeBuddy"), Some(ExecutorType::Codebuddy));
    }

    #[test]
    fn test_parse_executor_type_trims_whitespace() {
        assert_eq!(parse_executor_type(" claude "), Some(ExecutorType::Claudecode));
        assert_eq!(parse_executor_type("  opencode"), Some(ExecutorType::Opencode));
        assert_eq!(parse_executor_type("kimi  "), Some(ExecutorType::Kimi));
    }

    #[test]
    fn test_strip_think_tags_basic() {
        assert_eq!(strip_think_tags("<think>x</think>hello"), "hello");
    }

    #[test]
    fn test_strip_think_tags_multiline() {
        let input = "<think>\nline1\nline2\n</think>result";
        assert_eq!(strip_think_tags(input), "result");
    }

    #[test]
    fn test_strip_think_tags_no_tags() {
        assert_eq!(strip_think_tags("hello world"), "hello world");
    }

    #[test]
    fn test_strip_think_tags_multiple() {
        assert_eq!(strip_think_tags("<think>a</think><think>b</think>c"), "c");
    }

    #[tokio::test]
    async fn test_executor_registry_new_empty() {
        let reg = ExecutorRegistry::new();
        assert!(reg.list_executors().await.is_empty());
    }

    #[tokio::test]
    async fn test_executor_registry_register_and_get() {
        let reg = ExecutorRegistry::new();
        reg.register(mobilecoder::MobilecoderExecutor::new("mobile".to_string())).await;
        assert!(reg.get(ExecutorType::Mobilecoder).await.is_some());
    }

    #[tokio::test]
    async fn test_executor_registry_get_default() {
        let reg = ExecutorRegistry::new();
        reg.register(claude_code::ClaudeCodeExecutor::new("claude".to_string())).await;
        assert!(reg.get_default().await.is_some());
    }

    #[tokio::test]
    async fn test_executor_registry_get_default_when_empty() {
        let reg = ExecutorRegistry::new();
        assert!(reg.get_default().await.is_none());
    }

    #[tokio::test]
    async fn test_executor_registry_list_executors() {
        let reg = ExecutorRegistry::new();
        reg.register(mobilecoder::MobilecoderExecutor::new("mobile".to_string())).await;
        reg.register(claude_code::ClaudeCodeExecutor::new("claude".to_string())).await;
        let list = reg.list_executors().await;
        assert_eq!(list.len(), 2);
        assert!(list.contains(&ExecutorType::Mobilecoder));
        assert!(list.contains(&ExecutorType::Claudecode));
    }

    // 使用一个最小的 mock 实现来测试 trait 默认方法
    struct MockExecutor;

    #[async_trait]
    impl CodeExecutor for MockExecutor {
        fn executor_type(&self) -> ExecutorType { ExecutorType::Mobilecoder }
        fn executable_path(&self) -> &str { "mock" }
        fn command_args(&self, _message: &str) -> Vec<String> { vec![] }
        fn parse_output_line(&self, _line: &str) -> Option<ParsedLogEntry> { None }
        fn get_usage(&self, _logs: &[ParsedLogEntry]) -> Option<ExecutionUsage> { None }
        fn get_model(&self) -> Option<String> { None }
    }

    #[test]
    fn test_code_executor_default_check_success() {
        let exec = MockExecutor;
        assert!(exec.check_success(0));
        assert!(!exec.check_success(1));
        assert!(!exec.check_success(-1));
    }

    #[test]
    fn test_code_executor_default_get_final_result() {
        let exec = MockExecutor;
        let logs = vec![
            ParsedLogEntry::new("info", "start"),
            ParsedLogEntry::new("text", "partial"),
            ParsedLogEntry::new("result", "final answer"),
        ];
        assert_eq!(exec.get_final_result(&logs), Some("final answer".to_string()));
    }

    #[test]
    fn test_code_executor_default_get_final_result_fallback_to_text() {
        let exec = MockExecutor;
        let logs = vec![
            ParsedLogEntry::new("info", "start"),
            ParsedLogEntry::new("text", "only text"),
        ];
        assert_eq!(exec.get_final_result(&logs), Some("only text".to_string()));
    }

    #[test]
    fn test_code_executor_default_get_final_result_no_match() {
        let exec = MockExecutor;
        let logs = vec![ParsedLogEntry::new("info", "start")];
        assert_eq!(exec.get_final_result(&logs), None);
    }
}
