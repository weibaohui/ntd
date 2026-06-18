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

/// Dashboard 原始统计数据（查询阶段产出）。
///
/// 拆分 `get_dashboard_stats` 的关键中间结构：把所有 fetch_* 的结果装到一个
/// 平铺的 struct 中，第二阶段 `build_dashboard_stats` 再做派生计算。
/// 命名约定：所有 u64 数值与 `DashboardStats` 同名字段一一对应，便于逐字段搬移。
pub(super) struct RawDashboardStats {
    // Todo stats
    pub total_todos: i64,
    pub pending_todos: i64,
    pub running_todos: i64,
    pub completed_todos: i64,
    pub failed_todos: i64,
    pub scheduled_todos: i64,
    pub total_tags: i64,
    // Execution overall
    pub total_executions: i64,
    pub success_executions: i64,
    pub failed_executions: i64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cost: f64,
    pub total_duration: u64,
    pub duration_count: u64,
    // Distributions
    pub executor_distribution: Vec<ExecutorCount>,
    pub tag_distribution: Vec<TagCount>,
    pub model_distribution: Vec<ModelCount>,
    pub trigger_type_distribution: Vec<TriggerTypeCount>,
    pub executor_duration_stats: Vec<ExecutorDuration>,
    pub model_cache_stats: Vec<ModelCacheStat>,
    // Time-series + recents
    pub daily_executions: Vec<DailyExecution>,
    pub daily_token_stats: Vec<DailyTokenStats>,
    pub recent_executions: Vec<ExecutionRecord>,
    // Enhanced metrics (raw, 派生计算在第二阶段)
    pub today_executions: i64,
    pub executions_change: Option<f64>,
    pub success_rate_change: Option<f64>,
    pub cost_change: Option<f64>,
}

/// 第一轮并行查询的产出（`fetch_dashboard_base_stats` 的返回类型）。
pub(super) struct BaseStats {
    /// `(total, pending, running, completed, failed, scheduled)` 元组
    pub todo_stats: (i64, i64, i64, i64, i64, i64),
    /// `(total, success, failed, input_tokens, output_tokens, cache_read,
    ///   cache_creation, total_cost, total_duration, duration_count)` 元组
    pub execution_overall: (i64, i64, i64, u64, u64, u64, u64, f64, u64, u64),
    /// 每个 executor 对应的 todo 数量
    pub executor_todo_counts: std::collections::HashMap<String, i64>,
    /// 全部 tag 列表（用于派生 tag_distribution）
    pub tags: Vec<crate::models::Tag>,
}

/// 第二轮并行查询的产出（`fetch_dashboard_distribution_stats` 的返回类型）。
pub(super) struct DistributionStats {
    pub executor_distribution: Vec<ExecutorCount>,
    pub model_distribution: Vec<ModelCount>,
    pub trigger_type_distribution: Vec<TriggerTypeCount>,
    pub executor_duration_stats: Vec<ExecutorDuration>,
    pub model_cache_stats: Vec<ModelCacheStat>,
    /// `(daily_executions, daily_token_stats)` 平行数组
    pub daily_stats: (Vec<DailyExecution>, Vec<DailyTokenStats>),
    pub recent_executions: Vec<ExecutionRecord>,
    /// `(today_executions, executions_change)` 元组
    pub execution_change: (i64, Option<f64>),
    pub success_rate_change: Option<f64>,
    pub cost_change: Option<f64>,
    pub tag_distribution: Vec<TagCount>,
}

/// Dashboard 派生指标（基于 `RawDashboardStats` 的纯函数计算结果）。
pub(super) struct DerivedDashboardMetrics {
    pub avg_duration_ms: u64,
    pub active_days: i64,
    pub streak_days: i64,
    pub peak_daily_executions: i64,
    pub top_model: Option<String>,
    pub top_model_tokens: Option<u64>,
    pub leaderboard: Vec<LeaderboardItem>,
}

