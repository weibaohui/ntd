use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::{ExecutorType, ParsedLogEntry, TodoItem};

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
    ExecutorDef {
        name: "mimo",
        executor_type: ExecutorType::Mimo,
        binary_name: "mimo",
        display_name: "MiMo",
        default_path: "mimo",
        session_dir: "~/.local/share/mimocode",
        aliases: &["mimocode"],
    },
    ExecutorDef {
        name: "zhanlu",
        executor_type: ExecutorType::Zhanlu,
        binary_name: "zl",
        display_name: "Zhanlu",
        default_path: "zl",
        session_dir: "~/.local/share/zhanlu/storage",
        aliases: &["zhanlucode", "zl"],
    },
    ExecutorDef {
        // Kilo: 与 Opencode/Zhanlu 一致的开源 AI 编程执行器。
        // binary_name / default_path 都走 PATH 解析（统一为 `kilo`），
        // session 目录是 ~/.kilo。
        name: "kilo",
        executor_type: ExecutorType::Kilo,
        binary_name: "kilo",
        display_name: "Kilo",
        default_path: "kilo",
        session_dir: "~/.kilo",
        aliases: &[],
    },
];

/// 支持继续对话的执行器集合（与前端 RESUMABLE_EXECUTORS 保持一致）
pub const RESUMABLE_EXECUTORS: &[&str] = &["claudecode", "kimi", "opencode", "mobilecoder", "hermes", "codewhale", "pi", "mimo", "zhanlu", "kilo"];

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
///
/// Fallback 顺序（与 `pipeline.finalize()` 生成 `Result` 的逻辑保持一致）：
///   1. `result` 类型条目 —— `pipeline.finalize()` 从最后一个 `Assistant` 自动生成的结论
///   2. `text` 类型条目（去 think 标签后）—— 部分 executor 自己存储的文本内容
///   3. `stderr` 条目 —— 最后的兜底
pub fn default_final_result_with_think_stripping(logs: &[ParsedLogEntry]) -> Option<String> {
    // 优先取 result 类型（finalize() 生成的结论）
    if let Some(r) = logs.iter().rev().find(|l| l.log_type == "result") {
        return Some(r.content.clone());
    }
    // 其次取 text 条目
    let texts: Vec<String> = logs.iter()
        .filter(|l| l.log_type == "text")
        .map(|l| strip_think_tags(&l.content))
        .filter(|t| !t.trim().is_empty())
        .collect();

    if !texts.is_empty() {
        Some(texts.join("\n\n"))
    } else {
        // 最后兜底 stderr
        logs.iter()
            .rev()
            .find(|l| l.log_type == "stderr")
            .map(|l| l.content.clone())
    }
}

/// 共享的执行器基础状态：path + 可选 model。
///
/// `BaseExecutor` 解决 Issue #504 提到的 10 个 executor 适配器高度重复的问题。
/// 每个具体 executor 之前都要复制粘贴：
/// - `path: String` 字段
/// - `model: Arc<Mutex<Option<String>>>` 字段（部分）
/// - `impl Clone { ... }` 块
/// - `fn executable_path(&self) -> &str { &self.path }`
/// - `fn parse_stderr_line(...)` 默认实现（基于 "error" 关键字判定 log_type）
/// - `fn check_success(...)` 默认实现（exit_code == 0）
/// - `fn get_model(...)` 默认从内部 state 拷贝
///
/// 通过将这三个字段与默认行为集中到 `BaseExecutor`，
/// 具体 executor 只需用 `base: BaseExecutor` 组合，并显式 override 差异部分。
///
/// ## 组合 vs 继承
/// Rust 没有继承，使用结构体组合（struct composition）：
/// ```ignore
/// pub struct CodewhaleExecutor {
///     base: BaseExecutor,
/// }
/// ```
/// 当 executor 需要额外状态（如 `has_successful_finish`、`has_done`、`session_id`）时，
/// 保留自己的额外字段，并通过 `self.base.xxx()` 委托给 base。
#[derive(Clone)]
pub struct BaseExecutor {
    /// 可执行文件路径（构造时确定，执行期不变）
    pub path: String,
    /// 提取自 metadata/result 事件的模型名称，部分 executor 不使用
    pub model: Arc<Mutex<Option<String>>>,
}

