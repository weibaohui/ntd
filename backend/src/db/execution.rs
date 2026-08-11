use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Statement,
};

use crate::db::entity::execution_logs;
use crate::db::entity::execution_records;
use crate::db::entity::todos;
use crate::db::Database;
use crate::models::{ExecutionRecord, ExecutionStatus, ExecutionSummary, ExecutionUsage, ParsedLogEntry};

/// 事项中心批量聚合用：每个 todo 最近一次执行记录的摘要。
///
/// 仅取状态与时间两个轻量字段，避免把整条记录载入内存。
/// `at` 由 handler 取 `finished_at` 回退 `started_at` 作为展示时间。
pub struct LatestExecutionSummary {
    pub status: Option<String>,
    pub finished_at: Option<String>,
    pub started_at: Option<String>,
}

impl LatestExecutionSummary {
    /// 展示时间：优先 finished_at（真正结束），回退 started_at（仍在跑或未写完成时间）。
    pub fn display_at(&self) -> Option<&str> {
        self.finished_at
            .as_deref()
            .or(self.started_at.as_deref())
    }
}

/// 从查询行构造 LatestExecutionSummary（抽出以让批量查询函数低于 30 行）。
fn latest_summary_from_row(
    row: &sea_orm::QueryResult,
) -> Result<LatestExecutionSummary, sea_orm::DbErr> {
    Ok(LatestExecutionSummary {
        status: row.try_get_by("status")?,
        finished_at: row.try_get_by("finished_at")?,
        started_at: row.try_get_by("started_at")?,
    })
}

/// 把 `SELECT todo_id, COUNT(*) AS <cnt_col>` 的结果行收集成 `todo_id -> count` map。
/// 计数为 0 的 todo 不在结果集中（GROUP BY 不产生 0 行），调用方按 `unwrap_or(0)` 取。
fn rows_to_count_map(
    rows: Vec<sea_orm::QueryResult>,
    cnt_col: &str,
) -> std::collections::HashMap<i64, i64> {
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let todo_id: i64 = match row.try_get_by("todo_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Ok(cnt) = row.try_get_by::<i64, _>(cnt_col) {
            map.insert(todo_id, cnt);
        }
    }
    map
}

pub struct NewExecutionRecord<'a> {
    pub todo_id: Option<i64>,
    pub command: &'a str,
    pub executor: &'a str,
    pub trigger_type: &'a str,
    pub task_id: &'a str,
    pub session_id: Option<&'a str>,
    pub resume_message: Option<&'a str>,
    /// 触发这次执行的源 todo id（loop 评审或外部来源）。`None` 表示手动/无来源。
    pub source_todo_id: Option<i64>,
    /// 触发源的展示标题（loop 写 step 标题，auto_review 写原 todo 标题）。
    pub source_todo_title: Option<&'a str>,
    /// 当本次执行是 loop 环节的一部分时，指向 loop_step_executions 表的 id。
    pub loop_step_execution_id: Option<i64>,
    /// 环节 id（指向 steps 表），环节独立执行时使用
    pub step_id: Option<i64>,
    /// record 直接归属的 workspace（v89）：写入 execution_records.workspace_id，
    /// 归属校验改用它，不再经 todo 间接关联。
    pub workspace_id: Option<i64>,
}

pub struct UpdateExecutionRecordRequest<'a> {
    pub id: i64,
    pub status: &'a str,
    pub remaining_logs: &'a str,
    pub result: &'a str,
    pub usage: Option<&'a ExecutionUsage>,
    pub model: Option<&'a str>,
    /// 自动评审专用. Some((source_record_id, status)) 表示这条记录是评审实例,
    /// 其结果用于回填到 source_record_id 对应的原记录.
    /// 存到表里: source_execution_record_id = source_record_id,
    ///           last_review_status = status.
    pub review_meta: Option<(i64, &'a str)>,
}

pub struct ExecutionRecordQuery<'a> {
    pub todo_id: Option<i64>,
    pub step_id: Option<i64>,
    /// 工作空间过滤：execution_records 无 workspace_id 列，经 todos 子查询间接关联。
    /// 下推 SQL 而非内存过滤——内存过滤发生在 LIMIT/OFFSET 之后，会把本 ws 记录
    /// 稀释到各页导致分页条数与 total 对不上（056 修复的正确性 bug）。
    pub workspace_id: Option<i64>,
    pub limit: i64,
    pub offset: i64,
    pub status: Option<&'a str>,
    pub hours: Option<u32>,
}

/// 构造「本工作空间 todo」的子查询条件：todo_id IN (SELECT id FROM todos ...)。
///
/// 抽成独立函数是因为数据查询与 COUNT 查询必须共用完全相同的过滤条件，
/// 任何一边漏加都会让分页元数据失真。
fn workspace_todo_subquery(
    workspace_id: i64,
) -> sea_orm::sea_query::SelectStatement {
    sea_orm::sea_query::Query::select()
        .column(todos::Column::Id)
        .from(todos::Entity)
        .and_where(todos::Column::WorkspaceId.eq(workspace_id))
        .and_where(todos::Column::DeletedAt.is_null())
        .to_owned()
}

/// `get_execution_summary` 使用的固定 SQL 字面量。
const EXECUTION_SUMMARY_SQL: &str = "SELECT \
            COUNT(*) as total, \
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success_count, \
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed_count, \
            COALESCE(SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END), 0) as running_count, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.input_tokens'), 0)), 0) as input_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.output_tokens'), 0)), 0) as output_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_read_input_tokens'), 0)), 0) as cache_read, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_creation_input_tokens'), 0)), 0) as cache_creation, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.total_cost_usd'), 0.0)), 0.0) as total_cost \
            FROM execution_records WHERE todo_id = $1";

/// Skills 总体统计的扁平中间结构（`fetch_skills_overall` 的返回类型）。
#[derive(Default)]
struct SkillsOverallRow {
    total: i64,
    success: i64,
    failed: i64,
    avg_duration_ms: f64,
    today: i64,
}

impl From<execution_records::Model> for ExecutionRecord {
    fn from(m: execution_records::Model) -> Self {
        let usage = m
            .usage
            .as_deref()
            .and_then(|u| serde_json::from_str(u).ok());
        let execution_stats = m
            .execution_stats
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let status = m
            .status
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                tracing::warn!(
                    "Failed to parse execution status, defaulting to Running: {:?}",
                    m.status
                );
                ExecutionStatus::Running
            });
        ExecutionRecord {
            id: m.id,
            todo_id: m.todo_id.unwrap_or(0),
            status,
            command: m.command.unwrap_or_default(),
            stdout: m.stdout.unwrap_or_default(),
            stderr: m.stderr.unwrap_or_default(),
            result: m.result,
            started_at: m.started_at.unwrap_or_default(),
            finished_at: m.finished_at,
            usage,
            executor: m.executor,
            model: m.model,
            trigger_type: m.trigger_type.unwrap_or_else(|| "manual".to_string()),
            pid: m.pid,
            task_id: m.task_id,
            session_id: m.session_id,
            todo_progress: m.todo_progress,
            // agent_runs 是纯 JSON 字符串透传（前端 parse），无需像 usage/stats 那样反序列化。
            agent_runs: m.agent_runs,
            execution_stats,
            resume_message: m.resume_message,
            source_todo_id: m.source_todo_id,
            source_todo_title: m.source_todo_title,
            rating: m.rating,
            source_execution_record_id: m.source_execution_record_id,
            last_review_status: m.last_review_status,
            last_reviewed_at: m.last_reviewed_at,
            worktree_path: m.worktree_path,
            loop_step_execution_id: m.loop_step_execution_id,
            step_id: m.step_id,
            // 保留持久化归属：软删/迁移 todo 后，详情授权仍可用 record 自身 ws（v89 解耦）
            workspace_id: m.workspace_id,
        }
    }
}

impl Database {
    pub async fn get_execution_records(
        &self,
        query: ExecutionRecordQuery<'_>,
    ) -> Result<(Vec<ExecutionRecord>, i64), sea_orm::DbErr> {
        let base_filter = match (query.todo_id, query.step_id) {
            (Some(tid), Some(sid)) => {
                execution_records::Column::TodoId.eq(tid)
                    .or(execution_records::Column::StepId.eq(sid))
            }
            (Some(tid), None) => execution_records::Column::TodoId.eq(tid),
            (None, Some(sid)) => execution_records::Column::StepId.eq(sid),
            (None, None) => execution_records::Column::TodoId.is_not_null(),
        };
        let filter = match query.status {
            Some("all") | None => base_filter,
            Some(s) => base_filter.and(execution_records::Column::Status.eq(s)),
        };

        // workspace 过滤以 AND 叠加：与 (todo_id, step_id) 组合分支兼容——
        // 即使指定了 todo_id，也要求该 todo 属于本 ws（防 ?todo_id=<他人> 越权读）。
        let filter = if let Some(wid) = query.workspace_id {
            filter.and(execution_records::Column::TodoId.in_subquery(
                workspace_todo_subquery(wid),
            ))
        } else {
            filter
        };

        let filter = if let Some(h) = query.hours.filter(|&h| h > 0) {
            // 096-W2：预计算 UTC cutoff 做裸列 gte 绑定（093 同款口径）——
            // 替代 REPLACE(REPLACE(...)) >= datetime('now',...) 旧写法，started_at 索引保持命中
            filter.and(execution_records::Column::StartedAt.gte(crate::models::utc_timestamp_minus_hours(h)))
        } else {
            filter
        };

        let limit_u = if query.limit < 0 { 0 } else { query.limit as u64 };
        let offset_u = if query.offset < 0 { 0 } else { query.offset as u64 };

        let records = execution_records::Entity::find()
            .filter(filter.clone())
            .order_by_desc(execution_records::Column::StartedAt)
            .limit(limit_u)
            .offset(offset_u)
            .all(&self.conn)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();

        let count: i64 = execution_records::Entity::find()
            .filter(filter)
            .count(&self.conn)
            .await?
            .try_into()
            .unwrap_or(i64::MAX);

        Ok((records, count))
    }

