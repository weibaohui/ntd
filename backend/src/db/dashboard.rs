//! Dashboard statistics query helpers.
//!
//! 提取自 `execution.rs` 中的 `get_dashboard_stats` 函数（705 行），
//! 将各个独立的 SQL 查询和结果解析拆分为独立的异步函数。
//!
//! 每个函数职责单一：执行一条 SQL 查询、解析结果、返回领域结构体。
//! `get_dashboard_stats` 作为协调者，使用 `tokio::try_join!` 并行调用所有
//! 独立查询函数，组装最终的 `DashboardStats`。

use std::collections::HashMap;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use crate::models::{
    DailyExecution, DailyTokenStats, ExecutionRecord,
    ExecutorCount, ExecutorDuration, ModelCacheStat, ModelCount, TagCount,
    TriggerTypeCount, LeaderboardItem,
};

/// 辅助结构体：携带数据库连接和时间过滤条件，避免在每个函数中重复传递。
pub(super) struct DashboardQueryContext<'a> {
    pub conn: &'a dyn ConnectionTrait,
    pub backend: DbBackend,
    pub time_filter: String,
    pub heatmap_filter: String,
}

/// 查询 Todo 统计（总数、各状态计数、定时任务数）。
pub(super) async fn fetch_todo_stats(
    ctx: &DashboardQueryContext<'_>,
) -> Result<(i64, i64, i64, i64, i64, i64), sea_orm::DbErr> {
    let sql = "SELECT \
        COUNT(*) as total, \
        COALESCE(SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END), 0) as pending, \
        COALESCE(SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END), 0) as running, \
        COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0) as completed, \
        COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed, \
        COALESCE(SUM(CASE WHEN scheduler_enabled = 1 AND scheduler_config IS NOT NULL THEN 1 ELSE 0 END), 0) as scheduled \
        FROM todos WHERE deleted_at IS NULL";

    if let Some(row) = ctx.conn
        .query_one(Statement::from_string(ctx.backend, sql.to_string()))
        .await?
    {
        Ok((
            row.try_get_by("total").unwrap_or(0i64),
            row.try_get_by("pending").unwrap_or(0i64),
            row.try_get_by("running").unwrap_or(0i64),
            row.try_get_by("completed").unwrap_or(0i64),
            row.try_get_by("failed").unwrap_or(0i64),
            row.try_get_by("scheduled").unwrap_or(0i64),
        ))
    } else {
        Ok((0, 0, 0, 0, 0, 0))
    }
}

/// 查询执行记录总体统计（成功/失败数、token 总量、费用、时长）。
pub(super) async fn fetch_execution_overall(
    ctx: &DashboardQueryContext<'_>,
) -> Result<(i64, i64, i64, u64, u64, u64, u64, f64, u64, u64), sea_orm::DbErr> {
    let sql = format!(
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
        ctx.time_filter
    );

    if let Some(row) = ctx.conn
        .query_one(Statement::from_string(ctx.backend, sql))
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
        Ok((t, s, f, it as u64, ot as u64, cr as u64, cc as u64, tc, td as u64, dc as u64))
    } else {
        Ok((0, 0, 0, 0, 0, 0, 0, 0.0, 0, 0))
    }
}

/// 查询每个执行器的 Todo 数量。
pub(super) async fn fetch_executor_todo_counts(
    ctx: &DashboardQueryContext<'_>,
) -> Result<HashMap<String, i64>, sea_orm::DbErr> {
    let sql = "SELECT \
        COALESCE(executor, 'claudecode') as executor, \
        COUNT(*) as todo_count \
        FROM todos WHERE deleted_at IS NULL \
        GROUP BY COALESCE(executor, 'claudecode')";

    let rows = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql.to_string()))
        .await?;

    Ok(rows.into_iter()
        .filter_map(|row| {
            let exec: String = row.try_get_by("executor").ok()?;
            let count: i64 = row.try_get_by("todo_count").ok()?;
            Some((exec, count))
        })
        .collect())
}

