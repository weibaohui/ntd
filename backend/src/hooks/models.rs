use serde::{Deserialize, Serialize};
use crate::models::TodoStatus;

/// Hook trigger types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookTrigger {
    BeforeCreate,
    AfterCreate,
    BeforeStatusChange,
    AfterStatusChange,
    BeforeDelete,
    AfterDelete,
    BeforeExecute,
}

impl HookTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BeforeCreate => "before_create",
            Self::AfterCreate => "after_create",
            Self::BeforeStatusChange => "before_status_change",
            Self::AfterStatusChange => "after_status_change",
            Self::BeforeDelete => "before_delete",
            Self::AfterDelete => "after_delete",
            Self::BeforeExecute => "before_execute",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "before_create" => Some(Self::BeforeCreate),
            "after_create" => Some(Self::AfterCreate),
            "before_status_change" => Some(Self::BeforeStatusChange),
            "after_status_change" => Some(Self::AfterStatusChange),
            "before_delete" => Some(Self::BeforeDelete),
            "after_delete" => Some(Self::AfterDelete),
            "before_execute" => Some(Self::BeforeExecute),
            _ => None,
        }
    }

    /// Whether this trigger runs synchronously and can block the operation
    pub fn is_sync(&self) -> bool {
        matches!(
            self,
            Self::BeforeCreate | Self::BeforeStatusChange | Self::BeforeDelete | Self::BeforeExecute
        )
    }
}

impl std::fmt::Display for HookTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Filter conditions for a hook rule
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookFilter {
    /// Match if todo status is in this list (empty = match all)
    #[serde(default)]
    pub status: Vec<String>,
    /// Match if todo title contains this string (case-insensitive, empty = match all)
    #[serde(default)]
    pub title_contains: Option<String>,
    /// Match if todo has any of these tag IDs
    #[serde(default)]
    pub tags: Vec<i64>,
    /// Match if todo executor equals this value
    #[serde(default)]
    pub executor: Option<String>,
}

impl HookFilter {
    pub fn matches(&self, title: &str, status: &str, tag_ids: &[i64], executor: Option<&str>) -> bool {
        // Check status filter
        if !self.status.is_empty() && !self.status.contains(&status.to_string()) {
            return false;
        }

        // Check title filter
        if let Some(ref title_filter) = self.title_contains {
            if !title.to_lowercase().contains(&title_filter.to_lowercase()) {
                return false;
            }
        }

        // Check tags filter
        if !self.tags.is_empty() {
            if !self.tags.iter().any(|t| tag_ids.contains(t)) {
                return false;
            }
        }

        // Check executor filter
        if let Some(ref executor_filter) = self.executor {
            if executor != Some(executor_filter.as_str()) {
                return false;
            }
        }

        true
    }
}

/// Hook action to execute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookAction {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    30
}

/// Complete hook rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRule {
    pub id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub trigger: HookTrigger,
    #[serde(default)]
    pub filter: HookFilter,
    pub action: HookAction,
    #[serde(default = "default_async")]
    pub is_async: bool,
}

fn default_async() -> bool {
    true
}

/// Global hook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalHookConfig {
    pub enabled: bool,
    pub default_timeout_secs: u64,
    pub max_concurrency: u64,
}

impl Default for GlobalHookConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_timeout_secs: 30,
            max_concurrency: 5,
        }
    }
}

/// Per-Todo hook mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookMode {
    Inherit,
    Custom,
    Disabled,
}

impl Default for HookMode {
    fn default() -> Self {
        Self::Inherit
    }
}

impl HookMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Custom => "custom",
            Self::Disabled => "disabled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "inherit" => Some(Self::Inherit),
            "custom" => Some(Self::Custom),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

/// Per-Todo hook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoHookConfig {
    pub hook_mode: HookMode,
    pub override_enabled: bool,
}

/// Hook execution context (data passed to hook scripts)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    pub todo_id: Option<i64>,
    pub todo_title: String,
    pub old_status: Option<String>,
    pub new_status: Option<String>,
    pub executor: Option<String>,
    pub workspace: Option<String>,
    pub task_id: Option<String>,
    pub trigger_time: String,
    pub trigger: HookTrigger,
}

impl HookContext {
    pub fn for_create(todo_title: String, executor: Option<String>, workspace: Option<String>) -> Self {
        Self {
            todo_id: None,
            todo_title,
            old_status: None,
            new_status: Some("pending".to_string()),
            executor,
            workspace,
            task_id: None,
            trigger_time: crate::models::utc_timestamp(),
            trigger: HookTrigger::BeforeCreate,
        }
    }

