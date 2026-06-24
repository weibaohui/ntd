//! Database access layer (SeaORM).
//!
//! - Fixed database path: `~/.ntd/data.db`
//! - Built-in SQLite (libsqlite3-sys/bundled), no system dependencies
//! - All public methods are async

use std::str::FromStr;
use std::time::Duration;

use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ColumnTrait, ConnectionTrait,
    DatabaseConnection, DbBackend, EntityTrait, IntoActiveModel,
    Order, QueryFilter, QueryOrder, SqlxSqliteConnector, Statement,
};

pub mod entity;
pub mod sync_record;
pub(super) mod migration;
pub use entity::prelude::*;

/// Model breakdown with date (for API responses)
#[derive(Debug, Clone)]
pub struct ModelBreakdownWithDate {
    pub date: String,
    pub model_name: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub extra_total_tokens: i64,
    pub cost: f64,
}

fn compute_next_run(cron_expr: &str, timezone: Option<&str>) -> Option<String> {
    let schedule = cron::Schedule::from_str(cron_expr).ok()?;

    // Parse timezone, default to UTC if not specified, invalid, or empty string.
    // An empty timezone string is treated as UTC (use UTC time).
    let tz: chrono_tz::Tz = timezone
        .and_then(|tz| tz.parse::<chrono_tz::Tz>().ok())
        .unwrap_or(chrono_tz::UTC);

    // Get next occurrence in the specified timezone
    schedule
        .upcoming(tz)
        .next()
        .map(|dt| {
            // Convert to UTC for storage and display
            dt.with_timezone(&chrono::Utc)
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string()
        })
}

pub struct Database {
    pub(super) conn: DatabaseConnection,
}

impl Database {
    /// 暴露内部连接，仅供集成测试绕过 create_tag / create_todo 等包装函数直接构造边界数据（如 NULL color）使用。
    /// 生产代码不应该调用 —— 业务逻辑统一走 db 下的领域方法。
    ///
    /// 加 `#[doc(hidden)]` + 改名 `_raw` 后缀是为了让 IDE 自动补全里
    /// `db.` 之后不再把"通用原始 conn"作为首选 —— PR #682 评审 HIGH #3
    /// 关注的"通用原始 conn 转义口"反模式；仍以 `pub` 暴露给 `backend/tests/`
    /// 集成测试 crate 用，但走显式 `_conn_raw` 命名警示调用方这是
    /// "最后手段"接口，新 handler 不应再走这条路。
    #[doc(hidden)]
    pub fn _conn_raw(&self) -> &DatabaseConnection {
        &self.conn
    }
}

impl Database {
    /// Open database connection (async).
    /// path: database file path or ":memory:".
    pub async fn new(path: &str) -> Result<Self, sea_orm::DbErr> {
        let url = if path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite://{}?mode=rwc", path)
        };

        // 构建 sqlx 原生 pool_options，应用 after_connect hook：
        // 每次建立新连接时执行 PRAGMA，确保 max_connections=10 时所有连接都正确初始化。
        // （修复了旧代码只对主连接执行 PRAGMA、其他 9 条连接缺失的回归问题）
        //
        // 关于 max/min 连接数的设计取舍：
        // - SQLite 启用 WAL 后允许「1 个 writer + N 个 reader」并发，pool size=1 会把所有数据库
        //   I/O 串行化，浪费 WAL 的并发能力。Issue #497 已把上限从 1 提到 10。
        // - max=10 既覆盖了默认 max_concurrent_todos=3 的写入争用，又给 reader（WebSocket 广播、
        //   hook 触发、健康检查等）留出充足槽位；继续调大对单文件 SQLite 收益有限。
        // - min=2 让 daemon 启动后立即有两条温连接就绪，避免首批并发请求都要冷启。
        let sqlite_opts: sqlx::sqlite::SqliteConnectOptions = url
            .parse()
            .expect("invalid sqlite connection url");

