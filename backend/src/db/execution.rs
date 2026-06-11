use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Statement,
};

use crate::db::entity::execution_logs;
use crate::db::entity::execution_records;
use crate::db::Database;
use crate::models::{ExecutionRecord, ExecutionStatus, ExecutionSummary, ExecutionUsage, ParsedLogEntry};

pub struct NewExecutionRecord<'a> {
    pub todo_id: i64,
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
    pub todo_id: i64,
    pub limit: i64,
    pub offset: i64,
    pub status: Option<&'a str>,
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
        }
    }
}

impl Database {
    pub async fn get_execution_records(
        &self,
        query: ExecutionRecordQuery<'_>,
    ) -> Result<(Vec<ExecutionRecord>, i64), sea_orm::DbErr> {
        let base_filter = execution_records::Column::TodoId.eq(query.todo_id);
        let filter = if let Some(s) = query.status {
            if s == "all" {
                base_filter // "all" 等同于不传过滤条件
            } else {
                base_filter.and(execution_records::Column::Status.eq(s))
            }
        } else {
            base_filter
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
            todo_id: ActiveValue::Set(Some(record.todo_id)),
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
        let usage_json = req.usage.map(|u| {
            serde_json::to_string(u).unwrap_or_else(|e| {
                tracing::error!("Failed to serialize usage: {}", e);
                String::new()
            })
        });
        let model_val = req.model.map(|s| s.to_string());

        // Use raw SQL with WHERE status='running' to prevent race condition:
        // both the stop handler and spawned task's cancellation branch may try to
        // update the same record concurrently -- only the first write succeeds.
        let backend = self.conn.get_database_backend();
        let (sql, values): (&str, Vec<sea_orm::Value>) = if let Some((source_record_id, review_status)) = req.review_meta {
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
        };

        let res = self
            .conn
            .execute(Statement::from_sql_and_values(
                backend,
                sql,
                values,
            ))
            .await?;
        let updated = res.rows_affected() > 0;

        // Only insert logs if the status update succeeded (prevent duplicate logs on concurrent writes)
        if updated && !req.remaining_logs.is_empty() && req.remaining_logs != "[]" {
            self.insert_execution_logs(req.id, req.remaining_logs).await?;
        }

        Ok(updated)
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
        use std::collections::HashMap;

        let backend = self.conn.get_database_backend();
        let hours = hours.unwrap_or(720); // default 30 days = 720 hours (matches frontend)
        let time_filter = format!("datetime('now', '-{} hours')", hours);

        // 热力图使用固定时间范围：当年1月1日到12月31日，不受过滤条件影响
        let heatmap_filter = format!("datetime(strftime('%Y', 'now') || '-01-01 00:00:00')");
        let heatmap_limit = 366; // 闰年最多366天

        let todo_sql = "SELECT \
            COUNT(*) as total, \
            COALESCE(SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END), 0) as pending, \
            COALESCE(SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END), 0) as running, \
            COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0) as completed, \
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed, \
            COALESCE(SUM(CASE WHEN scheduler_enabled = 1 AND scheduler_config IS NOT NULL THEN 1 ELSE 0 END), 0) as scheduled \
            FROM todos WHERE deleted_at IS NULL";

        // Build time-filtered SQL queries
        let overall_sql = format!(
            "SELECT \
            COUNT(*) as total, \
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success, \
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.input_tokens'), 0)), 0) as input_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.output_tokens'), 0)), 0) as output_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_read_input_tokens'), 0)), 0) as cache_read, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_creation_input_tokens'), 0)), 0) as cache_creation, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.total_cost_usd'), 0.0)), 0.0) as total_cost, \
            COALESCE(SUM(CASE WHEN json_extract(usage, '$.duration_ms') IS NOT NULL THEN json_extract(usage, '$.duration_ms') ELSE 0 END), 0) as total_duration, \
            COALESCE(SUM(CASE WHEN json_extract(usage, '$.duration_ms') IS NOT NULL THEN 1 ELSE 0 END), 0) as duration_count \
            FROM execution_records \
            WHERE started_at >= {}",
            time_filter
        );

        let (
            total_todos,
            pending_todos,
            running_todos,
            completed_todos,
            failed_todos,
            scheduled_todos,
        ) = if let Some(row) = self
            .conn
            .query_one(Statement::from_string(backend, todo_sql.to_string()))
            .await?
        {
            (
                row.try_get_by("total").unwrap_or(0i64),
                row.try_get_by("pending").unwrap_or(0i64),
                row.try_get_by("running").unwrap_or(0i64),
                row.try_get_by("completed").unwrap_or(0i64),
                row.try_get_by("failed").unwrap_or(0i64),
                row.try_get_by("scheduled").unwrap_or(0i64),
            )
        } else {
            (0i64, 0i64, 0i64, 0i64, 0i64, 0i64)
        };

        let tags = self.get_tags().await?;
        let total_tags = tags.len() as i64;

        // Executor todo counts via SQL (replaces in-memory iteration over all todos)
        let executor_todo_sql = "SELECT \
            COALESCE(executor, 'claudecode') as executor, \
            COUNT(*) as todo_count \
            FROM todos WHERE deleted_at IS NULL \
            GROUP BY COALESCE(executor, 'claudecode')";

        let executor_todo_counts: HashMap<String, i64> = self
            .conn
            .query_all(Statement::from_string(
                backend,
                executor_todo_sql.to_string(),
            ))
            .await?
            .into_iter()
            .filter_map(|row| {
                let exec: String = row.try_get_by("executor").ok()?;
                let count: i64 = row.try_get_by("todo_count").ok()?;
                Some((exec, count))
            })
            .collect();

        // Tag todo counts via SQL (replaces fetch_tag_ids_for_many + in-memory counting)
        let tag_todo_sql = "SELECT tag_id, COUNT(*) as todo_count FROM todo_tags GROUP BY tag_id";

        let tag_todo_counts: HashMap<i64, i64> = self
            .conn
            .query_all(Statement::from_string(backend, tag_todo_sql.to_string()))
            .await?
            .into_iter()
            .filter_map(|row| {
                let tag_id: i64 = row.try_get_by("tag_id").ok()?;
                let count: i64 = row.try_get_by("todo_count").ok()?;
                Some((tag_id, count))
            })
            .collect();

        let (
            total_executions,
            success_executions,
            failed_executions,
            total_input_tokens,
            total_output_tokens,
            total_cache_read_tokens,
            total_cache_creation_tokens,
            total_cost,
            total_duration,
            duration_count,
        ) = if let Some(row) = self
            .conn
            .query_one(Statement::from_string(backend, overall_sql.to_string()))
            .await?
        {
            let t: i64 = row.try_get_by("total").unwrap_or(0);
            let s: i64 = row.try_get_by("success").unwrap_or(0);
            let f: i64 = row.try_get_by("failed").unwrap_or(0);
            let it: i64 = row.try_get_by("input_tokens").unwrap_or(0);
            let ot: i64 = row.try_get_by("output_tokens").unwrap_or(0);
            let cr: i64 = row.try_get_by("cache_read").unwrap_or(0);
            let cc: i64 = row.try_get_by("cache_creation").unwrap_or(0);
            let tc: f64 = row.try_get_by("total_cost").unwrap_or(0.0);
            let td: i64 = row.try_get_by("total_duration").unwrap_or(0);
            let dc: i64 = row.try_get_by("duration_count").unwrap_or(0);
            (
                t, s, f, it as u64, ot as u64, cr as u64, cc as u64, tc, td as u64, dc as u64,
            )
        } else {
            (0, 0, 0, 0u64, 0u64, 0u64, 0u64, 0.0f64, 0u64, 0u64)
        };

        let avg_duration_ms = if duration_count > 0 {
            total_duration / duration_count
        } else {
            0
        };

        // 3. Executor distribution via SQL
        let executor_sql = format!(
            "SELECT \
            COALESCE(executor, 'claudecode') as executor, \
            COUNT(*) as execution_count, \
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success_count, \
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed_count, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.input_tokens'), 0)), 0) as input_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.output_tokens'), 0)), 0) as output_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.total_cost_usd'), 0.0)), 0.0) as cost \
            FROM execution_records \
            WHERE started_at >= {} \
            GROUP BY COALESCE(executor, 'claudecode')",
            time_filter
        );

        let mut executor_distribution: Vec<crate::models::ExecutorCount> = self
            .conn
            .query_all(Statement::from_string(backend, executor_sql.to_string()))
            .await?
            .into_iter()
            .filter_map(|row| {
                let exec: String = row.try_get_by("executor").ok()?;
                let ec: i64 = row.try_get_by("execution_count").ok()?;
                if ec == 0 {
                    return None;
                }
                let sc: i64 = row.try_get_by("success_count").ok()?;
                let fc: i64 = row.try_get_by("failed_count").ok()?;
                let it: i64 = row.try_get_by("input_tokens").ok()?;
                let ot: i64 = row.try_get_by("output_tokens").ok()?;
                let cost: f64 = row.try_get_by("cost").ok()?;
                Some(crate::models::ExecutorCount {
                    count: *executor_todo_counts.get(&exec).unwrap_or(&0),
                    executor: exec,
                    execution_count: ec,
                    success_count: sc,
                    failed_count: fc,
                    total_input_tokens: it as u64,
                    total_output_tokens: ot as u64,
                    total_cost_usd: cost,
                })
            })
            .collect();
        executor_distribution.sort_by(|a, b| b.execution_count.cmp(&a.execution_count));

        // 4. Model distribution via SQL
        let model_sql = format!(
            "SELECT \
            COALESCE(model, 'unknown') as model, \
            COUNT(*) as execution_count, \
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success_count, \
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed_count, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.input_tokens'), 0)), 0) as input_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.output_tokens'), 0)), 0) as output_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_read_input_tokens'), 0)), 0) as cache_read, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_creation_input_tokens'), 0)), 0) as cache_creation, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.total_cost_usd'), 0.0)), 0.0) as cost \
            FROM execution_records \
            WHERE started_at >= {} \
            GROUP BY COALESCE(model, 'unknown')",
            time_filter
        );

        let mut model_distribution: Vec<crate::models::ModelCount> = self
            .conn
            .query_all(Statement::from_string(backend, model_sql.to_string()))
            .await?
            .into_iter()
            .filter_map(|row| {
                let model: String = row.try_get_by("model").ok()?;
                let ec: i64 = row.try_get_by("execution_count").ok()?;
                if ec == 0 {
                    return None;
                }
                let sc: i64 = row.try_get_by("success_count").ok()?;
                let fc: i64 = row.try_get_by("failed_count").ok()?;
                let it: i64 = row.try_get_by("input_tokens").ok()?;
                let ot: i64 = row.try_get_by("output_tokens").ok()?;
                let cr: i64 = row.try_get_by("cache_read").ok()?;
                let cc: i64 = row.try_get_by("cache_creation").ok()?;
                let cost: f64 = row.try_get_by("cost").ok()?;
                Some(crate::models::ModelCount {
                    model,
                    count: 0,
                    execution_count: ec,
                    success_count: sc,
                    failed_count: fc,
                    total_input_tokens: it as u64,
                    total_output_tokens: ot as u64,
                    total_cache_read_tokens: cr as u64,
                    total_cache_creation_tokens: cc as u64,
                    total_cost_usd: cost,
                })
            })
            .collect();
        model_distribution.sort_by(|a, b| b.execution_count.cmp(&a.execution_count));

        // Trigger type distribution
        let trigger_sql = format!(
            "SELECT \
            COALESCE(trigger_type, 'manual') as trigger_type, \
            COUNT(*) as count, \
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success_count, \
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed_count \
            FROM execution_records \
            WHERE started_at >= {} \
            GROUP BY COALESCE(trigger_type, 'manual')",
            time_filter
        );

        let mut trigger_type_distribution: Vec<crate::models::TriggerTypeCount> = self.conn
            .query_all(Statement::from_string(backend, trigger_sql.to_string()))
            .await?
            .into_iter()
            .filter_map(|row| {
                let tt: String = row.try_get_by("trigger_type").ok()?;
                let c: i64 = row.try_get_by("count").ok()?;
                let sc: i64 = row.try_get_by("success_count").ok()?;
                let fc: i64 = row.try_get_by("failed_count").ok()?;
                Some(crate::models::TriggerTypeCount {
                    trigger_type: tt,
                    count: c,
                    success_count: sc,
                    failed_count: fc,
                })
            })
            .collect();
        trigger_type_distribution.sort_by(|a, b| b.count.cmp(&a.count));

        // Executor average duration
        let duration_sql = format!(
            "SELECT \
            COALESCE(executor, 'claudecode') as executor, \
            ROUND(AVG(json_extract(usage, '$.duration_ms')), 0) as avg_duration, \
            COUNT(*) as execution_count \
            FROM execution_records \
            WHERE started_at >= {} AND json_extract(usage, '$.duration_ms') IS NOT NULL \
            GROUP BY COALESCE(executor, 'claudecode')",
            time_filter
        );

        let mut executor_duration_stats: Vec<crate::models::ExecutorDuration> = self.conn
            .query_all(Statement::from_string(backend, duration_sql.to_string()))
            .await?
            .into_iter()
            .filter_map(|row| {
                let exec: String = row.try_get_by("executor").ok()?;
                let ad: f64 = row.try_get_by("avg_duration").ok()?;
                let ec: i64 = row.try_get_by("execution_count").ok()?;
                Some(crate::models::ExecutorDuration {
                    executor: exec,
                    avg_duration_ms: ad,
                    execution_count: ec,
                })
            })
            .collect();
        executor_duration_stats.sort_by(|a, b| b.avg_duration_ms.partial_cmp(&a.avg_duration_ms).unwrap_or(std::cmp::Ordering::Equal));

        // Model cache stats
        let cache_sql = format!(
            "SELECT \
            COALESCE(model, 'unknown') as model, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.input_tokens'), 0)), 0) as input_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_read_input_tokens'), 0)), 0) as cache_read \
            FROM execution_records \
            WHERE started_at >= {} AND model IS NOT NULL \
            GROUP BY COALESCE(model, 'unknown')",
            time_filter
        );

        let mut model_cache_stats: Vec<crate::models::ModelCacheStat> = self.conn
            .query_all(Statement::from_string(backend, cache_sql.to_string()))
            .await?
            .into_iter()
            .filter_map(|row| {
                let model: String = row.try_get_by("model").ok()?;
                let it: i64 = row.try_get_by("input_tokens").ok()?;
                let cr: i64 = row.try_get_by("cache_read").ok()?;
                let total = it as u64 + cr as u64;
                let rate = if total > 0 { cr as f64 / total as f64 * 100.0 } else { 0.0 };
                Some(crate::models::ModelCacheStat {
                    model,
                    total_input_tokens: it as u64,
                    total_cache_read_tokens: cr as u64,
                    cache_hit_rate: rate,
                })
            })
            .collect();
        model_cache_stats.sort_by(|a, b| b.cache_hit_rate.partial_cmp(&a.cache_hit_rate).unwrap_or(std::cmp::Ordering::Equal));
        model_cache_stats.retain(|m| m.total_input_tokens > 0 || m.total_cache_read_tokens > 0);

        // 5. Daily execution stats via SQL (热力图使用固定当年范围)
        let daily_sql = format!(
            "SELECT \
            SUBSTR(COALESCE(started_at, ''), 1, 10) as day, \
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success, \
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.input_tokens'), 0)), 0) as input_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.output_tokens'), 0)), 0) as output_tokens, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_read_input_tokens'), 0)), 0) as cache_read, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_creation_input_tokens'), 0)), 0) as cache_creation, \
            COALESCE(SUM(COALESCE(json_extract(usage, '$.total_cost_usd'), 0.0)), 0.0) as cost \
            FROM execution_records \
            WHERE started_at IS NOT NULL AND LENGTH(started_at) >= 10 AND started_at >= {} \
            GROUP BY SUBSTR(started_at, 1, 10) \
            ORDER BY day DESC \
            LIMIT {}",
            heatmap_filter, heatmap_limit
        );

        let daily_rows = self
            .conn
            .query_all(Statement::from_string(backend, daily_sql.to_string()))
            .await?;

        let mut daily_executions: Vec<crate::models::DailyExecution> =
            Vec::with_capacity(daily_rows.len());
        let mut daily_token_stats: Vec<crate::models::DailyTokenStats> =
            Vec::with_capacity(daily_rows.len());
        for row in &daily_rows {
            let day: String = row.try_get_by("day").unwrap_or_default();
            let success: i64 = row.try_get_by("success").unwrap_or(0);
            let failed: i64 = row.try_get_by("failed").unwrap_or(0);
            daily_executions.push(crate::models::DailyExecution {
                date: day.clone(),
                success,
                failed,
            });

            let it: i64 = row.try_get_by("input_tokens").unwrap_or(0);
            let ot: i64 = row.try_get_by("output_tokens").unwrap_or(0);
            let cr: i64 = row.try_get_by("cache_read").unwrap_or(0);
            let cc: i64 = row.try_get_by("cache_creation").unwrap_or(0);
            let cost: f64 = row.try_get_by("cost").unwrap_or(0.0);
            daily_token_stats.push(crate::models::DailyTokenStats {
                date: day,
                input_tokens: it as u64,
                output_tokens: ot as u64,
                cache_read_tokens: cr as u64,
                cache_creation_tokens: cc as u64,
                total_cost_usd: cost,
            });
        }
        daily_executions.reverse();
        daily_token_stats.reverse();

        // 6. Tag distribution via SQL (join through todo_tags)
        let tag_sql = format!(
            "SELECT \
            tt.tag_id, \
            COUNT(*) as execution_count, \
            COALESCE(SUM(CASE WHEN er.status = 'success' THEN 1 ELSE 0 END), 0) as success_count, \
            COALESCE(SUM(CASE WHEN er.status = 'failed' THEN 1 ELSE 0 END), 0) as failed_count, \
            COALESCE(SUM(COALESCE(json_extract(er.usage, '$.input_tokens'), 0)), 0) as input_tokens, \
            COALESCE(SUM(COALESCE(json_extract(er.usage, '$.output_tokens'), 0)), 0) as output_tokens, \
            COALESCE(SUM(COALESCE(json_extract(er.usage, '$.total_cost_usd'), 0.0)), 0.0) as cost \
            FROM execution_records er \
            INNER JOIN todo_tags tt ON tt.todo_id = er.todo_id \
            WHERE er.todo_id IS NOT NULL AND er.started_at >= {} \
            GROUP BY tt.tag_id",
            time_filter
        );

        let tag_rows = self
            .conn
            .query_all(Statement::from_string(backend, tag_sql.to_string()))
            .await?;

        let mut tag_exec_stats: HashMap<i64, (i64, i64, i64, u64, u64, f64)> = HashMap::new();
        for row in tag_rows {
            let tag_id: i64 = row.try_get_by("tag_id").unwrap_or(0);
            let ec: i64 = row.try_get_by("execution_count").unwrap_or(0);
            let sc: i64 = row.try_get_by("success_count").unwrap_or(0);
            let fc: i64 = row.try_get_by("failed_count").unwrap_or(0);
            let it: i64 = row.try_get_by("input_tokens").unwrap_or(0);
            let ot: i64 = row.try_get_by("output_tokens").unwrap_or(0);
            let cost: f64 = row.try_get_by("cost").unwrap_or(0.0);
            tag_exec_stats.insert(tag_id, (ec, sc, fc, it as u64, ot as u64, cost));
        }

        let mut tag_distribution: Vec<crate::models::TagCount> = tags
            .iter()
            .filter_map(|t| {
                let todo_count = *tag_todo_counts.get(&t.id).unwrap_or(&0);
                if todo_count == 0 {
                    return None;
                }
                let (ec, sc, fc, it, ot, cost) = tag_exec_stats
                    .get(&t.id)
                    .copied()
                    .unwrap_or((0, 0, 0, 0, 0, 0.0));
                Some(crate::models::TagCount {
                    tag_id: t.id,
                    tag_name: t.name.clone(),
                    tag_color: t.color.clone(),
                    count: todo_count,
                    execution_count: ec,
                    success_count: sc,
                    failed_count: fc,
                    total_input_tokens: it,
                    total_output_tokens: ot,
                    total_cost_usd: cost,
                })
            })
            .collect();
        tag_distribution.sort_by(|a, b| b.execution_count.cmp(&a.execution_count));

        // 7. Recent executions (only load 10 rows, not the entire table)
        // Note: Only essential fields are loaded for performance; logs, stdout, stderr,
        // command, model, pid, todo_progress, execution_stats are omitted as they're large
        let recent_sql = format!(
            "SELECT id, todo_id, executor, trigger_type, status, started_at, finished_at, usage, task_id, session_id, result, resume_message FROM execution_records \
            WHERE started_at >= {} \
            ORDER BY started_at DESC LIMIT 10",
            time_filter
        );
        let recent_records: Vec<execution_records::Model> = self
            .conn
            .query_all(Statement::from_string(backend, recent_sql))
            .await?
            .into_iter()
            .map(|row| {
                execution_records::Model {
                    id: row.try_get_by("id").unwrap_or(0),
                    todo_id: row.try_get_by("todo_id").ok(),
                    executor: row.try_get_by("executor").ok(),
                    trigger_type: row.try_get_by("trigger_type").ok(),
                    status: row.try_get_by("status").ok(),
                    started_at: row.try_get_by("started_at").ok(),
                    finished_at: row.try_get_by("finished_at").ok(),
                    usage: row.try_get_by("usage").ok(),
                    task_id: row.try_get_by("task_id").ok(),
                    session_id: row.try_get_by("session_id").ok(),
                    result: row.try_get_by("result").ok(),
                    resume_message: row.try_get_by("resume_message").ok(),
                    command: None,
                    stdout: None,
                    stderr: None,
                    model: None,
                    pid: None,
                    todo_progress: None,
                    execution_stats: None,
                    source_todo_id: None,
                    source_todo_title: None,
                    source_hook_id: None,
                    rating: None,
                    source_execution_record_id: None,
                    last_review_status: None,
                    last_reviewed_at: None,
                }
            })
            .collect();

        let recent_executions: Vec<crate::models::ExecutionRecord> =
            recent_records.into_iter().map(Into::into).collect();

        // Calculate enhanced metrics
        // Today and yesterday executions for change calculation
        let today_sql = "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now')";
        let yesterday_sql = "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now', '-1 day')";

        let today_executions: i64 = self.conn
            .query_one(Statement::from_string(backend, today_sql.to_string()))
            .await?
            .and_then(|row| row.try_get_by("count").ok())
            .unwrap_or(0);

        let yesterday_executions: i64 = self.conn
            .query_one(Statement::from_string(backend, yesterday_sql.to_string()))
            .await?
            .and_then(|row| row.try_get_by("count").ok())
            .unwrap_or(0);

        let executions_change = if yesterday_executions > 0 {
            Some((today_executions as f64 - yesterday_executions as f64) / yesterday_executions as f64 * 100.0)
        } else {
            None
        };

        // Success rate change (today vs yesterday)
        let yesterday_success_sql = "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now', '-1 day') AND status = 'success'";
        let yesterday_failed_sql = "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now', '-1 day') AND status = 'failed'";
        let today_success_sql = "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now') AND status = 'success'";
        let today_failed_sql = "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now') AND status = 'failed'";

        let yesterday_success: i64 = self.conn
            .query_one(Statement::from_string(backend, yesterday_success_sql.to_string()))
            .await?
            .and_then(|row| row.try_get_by("count").ok())
            .unwrap_or(0);
        let yesterday_failed: i64 = self.conn
            .query_one(Statement::from_string(backend, yesterday_failed_sql.to_string()))
            .await?
            .and_then(|row| row.try_get_by("count").ok())
            .unwrap_or(0);
        let today_success: i64 = self.conn
            .query_one(Statement::from_string(backend, today_success_sql.to_string()))
            .await?
            .and_then(|row| row.try_get_by("count").ok())
            .unwrap_or(0);
        let today_failed: i64 = self.conn
            .query_one(Statement::from_string(backend, today_failed_sql.to_string()))
            .await?
            .and_then(|row| row.try_get_by("count").ok())
            .unwrap_or(0);

        let yesterday_total = yesterday_success + yesterday_failed;
        let today_total = today_success + today_failed;
        let yesterday_rate = if yesterday_total > 0 { yesterday_success as f64 / yesterday_total as f64 * 100.0 } else { 0.0 };
        let today_rate = if today_total > 0 { today_success as f64 / today_total as f64 * 100.0 } else { 0.0 };
        let success_rate_change = if yesterday_total > 0 { Some(today_rate - yesterday_rate) } else { None };

        // Cost change (today vs yesterday)
        let today_cost_sql = "SELECT COALESCE(SUM(COALESCE(json_extract(usage, '$.total_cost_usd'), 0.0)), 0.0) as cost FROM execution_records WHERE date(started_at) = date('now')";
        let yesterday_cost_sql = "SELECT COALESCE(SUM(COALESCE(json_extract(usage, '$.total_cost_usd'), 0.0)), 0.0) as cost FROM execution_records WHERE date(started_at) = date('now', '-1 day')";
        let today_cost: f64 = self.conn
            .query_one(Statement::from_string(backend, today_cost_sql.to_string()))
            .await?
            .and_then(|row| row.try_get_by("cost").ok())
            .unwrap_or(0.0);
        let yesterday_cost: f64 = self.conn
            .query_one(Statement::from_string(backend, yesterday_cost_sql.to_string()))
            .await?
            .and_then(|row| row.try_get_by("cost").ok())
            .unwrap_or(0.0);
        let cost_change = if yesterday_cost > 0.0 {
            Some((today_cost - yesterday_cost) / yesterday_cost * 100.0)
        } else {
            None
        };

        // Active days and streak days from daily_executions
        let active_days = daily_executions.len() as i64;

        let mut streak_days = 0i64;
        if !daily_executions.is_empty() {
            let today_str = chrono::Utc::now().format("%Y-%m-%d").to_string();
            let yesterday_str = (chrono::Utc::now() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();

            // Check if today or yesterday has executions (streak must include recent days)
            let has_recent = daily_executions.iter().any(|d| {
                let has_exec = d.success + d.failed > 0;
                let is_today_or_yesterday = d.date == today_str || d.date == yesterday_str;
                has_exec && is_today_or_yesterday
            });

            if has_recent {
                // Count consecutive days with executions (must be recent to count)
                for day in daily_executions.iter().rev() {
                    if day.success + day.failed > 0 {
                        streak_days += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        // Peak daily executions
        let peak_daily_executions = daily_executions.iter()
            .map(|d| d.success + d.failed)
            .max()
            .unwrap_or(0);

        // Top model by tokens
        let (top_model, top_model_tokens) = if !model_distribution.is_empty() {
            let top = model_distribution.iter()
                .max_by_key(|m| m.total_input_tokens + m.total_output_tokens)
                .unwrap();
            (Some(top.model.clone()), Some(top.total_input_tokens + top.total_output_tokens))
        } else {
            (None, None)
        };

        // Build leaderboard from model distribution
        let leaderboard: Vec<crate::models::LeaderboardItem> = model_distribution.iter()
            .enumerate()
            .map(|(i, m)| {
                // Calculate change (simplified: compare first half vs second half of the period)
                let half = model_distribution.len() / 2;
                let change = if i < half && i + half < model_distribution.len() {
                    let first_half_tokens = model_distribution[..i+1].iter().map(|x| x.total_input_tokens + x.total_output_tokens).sum::<u64>() as f64;
                    let second_half_tokens = model_distribution[i..i+half].iter().map(|x| x.total_input_tokens + x.total_output_tokens).sum::<u64>() as f64;
                    if first_half_tokens > 0.0 {
                        Some((second_half_tokens - first_half_tokens) / first_half_tokens * 100.0)
                    } else {
                        None
                    }
                } else {
                    None
                };
                crate::models::LeaderboardItem {
                    rank: (i + 1) as i32,
                    name: m.model.clone(),
                    tokens: m.total_input_tokens + m.total_output_tokens,
                    sessions: m.execution_count,
                    change,
                }
            })
            .collect();

        // Skills invocation statistics
        let skills_stats = match self.get_skills_stats(&time_filter).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Failed to load skills stats: {}", e);
                None
            }
        };

        // Backup statistics (filesystem scan)
        let backup_stats = match self.get_backup_stats().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Failed to load backup stats: {}", e);
                None
            }
        };

        Ok(crate::models::DashboardStats {
            total_todos,
            pending_todos,
            running_todos,
            completed_todos,
            failed_todos,
            total_tags,
            scheduled_todos,
            total_executions,
            success_executions,
            failed_executions,
            total_input_tokens,
            total_output_tokens,
            total_cache_read_tokens,
            total_cache_creation_tokens,
            total_cost_usd: total_cost,
            avg_duration_ms,
            executor_distribution,
            tag_distribution,
            model_distribution,
            daily_executions,
            daily_token_stats,
            recent_executions,
            trigger_type_distribution,
            executor_duration_stats,
            model_cache_stats,
            // Enhanced metrics
            today_executions,
            executions_change,
            success_rate_change,
            cost_change,
            active_days,
            streak_days,
            peak_daily_executions,
            top_model,
            top_model_tokens,
            leaderboard,
            // Skills & Backup metrics
            skills_stats,
            backup_stats,
        })
    }

    pub async fn get_execution_summary(
        &self,
        todo_id: i64,
    ) -> Result<ExecutionSummary, sea_orm::DbErr> {
        let backend = self.conn.get_database_backend();
        let sql = "SELECT \
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

        if let Some(row) = self
            .conn
            .query_one(Statement::from_sql_and_values(
                backend,
                sql,
                [todo_id.into()],
            ))
            .await?
        {
            let total_executions: i64 = row.try_get_by("total").unwrap_or(0);
            let success_count: i64 = row.try_get_by("success_count").unwrap_or(0);
            let failed_count: i64 = row.try_get_by("failed_count").unwrap_or(0);
            let running_count: i64 = row.try_get_by("running_count").unwrap_or(0);
            let input_tokens: i64 = row.try_get_by("input_tokens").unwrap_or(0);
            let output_tokens: i64 = row.try_get_by("output_tokens").unwrap_or(0);
            let cache_read: i64 = row.try_get_by("cache_read").unwrap_or(0);
            let cache_creation: i64 = row.try_get_by("cache_creation").unwrap_or(0);
            let total_cost: f64 = row.try_get_by("total_cost").unwrap_or(0.0);

            Ok(ExecutionSummary {
                todo_id,
                total_executions,
                success_count,
                failed_count,
                running_count,
                total_input_tokens: input_tokens as u64,
                total_output_tokens: output_tokens as u64,
                total_cache_read_tokens: cache_read as u64,
                total_cache_creation_tokens: cache_creation as u64,
                total_cost_usd: if total_cost > 0.0 {
                    Some(total_cost)
                } else {
                    None
                },
            })
        } else {
            Ok(ExecutionSummary {
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
            })
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

        // Skills overall stats
        let skills_overall_sql = format!(
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

        let (total_invocations, success_invocations, failed_invocations, avg_duration_ms, invocations_today) =
            if let Some(row) = self.conn.query_one(Statement::from_string(backend, skills_overall_sql)).await? {
                (
                    row.try_get_by::<i64, _>("total").unwrap_or(0),
                    row.try_get_by::<i64, _>("success").unwrap_or(0),
                    row.try_get_by::<i64, _>("failed").unwrap_or(0),
                    row.try_get_by::<f64, _>("avg_duration").unwrap_or(0.0),
                    row.try_get_by::<i64, _>("today").unwrap_or(0),
                )
            } else {
                (0, 0, 0, 0.0, 0)
            };

        // If no invocations, return None
        if total_invocations == 0 {
            return Ok(None);
        }

        // Top skills by invocation count
        let top_skills_sql = format!(
            "SELECT skill_name, COUNT(*) as count, \
            CAST(COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) AS FLOAT) / COUNT(*) * 100 as success_rate \
            FROM skill_invocations \
            WHERE invoked_at >= {} \
            GROUP BY skill_name \
            ORDER BY count DESC LIMIT 10",
            time_filter
        );

        let top_skills: Vec<crate::models::SkillTop> = self.conn
            .query_all(Statement::from_string(backend, top_skills_sql))
            .await?
            .into_iter()
            .filter_map(|row| {
                let skill_name: String = row.try_get_by("skill_name").ok()?;
                let count: i64 = row.try_get_by("count").ok()?;
                let success_rate: f64 = row.try_get_by("success_rate").ok()?;
                Some(crate::models::SkillTop {
                    skill_name,
                    count,
                    success_rate,
                })
            })
            .collect();

        // Executor skills count (distinct skill names per executor)
        let executor_skills_sql = format!(
            "SELECT executor, COUNT(DISTINCT skill_name) as skills_count \
            FROM skill_invocations \
            WHERE invoked_at >= {} \
            GROUP BY executor",
            time_filter
        );

        let executor_skills_count: Vec<crate::models::ExecutorSkillCount> = self.conn
            .query_all(Statement::from_string(backend, executor_skills_sql))
            .await?
            .into_iter()
            .filter_map(|row| {
                let executor: String = row.try_get_by("executor").ok()?;
                let skills_count: i64 = row.try_get_by("skills_count").ok()?;
                Some(crate::models::ExecutorSkillCount {
                    executor,
                    skills_count,
                })
            })
            .collect();

        // Daily skill invocations (last 30 days)
        let daily_skills_sql = format!(
            "SELECT SUBSTR(COALESCE(invoked_at, ''), 1, 10) as day, \
            COUNT(*) as count, \
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success \
            FROM skill_invocations \
            WHERE invoked_at IS NOT NULL AND LENGTH(invoked_at) >= 10 AND invoked_at >= {} \
            GROUP BY SUBSTR(invoked_at, 1, 10) \
            ORDER BY day DESC LIMIT 30",
            time_filter
        );

        let daily_invocations: Vec<crate::models::DailySkillInvocation> = self.conn
            .query_all(Statement::from_string(backend, daily_skills_sql))
            .await?
            .into_iter()
            .filter_map(|row| {
                let date: String = row.try_get_by("day").ok()?;
                let count: i64 = row.try_get_by("count").ok()?;
                let success: i64 = row.try_get_by("success").ok()?;
                Some(crate::models::DailySkillInvocation {
                    date,
                    count,
                    success,
                })
            })
            .collect();

        Ok(Some(crate::models::SkillsStats {
            total_invocations,
            success_invocations,
            failed_invocations,
            avg_duration_ms,
            invocations_today,
            top_skills,
            executor_skills_count,
            daily_invocations,
        }))
    }

    /// Get backup statistics by scanning filesystem
    async fn get_backup_stats(&self) -> Result<Option<crate::models::BackupStats>, sea_orm::DbErr> {
        let backup_dir = dirs::home_dir()
            .map(|h| h.join(".ntd").join("backups"))
            .unwrap_or_else(|| std::path::PathBuf::from(".ntd/backups"));

        if !backup_dir.exists() {
            return Ok(None);
        }

        // Scan backup subdirectories
        let database_stats = Self::scan_backup_category(&backup_dir.join("db"));
        let todo_stats = Self::scan_backup_category(&backup_dir.join("todo"));
        let skills_stats = Self::scan_backup_category(&backup_dir.join("skills"));

        let total_file_count = database_stats.file_count + todo_stats.file_count + skills_stats.file_count;
        let total_size = database_stats.total_size + todo_stats.total_size + skills_stats.total_size;

        // Collect recent backups from all categories
        let mut recent_backups: Vec<crate::models::RecentBackup> = Vec::new();

        if let Some(files) = Self::collect_backup_files(&backup_dir.join("db")) {
            for f in files.into_iter().take(5) {
                recent_backups.push(crate::models::RecentBackup {
                    backup_type: "database".to_string(),
                    name: f.name,
                    size: f.size,
                    created_at: f.created_at,
                });
            }
        }
        if let Some(files) = Self::collect_backup_files(&backup_dir.join("todo")) {
            for f in files.into_iter().take(5) {
                recent_backups.push(crate::models::RecentBackup {
                    backup_type: "todo".to_string(),
                    name: f.name,
                    size: f.size,
                    created_at: f.created_at,
                });
            }
        }
        if let Some(files) = Self::collect_backup_files(&backup_dir.join("skills")) {
            for f in files.into_iter().take(5) {
                recent_backups.push(crate::models::RecentBackup {
                    backup_type: "skills".to_string(),
                    name: f.name,
                    size: f.size,
                    created_at: f.created_at,
                });
            }
        }

        // Sort by created_at desc and take top 10
        recent_backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        recent_backups.truncate(10);

        // Find overall last backup time
        let last_backup = recent_backups.first().map(|b| b.created_at.clone());

        // Format total size
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
