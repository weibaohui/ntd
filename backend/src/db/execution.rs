use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Statement,
};

use crate::db::entity::execution_logs;
use crate::db::entity::execution_records;
use crate::db::Database;
use crate::models::{ExecutionRecord, ExecutionStatus, ExecutionSummary, ExecutionUsage, ParsedLogEntry};

pub struct NewExecutionRecord<'a> {
    pub todo_id: Option<i64>,
    pub command: &'a str,
    pub executor: &'a str,
    pub trigger_type: &'a str,
    pub task_id: &'a str,
    pub session_id: Option<&'a str>,
    pub resume_message: Option<&'a str>,
    /// Hook trigger provenance. `None` for manual/cron/webhook/feishu
    /// triggers; set by `execute_target_todo` for hook firings.
    pub source_todo_id: Option<i64>,
    pub source_todo_title: Option<&'a str>,
    pub source_hook_id: Option<i64>,
    /// 当本次执行是 loop 环节的一部分时，指向 loop_step_executions 表的 id。
    pub loop_step_execution_id: Option<i64>,
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
    pub limit: i64,
    pub offset: i64,
    pub status: Option<&'a str>,
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
            execution_stats,
            resume_message: m.resume_message,
            source_todo_id: m.source_todo_id,
            source_todo_title: m.source_todo_title,
            source_hook_id: m.source_hook_id,
            rating: m.rating,
            source_execution_record_id: m.source_execution_record_id,
            last_review_status: m.last_review_status,
            last_reviewed_at: m.last_reviewed_at,
            worktree_path: m.worktree_path,
        }
    }
}

impl Database {
    pub async fn get_execution_records(
        &self,
        query: ExecutionRecordQuery<'_>,
    ) -> Result<(Vec<ExecutionRecord>, i64), sea_orm::DbErr> {
        let base_filter = if let Some(todo_id) = query.todo_id {
            execution_records::Column::TodoId.eq(todo_id)
        } else {
            execution_records::Column::TodoId.is_not_null()
        };
        let filter = match query.status {
            Some("all") | None => base_filter,
            Some(s) => base_filter.and(execution_records::Column::Status.eq(s)),
        };

        let total: i64 = execution_records::Entity::find()
            .filter(filter.clone())
            .count(&self.conn)
            .await? as i64;

        let limit_u = if query.limit < 0 { 0 } else { query.limit as u64 };
        let offset_u = if query.offset < 0 { 0 } else { query.offset as u64 };

        let records = execution_records::Entity::find()
            .filter(filter)
            .order_by_desc(execution_records::Column::StartedAt)
            .limit(limit_u)
            .offset(offset_u)
            .all(&self.conn)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();

        Ok((records, total))
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

    /// 根据 task_id 获取执行记录
    pub async fn get_execution_record_by_task_id(
        &self,
        task_id: &str,
    ) -> Result<Option<ExecutionRecord>, sea_orm::DbErr> {
        let m = execution_records::Entity::find()
            .filter(execution_records::Column::TaskId.eq(task_id))
            .one(&self.conn)
            .await?;
        Ok(m.map(Into::into))
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
            source_hook_id: ActiveValue::Set(record.source_hook_id),
            loop_step_execution_id: ActiveValue::Set(record.loop_step_execution_id),
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

    /// 将 JSON 格式的日志条目批量插入 execution_logs 表
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
        if entries.is_empty() {
            return Ok(());
        }

        let models: Vec<execution_logs::ActiveModel> = entries
            .into_iter()
            .map(|e| {
                let metadata = serde_json::json!({
                    "usage": e.usage,
                    "tool_name": e.tool_name,
                    "tool_input_json": e.tool_input_json,
                });
                let metadata_str = serde_json::to_string(&metadata).unwrap_or_default();
                execution_logs::ActiveModel {
                    record_id: ActiveValue::Set(record_id),
                    timestamp: ActiveValue::Set(e.timestamp),
                    log_type: ActiveValue::Set(e.log_type),
                    content: ActiveValue::Set(e.content),
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

        let logs: Vec<ParsedLogEntry> = entries
            .into_iter()
            .map(|m| {
                let (usage, tool_name, tool_input_json) = m
                    .metadata
                    .as_deref()
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

                ParsedLogEntry {
                    timestamp: m.timestamp,
                    log_type: m.log_type,
                    content: m.content,
                    usage,
                    tool_name,
                    tool_input_json,
                }
            })
            .collect();

        Ok((logs, total))
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

    /// 批量获取多个 record_id 的所有执行日志（避免 WebSocket 同步时的 N+1 查询）
    pub async fn get_all_execution_logs_for_records(
        &self,
        record_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<ParsedLogEntry>>, sea_orm::DbErr> {
        if record_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let entries = execution_logs::Entity::find()
            .filter(execution_logs::Column::RecordId.is_in(record_ids.iter().copied()))
            .order_by_asc(execution_logs::Column::Id)
            .all(&self.conn)
            .await?;

        let mut map: std::collections::HashMap<i64, Vec<ParsedLogEntry>> =
            std::collections::HashMap::with_capacity(record_ids.len());
        for m in entries {
            let (usage, tool_name, tool_input_json) = m
                .metadata
                .as_deref()
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

            map.entry(m.record_id).or_default().push(ParsedLogEntry {
                timestamp: m.timestamp,
                log_type: m.log_type,
                content: m.content,
                usage,
                tool_name,
                tool_input_json,
            });
        }
        Ok(map)
    }

    /// 根据 session_id 获取所有执行记录（按 started_at 排序）
    pub async fn get_execution_records_by_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<ExecutionRecord>, sea_orm::DbErr> {
        Ok(execution_records::Entity::find()
            .filter(execution_records::Column::SessionId.eq(session_id))
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
        let time_filter = format!("datetime('now', '-{} hours')", hours);
        // 热力图使用固定时间范围：当年1月1日到12月31日，不受过滤条件影响
        let heatmap_filter = format!("datetime(strftime('%Y', 'now') || '-01-01 00:00:00')");
        crate::db::dashboard::DashboardQueryContext {
            conn: &self.conn,
            backend,
            time_filter,
            heatmap_filter,
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
    ) -> Result<Vec<ExecutionRecord>, sea_orm::DbErr> {
        let models = execution_records::Entity::find()
            .filter(execution_records::Column::Status.eq("running"))
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
            overall,
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
    fn build_skills_response(
        overall: SkillsOverallRow,
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
                            if last_backup.is_none() || modified_str > *last_backup.as_ref().unwrap() {
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
                        let created_at = metadata.modified().ok()
                            .map(|t| Self::system_time_to_iso_string(t))
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
}

#[cfg(test)]
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
