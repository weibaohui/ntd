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
    pub todo_id: Option<i64>,
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

        // 创建查询上下文，避免在每个函数中重复传递连接和过滤条件
        let ctx = crate::db::dashboard::DashboardQueryContext {
            conn: &self.conn,
            backend,
            time_filter,
            heatmap_filter,
        };

        // 并行执行独立的查询，提高性能
        let (
            (total_todos, pending_todos, running_todos, completed_todos, failed_todos, scheduled_todos),
            (total_executions, success_executions, failed_executions, total_input_tokens, total_output_tokens, total_cache_read_tokens, total_cache_creation_tokens, total_cost, total_duration, duration_count),
            executor_todo_counts,
            tags_result,
        ) = tokio::try_join!(
            crate::db::dashboard::fetch_todo_stats(&ctx),
            crate::db::dashboard::fetch_execution_overall(&ctx),
            crate::db::dashboard::fetch_executor_todo_counts(&ctx),
            self.get_tags(),
        )?;

        let total_tags = tags_result.len() as i64;

        // 计算平均时长
        let avg_duration_ms = if duration_count > 0 {
            total_duration / duration_count
        } else {
            0
        };

        // 并行执行独立的分布查询
        let (
            executor_distribution,
            model_distribution,
            trigger_type_distribution,
            executor_duration_stats,
            model_cache_stats,
            (daily_executions, daily_token_stats),
            recent_executions,
            (today_executions, executions_change),
            success_rate_change,
            cost_change,
        ) = tokio::try_join!(
            crate::db::dashboard::fetch_executor_distribution(&ctx, &executor_todo_counts),
            crate::db::dashboard::fetch_model_distribution(&ctx),
            crate::db::dashboard::fetch_trigger_distribution(&ctx),
            crate::db::dashboard::fetch_executor_durations(&ctx),
            crate::db::dashboard::fetch_model_cache_stats(&ctx),
            crate::db::dashboard::fetch_daily_stats(&ctx, heatmap_limit),
            crate::db::dashboard::fetch_recent_executions(&ctx),
            crate::db::dashboard::fetch_execution_change(&ctx),
            crate::db::dashboard::fetch_success_rate_change(&ctx),
            crate::db::dashboard::fetch_cost_change(&ctx),
        )?;

        // 计算标签分布（需要 tags 和 tag_todo_counts）
        // 使用 raw SQL 而非 SeaORM：GROUP BY + COUNT 聚合查询需要同时返回
        // 两个字段并映射为 HashMap，ORM 方式会引入额外的中间结构体，
        // raw SQL 更直接且 SeaORM 不会为这种聚合查询生成类型安全的 API。
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

        let tag_distribution = crate::db::dashboard::fetch_tag_distribution(
            &ctx, &tags_result, &tag_todo_counts,
        ).await?;

        // 计算连续活跃天数
        let active_days = daily_executions.len() as i64;
        let streak_days = crate::db::dashboard::calculate_streak_days(&daily_executions);

        // 计算峰值每日执行数
        let peak_daily_executions = daily_executions.iter()
            .map(|d| d.success + d.failed)
            .max()
            .unwrap_or(0);

        // 找出最佳模型
        let (top_model, top_model_tokens) = if !model_distribution.is_empty() {
            let top = model_distribution.iter()
                .max_by_key(|m| m.total_input_tokens + m.total_output_tokens)
                .unwrap();
            (Some(top.model.clone()), Some(top.total_input_tokens + top.total_output_tokens))
        } else {
            (None, None)
        };

        // 构建排行榜
        let leaderboard = crate::db::dashboard::build_leaderboard(&model_distribution);

        // Skills 统计
        let skills_stats = match self.get_skills_stats(&ctx.time_filter).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Failed to load skills stats: {}", e);
                None
            }
        };

        // Backup 统计
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
