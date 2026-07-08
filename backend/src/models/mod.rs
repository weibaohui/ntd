use serde::{Deserialize, Serialize};

pub mod loop_;
pub use loop_::*;

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
    /// 工作空间目录路径（cwd，仅后端内部使用，不通过 API 暴露给前端）。
    /// 业务层（前端 / CLI / sync）只通过 `workspace_id` 标识工作空间，
    /// path 字段保留供 executor_service / worktree 等需要 cwd 的子系统使用。
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// 所属工作空间 ID（project_directories.id），唯一键。
    /// 业务层（前端 / CLI / API）统一以此作为工作空间标识符。
    #[serde(default)]
    pub workspace_id: Option<i64>,
    #[serde(default)]
    pub webhook_enabled: bool,
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    /// 0=normal, 1=reviewer_template（已废弃：评审模板已迁出至 review_templates 表）,
    /// 2=review_instance（评审实例）.
    #[serde(default)]
    pub todo_type: i32,
    /// review_instance 关联到被评审的原 todo; 其它类型为 None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_todo_id: Option<i64>,
    /// review_instance 关联到生成它的 review_template; 其它类型为 None.
    /// NULL/NONE 可能是 V15 之前的迁移产物.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_template_id: Option<i64>,
    /// 是否在执行完成后自动派生一个评审 todo. 只对 normal 类型有意义.
    #[serde(default = "default_true")]
    pub auto_review_enabled: bool,
    /// Action 类型标记（如 "title_optimize"、"prompt_optimize"）。
    /// 与 action_key 配合，由 /api/actions/execute 用于查找或自动创建 action 模板 todo。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_type: Option<String>,
    /// Action 键值，与 action_type 配合唯一标识一个 action 模板 todo。
    /// 由 /api/actions/execute 用于查找或自动创建 action 模板 todo。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_key: Option<String>,
    /// 归档时间戳（UTC 字符串）。None=未归档，参与事项中心日常分类；
    /// Some=已归档，进入「已归档」分类，从日常视图隐藏但数据保留。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
}

/// 事项中心的五类驱动分类（computed_bucket）。
///
/// 这是运行时由底层事实字段推导的返回值，不落库。
/// 推导规则见 `compute_bucket`，优先级：已归档 > Loop 驱动 > 时间驱动 > 事件驱动 > 手动触发。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputedBucket {
    /// 手动触发：兜底分类，未归档且无调度/无 Webhook/未被 Loop 引用。
    Manual,
    /// 时间驱动：scheduler_config 非空（scheduler_enabled 仅表启停）。
    TimeDriven,
    /// 事件驱动：webhook_enabled=true 且无调度配置且未被 Loop 引用。
    EventDriven,
    /// Loop 驱动：被启用的 loop_steps 引用（used_by_loop_step_count > 0）。
    LoopDriven,
    /// 已归档：archived_at 非空，优先级最高。
    Archived,
}