/// 一次性计算 `RawDashboardStats` 中的全部派生指标。
///
/// 之所以抽到 dashboard 模块而不是 execution.rs：
/// - 这些函数都是纯函数（无 DB / 无 self），放在通用模块便于单元测试；
/// - `get_dashboard_stats` 不再被派生计算代码撑爆，主函数保持「协调」定位。
pub fn compute_dashboard_derived(raw: &RawDashboardStats) -> DerivedDashboardMetrics {
    DerivedDashboardMetrics {
        avg_duration_ms: compute_avg_duration(raw.total_duration, raw.duration_count),
        active_days: compute_active_days(&raw.daily_executions),
        streak_days: calculate_streak_days(&raw.daily_executions),
        peak_daily_executions: compute_peak_daily_executions(&raw.daily_executions),
        top_model: find_top_model(&raw.model_distribution).0,
        top_model_tokens: find_top_model(&raw.model_distribution).1,
        leaderboard: build_leaderboard(&raw.model_distribution),
    }
}

/// 把 `BaseStats` + `DistributionStats` 合并为 `RawDashboardStats`。
///
/// 独立成函数的目的：让 `fetch_dashboard_raw_stats` 主函数只剩「并行两轮
/// → 合并」两步，避免被 30 行的字段搬移撑爆。
pub fn assemble_raw_dashboard_stats(base: BaseStats, dist: DistributionStats) -> RawDashboardStats {
    let (total_todos, pending_todos, running_todos, completed_todos, failed_todos, scheduled_todos) =
        base.todo_stats;
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
    ) = base.execution_overall;
    let (daily_executions, daily_token_stats) = dist.daily_stats;
    let (today_executions, executions_change) = dist.execution_change;
    RawDashboardStats {
        total_todos,
        pending_todos,
        running_todos,
        completed_todos,
        failed_todos,
        scheduled_todos,
        total_tags: base.tags.len() as i64,
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
        executor_distribution: dist.executor_distribution,
        tag_distribution: dist.tag_distribution,
        model_distribution: dist.model_distribution,
        trigger_type_distribution: dist.trigger_type_distribution,
        executor_duration_stats: dist.executor_duration_stats,
        model_cache_stats: dist.model_cache_stats,
        daily_executions,
        daily_token_stats,
        recent_executions: dist.recent_executions,
        today_executions,
        executions_change,
        success_rate_change: dist.success_rate_change,
        cost_change: dist.cost_change,
    }
}

/// 把原始数据、派生指标、Skills/Backup 软失败结果组装为 `DashboardStats`。
///
/// 独立成函数的目的：让 `build_dashboard_stats` 不再被 30 行的 struct literal
/// 撑爆。`SkillsStats` / `BackupStats` 通过参数传入，
/// `execution.rs` 负责「软失败包装」，本函数只负责「逐字段搬移」。
pub fn assemble_dashboard_response(
    raw: RawDashboardStats,
    derived: DerivedDashboardMetrics,
    skills_stats: Option<crate::models::SkillsStats>,
    backup_stats: Option<crate::models::BackupStats>,
) -> crate::models::DashboardStats {
    crate::models::DashboardStats {
        total_todos: raw.total_todos,
        pending_todos: raw.pending_todos,
        running_todos: raw.running_todos,
        completed_todos: raw.completed_todos,
        failed_todos: raw.failed_todos,
        total_tags: raw.total_tags,
        scheduled_todos: raw.scheduled_todos,
        total_executions: raw.total_executions,
        success_executions: raw.success_executions,
        failed_executions: raw.failed_executions,
        total_input_tokens: raw.total_input_tokens,
        total_output_tokens: raw.total_output_tokens,
        total_cache_read_tokens: raw.total_cache_read_tokens,
        total_cache_creation_tokens: raw.total_cache_creation_tokens,
        total_cost_usd: raw.total_cost,
        avg_duration_ms: derived.avg_duration_ms,
        executor_distribution: raw.executor_distribution,
        tag_distribution: raw.tag_distribution,
        model_distribution: raw.model_distribution,
        daily_executions: raw.daily_executions,
        daily_token_stats: raw.daily_token_stats,
        recent_executions: raw.recent_executions,
        trigger_type_distribution: raw.trigger_type_distribution,
        executor_duration_stats: raw.executor_duration_stats,
        model_cache_stats: raw.model_cache_stats,
        today_executions: raw.today_executions,
        executions_change: raw.executions_change,
        success_rate_change: raw.success_rate_change,
        cost_change: raw.cost_change,
        active_days: derived.active_days,
        streak_days: derived.streak_days,
        peak_daily_executions: derived.peak_daily_executions,
        top_model: derived.top_model,
        top_model_tokens: derived.top_model_tokens,
        leaderboard: derived.leaderboard,
        skills_stats,
        backup_stats,
    }
}

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

