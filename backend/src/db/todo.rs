use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Statement,
};

use crate::db::entity::project_directories;
use crate::db::entity::tags;
use crate::db::entity::{todo_tags, todos};
use crate::db::Database;
use crate::models::{ComputedBucket, Todo, TodoBackup, TodoCenterItem, TodoStatus, compute_bucket};

/// todo_type 取值：异常处理载体 Todo（工艺安装时按 abnormal_handler.prompt 创建）。
/// 模式与 todo_type=2 评审实例对称；事项列表以「异常处理」标签区分。需求 035。
pub const TODO_TYPE_ABNORMAL_HANDLER: i32 = 3;

/// todo_type 取值：任务讨论区「@触发执行」的隐藏载体 Todo（需求 060）。
///
/// 讨论帖里 @专家/@执行器 时创建一个载体 Todo 承载执行（执行系统是 Todo 中心的，
/// 必须有 Todo 提供 executor/prompt/expert_name）。这类 Todo 不应出现在事项中心 /
/// todo 列表，因此：① 各列表查询用 `is_discussion_carrier` 片段排除；② 执行完成时
/// 由 `finalize_discussion_post` 软删（deleted_at），让所有 `deleted_at IS NULL` 查询兜底排除。
pub const TODO_TYPE_DISCUSSION: i32 = 4;

/// SQL 片段：在 todo 列表/事项中心查询里排除讨论载体 Todo（todo_type=4）。
/// 用 COALESCE 兜底 NULL，与 todo_type 既有「NULL 视为普通(0)」口径一致。
const DISCUSSION_CARRIER_EXCLUDE: &str = "COALESCE(t.todo_type, 0) != 4";

/// 056：computed_bucket 的 SQL 表达式，优先级与 `models::compute_bucket` 严格一致
/// （已归档 > Loop 驱动 > 时间驱动 > 事件驱动 > 手动）。
/// 两侧实现由测试对拍防漂移（见本文件测试 `test_center_bucket_sql_matches_rust`）。
const TODO_CENTER_BUCKET_EXPR: &str = "CASE \
    WHEN t.archived_at IS NOT NULL THEN 'archived' \
    WHEN (SELECT COUNT(*) FROM loop_steps ls WHERE ls.todo_id = t.id AND ls.enabled = 1) > 0 THEN 'loop_driven' \
    WHEN t.scheduler_config IS NOT NULL THEN 'time_driven' \
    WHEN t.webhook_enabled = 1 THEN 'event_driven' \
    ELSE 'manual' END";

/// ComputedBucket → SQL 表达式输出字符串（与 serde snake_case 保持一致）。
fn center_bucket_str(b: ComputedBucket) -> &'static str {
    match b {
        ComputedBucket::Archived => "archived",
        ComputedBucket::LoopDriven => "loop_driven",
        ComputedBucket::TimeDriven => "time_driven",
        ComputedBucket::EventDriven => "event_driven",
        ComputedBucket::Manual => "manual",
    }
}

/// 排序字段白名单：拒绝白名单外输入，退化为默认 id——
/// 排序列名要拼进 SQL 字符串（参数绑定不能绑标识符），白名单是唯一安全途径。
fn center_sort_column(sort_by: Option<&str>) -> &'static str {
    match sort_by {
        Some("updated_at") => "t.updated_at",
        Some("title") => "t.title COLLATE NOCASE",
        Some("status") => "t.status",
        Some("computed_bucket") => TODO_CENTER_BUCKET_EXPR,
        _ => "t.id",
    }
}

/// 事项中心分页查询参数（056）：打包传参避免 too_many_arguments。
/// `search=None` 表示不搜索；排序方向由 `sort_desc` 显式给出（handler 默认 true）。
#[derive(Debug, Clone)]
pub struct TodoCenterPageQuery<'a> {
    pub workspace_id: Option<i64>,
    pub bucket: Option<ComputedBucket>,
    pub search: Option<&'a str>,
    /// 状态精确过滤（卡片墙「状态筛选」下拉；None=全部）
    pub status: Option<&'a str>,
    /// 动作类型精确过滤（卡片墙「来源筛选」下拉；None=全部）
    pub action_type: Option<&'a str>,
    pub sort_by: Option<&'a str>,
    pub sort_desc: bool,
    pub page: i64,
    pub page_size: i64,
}

/// 事项中心分页结果（056）：`page` 是按 total 截断后的**有效页码**——
/// db 层截断后必须把它传回给调用方，否则响应元数据里的 page 会是
/// 未截断的请求值（评审 F2：page=100000 但内容是第 3 页的矛盾）。
#[derive(Debug)]
pub struct TodoCenterPageData {
    pub items: Vec<TodoCenterItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub bucket_counts: std::collections::HashMap<String, i64>,
    /// 当前工作空间内出现过的 action_type 去重列表（卡片墙「来源筛选」下拉数据源）
    pub action_types: Vec<String>,
}

/// QueryResult 行 → TodoBrief（056 轻量摘要）。独立函数保持 get_todo_briefs 函数体合规。
fn brief_from_row(row: &sea_orm::QueryResult) -> Result<crate::models::TodoBrief, sea_orm::DbErr> {
    Ok(crate::models::TodoBrief {
        id: row.try_get_by("id")?,
        // title 在 schema 上 NOT NULL：读不出来是数据损坏信号，传播错误而非静默置空
        title: row.try_get_by("title")?,
        status: row
            .try_get_by::<Option<String>, _>("status")?
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(TodoStatus::Pending),
        executor: row.try_get_by("executor")?,
        updated_at: row
            .try_get_by::<Option<String>, _>("updated_at")?
            .unwrap_or_default(),
        archived_at: row.try_get_by("archived_at")?,
        workspace_id: row.try_get_by("workspace_id")?,
        // tag_ids 由调用方批量补算（见 get_todo_briefs），此处占位
        tag_ids: Vec::new(),
        // SQLite 布尔表达式以 0/1 返回
        has_prompt: row.try_get_by::<i64, _>("has_prompt")? != 0,
    })
}

pub struct TodoUpdate<'a> {
    pub id: i64,
    pub title: &'a str,
    pub prompt: &'a str,
    pub status: TodoStatus,
    pub executor: Option<&'a str>,
    pub scheduler_enabled: Option<bool>,
    pub scheduler_config: Option<&'a str>,
    pub scheduler_timezone: Option<&'a str>,
    /// 工作空间 ID（project_directories.id）。
    /// None=保持当前工作空间，Some(id)=迁移到该工作空间（handler 同时传 path）。
    /// 不接受路径——DAO 不再单独接受 path 入参。
    pub workspace_id: Option<i64>,
    pub webhook_enabled: Option<bool>,
    pub acceptance_criteria: Option<&'a str>,
    pub auto_review_enabled: Option<bool>,
    /// Action 类型标记（如 "title_optimize"、"prompt_optimize"）。
    /// 与 action_key 配合，由 /api/actions/execute 用于查找或自动创建 action 模板 todo。
    pub action_type: Option<&'a str>,
    /// Action 键值，与 action_type 配合唯一标识一个 action 模板 todo。
    /// 由 /api/actions/execute 用于查找或自动创建 action 模板 todo。
    pub action_key: Option<&'a str>,
    /// 专家/团队名称（WorkBuddy plugin.json 中的 name 字段）。
    /// 执行时自动加载对应的 Agent MD 和 Skills 注入 prompt。
    pub expert_name: Option<&'a str>,
    /// 任务级执行模型。Some(非空)=设置，Some("")=清除，None=不修改。
    pub model: Option<&'a str>,
}

pub struct SchedulerUpdate<'a> {
    pub id: i64,
    pub enabled: bool,
    pub config: Option<&'a str>,
    pub timezone: Option<&'a str>,
}

/// 事项中心批量聚合结果：把组装 TodoCenterItem 需要的所有聚合 map 收进一个结构体，
/// 避免 build_center_item 参数膨胀（6+ 个 map 参数会触发 too_many_arguments 且难读）。
pub struct TodoCenterAggregates {
    pub loop_count_map: std::collections::HashMap<i64, i64>,
    pub referencing_loops_map: std::collections::HashMap<i64, Vec<crate::models::LoopRefSummary>>,
    pub last_exec_map: std::collections::HashMap<i64, crate::db::LatestExecutionSummary>,
    pub consecutive_fail_map: std::collections::HashMap<i64, i64>,
    pub last_webhook_map: std::collections::HashMap<i64, String>,
    pub slash_command_map: std::collections::HashMap<i64, String>,
}

/// 在事务内按 id 解析工作空间的 `(id, path)` 对。
/// 返回 `None` 表示该 id 在当前库不存在（跨环境导入时的悬空 id），
/// 由调用方降级处理——不强行写悬空 id，也不沿用备份里可能不存在的路径。
async fn resolve_workspace_pair(
    txn: &sea_orm::DatabaseTransaction,
    id: i64,
) -> Result<Option<(i64, String)>, sea_orm::DbErr> {
    Ok(project_directories::Entity::find_by_id(id)
        .one(txn)
        .await?
        .map(|m| (m.id, m.path)))

}

impl Database {
    fn model_to_todo(m: todos::Model, tag_ids: Vec<i64>) -> Todo {
        let scheduler_enabled = m.scheduler_enabled.unwrap_or(false);
        let scheduler_config = m.scheduler_config.clone();
        let scheduler_timezone = m.scheduler_timezone.clone();
        let scheduler_next_run_at = if scheduler_enabled {
            scheduler_config
                .as_deref()
                .and_then(|config| {
                    super::compute_next_run(config, scheduler_timezone.as_deref())
                })
        } else {
            None
        };
        Todo {
            id: m.id,
            title: m.title,
            prompt: m.prompt.unwrap_or_default(),
            status: m
                .status
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or(TodoStatus::Pending),
            created_at: m.created_at.unwrap_or_default(),
            updated_at: m.updated_at.unwrap_or_default(),
            tag_ids,
            executor: m.executor,
            expert_name: m.expert_name,
            model: m.model,
            // skills 列为 JSON 数组串（需求 055）；脏数据回退空数组，
            // 与 loop_steps.skill_names / step_template_refs 的解析口径一致。
            skills: serde_json::from_str(&m.skills).unwrap_or_default(),
            scheduler_enabled,
            scheduler_config,
            scheduler_timezone,
            scheduler_next_run_at,
            task_id: m.task_id,
            // cwd 字段保留——后端 executor_service / worktree / spawn_lifecycle 等
            // 子系统仍按 path 字符串读取 cwd。API 层不再把 workspace_path 暴露给前端。
            workspace_path: m.workspace_path,
            workspace_id: m.workspace_id,
            webhook_enabled: m.webhook_enabled.unwrap_or(false),
            acceptance_criteria: m.acceptance_criteria,
            todo_type: m.todo_type.unwrap_or(0),
            parent_todo_id: m.parent_todo_id,
            review_template_id: m.review_template_id,
            auto_review_enabled: m.auto_review_enabled.unwrap_or(false),
            action_type: m.action_type,
            action_key: m.action_key,
            archived_at: m.archived_at,
        }
    }