impl BaseExecutor {
    /// 构造时初始化三个字段，path 由调用方提供，model/usage 默认为 None。
    pub fn new(path: String) -> Self {
        Self {
            path,
            model: Arc::new(Mutex::new(None)),
        }
    }

    /// 在构造时直接注入已知的 model（可选的便捷构造方式）。
    /// 主要用于执行期前已经从配置中拿到模型名的场景。
    pub fn with_model(self, model: String) -> Self {
        *self.model.lock() = Some(model);
        self
    }

    /// 默认 stderr 解析：根据是否包含 "error" 决定 log_type。
    ///
    /// 之所以是关键字匹配而不是 JSON 解析：
    /// stderr 通常是非结构化输出（普通日志/警告/错误），
    /// 解析成本高且收益低，简单的 "error" 关键字足以让前端区分错误与提示。
    /// 整行去 trim 后写入 content，保证可读性。
    pub fn default_parse_stderr_line(line: &str) -> Option<ParsedLogEntry> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(ParsedLogEntry {
            timestamp: crate::models::utc_timestamp(),
            log_type: if trimmed.to_lowercase().contains("error") {
                "error".to_string()
            } else {
                "stderr".to_string()
            },
            content: trimmed.to_string(),
            usage: None,
            tool_name: None,
            tool_input_json: None,
        })
    }

    /// 默认的退出码判定：0 即成功。
    /// 覆盖此方法的 executor（mimo、codex）需要表达「非零但仍成功」的语义。
    pub fn default_check_success(exit_code: i32) -> bool {
        exit_code == 0
    }
}