impl ComputedBucket {
    /// 从查询参数解析分类。None/空字符串=不过滤（返回全部）。
    /// 不区分大小写，与 serde 的 snake_case 序列化对齐。
    pub fn parse_query(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "manual" => Some(Self::Manual),
            "time_driven" => Some(Self::TimeDriven),
            "event_driven" => Some(Self::EventDriven),
            "loop_driven" => Some(Self::LoopDriven),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// 由底层事实字段推导事项的主分类（computed_bucket）。
///
/// 优先级（设计文档）：已归档 > Loop 驱动 > 时间驱动 > 事件驱动 > 手动触发。
/// 纯函数、无 IO，便于单测覆盖各组合分支。
///
/// - `archived_at` 非空 → 已归档（最高优先级，用户明确希望隐藏）
/// - 否则被 Loop 引用（count>0）→ Loop 驱动（已成流程结构一部分）
/// - 否则 scheduler_config 非空 → 时间驱动（注意：scheduler_enabled 仅表启停，不决定是否时间驱动）
/// - 否则 webhook_enabled → 事件驱动
/// - 否则 → 手动触发（兜底）
pub fn compute_bucket(
    archived_at: Option<&str>,
    used_by_loop_step_count: i64,
    scheduler_config: Option<&str>,
    webhook_enabled: bool,
) -> ComputedBucket {
    if archived_at.is_some() {
        return ComputedBucket::Archived;
    }
    if used_by_loop_step_count > 0 {
        return ComputedBucket::LoopDriven;
    }
    if scheduler_config.is_some() {
        return ComputedBucket::TimeDriven;
    }
    if webhook_enabled {
        return ComputedBucket::EventDriven;
    }
    ComputedBucket::Manual
}

/// 事项中心列表项：在 Todo 之上附加运行时推导/聚合字段。
///
/// 附加字段由 handler 层批量补算（loop 引用计数、最近一次执行记录），
/// 普通 `get_todos` 路径不返回这些字段，因此独立成 DTO 而非塞进 Todo。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoCenterItem {
    /// 内联 Todo 全部字段，保持响应扁平（设计文档示例即扁平结构）。
    #[serde(flatten)]
    pub todo: Todo,
    /// 运行时推导的主分类，不落库。
    pub computed_bucket: ComputedBucket,
    /// 被启用 loop_steps 引用的次数（COUNT ... WHERE enabled=1 GROUP BY todo_id）。
    /// 0=未被任何启用的 Loop 引用。
    #[serde(default)]
    pub used_by_loop_step_count: i64,
    /// 最近一次执行记录的状态（success/failed/running/...），无记录则 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_execution_status: Option<String>,
    /// 最近一次执行记录的时间（优先 finished_at，回退 started_at），无记录则 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_execution_at: Option<String>,
    /// 引用该事项的启用 Loop 摘要（loop_id + name）。仅 Loop 驱动分类非空，
    /// 供卡片展示「所属 Loop」并跳转 Loop 详情。空 vec=未被引用或非 Loop 驱动。
    #[serde(default)]
    pub referencing_loops: Vec<LoopRefSummary>,
}

/// Loop 引用摘要：事项中心 Loop 驱动卡片展示「所属 Loop」用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopRefSummary {
    pub loop_id: i64,
    pub loop_name: String,
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
    /// Bot 所属的工作空间 ID
    pub workspace_id: i64,
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
    /// issue #643: 本次执行使用的 git worktree 目录。None = 未启用 worktree 或未创建成功。
    /// 字段语义：仅供"事后排查"，不影响子进程 cwd；auto_cleanup 决定它在执行后是否被删。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    /// 当本次执行是 loop 环节的一部分时，指向 loop_step_executions 表的 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_step_execution_id: Option<i64>,
    /// 已废弃，曾用于环节独立执行。现始终为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<i64>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedLogEntry {
    pub timestamp: String,
    #[serde(rename = "type")]
    pub log_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ExecutionUsage>,
    #[serde(rename = "toolName", alias = "tool_name", skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(rename = "toolInputJson", alias = "tool_input_json", skip_serializing_if = "Option::is_none")]
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
    pub acceptance_criteria: Option<String>,
    #[serde(default)]
    pub webhook_enabled: Option<bool>,
    #[serde(default)]
    pub auto_review_enabled: Option<bool>,
    /// 工作空间 ID（project_directories.id），唯一键。
    /// 创建时必填；handler 据此查 path 写入 DB cwd 字段。
    pub workspace_id: i64,
    /// Action 类型标记（如 "rewrite_title"、"optimize_prompt"），
    /// 仅供前端 ActionButton 组件做 UI 分类展示，不影响执行逻辑。
    #[serde(default)]
    pub action_type: Option<String>,
    /// Action 键值，与 action_type 配合唯一标识一个 action 模板 todo。
    #[serde(default)]
    pub action_key: Option<String>,
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
    /// 工作空间 ID（project_directories.id）。
    /// None=保持当前工作空间，Some(id)=迁移到该工作空间。
    /// 不接受路径——handler 一律按 id 解析 cwd 路径写入两列。
    #[serde(default)]
    pub workspace_id: Option<i64>,
    #[serde(default)]
    pub webhook_enabled: Option<bool>,
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    /// None=不变, Some(true)/Some(false)=更新. 不允许改 reviewer template 的开关.
    #[serde(default)]
    pub auto_review_enabled: Option<bool>,
    /// Action 类型标记（如 "rewrite_title"、"optimize_prompt"），
    /// 仅供前端 ActionButton 组件做 UI 分类展示，不影响执行逻辑。
    #[serde(default)]
    pub action_type: Option<String>,
    /// Action 键值，与 action_type 配合唯一标识一个 action 模板 todo。
    #[serde(default)]
    pub action_key: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct UpdateTagsRequest {
    pub tag_ids: Vec<i64>,
}

/// `PUT /api/todos/{id}/webhook` 请求体：开启/关闭事件驱动。
///
/// 扁平具名路由（设计文档），与 `PUT /api/todos/{id}/scheduler` 对称，
/// 让前端有一个明确的「事件驱动启停」入口，而非塞进通用 update_todo。
#[derive(Deserialize, Serialize)]
pub struct UpdateWebhookRequest {
    pub webhook_enabled: bool,
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
    /// 工作空间 ID（用于查询该工作空间的默认响应 Todo）
    pub workspace_id: i64,
}

#[derive(Deserialize)]
pub struct TodoIdQuery {
    #[serde(default)]
    pub todo_id: Option<i64>,
    #[serde(default)]
    pub step_id: Option<i64>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub status: Option<String>,
    /// 按工作空间 ID 过滤；不传则不过滤。当提供 todo_id 或 step_id 时忽略此字段。
    #[serde(default)]
    pub workspace_id: Option<i64>,
    /// 按最近 N 小时过滤（对 finished_at 生效）；不传或 0 表示不过滤。
    #[serde(default)]
    pub hours: Option<u32>,
}

/// 批量更新事项执行器请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchUpdateTodoExecutorRequest {
    pub ids: Vec<i64>,
    pub executor: String,
}

