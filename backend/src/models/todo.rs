//! Todo 域数据模型（096-W4-2：从 models/mod.rs 按域拆分，逐字搬迁零改动）。
//!
//! 含：TodoStatus / Todo / ComputedBucket / TodoCenter 分页族 / Tag / 各请求 DTO /
//! TodoTemplate 族 / 备份导出 DTO / 伪 ID 工具族 / prompt 占位符工具。
//! 经 `models::mod` 的 `pub use todo::*` 聚合，外部引用路径不变。

use serde::{Deserialize, Serialize};

// 跨域引用（经 models 聚合根可达）：Todo 关联的执行用量类型
use crate::models::ExecutionUsage;

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
    /// 所属工作空间 ID（workspaces.id），唯一键。
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
    #[serde(default = "crate::models::default_true")]
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
    /// 专家/团队名称（WorkBuddy plugin.json 中的 name 字段）。
    /// 执行时自动加载对应的 Agent MD 和 Skills 注入 prompt。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expert_name: Option<String>,
    /// 任务级指定的执行模型（覆盖 executor.default_model）。
    /// None = 未指定，执行时回退到执行器默认模型；执行器也未指定则不传 --model。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 事项级技能名列表（需求 055）。工艺安装时从环节 skills 写入；
    /// 执行时以 `/skill-name` 逐行注入 prompt 尾部，由执行器 CLI 解析。
    /// `#[serde(default)]` 保证旧客户端/旧数据反序列化不受影响。
    #[serde(default)]
    pub skills: Vec<String>,
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
    /// 连续失败次数：从最近一次执行往前数连续 failed 的条数，遇非 failed 即停。
    /// 0=最近一次非失败（或无记录）。时间/事件驱动卡片健康展示用。
    #[serde(default)]
    pub consecutive_failure_count: i64,
    /// 最近一次 webhook 触发的时间（trigger_type='webhook' 的最新记录）。
    /// 仅事件驱动卡片展示「最近触发时间」用；手动「执行一次」不顶替该时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_webhook_trigger_at: Option<String>,
    /// 绑定的工作空间斜杠命令（command_type='todo' 绑定该 todo 的第一条）。
    /// 卡片展示「绑定命令: /xxx」；手动触发 Tab 可据此筛「仅看可命令触发」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_slash_command: Option<String>,
}

/// 事项中心服务端分页响应（056）。
///
/// `bucket_counts` 供前端分类 Tab 角标：统计口径=应用 search 后、应用 bucket 过滤前，
/// 与「分页前完成分桶」的旧内存实现语义一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoCenterPage {
    pub items: Vec<TodoCenterItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub bucket_counts: std::collections::HashMap<String, i64>,
    /// 当前工作空间内出现过的 action_type 去重列表（卡片墙「来源筛选」下拉数据源）
    #[serde(default)]
    pub action_types: Vec<String>,
}

/// 事项轻量摘要（056）：不含 prompt/acceptance_criteria 大文本字段。
///
/// 供看板全量渲染、下拉选择、记录详情补标题等「只需要展示字段」的场景，
/// 替代过去「整行 Todo 全量拉取」的做法。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoBrief {
    pub id: i64,
    pub title: String,
    pub status: TodoStatus,
    #[serde(default)]
    pub executor: Option<String>,
    pub updated_at: String,
    #[serde(default)]
    pub archived_at: Option<String>,
    /// 所属工作空间（拖拽/归属展示用，小字段）
    #[serde(default)]
    pub workspace_id: Option<i64>,
    /// 标签 id 列表（看板标签徽章用；批量查询补算，不逐行 N+1）
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    /// prompt 是否非空（看板「展开 prompt」区块的显示开关，不必传输 prompt 本体）
    #[serde(default)]
    pub has_prompt: bool,
}

/// 事项列表分页响应（056，旧全量接口改造后的统一结构）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoListPage {
    pub items: Vec<Todo>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// Loop 引用摘要：事项中心展示「所属环路」与「工艺」两列共用。