    pub fn for_status_change(
        todo_id: i64,
        todo_title: String,
        old_status: TodoStatus,
        new_status: TodoStatus,
        executor: Option<String>,
        workspace: Option<String>,
    ) -> Self {
        Self {
            todo_id: Some(todo_id),
            todo_title,
            old_status: Some(old_status.to_string()),
            new_status: Some(new_status.to_string()),
            executor,
            workspace,
            task_id: None,
            trigger_time: crate::models::utc_timestamp(),
            trigger: HookTrigger::BeforeStatusChange,
        }
    }

    pub fn for_delete(
        todo_id: i64,
        todo_title: String,
        status: TodoStatus,
        executor: Option<String>,
        workspace: Option<String>,
    ) -> Self {
        Self {
            todo_id: Some(todo_id),
            todo_title,
            old_status: Some(status.to_string()),
            new_status: None,
            executor,
            workspace,
            task_id: None,
            trigger_time: crate::models::utc_timestamp(),
            trigger: HookTrigger::BeforeDelete,
        }
    }

    pub fn for_execute(
        todo_id: i64,
        todo_title: String,
        status: TodoStatus,
        executor: Option<String>,
        workspace: Option<String>,
        task_id: Option<String>,
    ) -> Self {
        Self {
            todo_id: Some(todo_id),
            todo_title,
            old_status: Some(status.to_string()),
            new_status: Some(status.to_string()),
            executor,
            workspace,
            task_id,
            trigger_time: crate::models::utc_timestamp(),
            trigger: HookTrigger::BeforeExecute,
        }
    }
}

/// Result of hook execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: i64,
    pub error_msg: Option<String>,
}

impl HookResult {
    pub fn success(exit_code: i32, stdout: String, stderr: String, duration_ms: i64) -> Self {
        Self {
            success: exit_code == 0,
            exit_code: Some(exit_code),
            stdout,
            stderr,
            duration_ms,
            error_msg: None,
        }
    }

    pub fn error(msg: String, duration_ms: i64) -> Self {
        Self {
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms,
            error_msg: Some(msg),
        }
    }
}

/// Hook execution log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookLogEntry {
    pub id: i64,
    pub hook_id: Option<i64>,
    pub hook_name: Option<String>,
    pub trigger: String,
    pub todo_id: Option<i64>,
    pub args_sent: Option<String>,
    pub env_sent: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: Option<i64>,
    pub success: Option<bool>,
    pub error_msg: Option<String>,
    pub created_at: String,
}

// API types for CRUD operations

#[derive(Debug, Deserialize)]
pub struct CreateHookRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub trigger: String,
    pub filter: Option<HookFilter>,
    pub action: HookAction,
    #[serde(default = "default_async")]
    pub is_async: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct UpdateHookRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub trigger: Option<String>,
    pub filter: Option<HookFilter>,
    pub action: Option<HookAction>,
    pub is_async: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct HookResponse {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub trigger: String,
    pub filter: Option<HookFilter>,
    pub action: HookAction,
    pub is_async: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl From<HookRule> for HookResponse {
    fn from(rule: HookRule) -> Self {
        Self {
            id: rule.id.unwrap_or(0),
            name: rule.name,
            description: rule.description,
            enabled: rule.enabled,
            trigger: rule.trigger.as_str().to_string(),
            filter: Some(rule.filter),
            action: rule.action,
            is_async: rule.is_async,
            created_at: None,
            updated_at: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateGlobalHookConfigRequest {
    pub enabled: Option<bool>,
    pub default_timeout_secs: Option<u64>,
    pub max_concurrency: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct GlobalHookConfigResponse {
    pub enabled: bool,
    pub default_timeout_secs: u64,
    pub max_concurrency: u64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTodoHookRequest {
    pub hook_mode: Option<String>,
    pub override_enabled: Option<bool>,
    pub rule_ids: Option<Vec<i64>>,
}

#[derive(Debug, Serialize)]
pub struct TodoHookConfigResponse {
    pub todo_id: i64,
    pub hook_mode: String,
    pub override_enabled: bool,
    pub rule_ids: Vec<i64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HookLogQuery {
    #[serde(default)]
    pub hook_id: Option<i64>,
    #[serde(default)]
    pub todo_id: Option<i64>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize)]
pub struct HookLogPage {
    pub logs: Vec<HookLogEntry>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}