/// 查询执行器分布（执行次数、成功/失败数、token、费用）。
pub(super) async fn fetch_executor_distribution(
    ctx: &DashboardQueryContext<'_>,
    executor_todo_counts: &HashMap<String, i64>,
) -> Result<Vec<ExecutorCount>, sea_orm::DbErr> {
    let sql = format!(
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
        ctx.time_filter
    );

    let mut distribution: Vec<ExecutorCount> = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql))
        .await?
        .into_iter()
        .filter_map(|row| {
            let exec: String = row.try_get_by("executor").ok()?;
            let ec: i64 = row.try_get_by("execution_count").ok()?;
            if ec == 0 { return None; }
            let sc: i64 = row.try_get_by("success_count").ok()?;
            let fc: i64 = row.try_get_by("failed_count").ok()?;
            let it: i64 = row.try_get_by("input_tokens").ok()?;
            let ot: i64 = row.try_get_by("output_tokens").ok()?;
            let cost: f64 = row.try_get_by("cost").ok()?;
            Some(ExecutorCount {
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
    distribution.sort_by(|a, b| b.execution_count.cmp(&a.execution_count));
    Ok(distribution)
}

/// 查询模型分布。
pub(super) async fn fetch_model_distribution(
    ctx: &DashboardQueryContext<'_>,
) -> Result<Vec<ModelCount>, sea_orm::DbErr> {
    let sql = format!(
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
        ctx.time_filter
    );

    let mut distribution: Vec<ModelCount> = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql))
        .await?
        .into_iter()
        .filter_map(|row| {
            let model: String = row.try_get_by("model").ok()?;
            let ec: i64 = row.try_get_by("execution_count").ok()?;
            if ec == 0 { return None; }
            let sc: i64 = row.try_get_by("success_count").ok()?;
            let fc: i64 = row.try_get_by("failed_count").ok()?;
            let it: i64 = row.try_get_by("input_tokens").ok()?;
            let ot: i64 = row.try_get_by("output_tokens").ok()?;
            let cr: i64 = row.try_get_by("cache_read").ok()?;
            let cc: i64 = row.try_get_by("cache_creation").ok()?;
            let cost: f64 = row.try_get_by("cost").ok()?;
            Some(ModelCount {
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
    distribution.sort_by(|a, b| b.execution_count.cmp(&a.execution_count));
    Ok(distribution)
}

/// 查询触发类型分布。
pub(super) async fn fetch_trigger_distribution(
    ctx: &DashboardQueryContext<'_>,
) -> Result<Vec<TriggerTypeCount>, sea_orm::DbErr> {
    let sql = format!(
        "SELECT \
        COALESCE(trigger_type, 'manual') as trigger_type, \
        COUNT(*) as count, \
        COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success_count, \
        COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed_count \
        FROM execution_records \
        WHERE started_at >= {} \
        GROUP BY COALESCE(trigger_type, 'manual')",
        ctx.time_filter
    );

    let mut distribution: Vec<TriggerTypeCount> = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql))
        .await?
        .into_iter()
        .filter_map(|row| {
            let tt: String = row.try_get_by("trigger_type").ok()?;
            let c: i64 = row.try_get_by("count").ok()?;
            let sc: i64 = row.try_get_by("success_count").ok()?;
            let fc: i64 = row.try_get_by("failed_count").ok()?;
            Some(TriggerTypeCount {
                trigger_type: tt,
                count: c,
                success_count: sc,
                failed_count: fc,
            })
        })
        .collect();
    distribution.sort_by(|a, b| b.count.cmp(&a.count));
    Ok(distribution)
}

/// 查询执行器平均时长。
pub(super) async fn fetch_executor_durations(
    ctx: &DashboardQueryContext<'_>,
) -> Result<Vec<ExecutorDuration>, sea_orm::DbErr> {
    let sql = format!(
        "SELECT \
        COALESCE(executor, 'claudecode') as executor, \
        ROUND(AVG(json_extract(usage, '$.duration_ms')), 0) as avg_duration, \
        COUNT(*) as execution_count \
        FROM execution_records \
        WHERE started_at >= {} AND json_extract(usage, '$.duration_ms') IS NOT NULL \
        GROUP BY COALESCE(executor, 'claudecode')",
        ctx.time_filter
    );

    let mut stats: Vec<ExecutorDuration> = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql))
        .await?
        .into_iter()
        .filter_map(|row| {
            let exec: String = row.try_get_by("executor").ok()?;
            let ad: f64 = row.try_get_by("avg_duration").ok()?;
            let ec: i64 = row.try_get_by("execution_count").ok()?;
            Some(ExecutorDuration {
                executor: exec,
                avg_duration_ms: ad,
                execution_count: ec,
            })
        })
        .collect();
    stats.sort_by(|a, b| b.avg_duration_ms.partial_cmp(&a.avg_duration_ms).unwrap_or(std::cmp::Ordering::Equal));
    Ok(stats)
}

/// 查询模型缓存统计。
pub(super) async fn fetch_model_cache_stats(
    ctx: &DashboardQueryContext<'_>,
) -> Result<Vec<ModelCacheStat>, sea_orm::DbErr> {
    let sql = format!(
        "SELECT \
        COALESCE(model, 'unknown') as model, \
        COALESCE(SUM(COALESCE(json_extract(usage, '$.input_tokens'), 0)), 0) as input_tokens, \
        COALESCE(SUM(COALESCE(json_extract(usage, '$.cache_read_input_tokens'), 0)), 0) as cache_read \
        FROM execution_records \
        WHERE started_at >= {} AND model IS NOT NULL \
        GROUP BY COALESCE(model, 'unknown')",
        ctx.time_filter
    );

    let mut stats: Vec<ModelCacheStat> = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql))
        .await?
        .into_iter()
        .filter_map(|row| {
            let model: String = row.try_get_by("model").ok()?;
            let it: i64 = row.try_get_by("input_tokens").ok()?;
            let cr: i64 = row.try_get_by("cache_read").ok()?;
            let total = it as u64 + cr as u64;
            let rate = if total > 0 { cr as f64 / total as f64 * 100.0 } else { 0.0 };
            Some(ModelCacheStat {
                model,
                total_input_tokens: it as u64,
                total_cache_read_tokens: cr as u64,
                cache_hit_rate: rate,
            })
        })
        .collect();
    stats.sort_by(|a, b| b.cache_hit_rate.partial_cmp(&a.cache_hit_rate).unwrap_or(std::cmp::Ordering::Equal));
    stats.retain(|m| m.total_input_tokens > 0 || m.total_cache_read_tokens > 0);
    Ok(stats)
}