    pub(crate) async fn fetch_tag_ids_for_many(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<i64>>, sea_orm::DbErr> {
        if todo_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let models = todo_tags::Entity::find()
            .filter(todo_tags::Column::TodoId.is_in(todo_ids.to_vec()))
            .all(&self.conn)
            .await?;
        Ok(models
            .into_iter()
            .fold(std::collections::HashMap::new(), |mut map, t| {
                map.entry(t.todo_id).or_default().push(t.tag_id);
                map
            }))
    }

    // 056：`get_todos` / `get_todos_by_workspace_id` / `get_todo_center` 三个全量
    // 查询已删除，由以下接口替代：
    // - get_todos_page_by_workspace（日常视图，强制分页）
    // - get_todo_center_page（事项中心，SQL 分桶 + 分页）
    // - get_todo_briefs / get_todo_ids_by_workspace / count_todos_by_workspace（轻量）
    // - get_todos_batch_after_id（云同步游标分批）

    /// 批量构造 TodoCenterAggregates（列表与单条路径共用，避免两处重复构造）。
    async fn build_center_aggregates(
        &self,
        ids: &[i64],
    ) -> Result<TodoCenterAggregates, sea_orm::DbErr> {
        Ok(TodoCenterAggregates {
            loop_count_map: self.count_enabled_loop_steps_by_todos(ids).await?,
            referencing_loops_map: self.get_referencing_loops_for_todos(ids).await?,
            last_exec_map: self.get_latest_execution_summaries_for_todos(ids).await?,
            consecutive_fail_map: self.get_consecutive_failure_counts_for_todos(ids).await?,
            last_webhook_map: self.get_last_webhook_trigger_for_todos(ids).await?,
            slash_command_map: self.get_bound_slash_commands_for_todos(ids).await?,
        })
    }

    /// 由已载入的 Todo + 批量聚合结果组装单个 TodoCenterItem（纯函数，便于复用与单测）。
    fn build_center_item(todo: Todo, aggs: &TodoCenterAggregates) -> TodoCenterItem {
        // 未出现在聚合 map 中的 todo 计数视为 0（未被任何启用 Loop 引用）
        let used_by_loop_step_count = aggs.loop_count_map.get(&todo.id).copied().unwrap_or(0);
        let computed_bucket = compute_bucket(
            todo.archived_at.as_deref(),
            used_by_loop_step_count,
            todo.scheduler_config.as_deref(),
            todo.webhook_enabled,
        );
        let (last_execution_status, last_execution_at) = aggs
            .last_exec_map
            .get(&todo.id)
            .map(|s| (s.status.clone(), s.display_at().map(str::to_string)))
            .unwrap_or((None, None));
        let referencing_loops = aggs
            .referencing_loops_map
            .get(&todo.id)
            .cloned()
            .unwrap_or_default();
        let consecutive_failure_count = aggs
            .consecutive_fail_map
            .get(&todo.id)
            .copied()
            .unwrap_or(0);
        let last_webhook_trigger_at = aggs.last_webhook_map.get(&todo.id).cloned();
        let bound_slash_command = aggs.slash_command_map.get(&todo.id).cloned();
        TodoCenterItem {
            todo,
            computed_bucket,
            used_by_loop_step_count,
            last_execution_status,
            last_execution_at,
            referencing_loops,
            consecutive_failure_count,
            last_webhook_trigger_at,
            bound_slash_command,
        }
    }

    // ==================== 056：服务端分页与轻量查询 ====================
    //
    // computed_bucket 的 SQL 表达式。优先级与 `models::compute_bucket` 严格一致：
    // 已归档 > Loop 驱动 > 时间驱动 > 事件驱动 > 手动。
    // 两侧实现由集成测试对拍，防止语义漂移（见 todo.rs 测试模块）。

    /// bucket 过滤/计数共用的 WHERE 组装：返回 (sql_fragment, values)。
    /// bucket 条件由 `q.bucket` 决定——bucket_counts 统计时调用方传
    /// `bucket=None` 的克隆，即「应用 search/status/action_type 但不应用 bucket」，
    /// 保证 Tab 角标不随当前选中分类塌缩。
    fn center_page_where(
        q: &TodoCenterPageQuery<'_>,
    ) -> (String, Vec<sea_orm::Value>) {
        let mut sql = String::from("t.deleted_at IS NULL");
        // 排除讨论载体 Todo（todo_type=4），避免 @触发的隐藏载体污染事项中心。
        sql.push_str(" AND ");
        sql.push_str(DISCUSSION_CARRIER_EXCLUDE);
        let mut values: Vec<sea_orm::Value> = Vec::new();
        if let Some(wid) = q.workspace_id {
            sql.push_str(" AND t.workspace_id = ?");
            values.push(wid.into());
        }
        if let Some(status) = q.status.map(str::trim).filter(|s| !s.is_empty()) {
            sql.push_str(" AND t.status = ?");
            values.push(status.into());
        }
        if let Some(at) = q.action_type.map(str::trim).filter(|s| !s.is_empty()) {
            sql.push_str(" AND t.action_type = ?");
            values.push(at.into());
        }
        if let Some(kw) = q.search.map(str::trim).filter(|s| !s.is_empty()) {
            // LIKE 通配符先转义再包 %，避免用户输入 %/_ 被当通配符；ESCAPE 用反斜杠
            let escaped = kw.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            let pattern = format!("%{}%", escaped.to_lowercase());
            sql.push_str(" AND (LOWER(t.title) LIKE ? ESCAPE '\\' OR LOWER(t.prompt) LIKE ? ESCAPE '\\')");
            values.push(pattern.clone().into());
            values.push(pattern.into());
        }
        if let Some(b) = q.bucket {
            sql.push_str(" AND ");
            sql.push_str(TODO_CENTER_BUCKET_EXPR);
            sql.push_str(" = ?");
            values.push(center_bucket_str(b).into());
        }
        (sql, values)
    }

    /// 事项中心服务端分页（056 核心）：SQL 层完成分桶/搜索/排序/分页，
    /// 聚合字段只对当页 ids 批量补算（避免全表聚合）。
    ///
    /// 返回 `TodoCenterPageData`；total/bucket_counts 过滤口径见 `center_page_where`，
    /// `page` 为按 total 截断后的有效页码（响应元数据必须与截断一致，评审 F2）。
    pub async fn get_todo_center_page(
        &self,
        q: TodoCenterPageQuery<'_>,
    ) -> Result<TodoCenterPageData, sea_orm::DbErr> {
        let page_size = q.page_size.clamp(1, 200);
        // 先计数再钳页码（CodeRabbit#2）：page 无上界时大 OFFSET 是外部可触发的慢查询；
        // offset 超出 total 时截断到最后一页，而不是让 SQLite 逐行跳过几千万行。
        let total = self.count_todo_center(&q).await?;
        let max_page = if total == 0 { 1 } else { (total + page_size - 1) / page_size };
        let page = q.page.max(1).min(max_page.max(1));
        let (where_sql, values) = Self::center_page_where(&q);
        let order = if q.sort_desc { "DESC" } else { "ASC" };
        let sql = format!(
            "SELECT t.id FROM todos t WHERE {where_sql} ORDER BY {} {order} LIMIT ? OFFSET ?",
            center_sort_column(q.sort_by),
        );
        let mut vals = values.clone();
        vals.push(page_size.into());
        vals.push(((page - 1) * page_size).into());
        let rows = self
            .conn
            .query_all(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                vals,
            ))
            .await?;
        let ids: Vec<i64> = rows
            .iter()
            .filter_map(|r| r.try_get_by::<i64, _>("id").ok())
            .collect();

        let bucket_counts = self.count_todo_center_buckets(&q).await?;
        let action_types = self.list_center_action_types(q.workspace_id).await?;
        let items = self.build_center_items_by_ids(&ids).await?;
        Ok(TodoCenterPageData { items, total, page, page_size, bucket_counts, action_types })
    }