/// 适配器层共享的解析与构造 helper（trim/JSON/Entry 构造）。
pub mod helpers;
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
pub mod mimo;
pub mod mimo_event;
pub mod zhanlu;
pub mod zhanlu_event;
pub mod kilo;
pub mod kilo_event;

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
    ///
    /// 默认返回 `None`，与 main 一致；具体 executor 若希望复用关键字分类（"error"
    /// 子串 → log_type="error"）可显式 override 并调用 `BaseExecutor::default_parse_stderr_line`。
    /// 这保留了"6 个原本继承 `None` 默认的 executor（claude_code / codebuddy / mimo /
    /// mobilecoder / opencode / pi）不被静默改变分类行为"的不变量。
    fn parse_stderr_line(&self, _line: &str) -> Option<ParsedLogEntry> {
        None
    }

    /// 是否解析成功（检查退出码）
    ///
    /// 默认实现委托给 `BaseExecutor::default_check_success`。
    /// 仅当需要「非零退出码也算成功」的特殊语义时才覆盖。
    fn check_success(&self, exit_code: i32) -> bool {
        BaseExecutor::default_check_success(exit_code)
    }

    /// 从日志列表中提取最终结果
    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        logs.iter()
            .rev()
            .find(|l| l.log_type == "result" || l.log_type == "text")
            .map(|l| l.content.clone())
    }

    fn get_model(&self) -> Option<String>;

    /// 执行完成后从外部数据源提取 todo 进度（用于无法从 stdout 获取工具调用的执行器）
    fn post_execution_todo_progress(&self) -> Option<Vec<TodoItem>> {
        None
    }

    /// 获取工具调用次数（用于从输出摘要中提取的执行器，如 hermes）
    fn get_tool_calls_count(&self) -> Option<u64> {
        None
    }

    /// 子进程启动后、关闭 stdin 之前要写入的内容。
    ///
    /// 用途：等价于 `echo "<content>" | <executor> ...` 的管道输入。
    /// 默认 `None`（直接关闭 stdin，与重构前一致）；个别执行器需要在 stdin 上预置
    /// 自动应答（例如 pi 在启用 Worktree 切目录后会在交互式 prompt 卡住，
    /// 通过预写 "y" 自动确认目录切换）时可 override 返回 `Some(...)`。
    ///
    /// 实现要求：内容应一次性写入并立刻 flush；写入失败由调用方记录 warning 但不视为致命错误，
    /// 因为 stdin 关闭本身仍能保证子进程正常退出。
    fn stdin_payload(&self) -> Option<String> {
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
            "mimo" => Arc::new(mimo::MimoExecutor::new(path.to_string())),
            "zhanlu" => Arc::new(zhanlu::ZhanluExecutor::new(path.to_string())),
            "kilo" => Arc::new(kilo::KiloExecutor::new(path.to_string())),
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
    use crate::models::ParsedLogEntry;

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
    fn test_parse_executor_type_mimo() {
        assert_eq!(parse_executor_type("mimo"), Some(ExecutorType::Mimo));
        assert_eq!(parse_executor_type("MIMO"), Some(ExecutorType::Mimo));
        assert_eq!(parse_executor_type("mimocode"), Some(ExecutorType::Mimo));
    }

    #[test]
    fn test_parse_executor_type_zhanlu() {
        // Issue #673: zhanlu 必须能被 parse_executor_type 解析为 ExecutorType::Zhanlu
        // 同时别名 zhanlucode / zl 也能解析到 Zhanlu
        assert_eq!(parse_executor_type("zhanlu"), Some(ExecutorType::Zhanlu));
        assert_eq!(parse_executor_type("ZHANLU"), Some(ExecutorType::Zhanlu));
        assert_eq!(parse_executor_type("zhanlucode"), Some(ExecutorType::Zhanlu));
        assert_eq!(parse_executor_type("zl"), Some(ExecutorType::Zhanlu));
    }

    #[test]
    fn test_parse_executor_type_kilo() {
        assert_eq!(parse_executor_type("kilo"), Some(ExecutorType::Kilo));
        assert_eq!(parse_executor_type("KILO"), Some(ExecutorType::Kilo));
        assert_eq!(parse_executor_type("Kilo"), Some(ExecutorType::Kilo));
        assert_eq!(parse_executor_type(" kilo "), Some(ExecutorType::Kilo));
    }

    #[test]
    fn test_find_executor_kilo() {
        let def = find_executor("kilo").expect("kilo should be found");
        assert_eq!(def.name, "kilo");
        assert_eq!(def.binary_name, "kilo");
        assert_eq!(def.display_name, "Kilo");
        assert_eq!(def.default_path, "kilo");
        assert_eq!(def.session_dir, "~/.kilo");
        assert!(def.aliases.is_empty());
        assert_eq!(def.executor_type, ExecutorType::Kilo);
    }

    #[test]
    fn test_resumable_executors_contains_kilo() {
        assert!(RESUMABLE_EXECUTORS.contains(&"kilo"),
            "kilo should be in RESUMABLE_EXECUTORS; current list: {:?}", RESUMABLE_EXECUTORS);
    }

    #[test]
    fn test_create_executor_kilo_returns_kilo_type() {
        let executor = ExecutorRegistry::create_executor("kilo", "/usr/local/bin/kilo")
            .expect("create_executor(\"kilo\", ...) should return Some");
        assert_eq!(executor.executor_type(), ExecutorType::Kilo);
        assert_eq!(executor.executable_path(), "/usr/local/bin/kilo");
    }

    #[test]
    fn test_create_executor_kilo_supports_resume() {
        let executor = ExecutorRegistry::create_executor("kilo", "kilo").unwrap();
        assert!(executor.supports_resume(), "Kilo executor should support resume");
    }

    #[test]
    fn test_kilo_has_no_aliases() {
        // Kilo intentionally has no aliases: only "kilo" maps to Kilo
        assert_eq!(find_executor("kilo").unwrap().aliases.len(), 0);
        // Sanity-check: a made-up alias does not accidentally resolve
        assert!(parse_executor_type("kc").is_none(),
            "\"kc\" should not resolve to any executor including Kilo");
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

    // ====================== default_final_result_with_think_stripping 单元测试 ======================

    #[test]
    fn test_default_final_result_result_type_takes_priority() {
        // pipeline.finalize() 从最后一个 Assistant 生成 result 类型；
        // get_final_result 应优先取 result（与 finalize 语义对齐）
        let logs = vec![
            ParsedLogEntry::new("text", "some text"),
            ParsedLogEntry::new("assistant", "assistant content"),
            ParsedLogEntry::new("result", "final conclusion from assistant"),
        ];
        assert_eq!(
            default_final_result_with_think_stripping(&logs),
            Some("final conclusion from assistant".to_string())
        );
    }

    #[test]
    fn test_default_final_result_text_fallback() {
        // 没有 result 类型时，fallback 到 text（去 think 标签）
        let logs = vec![
            ParsedLogEntry::new("info", "start"),
            ParsedLogEntry::new("text", "hello world"),
        ];
        assert_eq!(
            default_final_result_with_think_stripping(&logs),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn test_default_final_result_stderr_fallback() {
        // 没有 result 也没有 text 时，fallback 到 stderr
        let logs = vec![
            ParsedLogEntry::new("info", "start"),
            ParsedLogEntry::new("stderr", "error output"),
        ];
        assert_eq!(
            default_final_result_with_think_stripping(&logs),
            Some("error output".to_string())
        );
    }

    #[test]
    fn test_default_final_result_strips_think_tags_from_text() {
        let logs = vec![
            ParsedLogEntry::new("text", "<think>thinking content</think>\nactual answer"),
        ];
        assert_eq!(
            default_final_result_with_think_stripping(&logs),
            Some("actual answer".to_string())
        );
    }

    #[test]
    fn test_default_final_result_empty_logs() {
        let logs: Vec<ParsedLogEntry> = vec![];
        assert_eq!(default_final_result_with_think_stripping(&logs), None);
    }

    // ====================== BaseExecutor 单元测试 ======================
    //
    // BaseExecutor 解决 Issue #504 描述的「10 个 executor 适配器高度重复」问题。
    // 这些测试覆盖 BaseExecutor 自身以及它与 CodeExecutor trait 的集成。

    #[test]
    fn test_base_executor_new_initializes_fields() {
        let base = BaseExecutor::new("/usr/local/bin/claude".to_string());
        assert_eq!(base.path, "/usr/local/bin/claude");
        // model 默认为 None
        assert!(base.model.lock().is_none());
    }

    #[test]
    fn test_base_executor_with_model() {
        let base = BaseExecutor::new("claude".to_string()).with_model("claude-3-5-sonnet".to_string());
        assert_eq!(base.model.lock().clone(), Some("claude-3-5-sonnet".to_string()));
    }

    #[test]
    fn test_base_executor_clone_shares_arc_state() {
        // Clone 应当 clone Arc 引用，让克隆体共享内部状态；
        // 这是原 impl Clone 行为的关键不变量，refactor 不能破坏。
        let base = BaseExecutor::new("claude".to_string());
        let cloned = base.clone();
        *base.model.lock() = Some("gpt-4".to_string());
        // 克隆体能读到原始 base 写入的 model 状态
        assert_eq!(cloned.model.lock().clone(), Some("gpt-4".to_string()));
    }

    #[test]
    fn test_base_executor_default_parse_stderr_line_empty() {
        // 空行（trim 后）应返回 None，前端不会渲染空白行
        assert!(BaseExecutor::default_parse_stderr_line("").is_none());
        assert!(BaseExecutor::default_parse_stderr_line("   ").is_none());
    }

    #[test]
    fn test_base_executor_default_parse_stderr_line_error_keyword() {
        let entry = BaseExecutor::default_parse_stderr_line("ERROR: something failed").unwrap();
        // "error" 关键字命中，log_type 为 "error"，content 保留 trim 后的原文
        assert_eq!(entry.log_type, "error");
        assert_eq!(entry.content, "ERROR: something failed");
    }

    #[test]
    fn test_base_executor_default_parse_stderr_line_error_keyword_case_insensitive() {
        // 大小写不敏感是 .to_lowercase().contains("error") 的设计目标
        let entry = BaseExecutor::default_parse_stderr_line("Error: failed").unwrap();
        assert_eq!(entry.log_type, "error");
    }

    #[test]
    fn test_base_executor_default_parse_stderr_line_info_keyword() {
        // 非 error 行 → "stderr"（保留为通用 stderr 流，前端用「stderr」图标渲染）
        let entry = BaseExecutor::default_parse_stderr_line("Just some info").unwrap();
        assert_eq!(entry.log_type, "stderr");
        assert_eq!(entry.content, "Just some info");
    }

    #[test]
    fn test_base_executor_default_parse_stderr_line_trims_whitespace() {
        // 前后空白应被 trim 掉，避免前端显示时的多余边距
        let entry = BaseExecutor::default_parse_stderr_line("  hello  ").unwrap();
        assert_eq!(entry.content, "hello");
    }

    #[test]
    fn test_base_executor_default_check_success_zero_is_success() {
        assert!(BaseExecutor::default_check_success(0));
    }

    #[test]
    fn test_base_executor_default_check_success_non_zero_is_failure() {
        // 非零退出码默认视为失败
        assert!(!BaseExecutor::default_check_success(1));
        assert!(!BaseExecutor::default_check_success(127));
        assert!(!BaseExecutor::default_check_success(-1));
    }

    /// 一个最小化的 executor：直接 wrap BaseExecutor，
    /// 用来验证「组合后 BaseExecutor 提供的字段能正确暴露给 trait 方法」。
    struct BaseWrapExecutor {
        base: BaseExecutor,
    }

    impl BaseWrapExecutor {
        fn new(path: String) -> Self {
            Self { base: BaseExecutor::new(path) }
        }
    }

    impl Clone for BaseWrapExecutor {
        fn clone(&self) -> Self {
            Self { base: self.base.clone() }
        }
    }

    #[async_trait]
    impl CodeExecutor for BaseWrapExecutor {
        fn executor_type(&self) -> ExecutorType { ExecutorType::Claudecode }
        fn executable_path(&self) -> &str { &self.base.path }
        fn command_args(&self, _message: &str) -> Vec<String> { vec![] }
        fn parse_output_line(&self, _line: &str) -> Option<ParsedLogEntry> { None }
        fn get_model(&self) -> Option<String> {
            self.base.model.lock().clone()
        }
    }

    #[test]
    fn test_base_wrap_executor_executable_path_uses_base() {
        let exec = BaseWrapExecutor::new("/opt/claude".to_string());
        assert_eq!(exec.executable_path(), "/opt/claude");
    }

    #[test]
    fn test_base_wrap_executor_default_check_success_via_trait() {
        // 默认的 trait check_success 应该走 BaseExecutor::default_check_success
        let exec = BaseWrapExecutor::new("claude".to_string());
        assert!(exec.check_success(0));
        assert!(!exec.check_success(1));
    }

    #[test]
    fn test_base_wrap_executor_default_parse_stderr_via_trait() {
        // trait 默认的 parse_stderr_line 返回 None（与 main 一致，保留 6 个 executor
        // 不被静默改变分类行为的不变量）。keyword 分类需 executor 显式 override 并
        // 调用 BaseExecutor::default_parse_stderr_line。
        let exec = BaseWrapExecutor::new("claude".to_string());
        assert!(exec.parse_stderr_line("ERROR: bad").is_none());
    }

    #[test]
    fn test_base_wrap_executor_get_model_through_base() {
        let exec = BaseWrapExecutor::new("claude".to_string());
        // 写入 base 的共享状态
        *exec.base.model.lock() = Some("claude-3-5-sonnet".to_string());
        // 通过 trait 方法读出
        assert_eq!(exec.get_model(), Some("claude-3-5-sonnet".to_string()));
    }
}