/// 查询每日执行统计（用于热力图）。
pub(super) async fn fetch_daily_stats(
    ctx: &DashboardQueryContext<'_>,
    heatmap_limit: usize,
) -> Result<(Vec<DailyExecution>, Vec<DailyTokenStats>), sea_orm::DbErr> {
    let sql = format!(
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
        ctx.heatmap_filter, heatmap_limit
    );

    let rows = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql))
        .await?;

    let mut daily_executions = Vec::with_capacity(rows.len());
    let mut daily_token_stats = Vec::with_capacity(rows.len());
    for row in &rows {
        let day: String = row.try_get_by("day").unwrap_or_default();
        let success: i64 = row.try_get_by("success").unwrap_or(0);
        let failed: i64 = row.try_get_by("failed").unwrap_or(0);
        daily_executions.push(DailyExecution {
            date: day.clone(),
            success,
            failed,
        });

        let it: i64 = row.try_get_by("input_tokens").unwrap_or(0);
        let ot: i64 = row.try_get_by("output_tokens").unwrap_or(0);
        let cr: i64 = row.try_get_by("cache_read").unwrap_or(0);
        let cc: i64 = row.try_get_by("cache_creation").unwrap_or(0);
        let cost: f64 = row.try_get_by("cost").unwrap_or(0.0);
        daily_token_stats.push(DailyTokenStats {
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
    Ok((daily_executions, daily_token_stats))
}

/// 查询标签分布。
pub(super) async fn fetch_tag_distribution(
    ctx: &DashboardQueryContext<'_>,
    tags: &[crate::models::Tag],
    tag_todo_counts: &HashMap<i64, i64>,
) -> Result<Vec<TagCount>, sea_orm::DbErr> {
    let sql = format!(
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
        ctx.time_filter
    );

    let tag_rows = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql))
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

    let mut distribution: Vec<TagCount> = tags
        .iter()
        .filter_map(|t| {
            let todo_count = *tag_todo_counts.get(&t.id).unwrap_or(&0);
            if todo_count == 0 { return None; }
            let (ec, sc, fc, it, ot, cost) = tag_exec_stats
                .get(&t.id)
                .copied()
                .unwrap_or((0, 0, 0, 0, 0, 0.0));
            Some(TagCount {
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
    distribution.sort_by(|a, b| b.execution_count.cmp(&a.execution_count));
    Ok(distribution)
}

/// 查询最近 10 条执行记录（轻量字段）。
pub(super) async fn fetch_recent_executions(
    ctx: &DashboardQueryContext<'_>,
) -> Result<Vec<ExecutionRecord>, sea_orm::DbErr> {
    let sql = format!(
        "SELECT id, todo_id, executor, trigger_type, status, started_at, finished_at, usage, task_id, session_id, result, resume_message FROM execution_records \
        WHERE started_at >= {} \
        ORDER BY started_at DESC LIMIT 10",
        ctx.time_filter
    );

    use crate::db::entity::execution_records;
    let records: Vec<ExecutionRecord> = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql))
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
                // 其余未 SELECT 的字段使用 Default（Option 字段为 None，id 为 0）
                // 未来新增字段会自动得到 Default 值，无需手动维护 None 列表
                ..execution_records::Model::default()
            }
        })
        .map(Into::into)
        .collect();
    Ok(records)
}