    /// 当前工作空间内出现过的 action_type 去重列表（卡片墙「来源筛选」下拉数据源，056）。
    /// 口径：未删除事项 + 同 ws；不受 search/status 等过滤影响（下拉项应稳定）。
    async fn list_center_action_types(
        &self,
        workspace_id: Option<i64>,
    ) -> Result<Vec<String>, sea_orm::DbErr> {
        let mut sql = String::from(
            "SELECT DISTINCT action_type FROM todos WHERE deleted_at IS NULL AND action_type IS NOT NULL AND action_type != ''",
        );
        let mut values: Vec<sea_orm::Value> = Vec::new();
        if let Some(wid) = workspace_id {
            sql.push_str(" AND workspace_id = ?");
            values.push(wid.into());
        }
        sql.push_str(" ORDER BY action_type");
        let rows = self
            .conn
            .query_all(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                values,
            ))
            .await?;
        Ok(rows
            .iter()
            .filter_map(|r| r.try_get_by::<String, _>("action_type").ok())
            .collect())
    }

    /// 事项中心总数（与分页查询同一过滤口径，含 bucket）。
    async fn count_todo_center(
        &self,
        q: &TodoCenterPageQuery<'_>,
    ) -> Result<i64, sea_orm::DbErr> {
        let (where_sql, values) = Self::center_page_where(q);
        let sql = format!("SELECT COUNT(*) AS cnt FROM todos t WHERE {where_sql}");
        let row = self
            .conn
            .query_one(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                values,
            ))
            .await?;
        Ok(row
            .and_then(|r| r.try_get_by::<i64, _>("cnt").ok())
            .unwrap_or(0))
    }

    /// 各 bucket 计数（应用 search、不应用 bucket 过滤——Tab 角标语义）。
    async fn count_todo_center_buckets(
        &self,
        q: &TodoCenterPageQuery<'_>,
    ) -> Result<std::collections::HashMap<String, i64>, sea_orm::DbErr> {
        // bucket 角标口径：应用 search/status/action_type，但不应用 bucket 过滤
        let mut count_q = q.clone();
        count_q.bucket = None;
        let (where_sql, values) = Self::center_page_where(&count_q);
        let sql = format!(
            "SELECT {TODO_CENTER_BUCKET_EXPR} AS bucket, COUNT(*) AS cnt FROM todos t WHERE {where_sql} GROUP BY bucket"
        );
        let rows = self
            .conn
            .query_all(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                values,
            ))
            .await?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                Some((
                    r.try_get_by::<String, _>("bucket").ok()?,
                    r.try_get_by::<i64, _>("cnt").ok()?,
                ))
            })
            .collect())
    }

    /// 按 id 列表加载 TodoCenterItem（保序），tag/聚合批量补算——分页路径的组装段。
    async fn build_center_items_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<Vec<TodoCenterItem>, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let models = todos::Entity::find()
            .filter(todos::Column::Id.is_in(ids.iter().copied()))
            .all(&self.conn)
            .await?;
        // is_in 不保证返回顺序，按传入 ids 重排，保持与 ORDER BY 一致的分页语义
        let mut by_id: std::collections::HashMap<i64, todos::Model> =
            models.into_iter().map(|m| (m.id, m)).collect();
        let tag_map = self.fetch_tag_ids_for_many(ids).await?;
        let aggs = self.build_center_aggregates(ids).await?;
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(m) = by_id.remove(id) {
                let tag_ids = tag_map.get(id).cloned().unwrap_or_default();
                items.push(Self::build_center_item(Self::model_to_todo(m, tag_ids), &aggs));
            }
        }
        Ok(items)
    }

    /// 轻量摘要列表（056）：只取展示字段，不读 prompt 大文本。
    /// `ids=None` 返回该 ws 全部 brief（看板全量渲染用），此模式隐藏已归档事项
    /// （与旧「日常视图」语义一致）；`ids=Some` 为按 id 定点查询，不过滤归档
    /// （已归档事项也需要能解析出标题）。`hours` 过滤 updated_at。
    ///
    /// 实现说明：用原生 SQL 而非 select_only()——SeaORM 的 Model 映射要求全列，
    /// 部分列查询走 into_model 需额外派生类型，原生 SQL + try_get_by 更直白。
    pub async fn get_todo_briefs(
        &self,
        workspace_id: Option<i64>,
        ids: Option<&[i64]>,
        hours: Option<u32>,
    ) -> Result<Vec<crate::models::TodoBrief>, sea_orm::DbErr> {
        let mut sql = String::from(
            "SELECT id, title, status, executor, updated_at, archived_at, workspace_id, \
             (prompt IS NOT NULL AND prompt != '') AS has_prompt \
             FROM todos WHERE deleted_at IS NULL AND COALESCE(todo_type, 0) != 4",
        );
        let mut values: Vec<sea_orm::Value> = Vec::new();
        match ids {
            Some(ids) if !ids.is_empty() => {
                // 定点模式（CodeRabbit#6）：id 全局唯一，不再叠加 workspace_id 条件——
                // Dashboard 全局运营视图的跨 ws 标题反查、运行记录抽屉补标题都依赖这一点。
                let (ph, vals) = Database::in_clause(ids);
                sql.push_str(&format!(" AND id IN ({ph})"));
                values.extend(vals);
            }
            Some(_) => return Ok(Vec::new()), // 空 id 集 = 空结果，避免 IN () 非法 SQL
            None => {
                // 看板模式：按 ws 过滤且隐藏已归档（与旧 getAllTodos 数据源语义一致）
                if let Some(wid) = workspace_id {
                    sql.push_str(" AND workspace_id = ?");
                    values.push(wid.into());
                }
                sql.push_str(" AND archived_at IS NULL");
            }
        }
        if let Some(h) = hours.filter(|&h| h > 0) {
            // hours 已验证 > 0 的 u32，format! 是构建 SQL 时间表达式的唯一途径
            sql.push_str(&format!(
                " AND REPLACE(REPLACE(updated_at, 'T', ' '), 'Z', '') >= datetime('now', '-{h} hours')"
            ));
        }
        sql.push_str(" ORDER BY updated_at DESC");
        let rows = self
            .conn
            .query_all(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                values,
            ))
            .await?;
        // tag_ids 批量补算：一次 IN 查询建立 id→tags 映射，不逐行 N+1
        let mut briefs: Vec<crate::models::TodoBrief> =
            rows.iter().map(brief_from_row).collect::<Result<_, _>>()?;
        let ids: Vec<i64> = briefs.iter().map(|b| b.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;
        for b in &mut briefs {
            b.tag_ids = tag_map.get(&b.id).cloned().unwrap_or_default();
        }
        Ok(briefs)
    }

    /// 工作空间内全部 todo id（056 轻量接口）：只取 id 列，隐藏已归档（日常视图片语义）。
    pub async fn get_todo_ids_by_workspace(
        &self,
        workspace_id: i64,
    ) -> Result<Vec<i64>, sea_orm::DbErr> {
        let rows = self
            .conn
            .query_all(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                "SELECT id FROM todos WHERE deleted_at IS NULL AND archived_at IS NULL AND COALESCE(todo_type, 0) != 4 AND workspace_id = ? ORDER BY id DESC",
                vec![workspace_id.into()],
            ))
            .await?;
        Ok(rows
            .iter()
            .filter_map(|r| r.try_get_by::<i64, _>("id").ok())
            .collect())
    }

    /// 工作空间内未删除且未归档 todo 计数（056 轻量接口）：COUNT(*)，不拉行。
    pub async fn count_todos_by_workspace(
        &self,
        workspace_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let count = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .filter(todos::Column::ArchivedAt.is_null())
            .filter(todos::Column::WorkspaceId.eq(workspace_id))
            // 排除讨论载体 Todo（todo_type=4），不计入工作空间 todo 数。
            .filter(sea_orm::sea_query::Expr::cust("COALESCE(todo_type, 0) != 4"))
            .count(&self.conn)
            .await?;
        Ok(count.try_into().unwrap_or(i64::MAX))
    }

    /// 旧 /todos 接口的服务端分页版（056 决策 3b：直接强制分页）。
    /// 隐藏已归档（「日常视图」语义，与旧 get_todos_by_workspace_id 一致）；
    /// hours 过滤从内存下推 SQL；返回 (items, total, 有效页码)——
    /// 有效页码按 total 截断，调用方响应用它而非请求值（评审 F2）。
    pub async fn get_todos_page_by_workspace(
        &self,
        workspace_id: Option<i64>,
        hours: Option<u32>,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<Todo>, i64, i64), sea_orm::DbErr> {
        let page = page.max(1);
        let page_size = page_size.clamp(1, 200);
        let mut cond = todos::Column::DeletedAt.is_null()
            .and(todos::Column::ArchivedAt.is_null());
        // 排除讨论载体 Todo（todo_type=4），不让 @触发的隐藏载体出现在 todo 列表。
        cond = cond.and(sea_orm::sea_query::Expr::cust("COALESCE(todo_type, 0) != 4"));
        if let Some(wid) = workspace_id {
            cond = cond.and(todos::Column::WorkspaceId.eq(wid));
        }
        if let Some(h) = hours.filter(|&h| h > 0) {
            cond = cond.and(sea_orm::sea_query::Expr::cust(format!(
                "REPLACE(REPLACE(updated_at, 'T', ' '), 'Z', '') >= datetime('now', '-{h} hours')"
            )));
        }
        // 先计数再钳页码（CodeRabbit#2）：大 OFFSET 是外部可触发的慢查询
        let total: i64 = todos::Entity::find()
            .filter(cond.clone())
            .count(&self.conn)
            .await?
            .try_into()
            .unwrap_or(i64::MAX);
        let max_page = if total == 0 { 1 } else { (total + page_size - 1) / page_size };
        let page = page.min(max_page.max(1));
        let models = todos::Entity::find()
            .filter(cond)
            .order_by_desc(todos::Column::UpdatedAt)
            .limit(page_size as u64)
            .offset(((page - 1) * page_size) as u64)
            .all(&self.conn)
            .await?;
        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;
        Ok((
            models
                .into_iter()
                .map(|m| {
                    let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                    Self::model_to_todo(m, tag_ids)
                })
                .collect(),
            total,
            page,
        ))
    }

    /// 云同步合并用（056）：只取 id+title 两列建 title→id 映射，替代整行全量拉取。
    ///
    /// 实现说明：用原生 SQL 而非 select_only()——SeaORM 的 Model 反序列化要求全列
    /// （CodeRabbit#1 复核确认缺列会在运行时 ColumnNotFound），两列投影必须走原生 SQL。
    pub async fn get_todo_title_id_map(
        &self,
    ) -> Result<std::collections::HashMap<String, i64>, sea_orm::DbErr> {
        let rows = self
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT id, title FROM todos WHERE deleted_at IS NULL".to_string(),
            ))
            .await?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                Some((
                    r.try_get_by::<String, _>("title").ok()?.trim().to_lowercase(),
                    r.try_get_by::<i64, _>("id").ok()?,
                ))
            })
            .collect())
    }

    /// 云同步上传用（056）：id 游标分批拉取，避免一次性全表载入。
    /// 传入 `after_id`（上一批最后一个 id，首批传 0），按 id ASC 取 limit 条。
    pub async fn get_todos_batch_after_id(
        &self,
        after_id: i64,
        limit: i64,
    ) -> Result<Vec<Todo>, sea_orm::DbErr> {
        let models = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .filter(todos::Column::Id.gt(after_id))
            .order_by_asc(todos::Column::Id)
            .limit(limit.max(1) as u64)
            .all(&self.conn)
            .await?;
        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;
        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                Self::model_to_todo(m, tag_ids)
            })
            .collect())
    }

    /// 取单个 todo 的 TodoCenterItem（archive/restore/webhook 后回传用）。
    ///
    /// 单条路径：用与列表同构的批量聚合接口取该 todo 的 loop 引用计数与最近执行，
    /// 再交给 `build_center_item` 组装，保证单条与列表口径完全一致。
    pub async fn get_todo_center_item(
        &self,
        id: i64,
    ) -> Result<Option<TodoCenterItem>, sea_orm::DbErr> {
        let Some(todo) = self.get_todo(id).await? else {
            return Ok(None);
        };
        let ids = vec![id];
        let aggs = self.build_center_aggregates(&ids).await?;
        Ok(Some(Self::build_center_item(todo, &aggs)))
    }

    /// 归档事项：设置 archived_at = 当前 UTC 时间。
    ///
    /// 仅改 archived_at，不动 deleted_at/scheduler_config/webhook_enabled/Loop 引用，
    /// 保证归档是「纯隐藏」语义，可随时恢复。
    pub async fn archive_todo(&self, id: i64) -> Result<bool, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let res = todos::Entity::update_many()
            .col_expr(todos::Column::ArchivedAt, Some(now).into())
            .filter(todos::Column::Id.eq(id))
            .filter(todos::Column::DeletedAt.is_null())
            .exec(&self.conn)
            .await?;
        // 受影响行数=1 表示该 todo 存在且未软删；=0 表示找不到
        Ok(res.rows_affected == 1)
    }

    /// 恢复事项：清空 archived_at。
    ///
    /// 恢复后分类由当前真实关系重新推导（见 get_todo_center_item 的 computed_bucket）。
    pub async fn restore_todo(&self, id: i64) -> Result<bool, sea_orm::DbErr> {
        // None::<String>.into() → Value::Null，显式清空列
        let res = todos::Entity::update_many()
            .col_expr(todos::Column::ArchivedAt, None::<String>.into())
            .filter(todos::Column::Id.eq(id))
            .filter(todos::Column::DeletedAt.is_null())
            .exec(&self.conn)
            .await?;
        Ok(res.rows_affected == 1)
    }

    /// 开关事件驱动：设置 webhook_enabled。
    ///
    /// 与 `PUT /api/todos/{id}/scheduler` 对称的扁平具名路由后端实现。
    /// 关闭后若不再有调度/Loop 引用，computed_bucket 自然回到手动触发。
    pub async fn update_todo_webhook(&self, id: i64, enabled: bool) -> Result<bool, sea_orm::DbErr> {
        let res = todos::Entity::update_many()
            .col_expr(todos::Column::WebhookEnabled, enabled.into())
            .filter(todos::Column::Id.eq(id))
            .filter(todos::Column::DeletedAt.is_null())
            .exec(&self.conn)
            .await?;
        Ok(res.rows_affected == 1)
    }

    /// 按 action_type + action_key + workspace_id 查找 todo。
    ///
    /// 用于黑板（Blackboard）等需要按工作空间隔离 Action 模板的场景。
    /// 每个工作空间可以有自己的 blackboard update todo。
    pub async fn get_todo_by_action_type_and_key_and_workspace(
        &self,
        action_type: &str,
        action_key: &str,
        workspace_id: i64,
    ) -> Result<Option<Todo>, sea_orm::DbErr> {
        use crate::db::entity::todos;
        let model = todos::Entity::find()
            .filter(todos::Column::ActionType.eq(action_type))
            .filter(todos::Column::ActionKey.eq(action_key))
            .filter(todos::Column::WorkspaceId.eq(workspace_id))
            .filter(todos::Column::DeletedAt.is_null())
            .one(&self.conn)
            .await?;

        Ok(model.map(|m| {
            let tag_ids = vec![]; // action template 不需要 tags
            Self::model_to_todo(m, tag_ids)
        }))
    }

    pub async fn create_todo(&self, title: &str, prompt: &str) -> Result<i64, sea_orm::DbErr> {
        // 从数据库读取默认执行器，不再使用硬编码常量；
        // 若数据库未配置默认执行器，get_default_executor_name 内部会回退到 claudecode。
        let default_executor = self.get_default_executor_name().await?;
        self.create_todo_with_executor(title, prompt, Some(&default_executor)).await
    }

    /// 创建 Todo，可指定执行器。
    /// executor 为 None、空串或仅空白时使用数据库配置的默认执行器（防止空/空白字符串污染 DB）。
    ///
    /// 注意：本方法保留 path-only 语义作为「最简 helper」专用入口——调用方（feishu_listener /
    /// sync.rs 等）已经决定好了 workspace，没有工作空间语义可推导时不应使用本方法。
    /// 业务 API 入口请用 `create_todo_with_extras`（强制 workspace_id + workspace_path 双字段）。
    pub async fn create_todo_with_executor(&self, title: &str, prompt: &str, executor: Option<&str>) -> Result<i64, sea_orm::DbErr> {
        self.create_todo_with_extras(title, prompt, executor, None, false, 0, "").await
    }

    /// 创建 Todo，带所有可选字段。
    /// 工作空间必填且必须存在：handler 在调用本方法前已经按 id 解析得到 path，
    /// 这里同时写入 workspace_id（筛选键）+ workspace_path（cwd）保证双字段同步。
    /// 参数数量由业务必填字段决定，无法进一步合并
    #[allow(clippy::too_many_arguments)]
    pub async fn create_todo_with_extras(
        &self,
        title: &str,
        prompt: &str,
        executor: Option<&str>,
        acceptance_criteria: Option<&str>,
        webhook_enabled: bool,
        workspace_id: i64,
        workspace_path: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // 先尝试使用调用方传入的 executor；为空则从数据库读取默认执行器。
        // get_default_executor_name 内部会回退到 DEFAULT_EXECUTOR 常量，确保不会为空。
        let executor_str = match executor.map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => s.to_string(),
            None => self.get_default_executor_name().await?,
        };
        let am = todos::ActiveModel {
            title: ActiveValue::Set(title.to_string()),
            prompt: ActiveValue::Set(Some(prompt.to_string())),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            executor: ActiveValue::Set(Some(executor_str)),
            acceptance_criteria: ActiveValue::Set(acceptance_criteria.map(|s| s.to_string())),
            webhook_enabled: ActiveValue::Set(Some(webhook_enabled)),
            auto_review_enabled: ActiveValue::Set(Some(false)),
            todo_type: ActiveValue::Set(Some(0)),
            workspace_id: ActiveValue::Set(Some(workspace_id)),
            workspace_path: ActiveValue::Set(Some(workspace_path.to_string())),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    pub async fn update_todo_full(&self, update: TodoUpdate<'_>) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let mut am = todos::ActiveModel {
            id: ActiveValue::Unchanged(update.id),
            title: ActiveValue::Set(update.title.to_string()),
            prompt: ActiveValue::Set(Some(update.prompt.to_string())),
            status: ActiveValue::Set(Some(update.status.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        if let Some(exec) = update.executor {
            am.executor = ActiveValue::Set(Some(exec.to_string()));
        }
        // model：Some(非空)=写入任务级模型，Some("")=清除，None=不改（与 executor 语义对齐）。
        if let Some(model) = update.model {
            am.model = ActiveValue::Set(if model.is_empty() {
                None
            } else {
                Some(model.to_string())
            });
        }
        if let Some(enabled) = update.scheduler_enabled {
            am.scheduler_enabled = ActiveValue::Set(Some(enabled));
        }
        if let Some(cfg) = update.scheduler_config {
            am.scheduler_config = ActiveValue::Set(Some(cfg.to_string()));
        }
        if let Some(tz) = update.scheduler_timezone {
            if tz.is_empty() {
                am.scheduler_timezone = ActiveValue::Set(None);
            } else {
                am.scheduler_timezone = ActiveValue::Set(Some(tz.to_string()));
            }
        }
        if let Some(wid) = update.workspace_id {
            // handler 必须把 id 解析为 path 后再传 workspace_path；
            // DAO 只接 id 写筛选列，path 由 ActiveValue::Unchanged 保持旧值。
            // 真正的 cwd 同步交给 handler 调用 update_todo_workspace 完成。
            am.workspace_id = ActiveValue::Set(Some(wid));
        }
        if let Some(webhook_enabled) = update.webhook_enabled {
            am.webhook_enabled = ActiveValue::Set(Some(webhook_enabled));
        }
        if let Some(criteria) = update.acceptance_criteria {
            am.acceptance_criteria = ActiveValue::Set(Some(criteria.to_string()));
        }
        if let Some(enabled) = update.auto_review_enabled {
            am.auto_review_enabled = ActiveValue::Set(Some(enabled));
        }
        if let Some(at) = update.action_type {
            am.action_type = ActiveValue::Set(Some(at.to_string()));
        }
        if let Some(ak) = update.action_key {
            am.action_key = ActiveValue::Set(Some(ak.to_string()));
        }
        if let Some(en) = update.expert_name {
            if en.is_empty() {
                am.expert_name = ActiveValue::Set(None);
            } else {
                am.expert_name = ActiveValue::Set(Some(en.to_string()));
            }
        }
        self.exec_update(am).await
    }

    pub async fn update_todo_executor(
        &self,
        id: i64,
        executor: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            executor: ActiveValue::Set(Some(executor.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 仅更新 todo 的 expert_name 与 model（工艺模板安装时使用）。
    ///
    /// `expert_name` / `model` 为 Some(非空)=写入，Some("")=清除，None=不修改。
    pub async fn update_todo_expert_and_model(
        &self,
        id: i64,
        expert_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let mut am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        if let Some(en) = expert_name {
            am.expert_name = ActiveValue::Set(if en.is_empty() { None } else { Some(en.to_string()) });
        }
        if let Some(m) = model {
            am.model = ActiveValue::Set(if m.is_empty() { None } else { Some(m.to_string()) });
        }
        self.exec_update(am).await
    }

    /// 仅更新 todo 的 skills（工艺模板安装时从环节 skills 写入，需求 055）。
    ///
    /// skills 以 JSON 数组串落库，与 `loop_steps.skill_names` 同构；
    /// 传空切片会写回 `"[]"`，即显式清空事项技能。
    pub async fn update_todo_skills(
        &self,
        id: i64,
        skills: &[String],
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // 序列化失败兜底 "[]"：技能名是纯字符串数组，理论上不会失败，
        // 这里与 installer 里 expected_artifacts 的序列化兜底口径保持一致。
        let skills_json = serde_json::to_string(skills).unwrap_or_else(|_| "[]".to_string());
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            skills: ActiveValue::Set(skills_json),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 批量更新事项执行器（单条 SQL，原子语义）。
    pub async fn batch_update_todos_executor(
        &self,
        ids: &[i64],
        executor: &str,
    ) -> Result<u64, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(0);
        }
        let now = crate::models::utc_timestamp();
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{}", i)).collect();
        let in_clause = placeholders.join(",");
        let executor_idx = ids.len() + 1;
        let now_idx = ids.len() + 2;
        let sql = format!(
            "UPDATE todos SET executor = ?{executor_idx}, updated_at = ?{now_idx} WHERE id IN ({in_clause})"
        );
        let mut vals: Vec<sea_orm::Value> = ids.iter().map(|id| (*id).into()).collect();
        vals.push(executor.to_string().into());
        vals.push(now.into());
        let stmt = sea_orm::Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite, sql, vals);
        let rows_affected = self.conn.execute(stmt).await?.rows_affected();
        Ok(rows_affected)
    }

    /// 批量暂停/恢复事项的周期执行（单条 SQL，原子语义）。
    /// scheduler_enabled 为 true 表示恢复调度，false 表示暂停调度；scheduler_config 保持不变。
    pub async fn batch_update_todos_scheduler(
        &self,
        ids: &[i64],
        scheduler_enabled: bool,
    ) -> Result<u64, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(0);
        }
        let now = crate::models::utc_timestamp();
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{}", i)).collect();
        let in_clause = placeholders.join(",");
        let enabled_idx = ids.len() + 1;
        let now_idx = ids.len() + 2;
        let sql = format!(
            "UPDATE todos SET scheduler_enabled = ?{enabled_idx}, updated_at = ?{now_idx} WHERE id IN ({in_clause})"
        );
        let mut vals: Vec<sea_orm::Value> = ids.iter().map(|id| (*id).into()).collect();
        vals.push((scheduler_enabled as i32).into()); // SQLite 用整数存储布尔值
        vals.push(now.into());
        let stmt = sea_orm::Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite, sql, vals);
        let rows_affected = self.conn.execute(stmt).await?.rows_affected();
        Ok(rows_affected)
    }

    pub async fn update_todo_task_id(
        &self,
        id: i64,
        task_id: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            task_id: ActiveValue::Set(task_id.map(|s| s.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn update_todo_scheduler(
        &self,
        req: SchedulerUpdate<'_>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // Normalize empty strings to None
        let timezone = req.timezone.filter(|s| !s.is_empty());
        let config = req.config.filter(|s| !s.is_empty());
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(req.id),
            scheduler_enabled: ActiveValue::Set(Some(req.enabled)),
            scheduler_config: ActiveValue::Set(config.map(|s| s.to_string())),
            scheduler_timezone: ActiveValue::Set(timezone.map(|s| s.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 批量更新事项工作空间（移动到其他工作空间）。
    /// 单条 SQL，原子语义：handler 已按 id 解析得到 path，DAO 一次写入 id + path。
    pub async fn batch_update_todos_workspace(
        &self,
        ids: &[i64],
        workspace_id: i64,
        workspace_path: &str,
    ) -> Result<u64, sea_orm::DbErr> {
        if ids.is_empty() || workspace_path.trim().is_empty() {
            return Ok(0);
        }
        let now = crate::models::utc_timestamp();
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{}", i)).collect();
        let in_clause = placeholders.join(",");
        let ws_id_idx = ids.len() + 1;
        let ws_path_idx = ids.len() + 2;
        let now_idx = ids.len() + 3;
        let sql = format!(
            "UPDATE todos SET workspace_id = ?{ws_id_idx}, workspace_path = ?{ws_path_idx}, updated_at = ?{now_idx} WHERE id IN ({in_clause})"
        );
        let mut vals: Vec<sea_orm::Value> = ids.iter().map(|id| (*id).into()).collect();
        vals.push(workspace_id.into());
        vals.push(workspace_path.trim().to_string().into());
        vals.push(now.into());
        let stmt = sea_orm::Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite, sql, vals);
        let rows_affected = self.conn.execute(stmt).await?.rows_affected();
        Ok(rows_affected)
    }

    /// 批量复制事项到目标工作空间。
    /// 读取源事项的完整数据，在目标工作空间下创建副本。
    /// 入参 id + path 由 handler 解析后传入，DAO 双字段写入保证同步。
    pub async fn batch_copy_todos_to_workspace(
        &self,
        ids: &[i64],
        target_workspace_id: i64,
        target_workspace_path: &str,
    ) -> Result<Vec<i64>, sea_orm::DbErr> {
        if ids.is_empty() || target_workspace_path.trim().is_empty() {
            return Ok(vec![]);
        }
        let now = crate::models::utc_timestamp();
        let ws = target_workspace_path.trim().to_string();
        let mut created_ids = Vec::new();

        for &id in ids {
            // 直接查询 sea-orm entity，获取原始模型
            let source_model = todos::Entity::find_by_id(id)
                .filter(todos::Column::DeletedAt.is_null())
                .one(&self.conn)
                .await?;
            if let Some(model) = source_model {
                let am = todos::ActiveModel {
                    title: ActiveValue::Set(model.title),
                    prompt: ActiveValue::Set(model.prompt),
                    status: ActiveValue::Set(model.status),
                    created_at: ActiveValue::Set(Some(now.clone())),
                    updated_at: ActiveValue::Set(Some(now.clone())),
                    executor: ActiveValue::Set(model.executor),
                    scheduler_enabled: ActiveValue::Set(model.scheduler_enabled),
                    scheduler_config: ActiveValue::Set(model.scheduler_config),
                    scheduler_timezone: ActiveValue::Set(model.scheduler_timezone),
                    workspace_id: ActiveValue::Set(Some(target_workspace_id)),
                    workspace_path: ActiveValue::Set(Some(ws.clone())),
                    webhook_enabled: ActiveValue::Set(model.webhook_enabled),
                    acceptance_criteria: ActiveValue::Set(model.acceptance_criteria),
                    auto_review_enabled: ActiveValue::Set(model.auto_review_enabled),
                    todo_type: ActiveValue::Set(model.todo_type),
                    task_id: ActiveValue::Set(None),
                    parent_todo_id: ActiveValue::Set(model.parent_todo_id),
                    review_template_id: ActiveValue::Set(model.review_template_id),
                    kind: ActiveValue::Set(model.kind),
                    // 跨工作空间复制时同步保留源事项的 model 字段（任务级模型覆盖）。
                    model: ActiveValue::Set(model.model),
                    ..Default::default()
                };
                let inserted = am.insert(&self.conn).await?;
                created_ids.push(inserted.id);
            }
        }
        Ok(created_ids)
    }

    /// 按 id 单独更新 todo 工作空间（含 cwd path 双字段同步）。
    /// id=None 表示「清空工作空间」：handler 解析用户意图后调用此方法，
    /// DAO 只负责把 id + path 同时写进 DB，不会做反查。
    pub async fn update_todo_workspace(
        &self,
        id: i64,
        workspace_id: Option<i64>,
        workspace_path: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let ws_path = workspace_path.and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            workspace_id: ActiveValue::Set(workspace_id),
            workspace_path: ActiveValue::Set(ws_path),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 单独更新 auto_review_enabled. 在 create_todo 之后被 handler 调用, 以接受
    /// 来自请求的覆盖. review_instance / reviewer_template 类型不允许改这个开关.
    pub async fn update_todo_auto_review_enabled(
        &self,
        id: i64,
        enabled: bool,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            auto_review_enabled: ActiveValue::Set(Some(enabled)),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 创建一个"评审实例" todo (todo_type=2)。
    /// 设计原因: V15 之后 review_template 是独立表 (不再挂 todo_type=1),
    /// 评审模板不再有 executor 字段。执行评审时需要新建一条 todo:
    /// - `prompt` = caller 合成好的评审 prompt (含原 output 截断 + 模板占位符替换)
    /// - `executor` = 从被评审的 record/original todo 继承 (review_template 不带 executor)
    /// - `todo_type` = 2 (评审实例)
    /// - `parent_todo_id` = 源 todo id (loop 触发时为 0, 因为 loop step 没有单一 source todo)
    /// - `review_template_id` = 使用的评审模板 id
    /// - `auto_review_enabled` = false (评审实例自身不再评审, 防止无限嵌套)
    ///   评审实例是 transient 的, 不挂 hooks / scheduler.
    pub async fn create_review_instance_todo(
        &self,
        parent_todo_id: i64,
        review_template_id: i64,
        review_template_name: &str,
        composed_prompt: String,
        executor: Option<String>,
        workspace_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let title = format!("[评审] {}", review_template_name);
        let am = todos::ActiveModel {
            title: ActiveValue::Set(title),
            // todos.prompt 列是 Option<String>; 新建的评审实例一定有 prompt, 直接 Some 包一层.
            prompt: ActiveValue::Set(Some(composed_prompt)),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            executor: ActiveValue::Set(executor),
            todo_type: ActiveValue::Set(Some(2)),
            parent_todo_id: ActiveValue::Set(Some(parent_todo_id)),
            review_template_id: ActiveValue::Set(Some(review_template_id)),
            auto_review_enabled: ActiveValue::Set(Some(false)),
            // 继承原 todo 的 workspace，使评审实例在原工作空间中可见
            workspace_id: ActiveValue::Set(Some(workspace_id)),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// 创建一个「异常处理」载体 todo（todo_type=3，需求 035）。
    ///
    /// 工艺安装时为 `abnormal_handler.prompt` 自动创建，作为运行时执行异常处理的容器，
    /// 让异常处理不再依赖工艺外的「手选 Todo」。模式与 todo_type=2 评审实例对称：
    /// - `prompt` = 工艺 abnormal_handler.prompt（运行时再注入异常上下文 + 占位符替换）
    /// - `todo_type` = 3
    /// - `auto_review_enabled` = false（异常处理本身不评审，防止无限嵌套）
    /// - `parent_todo_id` = 0（异常处理无单一源 todo，与评审实例 loop 触发一致）
    /// - `executor` = None（运行时落默认执行器）
    pub async fn create_abnormal_handler_todo(
        &self,
        title: String,
        prompt: String,
        workspace_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            title: ActiveValue::Set(title),
            // todos.prompt 列是 Option<String>；载体 todo 一定有 prompt（= 工艺 prompt）
            prompt: ActiveValue::Set(Some(prompt)),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            todo_type: ActiveValue::Set(Some(TODO_TYPE_ABNORMAL_HANDLER)),
            parent_todo_id: ActiveValue::Set(Some(0)),
            auto_review_enabled: ActiveValue::Set(Some(false)),
            // 继承工作空间，使异常处理载体 todo 在对应工作空间可见
            workspace_id: ActiveValue::Set(Some(workspace_id)),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// 创建任务讨论区的「@触发执行」载体 Todo（需求 060）。
    ///
    /// 执行系统是 Todo 中心的：`run_todo_execution` 必须有一个 Todo 提供
    /// executor/prompt/expert_name。讨论帖里 @专家/@执行器 时用本方法建一个
    /// `todo_type=DISCUSSION` 的隐藏载体，承载这次执行；返回 todo id。
    /// `executor` 为 None 时由执行管线的 `resolve_executor_type` 回退默认执行器。
    pub async fn create_discussion_todo(
        &self,
        title: String,
        prompt: String,
        executor: Option<&str>,
        expert_name: Option<&str>,
        workspace_id: i64,
        workspace_path: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // executor 缺省时取数据库默认执行器（内部再回退 DEFAULT_EXECUTOR，保证非空）。
        let executor_str = match executor.map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => s.to_string(),
            None => self.get_default_executor_name().await?,
        };
        let am = todos::ActiveModel {
            title: ActiveValue::Set(title),
            // 载体 todo 的 prompt = 帖子正文 + 任务上下文（由 handler 拼装）。
            prompt: ActiveValue::Set(Some(prompt)),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            executor: ActiveValue::Set(Some(executor_str)),
            // 讨论帖触发的执行不进自动评审（它是一次性问答，不是工艺产物）。
            auto_review_enabled: ActiveValue::Set(Some(false)),
            todo_type: ActiveValue::Set(Some(TODO_TYPE_DISCUSSION)),
            workspace_id: ActiveValue::Set(Some(workspace_id)),
            workspace_path: ActiveValue::Set(Some(workspace_path.to_string())),
            // @专家 时把专家名存到 todo，inject_expert_context 会据此注入人设。
            expert_name: ActiveValue::Set(expert_name.map(|s| s.to_string())),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// 软删一个 todo（置 deleted_at）。讨论载体 todo 执行完成后由回写逻辑调用，
    /// 让所有 `deleted_at IS NULL` 的查询自动排除它（事项中心 / 列表 / 计数兜底）。
    pub async fn soft_delete_todo(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        let existing = todos::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(m) = existing {
            let mut am: todos::ActiveModel = m.into();
            am.deleted_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// 更新指定 todo 的 prompt（需求 035：工艺升级时刷新异常处理载体 todo 的 prompt）。
    ///
    /// 仅当 todo 存在时更新；不存在返回 Err，由调用方决定是否新建。
    pub async fn update_todo_prompt(
        &self,
        id: i64,
        prompt: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = todos::Entity::find_by_id(id).one(&self.conn).await?;
        let Some(m) = existing else {
            return Err(sea_orm::DbErr::Custom(format!("todo {id} not found")));
        };
        let mut am: todos::ActiveModel = m.into();
        am.prompt = ActiveValue::Set(Some(prompt.to_string()));
        am.updated_at = ActiveValue::Set(Some(now));
        am.update(&self.conn).await?;
        Ok(())
    }

    /// 根据 review_template_id 查找一条未删除的评审实例 todo (todo_type=2)。
    ///
    /// 复用语义：同一评审模板的所有评审执行共享同一条评审实例 todo,
    /// 避免 todos 表被「同一模板 N 次评审 → N 条 todo」刷屏。
    /// 多条匹配时返回 id 最大（最新创建）的那条，
    /// 保证 V17 数据清理前老数据也能被定位到。
    pub async fn find_review_instance_by_template(
        &self,
        review_template_id: i64,
    ) -> Result<Option<todos::Model>, sea_orm::DbErr> {
        todos::Entity::find()
            .filter(todos::Column::TodoType.eq(2_i32))
            .filter(todos::Column::ReviewTemplateId.eq(review_template_id))
            .filter(todos::Column::DeletedAt.is_null())
            .order_by_desc(todos::Column::Id)
            .one(&self.conn)
            .await
    }

    /// 复用现有评审实例 todo:重置 prompt/executor/status/updated_at,
    /// 保留 todo id 和 execution_records 关联(历史 record 仍可见)。
    ///
    /// 调用方负责先调 `find_review_instance_by_template` 拿到 id;
    /// 找不到时不要调本方法,应改走 `create_review_instance_todo`。
    pub async fn reset_review_instance_for_reuse(
        &self,
        id: i64,
        new_prompt: &str,
        new_executor: Option<&str>,
        workspace_id: i64,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            prompt: ActiveValue::Set(Some(new_prompt.to_string())),
            executor: ActiveValue::Set(new_executor.map(|s| s.to_string())),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            // 复用场景下 source todo 可能在不同 workspace；同步更新使评审实例跟随当前 source
            workspace_id: ActiveValue::Set(Some(workspace_id)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn force_update_todo_status(
        &self,
        id: i64,
        status: TodoStatus,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            status: ActiveValue::Set(Some(status.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn delete_todo(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            deleted_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 批量软删除事项（返回成功删除数）。
    /// 注意：调用方应在 handler 层校验每个 todo 的可删除性（引用校验等）。
    pub async fn batch_delete_todos(&self, ids: &[i64]) -> Result<u64, sea_orm::DbErr> {
        if ids.is_empty() { return Ok(0); }
        let now = crate::models::utc_timestamp();
        let res = todos::Entity::update_many()
            .col_expr(todos::Column::DeletedAt, Some(now).into())
            .filter(todos::Column::Id.is_in(ids.to_vec()))
            .filter(todos::Column::DeletedAt.is_null())
            .exec(&self.conn)
            .await?;
        Ok(res.rows_affected)
    }

    pub async fn get_todo(&self, id: i64) -> Result<Option<Todo>, sea_orm::DbErr> {
        let model = match todos::Entity::find_by_id(id)
            .filter(todos::Column::DeletedAt.is_null())
            .one(&self.conn)
            .await?
        {
            Some(m) => m,
            None => return Ok(None),
        };
        let tag_ids = todo_tags::Entity::find()
            .filter(todo_tags::Column::TodoId.eq(id))
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| t.tag_id)
            .collect();
        Ok(Some(Self::model_to_todo(model, tag_ids)))
    }

    pub async fn get_scheduler_todos(&self, workspace_id: Option<i64>) -> Result<Vec<Todo>, sea_orm::DbErr> {
        let mut find = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .filter(todos::Column::SchedulerEnabled.eq(true))
            .filter(todos::Column::SchedulerConfig.is_not_null());
        if let Some(wid) = workspace_id {
            find = find.filter(todos::Column::WorkspaceId.eq(wid));
        }
        let models = find.all(&self.conn).await?;

        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;

        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                Self::model_to_todo(m, tag_ids)
            })
            .collect())
    }

    /// 检查是否存在正在运行的 todo（status = "running" 且 task_id 非空）。
    /// 用于自动更新前判断是否可以安全执行升级。
    pub async fn has_running_todos(&self) -> Result<bool, sea_orm::DbErr> {
        use sea_orm::PaginatorTrait;
        let count = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .filter(todos::Column::Status.eq(TodoStatus::Running.to_string()))
            .filter(todos::Column::TaskId.is_not_null())
            .count(&self.conn)
            .await?;
        Ok(count > 0)
    }

    pub async fn get_running_todos(&self) -> Result<Vec<Todo>, sea_orm::DbErr> {
        let models = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .filter(todos::Column::Status.eq(TodoStatus::Running.to_string()))
            .filter(todos::Column::TaskId.is_not_null())
            .all(&self.conn)
            .await?;

        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;

        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                Self::model_to_todo(m, tag_ids)
            })
            .collect())
    }

    pub async fn update_todo_status(
        &self,
        todo_id: i64,
        status: TodoStatus,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(todo_id),
            status: ActiveValue::Set(Some(status.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn start_todo_execution(
        &self,
        todo_id: i64,
        task_id: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(todo_id),
            status: ActiveValue::Set(Some(TodoStatus::Running.to_string())),
            task_id: ActiveValue::Set(Some(task_id.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn finish_todo_execution(
        &self,
        todo_id: i64,
        success: bool,
    ) -> Result<(), sea_orm::DbErr> {
        if todo_id == 0 { return Ok(()); } // 环节独立执行
        let status = if success {
            TodoStatus::Completed
        } else {
            TodoStatus::Failed
        };
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(todo_id),
            status: ActiveValue::Set(Some(status.to_string())),
            task_id: ActiveValue::Set(None),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 获取所有 todo 的备份数据（非软删除），包含标签名称
    pub async fn get_todo_backups(&self) -> Result<Vec<TodoBackup>, sea_orm::DbErr> {
        let models = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .all(&self.conn)
            .await?;

        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;

        // 获取所有标签 id -> name 映射
        let all_tags: std::collections::HashMap<i64, String> = tags::Entity::find()
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| (t.id, t.name))
            .collect();

        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                let tag_names: Vec<String> = tag_ids
                    .iter()
                    .filter_map(|tid| all_tags.get(tid).cloned())
                    .collect();
                TodoBackup {
                    title: m.title,
                    prompt: m.prompt.unwrap_or_default(),
                    status: m
                        .status
                        .as_deref()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(TodoStatus::Pending),
                    executor: m.executor,
                    scheduler_enabled: m.scheduler_enabled.unwrap_or(false),
                    scheduler_config: m.scheduler_config,
                    tag_names,
                    workspace_path: m.workspace_path.clone(),
                    worktree: None,
                    action_type: m.action_type,
                    action_key: m.action_key,
                    // 备份时保留任务级模型，导入时恢复。
                    model: m.model.clone(),
                    // 备份时保留工作空间 ID，导入时用于关联到正确的工作空间
                    workspace_id: m.workspace_id,
                }
            })
            .collect::<Vec<_>>())
    }

    /// 按 ID 列表获取 todo 的备份数据
    pub async fn get_todo_backups_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<Vec<TodoBackup>, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let models = todos::Entity::find()
            .filter(todos::Column::Id.is_in(ids.to_vec()))
            .filter(todos::Column::DeletedAt.is_null())
            .all(&self.conn)
            .await?;

        let model_ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&model_ids).await?;

        let all_tags: std::collections::HashMap<i64, String> = tags::Entity::find()
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| (t.id, t.name))
            .collect();

        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                let tag_names: Vec<String> = tag_ids
                    .iter()
                    .filter_map(|tid| all_tags.get(tid).cloned())
                    .collect();
                TodoBackup {
                    title: m.title,
                    prompt: m.prompt.unwrap_or_default(),
                    status: m
                        .status
                        .as_deref()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(TodoStatus::Pending),
                    executor: m.executor,
                    scheduler_enabled: m.scheduler_enabled.unwrap_or(false),
                    scheduler_config: m.scheduler_config,
                    tag_names,
                    workspace_path: m.workspace_path.clone(),
                    worktree: None,
                    action_type: m.action_type,
                    action_key: m.action_key,
                    model: m.model.clone(),
                    // 备份时保留工作空间 ID，导入时用于关联到正确的工作空间
                    workspace_id: m.workspace_id,
                }
            })
            .collect())
    }

    /// 按 tag name 列表查询 tag 备份数据
    pub async fn get_tag_backups_by_names(
        &self,
        names: &[&str],
    ) -> Result<Vec<crate::models::TagBackup>, sea_orm::DbErr> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        Ok(tags::Entity::find()
            .filter(
                // is_in 接受 IntoIterator，无需 collect() 为 Vec
                tags::Column::Name.is_in(names.iter().map(|s| s.to_string())),
            )
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| crate::models::TagBackup {
                name: t.name,
                color: t.color.unwrap_or_default(),
            })
            .collect())
    }

    /// 从备份数据导入 todo（清空现有数据后导入，失败时自动回滚）
    pub async fn import_backup(
        &self,
        tags_in: &[crate::models::TagBackup],
        todos_in: &[TodoBackup],
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::QueryFilter;
        use sea_orm::TransactionTrait;

        let txn = self.conn.begin().await?;

        // 清空现有数据
        todo_tags::Entity::delete_many().exec(&txn).await?;
        todos::Entity::delete_many().exec(&txn).await?;
        tags::Entity::delete_many().exec(&txn).await?;

        // 导入标签
        for tag in tags_in {
            let am = crate::db::entity::tags::ActiveModel {
                name: ActiveValue::Set(tag.name.clone()),
                color: ActiveValue::Set(Some(tag.color.clone())),
                ..Default::default()
            };
            am.insert(&txn).await?;
        }

        // 导入 todo
        for todo in todos_in {
            let now = crate::models::utc_timestamp();
            let workspace_path = todo.workspace_path.clone();
            let am = todos::ActiveModel {
                title: ActiveValue::Set(todo.title.clone()),
                prompt: ActiveValue::Set(Some(todo.prompt.clone())),
                status: ActiveValue::Set(Some(todo.status.to_string())),
                executor: ActiveValue::Set(todo.executor.clone()),
                scheduler_enabled: ActiveValue::Set(Some(todo.scheduler_enabled)),
                scheduler_config: ActiveValue::Set(todo.scheduler_config.clone()),
                workspace_path: ActiveValue::Set(workspace_path),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now)),
                action_type: ActiveValue::Set(todo.action_type.clone()),
                action_key: ActiveValue::Set(todo.action_key.clone()),
                model: ActiveValue::Set(todo.model.clone()),
                ..Default::default()
            };
            let inserted = am.insert(&txn).await?;

            // 关联标签（通过名称查找 tag id）
            for tag_name in &todo.tag_names {
                let tid = tags::Entity::find()
                    .filter(tags::Column::Name.eq(tag_name))
                    .one(&txn)
                    .await?
                    .map(|t| t.id);
                if let Some(tid) = tid {
                    let rel = todo_tags::ActiveModel {
                        todo_id: ActiveValue::Set(inserted.id),
                        tag_id: ActiveValue::Set(tid),
                    };
                    todo_tags::Entity::insert(rel)
                        .on_conflict(
                            sea_orm::sea_query::OnConflict::columns([
                                todo_tags::Column::TodoId,
                                todo_tags::Column::TagId,
                            ])
                            .do_nothing()
                            .to_owned(),
                        )
                        .exec(&txn)
                        .await?;
                }
            }
        }

        txn.commit().await?;
        Ok(())
    }

    /// 智能合并导入：不删除现有数据，按 title+prompt 匹配进行覆盖或新建
    /// workspace_id：可选目标工作空间 ID，指定后覆盖备份数据中的 workspace_id
    pub async fn merge_backup(
        &self,
        tags_in: &[crate::models::TagBackup],
        todos_in: &[TodoBackup],
        target_workspace_id: Option<i64>,
    ) -> Result<(u64, u64), sea_orm::DbErr> {
        use sea_orm::TransactionTrait;

        let txn = self.conn.begin().await?;

        // 预解析目标工作空间的 (id, path) 对：handler 已校验存在性，这里在事务内再查一次拿 path，
        // 保证 workspace_id 与 workspace_path 成对写入，避免「id 指向 B、path 仍是备份源路径」的错配。
        // 查不到则降级为 None，不强行写悬空 id。
        let target_ws: Option<(i64, String)> = match target_workspace_id {
            Some(id) => resolve_workspace_pair(&txn, id).await?,
            None => None,
        };

        // 确保所有 tag 都存在（不存在则创建），并构建 name -> id 映射
        let mut tag_name_map: std::collections::HashMap<String, i64> = tags::Entity::find()
            .all(&txn)
            .await?
            .into_iter()
            .map(|t| (t.name, t.id))
            .collect();

        for tag in tags_in {
            if !tag_name_map.contains_key(&tag.name) {
                let now = crate::models::utc_timestamp();
                let am = tags::ActiveModel {
                    name: ActiveValue::Set(tag.name.clone()),
                    color: ActiveValue::Set(Some(tag.color.clone())),
                    created_at: ActiveValue::Set(Some(now)),
                    ..Default::default()
                };
                let inserted = am.insert(&txn).await?;
                tag_name_map.insert(tag.name.clone(), inserted.id);
            }
        }

        let mut created: u64 = 0;
        let mut updated: u64 = 0;

        for todo in todos_in {
            // 解析本条 todo 最终归属工作空间的 (id, path)，必须成对写入，避免 id/path 错配：
            // 1) 目标工作空间优先（已预解析，path 来自当前库而非备份，跨环境更可靠）
            // 2) 否则用备份里的 workspace_id，但必须在当前库重新解析 path——跨环境导入时备份
            //    id 可能在当前库不存在，此时降级为「未分配」哨兵 (0, None)：workspace_id 列是
            //    NOT NULL DEFAULT 0，0 表示未分配（与 loop_runner.rs 的语义一致），不写悬空 id。
            // resolved_id 同时用于限定「覆盖」匹配范围（只在同一工作空间内 title+prompt 匹配，
            // 避免跨工作空间抢占同名 todo）；为 0 时只匹配其它未分配 todo。
            let (resolved_id, resolved_path): (i64, Option<String>) = if let Some((id, path)) = &target_ws {
                (*id, Some(path.clone()))
            } else {
                match todo.workspace_id {
                    Some(id) => match resolve_workspace_pair(&txn, id).await? {
                        Some((rid, rpath)) => (rid, Some(rpath)),
                        None => (0, None),
                    },
                    None => (0, None),
                }
            };
            let existing = todos::Entity::find()
                .filter(todos::Column::Title.eq(&todo.title))
                .filter(todos::Column::Prompt.eq(&todo.prompt))
                .filter(todos::Column::DeletedAt.is_null())
                .filter(todos::Column::WorkspaceId.eq(resolved_id))
                .one(&txn)
                .await?;

            if let Some(model) = existing {
                // 覆盖：更新字段
                let mut am: todos::ActiveModel = model.into();
                am.status = ActiveValue::Set(Some(todo.status.to_string()));
                am.executor = ActiveValue::Set(todo.executor.clone());
                am.scheduler_enabled = ActiveValue::Set(Some(todo.scheduler_enabled));
                am.scheduler_config = ActiveValue::Set(todo.scheduler_config.clone());
                // workspace_id 与 workspace_path 成对写入解析出的目标对，避免 id/path 错配
                am.workspace_id = ActiveValue::Set(Some(resolved_id));
                am.workspace_path = ActiveValue::Set(resolved_path);
                am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
                am.action_type = ActiveValue::Set(todo.action_type.clone());
                am.action_key = ActiveValue::Set(todo.action_key.clone());
                am.model = ActiveValue::Set(todo.model.clone());
                let saved = am.update(&txn).await?;

                // 重建 tag 关联
                todo_tags::Entity::delete_many()
                    .filter(todo_tags::Column::TodoId.eq(saved.id))
                    .exec(&txn)
                    .await?;
                for tag_name in &todo.tag_names {
                    if let Some(&tid) = tag_name_map.get(tag_name) {
                        let rel = todo_tags::ActiveModel {
                            todo_id: ActiveValue::Set(saved.id),
                            tag_id: ActiveValue::Set(tid),
                        };
                        todo_tags::Entity::insert(rel)
                            .on_conflict(
                                sea_orm::sea_query::OnConflict::columns([
                                    todo_tags::Column::TodoId,
                                    todo_tags::Column::TagId,
                                ])
                                .do_nothing()
                                .to_owned(),
                            )
                            .exec(&txn)
                            .await?;
                    }
                }
                updated += 1;
            } else {
                // 新建
                let now = crate::models::utc_timestamp();
                // workspace_id / workspace_path 用上面解析出的成对值（目标 > 当前库校验过的备份值 > 未分配 0）
                let am = todos::ActiveModel {
                    title: ActiveValue::Set(todo.title.clone()),
                    prompt: ActiveValue::Set(Some(todo.prompt.clone())),
                    status: ActiveValue::Set(Some(todo.status.to_string())),
                    executor: ActiveValue::Set(todo.executor.clone()),
                    scheduler_enabled: ActiveValue::Set(Some(todo.scheduler_enabled)),
                    scheduler_config: ActiveValue::Set(todo.scheduler_config.clone()),
                    workspace_path: ActiveValue::Set(resolved_path),
                    workspace_id: ActiveValue::Set(Some(resolved_id)),
                    created_at: ActiveValue::Set(Some(now.clone())),
                    updated_at: ActiveValue::Set(Some(now)),
                    action_type: ActiveValue::Set(todo.action_type.clone()),
                    action_key: ActiveValue::Set(todo.action_key.clone()),
                    model: ActiveValue::Set(todo.model.clone()),
                    ..Default::default()
                };
                let inserted = am.insert(&txn).await?;

                for tag_name in &todo.tag_names {
                    if let Some(&tid) = tag_name_map.get(tag_name) {
                        let rel = todo_tags::ActiveModel {
                            todo_id: ActiveValue::Set(inserted.id),
                            tag_id: ActiveValue::Set(tid),
                        };
                        todo_tags::Entity::insert(rel)
                            .on_conflict(
                                sea_orm::sea_query::OnConflict::columns([
                                    todo_tags::Column::TodoId,
                                    todo_tags::Column::TagId,
                                ])
                                .do_nothing()
                                .to_owned(),
                            )
                            .exec(&txn)
                            .await?;
                    }
                }
                created += 1;
            }
        }

        txn.commit().await?;
        Ok((created, updated))
    }

    pub async fn get_recent_completed_todos(
        &self,
        hours: u32,
        workspace_id: Option<i64>,
    ) -> Result<Vec<crate::models::RecentCompletedTodo>, sea_orm::DbErr> {
        let backend = self.conn.get_database_backend();
        let time_filter = format!("datetime('now', '-{} hours')", hours);

        let mut conditions = vec![
            "t.deleted_at IS NULL".to_string(),
            "t.status IN ('completed', 'failed')".to_string(),
            format!("REPLACE(REPLACE(er.finished_at, 'T', ' '), 'Z', '') >= {}", time_filter),
        ];
        // 按 workspace_id 过滤：仅显示该工作空间下的事项
        if let Some(wid) = workspace_id {
            conditions.push(format!("t.workspace_id = {}", wid));
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            "SELECT t.id as todo_id, t.title, t.prompt, t.executor, t.workspace_id, \
             er.status as execution_status, er.finished_at, er.result, er.model, er.usage, \
             er.trigger_type, er.id as record_id, er.rating \
             FROM todos t \
             JOIN execution_records er ON er.id = ( \
                 SELECT er2.id FROM execution_records er2 \
                 WHERE er2.todo_id = t.id \
                 ORDER BY er2.finished_at DESC LIMIT 1 \
             ) \
             WHERE {} \
             ORDER BY er.finished_at DESC",
            where_clause
        );

        let rows = self
            .conn
            .query_all(Statement::from_string(backend, sql))
            .await?;

        let todo_ids: Vec<i64> = rows
            .iter()
            .filter_map(|r| r.try_get_by("todo_id").ok())
            .collect();
        let tag_map = self.fetch_tag_ids_for_many(&todo_ids).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let todo_id: i64 = row.try_get_by("todo_id").ok()?;
                let title: String = row.try_get_by("title").ok()?;
                let executor: Option<String> = row.try_get_by("executor").ok().flatten();
                let completed_at: String =
                    row.try_get_by("finished_at").ok().flatten().unwrap_or_default();
                let result: Option<String> = row.try_get_by("result").ok().flatten();
                let model: Option<String> = row.try_get_by("model").ok().flatten();
                let usage: Option<String> = row.try_get_by("usage").ok().flatten();
                let trigger_type: String =
                    row.try_get_by("trigger_type").ok().flatten().unwrap_or_default();
                let execution_status: String =
                    row.try_get_by("execution_status").ok().flatten().unwrap_or_default();
                let prompt: Option<String> = row.try_get_by("prompt").ok().flatten();
                let record_id: i64 = row.try_get_by("record_id").ok()?;

                let usage: Option<crate::models::ExecutionUsage> =
                    usage.and_then(|u| serde_json::from_str(&u).ok());

                Some(crate::models::RecentCompletedTodo {
                    todo_id,
                    title,
                    prompt,
                    executor,
                    tag_ids: tag_map.get(&todo_id).cloned().unwrap_or_default(),
                    workspace_id: row.try_get_by("workspace_id").ok().flatten(),
                    completed_at,
                    result,
                    model,
                    usage,
                    execution_status,
                    trigger_type,
                    record_id,
                    rating: row.try_get_by("rating").ok().flatten(),
                })
            })
            .collect())
    }

}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod review_instance_reuse_tests {
    //! 评审实例 todo 复用逻辑的单元测试。
    //!
    //! 关注三个新方法:`create_review_instance_todo` /
    //! `find_review_instance_by_template` / `reset_review_instance_for_reuse`。
    //! 每次评审运行共享同一条 todo(todo_type=2, review_template_id=N),
    //! 避免 todos 表被「同模板 N 次评审 → N 条 todo」刷屏。

    use super::*;
    use crate::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    async fn seed_template(db: &Database, name: &str) -> i64 {
        // review_templates 表有 review_template_id, 直接插一条确保模板存在
        use sea_orm::{ActiveModelTrait, Set};
        let now = crate::models::utc_timestamp();
        let am = crate::db::entity::review_templates::ActiveModel {
            name: Set(name.to_string()),
            description: Set(None),
            prompt: Set(format!("{name} prompt")),
            created_at: Set(Some(now.clone())),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        let inserted = am.insert(&db.conn).await.expect("insert template");
        inserted.id
    }

    // -------- find_review_instance_by_template --------

    #[tokio::test]
    async fn find_review_instance_by_template_returns_existing() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "默认评审").await;
        let first_id = db
            .create_review_instance_todo(0, template_id, "默认评审", "p1".into(), None, 0)
            .await
            .expect("create first");
        let second_id = db
            .create_review_instance_todo(0, template_id, "默认评审", "p2".into(), None, 0)
            .await
            .expect("create second");
        // 多条匹配 → 返回最新 (id 大的那条)
        let found = db
            .find_review_instance_by_template(template_id)
            .await
            .expect("find");
        assert!(found.is_some(), "must find a review instance");
        let found = found.unwrap();
        assert_eq!(found.id, second_id, "newest by id wins");
        assert_ne!(first_id, second_id);
        assert_eq!(found.review_template_id, Some(template_id));
        assert_eq!(found.todo_type, Some(2));
    }

    #[tokio::test]
    async fn find_review_instance_by_template_returns_none_when_absent() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "未使用").await;
        let found = db
            .find_review_instance_by_template(template_id)
            .await
            .expect("find");
        assert!(found.is_none(), "no review instance yet");
    }

    #[tokio::test]
    async fn find_review_instance_by_template_excludes_deleted() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "X").await;
        let id = db
            .create_review_instance_todo(0, template_id, "X", "p".into(), None, 0)
            .await
            .expect("create");
        // 软删除
        use sea_orm::{ActiveModelTrait, Set};
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: Set(id),
            deleted_at: Set(Some(now)),
            ..Default::default()
        };
        am.update(&db.conn).await.expect("soft delete");
        let found = db
            .find_review_instance_by_template(template_id)
            .await
            .expect("find");
        assert!(found.is_none(), "soft-deleted must be excluded");
    }

    #[tokio::test]
    async fn find_review_instance_by_template_isolates_by_template_id() {
        let db = fresh_db().await;
        let t1 = seed_template(&db, "T1").await;
        let t2 = seed_template(&db, "T2").await;
        db.create_review_instance_todo(0, t1, "T1", "p".into(), None, 0).await.expect("c1");
        db.create_review_instance_todo(0, t2, "T2", "p".into(), None, 0).await.expect("c2");
        let f1 = db.find_review_instance_by_template(t1).await.expect("f1");
        let f2 = db.find_review_instance_by_template(t2).await.expect("f2");
        assert_eq!(f1.unwrap().review_template_id, Some(t1));
        assert_eq!(f2.unwrap().review_template_id, Some(t2));
    }

    // -------- reset_review_instance_for_reuse --------

    #[tokio::test]
    async fn reset_review_instance_for_reuse_updates_prompt_status_executor() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "R").await;
        let id = db
            .create_review_instance_todo(0, template_id, "R", "old-prompt".into(), Some("claude".to_string()), 0)
            .await
            .expect("create");
        db.reset_review_instance_for_reuse(id, "new-prompt", Some("pi"), 0)
            .await
            .expect("reset");
        let found = db
            .find_review_instance_by_template(template_id)
            .await
            .expect("find")
            .expect("must exist");
        assert_eq!(found.id, id, "id preserved");
        assert_eq!(found.prompt.as_deref(), Some("new-prompt"));
        assert_eq!(found.executor.as_deref(), Some("pi"));
        assert_eq!(found.status.as_deref(), Some("pending"), "reset to pending");
        assert_eq!(found.review_template_id, Some(template_id));
        assert_eq!(found.todo_type, Some(2));
    }

    #[tokio::test]
    async fn reset_review_instance_for_reuse_allows_executor_to_become_none() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "N").await;
        let id = db
            .create_review_instance_todo(0, template_id, "N", "p".into(), Some("claude".to_string()), 0)
            .await
            .expect("create");
        db.reset_review_instance_for_reuse(id, "p2", None, 0)
            .await
            .expect("reset");
        let found = db.find_review_instance_by_template(template_id).await.expect("find").unwrap();
        assert!(found.executor.is_none(), "executor must clear to None");
    }
}

