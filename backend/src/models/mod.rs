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
    pub scheduler_next_run_at: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub worktree_enabled: bool,
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
    pub workspace: Option<String>,
    #[serde(default)]
    pub worktree_enabled: Option<bool>,
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
}

#[derive(Deserialize)]
pub struct SmartCreateRequest {
    pub content: String,
}

#[derive(Deserialize)]
pub struct TodoIdQuery {
    pub todo_id: i64,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub status: Option<String>,
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

#[derive(Deserialize)]
pub struct UpdateSchedulerRequest {
    pub scheduler_enabled: bool,
    pub scheduler_config: Option<String>,
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

#[derive(Serialize)]
pub struct ExecutorTestResult {
    pub test_passed: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

// Executor types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ExecutorType {
    Joinai,
    #[default]
    Claudecode,
    Codebuddy,
    Opencode,
    Atomcode,
    Hermes,
    Kimi,
    Codex,
}


impl ExecutorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutorType::Joinai => "joinai",
            ExecutorType::Claudecode => "claudecode",
            ExecutorType::Codebuddy => "codebuddy",
            ExecutorType::Opencode => "opencode",
            ExecutorType::Atomcode => "atomcode",
            ExecutorType::Hermes => "hermes",
            ExecutorType::Kimi => "kimi",
            ExecutorType::Codex => "codex",
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
        assert_eq!(ExecutorType::Joinai.as_str(), "joinai");
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
}
