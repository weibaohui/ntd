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
        }
    }
}

impl Database {
    pub async fn get_execution_records(
        &self,
        todo_id: i64,
        limit: i64,
        offset: i64,
        status: Option<&str>,
    ) -> Result<(Vec<ExecutionRecord>, i64), sea_orm::DbErr> {
        let base_filter = execution_records::Column::TodoId.eq(todo_id);
        let filter = if let Some(s) = status {
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

        let limit_u = if limit < 0 { 0 } else { limit as u64 };
        let offset_u = if offset < 0 { 0 } else { offset as u64 };

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
        id: i64,
        status: &str,
        remaining_logs: &str,
        result: &str,
        usage: Option<&ExecutionUsage>,
        model: Option<&str>,
    ) -> Result<bool, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let usage_json = usage.map(|u| {
            serde_json::to_string(u).unwrap_or_else(|e| {
                tracing::error!("Failed to serialize usage: {}", e);
                String::new()
            })
        });
        let model_val = model.map(|s| s.to_string());

        // Use raw SQL with WHERE status='running' to prevent race condition:
        // both the stop handler and spawned task's cancellation branch may try to
        // update the same record concurrently -- only the first write succeeds.
        let backend = self.conn.get_database_backend();
        let sql = "UPDATE execution_records SET \
            status = $1, \
            result = $2, \
            usage = $3, \
            model = $4, \
            finished_at = $5 \
            WHERE id = $6 AND status = 'running'";

        let res = self
            .conn
            .execute(Statement::from_sql_and_values(
                backend,
                sql,
                [
                    status.into(),
                    result.into(),
                    usage_json.into(),
                    model_val.into(),
                    now.into(),
                    id.into(),
                ],
            ))
            .await?;
        let updated = res.rows_affected() > 0;

        // Only insert logs if the status update succeeded (prevent duplicate logs on concurrent writes)
        if updated && !remaining_logs.is_empty() && remaining_logs != "[]" {
            self.insert_execution_logs(id, remaining_logs).await?;
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
}