/// 事项中心（computed_bucket / archive / restore / webhook）DAO 测试。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark, clippy::too_many_lines)]
mod todo_center_tests {
    use super::*;
    use crate::db::LatestExecutionSummary;
    use crate::db::Database;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    /// 056 评审 F2 修复后：`get_todo_center_page` 返回 TodoCenterPageData struct。
    /// 展开为五元组，保持既有测试的解构写法稳定，同时暴露有效页码供断言。
    fn center_tuple(
        d: crate::db::TodoCenterPageData,
    ) -> (Vec<TodoCenterItem>, i64, i64, std::collections::HashMap<String, i64>, Vec<String>) {
        (d.items, d.total, d.page, d.bucket_counts, d.action_types)
    }

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 用最小列集插一条 todo，返回其 id。
    /// 直接 SQL 而非走 create_todo_with_extras，避免拉起 workspace/executor 依赖。
    async fn seed_todo(db: &Database, title: &str) -> i64 {
        db.exec(&format!(
            "INSERT INTO todos (title, prompt, status) VALUES ('{title}', 'p', 'pending')"
        ))
        .await
        .expect("insert todo");
        let row = db
            .conn
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!("SELECT id FROM todos WHERE title = '{title}'"),
            ))
            .await
            .expect("query id")
            .expect("row exists");
        row.try_get_by_index::<i64>(0).expect("id readable")
    }

    /// 日常视图（get_todos_page_by_workspace）不返回已归档事项。
    /// 这是归档「从日常视图隐藏」语义的落地点。
    #[tokio::test]
    async fn test_get_todos_by_workspace_id_excludes_archived() {
        let db = fresh_db().await;
        let id = seed_todo(&db, "归档项").await;
        let (before, _, _) = db.get_todos_page_by_workspace(None, None, 1, 200).await.unwrap();
        assert_eq!(before.len(), 1, "归档前应在日常视图可见");

        assert!(db.archive_todo(id).await.unwrap(), "archive 应命中一行");
        let (after, _, _) = db.get_todos_page_by_workspace(None, None, 1, 200).await.unwrap();
        assert!(after.is_empty(), "归档后日常视图应隐藏该事项");
    }

    /// archive / restore 往返：archived_at 从 None → Some → None。
    #[tokio::test]
    async fn test_archive_restore_roundtrip() {
        let db = fresh_db().await;
        let id = seed_todo(&db, "可恢复").await;

        assert!(db.archive_todo(id).await.unwrap());
        let archived = db.get_todo(id).await.unwrap().unwrap();
        assert!(archived.archived_at.is_some(), "归档后 archived_at 应非空");

        assert!(db.restore_todo(id).await.unwrap());
        let restored = db.get_todo(id).await.unwrap().unwrap();
        assert!(restored.archived_at.is_none(), "恢复后 archived_at 应清空");
    }

    /// archive 命中不存在的 id 返回 false（不报错），handler 据此回 404。
    #[tokio::test]
    async fn test_archive_missing_id_returns_false() {
        let db = fresh_db().await;
        assert!(!db.archive_todo(99999).await.unwrap());
        assert!(!db.restore_todo(99999).await.unwrap());
        assert!(!db.update_todo_webhook(99999, true).await.unwrap());
    }

    /// update_todo_webhook 切换 webhook_enabled，影响事件驱动分桶。
    #[tokio::test]
    async fn test_update_todo_webhook_toggles() {
        let db = fresh_db().await;
        let id = seed_todo(&db, "事件项").await;
        assert!(db.update_todo_webhook(id, true).await.unwrap());
        assert!(db.get_todo(id).await.unwrap().unwrap().webhook_enabled);
        assert!(db.update_todo_webhook(id, false).await.unwrap());
        assert!(!db.get_todo(id).await.unwrap().unwrap().webhook_enabled);
    }

    /// get_todo_center 把普通事项分到 Manual，已归档事项分到 Archived，
    /// 且 bucket 过滤生效。
    #[tokio::test]
    async fn test_get_todo_center_manual_and_archived_buckets() {
        let db = fresh_db().await;
        let manual_id = seed_todo(&db, "手动").await;
        let archived_id = seed_todo(&db, "已归档").await;
        db.archive_todo(archived_id).await.unwrap();

        // 不过滤：应同时含两类
        let (all, _, _, _, _) = db.get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None, bucket: None, search: None, status: None, action_type: None, sort_by: None, sort_desc: true, page: 1, page_size: 200,
            }).await.map(center_tuple).unwrap();
        assert_eq!(all.len(), 2, "未过滤应返回全部非软删事项");

        // 手动桶
        let (manual, _, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None, bucket: Some(ComputedBucket::Manual), search: None, status: None, action_type: None, sort_by: None, sort_desc: true, page: 1, page_size: 200,
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(manual.len(), 1);
        assert_eq!(manual[0].todo.id, manual_id);
        assert_eq!(manual[0].computed_bucket, ComputedBucket::Manual);

        // 已归档桶
        let (archived, _, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None, bucket: Some(ComputedBucket::Archived), search: None, status: None, action_type: None, sort_by: None, sort_desc: true, page: 1, page_size: 200,
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].todo.id, archived_id);
        assert_eq!(archived[0].computed_bucket, ComputedBucket::Archived);
    }

    /// 插一条启用 loop_step 引用 todo，确认 get_todo_center 分到 LoopDriven。
    /// 验证 Loop 引用计数聚合与「Loop 驱动优先于时间/事件」优先级在 DB 层落地。
    #[tokio::test]
    async fn test_get_todo_center_loop_driven_classification() {
        let db = fresh_db().await;
        let id = seed_todo(&db, "被引用").await;
        // 先建 loop 行（loop_steps.loop_id 有 FK 约束）
        db.exec("INSERT INTO loops (name) VALUES ('L1')")
            .await
            .expect("insert loop");
        // 插一条启用的 step 引用该 todo；enabled=1 才计入 Loop 驱动
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES (1, 's1', {id}, 1)"
        ))
        .await
        .expect("insert step");

        let (items, _, _, _, _) = db.get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None, bucket: None, search: None, status: None, action_type: None, sort_by: None, sort_desc: true, page: 1, page_size: 200,
            }).await.map(center_tuple).unwrap();
        let item = items.iter().find(|i| i.todo.id == id).expect("todo present");
        assert_eq!(item.used_by_loop_step_count, 1, "应聚合到 1 次启用引用");
        assert_eq!(item.computed_bucket, ComputedBucket::LoopDriven);
    }

    /// get_todo_center search 参数：按 title/prompt 子串过滤（大小写不敏感）。
    #[tokio::test]
    async fn test_get_todo_center_search_filters_by_title() {
        let db = fresh_db().await;
        seed_todo(&db, "修复登录").await;
        seed_todo(&db, "优化prompt").await;
        // 全量应含两条
        let (all, _, _, _, _) = db.get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None, bucket: None, search: None, status: None, action_type: None, sort_by: None, sort_desc: true, page: 1, page_size: 200,
            }).await.map(center_tuple).unwrap();
        assert_eq!(all.len(), 2);
        // search="登录" 只命中第一条
        let (hit, _, _, _, _) = db.get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None, bucket: None, search: Some("登录"), status: None, action_type: None, sort_by: None, sort_desc: true, page: 1, page_size: 200,
            }).await.map(center_tuple).unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].todo.title, "修复登录");
        // 大小写不敏感：search="PROMPT" 命中 prompt 子串
        let (hit2, _, _, _, _) = db.get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None, bucket: None, search: Some("PROMPT"), status: None, action_type: None, sort_by: None, sort_desc: true, page: 1, page_size: 200,
            }).await.map(center_tuple).unwrap();
        assert_eq!(hit2.len(), 1);
        assert_eq!(hit2[0].todo.title, "优化prompt");
    }

    /// get_todo_center_item：不存在的 id 返回 None（archive/restore/webhook 据此回 404）。
    #[tokio::test]
    async fn test_get_todo_center_item_not_found() {
        let db = fresh_db().await;
        assert!(db.get_todo_center_item(99999).await.unwrap().is_none());
    }

    /// get_todo_center_item：单条路径与列表口径一致（manual 分类 + 字段填充）。
    #[tokio::test]
    async fn test_get_todo_center_item_assembles_manual() {
        let db = fresh_db().await;
        let id = seed_todo(&db, "单查").await;
        let item = db.get_todo_center_item(id).await.unwrap().expect("应存在");
        assert_eq!(item.todo.id, id);
        assert_eq!(item.computed_bucket, ComputedBucket::Manual);
        assert_eq!(item.used_by_loop_step_count, 0);
    }

    /// build_center_item 纯函数：最近执行记录摘要正确映射到 last_execution_* 字段。
    /// 优先 finished_at，回退 started_at。
    #[test]
    fn test_build_center_item_maps_last_execution() {
        let mut todo = make_minimal_todo();
        todo.id = 42;
        let mut loop_map = std::collections::HashMap::new();
        // 给一个 Loop 引用计数，确认同时透传
        loop_map.insert(42_i64, 2_i64);
        let mut exec_map = std::collections::HashMap::new();
        exec_map.insert(
            42,
            LatestExecutionSummary {
                status: Some("failed".into()),
                finished_at: Some("2026-07-08T10:00:00Z".into()),
                started_at: Some("2026-07-08T09:59:00Z".into()),
            },
        );
        let aggs = aggs_with(loop_map, exec_map);
        let item = Database::build_center_item(todo, &aggs);
        assert_eq!(item.used_by_loop_step_count, 2);
        assert_eq!(item.computed_bucket, ComputedBucket::LoopDriven);
        assert_eq!(item.last_execution_status.as_deref(), Some("failed"));
        // 优先 finished_at
        assert_eq!(item.last_execution_at.as_deref(), Some("2026-07-08T10:00:00Z"));
    }

    /// build_center_item：无 finished_at 时回退 started_at 作为展示时间。
    #[test]
    fn test_build_center_item_falls_back_to_started_at() {
        let mut todo = make_minimal_todo();
        todo.id = 7;
        let mut exec_map = std::collections::HashMap::new();
        exec_map.insert(
            7,
            LatestExecutionSummary {
                status: Some("running".into()),
                finished_at: None,
                started_at: Some("2026-07-08T09:00:00Z".into()),
            },
        );
        let aggs = aggs_with(Default::default(), exec_map);
        let item = Database::build_center_item(todo, &aggs);
        assert_eq!(item.last_execution_at.as_deref(), Some("2026-07-08T09:00:00Z"));
    }

    /// build_center_item：无执行记录时 last_execution_* 均为 None，连续失败 0。
    #[test]
    fn test_build_center_item_no_execution() {
        let todo = make_minimal_todo();
        let item = Database::build_center_item(todo, &empty_aggs());
        assert!(item.last_execution_status.is_none());
        assert!(item.last_execution_at.is_none());
        assert_eq!(item.computed_bucket, ComputedBucket::Manual);
        assert_eq!(item.consecutive_failure_count, 0);
    }

    /// build_center_item：引用 Loop 摘要按 todo_id 透传，未被引用时为空 vec。
    #[test]
    fn test_build_center_item_references_loops() {
        let mut todo = make_minimal_todo();
        todo.id = 42;
        let mut loop_map = std::collections::HashMap::new();
        loop_map.insert(42_i64, 1_i64);
        let mut ref_map = std::collections::HashMap::new();
        ref_map.insert(
            42,
            vec![
                crate::models::LoopRefSummary { loop_id: 5, loop_name: "L5".into(), process_template_id: None, process_template_name: None, process_template_version: None },
                crate::models::LoopRefSummary { loop_id: 8, loop_name: "L8".into(), process_template_id: None, process_template_name: None, process_template_version: None },
            ],
        );
        let mut aggs = empty_aggs();
        aggs.loop_count_map = loop_map;
        aggs.referencing_loops_map = ref_map;
        let item = Database::build_center_item(todo, &aggs);
        assert_eq!(item.computed_bucket, ComputedBucket::LoopDriven);
        assert_eq!(item.referencing_loops.len(), 2);
        assert_eq!(item.referencing_loops[0].loop_id, 5);
        assert_eq!(item.referencing_loops[1].loop_name, "L8");
    }

    /// build_center_item：连续失败次数与 webhook 最近触发时间按 todo_id 透传。
    #[test]
    fn test_build_center_item_failure_count_and_webhook() {
        let mut todo = make_minimal_todo();
        todo.id = 9;
        let mut aggs = empty_aggs();
        aggs.consecutive_fail_map.insert(9, 3);
        aggs.last_webhook_map.insert(9, "2026-07-08T11:00:00Z".into());
        let item = Database::build_center_item(todo, &aggs);
        assert_eq!(item.consecutive_failure_count, 3);
        assert_eq!(item.last_webhook_trigger_at.as_deref(), Some("2026-07-08T11:00:00Z"));
    }

    /// 构造全空的聚合结构体，测试里按需覆盖个别字段。
    fn empty_aggs() -> TodoCenterAggregates {
        TodoCenterAggregates {
            loop_count_map: Default::default(),
            referencing_loops_map: Default::default(),
            last_exec_map: Default::default(),
            consecutive_fail_map: Default::default(),
            last_webhook_map: Default::default(),
            slash_command_map: Default::default(),
        }
    }

    /// 由 loop_count_map + last_exec_map 构造聚合（其余空），简化常见用例。
    fn aggs_with(
        loop_count_map: std::collections::HashMap<i64, i64>,
        last_exec_map: std::collections::HashMap<i64, LatestExecutionSummary>,
    ) -> TodoCenterAggregates {
        TodoCenterAggregates {
            loop_count_map,
            last_exec_map,
            ..empty_aggs()
        }
    }

    /// 构造一个最小合法 Todo 供 build_center_item 纯函数测试复用。
    fn make_minimal_todo() -> Todo {
        Todo {
            id: 0,
            title: "t".into(),
            prompt: "p".into(),
            status: TodoStatus::Pending,
            created_at: String::new(),
            updated_at: String::new(),
            tag_ids: vec![],
            executor: None,
            expert_name: None,
            model: None,
            scheduler_enabled: false,
            scheduler_config: None,
            scheduler_timezone: None,
            scheduler_next_run_at: None,
            task_id: None,
            workspace_path: None,
            workspace_id: None,
            webhook_enabled: false,
            acceptance_criteria: None,
            todo_type: 0,
            parent_todo_id: None,
            review_template_id: None,
            auto_review_enabled: false,
            action_type: None,
            action_key: None,
            archived_at: None,
            // 需求 055 新增字段；build_center_item 用例不关心技能，给空数组即可
            skills: vec![],
        }
    }

    // ==================== 056：服务端分页与轻量查询测试 ====================

    /// 056 核心防漂移测试：SQL bucket 表达式与 Rust `compute_bucket` 逐条对拍。
    /// 四种事实字段组合（手动/时间/事件/归档）+ Loop 引用，两侧分类必须一致。
    #[tokio::test]
    async fn test_center_bucket_sql_matches_rust() {
        let db = fresh_db().await;
        // 手动：无任何标记
        let manual_id = seed_todo(&db, "纯手动").await;
        // 时间驱动：scheduler_config 非空
        let time_id = seed_todo(&db, "时间").await;
        db.exec(&format!(
            "UPDATE todos SET scheduler_config = '0 0 9 * * *' WHERE id = {time_id}"
        ))
        .await
        .unwrap();
        // 事件驱动：webhook_enabled=1
        let event_id = seed_todo(&db, "事件").await;
        db.update_todo_webhook(event_id, true).await.unwrap();
        // 已归档
        let archived_id = seed_todo(&db, "归档").await;
        db.archive_todo(archived_id).await.unwrap();
        // Loop 驱动：启用 loop_step 引用
        let loop_id = seed_todo(&db, "被环引用").await;
        db.exec("INSERT INTO loops (name) VALUES ('Lx')").await.unwrap();
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES (1, 's', {loop_id}, 1)"
        ))
        .await
        .unwrap();

        // SQL 侧：bucket_counts 是 SQL 表达式的直接输出
        let counts = db
            .count_todo_center_buckets(&crate::db::TodoCenterPageQuery {
                workspace_id: None, bucket: None, search: None, status: None, action_type: None,
                sort_by: None, sort_desc: true, page: 1, page_size: 200,
            })
            .await
            .unwrap();
        assert_eq!(counts.get("manual"), Some(&1), "manual 计数");
        assert_eq!(counts.get("time_driven"), Some(&1), "time 计数");
        assert_eq!(counts.get("event_driven"), Some(&1), "event 计数");
        assert_eq!(counts.get("archived"), Some(&1), "archived 计数");
        assert_eq!(counts.get("loop_driven"), Some(&1), "loop 计数");

        // Rust 侧：同一批数据经 build_center_item 推导（聚合来自 DB，同源）
        let (items, total, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None,
                bucket: None,
                search: None,
                status: None, action_type: None, sort_by: None,
                sort_desc: true,
                page: 1,
                page_size: 200,
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(total, 5);
        let rust_bucket = |id: i64| {
            items
                .iter()
                .find(|i| i.todo.id == id)
                .map(|i| format!("{:?}", i.computed_bucket))
        };
        assert_eq!(rust_bucket(manual_id).as_deref(), Some("Manual"));
        assert_eq!(rust_bucket(time_id).as_deref(), Some("TimeDriven"));
        assert_eq!(rust_bucket(event_id).as_deref(), Some("EventDriven"));
        assert_eq!(rust_bucket(archived_id).as_deref(), Some("Archived"));
        assert_eq!(rust_bucket(loop_id).as_deref(), Some("LoopDriven"));
    }

    /// 056：分页元数据——page 越界截断、total 与 bucket 过滤一致、counts 不含 bucket 过滤。
    #[tokio::test]
    async fn test_center_page_pagination_metadata() {
        let db = fresh_db().await;
        for i in 0..5 {
            seed_todo(&db, &format!("事项{i}")).await;
        }
        // page_size=2 时第 3 页只有 1 条，total=5
        let (items, total, _, counts, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None,
                bucket: None,
                search: None,
                status: None, action_type: None, sort_by: None,
                sort_desc: true,
                page: 3,
                page_size: 2,
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(total, 5);
        assert_eq!(counts.get("manual"), Some(&5));
        // bucket 过滤后 total 收縮，counts 不受影响（Tab 角标语义）
        let (items, total, _, counts, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None,
                bucket: Some(ComputedBucket::Archived),
                search: None,
                status: None, action_type: None, sort_by: None,
                sort_desc: true,
                page: 1,
                page_size: 20,
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert!(items.is_empty());
        assert_eq!(total, 0, "archived 桶为空，total=0");
        assert_eq!(counts.get("manual"), Some(&5), "counts 不随 bucket 过滤塌缩");
    }

    /// 056：brief 语义——看板模式（ids=None）隐藏已归档；定点模式（ids=Some）包含已归档。
    #[tokio::test]
    async fn test_todo_briefs_archived_semantics() {
        let db = fresh_db().await;
        let keep = seed_todo(&db, "保留").await;
        let gone = seed_todo(&db, "归档").await;
        db.archive_todo(gone).await.unwrap();

        let kanban = db.get_todo_briefs(None, None, None).await.unwrap();
        assert!(kanban.iter().any(|b| b.id == keep));
        assert!(!kanban.iter().any(|b| b.id == gone), "看板模式隐藏已归档");

        let lookup = db.get_todo_briefs(None, Some(&[gone]), None).await.unwrap();
        assert_eq!(lookup.len(), 1, "定点模式能查到已归档事项的标题");
        assert_eq!(lookup[0].title, "归档");
        assert!(lookup[0].archived_at.is_some());
    }

    /// 056：旧 /todos 分页版——hours 过滤下推 SQL、未归档隐藏、total 正确。
    #[tokio::test]
    async fn test_todos_page_by_workspace_hours_and_total() {
        let db = fresh_db().await;
        let recent = seed_todo(&db, "最近").await;
        let old = seed_todo(&db, "老旧").await;
        // seed_todo 最小列集不带 updated_at（生产由触发器写入），测试显式赋值
        db.exec(&format!(
            "UPDATE todos SET updated_at = datetime('now') WHERE id = {recent}"
        ))
        .await
        .unwrap();
        // 把 old 的 updated_at 改到 10 天前
        db.exec(&format!(
            "UPDATE todos SET updated_at = datetime('now', '-240 hours') WHERE id = {old}"
        ))
        .await
        .unwrap();
        let (items, total, _) = db
            .get_todos_page_by_workspace(None, Some(24), 1, 200)
            .await
            .unwrap();
        assert_eq!(total, 1, "hours=24 只留最近一条");
        assert_eq!(items[0].id, recent);

        let (_, total_all, _) = db
            .get_todos_page_by_workspace(None, None, 1, 200)
            .await
            .unwrap();
        assert_eq!(total_all, 2);
    }

    /// 056：云同步游标分批——批间不漏不重，末批为空终止。
    #[tokio::test]
    async fn test_todos_batch_after_id_cursor() {
        let db = fresh_db().await;
        let mut seeded = Vec::new();
        for i in 0..5 {
            seeded.push(seed_todo(&db, &format!("批{i}")).await);
        }
        seeded.sort_unstable();
        let mut collected = Vec::new();
        let mut after = 0i64;
        loop {
            let batch = db.get_todos_batch_after_id(after, 2).await.unwrap();
            if batch.is_empty() {
                break;
            }
            after = batch.last().map(|t| t.id).unwrap_or(after);
            collected.extend(batch.into_iter().map(|t| t.id));
        }
        assert_eq!(collected, seeded, "游标分批应不重不漏收齐全部");
    }

    /// 056：ids/count 轻量接口——不拉行，且隐藏已归档（日常视图片语义）。
    #[tokio::test]
    async fn test_todo_ids_and_count_exclude_archived() {
        let db = fresh_db().await;
        // ids/count 接口按 ws 过滤，需要一个真实 workspace
        let ws = db
            .create_project_directory("/tmp/056-ids", Some("w056"), false, false)
            .await
            .unwrap();
        let keep = seed_todo(&db, "ws内").await;
        db.exec(&format!("UPDATE todos SET workspace_id = {ws} WHERE id = {keep}"))
            .await
            .unwrap();
        let gone = seed_todo(&db, "ws归档").await;
        db.exec(&format!("UPDATE todos SET workspace_id = {ws} WHERE id = {gone}"))
            .await
            .unwrap();
        db.archive_todo(gone).await.unwrap();
        seed_todo(&db, "ws外").await; // 不属于该 ws

        let ids = db.get_todo_ids_by_workspace(ws).await.unwrap();
        assert_eq!(ids, vec![keep]);
        assert_eq!(db.count_todos_by_workspace(ws).await.unwrap(), 1);
    }

    /// 056 评审补测：sort_by=computed_bucket 的 SQL 表达式排序。
    /// 按 CASE 输出串字典序：archived < event_driven < loop_driven < manual < time_driven。
    #[tokio::test]
    async fn test_center_page_sort_by_computed_bucket() {
        let db = fresh_db().await;
        let manual_id = seed_todo(&db, "m").await;
        let time_id = seed_todo(&db, "t").await;
        db.exec(&format!(
            "UPDATE todos SET scheduler_config = '0 0 9 * * *' WHERE id = {time_id}"
        ))
        .await
        .unwrap();
        let archived_id = seed_todo(&db, "a").await;
        db.archive_todo(archived_id).await.unwrap();

        let (items, _, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None,
                bucket: None,
                search: None,
                status: None,
                action_type: None,
                sort_by: Some("computed_bucket"),
                sort_desc: false, // ASC
                page: 1,
                page_size: 20,
            })
            .await
            .map(center_tuple)
            .unwrap();
        let order: Vec<i64> = items.iter().map(|i| i.todo.id).collect();
        assert_eq!(
            order,
            vec![archived_id, manual_id, time_id],
            "ASC 应按 bucket 名字典序 archived < manual < time_driven"
        );
    }

    /// 056 评审补测：搜索词含 LIKE 通配符（%/_）时被转义，不作为通配符匹配。
    #[tokio::test]
    async fn test_center_page_search_escapes_like_wildcards() {
        let db = fresh_db().await;
        seed_todo(&db, "进度 50% 完成").await;
        seed_todo(&db, "进度 50x 完成").await;
        // 未转义时 "%50%" 中的 % 会同时命中两条；转义后只命中字面条目
        let (items, total, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None,
                bucket: None,
                search: Some("50%"),
                status: None,
                action_type: None,
                sort_by: None,
                sort_desc: true,
                page: 1,
                page_size: 20,
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(total, 1, "% 必须按字面匹配");
        assert_eq!(items[0].todo.title, "进度 50% 完成");
        // 下划线同样转义
        let (_, total2, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None,
                bucket: None,
                search: Some("50_"),
                status: None,
                action_type: None,
                sort_by: None,
                sort_desc: true,
                page: 1,
                page_size: 20,
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(total2, 0, "_ 必须按字面匹配，不存在含 50_ 的标题");
    }

    /// 056 评审补测：status / action_type 精确过滤下推 SQL。
    #[tokio::test]
    async fn test_center_page_status_and_action_type_filters() {
        let db = fresh_db().await;
        let pending_id = seed_todo(&db, "待办").await;
        let done_id = seed_todo(&db, "完成").await;
        db.exec(&format!("UPDATE todos SET status = 'completed' WHERE id = {done_id}"))
            .await
            .unwrap();
        let act_id = seed_todo(&db, "快捷").await;
        db.exec(&format!("UPDATE todos SET action_type = 'quick' WHERE id = {act_id}"))
            .await
            .unwrap();

        let base = crate::db::TodoCenterPageQuery {
            workspace_id: None,
            bucket: None,
            search: None,
            status: None,
            action_type: None,
            sort_by: None,
            sort_desc: true,
            page: 1,
            page_size: 20,
        };
        let (items, total, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                status: Some("completed"),
                ..base.clone()
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(items[0].todo.id, done_id);

        let (items, total, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                action_type: Some("quick"),
                ..base.clone()
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(items[0].todo.id, act_id);

        // 组合过滤：completed + quick 应无交集（quick 是 pending）
        let (_, total3, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                status: Some("completed"),
                action_type: Some("quick"),
                ..base.clone()
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(total3, 0);
        // pending_id 只是防止 unused 警告，并验证 status=pending 能命中
        let (_, total4, _, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                status: Some("pending"),
                ..base.clone()
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(total4, 2, "pending 应命中 待办+快捷（{pending_id}/{act_id}）");
    }

    /// CodeRabbit#1 回归：两列投影必须走原生 SQL（select_only + 部分列 + Model 反序列化
    /// 会在运行时 ColumnNotFound）。验证映射键为小写 title、值为 id。
    #[tokio::test]
    async fn test_todo_title_id_map_two_column_projection() {
        let db = fresh_db().await;
        let id = seed_todo(&db, "MyTask").await;
        let map = db.get_todo_title_id_map().await.unwrap();
        assert_eq!(map.get("mytask"), Some(&id), "键为小写化 title");
    }

    /// CodeRabbit#2 回归：page 超界时按 total 截断到最后一页，不执行大 OFFSET。
    /// 评审 F2 回归：返回值中的页码必须是截断后的有效页（响应元数据与内容一致）。
    #[tokio::test]
    async fn test_center_page_clamps_overflow_page() {
        let db = fresh_db().await;
        for i in 0..5 {
            seed_todo(&db, &format!("项{i}")).await;
        }
        // page=100000 应被截断到最后一页（total=5, page_size=2 → max_page=3）
        let (items, total, page, _, _) = db
            .get_todo_center_page(crate::db::TodoCenterPageQuery {
                workspace_id: None,
                bucket: None,
                search: None,
                status: None,
                action_type: None,
                sort_by: None,
                sort_desc: true,
                page: 100000,
                page_size: 2,
            })
            .await
            .map(center_tuple)
            .unwrap();
        assert_eq!(total, 5);
        assert_eq!(page, 3, "返回的页码必须是截断后的有效页（评审 F2）");
        assert_eq!(items.len(), 1, "截断到最后一页只剩 1 条");

        // 旧 /todos 分页同样截断且返回有效页码
        let (items2, _, page2) = db
            .get_todos_page_by_workspace(None, None, 99999, 2)
            .await
            .unwrap();
        assert_eq!(items2.len(), 1);
        assert_eq!(page2, 3, "旧 /todos 分页同样返回有效页码（评审 F2）");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod todo_skills_tests {
    //! `update_todo_skills` 与 `model_to_todo` skills 解析的单元测试（需求 055）。
    //!
    //! 关注三点：写入后能读回、覆盖写生效、未设置时默认为空数组（列默认值 '[]'）。

    // 只用 Database 的公开方法，不引用父模块私有项，故不导 super::*。
    use crate::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    #[tokio::test]
    async fn test_update_todo_skills_round_trip() {
        let db = fresh_db().await;
        let todo_id = db
            .create_todo("带技能的事项", "do something")
            .await
            .expect("create todo");
        // 新建事项默认无技能（列默认值 '[]' → 空数组）
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert!(todo.skills.is_empty(), "新建事项 skills 应为空数组");

        // 写入两个技能后能逐字读回
        let skills = vec!["code-review".to_string(), "test-gen".to_string()];
        db.update_todo_skills(todo_id, &skills).await.expect("update skills");
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert_eq!(todo.skills, skills, "skills 写入后应读回一致");
    }

    #[tokio::test]
    async fn test_update_todo_skills_overwrite_and_clear() {
        let db = fresh_db().await;
        let todo_id = db
            .create_todo("覆盖写事项", "do something")
            .await
            .expect("create todo");
        db.update_todo_skills(todo_id, &["a".to_string(), "b".to_string()])
            .await
            .expect("seed skills");
        // 覆盖写：新列表整体替换旧列表（升级工艺重建场景依赖该语义）
        db.update_todo_skills(todo_id, &["c".to_string()])
            .await
            .expect("overwrite skills");
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert_eq!(todo.skills, vec!["c".to_string()], "覆盖写应整体替换");

        // 传空切片 → 显式清空回 '[]'
        db.update_todo_skills(todo_id, &[]).await.expect("clear skills");
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert!(todo.skills.is_empty(), "空切片应清空 skills");
    }
}
