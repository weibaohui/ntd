use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TodoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for TodoStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Running,
    Success,
    Failed,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ExecutionStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "success" => Ok(Self::Success),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("unknown execution status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: i64,
    pub title: String,
    pub prompt: String,
    pub status: TodoStatus,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    #[serde(default)]
    pub executor: Option<String>,
    #[serde(default)]
    pub scheduler_enabled: bool,
    #[serde(default)]
    pub scheduler_config: Option<String>,
    #[serde(default)]
    pub scheduler_timezone: Option<String>,
    #[serde(default)]
    pub scheduler_next_run_at: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub worktree_enabled: bool,
    /// Inline hooks owned by this todo. Parsed from the `todos.hooks` column.
    #[serde(default)]
    pub hooks: Vec<crate::hooks::TodoHookItem>,
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    /// 0=normal, 1=reviewer_template(评审师模板), 2=review_instance(评审实例).
    #[serde(default)]
    pub todo_type: i32,
    /// review_instance 关联到被评审的原 todo; 其它类型为 None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_todo_id: Option<i64>,
    /// 是否在执行完成后自动派生一个评审 todo. 只对 normal 类型有意义.
    #[serde(default = "default_true")]
    pub auto_review_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBot {
    pub id: i64,
    pub bot_type: String,
    pub bot_name: String,
    pub app_id: String,
    #[serde(skip_serializing)]
    pub app_secret: String,
    pub bot_open_id: Option<String>,
    pub domain: Option<String>,
    pub enabled: bool,
    pub config: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    #[serde(default = "default_true")]
    pub dm_enabled: bool,
    #[serde(default = "default_true")]
    pub group_enabled: bool,
    #[serde(default = "default_true")]
    pub group_require_mention: bool,
    #[serde(default = "default_true")]
    pub echo_reply: bool,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            dm_enabled: true,
            group_enabled: true,
            group_require_mention: true,
            echo_reply: true,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_trigger_type() -> String { "manual".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub id: i64,
    pub todo_id: i64,
    pub status: ExecutionStatus,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub result: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub usage: Option<ExecutionUsage>,
    pub executor: Option<String>,
    pub model: Option<String>,
    #[serde(default = "default_trigger_type")]
    pub trigger_type: String,
    #[serde(default)]
    pub pid: Option<i32>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_progress: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_stats: Option<ExecutionStats>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_message: Option<String>,
    /// Hook trigger provenance: the source todo that fired this execution.
    /// `Some` only when `trigger_type` starts with `hook:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_todo_id: Option<i64>,
    /// Snapshot of the source todo's title at trigger time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_todo_title: Option<String>,
    /// The `TodoHookItem.id` that triggered this execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hook_id: Option<i64>,
    /// User-provided score for this execution's result (0-100, optional).
    /// Only meaningful on terminal records (success/failed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<i32>,
    /// 自动评审时, 评审记录精确指向被评审的"原执行记录"。
    /// 这条记录的 rating 应被视为对 source_execution_record_id 的评分.
    /// NULL = 这条记录不是被自动评审的产物.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_execution_record_id: Option<i64>,
    /// 这条原执行记录最近一次自动评审的状态.
    /// 仅在原执行记录上有意义; 评审实例自己的 execution_record 该字段为 NULL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_review_status: Option<String>,
    /// 这条原执行记录最近一次自动评审 spawn 的 UTC 时间.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reviewed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub total_cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStats {
    pub tool_calls: u64,
    pub conversation_turns: u64,
    pub thinking_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub todo_id: i64,
    pub total_executions: i64,
    pub success_count: i64,
    pub failed_count: i64,
    pub running_count: i64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: Option<String>,
    pub content: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedLogEntry {
    pub timestamp: String,
    #[serde(rename = "type")]
    pub log_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ExecutionUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input_json: Option<String>,
}

impl ParsedLogEntry {
    pub fn new(log_type: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            timestamp: utc_timestamp(),
            log_type: log_type.into(),
            content: content.into(),
            usage: None,
            tool_name: None,
            tool_input_json: None,
        }
    }

    pub fn info(content: impl Into<String>) -> Self {
        Self::new("info", content)
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self::new("error", content)
    }

    pub fn stderr(content: impl Into<String>) -> Self {
        Self::new("stderr", content)
    }

    pub fn with_usage(mut self, usage: ExecutionUsage) -> Self {
        self.usage = Some(usage);
        self
    }
}

// Request/Response types
#[derive(Deserialize, Serialize)]
pub struct CreateTodoRequest {
    pub title: String,
    pub prompt: String,
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    #[serde(default)]
    pub executor: Option<String>,
    #[serde(default)]
    pub scheduler_enabled: Option<bool>,
    #[serde(default)]
    pub scheduler_config: Option<String>,
    #[serde(default)]
    pub scheduler_timezone: Option<String>,
    #[serde(default)]
    pub hooks: Option<Vec<crate::hooks::TodoHookItem>>,
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    #[serde(default)]
    pub auto_review_enabled: Option<bool>,
}

#[derive(Deserialize, Serialize)]
pub struct UpdateTodoRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub status: Option<TodoStatus>,
    #[serde(default)]
    pub executor: Option<String>,
    #[serde(default)]
    pub scheduler_enabled: Option<bool>,
    #[serde(default)]
    pub scheduler_config: Option<String>,
    #[serde(default)]
    pub scheduler_timezone: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub worktree_enabled: Option<bool>,
    /// Replace the todo's inline hooks. `None` keeps the existing list.
    #[serde(default)]
    pub hooks: Option<Vec<crate::hooks::TodoHookItem>>,
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    /// None=不变, Some(true)/Some(false)=更新. 不允许改 reviewer template 的开关.
    #[serde(default)]
    pub auto_review_enabled: Option<bool>,
}

#[derive(Deserialize, Serialize)]
pub struct UpdateTagsRequest {
    pub tag_ids: Vec<i64>,
}

#[derive(Deserialize, Serialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub color: String,
}