// =============================================================================
// 纯派生计算函数（无副作用，便于单元测试）
// =============================================================================

/// 计算平均执行时长（毫秒）。`duration_count == 0` 时返回 0，避免除零。
pub fn compute_avg_duration(total_duration_ms: u64, duration_count: u64) -> u64 {
    if duration_count > 0 {
        total_duration_ms / duration_count
    } else {
        0
    }
}

/// 计算峰值每日执行数（success + failed 之和的最大值）。
pub fn compute_peak_daily_executions(daily_executions: &[DailyExecution]) -> i64 {
    daily_executions
        .iter()
        .map(|d| d.success + d.failed)
        .max()
        .unwrap_or(0)
}

/// 计算活跃天数（daily_executions 非空条目的数量）。
pub fn compute_active_days(daily_executions: &[DailyExecution]) -> i64 {
    daily_executions.len() as i64
}

/// 找出 token 用量最高的模型（按 input + output 之和）。
pub fn find_top_model(model_distribution: &[ModelCount]) -> (Option<String>, Option<u64>) {
    match model_distribution.iter().max_by_key(|m| m.total_input_tokens + m.total_output_tokens) {
        Some(top) => (
            Some(top.model.clone()),
            Some(top.total_input_tokens + top.total_output_tokens),
        ),
        None => (None, None),
    }
}

