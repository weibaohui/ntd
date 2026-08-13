use serde::{Deserialize, Serialize};

pub mod loop_;
pub use loop_::*;

// 096-W4-2：按域拆分四文件（todo/execution/loop/executor）。
// pub use 聚合保持外部引用路径（crate::models::Xxx）零改动。
pub mod execution;
pub mod executor;
pub mod todo;
pub use execution::*;
pub use executor::*;
pub use todo::*;











#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBot {
    pub id: i64,
    pub bot_type: String,
    pub bot_name: String,
    pub app_id: String,
    #[serde(skip_serializing)]
    pub app_secret: String,
    pub bot_open_id: Option<String>,
    /// 所有者 open_id（推送目标权威来源）；区别于语义错位的历史字段 bot_open_id
    pub owner_open_id: Option<String>,
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

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn default_trigger_type() -> String { "manual".to_string() }












































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







// ============ 环路导入导出 DTO ============
// 方案：伪ID引用（@loop_1, @todo_1 等）解决跨实体引用问题

// ============ 导入预览/执行响应 DTO ============

// Business error codes
pub mod codes {
    pub const NOT_FOUND: i32 = 40001;
    pub const BAD_REQUEST: i32 = 40002;
    pub const FORBIDDEN: i32 = 40003;
    /// 资源状态冲突（409）：编辑系统工艺、重名创建、已有实例 Loop 拒绝删除等。
    pub const CONFLICT: i32 = 40901;
    pub const INTERNAL: i32 = 50001;
}

// ============ 伪ID工具函数 ============








#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    /// 093：cutoff helper 的格式契约（与 utc_timestamp 同格式）与时间偏移正确性。
    #[test]
    fn test_utc_timestamp_minus_hours_format_and_offset() {
        let cutoff = super::utc_timestamp_minus_hours(24);
        // 格式形状必须与 utc_timestamp 一致（毫秒精度 T/Z ISO），否则与存量数据比较失真
        let re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$").unwrap();
        assert!(re.is_match(&cutoff), "cutoff 格式应为毫秒级 ISO：{cutoff}");
        // 与「现在」做字符串比较应恒小（同格式 ISO 字符串序 = 时间序），且差值≈24h
        let now = super::utc_timestamp();
        assert!(cutoff < now, "24 小时前的 cutoff 应小于当前时间戳");
        // 解析回时间类型验证差值在容差内（防格式对了但偏移算错）
        let parsed = chrono::DateTime::parse_from_rfc3339(&cutoff).unwrap();
        let delta = chrono::Utc::now() - parsed.with_timezone(&chrono::Utc);
        assert!(
            delta.num_hours() >= 23 && delta.num_hours() <= 25,
            "偏移应在 24h±1h 容差内，实际 {delta}"
        );
    }

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
        assert_eq!(req.workspace_id, Some(42));
    }

    #[test]
    fn test_create_todo_request_default_tag_ids() {
        // workspace_id 可选（#[serde(default)]）：缺失时默认 None，v1 路由从路径覆盖。
        // 测试确保 tag_ids 的 #[serde(default)] 仍正常工作。
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