#[derive(Deserialize, Serialize)]
pub struct ExecuteRequest {
    pub todo_id: i64,
    pub message: Option<String>,
    pub executor: Option<String>,
    #[serde(default)]
    pub params: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct SmartCreateRequest {
    pub content: String,
}

#[derive(Deserialize)]
pub struct TodoIdQuery {
    #[serde(default)]
    pub todo_id: Option<i64>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningBoardResponse {
    pub records: Vec<ExecutionRecord>,
    pub scheduled_todos: Vec<Todo>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecordsPage {
    pub records: Vec<ExecutionRecord>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLogsPage {
    pub logs: Vec<ParsedLogEntry>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorCount {
    pub executor: String,
    pub count: i64,
    pub execution_count: i64,
    pub success_count: i64,
    pub failed_count: i64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagCount {
    pub tag_id: i64,
    pub tag_name: String,
    pub tag_color: String,
    pub count: i64,
    pub execution_count: i64,
    pub success_count: i64,
    pub failed_count: i64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCount {
    pub model: String,
    pub count: i64,
    pub execution_count: i64,
    pub success_count: i64,
    pub failed_count: i64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyExecution {
    pub date: String,
    pub success: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyTokenStats {
    pub date: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_todos: i64,
    pub pending_todos: i64,
    pub running_todos: i64,
    pub completed_todos: i64,
    pub failed_todos: i64,
    pub total_tags: i64,
    pub scheduled_todos: i64,
    pub total_executions: i64,
    pub success_executions: i64,
    pub failed_executions: i64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cost_usd: f64,
    pub avg_duration_ms: u64,
    pub executor_distribution: Vec<ExecutorCount>,
    pub tag_distribution: Vec<TagCount>,
    pub model_distribution: Vec<ModelCount>,
    pub daily_executions: Vec<DailyExecution>,
    pub daily_token_stats: Vec<DailyTokenStats>,
    pub recent_executions: Vec<ExecutionRecord>,
    pub trigger_type_distribution: Vec<TriggerTypeCount>,
    pub executor_duration_stats: Vec<ExecutorDuration>,
    pub model_cache_stats: Vec<ModelCacheStat>,
    // Enhanced metrics
    pub today_executions: i64,
    pub executions_change: Option<f64>,
    pub success_rate_change: Option<f64>,
    pub cost_change: Option<f64>,
    pub active_days: i64,
    pub streak_days: i64,
    pub peak_daily_executions: i64,
    pub top_model: Option<String>,
    pub top_model_tokens: Option<u64>,
    pub leaderboard: Vec<LeaderboardItem>,
    // Skills metrics
    pub skills_stats: Option<SkillsStats>,
    // Backup metrics
    pub backup_stats: Option<BackupStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardItem {
    pub rank: i32,
    pub name: String,
    pub tokens: u64,
    pub sessions: i64,
    pub change: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentCompletedTodo {
    pub todo_id: i64,
    pub title: String,
    pub prompt: Option<String>,
    pub executor: Option<String>,
    pub tag_ids: Vec<i64>,
    pub completed_at: String,
    pub result: Option<String>,
    pub model: Option<String>,
    pub usage: Option<ExecutionUsage>,
    pub execution_status: String,
    pub trigger_type: String,
    pub record_id: i64,
    /// User-provided score for the most recent execution record (0-100).
    /// Mirrors `ExecutionRecord::rating` so that the conclusion/memorial view
    /// can render the score badge without an extra round-trip per card.
    pub rating: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerTypeCount {
    pub trigger_type: String,
    pub count: i64,
    pub success_count: i64,
    pub failed_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorDuration {
    pub executor: String,
    pub avg_duration_ms: f64,
    pub execution_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCacheStat {
    pub model: String,
    pub total_input_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub cache_hit_rate: f64,
}

// Skills invocation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsStats {
    pub total_invocations: i64,
    pub success_invocations: i64,
    pub failed_invocations: i64,
    pub avg_duration_ms: f64,
    pub invocations_today: i64,
    pub top_skills: Vec<SkillTop>,
    pub executor_skills_count: Vec<ExecutorSkillCount>,
    pub daily_invocations: Vec<DailySkillInvocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTop {
    pub skill_name: String,
    pub count: i64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorSkillCount {
    pub executor: String,
    pub skills_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySkillInvocation {
    pub date: String,
    pub count: i64,
    pub success: i64,
}

// Backup statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStats {
    pub auto_backup_enabled: bool,
    pub last_backup: Option<String>,
    pub auto_backup_cron: String,
    pub database: BackupCategoryStats,
    pub todo: BackupCategoryStats,
    pub skills: BackupCategoryStats,
    pub total_file_count: i64,
    pub total_size: i64,
    pub total_size_formatted: String,
    pub recent_backups: Vec<RecentBackup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupCategoryStats {
    pub file_count: i64,
    pub total_size: i64,
    pub last_backup: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentBackup {
    #[serde(rename = "type")]
    pub backup_type: String,
    pub name: String,
    pub size: i64,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct UpdateSchedulerRequest {
    pub scheduler_enabled: bool,
    pub scheduler_config: Option<String>,
    pub scheduler_timezone: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateConfigRequest {
    pub port: Option<u16>,
    pub host: Option<String>,
    pub db_path: Option<String>,
    pub log_level: Option<String>,
    pub slash_command_rules: Option<Vec<crate::config::SlashCommandRule>>,
    pub default_response_todo_id: Option<i64>,
    pub history_message_max_age_secs: Option<u64>,
    pub max_concurrent_todos: Option<u32>,
    pub execution_timeout_secs: Option<u64>,
    pub scheduler_default_timezone: Option<String>,
    /// WebSocket broadcast channel 容量。修改后需要重启服务才会在新连接上生效。
    pub broadcast_channel_capacity: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuMessageStats {
    pub total_messages: i64,
    pub processed: i64,
    pub unprocessed: i64,
    pub triggered_todos: i64,
    pub unique_senders: i64,
    pub last_24h_messages: i64,
    pub unique_chats: i64,
}

/// Placeholder chat_id for bindings created via Web UI before Feishu /bind.
pub const PENDING_CHAT_ID: &str = "__pending__";

/// Binding status constants — ensure consistency across DB writes and reads.
pub mod binding_status {
    pub const IDLE: &str = "idle";
    pub const RUNNING: &str = "running";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub enabled: bool,
    pub display_name: String,
    pub session_dir: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateExecutorRequest {
    pub path: Option<String>,
    pub enabled: Option<bool>,
    pub display_name: Option<String>,
    pub session_dir: Option<String>,
}

#[derive(Serialize)]
pub struct ExecutorDetectResult {
    pub binary_found: bool,
    pub path_resolved: Option<String>,
}

/// Result of a repair operation on a single executor.
#[derive(Serialize)]
pub struct ExecutorRepairResult {
    /// Whether the binary was found and path was updated.
    pub repaired: bool,
    /// The new path written to DB (None if not found or unchanged).
    pub new_path: Option<String>,
    /// Human-readable message.
    pub message: String,
}

/// resolve 操作用的结果：包含检测结果 + 是否触发了数据库更新
#[derive(Serialize)]
pub struct ExecutorPathResolveResult {
    pub binary_found: bool,
    pub path_resolved: Option<String>,
    /// 数据库路径是否被更新（仅在 binary_found=true 且路径与原值不同时为 true）
    pub path_updated: bool,
    pub old_path: Option<String>,
    pub new_path: Option<String>,
}

#[derive(Serialize)]
pub struct ExecutorTestResult {
    pub test_passed: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct ExecutorBatchDetectResult {
    pub results: Vec<ExecutorDetectInfo>,
    pub total: usize,
    pub found_count: usize,
}

#[derive(Serialize)]
pub struct ExecutorDetectInfo {
    pub name: String,
    pub display_name: String,
    pub binary_found: bool,
    pub path_resolved: Option<String>,
    pub enabled: bool,
}

// Executor types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ExecutorType {
    Mobilecoder,
    #[default]
    Claudecode,
    Codebuddy,
    Opencode,
    Atomcode,
    Hermes,
    Kimi,
    Codex,
    Codewhale,
    Pi,
    Mimo,
}


impl ExecutorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutorType::Mobilecoder => "mobilecoder",
            ExecutorType::Claudecode => "claudecode",
            ExecutorType::Codebuddy => "codebuddy",
            ExecutorType::Opencode => "opencode",
            ExecutorType::Atomcode => "atomcode",
            ExecutorType::Hermes => "hermes",
            ExecutorType::Kimi => "kimi",
            ExecutorType::Codex => "codex",
            ExecutorType::Codewhale => "codewhale",
            ExecutorType::Pi => "pi",
            ExecutorType::Mimo => "mimo",
        }
    }
}

impl std::fmt::Display for ExecutorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// Unified API Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub data: Option<T>,
    pub message: String,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self { code: 0, data: Some(data), message: "ok".to_string() }
    }

    pub fn err(code: i32, message: &str) -> Self {
        Self { code, data: None, message: message.to_string() }
    }
}

pub type ClientResponse<T> = ApiResponse<T>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoTemplate {
    pub id: i64,
    pub title: String,
    pub prompt: Option<String>,
    pub category: String,
    pub sort_order: i32,
    pub is_system: bool,
    pub source_url: Option<String>,
    pub last_sync_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTemplateRequest {
    pub title: String,
    pub prompt: Option<String>,
    pub category: String,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTemplateRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub sort_order: Option<i32>,
}

/// 导入导出备份数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupData {
    pub version: String,
    pub created_at: String,
    pub tags: Vec<TagBackup>,
    pub todos: Vec<TodoBackup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagBackup {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoBackup {
    pub title: String,
    pub prompt: String,
    pub status: TodoStatus,
    pub executor: Option<String>,
    pub scheduler_enabled: bool,
    pub scheduler_config: Option<String>,
    pub tag_names: Vec<String>,
    pub workspace: Option<String>,
    pub worktree: Option<String>,
}

// Business error codes
pub mod codes {
    pub const NOT_FOUND: i32 = 40001;
    pub const BAD_REQUEST: i32 = 40002;
    pub const INTERNAL: i32 = 50001;
}

/// 返回当前 UTC 时间的 ISO 8601 格式字符串 (2024-01-15T08:30:00.000Z)
pub fn utc_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todo_status_as_str() {
        assert_eq!(TodoStatus::Pending.as_str(), "pending");
        assert_eq!(TodoStatus::InProgress.as_str(), "in_progress");
        assert_eq!(TodoStatus::Running.as_str(), "running");
        assert_eq!(TodoStatus::Completed.as_str(), "completed");
        assert_eq!(TodoStatus::Failed.as_str(), "failed");
        assert_eq!(TodoStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn test_todo_status_from_str() {
        assert_eq!("pending".parse::<TodoStatus>().unwrap(), TodoStatus::Pending);
        assert_eq!("in_progress".parse::<TodoStatus>().unwrap(), TodoStatus::InProgress);
        assert_eq!("running".parse::<TodoStatus>().unwrap(), TodoStatus::Running);
        assert_eq!("completed".parse::<TodoStatus>().unwrap(), TodoStatus::Completed);
        assert_eq!("failed".parse::<TodoStatus>().unwrap(), TodoStatus::Failed);
        assert_eq!("cancelled".parse::<TodoStatus>().unwrap(), TodoStatus::Cancelled);
        assert!("unknown".parse::<TodoStatus>().is_err());
    }

    #[test]
    fn test_todo_status_display() {
        assert_eq!(format!("{}", TodoStatus::Running), "running");
        assert_eq!(format!("{}", TodoStatus::Completed), "completed");
    }

    #[test]
    fn test_todo_status_serde() {
        let status = TodoStatus::Pending;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"pending\"");
        let de: TodoStatus = serde_json::from_str("\"failed\"").unwrap();
        assert_eq!(de, TodoStatus::Failed);
    }

    #[test]
    fn test_execution_status_as_str() {
        assert_eq!(ExecutionStatus::Running.as_str(), "running");
        assert_eq!(ExecutionStatus::Success.as_str(), "success");
        assert_eq!(ExecutionStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn test_execution_status_display() {
        assert_eq!(format!("{}", ExecutionStatus::Success), "success");
    }

    #[test]
    fn test_execution_status_serde() {
        let json = serde_json::to_string(&ExecutionStatus::Running).unwrap();
        assert_eq!(json, "\"running\"");
        let de: ExecutionStatus = serde_json::from_str("\"success\"").unwrap();
        assert_eq!(de, ExecutionStatus::Success);
    }

    #[test]
    fn test_executor_type_as_str() {
        assert_eq!(ExecutorType::Mobilecoder.as_str(), "mobilecoder");
        assert_eq!(ExecutorType::Claudecode.as_str(), "claudecode");
        assert_eq!(ExecutorType::Codebuddy.as_str(), "codebuddy");
        assert_eq!(ExecutorType::Opencode.as_str(), "opencode");
        assert_eq!(ExecutorType::Atomcode.as_str(), "atomcode");
    }

    #[test]
    fn test_executor_type_default() {
        assert_eq!(ExecutorType::default(), ExecutorType::Claudecode);
    }

    #[test]
    fn test_parsed_log_entry_new() {
        let entry = ParsedLogEntry::new("info", "hello");
        assert_eq!(entry.log_type, "info");
        assert_eq!(entry.content, "hello");
        assert!(entry.usage.is_none());
    }

    #[test]
    fn test_parsed_log_entry_info() {
        let entry = ParsedLogEntry::info("msg");
        assert_eq!(entry.log_type, "info");
        assert_eq!(entry.content, "msg");
    }

    #[test]
    fn test_parsed_log_entry_error() {
        let entry = ParsedLogEntry::error("msg");
        assert_eq!(entry.log_type, "error");
        assert_eq!(entry.content, "msg");
    }

    #[test]
    fn test_parsed_log_entry_stderr() {
        let entry = ParsedLogEntry::stderr("msg");
        assert_eq!(entry.log_type, "stderr");
        assert_eq!(entry.content, "msg");
    }

    #[test]
    fn test_parsed_log_entry_with_usage() {
        let entry = ParsedLogEntry::info("msg").with_usage(ExecutionUsage {
            input_tokens: 10,
            output_tokens: 20,
            cache_read_input_tokens: Some(5),
            cache_creation_input_tokens: None,
            total_cost_usd: Some(0.001),
            duration_ms: Some(100),
        });
        assert!(entry.usage.is_some());
        let usage = entry.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 20);
    }

    #[test]
    fn test_api_response_ok() {
        let resp = ApiResponse::ok(42);
        assert_eq!(resp.code, 0);
        assert_eq!(resp.data, Some(42));
        assert_eq!(resp.message, "ok");
    }

    #[test]
    fn test_api_response_err() {
        let resp = ApiResponse::<i32>::err(40001, "bad request");
        assert_eq!(resp.code, 40001);
        assert!(resp.data.is_none());
        assert_eq!(resp.message, "bad request");
    }

    #[test]
    fn test_utc_timestamp_format() {
        let ts = utc_timestamp();
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 24); // 2024-01-15T08:30:00.000Z
        assert!(chrono::DateTime::parse_from_rfc3339(&ts).is_ok());
    }

    #[test]
    fn test_create_todo_request_deserialize() {
        let json = r#"{"title":"Test","prompt":"Do this","tag_ids":[1,2]}"#;
        let req: CreateTodoRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, "Test");
        assert_eq!(req.prompt, "Do this");
        assert_eq!(req.tag_ids, vec![1, 2]);
    }

    #[test]
    fn test_create_todo_request_default_tag_ids() {
        let json = r#"{"title":"Test","prompt":"Do this"}"#;
        let req: CreateTodoRequest = serde_json::from_str(json).unwrap();
        assert!(req.tag_ids.is_empty());
    }

    #[test]
    fn test_update_todo_request_deserialize() {
        let json = r#"{"title":"Test","prompt":"Do this","status":"running","executor":"claudecode","scheduler_enabled":true,"scheduler_config":"0 0 * * *"}"#;
        let req: UpdateTodoRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, Some("Test".to_string()));
        assert_eq!(req.executor, Some("claudecode".to_string()));
        assert_eq!(req.scheduler_enabled, Some(true));
        assert_eq!(req.scheduler_config, Some("0 0 * * *".to_string()));
    }

    #[test]
    fn test_update_todo_request_defaults() {
        let json = r#"{"title":"Test","prompt":"Do this","status":"pending"}"#;
        let req: UpdateTodoRequest = serde_json::from_str(json).unwrap();
        assert!(req.executor.is_none());
        assert!(req.scheduler_enabled.is_none());
        assert!(req.scheduler_config.is_none());
    }
}

/// Replace placeholders in a string using a map of key-value pairs.
/// Format: `{{key}}` will be replaced with the corresponding value from the map.
/// If a key is not found in the map, it remains unchanged.
pub fn replace_placeholders(text: &str, params: &std::collections::HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (key, value) in params {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Build standard trigger params from message content.
/// This unifies how params are constructed across slash commands, default responses,
/// and other trigger types.
///
/// Returns (trigger_type, params):
/// - For slash commands (content starts with '/'): trigger_type = "slash_command"
/// - For other messages: trigger_type = "default_response"
///
/// Standard params always include:
/// - `content`: the message body
/// - `message`: the message body
/// - `raw_message`: full raw message (for slash commands, includes the command prefix)
pub fn build_trigger_params(content: &str) -> (String, std::collections::HashMap<String, String>) {
    let trimmed = content.trim();

    if trimmed.starts_with('/') {
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let command = parts.next().unwrap_or("").trim();
        let body = parts.next().unwrap_or("").trim();

        if !body.is_empty() {
            let mut params = std::collections::HashMap::new();
            params.insert("content".to_string(), body.to_string());
            params.insert("message".to_string(), body.to_string());
            params.insert(
                "raw_message".to_string(),
                format!("{} {}", command, body).trim().to_string(),
            );
            params.insert("slash_command".to_string(), command.to_string());
            return ("slash_command".to_string(), params);
        }
    }

    let mut params = std::collections::HashMap::new();
    params.insert("content".to_string(), trimmed.to_string());
    params.insert("message".to_string(), trimmed.to_string());
    params.insert("raw_message".to_string(), trimmed.to_string());
    ("default_response".to_string(), params)
}

#[cfg(test)]
mod placeholder_tests {
    use super::*;

    #[test]
    fn test_replace_placeholders() {
        let mut params = std::collections::HashMap::new();
        params.insert("name".to_string(), "Alice".to_string());
        params.insert("task".to_string(), "review code".to_string());

        let text = "Hello {{name}}, please {{task}}.";
        let result = replace_placeholders(text, &params);
        assert_eq!(result, "Hello Alice, please review code.");
    }

    #[test]
    fn test_replace_placeholders_missing_key() {
        let mut params = std::collections::HashMap::new();
        params.insert("name".to_string(), "Bob".to_string());

        let text = "Hello {{name}}, please {{unknown}}.";
        let result = replace_placeholders(text, &params);
        assert_eq!(result, "Hello Bob, please {{unknown}}.");
    }

    #[test]
    fn test_replace_placeholders_empty_params() {
        let params = std::collections::HashMap::new();
        let text = "Hello {{name}}!";
        let result = replace_placeholders(text, &params);
        assert_eq!(result, "Hello {{name}}!");
    }

    #[test]
    fn test_build_trigger_params_slash_command() {
        let (trigger_type, params) = build_trigger_params("/help some query");
        assert_eq!(trigger_type, "slash_command");
        assert_eq!(params.get("content"), Some(&"some query".to_string()));
        assert_eq!(params.get("message"), Some(&"some query".to_string()));
        assert_eq!(params.get("raw_message"), Some(&"/help some query".to_string()));
        assert_eq!(params.get("slash_command"), Some(&"/help".to_string()));
    }

    #[test]
    fn test_build_trigger_params_default_response() {
        let (trigger_type, params) = build_trigger_params("hello world");
        assert_eq!(trigger_type, "default_response");
        assert_eq!(params.get("content"), Some(&"hello world".to_string()));
        assert_eq!(params.get("message"), Some(&"hello world".to_string()));
        assert_eq!(params.get("raw_message"), Some(&"hello world".to_string()));
        assert!(params.get("slash_command").is_none());
    }

    #[test]
    fn test_build_trigger_params_slash_only_no_body() {
        let (trigger_type, params) = build_trigger_params("/help");
        assert_eq!(trigger_type, "default_response");
        assert_eq!(params.get("content"), Some(&"/help".to_string()));
        assert_eq!(params.get("message"), Some(&"/help".to_string()));
        assert!(params.get("slash_command").is_none());
    }
}

/// Property-based tests for `replace_placeholders`.
///
/// 不变量设计:
/// 1. **空参数 → 恒等映射**: 没有占位符可替换时,函数是 no-op。
/// 2. **已替换消失**: 如果值本身不含 `{{key}}`,那么替换之后输入里所有的
///    `{{key}}` 都应消失(被替换成 value)。
/// 3. **未提供的 key 保持原样**: key 不在 params 里的占位符必须保留为
///    `{{key}}` 形态,不能误吃其它 key 的同名占位符。
/// 4. **无 `{{}}` 模式 → 不变**: 文本里完全没有占位符语法时,函数是恒等映射。
///
/// 这些不变量是 issue #514 引入 property-based testing 的起点;
/// 后续如果出现新解析器/转义语义,可在此扩展。
#[cfg(test)]
mod replace_placeholders_proptests {
    use super::replace_placeholders;
    use proptest::prelude::*;

    /// 任意 ASCII 文本,用作待替换的输入。
    fn text_strategy() -> BoxedStrategy<String> {
        // 用 `any::<String>()` 太宽,容易产生包含 `{{` `}}` 的字符串,
        // 与 `{{key}}` 边界冲突。这里只接受不含 `{{` `}}` 的字符串,
        // 保证测试焦点在"已知占位符的替换行为"。
        "[^{}]*".boxed()
    }

    /// 不包含 `{` `}` 的安全值,避免替换后再被下一轮替换误吃。
    fn safe_value_strategy() -> BoxedStrategy<String> {
        "[^\\{\\}]*".boxed()
    }

    /// 简单的 key 名:字母数字下划线短串,匹配实际模板里 `{{name}}`、
    /// `{{message}}` 等命名风格。
    fn key_strategy() -> BoxedStrategy<String> {
        "[a-zA-Z_][a-zA-Z0-9_]{0,16}".boxed()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// 空参数映射:任意文本都应该原样返回。
        #[test]
        fn empty_params_is_identity(text in text_strategy()) {
            let params = std::collections::HashMap::new();
            let result = replace_placeholders(&text, &params);
            prop_assert_eq!(result, text);
        }

        /// 文本里没有 `{{}}` 占位符时,无论 params 是否非空,都应原样返回。
        /// 反过来,如果替换改变了无占位符的文本,说明解析逻辑出 bug
        /// (例如误把 `${...}` 当成占位符)。
        #[test]
        fn no_placeholder_means_no_change(
            text in text_strategy(),
            (k, v) in (key_strategy(), safe_value_strategy()),
        ) {
            let mut params = std::collections::HashMap::new();
            params.insert(k, v);
            let result = replace_placeholders(&text, &params);
            prop_assert_eq!(result, text);
        }

        /// 当所有 key 都"安全"(value 不含占位符语法)时,替换后结果里
        /// 不应再出现任何 `{{key}}` 模式。换句话说:出现过的占位符必
        /// 须被吃掉;否则等于模板没生效。
        #[test]
        fn all_placeholders_get_replaced(
            key in key_strategy(),
            value in safe_value_strategy(),
            prefix in text_strategy(),
            suffix in text_strategy(),
        ) {
            let mut params = std::collections::HashMap::new();
            params.insert(key.clone(), value.clone());
            // 手工拼模板的 `{{` / `}}`,避免 `format!` 在转义上的歧义。
            // `{{` `}}` 在源码里出现时,format! 解析规则稍不慎就会被
            // 误解为 positional arg。
            let open = "{{".to_string();
            let close = "}}".to_string();
            let template = format!(
                "{prefix}{open}{key}{close}{suffix}",
                key = key,
                open = open,
                close = close,
            );
            let placeholder_pattern = format!(
                "{open}{key}{close}",
                key = key,
                open = "{{",
                close = "}}",
            );
            let result = replace_placeholders(&template, &params);
            // 占位符模式必须消失。
            let msg = format!(
                "placeholder {open}{key}{close} should be gone, got: {result}",
                key = key, open = "{{", close = "}}",
            );
            prop_assert!(!result.contains(&placeholder_pattern), "{}", msg);
            // prefix/suffix 应该原样保留。
            prop_assert!(result.starts_with(&prefix));
            prop_assert!(result.ends_with(&suffix));
        }

        /// key 不在 params 里时,占位符必须保留原文。
        /// 这是替换函数最容易踩的坑:把 `{{user}}` 当成 `{{users}}` 的子串
        /// 误吃,或者试图"补全"未声明的 key。
        #[test]
        fn missing_key_preserves_placeholder(
            keys in (key_strategy(), key_strategy())
                .prop_filter("keys must differ", |(d, u)| d != u),
            declared_value in safe_value_strategy(),
            prefix in text_strategy(),
            suffix in text_strategy(),
        ) {
            // 策略里直接产生 "declared 和 undeclared" 二元组,避免
            // proptest 闭包跨策略参数捕获的语法坑 (move closure 写法
            // 在新版 proptest 里不稳定)。这里解构后取两个 key。
            let (declared_key, undeclared_key) = keys;
            let mut params = std::collections::HashMap::new();
            params.insert(declared_key.clone(), declared_value.clone());
            // 手工拼接模板的 `{{` / `}}`,避免 `format!` 在占位符
            // 转义上的歧义 —— 写成 `format!("{{{{ {} }}}}", key)`
            // 容易被 format 解析为 1 个 positional arg。
            let open = "{{".to_string();
            let close = "}}".to_string();
            let template = format!(
                "{prefix}{open}{dk}{close}{open}{uk}{close}{suffix}",
                dk = declared_key,
                uk = undeclared_key,
                open = open,
                close = close,
            );
            let declared_pattern = format!("{open}{dk}{close}", dk = declared_key, open = "{{", close = "}}");
            let undeclared_pattern = format!("{open}{uk}{close}", uk = undeclared_key, open = "{{", close = "}}");
            let result = replace_placeholders(&template, &params);
            // 已知 key 的占位符被替换
            prop_assert!(!result.contains(&declared_pattern));
            // 未声明 key 的占位符保留
            let msg = format!("undeclared placeholder {open}{uk}{close} should remain, got: {result}",
                uk = undeclared_key, open = "{{", close = "}}");
            prop_assert!(result.contains(&undeclared_pattern), "{}", msg);
        }

        /// 替换函数必须是幂等的:对同样的输入重复调用,结果相同。
        /// (这条单独成立没有意义,因为每次调用之间结果相同就是恒等,
        /// 但组合 `replace(x, p) == replace(replace(x, p), p)` 是对
        /// "再次扫描替换"类 bug 的强约束。)
        #[test]
        fn replacement_is_idempotent(
            key in key_strategy(),
            value in safe_value_strategy(),
            text in text_strategy(),
        ) {
            let mut params = std::collections::HashMap::new();
            params.insert(key, value);
            let once = replace_placeholders(&text, &params);
            let twice = replace_placeholders(&once, &params);
            prop_assert_eq!(once, twice);
        }

        /// 锁定"value 中含 `{{...}}` 也会被替换"的不变量。
        ///
        /// `replace_placeholders` 在循环中对每个 (k,v) 都执行一次
        /// `result.replace(&placeholder, value)`,因此如果某个 value 本身包含
        /// `{{otherkey}}`,那次替换**是否发生**取决于 HashMap 迭代顺序——这是一个
        /// 隐性 footgun。本测试通过把两个 key 都放进 params 且 value 里嵌入对方的
        /// 占位符,验证：最终结果里 `{{outer}}` 与 `{{inner}}` 都不再出现（无论
        /// 迭代顺序如何,循环会扫两遍）。
        ///
        /// 如果未来重构把循环改成"先把所有 placeholder 收集起来再一次性替换",
        /// 本测试会失败并提示 author 这是行为变更。
        #[test]
        fn value_containing_placeholder_is_fully_replaced(
            outer in "[a-zA-Z_][a-zA-Z0-9_]{0,8}",
            inner in "[a-zA-Z_][a-zA-Z0-9_]{0,8}",
        ) {
            prop_assume!(outer != inner);
            let mut params = std::collections::HashMap::new();
            // outer's value contains {{inner}}; inner's value is plain text.
            params.insert(outer.clone(), format!("prefix-{{{{{}}}}}-suffix", inner));
            params.insert(inner.clone(), "REPLACED".to_string());
            let text = format!("begin {{{{{}}}}}-mid-{{{{{}}}}}-end", outer, inner);
            let result = replace_placeholders(&text, &params);
            // outer 占位符应被替换（其 value 里的 {{inner}} 也会被第二轮吃掉）
            prop_assert!(!result.contains(&format!("{{{{{}}}}}", outer)));
            // inner 占位符应被替换（无论 HashMap 迭代顺序如何,循环两轮都覆盖）
            prop_assert!(!result.contains(&format!("{{{{{}}}}}", inner)));
        }
    }
}