/// 批量更新事项执行器返回结果。
#[derive(Debug, Clone, Serialize)]
pub struct BatchUpdateTodoResult {
    pub updated_count: i64,
    pub total: i64,
}

/// 批量更新事项工作空间请求体（移动到其他工作空间）。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchUpdateTodoWorkspaceRequest {
    pub ids: Vec<i64>,
    /// 目标工作空间 ID（project_directories.id）。
    pub workspace_id: i64,
}

/// 批量复制事项到其他工作空间请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCopyTodoWorkspaceRequest {
    pub ids: Vec<i64>,
    /// 目标工作空间 ID（project_directories.id）。
    pub workspace_id: i64,
}

/// 批量暂停/恢复周期执行请求体。scheduler_enabled 为 true 表示恢复，false 表示暂停。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchUpdateTodoSchedulerRequest {
    pub ids: Vec<i64>,
    pub scheduler_enabled: bool,
}

/// 批量更新环路工作空间请求体（移动到其他工作空间）。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchUpdateLoopWorkspaceRequest {
    pub ids: Vec<i64>,
    /// 目标工作空间 ID（project_directories.id）。
    pub workspace_id: i64,
}

/// 批量复制环路到其他工作空间请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCopyLoopWorkspaceRequest {
    pub ids: Vec<i64>,
    /// 目标工作空间 ID（project_directories.id）。
    pub workspace_id: i64,
}

/// 批量 workspace 操作返回结果（通用）。
#[derive(Debug, Clone, Serialize)]
pub struct BatchWorkspaceResult {
    pub updated_count: i64,
    pub total: i64,
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
    pub history_message_max_age_secs: Option<u64>,
    pub max_concurrent_todos: Option<u32>,
    pub execution_timeout_secs: Option<u64>,
    pub scheduler_default_timezone: Option<String>,
    /// WebSocket broadcast channel 容量。修改后需要重启服务才会在新连接上生效。
    pub broadcast_channel_capacity: Option<usize>,
    /// 是否开启自动版本更新检查
    pub auto_update_enabled: Option<bool>,
    /// 自动更新检查间隔类型："day" / "week" / "month"
    pub auto_update_interval: Option<String>,
    /// 自动更新检查小时（0-23）
    pub auto_update_hour: Option<u32>,
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
    Zhanlu,
    Kilo,
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
            ExecutorType::Zhanlu => "zhanlu",
            ExecutorType::Kilo => "kilo",
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

// ============================================================================
// 评审模板 (review_templates) 模型
// ============================================================================
// 历史背景：评审模板曾以 todos.todo_type=1 (标题"评审任务") 兼任。V15 迁移
// 把这部分数据搬到独立的 review_templates 表。与 todo_templates (可导入的
// todo 模板库) 是不同概念，**不要混用**。

/// 评审模板完整模型（含 prompt，用于评审时拉取原文）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewTemplate {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub prompt: String,
    /// 所属工作空间 ID（project_directories.id）。null = 全局模板。
    pub workspace_id: Option<i64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// 评审模板轻量选项（不含 prompt），用于 loop 编辑器下拉选择。
/// 不返回 prompt 字段的原因：
/// 1. 下拉列表不需要 prompt 内容，省字节
/// 2. 防止前端误把 prompt 文本渲染到 UI（prompt 可能含占位符代码）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewTemplateOption {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    /// 所属工作空间 ID（project_directories.id）。null = 全局模板。
    pub workspace_id: Option<i64>,
}