    /// 按 workspace 查执行记录（最近任务 + 历史子页用）。
    /// execution_records 无 workspace_id，经 todos 间接关联：先取该 workspace 的 todo_id，
    /// 再按 todo_id IN 查记录。倒序 + 分页，返回 (records, total)。
    pub async fn get_execution_records_by_workspace(
        &self,
        workspace_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<ExecutionRecord>, i64), sea_orm::DbErr> {
        // 第一步：workspace 的所有 todo_id（execution_records 经 todos 关联到 workspace）
        let todo_ids: Vec<i64> = todos::Entity::find()
            .filter(todos::Column::WorkspaceId.eq(workspace_id))
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| t.id)
            .collect();
        if todo_ids.is_empty() {
            return Ok((Vec::new(), 0));
        }
        let limit_u = if limit < 0 { 0 } else { limit as u64 };
        let offset_u = if offset < 0 { 0 } else { offset as u64 };
        // 第二步：按 todo_id IN 查记录，倒序 + 分页
        let records: Vec<ExecutionRecord> = execution_records::Entity::find()
            .filter(execution_records::Column::TodoId.is_in(todo_ids.clone()))
            .order_by_desc(execution_records::Column::StartedAt)
            .limit(limit_u)
            .offset(offset_u)
            .all(&self.conn)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        let count: i64 = execution_records::Entity::find()
            .filter(execution_records::Column::TodoId.is_in(todo_ids))
            .count(&self.conn)
            .await?
            .try_into()
            .unwrap_or(i64::MAX);
        Ok((records, count))
    }

    pub async fn get_execution_record(
        &self,
        record_id: i64,
    ) -> Result<Option<ExecutionRecord>, sea_orm::DbErr> {
        let m = execution_records::Entity::find()
            .filter(execution_records::Column::Id.eq(record_id))
            .one(&self.conn)
            .await?;
        Ok(m.map(Into::into))
    }

    /// 批量按 id 取 execution_record（含 usage），按 id 分组返回。
    /// 执行历史列表批量 enrich token 用量时调用，一次 IN 查询消除逐 step 的 N+1（091 性能优化）。
    pub async fn get_execution_records_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, ExecutionRecord>, sea_orm::DbErr> {
        use std::collections::HashMap;
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = execution_records::Entity::find()
            .filter(execution_records::Column::Id.is_in(ids.to_vec()))
            .all(&self.conn)
            .await?;
        // Entity Model 转 domain ExecutionRecord（含 usage 反序列化），按 id 索引。
        let map: HashMap<i64, ExecutionRecord> = rows
            .into_iter()
            .map(|m| (m.id, m.into()))
            .collect();
        Ok(map)
    }

    /// 获取指定 todo 的最新一条执行记录（按 id 降序）。
    ///
    /// 用于黑板 debouncer：从 pending 队列取出 todo_id 后查其最新执行结论。
    pub async fn get_latest_execution_record_for_todo(
        &self,
        todo_id: i64,
    ) -> Result<Option<ExecutionRecord>, sea_orm::DbErr> {
        let m = execution_records::Entity::find()
            .filter(execution_records::Column::TodoId.eq(todo_id))
            .order_by_desc(execution_records::Column::Id)
            .limit(1)
            .one(&self.conn)
            .await?;
        Ok(m.map(Into::into))
    }

    /// 事项中心用：批量取每个 todo 的最近一次执行记录摘要（状态 + 时间）。
    ///
    /// 「最近」按 `id` 降序定义（id 单调递增，等价于最新创建的一条），
    /// 与单条版 `get_latest_execution_record_for_todo` 语义一致。
    /// 用 `WHERE id IN (SELECT MAX(id) ... GROUP BY todo_id)` 一次性聚合，
    /// 避免列表场景逐 todo 调用单条版造成 N+1。
    /// 返回 `todo_id -> 摘要`，未出现的 todo 视为无执行记录。
    pub async fn get_latest_execution_summaries_for_todos(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, LatestExecutionSummary>, sea_orm::DbErr> {
        if todo_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let (placeholders, values) = Database::in_clause(todo_ids);
        // 内层子查询取每个 todo 的最大 id（即最近一条），外层按 id 回查状态/时间
        let sql = format!(
            "SELECT todo_id, status, finished_at, started_at FROM execution_records \
             WHERE id IN (SELECT MAX(id) FROM execution_records \
             WHERE todo_id IS NOT NULL AND todo_id IN ({placeholders}) GROUP BY todo_id)"
        );
        let rows = self.query_all_sql(sql, values).await?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let todo_id: i64 = row.try_get_by("todo_id")?;
            map.insert(todo_id, latest_summary_from_row(&row)?);
        }
        Ok(map)
    }

    /// 批量统计每个 todo 的连续失败次数（事项中心时间/事件驱动卡片展示用）。
    ///
    /// 「连续失败」= 从最近一条执行记录往前数，连续 status='failed' 的条数；
    /// 一旦遇到非 failed（success/running/cancelled/NULL）即停。最近一条非 failed 则为 0。
    ///
    /// SQL 思路：统计 id 大于「最后一条非 failed 记录 id」的 failed 记录数。
    /// - 最近一条非 failed → 该 id 即分界，之后的 failed 即尾部连续失败。
    /// - 全部 failed（无非 failed 记录）→ COALESCE 回 0，所有 failed 计入。
    /// - 最近一条非 failed（无后续 failed）→ 计数 0。
    ///
    /// `status IS NOT 'failed'` 是 NULL-safe：NULL 状态视为非 failed 的断点。
    pub async fn get_consecutive_failure_counts_for_todos(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, sea_orm::DbErr> {
        if todo_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let (placeholders, values) = Database::in_clause(todo_ids);
        let sql = format!(
            "SELECT e.todo_id, COUNT(*) AS cnt FROM execution_records e \
             WHERE e.status='failed' AND e.todo_id IN ({placeholders}) \
             AND e.id > COALESCE((SELECT MAX(e2.id) FROM execution_records e2 \
             WHERE e2.todo_id=e.todo_id AND e2.status IS NOT 'failed'),0) GROUP BY e.todo_id"
        );
        let rows = self.query_all_sql(sql, values).await?;
        Ok(rows_to_count_map(rows, "cnt"))
    }

    /// 批量取每个 todo 最近一次 webhook 触发的时间（事项中心事件驱动卡片「最近触发时间」用）。
    ///
    /// 事件驱动卡片需要区分「最近一次触发」（webhook）与「最近一次执行」（任意触发源）：
    /// 用户手动「执行一次」不应顶掉 webhook 最近触发时间。只看 trigger_type='webhook'。
    /// 返回 `todo_id -> 时间串`（优先 finished_at 回退 started_at），无 webhook 记录则不在 map 中。
    pub async fn get_last_webhook_trigger_for_todos(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, String>, sea_orm::DbErr> {
        if todo_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let (placeholders, values) = Database::in_clause(todo_ids);
        // 相关子查询取每个 todo 的最大 webhook 记录 id，避免占位符重复
        let sql = format!(
            "SELECT e.todo_id, COALESCE(e.finished_at, e.started_at) AS at \
             FROM execution_records e WHERE e.trigger_type='webhook' AND e.todo_id IN ({placeholders}) \
             AND e.id=(SELECT MAX(e2.id) FROM execution_records e2 \
             WHERE e2.todo_id=e.todo_id AND e2.trigger_type='webhook')"
        );
        let rows = self.query_all_sql(sql, values).await?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let todo_id: i64 = row.try_get_by("todo_id")?;
            // at 为 NULL（无 finished/started）时不收入，保持「无 webhook 记录」语义
            let at: Option<String> = row.try_get_by("at")?;
            if let Some(t) = at {
                map.insert(todo_id, t);
            }
        }
        Ok(map)
    }

    /// 批量根据 task_id 列表获取执行记录（用于 WebSocket 同步等场景）
    pub async fn get_execution_records_by_task_ids(
        &self,
        task_ids: &[String],
    ) -> Result<Vec<ExecutionRecord>, sea_orm::DbErr> {
        if task_ids.is_empty() {
            return Ok(vec![]);
        }
        let models = execution_records::Entity::find()
            .filter(execution_records::Column::TaskId.is_in(task_ids.iter().map(|s| s.as_str())))
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(Into::into).collect())
    }

    pub async fn create_execution_record(
        &self,
        record: NewExecutionRecord<'_>,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = execution_records::ActiveModel {
            todo_id: ActiveValue::Set(record.todo_id),
            command: ActiveValue::Set(Some(record.command.to_string())),
            executor: ActiveValue::Set(Some(record.executor.to_string())),
            trigger_type: ActiveValue::Set(Some(record.trigger_type.to_string())),
            status: ActiveValue::Set(Some(crate::models::ExecutionStatus::Running.to_string())),
            started_at: ActiveValue::Set(Some(now)),
            task_id: ActiveValue::Set(Some(record.task_id.to_string())),
            session_id: ActiveValue::Set(record.session_id.map(|s| s.to_string())),
            resume_message: ActiveValue::Set(record.resume_message.map(|s| s.to_string())),
            source_todo_id: ActiveValue::Set(record.source_todo_id),
            source_todo_title: ActiveValue::Set(record.source_todo_title.map(|s| s.to_string())),
            loop_step_execution_id: ActiveValue::Set(record.loop_step_execution_id),
            step_id: ActiveValue::Set(record.step_id),
            // 写入 record 直接归属：把授权链路与 todo 生命周期解耦的前提（v89 核心）
            workspace_id: ActiveValue::Set(record.workspace_id),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// Update execution record status, but only if it is still "running".
    /// This prevents race conditions where both a stop handler and a spawned task
    /// try to update the same record concurrently -- only the first write succeeds.
    /// remaining_logs: 内存中尚未刷入 execution_logs 表的剩余日志
    /// Returns Ok(true) if the row was updated, Ok(false) if status was not "running".
    pub async fn update_execution_record(
        &self,
        req: UpdateExecutionRecordRequest<'_>,
    ) -> Result<bool, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let usage_json = Self::serialize_usage_json(req.usage);
        let model_val = req.model.map(|s| s.to_string());
        let backend = self.conn.get_database_backend();
        let (sql, values) = Self::build_update_statement(&req, now.clone(), usage_json, model_val);

        let res = self
            .conn
            .execute(Statement::from_sql_and_values(backend, sql, values))
            .await?;
        let updated = res.rows_affected() > 0;

        // Only insert logs if the status update succeeded (prevent duplicate logs on concurrent writes)
        //
        // 注：截至当前（fix #653 之后）已无内部 caller 触发此分支 —— 所有生产 caller（终态 cancel /
        // timeout / 正常完成 / 启动失败 共 4 处）均传 `remaining_logs: "[]"`，日志全部交给
        // `LogFlusher` 在 `finalize()` 阶段 drain 入库。本分支保留的原因：
        // 1. `UpdateExecutionRecord` 是公共 API surface，外部集成方可能仍依赖此行为；
        // 2. `backend/src/db/mod.rs::test_update_execution_record_does_not_duplicate_logs_issue_653`
        //    故意传全量 JSON 来回归 issue #653（5/10/5 断言）。
        // 后续若 release window 有空档，建议把此分支抽成 `append_logs_only` 单独方法并删除，
        // 让 `update_execution_record` 只负责 status / stats / usage 字段。
        Self::maybe_append_remaining_logs(self, req.id, updated, req.remaining_logs).await?;
        Ok(updated)
    }

    /// 将 `ExecutionUsage` 序列化为 JSON 字符串；序列化失败时降级为空串。
    fn serialize_usage_json(usage: Option<&crate::models::ExecutionUsage>) -> Option<String> {
        usage.map(|u| {
            serde_json::to_string(u).unwrap_or_else(|e| {
                tracing::error!("Failed to serialize usage: {}", e);
                String::new()
            })
        })
    }

    /// 根据是否携带 `review_meta` 构造两条不同的 UPDATE 语句。
    fn build_update_statement<'a>(
        req: &UpdateExecutionRecordRequest<'a>,
        now: String,
        usage_json: Option<String>,
        model_val: Option<String>,
    ) -> (&'static str, Vec<sea_orm::Value>) {
        if let Some((source_record_id, review_status)) = req.review_meta {
            (
                "UPDATE execution_records SET \
                    status = $1, \
                    result = $2, \
                    usage = $3, \
                    model = $4, \
                    finished_at = $5, \
                    source_execution_record_id = $7, \
                    last_review_status = $8, \
                    last_reviewed_at = $9 \
                    WHERE id = $6 AND status = 'running'",
                vec![
                    req.status.into(),
                    req.result.into(),
                    usage_json.into(),
                    model_val.into(),
                    now.clone().into(),
                    req.id.into(),
                    source_record_id.into(),
                    review_status.to_string().into(),
                    now.into(),
                ],
            )
        } else {
            (
                "UPDATE execution_records SET \
                    status = $1, \
                    result = $2, \
                    usage = $3, \
                    model = $4, \
                    finished_at = $5 \
                    WHERE id = $6 AND status = 'running'",
                vec![
                    req.status.into(),
                    req.result.into(),
                    usage_json.into(),
                    model_val.into(),
                    now.into(),
                    req.id.into(),
                ],
            )
        }
    }

    /// 当 status 更新成功且 remaining_logs 携带真实日志时，写入 execution_logs。
    async fn maybe_append_remaining_logs(
        &self,
        record_id: i64,
        updated: bool,
        remaining_logs: &str,
    ) -> Result<(), sea_orm::DbErr> {
        if updated && !remaining_logs.is_empty() && remaining_logs != "[]" {
            self.insert_execution_logs(record_id, remaining_logs).await?;
        }
        Ok(())
    }

    /// 更新执行记录的 pid
    pub async fn update_execution_record_pid(
        &self,
        id: i64,
        pid: Option<i32>,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(id),
            pid: ActiveValue::Set(pid),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// issue #643: 把本次执行实际使用的 git worktree 目录回写到 execution_record。
    ///
    /// 这一步在 `create_execution_record` 之后、真正 spawn 子进程之前发生，
    /// 用于"事后排查"：用户看到执行记录时能直接定位 worktree 目录。
    ///
    /// 失败时只 warn 不中断执行流程：worktree 路径写不进 DB 不影响子进程跑通。
    pub async fn update_execution_record_worktree_path(
        &self,
        id: i64,
        worktree_path: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(id),
            worktree_path: ActiveValue::Set(Some(worktree_path.to_string())),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 更新执行记录的 session_id
    pub async fn update_execution_record_session_id(
        &self,
        id: i64,
        session_id: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(id),
            session_id: ActiveValue::Set(Some(session_id.to_string())),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 更新执行记录的 todo_progress
    pub async fn update_execution_record_todo_progress(
        &self,
        id: i64,
        todo_progress_json: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(id),
            todo_progress: ActiveValue::Set(Some(todo_progress_json.to_string())),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 更新执行记录的 agent_runs（多 Agent 协作的子 agent 元数据 JSON）。
    ///
    /// 与 todo_progress 同构：调用方序列化 `Vec<AgentRun>` 为 JSON 字符串后传入。
    /// 完成态（persist_completion_record）一次性写入，不做实时增量。
    pub async fn update_execution_record_agent_runs(
        &self,
        id: i64,
        agent_runs_json: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(id),
            agent_runs: ActiveValue::Set(Some(agent_runs_json.to_string())),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 更新执行记录的 execution_stats
    pub async fn update_execution_record_stats(
        &self,
        id: i64,
        stats_json: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(id),
            execution_stats: ActiveValue::Set(Some(stats_json.to_string())),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 更新执行记录的评分。
    /// 评分属于“执行结果”，因此要求记录已结束（success/failed）；running 记录
    /// 不接受评分，handler 层会先拦抁返回错误。
    /// `Some(value)` 写入评分，`None` 清除评分（设为 NULL）。
    pub async fn update_execution_record_rating(
        &self,
        id: i64,
        rating: Option<i32>,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(id),
            rating: ActiveValue::Set(rating),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 追加日志条目到执行记录（直接写入 execution_logs 表，支持分页加载）
    pub async fn append_execution_record_logs(
        &self,
        id: i64,
        new_logs_json: &str,
    ) -> Result<(), sea_orm::DbErr> {
        if new_logs_json.is_empty() || new_logs_json == "[]" {
            return Ok(());
        }
        self.insert_execution_logs(id, new_logs_json).await
    }

    /// 将 JSON 格式的日志条目批量插入 execution_logs 表。
    ///
    /// 095：薄壳——解析 JSON 后转调 [`Self::insert_execution_log_entries`]。
    /// 本方法只服务存量 JSON 调用方（v2_v5 迁移、`remaining_logs` 路径）；
    /// 生产热路径（LogFlusher 落库）走对象版接口，无 JSON 中转。
    pub async fn insert_execution_logs(
        &self,
        record_id: i64,
        logs_json: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let entries: Vec<ParsedLogEntry> = serde_json::from_str(logs_json)
            .map_err(|e| sea_orm::DbErr::Custom(format!(
                "Failed to parse logs JSON for record {}: {}",
                record_id, e
            )))?;
        self.insert_execution_log_entries(record_id, &entries).await
    }

    /// 095：对象版批量插入（生产热路径）。入参即内存对象切片，
    /// 相比 JSON 版省掉「to_string 全量序列化 + from_str 全量反序列化」两趟转换。
    ///
    /// metadata 重打包不可省：execution_logs 表 schema 是 timestamp/log_type/content/metadata
    /// 四列，对象→列格式的转换属持久化必需（usage/tool_name/tool_input_json 合入 metadata 列）。
    pub async fn insert_execution_log_entries(
        &self,
        record_id: i64,
        entries: &[ParsedLogEntry],
    ) -> Result<(), sea_orm::DbErr> {
        // 空切片短路：insert_many 对空 Vec 会生成非法 SQL，且语义上无活可干
        if entries.is_empty() {
            return Ok(());
        }

        let models: Vec<execution_logs::ActiveModel> = entries
            .iter()
            .map(|e| {
                // metadata 三字段打包与旧 JSON 路径同一份代码逻辑——落库内容逐字节一致
                let metadata = serde_json::json!({
                    "usage": e.usage,
                    "tool_name": e.tool_name,
                    "tool_input_json": e.tool_input_json,
                });
                let metadata_str = serde_json::to_string(&metadata).unwrap_or_default();
                execution_logs::ActiveModel {
                    record_id: ActiveValue::Set(record_id),
                    // 切片只读借用，timestamp/log_type/content 需 clone 进 owned ActiveModel
                    timestamp: ActiveValue::Set(e.timestamp.clone()),
                    log_type: ActiveValue::Set(e.log_type.clone()),
                    content: ActiveValue::Set(e.content.clone()),
                    metadata: ActiveValue::Set(Some(metadata_str)),
                    ..Default::default()
                }
            })
            .collect();

        execution_logs::Entity::insert_many(models)
            .exec(&self.conn)
            .await?;
        Ok(())
    }

    /// 093-B5：删除早于 cutoff 的执行日志，返回实际删除行数。
    ///
    /// 从 `handlers/backup.rs::cleanup_old_logs` 下沉的 DAO（分层规范：handler 不写 SQL）。
    /// 两个修法一并落地：cutoff 走参数绑定（原 format! 拼接，违反禁止清单 #4）；
    /// 行数取 `ExecResult::rows_affected`（原额外跑一条 `SELECT changes()`）。
    /// cutoff 与 timestamp 列同为 ISO8601 字符串，字典序即时间序。
    pub async fn delete_execution_logs_before(&self, cutoff: &str) -> Result<u64, sea_orm::DbErr> {
        let res = self
            .conn
            // from_sql_and_values 走参数绑定：cutoff 不进 SQL 文本，
            // 从根上消除拼接注入面（禁止清单 #4），也不再依赖调用方的格式转义
            .execute(sea_orm::Statement::from_sql_and_values(
                // 本项目的生产/开发/测试库均为 SQLite，固定 DbBackend::Sqlite
                // 而非从连接探测——省一次运行时判断，语义显式
                sea_orm::DbBackend::Sqlite,
                "DELETE FROM execution_logs WHERE timestamp < ?",
                [cutoff.into()],
            ))
            // `?` 传播错误：handler 侧原样转成 String 错误文案，语义与旧路径一致
            .await?;
        // rows_affected 取自同一 statement 的执行结果；旧实现另跑 SELECT changes()
        // 在连接池下可能落到不同连接而取错行数，此处一并修正
        Ok(res.rows_affected())
    }

    /// 分页获取执行日志
    pub async fn get_execution_logs(
        &self,
        record_id: i64,
        page: i64,
        per_page: i64,
    ) -> Result<(Vec<ParsedLogEntry>, i64), sea_orm::DbErr> {
        let total: i64 = execution_logs::Entity::find()
            .filter(execution_logs::Column::RecordId.eq(record_id))
            .count(&self.conn)
            .await? as i64;

        if total == 0 {
            return Ok((Vec::new(), 0));
        }

        let offset = ((page - 1) * per_page).max(0) as u64;
        let entries = execution_logs::Entity::find()
            .filter(execution_logs::Column::RecordId.eq(record_id))
            .order_by_asc(execution_logs::Column::Id)
            .limit(per_page as u64)
            .offset(offset)
            .all(&self.conn)
            .await?;

        // 093：metadata 解析收敛到 parsed_log_entry_from_parts，不再在此重复一份
        let logs: Vec<ParsedLogEntry> = entries
            .into_iter()
            .map(|m| Self::parsed_log_entry_from_parts(m.timestamp, m.log_type, m.content, m.metadata.as_deref()))
            .collect();

        Ok((logs, total))
    }

    /// 把 execution_logs 行的四个字段组装成 `ParsedLogEntry`（093 抽取的共享映射）。
    ///
    /// metadata 列是 JSON 串 `{"usage":..,"tool_name":..,"tool_input_json":..}`，
    /// 解析失败（历史脏数据 / 缺字段）一律降级为 None 而非报错——日志是只读展示数据，
    /// 单行 metadata 损坏不应拖垮整页查询。
    fn parsed_log_entry_from_parts(
        timestamp: String,
        log_type: String,
        content: String,
        metadata: Option<&str>,
    ) -> ParsedLogEntry {
        // metadata 只读不消费，用 &str 避免调用方为传参克隆一份 JSON 串
        let (usage, tool_name, tool_input_json) = metadata
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .map(|v| {
                (
                    v.get("usage")
                        .and_then(|u| serde_json::from_value(u.clone()).ok()),
                    v.get("tool_name")
                        .and_then(|n| n.as_str().map(String::from)),
                    v.get("tool_input_json")
                        .and_then(|t| t.as_str().map(String::from)),
                )
            })
            .unwrap_or((None, None, None));
        ParsedLogEntry { timestamp, log_type, content, usage, tool_name, tool_input_json }
    }

    /// 093：批量统计多个 record 的日志总行数（WS 重连 Sync 的 `log_total` 数据源）。
    ///
    /// 单条 GROUP BY 聚合替代旧实现「全量读回再 Vec::len」：长跑任务数万行日志时，
    /// 重连不再需要把整表读进内存只为计数。
    pub async fn count_execution_logs_for_records(
        &self,
        record_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, sea_orm::DbErr> {
        if record_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        // in_clause 生成参数化占位符，杜绝字符串拼接注入面
        let (placeholders, values) = Self::in_clause(record_ids);
        let sql = format!(
            "SELECT record_id, COUNT(*) AS cnt FROM execution_logs \
             WHERE record_id IN ({placeholders}) GROUP BY record_id"
        );
        let rows = self
            .conn
            .query_all(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                values,
            ))
            .await?;
        // COUNT(*) 在无匹配行时该 record 不出现在结果集——调用方以 get().unwrap_or(0) 读
        Ok(rows
            .iter()
            .filter_map(|r| {
                Some((
                    r.try_get_by::<i64, _>("record_id").ok()?,
                    r.try_get_by::<i64, _>("cnt").ok()?,
                ))
            })
            .collect())
    }

    /// 093：批量取多个 record 的「尾部 cap 条」日志（WS 重连 Sync 的日志摘要数据源）。
    ///
    /// 窗口函数单查询完成「每个 record 各取最新 cap 条」，避免按 record 逐条查询的 N+1；
    /// 外层按 record_id, id ASC 归序，与旧实现「内存截尾后升序」语义完全一致，前端无感知。
    /// cap 参数化（生产传 RECONNECT_LOG_CAP=200，测试用小值验证边界）。
    pub async fn get_tail_execution_logs_for_records(
        &self,
        record_ids: &[i64],
        cap: i64,
    ) -> Result<std::collections::HashMap<i64, Vec<ParsedLogEntry>>, sea_orm::DbErr> {
        if record_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let (placeholders, mut values) = Self::in_clause(record_ids);
        // cap 拼在 WHERE rn <= ? 处，作为最后一个绑定值
        values.push(cap.into());
        let sql = format!(
            "SELECT id, record_id, timestamp, log_type, content, metadata FROM ( \
               SELECT id, record_id, timestamp, log_type, content, metadata, \
                      ROW_NUMBER() OVER (PARTITION BY record_id ORDER BY id DESC) AS rn \
               FROM execution_logs WHERE record_id IN ({placeholders}) \
             ) WHERE rn <= ? ORDER BY record_id, id ASC"
        );
        let rows = self
            .conn
            .query_all(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                values,
            ))
            .await?;
        let mut map: std::collections::HashMap<i64, Vec<ParsedLogEntry>> =
            std::collections::HashMap::with_capacity(record_ids.len());
        for r in &rows {
            // 单行字段读取失败时跳过该行而不是整批失败——日志是展示数据，可用性优先
            let (Ok(record_id), Ok(timestamp), Ok(log_type), Ok(content), Ok(metadata)) = (
                r.try_get_by::<i64, _>("record_id"),
                r.try_get_by::<String, _>("timestamp"),
                r.try_get_by::<String, _>("log_type"),
                r.try_get_by::<String, _>("content"),
                r.try_get_by::<Option<String>, _>("metadata"),
            ) else {
                continue;
            };
            map.entry(record_id)
                .or_default()
                .push(Self::parsed_log_entry_from_parts(timestamp, log_type, content, metadata.as_deref()));
        }
        Ok(map)
    }

    /// 获取所有执行日志（用于 WebSocket 同步等场景，请谨慎使用）
    pub async fn get_all_execution_logs(
        &self,
        record_id: i64,
    ) -> Result<Vec<ParsedLogEntry>, sea_orm::DbErr> {
        let (logs, _) = self
            .get_execution_logs(record_id, 1, i64::MAX)
            .await?;
        Ok(logs)
    }

    // 093：原 `get_all_execution_logs_for_records`（全量读回 + 内存截尾）已删除。
    // WS 重连 Sync 路径改用 `count_execution_logs_for_records`（聚合计数）
    // + `get_tail_execution_logs_for_records`（窗口函数取尾部 cap 条）组合，
    // 长跑任务重连不再全量读表。全量读取场景请显式使用分页 `get_execution_logs`。

    /// 根据 session_id 获取所有执行记录（按 started_at 排序）
    pub async fn get_execution_records_by_session(
        &self,
        session_id: &str,
        workspace_id: Option<i64>,
    ) -> Result<Vec<ExecutionRecord>, sea_orm::DbErr> {
        // V1 隔离：同一 session 可能含跨 ws 记录，按 record 自身归属过滤只留本 ws。
        // 用 execution_records.workspace_id（v89 新列）而非经 todos 子查询间接关联——
        // 060 讨论执行完成会软删 carrier todo，旧子查询过滤 deleted_at 导致软删后
        // 查不到本 session 记录（帖子页「暂无执行记录」）。record 直接归属与 todo 生命周期解耦。
        let filter = execution_records::Column::SessionId.eq(session_id);
        let filter = if let Some(wid) = workspace_id {
            filter.and(execution_records::Column::WorkspaceId.eq(wid))
        } else {
            filter
        };
        Ok(execution_records::Entity::find()
            .filter(filter)
            .order_by_asc(execution_records::Column::StartedAt)
            .all(&self.conn)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub async fn get_dashboard_stats(
        &self,
        hours: Option<u32>,
    ) -> Result<crate::models::DashboardStats, sea_orm::DbErr> {
        let ctx = self.build_dashboard_query_context(hours);
        let raw = self.fetch_dashboard_raw_stats(&ctx).await?;
        Ok(self.build_dashboard_stats(raw, &ctx.time_filter).await)
    }

    /// 构建 Dashboard 查询上下文（包含过滤条件、DB backend 等共享参数）。
    fn build_dashboard_query_context(&self, hours: Option<u32>) -> crate::db::dashboard::DashboardQueryContext<'_> {
        let backend = self.conn.get_database_backend();
        // default 30 days = 720 hours (matches frontend)
        let hours = hours.unwrap_or(720);
        // 096-W2：cutoff 预计算为 T/Z ISO 字面量（与存储格式同构，字典序=时间序），
        // 供各统计查询 `started_at >= ?` 参数绑定——裸列比较保持索引命中。
        let time_cutoff = crate::models::utc_timestamp_minus_hours(hours);
        // 热力图固定范围：当年 1 月 1 日起（不受 hours 过滤影响），同构 T/Z 字面量
        let heatmap_cutoff = format!("{}-01-01T00:00:00.000Z", chrono::Utc::now().format("%Y"));
        // skills 统计仍消费 datetime('now') 表达式（存量口径，精度退化问题见 ctx 字段注释）
        let time_filter = format!("datetime('now', '-{} hours')", hours);
        crate::db::dashboard::DashboardQueryContext {
            conn: &self.conn,
            backend,
            time_cutoff,
            heatmap_cutoff,
            time_filter,
        }
    }

    /// 第一阶段：并行拉取 Dashboard 所需的全部原始统计数据。
    ///
    /// 关于 &self.conn 的并发安全性：
    /// self.conn 是 sea_orm::DatabaseConnection（PR #477 后底层是 sqlx Pool，
    /// max_connections=10），传递 &self.conn 给每个 fetch_* 函数，
    /// fetch_* 内部调用 query_all() 时会从池中 acquire() 不同连接。
    /// 多个 future 在 tokio::try_join! 中交错执行时，pool 调度确保
    /// 同一连接不会被两个查询同时占用，因此这里真正能做到并行查询。
    async fn fetch_dashboard_raw_stats(
        &self,
        ctx: &crate::db::dashboard::DashboardQueryContext<'_>,
    ) -> Result<crate::db::dashboard::RawDashboardStats, sea_orm::DbErr> {
        let base = self.fetch_dashboard_base_stats(ctx).await?;
        let dist = self
            .fetch_dashboard_distribution_stats(ctx, &base.executor_todo_counts, &base.tags)
            .await?;
        Ok(crate::db::dashboard::assemble_raw_dashboard_stats(base, dist))
    }

    /// 第一轮并行查询：todo 状态、execution 总体、executor todo 计数、tags 列表。
    async fn fetch_dashboard_base_stats(
        &self,
        ctx: &crate::db::dashboard::DashboardQueryContext<'_>,
    ) -> Result<crate::db::dashboard::BaseStats, sea_orm::DbErr> {
        let (todo_stats, execution_overall, executor_todo_counts, tags) = tokio::try_join!(
            crate::db::dashboard::fetch_todo_stats(ctx),
            crate::db::dashboard::fetch_execution_overall(ctx),
            crate::db::dashboard::fetch_executor_todo_counts(ctx),
            self.get_tags(),
        )?;
        Ok(crate::db::dashboard::BaseStats {
            todo_stats,
            execution_overall,
            executor_todo_counts,
            tags,
        })
    }

    /// 第二轮并行查询：11 个独立分布查询 + tag_distribution 派生查询。
    ///
    /// 依赖第一轮的 `executor_todo_counts` 和 `tags`：前者用于
    /// `fetch_executor_distribution`，后者用于 `fetch_tag_distribution`。
    async fn fetch_dashboard_distribution_stats(
        &self,
        ctx: &crate::db::dashboard::DashboardQueryContext<'_>,
        executor_todo_counts: &std::collections::HashMap<String, i64>,
        tags: &[crate::models::Tag],
    ) -> Result<crate::db::dashboard::DistributionStats, sea_orm::DbErr> {
        let heatmap_limit = 366; // 闰年最多366天
        // 第一步：11 个独立 GROUP BY 查询并行执行
        let (
            executor_distribution,
            model_distribution,
            trigger_type_distribution,
            executor_duration_stats,
            model_cache_stats,
            daily_stats,
            recent_executions,
            execution_change,
            success_rate_change,
            cost_change,
            tag_todo_counts,
        ) = tokio::try_join!(
            crate::db::dashboard::fetch_executor_distribution(ctx, executor_todo_counts),
            crate::db::dashboard::fetch_model_distribution(ctx),
            crate::db::dashboard::fetch_trigger_distribution(ctx),
            crate::db::dashboard::fetch_executor_durations(ctx),
            crate::db::dashboard::fetch_model_cache_stats(ctx),
            crate::db::dashboard::fetch_daily_stats(ctx, heatmap_limit),
            crate::db::dashboard::fetch_recent_executions(ctx),
            crate::db::dashboard::fetch_execution_change(ctx),
            crate::db::dashboard::fetch_success_rate_change(ctx),
            crate::db::dashboard::fetch_cost_change(ctx),
            crate::db::dashboard::fetch_tag_todo_counts(ctx),
        )?;

        // 第二步：派生 tag_distribution（依赖 tags + tag_todo_counts）
        let tag_distribution = crate::db::dashboard::fetch_tag_distribution(
            ctx, tags, &tag_todo_counts,
        ).await?;
        Ok(crate::db::dashboard::DistributionStats {
            executor_distribution,
            model_distribution,
            trigger_type_distribution,
            executor_duration_stats,
            model_cache_stats,
            daily_stats,
            recent_executions,
            execution_change,
            success_rate_change,
            cost_change,
            tag_distribution,
        })
    }

    /// 第二阶段：基于原始统计数据计算派生字段、组装 `DashboardStats`。
    async fn build_dashboard_stats(
        &self,
        raw: crate::db::dashboard::RawDashboardStats,
        time_filter: &str,
    ) -> crate::models::DashboardStats {
        let derived = crate::db::dashboard::compute_dashboard_derived(&raw);
        let (skills_stats, backup_stats) =
            self.load_skills_and_backup_stats(time_filter).await;
        crate::db::dashboard::assemble_dashboard_response(raw, derived, skills_stats, backup_stats)
    }

    /// 软失败加载 Skills/Backup 统计。
    async fn load_skills_and_backup_stats(
        &self,
        time_filter: &str,
    ) -> (
        Option<crate::models::SkillsStats>,
        Option<crate::models::BackupStats>,
    ) {
        let skills_stats = match self.get_skills_stats(time_filter).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Failed to load skills stats: {}", e);
                None
            }
        };
        let backup_stats = match self.get_backup_stats().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Failed to load backup stats: {}", e);
                None
            }
        };
        (skills_stats, backup_stats)
    }

    pub async fn get_execution_summary(
        &self,
        todo_id: i64,
    ) -> Result<ExecutionSummary, sea_orm::DbErr> {
        let backend = self.conn.get_database_backend();
        let row = self
            .conn
            .query_one(Statement::from_sql_and_values(
                backend,
                EXECUTION_SUMMARY_SQL,
                [todo_id.into()],
            ))
            .await?;
        Ok(match row {
            Some(r) => Self::parse_summary_row(todo_id, &r),
            None => Self::empty_summary(todo_id),
        })
    }

    /// 从单行查询结果解析为 `ExecutionSummary`。
    fn parse_summary_row(todo_id: i64, row: &sea_orm::QueryResult) -> ExecutionSummary {
        let total_executions: i64 = row.try_get_by("total").unwrap_or(0);
        let success_count: i64 = row.try_get_by("success_count").unwrap_or(0);
        let failed_count: i64 = row.try_get_by("failed_count").unwrap_or(0);
        let running_count: i64 = row.try_get_by("running_count").unwrap_or(0);
        let input_tokens: i64 = row.try_get_by("input_tokens").unwrap_or(0);
        let output_tokens: i64 = row.try_get_by("output_tokens").unwrap_or(0);
        let cache_read: i64 = row.try_get_by("cache_read").unwrap_or(0);
        let cache_creation: i64 = row.try_get_by("cache_creation").unwrap_or(0);
        let total_cost: f64 = row.try_get_by("total_cost").unwrap_or(0.0);

        ExecutionSummary {
            todo_id,
            total_executions,
            success_count,
            failed_count,
            running_count,
            total_input_tokens: input_tokens as u64,
            total_output_tokens: output_tokens as u64,
            total_cache_read_tokens: cache_read as u64,
            total_cache_creation_tokens: cache_creation as u64,
            total_cost_usd: if total_cost > 0.0 { Some(total_cost) } else { None },
        }
    }

    /// 查询无结果时返回的全零 `ExecutionSummary`。
    fn empty_summary(todo_id: i64) -> ExecutionSummary {
        ExecutionSummary {
            todo_id,
            total_executions: 0,
            success_count: 0,
            failed_count: 0,
            running_count: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_read_tokens: 0,
            total_cache_creation_tokens: 0,
            total_cost_usd: None,
        }
    }

    /// 查询所有 status='running' 的执行记录（包括僵尸记录）
    pub async fn get_running_execution_records(
        &self,
        workspace_id: Option<i64>,
    ) -> Result<Vec<ExecutionRecord>, sea_orm::DbErr> {
        let filter = execution_records::Column::Status.eq("running");
        let filter = if let Some(wid) = workspace_id {
            filter.and(execution_records::Column::TodoId.in_subquery(
                workspace_todo_subquery(wid),
            ))
        } else {
            filter
        };
        let models = execution_records::Entity::find()
            .filter(filter)
            .order_by_desc(execution_records::Column::StartedAt)
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(Into::into).collect())
    }

    /// 查询指定 todo_id 下 status='running' 的执行记录
    pub async fn get_running_records_by_todo_id(
        &self,
        todo_id: i64,
    ) -> Result<Vec<ExecutionRecord>, sea_orm::DbErr> {
        let models = execution_records::Entity::find()
            .filter(execution_records::Column::Status.eq("running"))
            .filter(execution_records::Column::TodoId.eq(todo_id))
            .order_by_desc(execution_records::Column::StartedAt)
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(Into::into).collect())
    }

    /// 按 workspace 查运行中的执行记录（act:/stop 停止用）。
    /// execution_records 无 workspace_id，经 todos 间接关联：先取该 workspace 的 todo_id，
    /// 再按 todo_id IN + status=running 过滤。返回第一条运行中的记录。
    pub async fn get_running_records_by_workspace(
        &self,
        workspace_id: i64,
    ) -> Result<Vec<ExecutionRecord>, sea_orm::DbErr> {
        let todo_ids: Vec<i64> = todos::Entity::find()
            .filter(todos::Column::WorkspaceId.eq(workspace_id))
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| t.id)
            .collect();
        if todo_ids.is_empty() {
            return Ok(Vec::new());
        }
        let models = execution_records::Entity::find()
            .filter(execution_records::Column::TodoId.is_in(todo_ids))
            .filter(execution_records::Column::Status.eq("running"))
            .order_by_desc(execution_records::Column::StartedAt)
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(Into::into).collect())
    }

    /// 强制将一条执行记录标记为失败（用于僵尸记录清理）
    pub async fn force_fail_execution_record(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(id),
            status: ActiveValue::Set(Some("failed".to_string())),
            finished_at: ActiveValue::Set(Some(now)),
            result: ActiveValue::Set(Some("手动终止".to_string())),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 清理孤儿执行记录：状态为running但todo没有对应task_id的记录
    /// 程序崩溃后，执行记录可能保持running状态，需要修复
    pub async fn cleanup_orphan_execution_records(&self) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let backend = self.conn.get_database_backend();
        let sql = "UPDATE execution_records SET \
                status = 'failed', \
                finished_at = $1, \
                result = CASE \
                    WHEN todo_id NOT IN (SELECT id FROM todos WHERE deleted_at IS NULL) THEN '任务已被删除' \
                    ELSE '程序崩溃，任务被中断' \
                END \
                WHERE status = 'running' AND ( \
                    todo_id NOT IN (SELECT id FROM todos WHERE deleted_at IS NULL) \
                    OR todo_id IN (SELECT id FROM todos WHERE task_id IS NULL AND deleted_at IS NULL) \
                )";
        let res = self
            .conn
            .execute(Statement::from_sql_and_values(backend, sql, [now.into()]))
            .await?;
        let rows = res.rows_affected();
        if rows > 0 {
            tracing::info!("Cleaned up {} orphan execution records", rows);
        }
        Ok(())
    }

    /// Get skills invocation statistics
    async fn get_skills_stats(
        &self,
        time_filter: &str,
    ) -> Result<Option<crate::models::SkillsStats>, sea_orm::DbErr> {
        let backend = self.conn.get_database_backend();

        // 第一阶段：并行拉取 4 类原始数据
        let (overall, top_skills, executor_skills_count, daily_invocations) = tokio::try_join!(
            Self::fetch_skills_overall(backend, &self.conn, time_filter),
            Self::fetch_top_skills(backend, &self.conn, time_filter),
            Self::fetch_executor_skills_count(backend, &self.conn, time_filter),
            Self::fetch_daily_skill_invocations(backend, &self.conn, time_filter),
        )?;

        // 若无任何调用记录，整体短路返回 None（与重构前语义一致）
        if overall.total == 0 {
            return Ok(None);
        }

        Ok(Some(Self::build_skills_response(
            &overall,
            top_skills,
            executor_skills_count,
            daily_invocations,
        )))
    }

    /// 查询 skills 总体统计：(总数, 成功, 失败, 平均时长, 今日数)。
    async fn fetch_skills_overall(
        backend: sea_orm::DbBackend,
        conn: &sea_orm::DatabaseConnection,
        time_filter: &str,
    ) -> Result<SkillsOverallRow, sea_orm::DbErr> {
        let sql = format!(
            "SELECT \
            COUNT(*) as total, \
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success, \
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed, \
            COALESCE(AVG(duration_ms), 0) as avg_duration, \
            COALESCE(SUM(CASE WHEN date(invoked_at) = date('now') THEN 1 ELSE 0 END), 0) as today \
            FROM skill_invocations \
            WHERE invoked_at >= {}",
            time_filter
        );
        let row = conn.query_one(Statement::from_string(backend, sql)).await?;
        Ok(match row {
            Some(r) => SkillsOverallRow {
                total: r.try_get_by::<i64, _>("total").unwrap_or(0),
                success: r.try_get_by::<i64, _>("success").unwrap_or(0),
                failed: r.try_get_by::<i64, _>("failed").unwrap_or(0),
                avg_duration_ms: r.try_get_by::<f64, _>("avg_duration").unwrap_or(0.0),
                today: r.try_get_by::<i64, _>("today").unwrap_or(0),
            },
            None => SkillsOverallRow::default(),
        })
    }

    /// 查询调用次数 Top 10 skills。
    async fn fetch_top_skills(
        backend: sea_orm::DbBackend,
        conn: &sea_orm::DatabaseConnection,
        time_filter: &str,
    ) -> Result<Vec<crate::models::SkillTop>, sea_orm::DbErr> {
        let sql = format!(
            "SELECT skill_name, COUNT(*) as count, \
            CAST(COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) AS FLOAT) / COUNT(*) * 100 as success_rate \
            FROM skill_invocations \
            WHERE invoked_at >= {} \
            GROUP BY skill_name \
            ORDER BY count DESC LIMIT 10",
            time_filter
        );
        Ok(conn
            .query_all(Statement::from_string(backend, sql))
            .await?
            .into_iter()
            .filter_map(|row| {
                let skill_name: String = row.try_get_by("skill_name").ok()?;
                let count: i64 = row.try_get_by("count").ok()?;
                let success_rate: f64 = row.try_get_by("success_rate").ok()?;
                Some(crate::models::SkillTop { skill_name, count, success_rate })
            })
            .collect())
    }

    /// 查询每个执行器调用过的不同 skill 数量。
    async fn fetch_executor_skills_count(
        backend: sea_orm::DbBackend,
        conn: &sea_orm::DatabaseConnection,
        time_filter: &str,
    ) -> Result<Vec<crate::models::ExecutorSkillCount>, sea_orm::DbErr> {
        let sql = format!(
            "SELECT executor, COUNT(DISTINCT skill_name) as skills_count \
            FROM skill_invocations \
            WHERE invoked_at >= {} \
            GROUP BY executor",
            time_filter
        );
        Ok(conn
            .query_all(Statement::from_string(backend, sql))
            .await?
            .into_iter()
            .filter_map(|row| {
                let executor: String = row.try_get_by("executor").ok()?;
                let skills_count: i64 = row.try_get_by("skills_count").ok()?;
                Some(crate::models::ExecutorSkillCount { executor, skills_count })
            })
            .collect())
    }

    /// 查询最近 30 天的每日 skill 调用次数。
    async fn fetch_daily_skill_invocations(
        backend: sea_orm::DbBackend,
        conn: &sea_orm::DatabaseConnection,
        time_filter: &str,
    ) -> Result<Vec<crate::models::DailySkillInvocation>, sea_orm::DbErr> {
        let sql = format!(
            "SELECT SUBSTR(COALESCE(invoked_at, ''), 1, 10) as day, \
            COUNT(*) as count, \
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success \
            FROM skill_invocations \
            WHERE invoked_at IS NOT NULL AND LENGTH(invoked_at) >= 10 AND invoked_at >= {} \
            GROUP BY SUBSTR(invoked_at, 1, 10) \
            ORDER BY day DESC LIMIT 30",
            time_filter
        );
        Ok(conn
            .query_all(Statement::from_string(backend, sql))
            .await?
            .into_iter()
            .filter_map(|row| {
                let date: String = row.try_get_by("day").ok()?;
                let count: i64 = row.try_get_by("count").ok()?;
                let success: i64 = row.try_get_by("success").ok()?;
                Some(crate::models::DailySkillInvocation { date, count, success })
            })
            .collect())
    }

    /// 组装 `SkillsStats` 响应结构体。
    // 整体统计为小结构体，按值传递语义更清晰且成本极低
    fn build_skills_response(
        overall: &SkillsOverallRow,
        top_skills: Vec<crate::models::SkillTop>,
        executor_skills_count: Vec<crate::models::ExecutorSkillCount>,
        daily_invocations: Vec<crate::models::DailySkillInvocation>,
    ) -> crate::models::SkillsStats {
        crate::models::SkillsStats {
            total_invocations: overall.total,
            success_invocations: overall.success,
            failed_invocations: overall.failed,
            avg_duration_ms: overall.avg_duration_ms,
            invocations_today: overall.today,
            top_skills,
            executor_skills_count,
            daily_invocations,
        }
    }

    /// Get backup statistics by scanning filesystem
    async fn get_backup_stats(&self) -> Result<Option<crate::models::BackupStats>, sea_orm::DbErr> {
        let backup_dir = dirs::home_dir()
            .map(|h| h.join(".ntd").join("backups"))
            .unwrap_or_else(|| std::path::PathBuf::from(".ntd/backups"));

        if !backup_dir.exists() {
            return Ok(None);
        }

        // 三个分类目录相互独立，逐个扫描。
        // 这里保留同步调用：每个 scan 是单次目录 read_dir，文件量级在百以内，
        // 引入 spawn_blocking 反而增加跨线程调度成本，违背 YAGNI。
        let database_stats = Self::scan_backup_category(&backup_dir.join("db"));
        let todo_stats = Self::scan_backup_category(&backup_dir.join("todo"));
        let skills_stats = Self::scan_backup_category(&backup_dir.join("skills"));
        let recent_backups = Self::collect_recent_backups(&backup_dir);

        let (total_file_count, total_size) = Self::aggregate_backup_totals([
            &database_stats, &todo_stats, &skills_stats,
        ]);
        let last_backup = recent_backups.first().map(|b| b.created_at.clone());
        let total_size_formatted = Self::format_bytes(total_size as u64);

        Ok(Some(crate::models::BackupStats {
            auto_backup_enabled: false,
            last_backup,
            auto_backup_cron: String::new(),
            database: database_stats,
            todo: todo_stats,
            skills: skills_stats,
            total_file_count,
            total_size,
            total_size_formatted,
            recent_backups,
        }))
    }

    /// 聚合三个分类的 (file_count, total_size)。
    fn aggregate_backup_totals(
        categories: [&crate::models::BackupCategoryStats; 3],
    ) -> (i64, i64) {
        let total_file_count = categories.iter().map(|c| c.file_count).sum();
        let total_size = categories.iter().map(|c| c.total_size).sum();
        (total_file_count, total_size)
    }

    /// 收集所有 backup 子目录（db/todo/skills）的最近 5 条，
    /// 合并后按时间排序并截断到前 10 条。
    fn collect_recent_backups(
        backup_dir: &std::path::Path,
    ) -> Vec<crate::models::RecentBackup> {
        let buckets = [
            ("database", Self::collect_backup_files(&backup_dir.join("db"))),
            ("todo", Self::collect_backup_files(&backup_dir.join("todo"))),
            ("skills", Self::collect_backup_files(&backup_dir.join("skills"))),
        ];
        Self::merge_recent_backup_buckets(&buckets)
    }

    /// 把三个分类的 recent 文件合并为统一排序的列表，截断到前 10 条。
    fn merge_recent_backup_buckets(
        buckets: &[(&str, Option<Vec<crate::models::RecentBackup>>); 3],
    ) -> Vec<crate::models::RecentBackup> {
        let mut recent_backups: Vec<crate::models::RecentBackup> = Vec::new();
        for (backup_type, files) in buckets {
            if let Some(files) = files {
                for f in files.iter().take(5) {
                    recent_backups.push(crate::models::RecentBackup {
                        backup_type: (*backup_type).to_string(),
                        name: f.name.clone(),
                        size: f.size,
                        created_at: f.created_at.clone(),
                    });
                }
            }
        }
        recent_backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        recent_backups.truncate(10);
        recent_backups
    }

    /// Scan a backup category directory and return stats
    fn scan_backup_category(dir: &std::path::Path) -> crate::models::BackupCategoryStats {
        use std::fs;

        if !dir.exists() {
            return crate::models::BackupCategoryStats {
                file_count: 0,
                total_size: 0,
                last_backup: None,
            };
        }

        let mut file_count = 0i64;
        let mut total_size = 0i64;
        let mut last_backup: Option<String> = None;

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        file_count += 1;
                        total_size += metadata.len() as i64;

                        if let Ok(modified) = metadata.modified() {
                            let modified_str = Self::system_time_to_iso_string(modified);
                            // 用 as_ref().map() 避免 unwrap()：last_backup 为 None 时直接替换
                            if last_backup.as_ref().map_or(true, |latest| modified_str > *latest) {
                                last_backup = Some(modified_str);
                            }
                        }
                    }
                }
            }
        }

        crate::models::BackupCategoryStats {
            file_count,
            total_size,
            last_backup,
        }
    }

    /// Collect backup files from a directory
    fn collect_backup_files(dir: &std::path::Path) -> Option<Vec<crate::models::RecentBackup>> {
        use std::fs;

        if !dir.exists() {
            return None;
        }

        let mut files: Vec<crate::models::RecentBackup> = Vec::new();

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let size = metadata.len() as i64;
                        // ok() 转换 Option 后直接 map，无需额外闭包包装
                        let created_at = metadata.modified().ok()
                            .map(Self::system_time_to_iso_string)
                            .unwrap_or_default();

                        files.push(crate::models::RecentBackup {
                            backup_type: String::new(), // Will be set by caller
                            name,
                            size,
                            created_at,
                        });
                    }
                }
            }
        }

        // Sort by created_at descending (newest first)
        files.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Some(files)
    }

    /// Convert SystemTime to ISO string
    fn system_time_to_iso_string(time: std::time::SystemTime) -> String {
        use chrono::{DateTime, Utc};
        let datetime: DateTime<Utc> = time.into();
        datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    }

    /// Format bytes to human readable string
    fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    // ===== 自动评审辅助方法 =====

    /// 写入/更新原执行记录的 last_review_status 字段（pending/success/failed/interrupted/skipped）.
    pub async fn set_record_last_review_status(
        &self,
        record_id: i64,
        status: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(record_id),
            last_review_status: ActiveValue::Set(Some(status.to_string())),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 写入/更新原执行记录的 last_reviewed_at 字段（UTC ISO8601）.
    pub async fn set_record_last_reviewed_at(
        &self,
        record_id: i64,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(record_id),
            last_reviewed_at: ActiveValue::Set(Some(crate::models::utc_timestamp())),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 评审实例完成时调用: 把评审实例的 source_execution_record_id 指向"原那条",
    /// 并把 last_review_status 设为终态 (success/failed/interrupted).
    /// 同步更新原记录的 last_review_status.
    pub async fn link_review_to_source(
        &self,
        review_record_id: i64,
        source_record_id: i64,
        final_status: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let am = execution_records::ActiveModel {
            id: ActiveValue::Unchanged(review_record_id),
            source_execution_record_id: ActiveValue::Set(Some(source_record_id)),
            last_review_status: ActiveValue::Set(Some(final_status.to_string())),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 反查评审 record：`source_execution_record_id` 指向原 step record（需求 047）。
    ///
    /// 执行历史展示「评分来源」：给定原 step 的 execution_record_id，找到评审它的评审实例
    /// record（其 `result` 含评审理由 + `RATING`、`rating` 是评分）。走已有索引
    /// `idx_execution_records_source_record_id`。返工重跑可能产生多条，取 id 最大（最新一次评审）。
    pub async fn find_review_record_id_by_source(
        &self,
        source_record_id: i64,
    ) -> Result<Option<i64>, sea_orm::DbErr> {
        let row = execution_records::Entity::find()
            .filter(execution_records::Column::SourceExecutionRecordId.eq(source_record_id))
            .order_by_desc(execution_records::Column::Id)
            .one(&self.conn)
            .await?;
        Ok(row.map(|m| m.id))
    }

    /// 批量反查评审 record：给定一批「原 step 的 execution_record_id」，找每个对应的最新评审实例 id。
    /// 单次 IN 查询（按 id 倒序）+ Rust 端按 source 取首条，消除逐 step 的 N+1（091 性能优化）。
    /// 返回 `source_record_id -> 评审 record id`；无评审的 source 不出现在 map 中（调用方视作 None）。
    pub async fn find_review_record_id_by_source_batch(
        &self,
        source_record_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, sea_orm::DbErr> {
        use std::collections::HashMap;
        if source_record_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = execution_records::Entity::find()
            .filter(execution_records::Column::SourceExecutionRecordId.is_in(source_record_ids.to_vec()))
            .order_by_desc(execution_records::Column::Id)
            .all(&self.conn)
            .await?;
        let mut latest: HashMap<i64, i64> = HashMap::new();
        for row in rows {
            // 倒序遍历：首个出现的 source 即其最新评审 record（id 最大），后续同 source 跳过。
            if let Some(src) = row.source_execution_record_id {
                latest.entry(src).or_insert(row.id);
            }
        }
        Ok(latest)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use crate::models::{BackupCategoryStats, ExecutionUsage, RecentBackup};

    #[test]
    fn test_serialize_usage_json_some_returns_valid_json() {
        let usage = ExecutionUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: Some(10),
            cache_creation_input_tokens: Some(0),
            total_cost_usd: Some(0.01),
            duration_ms: Some(1000),
        };
        let json = Database::serialize_usage_json(Some(&usage)).expect("usage Some");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse roundtrip");
        assert_eq!(parsed["input_tokens"], serde_json::json!(100));
        assert_eq!(parsed["output_tokens"], serde_json::json!(50));
    }

    #[test]
    fn test_serialize_usage_json_none_returns_none() {
        assert!(Database::serialize_usage_json(None).is_none());
    }

    #[test]
    fn test_empty_summary_returns_all_zeros_with_todo_id() {
        let s = Database::empty_summary(42);
        assert_eq!(s.todo_id, 42);
        assert_eq!(s.total_executions, 0);
        assert_eq!(s.success_count, 0);
        assert_eq!(s.failed_count, 0);
        assert_eq!(s.running_count, 0);
        assert_eq!(s.total_input_tokens, 0);
        assert_eq!(s.total_output_tokens, 0);
        assert_eq!(s.total_cache_read_tokens, 0);
        assert_eq!(s.total_cache_creation_tokens, 0);
        assert!(s.total_cost_usd.is_none());
    }

    #[test]
    fn test_aggregate_backup_totals_sums_all_three_categories() {
        let db = BackupCategoryStats { file_count: 10, total_size: 1024, last_backup: None };
        let todo = BackupCategoryStats { file_count: 5, total_size: 2048, last_backup: None };
        let skills = BackupCategoryStats { file_count: 3, total_size: 4096, last_backup: None };
        let (count, size) = Database::aggregate_backup_totals([&db, &todo, &skills]);
        assert_eq!(count, 18);
        assert_eq!(size, 7168);
    }

    #[test]
    fn test_aggregate_backup_totals_empty_categories() {
        let zero = BackupCategoryStats { file_count: 0, total_size: 0, last_backup: None };
        let (count, size) = Database::aggregate_backup_totals([&zero, &zero, &zero]);
        assert_eq!(count, 0);
        assert_eq!(size, 0);
    }

    #[test]
    fn test_merge_recent_backup_buckets_sorts_and_truncates_to_ten() {
        let make = |prefix: &str, n: i64| -> Vec<RecentBackup> {
            (0..n).map(|i| RecentBackup {
                backup_type: String::new(),
                name: format!("{}-{}", prefix, i),
                size: 100,
                created_at: format!("2026-06-18T10:00:{:02}Z", i),
            }).collect()
        };
        let buckets = [
            ("database", Some(make("db", 5))),
            ("todo", Some(make("todo", 5))),
            ("skills", Some(make("sk", 5))),
        ];
        let merged = Database::merge_recent_backup_buckets(&buckets);
        assert_eq!(merged.len(), 10);
        assert!(merged[0].created_at >= merged[9].created_at);
        assert!(merged.iter().all(|b| !b.backup_type.is_empty()));
    }

    #[test]
    fn test_merge_recent_backup_buckets_handles_none_inputs() {
        let buckets: [(&str, Option<Vec<RecentBackup>>); 3] = [
            ("database", None),
            ("todo", Some(vec![RecentBackup {
                backup_type: String::new(),
                name: "only".into(),
                size: 1,
                created_at: "2026-06-18T10:00:00Z".into(),
            }])),
            ("skills", None),
        ];
        let merged = Database::merge_recent_backup_buckets(&buckets);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].backup_type, "todo");
    }

    /// 校验 `build_update_statement` 在两个分支下的占位符顺序与数量。
    #[test]
    fn test_build_update_statement_normal_branch() {
        let req = UpdateExecutionRecordRequest {
            id: 7,
            status: "success",
            remaining_logs: "[]",
            result: "ok",
            usage: None,
            model: Some("claude"),
            review_meta: None,
        };
        let (sql, values) = Database::build_update_statement(
            &req, "2026-06-18T10:00:00Z".to_string(), None, Some("claude".to_string()),
        );
        assert!(sql.contains("$6"));
        assert!(!sql.contains("source_execution_record_id"));
        assert_eq!(values.len(), 6);
    }

    #[test]
    fn test_build_update_statement_review_branch() {
        let req = UpdateExecutionRecordRequest {
            id: 7,
            status: "success",
            remaining_logs: "[]",
            result: "ok",
            usage: None,
            model: Some("claude"),
            review_meta: Some((100, "success")),
        };
        let (sql, values) = Database::build_update_statement(
            &req, "2026-06-18T10:00:00Z".to_string(), None, Some("claude".to_string()),
        );
        assert!(sql.contains("source_execution_record_id"));
        assert!(sql.contains("last_review_status"));
        assert!(sql.contains("$9"));
        assert_eq!(values.len(), 9);
    }
}

/// 事项中心聚合查询测试：连续失败次数 + webhook 最近触发时间。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod center_aggregate_tests {
    use super::*;
    use crate::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 插一条 todo 并返回 id。
    async fn seed_todo(db: &Database, title: &str) -> i64 {
        db.exec(&format!(
            "INSERT INTO todos (title, prompt, status) VALUES ('{title}', 'p', 'pending')"
        ))
        .await
        .expect("insert todo");
        let row = db
            .conn
            .query_one(Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT id FROM todos WHERE title = '{title}'"),
            ))
            .await
            .expect("query id")
            .expect("row exists");
        row.try_get_by_index::<i64>(0).expect("id readable")
    }

    /// 插一条执行记录（trigger_type / status / todo_id 可指定），id 自增。
    async fn seed_exec(db: &Database, todo_id: i64, status: &str, trigger: &str) {
        db.exec(&format!(
            "INSERT INTO execution_records (todo_id, status, trigger_type, started_at, finished_at) \
             VALUES ({todo_id}, '{status}', '{trigger}', '2026-07-08T09:00:00Z', '2026-07-08T09:01:00Z')"
        ))
        .await
        .expect("insert exec");
    }

    /// 093：插一条执行日志，content 可指定（断言尾部内容用），metadata 给 NULL 走默认降级路径。
    async fn seed_log(db: &Database, record_id: i64, content: &str) {
        db.exec(&format!(
            "INSERT INTO execution_logs (record_id, timestamp, log_type, content) \
             VALUES ({record_id}, '2026-07-08T09:00:00Z', 'info', '{content}')"
        ))
        .await
        .expect("insert log");
    }

    /// 093-B5：delete_execution_logs_before——参数化删除 + rows_affected 返回真实行数。
    #[tokio::test]
    async fn test_delete_execution_logs_before_cutoff() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        seed_exec(&db, t, "success", "manual").await; // record id=1
        // 造 2 旧 1 新三条日志：验证边界「早于 cutoff 才删」两侧都覆盖到
        db.exec("INSERT INTO execution_logs (record_id, timestamp, log_type, content) VALUES (1, '2026-07-01T00:00:00Z', 'info', 'old1')").await.unwrap();
        db.exec("INSERT INTO execution_logs (record_id, timestamp, log_type, content) VALUES (1, '2026-07-02T00:00:00Z', 'info', 'old2')").await.unwrap();
        db.exec("INSERT INTO execution_logs (record_id, timestamp, log_type, content) VALUES (1, '2026-08-08T00:00:00Z', 'info', 'new')").await.unwrap();

        let deleted = db.delete_execution_logs_before("2026-08-01T00:00:00Z").await.unwrap();
        assert_eq!(deleted, 2, "应删除两条旧日志");
        // 幂等边界：再删一次为 0；新日志仍在
        assert_eq!(db.delete_execution_logs_before("2026-08-01T00:00:00Z").await.unwrap(), 0);
        let remaining = db.count_execution_logs_for_records(&[1]).await.unwrap();
        assert_eq!(remaining.get(&1).copied(), Some(1), "新日志不应被删");
    }

    /// 095：insert_execution_log_entries——空切片短路（拆出的独立场景，CodeRabbit #1014）。
    #[tokio::test]
    async fn test_insert_execution_log_entries_empty_slice_is_noop() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        seed_exec(&db, t, "running", "manual").await; // record id=1

        // insert_many 对空 Vec 会生成非法 SQL；语义上空入参即无活可干——短路为 Ok
        db.insert_execution_log_entries(1, &[]).await.unwrap();
        let count = db.count_execution_logs_for_records(&[1]).await.unwrap();
        assert_eq!(count.get(&1).copied().unwrap_or(0), 0, "空切片不应插入任何行");
    }

    /// 095：insert_execution_log_entries——多条目落库与 metadata 打包正确性。
    #[tokio::test]
    async fn test_insert_execution_log_entries_packs_metadata() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        seed_exec(&db, t, "running", "manual").await; // record id=1

        // 两条目对照：裸 info（无元数据字段）与带完整工具元数据的 tool_call，
        // 覆盖 metadata 打包的「全空」与「全有」两个极端形态
        let entries = vec![
            crate::models::ParsedLogEntry::info("plain".to_string()),
            crate::models::ParsedLogEntry {
                timestamp: "2026-08-10T10:00:00Z".to_string(),
                log_type: "tool_call".to_string(),
                content: "edit".to_string(),
                usage: None,
                tool_name: Some("edit".to_string()),
                tool_input_json: Some(r#"{"filePath":"/x.rs"}"#.to_string()),
            },
        ];
        db.insert_execution_log_entries(1, &entries).await.unwrap();

        // 行数断言：两条目全部落库
        let count = db.count_execution_logs_for_records(&[1]).await.unwrap();
        assert_eq!(count.get(&1).copied(), Some(2), "两条目应全部落库");
        // 逐列读回断言（_conn_raw 是测试既定的原始连接口子）：
        // metadata 打包内容必须与旧 JSON 路径逐字节一致（usage/tool_name/tool_input_json）
        let rows = db
            ._conn_raw()
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT log_type, content, metadata FROM execution_logs WHERE record_id = 1 ORDER BY id".to_string(),
            ))
            .await
            .unwrap();
        // 裸条目：三字段全 null（serde_json::json! 对 None 输出 null，无 skip）
        let meta_plain: String = rows[0].try_get_by("metadata").unwrap();
        assert_eq!(meta_plain, r#"{"tool_input_json":null,"tool_name":null,"usage":null}"#,
            "裸条目三字段应为 null（与旧 JSON 路径落库内容一致）");
        // 工具条目：tool_name/tool_input_json 实值入 metadata
        let meta_tool: String = rows[1].try_get_by("metadata").unwrap();
        assert!(meta_tool.contains(r#""tool_name":"edit""#), "tool_name 应入 metadata: {meta_tool}");
        assert!(meta_tool.contains("filePath"), "tool_input_json 应入 metadata: {meta_tool}");
        let log_type: String = rows[1].try_get_by("log_type").unwrap();
        assert_eq!(log_type, "tool_call");
    }

    /// 095：JSON 薄壳回归——完整载荷（含 toolName/toolInputJson/usage 的线上 serde 形态）
    /// 经薄壳落库后与对象版逐列一致（CodeRabbit #1014：防 serde 映射漂移）。
    #[tokio::test]
    async fn test_insert_execution_logs_json_shell_full_payload_matches_object_path() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        seed_exec(&db, t, "running", "manual").await; // record id=1

        // 完整 JSON 载荷：serde rename 后的线上形态（type/toolName/toolInputJson + usage 对象）
        let json = r#"[{"timestamp":"2026-08-10T10:00:00Z","type":"tool_call","content":"edit","usage":{"input_tokens":10,"output_tokens":20},"toolName":"edit","toolInputJson":"{\"filePath\":\"/x.rs\"}"}]"#;
        db.insert_execution_logs(1, json).await.unwrap();

        // 同内容对象版落库到另一 record，逐列比对两路径产物
        seed_exec(&db, t, "running", "manual").await; // record id=2
        let entry = crate::models::ParsedLogEntry {
            timestamp: "2026-08-10T10:00:00Z".to_string(),
            log_type: "tool_call".to_string(),
            content: "edit".to_string(),
            usage: Some(crate::models::ExecutionUsage { input_tokens: 10, output_tokens: 20, cache_read_input_tokens: None, cache_creation_input_tokens: None, total_cost_usd: None, duration_ms: None }),
            tool_name: Some("edit".to_string()),
            tool_input_json: Some(r#"{"filePath":"/x.rs"}"#.to_string()),
        };
        db.insert_execution_log_entries(2, &[entry]).await.unwrap();

        // 逐列比较（log_type/content/metadata 三列必须一致）
        let rows = db
            ._conn_raw()
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT record_id, log_type, content, metadata FROM execution_logs WHERE record_id IN (1,2) ORDER BY record_id".to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(rows.len(), 2, "两条路径各落一行");
        let cols = |i: usize| -> (String, String, String) {
            (rows[i].try_get_by("log_type").unwrap(),
             rows[i].try_get_by("content").unwrap(),
             rows[i].try_get_by("metadata").unwrap())
        };
        assert_eq!(cols(0), cols(1), "JSON 薄壳与对象版落库内容必须逐列一致");
    }

    /// 095：JSON 薄壳错误分支——非法 JSON 返回解析错误（存量调用方错误语义保持）。
    #[tokio::test]
    async fn test_insert_execution_logs_json_shell_rejects_invalid_json() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        seed_exec(&db, t, "running", "manual").await; // record id=1
        assert!(db.insert_execution_logs(1, "not-json").await.is_err());
    }

    /// count_execution_logs_for_records：GROUP BY 聚合计数（093 WS 重连 log_total 数据源）。
    /// 覆盖：多 record 各自计数、无日志 record 不出现、空入参返空。
    #[tokio::test]
    async fn test_count_execution_logs_for_records_groups_by_record() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        seed_exec(&db, t, "running", "manual").await; // record id=1
        seed_exec(&db, t, "running", "manual").await; // record id=2
        for i in 0..3 {
            seed_log(&db, 1, &format!("r1-{i}")).await;
        }
        seed_log(&db, 2, "r2-0").await;
        let map = db.count_execution_logs_for_records(&[1, 2, 9999]).await.unwrap();
        assert_eq!(map.get(&1).copied(), Some(3));
        assert_eq!(map.get(&2).copied(), Some(1));
        // COUNT 聚合对无匹配行的 record 不产出分组——调用方以 unwrap_or(0) 读
        assert!(!map.contains_key(&9999), "无日志的 record 不应出现在聚合结果中");
        assert!(
            db.count_execution_logs_for_records(&[]).await.unwrap().is_empty(),
            "空入参应返回空 map"
        );
    }

    /// get_tail_execution_logs_for_records：窗口函数取尾部 cap 条（093 WS 重连日志摘要源）。
    /// 覆盖：cap=2 取尾部 2 条且按 id 升序、跨 record 互不串扰、metadata JSON 正常解析。
    #[tokio::test]
    async fn test_get_tail_execution_logs_for_records_returns_tail_asc() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        seed_exec(&db, t, "running", "manual").await; // record id=1
        seed_exec(&db, t, "running", "manual").await; // record id=2
        for i in 0..5 {
            seed_log(&db, 1, &format!("r1-{i}")).await;
        }
        seed_log(&db, 2, "r2-only").await;
        // 给尾部将命中的一行补上 metadata，验证 JSON 解析经共享 helper 走通
        db.exec(
            "UPDATE execution_logs SET metadata = '{\"tool_name\":\"Bash\"}' \
             WHERE record_id = 1 AND content = 'r1-4'",
        )
        .await
        .expect("update metadata");

        let map = db.get_tail_execution_logs_for_records(&[1, 2], 2).await.unwrap();
        let r1 = map.get(&1).expect("record 1 应有尾部日志");
        // 5 行取尾 2：r1-3、r1-4；外层按 id ASC 归序，与旧内存截尾语义一致
        let contents: Vec<&str> = r1.iter().map(|l| l.content.as_str()).collect();
        assert_eq!(contents, ["r1-3", "r1-4"], "应取尾部 2 条且升序");
        assert_eq!(r1[1].tool_name.as_deref(), Some("Bash"), "metadata 应被解析");
        // record 2 仅 1 行，cap 超过总量时返回全部
        let r2 = map.get(&2).expect("record 2 应有日志");
        assert_eq!(r2.len(), 1, "cap 超过总量应返回全部");
        assert_eq!(r2[0].content, "r2-only", "跨 record 不应串扰");
        assert!(
            db.get_tail_execution_logs_for_records(&[], 2).await.unwrap().is_empty(),
            "空入参应返回空 map"
        );
    }

    /// get_execution_records_by_ids：按 id 批量取执行记录并按 id 索引；
    /// 不存在的 id 不出现，空入参返空 map（091 批量化新增，消除逐 id 查询的 N+1）。
    #[tokio::test]
    async fn test_get_execution_records_by_ids_batch_indexes_by_id() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        seed_exec(&db, t, "success", "manual").await; // id=1
        seed_exec(&db, t, "failed", "manual").await; // id=2
        // 入参故意乱序并夹一个不存在的 id=9999，验证只返回真实存在的记录。
        let map = db.get_execution_records_by_ids(&[2, 1, 9999]).await.unwrap();
        assert_eq!(map.len(), 2, "只返回存在的 id");
        assert!(map.contains_key(&1) && map.contains_key(&2), "存在的 id 都应入 map");
        assert!(!map.contains_key(&9999), "不存在的 id 不应出现");
        assert!(
            db.get_execution_records_by_ids(&[]).await.unwrap().is_empty(),
            "空入参应返回空 map"
        );
    }

    /// find_review_record_id_by_source_batch：批量反查每个 source 的最新评审 record（id 最大）；
    /// 同一 source 多条只取最新，未引用的 source 不出现，空入参返空（091 批量化新增）。
    #[tokio::test]
    async fn test_find_review_record_id_by_source_batch_picks_latest() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        seed_exec(&db, t, "success", "manual").await; // id=1：被评审的原记录
        // 同一 source=1 两条评审 record（id=2、3），batch 应取 id 最大者（最新一次返工）。
        for _ in 0..2 {
            db.exec(
                "INSERT INTO execution_records (todo_id, status, trigger_type, started_at, \
                 source_execution_record_id) \
                 VALUES (1, 'success', 'auto_review', '2026-07-08T09:00:00Z', 1)",
            )
            .await
            .expect("insert review record");
        }
        let map = db.find_review_record_id_by_source_batch(&[1, 9999]).await.unwrap();
        assert_eq!(map.get(&1).copied(), Some(3), "同一 source 取 id 最大（最新）的评审 record");
        assert!(!map.contains_key(&9999), "未引用的 source 不应出现");
        assert!(
            db.find_review_record_id_by_source_batch(&[]).await.unwrap().is_empty(),
            "空入参应返回空 map"
        );
    }

    /// get_latest_execution_summaries_for_todos：批量取每个 todo 最近一条执行记录摘要。
    #[tokio::test]
    async fn test_get_latest_execution_summaries_for_todos_batch() {
        let db = fresh_db().await;
        let t1 = seed_todo(&db, "T1").await;
        let t2 = seed_todo(&db, "T2").await;
        seed_exec(&db, t1, "success", "manual").await;
        seed_exec(&db, t1, "failed", "manual").await; // t1 最近一条
        // t2 无执行记录
        let map = db.get_latest_execution_summaries_for_todos(&[t1, t2]).await.unwrap();
        let s1 = map.get(&t1).expect("t1 应有记录");
        assert_eq!(s1.status.as_deref(), Some("failed"), "应取最近一条");
        // t2 无记录 → 不在 map 中
        assert!(!map.contains_key(&t2));
    }

    /// find_review_record_id_by_source：反查评审 record，多条取最新；不存在返 None（需求 047）。
    #[tokio::test]
    async fn test_find_review_record_id_by_source_returns_latest_and_none() {
        let db = fresh_db().await;
        let todo_id = seed_todo(&db, "T").await;
        // 原记录（被评审的 step record），execution_records 首条 → id=1。
        seed_exec(&db, todo_id, "success", "manual").await;
        let source_id: i64 = 1;

        // 不存在的 source → None。
        assert_eq!(db.find_review_record_id_by_source(9999).await.unwrap(), None);

        // 两条评审 record 指向同一原记录（模拟返工重跑），id 自增为 2、3。
        for _ in 0..2 {
            db.exec(
                "INSERT INTO execution_records (todo_id, status, trigger_type, started_at, \
                 source_execution_record_id) \
                 VALUES (1, 'success', 'auto_review', '2026-07-08T09:00:00Z', 1)",
            )
            .await
            .expect("insert review record");
        }
        // 多条 → order_by_desc(Id) 取最新一次评审（id=3）。
        assert_eq!(
            db.find_review_record_id_by_source(source_id)
                .await
                .unwrap(),
            Some(3)
        );
    }

    /// update_execution_record_agent_runs：写入 agent_runs JSON 后能按 id 原样读回（CodeRabbit）。
    #[tokio::test]
    async fn test_update_execution_record_agent_runs_roundtrip() {
        let db = fresh_db().await;
        let todo_id = seed_todo(&db, "T").await;
        // 插一条执行记录并取自增 id（update 方法按 id 定位）。
        db.exec(&format!(
            "INSERT INTO execution_records (todo_id, status, trigger_type, started_at) \
             VALUES ({todo_id}, 'success', 'manual', '2026-07-18T03:48:20Z')"
        ))
        .await
        .expect("insert exec");
        let row = db
            .conn
            .query_one(Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT id FROM execution_records ORDER BY id DESC LIMIT 1".to_string(),
            ))
            .await
            .expect("query id")
            .expect("row exists");
        let id: i64 = row.try_get_by_index(0).expect("id readable");

        let json = r#"[{"name":"张三丰","role":"general","status":"completed"}]"#;
        db.update_execution_record_agent_runs(id, json)
            .await
            .expect("update agent_runs");

        // 直接读列验证（不依赖 API 反序列化路径），确保落库内容与写入完全一致。
        let row = db
            .conn
            .query_one(Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT agent_runs FROM execution_records WHERE id = {id}"),
            ))
            .await
            .expect("query agent_runs")
            .expect("row exists");
        let stored: Option<String> = row.try_get_by_index(0).expect("col readable");
        assert_eq!(stored.as_deref(), Some(json));
    }

    /// LatestExecutionSummary::display_at：优先 finished_at，回退 started_at，均无则 None。
    #[test]
    fn test_latest_execution_summary_display_at() {
        let both = LatestExecutionSummary {
            status: Some("success".into()),
            finished_at: Some("2026-07-08T10:00:00Z".into()),
            started_at: Some("2026-07-08T09:59:00Z".into()),
        };
        assert_eq!(both.display_at(), Some("2026-07-08T10:00:00Z"));
        let only_started = LatestExecutionSummary {
            status: Some("running".into()),
            finished_at: None,
            started_at: Some("2026-07-08T09:00:00Z".into()),
        };
        assert_eq!(only_started.display_at(), Some("2026-07-08T09:00:00Z"));
        let none = LatestExecutionSummary {
            status: None,
            finished_at: None,
            started_at: None,
        };
        assert!(none.display_at().is_none());
    }

    /// 连续失败计数：尾部 2 条 failed（前面有 success 断点）→ 计数 2。
    #[tokio::test]
    async fn test_get_consecutive_failure_counts_for_todos_trailing_failures() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "A").await;
        seed_exec(&db, t, "success", "manual").await;
        seed_exec(&db, t, "failed", "manual").await;
        seed_exec(&db, t, "failed", "manual").await;
        let map = db.get_consecutive_failure_counts_for_todos(&[t]).await.unwrap();
        assert_eq!(map.get(&t).copied().unwrap_or(0), 2, "尾部连续 2 次 failed");
    }

    /// 连续失败计数：最近一条非 failed → 计数 0（即使历史有 failed）。
    #[tokio::test]
    async fn test_get_consecutive_failure_counts_for_todos_zero_when_latest_not_failed() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "B").await;
        seed_exec(&db, t, "failed", "manual").await;
        seed_exec(&db, t, "success", "manual").await;
        let map = db.get_consecutive_failure_counts_for_todos(&[t]).await.unwrap();
        // 计数 0 时 todo 不在 map 中（GROUP BY 不产生 0 行），unwrap_or(0) 表达「无连续失败」
        assert_eq!(map.get(&t).copied().unwrap_or(0), 0, "最近一条 success 应计数 0");
    }

    /// 096-W2：get_execution_records 的 hours 过滤（参数化 `started_at >= ?`）±1 小时边界。
    /// 数据用生产契约的 T/Z ISO 格式（应用层 utc_timestamp 写入；秒级为触发器兜底形态）。
    #[tokio::test]
    async fn test_get_execution_records_hours_filter_boundary() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "T").await;
        // 23 小时前（24h 窗口内，应命中）与 25 小时前（窗口外，应排除）
        db.exec(&format!(
            "INSERT INTO execution_records (todo_id, status, trigger_type, started_at) \
             VALUES ({t}, 'success', 'manual', strftime('%Y-%m-%dT%H:%M:%SZ','now','-23 hours'))"
        ))
        .await
        .expect("insert recent exec");
        db.exec(&format!(
            "INSERT INTO execution_records (todo_id, status, trigger_type, started_at) \
             VALUES ({t}, 'success', 'manual', strftime('%Y-%m-%dT%H:%M:%SZ','now','-25 hours'))"
        ))
        .await
        .expect("insert old exec");

        let query = |hours: Option<u32>| ExecutionRecordQuery {
            todo_id: Some(t),
            step_id: None,
            workspace_id: None,
            limit: 20,
            offset: 0,
            status: None,
            hours,
        };
        // 不过滤 → 两条全返回（对照组，确认排除效果来自 hours 而非其他条件）
        let (_, total_all) = db.get_execution_records(query(None)).await.expect("query all");
        assert_eq!(total_all, 2);
        // hours=24 → 仅窗口内一条；total 与分页数据一致（过滤下推 SQL 而非内存裁剪）
        let (records, total) = db.get_execution_records(query(Some(24))).await.expect("query 24h");
        assert_eq!(total, 1, "25 小时前的记录应被 24h 窗口排除");
        assert_eq!(records.len(), 1);
        // hours=0 按契约视为不过滤（filter(|&h| h > 0) 短路）
        let (_, total_zero) = db.get_execution_records(query(Some(0))).await.expect("query h0");
        assert_eq!(total_zero, 2, "hours=0 应退化为不过滤");
    }

    /// 连续失败计数：全部 failed（无非 failed 断点）→ 计数全部。
    #[tokio::test]
    async fn test_get_consecutive_failure_counts_for_todos_all_failed() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "C").await;
        seed_exec(&db, t, "failed", "manual").await;
        seed_exec(&db, t, "failed", "manual").await;
        seed_exec(&db, t, "failed", "manual").await;
        let map = db.get_consecutive_failure_counts_for_todos(&[t]).await.unwrap();
        assert_eq!(map.get(&t).copied().unwrap_or(0), 3, "全部 failed 应计数 3");
    }

    /// webhook 最近触发时间：只取 trigger_type='webhook' 的最新一条，忽略手动执行。
    #[tokio::test]
    async fn test_get_last_webhook_trigger_for_todos_ignores_manual() {
        let db = fresh_db().await;
        let t = seed_todo(&db, "D").await;
        // 先一条 webhook 触发，再一条手动执行（更晚）
        seed_exec(&db, t, "success", "webhook").await;
        seed_exec(&db, t, "success", "manual").await;
        let map = db.get_last_webhook_trigger_for_todos(&[t]).await.unwrap();
        assert!(map.contains_key(&t), "应有 webhook 触发记录");
        // 无 webhook 记录的 todo 不在 map 中
        let t2 = seed_todo(&db, "E").await;
        seed_exec(&db, t2, "success", "manual").await;
        let map2 = db.get_last_webhook_trigger_for_todos(&[t2]).await.unwrap();
        assert!(!map2.contains_key(&t2), "纯手动执行不应出现在 webhook 触发 map");
    }
}