/// - loop_id/loop_name：引用该事项的环路实例。
/// - process_template_*：该环路所基于的工艺模板（loops.process_template_id → process_templates），
///   供「工艺」列按 #模板ID-模板名-版本 展示；环路无模板时为 None。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopRefSummary {
    pub loop_id: i64,
    pub loop_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_template_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_template_name: Option<String>,
    /// 工艺版本：优先取环路实例化快照，缺失时由 SQL 回退模板当前版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_template_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: Option<String>,
    pub content: String,
    pub status: String,
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
    /// 工作空间 ID（workspaces.id），唯一键。
    /// 创建时必填；handler 据此查 path 写入 DB cwd 字段。
    /// 使用 #[serde(default)]：v1 路由从 URL 路径覆盖此值，body 中不传也不影响。
    #[serde(default)]
    pub workspace_id: Option<i64>,
    /// Action 类型标记（如 "rewrite_title"、"optimize_prompt"），
    /// 仅供前端 ActionButton 组件做 UI 分类展示，不影响执行逻辑。
    #[serde(default)]
    pub action_type: Option<String>,
    /// Action 键值，与 action_type 配合唯一标识一个 action 模板 todo。
    #[serde(default)]
    pub action_key: Option<String>,
    /// 专家/团队名称（WorkBuddy plugin.json 中的 name 字段）。
    /// 执行时自动加载对应的 Agent MD 和 Skills 注入 prompt。
    #[serde(default)]
    pub expert_name: Option<String>,
    /// 任务级执行模型（覆盖执行器默认）。None = 用执行器默认模型。
    #[serde(default)]
    pub model: Option<String>,
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
    /// 工作空间 ID（workspaces.id）。
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
    /// 专家/团队名称（WorkBuddy plugin.json 中的 name 字段）。
    #[serde(default)]
    pub expert_name: Option<String>,
    /// 任务级执行模型（覆盖执行器默认）。None = 不修改；Some("") = 清除；Some(v) = 设置。
    #[serde(default)]
    pub model: Option<String>,
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
    /// 目标工作空间 ID（workspaces.id）。
    pub workspace_id: i64,
}

/// 批量复制事项到其他工作空间请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCopyTodoWorkspaceRequest {
    pub ids: Vec<i64>,
    /// 目标工作空间 ID（workspaces.id）。
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
    /// 目标工作空间 ID（workspaces.id）。
    pub workspace_id: i64,
}

/// 批量复制环路到其他工作空间请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCopyLoopWorkspaceRequest {
    pub ids: Vec<i64>,
    /// 目标工作空间 ID（workspaces.id）。
    pub workspace_id: i64,
}

/// 批量 workspace 操作返回结果（通用）。
#[derive(Debug, Clone, Serialize)]
pub struct BatchWorkspaceResult {
    pub updated_count: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentCompletedTodo {
    pub todo_id: i64,
    pub title: String,
    pub prompt: Option<String>,
    pub executor: Option<String>,
    pub tag_ids: Vec<i64>,
    /// 所属工作空间（056 补充：纪念板据此反查项目名，免去前端按 id 二次查询）
    #[serde(default)]
    pub workspace_id: Option<i64>,
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
    /// 备份时的工作空间 ID，为空表示未分配
    #[serde(default)]
    pub workspace_id: Option<i64>,
    /// 任务级指定的执行模型（备份恢复后保留，不影响老备份导入）。
    #[serde(default)]
    pub model: Option<String>,
}

impl TodoBackup {
    /// 备份导入（merge_backup 新建分支）的 ActiveModel 转换单点（096-W4-1）。
    ///
    /// 与 `db::todo::todo_backup_from_model`（导出侧）构成同一字段映射的两个方向，
    /// 加备份字段时两处必须同步——集中在相邻代码块以降低漏改概率。
    /// `resolved` 为调用方解析出的 (workspace_id, workspace_path) 成对值；时间戳由调用方给。
    pub fn into_active_model(
        &self,
        resolved: (i64, Option<String>),
        now: String,
    ) -> crate::db::entity::todos::ActiveModel {
        use sea_orm::ActiveValue;
        crate::db::entity::todos::ActiveModel {
            title: ActiveValue::Set(self.title.clone()),
            prompt: ActiveValue::Set(Some(self.prompt.clone())),
            status: ActiveValue::Set(Some(self.status.to_string())),
            executor: ActiveValue::Set(self.executor.clone()),
            scheduler_enabled: ActiveValue::Set(Some(self.scheduler_enabled)),
            scheduler_config: ActiveValue::Set(self.scheduler_config.clone()),
            workspace_path: ActiveValue::Set(resolved.1),
            workspace_id: ActiveValue::Set(Some(resolved.0)),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            action_type: ActiveValue::Set(self.action_type.clone()),
            action_key: ActiveValue::Set(self.action_key.clone()),
            model: ActiveValue::Set(self.model.clone()),
            ..Default::default()
        }
    }
}

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