/// 评审模板创建请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReviewTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub prompt: String,
    /// 所属工作空间 ID（project_directories.id）。null = 全局模板。
    #[serde(default)]
    pub workspace_id: Option<i64>,
}

/// 评审模板更新请求（name/prompt 必传，description 可选）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateReviewTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub prompt: String,
}

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
    pub workspace_path: Option<String>,
    pub worktree: Option<String>,
    pub action_type: Option<String>,
    pub action_key: Option<String>,
}

// ============ 环路导入导出 DTO ============
// 方案：伪ID引用（@loop_1, @todo_1 等）解决跨实体引用问题

/// 导出文件顶层结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopExportData {
    pub version: String,
    #[serde(rename = "type")]
    pub export_type: String,
    pub created_at: String,
    pub source: String,
    pub schema_version: i32,
    pub tags: Vec<TagExportItem>,
    pub review_templates: Vec<ReviewTemplateExportItem>,
    pub todos: Vec<TodoExportItem>,
    pub loops: Vec<LoopExportItem>,
}

/// 标签导出项（伪ID格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagExportItem {
    pub id: String,    // 伪ID: "@tag_1"
    pub name: String,
    pub color: String,
}

/// 评审模板导出项（伪ID格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewTemplateExportItem {
    pub id: String,               // 伪ID: "@template_1"
    pub name: String,
    pub description: Option<String>,
    pub prompt: String,
}

/// Todo 导出项（伪ID格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoExportItem {
    pub id: String,               // 伪ID: "@todo_1"
    pub title: String,
    pub prompt: String,
    pub status: String,
    pub executor: Option<String>,
    pub scheduler_enabled: bool,
    pub webhook_enabled: bool,
    pub acceptance_criteria: Option<String>,
    pub auto_review_enabled: bool,
    pub review_template_id: Option<String>,    // 伪ID引用
    pub review_template_name: Option<String>,  // 展示用
    pub kind: String,
    pub tag_ids: Vec<String>,                   // 伪ID引用
    pub tag_names: Vec<String>,                // 展示用
    /// 是否为异常处理 Todo（异常处理 Todo 不导出标签）
    #[serde(default)]
    pub is_abnormal_handler: bool,
    /// Action 类型标记（如 "rewrite_title"、"optimize_prompt"），
    /// 仅供前端 ActionButton 组件做 UI 分类展示，不影响执行逻辑。
    #[serde(default)]
    pub action_type: Option<String>,
    /// Action 键值，与 action_type 配合唯一标识一个 action 模板 todo。
    #[serde(default)]
    pub action_key: Option<String>,
}

/// 环路导出项（伪ID格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopExportItem {
    pub id: String,               // 伪ID: "@loop_1"
    pub name: String,
    pub description: String,
    pub icon: String,
    pub color: String,
    pub status: String,
    pub webhook_enabled: bool,
    pub limits_config: serde_json::Value,
    pub review_template_id: Option<String>,          // 伪ID引用
    pub review_template_name: Option<String>,        // 展示用
    pub abnormal_handler_todo_id: Option<String>,   // 伪ID引用
    pub abnormal_handler_todo_title: Option<String>, // 展示用
    pub abnormal_handler_trigger_on: Vec<String>,
    pub tag_ids: Vec<String>,                        // 伪ID引用
    pub tag_names: Vec<String>,                      // 展示用
    pub triggers: Vec<LoopTriggerExportItem>,
    pub steps: Vec<LoopStepExportItem>,
}