/// 查询今日/昨日执行数量，计算变化率。
pub(super) async fn fetch_execution_change(
    ctx: &DashboardQueryContext<'_>,
) -> Result<(i64, Option<f64>), sea_orm::DbErr> {
    let today_sql = "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now')";
    let yesterday_sql = "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now', '-1 day')";

    let today_executions: i64 = ctx.conn
        .query_one(Statement::from_string(ctx.backend, today_sql.to_string()))
        .await?
        .map(|row| row.try_get_by("count").ok().unwrap_or(0))
        .unwrap_or(0);

    let yesterday_executions: i64 = ctx.conn
        .query_one(Statement::from_string(ctx.backend, yesterday_sql.to_string()))
        .await?
        .map(|row| row.try_get_by("count").ok().unwrap_or(0))
        .unwrap_or(0);

    let change = if yesterday_executions > 0 {
        Some((today_executions as f64 - yesterday_executions as f64) / yesterday_executions as f64 * 100.0)
    } else {
        None
    };

    Ok((today_executions, change))
}

/// 查询今日/昨日成功率变化。
pub(super) async fn fetch_success_rate_change(
    ctx: &DashboardQueryContext<'_>,
) -> Result<Option<f64>, sea_orm::DbErr> {
    let yesterday_success: i64 = ctx.conn
        .query_one(Statement::from_string(ctx.backend, "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now', '-1 day') AND status = 'success'".to_string()))
        .await?
        .map(|row| row.try_get_by("count").ok().unwrap_or(0))
        .unwrap_or(0);

    let yesterday_failed: i64 = ctx.conn
        .query_one(Statement::from_string(ctx.backend, "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now', '-1 day') AND status = 'failed'".to_string()))
        .await?
        .map(|row| row.try_get_by("count").ok().unwrap_or(0))
        .unwrap_or(0);

    let today_success: i64 = ctx.conn
        .query_one(Statement::from_string(ctx.backend, "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now') AND status = 'success'".to_string()))
        .await?
        .map(|row| row.try_get_by("count").ok().unwrap_or(0))
        .unwrap_or(0);

    let today_failed: i64 = ctx.conn
        .query_one(Statement::from_string(ctx.backend, "SELECT COUNT(*) as count FROM execution_records WHERE date(started_at) = date('now') AND status = 'failed'".to_string()))
        .await?
        .map(|row| row.try_get_by("count").ok().unwrap_or(0))
        .unwrap_or(0);

    let yesterday_total = yesterday_success + yesterday_failed;
    let today_total = today_success + today_failed;
    let yesterday_rate = if yesterday_total > 0 { yesterday_success as f64 / yesterday_total as f64 * 100.0 } else { 0.0 };
    let today_rate = if today_total > 0 { today_success as f64 / today_total as f64 * 100.0 } else { 0.0 };
    Ok(if yesterday_total > 0 { Some(today_rate - yesterday_rate) } else { None })
}