/// get_execution_records_by_session 测试：软删 todo 的 session 记录仍应能查到（v89 归属解耦）。
///
/// 背景：060 讨论区执行完成会软删 carrier todo。旧实现按
/// `todo_id IN (SELECT id FROM todos WHERE workspace_id=? AND deleted_at IS NULL)` 过滤，
/// 软删后子查询排除该 todo → session 查询返回空 → 帖子页「暂无执行记录」。
/// v89 给 execution_records 加了 workspace_id 列，本查询改用 record 自身归属，与 todo 生命周期解耦。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod session_query_tests {
    use super::*;

    /// 建 workspace + 归属于它的 todo + 一条带 session_id 的 execution record，
    /// 返回 (workspace_id, todo_id, record_id)。
    async fn seed_session_record(
        db: &Database,
        path: &str,
        session_id: &str,
    ) -> (i64, i64, i64) {
        // 直接写 project_directories 表造 workspace，避免依赖 create_todo 等业务入口
        let ws = db
            .create_project_directory(path, None, false, false)
            .await
            .expect("create workspace");
        // todo 归属 ws：旧实现的子查询要求 todo.workspace_id=ws 且未软删
        db.exec(&format!(
            "INSERT INTO todos (title, prompt, status, workspace_id) \
             VALUES ('t-{path}', 'p', 'pending', {ws})"
        ))
        .await
        .expect("insert todo");
        let todo_row = db
            .conn
            .query_one(Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT id FROM todos WHERE title = 't-{path}'"),
            ))
            .await
            .expect("query todo id")
            .expect("todo exists");
        let todo_id: i64 = todo_row.try_get_by_index(0).expect("todo id readable");
        // record 直接归属 ws（v89 语义，与生产写入点一致）
        db.exec(&format!(
            "INSERT INTO execution_records (todo_id, workspace_id, session_id, status, trigger_type) \
             VALUES ({todo_id}, {ws}, '{session_id}', 'success', 'manual')"
        ))
        .await
        .expect("insert record");
        let record_row = db
            .conn
            .query_one(Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT id FROM execution_records ORDER BY id DESC LIMIT 1".to_string(),
            ))
            .await
            .expect("query record id")
            .expect("record exists");
        let record_id: i64 = record_row.try_get_by_index(0).expect("record id readable");
        (ws, todo_id, record_id)
    }

    #[tokio::test]
    async fn test_get_execution_records_by_session_returns_record_when_todo_soft_deleted() {
        let db = Database::new(":memory:").await.expect("db must open");
        let session = "sess-bug-1";
        let (ws, todo_id, record_id) = seed_session_record(&db, "/tmp/ws-bug", session).await;
        // 软删 carrier todo，模拟 060 讨论执行完成后的清理（task_post finalize 阶段）
        db.soft_delete_todo(todo_id).await.expect("soft delete todo");
        // 修复前：todo 被软删 → 旧子查询排除 → 返回空；修复后：按 record.workspace_id 直接命中
        let records = db
            .get_execution_records_by_session(session, Some(ws))
            .await
            .expect("query must succeed");
        assert_eq!(records.len(), 1, "软删 todo 的 session 记录仍应查到");
        assert_eq!(records[0].id, record_id);
    }

    #[tokio::test]
    async fn test_get_execution_records_by_session_cross_workspace_excluded() {
        let db = Database::new(":memory:").await.expect("db must open");
        // 记录归属 ws_a；用另一个 ws 查同 session → V1 隔离，应返回空
        let (_ws_a, _todo, _record) = seed_session_record(&db, "/tmp/ws-a", "sess-x").await;
        let other_ws = db
            .create_project_directory("/tmp/ws-other", None, false, false)
            .await
            .expect("create other ws");
        let records = db
            .get_execution_records_by_session("sess-x", Some(other_ws))
            .await
            .expect("query must succeed");
        assert!(records.is_empty(), "跨 ws 不应命中（V1 隔离）");
        // 无 workspace 过滤时（None）应能命中——session 本身不受 ws 限制
        let all = db
            .get_execution_records_by_session("sess-x", None)
            .await
            .expect("query must succeed");
        assert_eq!(all.len(), 1, "不过滤 ws 时同 session 记录应全部返回");
    }
}