/// 触发器导出项（只包含 manual 和 cron）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopTriggerExportItem {
    pub id: String,           // 伪ID: "@trigger_1"
    pub trigger_type: String, // "manual" 或 "cron"
    pub config: serde_json::Value,
    pub enabled: bool,
    pub priority: i32,
}

/// 步骤导出项（伪ID格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStepExportItem {
    pub id: String,               // 伪ID: "@step_1"
    pub name: String,
    pub description: String,
    pub todo_id: String,          // 伪ID引用
    pub todo_title: String,       // 展示用
    pub order_index: i32,
    pub run_mode: String,
    pub skip_on_source_failed: bool,
    pub min_rating: Option<i32>,
    pub unrated_policy: String,
    pub on_success: String,
    pub success_goto_step_id: Option<String>,   // 伪ID引用
    pub success_goto_step_name: Option<String>, // 展示用
    pub on_rating_fail: String,
    pub fail_goto_step_id: Option<String>,      // 伪ID引用
    pub fail_goto_step_name: Option<String>,    // 展示用
    pub review_type: String,
    pub enabled: bool,
}

// ============ 导入预览/执行响应 DTO ============

/// 导入预览响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopImportPreviewResponse {
    pub valid: bool,
    pub pseudo_ids: Vec<String>,
    pub summary: LoopImportSummary,
    pub conflicts: Vec<LoopImportConflict>,
    pub warnings: Vec<LoopImportWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopImportSummary {
    pub loops: usize,
    pub steps: usize,
    pub todos: usize,
    pub review_templates: usize,
    pub tags: usize,
    pub triggers: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopImportConflict {
    #[serde(rename = "type")]
    pub conflict_type: String,
    pub name: String,
    pub action: String, // "rename" | "overwrite" | "skip"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopImportWarning {
    #[serde(rename = "type")]
    pub warning_type: String,
    pub message: String,
}

/// 导入执行响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopImportResponse {
    pub success: bool,
    pub created: LoopImportCreatedCounts,
    pub warnings: Vec<LoopImportWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopImportCreatedCounts {
    pub loops: usize,
    pub todos: usize,
    pub review_templates: usize,
    pub tags: usize,
    pub triggers: usize,
    pub steps: usize,
}

/// 导出选择请求
#[derive(Debug, Clone, Deserialize)]
pub struct ExportLoopSelectedRequest {
    pub loop_ids: Vec<i64>,
}

// Business error codes
pub mod codes {
    pub const NOT_FOUND: i32 = 40001;
    pub const BAD_REQUEST: i32 = 40002;
    pub const INTERNAL: i32 = 50001;
}

// ============ 伪ID工具函数 ============

/// 伪ID类型前缀
const PSEUDO_ID_PREFIXES: &[&str] = &["loop", "todo", "step", "trigger", "template", "tag"];

/// 生成伪ID: "@{prefix}_{index}"
pub fn generate_pseudo_id(prefix: &str, index: usize) -> String {
    format!("@{}_{}", prefix, index)
}

/// 校验伪ID格式是否合法
/// 格式: ^@(loop|todo|step|trigger|template|tag)_\d+$
pub fn validate_pseudo_id(id: &str) -> bool {
    // 必须以 @ 开头
    if !id.starts_with('@') {
        return false;
    }
    // 去掉 @ 前缀后检查格式
    let rest = &id[1..];
    // 检查是否包含 _ 和数字部分
    if let Some(underscore_pos) = rest.find('_') {
        let prefix = &rest[..underscore_pos];
        let suffix = &rest[underscore_pos + 1..];
        return PSEUDO_ID_PREFIXES.contains(&prefix) && suffix.parse::<usize>().is_ok();
    }
    false
}

/// 从伪ID提取前缀
pub fn extract_pseudo_prefix(id: &str) -> Option<&str> {
    if !id.starts_with('@') {
        return None;
    }
    let rest = &id[1..];
    // 没有下划线则不是合法伪ID格式
    let underscore_pos = rest.find('_')?;
    Some(&rest[..underscore_pos])
}

/// 从伪ID提取序号
pub fn extract_pseudo_index(id: &str) -> Option<usize> {
    if !id.starts_with('@') {
        return None;
    }
    let rest = &id[1..];
    rest.split('_').nth(1)?.parse().ok()
}

/// 返回当前 UTC 时间的 ISO 8601 格式字符串 (2024-01-15T08:30:00.000Z)
pub fn utc_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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

    // -------- computed_bucket 推导 --------

    /// 手动触发：兜底分类，无任何驱动事实。
    #[test]
    fn test_compute_bucket_manual_when_no_facts() {
        assert_eq!(
            compute_bucket(None, 0, None, false),
            ComputedBucket::Manual
        );
    }

    /// 已归档优先级最高：即便同时有调度/事件/Loop 引用，也归已归档。
    /// 原因：归档代表用户明确希望日常隐藏，盖过一切驱动能力。
    #[test]
    fn test_compute_bucket_archived_wins_over_all() {
        assert_eq!(
            compute_bucket(Some("2026-07-08T10:00:00Z"), 3, Some("0 9 * * * *"), true),
            ComputedBucket::Archived
        );
    }

    /// Loop 驱动优先于时间/事件驱动：被启用 loop_steps 引用即视为流程结构一部分。
    #[test]
    fn test_compute_bucket_loop_driven_beats_time_and_event() {
        assert_eq!(
            compute_bucket(None, 1, Some("0 9 * * * *"), true),
            ComputedBucket::LoopDriven
        );
    }

    /// 时间驱动：scheduler_config 非空（scheduler_enabled 不参与判断，仅表启停）。
    #[test]
    fn test_compute_bucket_time_driven_when_scheduler_config_present() {
        assert_eq!(
            compute_bucket(None, 0, Some("0 9 * * * *"), false),
            ComputedBucket::TimeDriven
        );
    }

    /// 时间驱动优先于事件驱动：同时有调度与 Webhook 时归时间驱动。
    #[test]
    fn test_compute_bucket_time_driven_beats_event() {
        assert_eq!(
            compute_bucket(None, 0, Some("0 9 * * * *"), true),
            ComputedBucket::TimeDriven
        );
    }

    /// 事件驱动：无调度、未被 Loop 引用、且 webhook_enabled。
    #[test]
    fn test_compute_bucket_event_driven_when_webhook_only() {
        assert_eq!(
            compute_bucket(None, 0, None, true),
            ComputedBucket::EventDriven
        );
    }

    /// parse_query：合法串（含大小写/下划线）正确解析，非法串返回 None。
    #[test]
    fn test_computed_bucket_parse_query() {
        assert_eq!(ComputedBucket::parse_query("manual"), Some(ComputedBucket::Manual));
        assert_eq!(ComputedBucket::parse_query("Time_Driven"), Some(ComputedBucket::TimeDriven));
        assert_eq!(ComputedBucket::parse_query(" loop_driven "), Some(ComputedBucket::LoopDriven));
        assert_eq!(ComputedBucket::parse_query("archived"), Some(ComputedBucket::Archived));
        assert_eq!(ComputedBucket::parse_query(""), None);
        assert_eq!(ComputedBucket::parse_query("bogus"), None);
    }

    /// serde 序列化为 snake_case，与 parse_query 对齐（前端按此串回传）。
    #[test]
    fn test_computed_bucket_serializes_snake_case() {
        let json = serde_json::to_string(&ComputedBucket::LoopDriven).unwrap();
        assert_eq!(json, "\"loop_driven\"");
        assert_eq!(
            serde_json::to_string(&ComputedBucket::TimeDriven).unwrap(),
            "\"time_driven\""
        );
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
    fn test_executor_type_kilo_as_str() {
        assert_eq!(ExecutorType::Kilo.as_str(), "kilo");
    }

    #[test]
    fn test_executor_type_kilo_display() {
        assert_eq!(format!("{}", ExecutorType::Kilo), "kilo");
    }

    #[test]
    fn test_executor_type_kilo_is_distinct_from_others() {
        // Kilo must not accidentally compare equal to any other variant
        assert_ne!(ExecutorType::Kilo, ExecutorType::Opencode);
        assert_ne!(ExecutorType::Kilo, ExecutorType::Zhanlu);
        assert_ne!(ExecutorType::Kilo, ExecutorType::Claudecode);
    }

    #[test]
    fn test_executor_type_kilo_clone() {
        let et = ExecutorType::Kilo;
        let cloned = et.clone();
        assert_eq!(cloned, ExecutorType::Kilo);
        assert_eq!(cloned.as_str(), "kilo");
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

    /// 锁住 `ParsedLogEntry` 的 wire 格式契约：序列化输出 camelCase（`toolName` /
    /// `toolInputJson`），None 字段被省略（`skip_serializing_if`），并通过 `alias`
    /// 兼容反序列化 snake_case 旧数据。
    /// 对应 PR #656 评审 MEDIUM #1：单元测试覆盖序列化产物与反序列化兼容性。
    #[test]
    fn test_parsed_log_entry_serde_uses_camel_case() {
        // 准备：直接构造全量字段的 entry，绕开 builder 缺失（目前无 with_tool_name）
        let mut entry = ParsedLogEntry::info("hi");
        entry.tool_name = Some("Bash".to_string());
        entry.tool_input_json = Some("{\"cmd\":\"ls\"}".to_string());

        // 序列化：wire 必须是 camelCase
        let value = serde_json::to_value(&entry).expect("serialize");
        assert_eq!(value["toolName"], "Bash", "wire 格式必须为 toolName");
        assert_eq!(
            value["toolInputJson"], "{\"cmd\":\"ls\"}",
            "wire 格式必须为 toolInputJson"
        );
        // 旧 snake_case 字段不应出现在序列化产物中（rename 而非 alias 输出）
        assert!(
            value.get("tool_name").is_none(),
            "序列化不应输出 snake_case tool_name"
        );
        assert!(
            value.get("tool_input_json").is_none(),
            "序列化不应输出 snake_case tool_input_json"
        );

        // None 时字段被省略（skip_serializing_if）
        let bare = ParsedLogEntry::info("hi");
        let bare_value = serde_json::to_value(&bare).expect("serialize bare");
        assert!(bare_value.get("toolName").is_none());
        assert!(bare_value.get("toolInputJson").is_none());
        assert!(bare_value.get("usage").is_none());

        // alias 兼容：DB 历史数据若仍写 snake_case，反序列化能正确读出
        let legacy = serde_json::json!({
            "timestamp": "2026-01-01T00:00:00.000Z",
            "type": "info",
            "content": "legacy",
            "tool_name": "Read",
            "tool_input_json": "{\"path\":\"/tmp/x\"}"
        });
        let de: ParsedLogEntry = serde_json::from_value(legacy).expect("alias deserialize");
        assert_eq!(de.tool_name.as_deref(), Some("Read"));
        assert_eq!(de.tool_input_json.as_deref(), Some("{\"path\":\"/tmp/x\"}"));
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
        let json = r#"{"title":"Test","prompt":"Do this","tag_ids":[1,2],"workspace_id":42}"#;
        let req: CreateTodoRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, "Test");
        assert_eq!(req.prompt, "Do this");
        assert_eq!(req.tag_ids, vec![1, 2]);
        assert_eq!(req.workspace_id, 42);
    }

    #[test]
    fn test_create_todo_request_default_tag_ids() {
        // workspace_id 必填：缺失则反序列化失败，这是 API 契约的一部分。
        let json = r#"{"title":"Test","prompt":"Do this","workspace_id":1}"#;
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

    // ============ 伪ID工具函数测试 ============

    #[test]
    fn test_generate_pseudo_id() {
        assert_eq!(generate_pseudo_id("loop", 1), "@loop_1");
        assert_eq!(generate_pseudo_id("todo", 42), "@todo_42");
        assert_eq!(generate_pseudo_id("step", 100), "@step_100");
    }

    #[test]
    fn test_validate_pseudo_id_valid() {
        assert!(validate_pseudo_id("@loop_1"));
        assert!(validate_pseudo_id("@todo_42"));
        assert!(validate_pseudo_id("@step_100"));
        assert!(validate_pseudo_id("@trigger_5"));
        assert!(validate_pseudo_id("@template_3"));
        assert!(validate_pseudo_id("@tag_99"));
    }

    #[test]
    fn test_validate_pseudo_id_invalid() {
        // 不是以 @ 开头
        assert!(!validate_pseudo_id("loop_1"));
        assert!(!validate_pseudo_id(""));
        // 没有下划线
        assert!(!validate_pseudo_id("@loop"));
        assert!(!validate_pseudo_id("@todoabc"));
        // 前缀不合法
        assert!(!validate_pseudo_id("@invalid_1"));
        assert!(!validate_pseudo_id("@_1"));
        // 数字部分不合法
        assert!(!validate_pseudo_id("@loop_abc"));
        assert!(!validate_pseudo_id("@loop_-1"));
    }

    #[test]
    fn test_extract_pseudo_prefix() {
        assert_eq!(extract_pseudo_prefix("@loop_1"), Some("loop"));
        assert_eq!(extract_pseudo_prefix("@todo_42"), Some("todo"));
        assert_eq!(extract_pseudo_prefix("@step_100"), Some("step"));
        assert_eq!(extract_pseudo_prefix("loop_1"), None);  // 没有 @ 前缀
        assert_eq!(extract_pseudo_prefix("@"), None);       // @ 后无下划线，不是合法伪ID
    }

    #[test]
    fn test_extract_pseudo_index() {
        assert_eq!(extract_pseudo_index("@loop_1"), Some(1));
        assert_eq!(extract_pseudo_index("@todo_42"), Some(42));
        assert_eq!(extract_pseudo_index("@step_100"), Some(100));
        assert_eq!(extract_pseudo_index("loop_1"), None);
        assert_eq!(extract_pseudo_index("@loop"), None);
        assert_eq!(extract_pseudo_index("@loop_abc"), None);
    }
}

/// Replace placeholders in a string using a map of key-value pairs.
/// Format: `{{key}}` will be replaced with the corresponding value from the map.
/// If a key is not found in the map, it remains unchanged.
///
/// **Footgun — value 中含占位符**: 如果某个 `value` 本身包含 `{{otherkey}}` 而
/// `otherkey` 也在 `params` 里,**只有当 `otherkey` 先于当前 (k,v) 被替换时**,value
/// 中的 `{{otherkey}}` 才会被吃掉。`HashMap` 的迭代顺序是 `RandomState` 加盐的随机化,
/// 因此这种行为不可预测。**调用方请避免在 value 中嵌入另一个 key 的占位符**,或
/// 自行预处理 value(把嵌入的占位符先替换为最终文本)。
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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

        /// 锁定"value 中含 `{{...}}` 也会被替换"的不变量 —— **因 HashMap 迭代顺序
        /// 不可预测,本测试无法直接验证**。`replace_placeholders` 的单遍循环行为
        /// 取决于哪个 key 先被处理;value 中的 `{{otherkey}}` 是否被替换是
        /// 顺序依赖的(proptest 用 `HashMap` 也只能覆盖部分 case)。
        ///
        /// 该 footgun 已在 `replace_placeholders` 的 doc 注释里以 **Footgun** 段标注,
        /// 建议调用方避免在 value 中嵌入另一个 key 的占位符。本 mod 不写 proptest
        /// 覆盖,改由 README/AGENTS.md 的使用规范承担。
        ///
        /// 此处保留 `value_containing_placeholder_outer_placeholder_always_gone` 单测:
        /// 只覆盖"value 含占位符但 outer 的占位符被替换后,**不再**被替换"这条
        /// 一定成立的弱不变量(无论 HashMap 顺序如何,outer 的 key 一定会被一次
        /// `result.replace`,且 outer 的 value 里的 `{{inner}}` 在 outer 那次
        /// 替换**之前**还没有机会被替换)。
        #[test]
        fn value_containing_placeholder_outer_placeholder_always_gone(
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
            // outer 自己的占位符一定被替换(HashMap 迭代一定会扫到 outer 这一行)。
            let outer_pat = format!("{{{{{}}}}}", outer);
            prop_assert!(
                !result.contains(&outer_pat),
                "outer placeholder {{{{outer}}}} must always be replaced, got: {}",
                result,
            );
        }
    }
}