/// 查询今日/昨日费用变化。
pub(super) async fn fetch_cost_change(
    ctx: &DashboardQueryContext<'_>,
) -> Result<Option<f64>, sea_orm::DbErr> {
    let today_cost: f64 = ctx.conn
        .query_one(Statement::from_string(ctx.backend, "SELECT COALESCE(SUM(COALESCE(json_extract(usage, '$.total_cost_usd'), 0.0)), 0.0) as cost FROM execution_records WHERE date(started_at) = date('now')".to_string()))
        .await?
        .map(|row| row.try_get_by("cost").ok().unwrap_or(0.0))
        .unwrap_or(0.0);

    let yesterday_cost: f64 = ctx.conn
        .query_one(Statement::from_string(ctx.backend, "SELECT COALESCE(SUM(COALESCE(json_extract(usage, '$.total_cost_usd'), 0.0)), 0.0) as cost FROM execution_records WHERE date(started_at) = date('now', '-1 day')".to_string()))
        .await?
        .map(|row| row.try_get_by("cost").ok().unwrap_or(0.0))
        .unwrap_or(0.0);

    Ok(if yesterday_cost > 0.0 {
        Some((today_cost - yesterday_cost) / yesterday_cost * 100.0)
    } else {
        None
    })
}

/// 计算连续活跃天数（从 daily_executions 倒序计数）。
pub fn calculate_streak_days(daily_executions: &[DailyExecution]) -> i64 {
    if daily_executions.is_empty() {
        return 0;
    }

    let today_str = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let yesterday_str = (chrono::Utc::now() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();

    let has_recent = daily_executions.iter().any(|d| {
        let has_exec = d.success + d.failed > 0;
        let is_today_or_yesterday = d.date == today_str || d.date == yesterday_str;
        has_exec && is_today_or_yesterday
    });

    if !has_recent {
        return 0;
    }

    let mut streak = 0i64;
    for day in daily_executions.iter().rev() {
        if day.success + day.failed > 0 {
            streak += 1;
        } else {
            break;
        }
    }
    streak
}

/// 从模型分布构建排行榜。
pub fn build_leaderboard(model_distribution: &[ModelCount]) -> Vec<LeaderboardItem> {
    model_distribution.iter()
        .enumerate()
        .map(|(i, m)| {
            let half = model_distribution.len() / 2;
            let change = if i < half && i + half < model_distribution.len() {
                let first_half_tokens = model_distribution[..i+1].iter()
                    .map(|x| x.total_input_tokens + x.total_output_tokens)
                    .sum::<u64>() as f64;
                let second_half_tokens = model_distribution[i..i+half].iter()
                    .map(|x| x.total_input_tokens + x.total_output_tokens)
                    .sum::<u64>() as f64;
                if first_half_tokens > 0.0 {
                    Some((second_half_tokens - first_half_tokens) / first_half_tokens * 100.0)
                } else {
                    None
                }
            } else {
                None
            };
            LeaderboardItem {
                rank: (i + 1) as i32,
                name: m.model.clone(),
                tokens: m.total_input_tokens + m.total_output_tokens,
                sessions: m.execution_count,
                change,
            }
        })
        .collect()
}
