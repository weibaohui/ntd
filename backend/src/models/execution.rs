//! Execution 域数据模型（096-W4-2：从 models/mod.rs 按域拆分，逐字搬迁零改动）。
//!
//! 含：ExecutionStatus / ExecutionRecord / Usage / ParsedLogEntry / 执行请求 DTO /
//! Dashboard 统计族（基于 execution_records 的派生统计）/ UTC 时间戳工具。
//! 经 `models::mod` 的 `pub use execution::*` 聚合，外部引用路径不变。

use serde::{Deserialize, Serialize};

// 跨域引用（经 models 聚合根可达）：运行看板关联的 Todo 类型
use crate::models::Todo;

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

/// `execution_records.last_review_status` 取值族（5 态）。
/// 自动评审对原执行记录的复核结果；`Interrupted` 表示执行非成功非失败（如超时、取消），
/// 不能默认归为 Failed——否则会把「未完成」误标成「评审不通过」。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Success,
    Failed,
    Interrupted,
    Skipped,
}

impl ReviewStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Skipped => "skipped",
        }
    }

    /// DB 读取侧解析：未知值回退 None（调用方按场景兜底），不写死默认态。
    /// 与 loop_ 域统一用 from_db（Option），不沿用本文件 D2 的 FromStr 历史范式。
    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => Self::Pending,
            "success" => Self::Success,
            "failed" => Self::Failed,
            "interrupted" => Self::Interrupted,
            "skipped" => Self::Skipped,
            _ => return None,
        })
    }
}

impl std::fmt::Display for ReviewStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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
    #[serde(default = "crate::models::default_trigger_type")]
    pub trigger_type: String,
    #[serde(default)]
    pub pid: Option<i32>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_progress: Option<String>,
    /// 多 Agent 协作的子 agent 元数据（JSON 字符串 `Vec<AgentRun>`）。
    /// 透传 entity 的 agent_runs 列，前端自行 parse。与 todo_progress 一样不在此处反序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_runs: Option<String>,
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
    /// record 直接归属的 workspace（v89 新增）。归属校验用它，不再经 todo 间接关联。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<i64>,
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

#[derive(Deserialize, Serialize)]
pub struct ExecuteRequest {
    pub todo_id: i64,
    pub message: Option<String>,
    pub executor: Option<String>,
    /// 手动执行时临时指定模型（优先级最高，覆盖 todo.model / executor.default_model）。
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub params: Option<std::collections::HashMap<String, String>>,
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

/// 返回当前 UTC 时间的 ISO 8601 格式字符串 (2024-01-15T08:30:00.000Z)
pub fn utc_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// 093：返回「hours 小时前」的 UTC cutoff，格式与 `utc_timestamp` 完全一致。
///
/// 用途：列表 hours 过滤从 `REPLACE(REPLACE(updated_at,...)>= datetime('now',...)`
/// （列上套函数使索引失效）改为 `updated_at >= ?` 参数绑定裸列比较。
/// 存储侧格式考据（生产统一 T/Z ISO，见 093 设计文档 §1.3）：
/// 应用层写入走 `utc_timestamp()`（毫秒精度），触发器兜底走 `%Y-%m-%dT%H:%M:%SZ`
/// （秒级精度）；两种形态与本 cutoff 做字符串比较在秒级等价于时间比较，
/// 毫秒边界误差 <1s，对 hours 级过滤可忽略。
pub fn utc_timestamp_minus_hours(hours: u32) -> String {
    (chrono::Utc::now() - chrono::Duration::hours(i64::from(hours)))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

// 测试模块允许 unwrap/expect/panic：单测里 panic 即断言失败，语义正当；
// clippy::unwrap_used 默认对 #[cfg(test)] 不豁免，故显式 allow（同 loop_.rs 范式）。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// ReviewStatus 枚举族契约（5 态）——as_str/from_db 往返恒等 + 全集锁定。
    /// 新增状态须同步枚举；删除属破坏性变更需评审。
    #[test]
    fn test_review_status_roundtrip_and_full_set() {
        let all = [
            (ReviewStatus::Pending, "pending"),
            (ReviewStatus::Success, "success"),
            (ReviewStatus::Failed, "failed"),
            (ReviewStatus::Interrupted, "interrupted"),
            (ReviewStatus::Skipped, "skipped"),
        ];
        for (variant, db_str) in all {
            assert_eq!(variant.as_str(), db_str, "as_str 与 DB 字面量必须一致");
            assert_eq!(
                ReviewStatus::from_db(db_str),
                Some(variant),
                "from_db 必须能解析全部枚举值"
            );
        }
        // 未知值返回 None（调用方按场景兜底，不写死默认态）
        assert_eq!(ReviewStatus::from_db("unknown_x"), None);
        assert_eq!(ReviewStatus::from_db(""), None);
    }

    /// serde 形态与存量 DB/前端 JSON 的 snake_case 字面量一致；
    /// interrupted/skipped 是本族独有值，须锁定字面。
    #[test]
    fn test_review_status_serde_snake_case_compatible() {
        assert_eq!(
            serde_json::to_string(&ReviewStatus::Interrupted).unwrap(),
            r#""interrupted""#
        );
        assert_eq!(
            serde_json::to_string(&ReviewStatus::Skipped).unwrap(),
            r#""skipped""#
        );
        let back: ReviewStatus = serde_json::from_str(r#""interrupted""#).unwrap();
        assert_eq!(back, ReviewStatus::Interrupted);
    }
}