/// 查询 todo_tags 表，得到 (tag_id -> todo_count) 映射。
pub(super) async fn fetch_tag_todo_counts(
    ctx: &DashboardQueryContext<'_>,
) -> Result<HashMap<i64, i64>, sea_orm::DbErr> {
    let sql = "SELECT tag_id, COUNT(*) as todo_count FROM todo_tags GROUP BY tag_id";
    let rows = ctx.conn
        .query_all(Statement::from_string(ctx.backend, sql.to_string()))
        .await?;
    Ok(rows.into_iter()
        .filter_map(|row| {
            let tag_id: i64 = row.try_get_by("tag_id").ok()?;
            let count: i64 = row.try_get_by("todo_count").ok()?;
            Some((tag_id, count))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_model(name: &str, input: u64, output: u64) -> ModelCount {
        ModelCount {
            model: name.into(),
            count: 0,
            execution_count: 1,
            success_count: 1,
            failed_count: 0,
            total_input_tokens: input,
            total_output_tokens: output,
            total_cache_read_tokens: 0,
            total_cache_creation_tokens: 0,
            total_cost_usd: 0.0,
        }
    }

    fn make_raw(total_duration: u64, duration_count: u64, daily: Vec<DailyExecution>, models: Vec<ModelCount>) -> RawDashboardStats {
        RawDashboardStats {
            total_todos: 0, pending_todos: 0, running_todos: 0, completed_todos: 0, failed_todos: 0,
            scheduled_todos: 0, total_tags: 0,
            total_executions: 0, success_executions: 0, failed_executions: 0,
            total_input_tokens: 0, total_output_tokens: 0,
            total_cache_read_tokens: 0, total_cache_creation_tokens: 0,
            total_cost: 0.0, total_duration, duration_count,
            executor_distribution: vec![], tag_distribution: vec![], model_distribution: models,
            trigger_type_distribution: vec![], executor_duration_stats: vec![], model_cache_stats: vec![],
            daily_executions: daily,
            daily_token_stats: vec![],
            recent_executions: vec![],
            today_executions: 0, executions_change: None, success_rate_change: None, cost_change: None,
        }
    }

    #[test]
    fn test_compute_avg_duration_normal() {
        assert_eq!(compute_avg_duration(120_000, 3), 40_000);
    }

    #[test]
    fn test_compute_avg_duration_zero_count_returns_zero() {
        assert_eq!(compute_avg_duration(123_456, 0), 0);
    }

    #[test]
    fn test_compute_avg_duration_zero_total() {
        assert_eq!(compute_avg_duration(0, 5), 0);
    }

    #[test]
    fn test_compute_peak_daily_executions_picks_max_sum() {
        let daily = vec![
            DailyExecution { date: "2026-06-01".into(), success: 1, failed: 2 },
            DailyExecution { date: "2026-06-02".into(), success: 3, failed: 3 },
            DailyExecution { date: "2026-06-03".into(), success: 5, failed: 1 },
            DailyExecution { date: "2026-06-04".into(), success: 0, failed: 0 },
        ];
        assert_eq!(compute_peak_daily_executions(&daily), 6);
    }

    #[test]
    fn test_compute_peak_daily_executions_empty_returns_zero() {
        assert_eq!(compute_peak_daily_executions(&[]), 0);
    }

    #[test]
    fn test_compute_active_days_returns_length() {
        let daily = vec![
            DailyExecution { date: "2026-06-01".into(), success: 1, failed: 0 },
            DailyExecution { date: "2026-06-02".into(), success: 0, failed: 0 },
        ];
        assert_eq!(compute_active_days(&daily), 2);
    }

    #[test]
    fn test_compute_active_days_empty_returns_zero() {
        assert_eq!(compute_active_days(&[]), 0);
    }

    #[test]
    fn test_find_top_model_picks_highest_total_tokens() {
        let dist = vec![
            make_model("m1", 100, 200),
            make_model("m2", 600, 300),
            make_model("m3", 200, 200),
        ];
        assert_eq!(find_top_model(&dist), (Some("m2".to_string()), Some(900)));
    }

    #[test]
    fn test_find_top_model_empty_distribution_returns_none() {
        assert_eq!(find_top_model(&[]), (None, None));
    }

    #[test]
    fn test_compute_dashboard_derived_combines_all_metrics() {
        let daily = vec![
            DailyExecution { date: "2026-06-01".into(), success: 1, failed: 0 },
            DailyExecution { date: "2026-06-02".into(), success: 0, failed: 0 },
            DailyExecution { date: "2026-06-03".into(), success: 5, failed: 5 },
        ];
        let raw = make_raw(1000, 4, daily, vec![make_model("m", 500, 500)]);
        let derived = compute_dashboard_derived(&raw);
        assert_eq!(derived.avg_duration_ms, 250);
        assert_eq!(derived.active_days, 3);
        assert_eq!(derived.peak_daily_executions, 10);
        assert_eq!(derived.top_model, Some("m".to_string()));
        assert_eq!(derived.top_model_tokens, Some(1000));
        assert_eq!(derived.leaderboard.len(), 1);
    }

    #[test]
    fn test_assemble_dashboard_response_maps_all_fields() {
        let daily = vec![DailyExecution { date: "2026-06-01".into(), success: 1, failed: 0 }];
        let raw = make_raw(500, 2, daily, vec![make_model("a", 100, 100)]);
        let derived = compute_dashboard_derived(&raw);
        let resp = assemble_dashboard_response(raw, derived, None, None);
        assert_eq!(resp.avg_duration_ms, 250);
        assert_eq!(resp.active_days, 1);
        assert_eq!(resp.peak_daily_executions, 1);
        assert_eq!(resp.top_model, Some("a".to_string()));
        assert_eq!(resp.top_model_tokens, Some(200));
        assert!(resp.skills_stats.is_none());
        assert!(resp.backup_stats.is_none());
    }
}