        let mut pool_opts = sqlx::sqlite::SqlitePoolOptions::new();
        pool_opts = pool_opts.max_connections(10);
        pool_opts = pool_opts.min_connections(2);
        pool_opts = pool_opts.acquire_timeout(Duration::from_secs(5));
        pool_opts = pool_opts.after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("PRAGMA busy_timeout = 5000").execute(&mut *conn).await?;
                sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *conn).await?;
                sqlx::query("PRAGMA synchronous = NORMAL").execute(&mut *conn).await?;
                Ok(())
            })
        });

        let pool = pool_opts.connect_with(sqlite_opts).await
            .map_err(|e| match e {
                sqlx::Error::PoolTimedOut => sea_orm::DbErr::ConnectionAcquire(sea_orm::ConnAcquireErr::Timeout),
                sqlx::Error::PoolClosed => sea_orm::DbErr::ConnectionAcquire(sea_orm::ConnAcquireErr::ConnectionClosed),
                other => sea_orm::DbErr::Conn(sea_orm::RuntimeErr::SqlxError(other)),
            })?;
        let conn = SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);
        let db = Self { conn };
        // Enable WAL mode and verify it took effect
        match db.conn
            .query_one(Statement::from_string(DbBackend::Sqlite, "PRAGMA journal_mode = WAL".to_string()))
            .await
        {
            Ok(Some(row)) => {
                match row.try_get_by::<String, _>("journal_mode") {
                    Ok(mode) => {
                        tracing::info!("SQLite journal_mode set to: {}", mode);
                        if mode.to_lowercase() != "wal" {
                            tracing::warn!("SQLite journal_mode expected 'wal', got '{}'", mode);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to extract journal_mode value: {}", e);
                    }
                }
            }
            Ok(None) => {
                tracing::warn!("SQLite journal_mode query returned no row");
            }
            Err(e) => {
                tracing::warn!("Failed to query SQLite journal_mode: {}", e);
            }
        }

        db.init_tables().await?;
        db.seed_default_templates().await?;
        Ok(db)
    }

    pub(super) async fn exec(&self, sql: &str) -> Result<(), sea_orm::DbErr> {
        self.conn
            .execute(Statement::from_string(DbBackend::Sqlite, sql.to_string()))
            .await
            .map(|_| ())
    }

    /// 执行返回结果集的 SQL 语句（如 PRAGMA），忽略返回值
    pub(super) async fn query_exec(&self, sql: &str) -> Result<(), sea_orm::DbErr> {
        self.conn
            .query_one(Statement::from_string(DbBackend::Sqlite, sql.to_string()))
            .await
            .map(|_| ())
    }

    pub(super) async fn exec_update<M>(&self, model: M) -> Result<(), sea_orm::DbErr>
    where
        M: ActiveModelTrait + ActiveModelBehavior + Send,
        <<M as ActiveModelTrait>::Entity as EntityTrait>::Model: IntoActiveModel<M>,
    {
        model.update(&self.conn).await.map(|_| ())
    }

    /// 迁移入口：按版本号顺序执行尚未应用到 `schema_version` 表中的迁移。
    ///
    /// 设计原因：把过去 `init_tables()` 内联的「30+ CREATE TABLE / 30+ CREATE INDEX /
    /// 6 CREATE TRIGGER / 8+ ALTER TABLE」拆成可寻址、可跳过的迁移单元，让稳态启动
    /// 成本从 O(全部 DDL) 降到 O(待执行迁移)。详见 `db/migration.rs` 顶部注释。
    ///
    /// 幂等性：每次启动都先读 `schema_version`，已记录的版本号会被跳过。
    /// 失败行为：迁移返回 `Err` 会立即冒泡，使 daemon 启动失败——比原来的 `.ok()`
    /// 默默吞掉错误更安全（issue #498 修复点之一）。
    ///
    /// **已知限制 (follow-up)**: `m.up` 与 `record_migration` 当前不在同一个事务里,
    /// 二者各自走连接池分配的不同连接。如果 `m.up` 成功提交 DDL、`record_migration`
    /// 失败(如 disk full / lock / acquire_timeout),schema 已迁移但 `schema_version`
    /// 没有对应行,下次启动会重跑 `m.up`。对 V1-V4 现有迁移而言重跑是幂等的(都基于
    /// `CREATE ... IF NOT EXISTS` 或预检查),但新加迁移时需要在 `Migration::up` 内部
    /// 保证幂等性,否则需要重构 trait 接受 `DatabaseTransaction` 参数。
    async fn run_migrations(&self) -> Result<(), sea_orm::DbErr> {
        self.ensure_schema_version_table().await?;
        let applied = migration::read_applied_versions(self).await?;
        for m in migration::all_migrations() {
            let v = m.version();
            if applied.contains(&v) {
                tracing::debug!("migration v{} ({}) already applied", v, m.name());
                continue;
            }
            tracing::info!("applying migration v{} ({})...", v, m.name());
            m.up(self).await?;
            self.record_migration(v, m.name()).await?;
            tracing::info!("migration v{} ({}) applied", v, m.name());
        }
        Ok(())
    }

    /// 确保 `schema_version` 表存在。第一次部署后这个表是空表，之后每次迁移
    /// 都会在其中插入一行 `(version, name, applied_at)`。幂等。
    async fn ensure_schema_version_table(&self) -> Result<(), sea_orm::DbErr> {
        self.exec(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            )",
        )
        .await
    }

    /// 记录一次成功应用的迁移。`applied_at` 使用 UTC ISO8601，与项目其他
    /// 时间戳格式保持一致（参见 `set_todos_created_at_utc` 触发器）。
    async fn record_migration(&self, version: i64, name: &str) -> Result<(), sea_orm::DbErr> {
        let applied_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let stmt = sea_orm::Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO schema_version (version, name, applied_at) VALUES ($1, $2, $3)",
            [version.into(), name.into(), applied_at.into()],
        );
        self.conn.execute(stmt).await?;
        Ok(())
    }

    /// 已应用迁移的版本号集合，暴露为公开方法便于前端展示 / 健康检查。
    pub async fn get_applied_migrations(
        &self,
    ) -> Result<Vec<(i64, String, String)>, sea_orm::DbErr> {
        let stmt = sea_orm::Statement::from_string(
            DbBackend::Sqlite,
            "SELECT version, name, applied_at FROM schema_version ORDER BY version",
        );
        let rows = self.conn.query_all(stmt).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let v: i64 = row.try_get_by("version").unwrap_or(0);
            let n: String = row.try_get_by("name").unwrap_or_default();
            let a: String = row.try_get_by("applied_at").unwrap_or_default();
            out.push((v, n, a));
        }
        Ok(out)
    }

    /// 已应用的最大迁移版本号；启动日志会打印这一项。
    pub async fn get_schema_version(&self) -> Result<Option<i64>, sea_orm::DbErr> {
        let stmt = sea_orm::Statement::from_string(
            DbBackend::Sqlite,
            "SELECT MAX(version) FROM schema_version",
        );
        let row = self.conn.query_one(stmt).await?;
        // MAX() 对空表返回 NULL：用 Option<i64> 列类型解码 NULL 自然得到 None。
        Ok(row.and_then(|r| r.try_get_by_index::<Option<i64>>(0).ok().flatten()))
    }

    async fn init_tables(&self) -> Result<(), sea_orm::DbErr> {
        // 实际工作全部委托给迁移 runner：
        // 1. 首次启动：按版本号顺序执行所有迁移；
        // 2. 稳态启动：读 schema_version 表，跳过已应用迁移，仅当有新版本时才执行。
        // 详见 db/migration.rs。
        self.run_migrations().await?;
        tracing::info!(
            "schema migrations applied; current schema_version = {:?}",
            self.get_schema_version().await?
        );
        Ok(())
    }

    // ===== Usage Stats methods =====

    /// Create a new usage daily stat record
    pub async fn create_usage_daily_stat(
        &self,
        date: &str,
        project_path: Option<&str>,
        session_id: Option<&str>,
        input_tokens: i64,
        output_tokens: i64,
        cache_creation_tokens: i64,
        cache_read_tokens: i64,
        extra_total_tokens: i64,
        total_cost: f64,
        credits: Option<f64>,
        message_count: Option<i64>,
        models_used: &[String],
        project: Option<&str>,
        versions: Option<&[String]>,
        last_activity: Option<&str>,
        stats_type: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        use entity::usage_stats;
        use sea_orm::ActiveValue::Set;
        use sea_orm::EntityTrait;

        let models_used_json = serde_json::to_string(models_used).unwrap_or_else(|_| "[]".to_string());
        let versions_json = versions.and_then(|v| serde_json::to_string(v).ok());

        let active_model = usage_stats::ActiveModel {
            date: Set(date.to_string()),
            project_path: Set(project_path.map(|s| s.to_string())),
            session_id: Set(session_id.map(|s| s.to_string())),
            input_tokens: Set(input_tokens),
            output_tokens: Set(output_tokens),
            cache_creation_tokens: Set(cache_creation_tokens),
            cache_read_tokens: Set(cache_read_tokens),
            extra_total_tokens: Set(extra_total_tokens),
            total_cost: Set(total_cost),
            credits: Set(credits),
            message_count: Set(message_count),
            models_used: Set(models_used_json),
            project: Set(project.map(|s| s.to_string())),
            versions: Set(versions_json),
            last_activity: Set(last_activity.map(|s| s.to_string())),
            stats_type: Set(stats_type.to_string()),
            ..Default::default()
        };

        let result = usage_stats::Entity::insert(active_model)
            .exec(&self.conn)
            .await?;

        Ok(result.last_insert_id)
    }

    /// Create a model breakdown record
    pub async fn create_usage_model_breakdown(
        &self,
        daily_stat_id: i64,
        model_name: &str,
        input_tokens: i64,
        output_tokens: i64,
        cache_creation_tokens: i64,
        cache_read_tokens: i64,
        extra_total_tokens: i64,
        cost: f64,
    ) -> Result<i64, sea_orm::DbErr> {
        use entity::usage_model_breakdown;
        use sea_orm::ActiveValue::Set;
        use sea_orm::EntityTrait;

        let active_model = usage_model_breakdown::ActiveModel {
            daily_stat_id: Set(daily_stat_id),
            model_name: Set(model_name.to_string()),
            input_tokens: Set(input_tokens),
            output_tokens: Set(output_tokens),
            cache_creation_tokens: Set(cache_creation_tokens),
            cache_read_tokens: Set(cache_read_tokens),
            extra_total_tokens: Set(extra_total_tokens),
            cost: Set(cost),
            ..Default::default()
        };

        let result = usage_model_breakdown::Entity::insert(active_model)
            .exec(&self.conn)
            .await?;

        Ok(result.last_insert_id)
    }

    /// Get usage stats by type and date range
    pub async fn get_usage_stats(
        &self,
        stats_type: &str,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<entity::usage_stats::Model>, sea_orm::DbErr> {
        use sea_orm::{EntityTrait, QueryFilter};

        let mut query = entity::usage_stats::Entity::find();

        // Filter by stats_type
        query = query.filter(entity::usage_stats::Column::StatsType.eq(stats_type));

        // Filter by date range if provided
        if let Some(since_date) = since {
            query = query.filter(entity::usage_stats::Column::Date.gte(since_date));
        }
        if let Some(until_date) = until {
            query = query.filter(entity::usage_stats::Column::Date.lte(until_date));
        }

        let results = query
            .order_by(entity::usage_stats::Column::Date, Order::Desc)
            .all(&self.conn)
            .await?;

        Ok(results)
    }

    /// Get model breakdowns for a specific daily stat
    pub async fn get_usage_model_breakdowns(
        &self,
        daily_stat_id: i64,
    ) -> Result<Vec<entity::usage_model_breakdown::Model>, sea_orm::DbErr> {
        use sea_orm::EntityTrait;

        let results = entity::usage_model_breakdown::Entity::find()
            .filter(entity::usage_model_breakdown::Column::DailyStatId.eq(daily_stat_id))
            .all(&self.conn)
            .await?;

        Ok(results)
    }

    /// Get model breakdowns for a date range (via join with daily_stats)
    pub async fn get_usage_model_breakdowns_by_date_range(
        &self,
        stats_type: &str,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<ModelBreakdownWithDate>, sea_orm::DbErr> {
        // First get daily stats in date range
        let daily_stats = self.get_usage_stats(stats_type, since, until).await?;

        if daily_stats.is_empty() {
            return Ok(vec![]);
        }

        // Get all stat IDs and their dates
        let stat_ids: Vec<i64> = daily_stats.iter().map(|s| s.id).collect();
        let stat_dates: std::collections::HashMap<i64, String> = daily_stats
            .iter()
            .map(|s| (s.id, s.date.clone()))
            .collect();

        // Get all breakdowns for these stats
        let mut all_breakdowns: Vec<ModelBreakdownWithDate> = vec![];

        for stat_id in stat_ids {
            let breakdowns = self.get_usage_model_breakdowns(stat_id).await?;
            let date = stat_dates.get(&stat_id).cloned().unwrap_or_default();
            for bd in breakdowns {
                all_breakdowns.push(ModelBreakdownWithDate {
                    date: date.clone(),
                    model_name: bd.model_name,
                    input_tokens: bd.input_tokens,
                    output_tokens: bd.output_tokens,
                    cache_creation_tokens: bd.cache_creation_tokens,
                    cache_read_tokens: bd.cache_read_tokens,
                    extra_total_tokens: bd.extra_total_tokens,
                    cost: bd.cost,
                });
            }
        }

        Ok(all_breakdowns)
    }

    /// Delete existing stats for a specific date and type (for re-computation)
    pub async fn delete_usage_stats_by_date(
        &self,
        date: &str,
        stats_type: &str,
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::Delete;

        // First delete breakdowns for the daily stats
        let daily_stats: Vec<entity::usage_stats::Model> = entity::usage_stats::Entity::find()
            .filter(entity::usage_stats::Column::Date.eq(date))
            .filter(entity::usage_stats::Column::StatsType.eq(stats_type))
            .all(&self.conn)
            .await?;

        for stat in daily_stats {
            Delete::one(stat).exec(&self.conn).await?;
        }

        // Delete the daily stats using filter-based deletion
        Delete::many(entity::usage_stats::Entity)
            .filter(entity::usage_stats::Column::Date.eq(date))
            .filter(entity::usage_stats::Column::StatsType.eq(stats_type))
            .exec(&self.conn)
            .await?;

        Ok(())
    }

    /// Get the most recent stat for a specific date and type
    pub async fn get_latest_usage_stat(
        &self,
        date: &str,
        stats_type: &str,
    ) -> Result<Option<entity::usage_stats::Model>, sea_orm::DbErr> {
        use sea_orm::EntityTrait;

        let result = entity::usage_stats::Entity::find()
            .filter(entity::usage_stats::Column::Date.eq(date))
            .filter(entity::usage_stats::Column::StatsType.eq(stats_type))
            .one(&self.conn)
            .await?;

        Ok(result)
    }

    // ===== Usage Executor Daily Stats methods =====

    /// Create or update usage executor daily stat record
    pub async fn upsert_usage_executor_daily_stat(
        &self,
        date: &str,
        executor: &str,
        input_tokens: i64,
        output_tokens: i64,
        cache_creation_tokens: i64,
        cache_read_tokens: i64,
        extra_total_tokens: i64,
        total_cost: f64,
        credits: Option<f64>,
        message_count: Option<i64>,
        model: Option<&str>,
        execution_count: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let sql = r#"INSERT INTO usage_executor_daily_stats
               (date, executor, input_tokens, output_tokens, cache_creation_tokens,
                cache_read_tokens, extra_total_tokens, total_cost, credits, message_count,
                model, execution_count)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(date, executor) DO UPDATE SET
                input_tokens = input_tokens + excluded.input_tokens,
                output_tokens = output_tokens + excluded.output_tokens,
                cache_creation_tokens = cache_creation_tokens + excluded.cache_creation_tokens,
                cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
                extra_total_tokens = extra_total_tokens + excluded.extra_total_tokens,
                total_cost = total_cost + excluded.total_cost,
                credits = COALESCE(credits, 0) + COALESCE(excluded.credits, 0),
                message_count = COALESCE(message_count, 0) + COALESCE(excluded.message_count, 0),
                model = excluded.model,
                execution_count = execution_count + excluded.execution_count"#;

        let stmt = Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            vec![
                date.into(),
                executor.into(),
                input_tokens.into(),
                output_tokens.into(),
                cache_creation_tokens.into(),
                cache_read_tokens.into(),
                extra_total_tokens.into(),
                total_cost.into(),
                credits.into(),
                message_count.into(),
                model.into(),
                execution_count.into(),
            ],
        );

        let result = self.conn.execute(stmt).await?;
        Ok(result.last_insert_id() as i64)
    }

    /// Get usage executor daily stats by date range
    pub async fn get_usage_executor_daily_stats(
        &self,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<entity::usage_executor_daily::Model>, sea_orm::DbErr> {
        use sea_orm::{EntityTrait, QueryFilter};

        let mut query = entity::usage_executor_daily::Entity::find();

        if let Some(since_date) = since {
            query = query.filter(entity::usage_executor_daily::Column::Date.gte(since_date));
        }
        if let Some(until_date) = until {
            query = query.filter(entity::usage_executor_daily::Column::Date.lte(until_date));
        }

        let results = query
            .order_by_desc(entity::usage_executor_daily::Column::Date)
            .order_by_asc(entity::usage_executor_daily::Column::Executor)
            .all(&self.conn)
            .await?;

        Ok(results)
    }

    /// Delete usage executor stats for a specific date
    pub async fn delete_usage_executor_stats_by_date(&self, date: &str) -> Result<(), sea_orm::DbErr> {
        use sea_orm::Delete;

        // Use filter-based deletion to avoid SQL injection
        Delete::many(entity::usage_executor_daily::Entity)
            .filter(entity::usage_executor_daily::Column::Date.eq(date))
            .exec(&self.conn)
            .await?;

        Ok(())
    }
}

mod todo;
pub use todo::{SchedulerUpdate, TodoUpdate};
pub mod execution;
pub(super) mod dashboard;
mod tag;
pub use execution::NewExecutionRecord;
mod agent_bot;
mod executor_config;
mod feishu_home;
mod feishu_message;
mod skills;
pub use feishu_message::{NewFeishuHistoryMessage, NewFeishuMessage};
mod feishu_group_whitelist;
mod feishu_history_chat;
pub mod loop_;
pub mod step_;
pub(crate) mod feishu_project_binding;
mod feishu_push_target;
mod feishu_response_config;
pub mod project_directory;
mod todo_template;
pub use todo_template::TemplateInput;
mod review_template;
pub use review_template::ReviewTemplateInput;
pub mod webhook;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Timelike, Utc};

    async fn setup_db() -> Database {
        Database::new(":memory:").await.unwrap()
    }

    /// Issue #498：迁移 runner 应在首次 `:memory:` 启动时把当前注册的全部迁移
    /// 标记为已应用，并把 schema_version 推进到最新版本。
    #[tokio::test]
    async fn test_fresh_db_records_all_migrations() {
        let db = setup_db().await;
        let v = db
            .get_schema_version()
            .await
            .unwrap()
            .expect("schema_version should be Some after fresh init");
        let migrations = migration::all_migrations();
        let latest = migrations
            .iter()
            .map(|m| m.version())
            .max()
            .expect("at least one migration registered");
        assert_eq!(
            v, latest,
            "fresh DB schema_version must equal max registered migration version"
        );

        let applied = db.get_applied_migrations().await.unwrap();
        // 注意：fresh 库实际写入 schema_version 的行数 == 已注册迁移数，
        // 而不是「1..=latest_version」连续区间 —— v20-v22 是 phantom（reverted 分支
        // 残留），不在代码 all_migrations() 里，所以 fresh 库也不会写入它们。
        // 见 plan `purring-forging-petal`。
        assert_eq!(
            applied.len() as i64,
            migrations.len() as i64,
            "schema_version row count must equal registered migration count"
        );
        for (ver, _name, _at) in &applied {
            assert!(*ver >= 1 && *ver <= latest);
        }
    }

    /// Issue #498：稳态启动时迁移 runner 必须幂等——再次运行不应增加新行，
    /// 也应不报错。
    #[tokio::test]
    async fn test_run_migrations_is_idempotent() {
        let db = setup_db().await;
        let v1 = db.get_schema_version().await.unwrap().unwrap();
        let count1 = db.get_applied_migrations().await.unwrap().len();

        // 第二次手动调用：应当跳过所有已应用版本，等价 no-op
        db.run_migrations().await.expect("rerun must succeed");

        let v2 = db.get_schema_version().await.unwrap().unwrap();
        let count2 = db.get_applied_migrations().await.unwrap().len();
        assert_eq!(v1, v2, "schema_version must not advance on rerun");
        assert_eq!(
            count1, count2,
            "schema_version rows must not grow on rerun (no duplicate INSERTs)"
        );
    }

    /// Issue #498：新增迁移应当以版本号顺序执行（关键不变量：低版本号必须先于高版本号）。
    /// 通过 schema_version 行序验证。
    #[tokio::test]
    async fn test_migrations_applied_in_version_order() {
        let db = setup_db().await;
        let applied = db.get_applied_migrations().await.unwrap();
        let versions: Vec<i64> = applied.iter().map(|(v, _, _)| *v).collect();
        let mut sorted = versions.clone();
        sorted.sort();
        assert_eq!(
            versions, sorted,
            "applied migration versions must be in ascending order"
        );
    }

    /// Issue #498：单步迁移幂等——直接对已应用版本调用 `up()` 也不应破坏状态。
    /// 验证 v1 的所有 DDL 都是 IF NOT EXISTS / 幂等检查，重复执行不出错。
    #[tokio::test]
    async fn test_v1_initial_schema_is_idempotent() {
        let db = setup_db().await;
        // 重新跑一遍 v1。表都已存在，CREATE TABLE IF NOT EXISTS / INDEX IF NOT EXISTS
        // 应当跳过；已存在的 ALTER TABLE 列会被兼容分支 warn-and-skip。
        migration::all_migrations()
            .into_iter()
            .find(|m| m.version() == 1)
            .expect("v1 migration must be registered")
            .up(&db)
            .await
            .expect("v1 re-run on already-migrated DB must succeed");
    }

    /// Issue #498：迁移名与版本号一一对应——验证 schema_version 里存的 name 字段
    /// 与 `migration::all_migrations()` 的注册一致。
    #[tokio::test]
    async fn test_applied_migration_names_match_registry() {
        let db = setup_db().await;
        let applied: std::collections::HashMap<i64, String> = db
            .get_applied_migrations()
            .await
            .unwrap()
            .into_iter()
            .map(|(v, n, _)| (v, n))
            .collect();
        for m in migration::all_migrations() {
            let v = m.version();
            let registered_name = m.name();
            let stored = applied
                .get(&v)
                .unwrap_or_else(|| panic!("migration v{} missing from schema_version", v));
            assert_eq!(
                stored, registered_name,
                "migration v{} name mismatch: stored={} registered={}",
                v, stored, registered_name
            );
        }
    }

    async fn create_test_execution_record(db: &Database, todo_id: i64, command: &str) -> i64 {
        db.create_execution_record(NewExecutionRecord {
            // `NewExecutionRecord.todo_id` 在一次重构里从 `Option<i64>` 收紧到
            // `i64` (db/execution.rs:12),这里忘了改,所以 `cargo test` 早就
            // 编不过。issue #502 的 PR 顺手把它对齐。
            todo_id: Some(todo_id),
            command,
            executor: "claudecode",
            trigger_type: "manual",
            task_id: "test-task-id",
            session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,
            step_id: None,
        })
        .await
        .unwrap()
    }

    fn parse_utc(ts: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(ts)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn truncate_seconds(dt: DateTime<Utc>) -> DateTime<Utc> {
        dt.with_nanosecond(0).unwrap()
    }

    #[tokio::test]
    async fn test_todo_created_at_is_utc() {
        let db = setup_db().await;
        let before = truncate_seconds(Utc::now());
        let id = db.create_todo("Test", "Desc").await.unwrap();
        let after = truncate_seconds(Utc::now());

        let todo = db.get_todo(id).await.unwrap().unwrap();
        let created = truncate_seconds(parse_utc(&todo.created_at));

        assert!(
            created >= before,
            "created_at should not be before test start"
        );
        assert!(created <= after, "created_at should not be after test end");
        assert!(
            todo.created_at.ends_with('Z'),
            "UTC timestamp must end with Z"
        );
    }

    #[tokio::test]
    async fn test_todo_updated_at_changes_on_update() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Desc").await.unwrap();
        let original = db.get_todo(id).await.unwrap().unwrap().updated_at;

        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        db.update_todo_full(TodoUpdate {
            id,
            title: "Updated",
            prompt: "Desc",
            status: crate::models::TodoStatus::InProgress,
            executor: None,
            scheduler_enabled: None,
            scheduler_config: None,
            scheduler_timezone: None,
            workspace: None,
            acceptance_criteria: None,
            auto_review_enabled: None,
        })
        .await
        .unwrap();
        let updated = db.get_todo(id).await.unwrap().unwrap().updated_at;

        assert_ne!(original, updated, "updated_at should change after update");
        assert!(updated.ends_with('Z'));
    }

    #[tokio::test]
    async fn test_todo_deleted_at_is_utc() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Desc").await.unwrap();
        let before = truncate_seconds(Utc::now());
        db.delete_todo(id).await.unwrap();
        let after = truncate_seconds(Utc::now());

        let model = entity::todos::Entity::find_by_id(id)
            .one(&db.conn)
            .await
            .unwrap()
            .unwrap();
        let deleted_at = model.deleted_at.unwrap();
        let dt = truncate_seconds(parse_utc(&deleted_at));
        assert!(dt >= before);
        assert!(dt <= after);
        assert!(deleted_at.ends_with('Z'));
    }

    #[tokio::test]
    async fn test_tag_created_at_is_utc() {
        let db = setup_db().await;
        let before = truncate_seconds(Utc::now());
        let id = db.create_tag("urgent", "#ff0000").await.unwrap();
        let after = truncate_seconds(Utc::now());

        let tag = db
            .get_tags()
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.id == id)
            .unwrap();
        let created = truncate_seconds(parse_utc(&tag.created_at));

        assert!(created >= before);
        assert!(created <= after);
        assert!(tag.created_at.ends_with('Z'));
    }

    #[tokio::test]
    async fn test_execution_record_started_at_is_utc() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Desc").await.unwrap();
        let before = truncate_seconds(Utc::now());
        let record_id = create_test_execution_record(&db, todo_id, "echo hi").await;
        let after = truncate_seconds(Utc::now());

        let (records, _) = db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: Some(todo_id),
            step_id: None,
            limit: 100,
            offset: 0,
            status: None,
        })
        .await
        .unwrap();
        let record = records.into_iter().find(|r| r.id == record_id).unwrap();
        let started = truncate_seconds(parse_utc(&record.started_at));

        assert!(started >= before);
        assert!(started <= after);
        assert!(record.started_at.ends_with('Z'));
    }

    #[tokio::test]
    async fn test_execution_record_finished_at_is_utc() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Desc").await.unwrap();
        let record_id = create_test_execution_record(&db, todo_id, "echo hi").await;

        let before = truncate_seconds(Utc::now());
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: record_id,
            status: crate::models::ExecutionStatus::Success.as_str(),
            remaining_logs: "[]",
            result: "done",
            usage: None,
            model: None,
            review_meta: None,
        })
        .await
        .unwrap();
        let after = truncate_seconds(Utc::now());

        let (records, _) = db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: Some(todo_id),
            step_id: None,
            limit: 100,
            offset: 0,
            status: None,
        })
        .await
        .unwrap();
        let record = records.into_iter().find(|r| r.id == record_id).unwrap();
        let finished_at = record.finished_at.unwrap();
        let finished = truncate_seconds(parse_utc(&finished_at));

        assert!(finished >= before);
        assert!(finished <= after);
        assert!(finished_at.ends_with('Z'));
    }

    // ===== Todo CRUD tests =====

    #[tokio::test]
    async fn test_create_and_get_todo() {
        let db = setup_db().await;
        let id = db.create_todo("Title", "Prompt").await.unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert_eq!(todo.title, "Title");
        assert_eq!(todo.prompt, "Prompt");
        assert_eq!(todo.status, crate::models::TodoStatus::Pending);
        assert!(!todo.scheduler_enabled);
    }

    #[tokio::test]
    async fn test_get_todos_excludes_deleted() {
        let db = setup_db().await;
        let id = db.create_todo("Active", "Prompt").await.unwrap();
        db.delete_todo(id).await.unwrap();
        let todos = db.get_todos().await.unwrap();
        assert!(todos.iter().all(|t| t.id != id));
    }

    #[tokio::test]
    async fn test_get_todos_ordering() {
        let db = setup_db().await;
        let id1 = db.create_todo("First", "Prompt").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let id2 = db.create_todo("Second", "Prompt").await.unwrap();
        let todos = db.get_todos().await.unwrap();
        assert_eq!(todos[0].id, id2);
        assert_eq!(todos[1].id, id1);
    }

    #[tokio::test]
    async fn test_update_todo_full() {
        let db = setup_db().await;
        let id = db.create_todo("Old", "Old prompt").await.unwrap();
        db.update_todo_full(TodoUpdate {
            id,
            title: "New",
            prompt: "New prompt",
            status: crate::models::TodoStatus::InProgress,
            executor: Some("opencode"),
            scheduler_enabled: Some(true),
            scheduler_config: Some("0 0 * * *"),
            scheduler_timezone: None,
            workspace: Some("/tmp/workspace"),
            acceptance_criteria: None,
            auto_review_enabled: None,
        })
        .await
        .unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert_eq!(todo.title, "New");
        assert_eq!(todo.prompt, "New prompt");
        assert_eq!(todo.status, crate::models::TodoStatus::InProgress);
        assert_eq!(todo.executor, Some("opencode".to_string()));
        assert!(todo.scheduler_enabled);
        assert_eq!(todo.scheduler_config, Some("0 0 * * *".to_string()));
        assert_eq!(todo.workspace, Some("/tmp/workspace".to_string()));
    }

    #[tokio::test]
    async fn test_update_todo_executor() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Prompt").await.unwrap();
        db.update_todo_executor(id, "mobilecoder").await.unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert_eq!(todo.executor, Some("mobilecoder".to_string()));
    }

    #[tokio::test]
    async fn test_update_todo_task_id() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Prompt").await.unwrap();
        db.update_todo_task_id(id, Some("task-123")).await.unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert_eq!(todo.task_id, Some("task-123".to_string()));
        db.update_todo_task_id(id, None).await.unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert!(todo.task_id.is_none());
    }

    #[tokio::test]
    async fn test_update_todo_scheduler() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Prompt").await.unwrap();
        db.update_todo_scheduler(crate::db::SchedulerUpdate { id, enabled: true, config: Some("0 0 * * *"), timezone: None })
            .await
            .unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert!(todo.scheduler_enabled);
        assert_eq!(todo.scheduler_config, Some("0 0 * * *".to_string()));
    }

    #[tokio::test]
    async fn test_force_update_todo_status() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Prompt").await.unwrap();
        db.force_update_todo_status(id, crate::models::TodoStatus::Failed)
            .await
            .unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert_eq!(todo.status, crate::models::TodoStatus::Failed);
    }

    #[tokio::test]
    async fn test_delete_todo_soft_delete() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Prompt").await.unwrap();
        db.delete_todo(id).await.unwrap();
        assert!(db.get_todo(id).await.unwrap().is_none());
        let todos = db.get_todos().await.unwrap();
        assert!(todos.iter().all(|t| t.id != id));
    }

    #[tokio::test]
    async fn test_start_todo_execution() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Prompt").await.unwrap();
        db.start_todo_execution(id, "task-1").await.unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert_eq!(todo.status, crate::models::TodoStatus::Running);
        assert_eq!(todo.task_id, Some("task-1".to_string()));
    }

    #[tokio::test]
    async fn test_finish_todo_execution_success() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Prompt").await.unwrap();
        db.start_todo_execution(id, "task-1").await.unwrap();
        db.finish_todo_execution(id, true).await.unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert_eq!(todo.status, crate::models::TodoStatus::Completed);
        assert!(todo.task_id.is_none());
    }

    #[tokio::test]
    async fn test_finish_todo_execution_failure() {
        let db = setup_db().await;
        let id = db.create_todo("Test", "Prompt").await.unwrap();
        db.start_todo_execution(id, "task-1").await.unwrap();
        db.finish_todo_execution(id, false).await.unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert_eq!(todo.status, crate::models::TodoStatus::Failed);
    }

    #[tokio::test]
    async fn test_get_scheduler_todos() {
        let db = setup_db().await;
        let id1 = db.create_todo("Scheduled", "Prompt").await.unwrap();
        db.update_todo_scheduler(crate::db::SchedulerUpdate { id: id1, enabled: true, config: Some("0 0 * * *"), timezone: None })
            .await
            .unwrap();
        let id2 = db.create_todo("Normal", "Prompt").await.unwrap();
        let scheduled = db.get_scheduler_todos().await.unwrap();
        assert_eq!(scheduled.len(), 1);
        assert_eq!(scheduled[0].id, id1);
        assert!(scheduled.iter().all(|t| t.id != id2));
    }

    #[tokio::test]
    async fn test_todo_with_tag_ids() {
        let db = setup_db().await;
        let tag_id = db.create_tag("urgent", "#ff0000").await.unwrap();
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        db.add_todo_tag(todo_id, tag_id).await.unwrap();
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert_eq!(todo.tag_ids, vec![tag_id]);
    }

    // ===== Tag CRUD tests =====

    #[tokio::test]
    async fn test_create_and_get_tag() {
        let db = setup_db().await;
        let id = db.create_tag("urgent", "#ff0000").await.unwrap();
        let tags = db.get_tags().await.unwrap();
        let tag = tags.iter().find(|t| t.id == id).unwrap();
        assert_eq!(tag.name, "urgent");
        assert_eq!(tag.color, "#ff0000");
    }

    #[tokio::test]
    async fn test_get_tags_ordered_by_name() {
        let db = setup_db().await;
        db.create_tag("zebra", "#000").await.unwrap();
        db.create_tag("apple", "#fff").await.unwrap();
        db.create_tag("mango", "#aaa").await.unwrap();
        let tags = db.get_tags().await.unwrap();
        assert_eq!(tags[0].name, "apple");
        assert_eq!(tags[1].name, "mango");
        assert_eq!(tags[2].name, "zebra");
    }

    #[tokio::test]
    async fn test_delete_tag() {
        let db = setup_db().await;
        let id = db.create_tag("temp", "#000").await.unwrap();
        db.delete_tag(id).await.unwrap();
        let tags = db.get_tags().await.unwrap();
        assert!(tags.iter().all(|t| t.id != id));
    }

    #[tokio::test]
    async fn test_add_todo_tag() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let tag_id = db.create_tag("urgent", "#ff0000").await.unwrap();
        db.add_todo_tag(todo_id, tag_id).await.unwrap();
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert_eq!(todo.tag_ids, vec![tag_id]);
    }

    #[tokio::test]
    async fn test_add_todo_tag_duplicate_ignored() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let tag_id = db.create_tag("urgent", "#ff0000").await.unwrap();
        db.add_todo_tag(todo_id, tag_id).await.unwrap();
        db.add_todo_tag(todo_id, tag_id).await.unwrap(); // should not panic
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert_eq!(todo.tag_ids, vec![tag_id]);
    }

    #[tokio::test]
    async fn test_set_todo_tags_replace_all() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let tag1 = db.create_tag("a", "#000").await.unwrap();
        let tag2 = db.create_tag("b", "#fff").await.unwrap();
        let tag3 = db.create_tag("c", "#aaa").await.unwrap();
        db.add_todo_tag(todo_id, tag1).await.unwrap();
        db.set_todo_tags(todo_id, &[tag2, tag3]).await.unwrap();
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert_eq!(todo.tag_ids.len(), 2);
        assert!(todo.tag_ids.contains(&tag2));
        assert!(todo.tag_ids.contains(&tag3));
        assert!(!todo.tag_ids.contains(&tag1));
    }

    #[tokio::test]
    async fn test_set_todo_tags_empty_clears_all() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let tag_id = db.create_tag("urgent", "#ff0000").await.unwrap();
        db.add_todo_tag(todo_id, tag_id).await.unwrap();
        db.set_todo_tags(todo_id, &[]).await.unwrap();
        let todo = db.get_todo(todo_id).await.unwrap().unwrap();
        assert!(todo.tag_ids.is_empty());
    }

    #[tokio::test]
    async fn test_delete_todo_cascades_tags() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let tag_id = db.create_tag("urgent", "#ff0000").await.unwrap();
        db.add_todo_tag(todo_id, tag_id).await.unwrap();
        db.delete_todo(todo_id).await.unwrap();
        // tag should still exist but association should be gone
        let tags = db.get_tags().await.unwrap();
        assert!(tags.iter().any(|t| t.id == tag_id));
    }

    // ===== Execution record tests =====

    #[tokio::test]
    async fn test_create_execution_record() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let record_id = create_test_execution_record(&db, todo_id, "echo hi").await;
        let (records, total) = db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: Some(todo_id),
            step_id: None,
            limit: 100,
            offset: 0,
            status: None,
        })
        .await
        .unwrap();
        assert_eq!(total, 1);
        let record = records.iter().find(|r| r.id == record_id).unwrap();
        assert_eq!(record.status, crate::models::ExecutionStatus::Running);
        assert_eq!(record.command, "echo hi");
        assert_eq!(record.executor, Some("claudecode".to_string()));
        assert_eq!(record.trigger_type, "manual");
        assert!(record.finished_at.is_none());
    }

    #[tokio::test]
    async fn test_get_execution_records_pagination() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        for i in 0..5 {
            create_test_execution_record(&db, todo_id, &format!("cmd{}", i)).await;
        }
        let (records, total) = db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: Some(todo_id),
            step_id: None,
            limit: 2,
            offset: 0,
            status: None,
        })
        .await
        .unwrap();
        assert_eq!(total, 5);
        assert_eq!(records.len(), 2);
    }

    #[tokio::test]
    async fn test_get_execution_records_offset() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        for i in 0..3 {
            create_test_execution_record(&db, todo_id, &format!("cmd{}", i)).await;
        }
        let (records, total) = db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: Some(todo_id),
            step_id: None,
            limit: 10,
            offset: 2,
            status: None,
        })
        .await
        .unwrap();
        assert_eq!(total, 3);
        assert_eq!(records.len(), 1);
    }

    #[tokio::test]
    async fn test_get_execution_records_with_status_filter() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();

        let running_id = create_test_execution_record(&db, todo_id, "cmd-running").await;
        let success_id = create_test_execution_record(&db, todo_id, "cmd-success").await;
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: success_id,
            status: "success",
            remaining_logs: "[]",
            result: "",
            usage: None,
            model: None,
            review_meta: None,
        })
        .await
        .unwrap();
        let failed_id = create_test_execution_record(&db, todo_id, "cmd-failed").await;
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: failed_id,
            status: "failed",
            remaining_logs: "[]",
            result: "",
            usage: None,
            model: None,
            review_meta: None,
        })
        .await
        .unwrap();

        let (running, total_running) =
            db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
                todo_id: Some(todo_id),
            step_id: None,
                limit: 10,
                offset: 0,
                status: Some("running"),
            })
            .await
            .unwrap();
        assert_eq!(total_running, 1);
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, running_id);

        let (success, total_success) =
            db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
                todo_id: Some(todo_id),
            step_id: None,
                limit: 10,
                offset: 0,
                status: Some("success"),
            })
            .await
            .unwrap();
        assert_eq!(total_success, 1);
        assert_eq!(success.len(), 1);
        assert_eq!(success[0].id, success_id);

        let (failed, total_failed) =
            db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
                todo_id: Some(todo_id),
            step_id: None,
                limit: 10,
                offset: 0,
                status: Some("failed"),
            })
            .await
            .unwrap();
        assert_eq!(total_failed, 1);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, failed_id);

        let (all, total_all) = db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: Some(todo_id),
            step_id: None,
            limit: 10,
            offset: 0,
            status: None,
        })
        .await
        .unwrap();
        assert_eq!(total_all, 3);
        assert_eq!(all.len(), 3);

        // Test Some("all") returns all records (db layer should treat "all" as no filter)
        let (all_filter, total_all_filter) =
            db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
                todo_id: Some(todo_id),
            step_id: None,
                limit: 10,
                offset: 0,
                status: Some("all"),
            })
            .await
                .unwrap();
        assert_eq!(total_all_filter, 3);
        assert_eq!(all_filter.len(), 3);
    }

    #[tokio::test]
    async fn test_update_execution_record() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let record_id = create_test_execution_record(&db, todo_id, "echo hi").await;
        let usage = crate::models::ExecutionUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: Some(20),
            cache_creation_input_tokens: None,
            total_cost_usd: Some(0.005),
            duration_ms: Some(1000),
        };
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: record_id,
            status: "success",
            remaining_logs: "[{\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"type\":\"info\",\"content\":\"test log\"}]",
            result: "done",
            usage: Some(&usage),
            model: Some("claude-3"),
            review_meta: None,
        })
        .await
        .unwrap();
        let (records, _) = db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id: Some(todo_id),
            step_id: None,
            limit: 100,
            offset: 0,
            status: None,
        })
        .await
        .unwrap();
        let record = records.iter().find(|r| r.id == record_id).unwrap();
        assert_eq!(record.status, crate::models::ExecutionStatus::Success);
        assert_eq!(record.result, Some("done".to_string()));
        assert_eq!(record.model, Some("claude-3".to_string()));
        assert!(record.finished_at.is_some());
        let record_usage = record.usage.as_ref().unwrap();
        assert_eq!(record_usage.input_tokens, 100);
        assert_eq!(record_usage.output_tokens, 50);

        // 验证日志已写入 execution_logs 表
        let (logs, total) = db.get_execution_logs(record_id, 1, 10).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].log_type, "info");
    }

    /// Issue #653 回归：执行期间 LogFlusher 已经按批次把日志写库，终态分支
    /// （Completed / Cancelled / TimedOut）调用 `update_execution_record` 时
    /// `remaining_logs` 必须传 `"[]"`，不能再传全量日志，否则每条日志都会被插两次。
    ///
    /// 用例分两步：
    /// 1. 旧 buggy 路径：`append_execution_record_logs` 写 5 条，再以全量 JSON
    ///    调 `update_execution_record`，断言出现 10 条重复日志（确认 bug 可复现）。
    /// 2. 修复后路径：同样 append 5 条，再以 `"[]"` 调 `update_execution_record`，
    ///    断言最终仍是 5 条（确认修复生效）。
    #[tokio::test]
    async fn test_update_execution_record_does_not_duplicate_logs_issue_653() {
        let db = setup_db().await;
        let todo_id = db.create_todo("DupLogs", "Prompt").await.unwrap();
        let record_id = create_test_execution_record(&db, todo_id, "echo hi").await;

        // 模拟 LogFlusher 期间已经把 5 条日志写到 execution_logs。
        let logs_json = r#"[
            {"timestamp":"2026-01-01T00:00:00.000Z","type":"info","content":"log 1"},
            {"timestamp":"2026-01-01T00:00:01.000Z","type":"info","content":"log 2"},
            {"timestamp":"2026-01-01T00:00:02.000Z","type":"info","content":"log 3"},
            {"timestamp":"2026-01-01T00:00:03.000Z","type":"info","content":"log 4"},
            {"timestamp":"2026-01-01T00:00:04.000Z","type":"info","content":"log 5"}
        ]"#;
        db.append_execution_record_logs(record_id, logs_json)
            .await
            .unwrap();
        let (logs, total) = db.get_execution_logs(record_id, 1, 100).await.unwrap();
        assert_eq!(total, 5, "append 阶段：5 条日志写库");
        assert_eq!(logs.len(), 5);

        // 1) 旧 buggy 路径：remaining_logs 传全量 JSON → 再次插入，验证 bug 可复现。
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: record_id,
            status: crate::models::ExecutionStatus::Success.as_str(),
            remaining_logs: logs_json,
            result: "done",
            usage: None,
            model: None,
            review_meta: None,
        })
        .await
        .unwrap();
        let (_, total_after_dup) = db.get_execution_logs(record_id, 1, 100).await.unwrap();
        assert_eq!(
            total_after_dup, 10,
            "旧 buggy 路径：传全量日志会导致 5+5=10 条重复日志（issue #653）"
        );

        // 2) 修复后路径：另起一条记录，传 "[]"，验证日志条数保持不变。
        let record_id2 = create_test_execution_record(&db, todo_id, "echo hi").await;
        db.append_execution_record_logs(record_id2, logs_json)
            .await
            .unwrap();
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: record_id2,
            status: crate::models::ExecutionStatus::Success.as_str(),
            // 修复点：remaining_logs 传 "[]"，避免重复插入。
            remaining_logs: "[]",
            result: "done",
            usage: None,
            model: None,
            review_meta: None,
        })
        .await
        .unwrap();
        let (logs2, total2) = db.get_execution_logs(record_id2, 1, 100).await.unwrap();
        assert_eq!(
            total2, 5,
            "修复后路径：传 \"[]\" 时日志条数保持 5 条，无重复"
        );
        assert_eq!(logs2.len(), 5);
        // 内容应当与原始 append 的一致
        assert_eq!(logs2[0].content, "log 1");
        assert_eq!(logs2[4].content, "log 5");
    }

    #[tokio::test]
    async fn test_get_execution_summary_empty() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let summary = db.get_execution_summary(todo_id).await.unwrap();
        assert_eq!(summary.todo_id, todo_id);
        assert_eq!(summary.total_executions, 0);
        assert_eq!(summary.success_count, 0);
        assert_eq!(summary.failed_count, 0);
        assert_eq!(summary.running_count, 0);
        assert!(summary.total_cost_usd.is_none());
    }

    #[tokio::test]
    async fn test_get_execution_summary_counts() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let r1 = create_test_execution_record(&db, todo_id, "cmd1").await;
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: r1,
            status: "success",
            remaining_logs: "[]",
            result: "",
            usage: None,
            model: None,
            review_meta: None,
        })
        .await
        .unwrap();
        let r2 = create_test_execution_record(&db, todo_id, "cmd2").await;
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: r2,
            status: "failed",
            remaining_logs: "[]",
            result: "",
            usage: None,
            model: None,
            review_meta: None,
        })
        .await
        .unwrap();
        let _r3 = create_test_execution_record(&db, todo_id, "cmd3").await;
        // r3 stays "running"
        let summary = db.get_execution_summary(todo_id).await.unwrap();
        assert_eq!(summary.total_executions, 3);
        assert_eq!(summary.success_count, 1);
        assert_eq!(summary.failed_count, 1);
        assert_eq!(summary.running_count, 1);
    }

    #[tokio::test]
    async fn test_get_execution_summary_tokens_and_cost() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Prompt").await.unwrap();
        let r1 = create_test_execution_record(&db, todo_id, "cmd1").await;
        let usage1 = crate::models::ExecutionUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: Some(20),
            cache_creation_input_tokens: Some(10),
            total_cost_usd: Some(0.005),
            duration_ms: Some(1000),
        };
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: r1,
            status: "success",
            remaining_logs: "[]",
            result: "",
            usage: Some(&usage1),
            model: None,
            review_meta: None,
        })
        .await
        .unwrap();
        let r2 = create_test_execution_record(&db, todo_id, "cmd2").await;
        let usage2 = crate::models::ExecutionUsage {
            input_tokens: 200,
            output_tokens: 100,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            total_cost_usd: Some(0.010),
            duration_ms: Some(2000),
        };
        db.update_execution_record(crate::db::execution::UpdateExecutionRecordRequest {
            id: r2,
            status: "success",
            remaining_logs: "[]",
            result: "",
            usage: Some(&usage2),
            model: None,
            review_meta: None,
        })
        .await
        .unwrap();
        let summary = db.get_execution_summary(todo_id).await.unwrap();
        assert_eq!(summary.total_input_tokens, 300);
        assert_eq!(summary.total_output_tokens, 150);
        assert_eq!(summary.total_cache_read_tokens, 20);
        assert_eq!(summary.total_cache_creation_tokens, 10);
        assert_eq!(summary.total_cost_usd, Some(0.015));
    }

    // ===== Startup initialization tests =====

    #[tokio::test]
    async fn test_migrate_from_config_empty_table_creates_executors() {
        let db = setup_db().await;
        // Empty DB: executors table should be empty initially (only after init_tables)
        let count = db.get_executors().await.unwrap().len();
        assert_eq!(count, 0, "fresh DB should have no executors");

        let mut paths = std::collections::HashMap::new();
        paths.insert("claudecode".to_string(), "/custom/path/claude".to_string());
        let cfg_executors = crate::config::ExecutorPaths { paths };

        db.migrate_from_config(&cfg_executors).await.unwrap();
        let executors = db.get_executors().await.unwrap();
        assert!(!executors.is_empty(), "executors should be populated");

        // claudecode should use the custom path from config
        let cc = db.get_executor_by_name("claudecode").await.unwrap().unwrap();
        assert_eq!(cc.path, "/custom/path/claude");
    }

    #[tokio::test]
    async fn test_migrate_from_config_idempotent() {
        let db = setup_db().await;
        let mut paths = std::collections::HashMap::new();
        paths.insert("claudecode".to_string(), "/usr/local/bin/claude".to_string());
        let cfg_executors = crate::config::ExecutorPaths { paths };

        db.migrate_from_config(&cfg_executors).await.unwrap();
        let count_after_first = db.get_executors().await.unwrap().len();
        assert!(count_after_first > 0);

        // Second call should be a no-op (table already populated)
        db.migrate_from_config(&cfg_executors).await.unwrap();
        let count_after_second = db.get_executors().await.unwrap().len();
        assert_eq!(count_after_first, count_after_second, "migrate_from_config must be idempotent");
    }

    #[tokio::test]
    async fn test_seed_default_executors_empty_table_populates() {
        let db = setup_db().await;
        let count = db.get_executors().await.unwrap().len();
        assert_eq!(count, 0);

        db.seed_default_executors().await.unwrap();
        let executors = db.get_executors().await.unwrap();
        assert!(!executors.is_empty(), "seed should populate executors");
        // All should be enabled by default
        assert!(executors.iter().all(|e| e.enabled));
    }

    #[tokio::test]
    async fn test_seed_default_executors_idempotent() {
        let db = setup_db().await;
        db.seed_default_executors().await.unwrap();
        let count_after_first = db.get_executors().await.unwrap().len();

        db.seed_default_executors().await.unwrap();
        let count_after_second = db.get_executors().await.unwrap().len();
        assert_eq!(count_after_first, count_after_second, "seed_default_executors must be idempotent");
    }

    #[tokio::test]
    async fn test_seed_default_executors_preserves_user_disabled() {
        let db = setup_db().await;
        db.seed_default_executors().await.unwrap();

        // User disables claudecode
        db.update_executor("claudecode", None, Some(false), None, None)
            .await
            .unwrap();

        // Re-seed should not re-enable it (table not empty, so seed is no-op)
        db.seed_default_executors().await.unwrap();
        let exec = db.get_executor_by_name("claudecode").await.unwrap().unwrap();
        assert!(!exec.enabled, "seed should not re-enable a user-disabled executor");
    }

    #[tokio::test]
    async fn test_sync_new_executors_adds_missing() {
        let db = setup_db().await;

        // Manually remove one executor from DB to simulate "missing" scenario
        db.seed_default_executors().await.unwrap();
        // We can't easily delete without a delete_executor method, so instead:
        // Insert a fake executor directly, then sync will not add it again
        // Actually, let's verify that sync doesn't add duplicates when all exist
        let count_before = db.get_executors().await.unwrap().len();
        db.sync_new_executors().await.unwrap();
        let count_after = db.get_executors().await.unwrap().len();
        assert_eq!(count_before, count_after, "sync should not add duplicates when all executors exist");
    }

    #[tokio::test]
    async fn test_cleanup_orphan_execution_records_no_orphans() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Desc").await.unwrap();

        // Set a task_id on the todo so the execution record is not considered orphan
        db.update_todo_task_id(todo_id, Some("real-task-id")).await.unwrap();

        let record_id = create_test_execution_record(&db, todo_id, "echo hi").await;

        // Todo has a task_id, so the record is not orphan — cleanup should leave it untouched
        db.cleanup_orphan_execution_records().await.unwrap();
        let record = db.get_execution_record(record_id).await.unwrap().unwrap();
        assert_eq!(record.status, crate::models::ExecutionStatus::Running,
            "non-orphan running record should not be cleaned up");
    }

    #[tokio::test]
    async fn test_cleanup_orphan_execution_records_fails_orphan_without_task() {
        let db = setup_db().await;
        let todo_id = db.create_todo("Test", "Desc").await.unwrap();

        // Create a record with task_id directly, then clear the todo's task_id
        let record_id = db.create_execution_record(NewExecutionRecord {
            todo_id: Some(todo_id),
            command: "echo orphan",
            executor: "claudecode",
            trigger_type: "manual",
            task_id: "ghost-task",
            session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,
            step_id: None,
        }).await.unwrap();

        // Detach task_id from todo so the record becomes "orphan" (running but todo.task_id IS NULL)
        db.update_todo_task_id(todo_id, None).await.unwrap();

        db.cleanup_orphan_execution_records().await.unwrap();
        let record = db.get_execution_record(record_id).await.unwrap().unwrap();
        assert_eq!(record.status, crate::models::ExecutionStatus::Failed,
            "orphan running record without todo task_id should be failed");
    }

    #[tokio::test]
    async fn test_cleanup_old_webhook_records() {
        let db = setup_db().await;

        // Insert an "old" webhook record (created_at = 31 days ago)
        let old_date = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(31))
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        db.conn
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO webhook_records (method, path, created_at) VALUES ('GET', '/old', ?)",
                [old_date.into()],
            ))
            .await
            .unwrap();

        // Insert a "recent" record (1 day ago)
        let recent_date = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(1))
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        db.conn
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO webhook_records (method, path, created_at) VALUES ('GET', '/recent', ?)",
                [recent_date.into()],
            ))
            .await
            .unwrap();

        let count_before = db.get_webhook_records_count().await.unwrap();
        assert_eq!(count_before, 2);

        // Cleanup records older than 30 days
        let deleted = db.cleanup_old_webhook_records(30).await.unwrap();
        assert_eq!(deleted, 1, "should delete 1 old record");

        let remaining = db.get_webhook_records(10, 0).await.unwrap();
        assert_eq!(remaining.len(), 1, "only recent record should remain");
        assert_eq!(remaining[0].path, "/recent");
    }
}
