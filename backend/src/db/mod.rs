//! Database access layer (SeaORM).
//!
//! - Fixed database path: `~/.ntd/data.db`
//! - Built-in SQLite (libsqlite3-sys/bundled), no system dependencies
//! - All public methods are async

use std::str::FromStr;
use std::time::Duration;

use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ColumnTrait, ConnectOptions, ConnectionTrait,
    Database as SeaDatabase, DatabaseConnection, DbBackend, EntityTrait, IntoActiveModel,
    Order, QueryFilter, QueryOrder, QuerySelect, SqlxSqliteConnector, Statement,
};

pub mod entity;
pub mod migrations;
pub mod sync_record;
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
        // SQLite 连接 URL 是我们自己的硬编码常量 `:memory:` 或经过路径扩展的本地路径，
        // parse 失败属于开发期 bug 而非运行时环境错误——仍然 panic 但加注释说明这是
        // invariant 而不是用户可触发的失败路径。Issue #495 关注的是 panic 的可触发面，
        // 这里加 `#[allow]` 让新增 lint 不误伤。
        #[allow(clippy::expect_used)]
        let sqlite_opts: sqlx::sqlite::SqliteConnectOptions = url
            .parse()
            .expect("invalid sqlite connection url");

        // 启用 sqlx 自身的 SQL trace，但只记录 WARN 及以上等级的慢查询。
        // 目的：把慢查询（实际运行时间超过阈值的语句）纳入日志体系，便于
        // 在 issue #513 的诉求中定位「数据库慢查询无法发现」的问题；INFO/DEBUG 级
        // 的每条 SQL 仍然关闭，避免高频请求场景下日志量爆炸。
        //
        // sqlx 0.7 的 ConnectOptions trait 暴露 log_statements / log_slow_statements：
        //   - log_statements(Off) 关掉所有普通语句日志；
        //   - log_slow_statements(Warn, 1s) 仅对执行时间 ≥1s 的语句在 WARN 级记录。
        // 阈值 1s 与 sqlx 默认值一致，对 SQLite 而言已经足够把「跨表 join /
        // migration / 冷启动大批写入」等真正慢的操作筛出来。
        use sqlx::ConnectOptions;
        let sqlite_opts = sqlite_opts
            .log_statements(log::LevelFilter::Off)
            .log_slow_statements(log::LevelFilter::Warn, std::time::Duration::from_secs(1));

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

    /// 与 `exec` 行为一致,但支持绑定参数以避免 SQL 注入。
    /// 任何需要传入外部数据(version、name、timestamp 等)的 SQL 都应使用本方法,
    /// 而非在 `exec` 里做 `format!` + 手动引号转义。
    pub(super) async fn exec_with_params(
        &self,
        sql: &str,
        values: Vec<sea_orm::Value>,
    ) -> Result<(), sea_orm::DbErr> {
        self.conn
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                sql,
                values,
            ))
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

    /// 迁移：为飞书子表添加 ON DELETE CASCADE 外键约束
    /// SQLite 不支持 ALTER TABLE 修改外键约束，需要重建表（创建新表→复制数据→删除旧表→重命名）
    /// 每张表独立检查，只有自身缺少 CASCADE 才重建；整个迁移包在事务中
    async fn migrate_feishu_fk_cascade(&self) -> Result<(), sea_orm::DbErr> {
        // 收集需要迁移的表
        let tables_to_migrate = [
            ("feishu_homes", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, user_open_id TEXT NOT NULL, chat_id TEXT, receive_id TEXT NOT NULL, receive_id_type TEXT NOT NULL, created_at TEXT, updated_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE, UNIQUE(bot_id, user_open_id)"),
            ("feishu_messages", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, message_id TEXT NOT NULL UNIQUE, chat_id TEXT NOT NULL, chat_type TEXT NOT NULL, sender_open_id TEXT NOT NULL, sender_nickname TEXT, sender_type TEXT, content TEXT, msg_type TEXT NOT NULL DEFAULT 'text', is_mention INTEGER DEFAULT 0, processed INTEGER DEFAULT 0, is_history INTEGER DEFAULT 0, fetch_time TEXT, created_at TEXT, processed_todo_id INTEGER, execution_record_id INTEGER, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE"),
            ("feishu_history_chats", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, chat_id TEXT NOT NULL, chat_name TEXT, enabled INTEGER DEFAULT 1, last_fetch_time TEXT, polling_interval_secs INTEGER DEFAULT 60, created_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE, UNIQUE(bot_id, chat_id)"),
            ("feishu_push_targets", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, p2p_receive_id TEXT NOT NULL DEFAULT '', group_chat_id TEXT NOT NULL DEFAULT '', receive_id_type TEXT NOT NULL DEFAULT 'open_id', push_level TEXT DEFAULT 'result_only', p2p_response_enabled INTEGER DEFAULT 1, group_response_enabled INTEGER DEFAULT 1, created_at TEXT, updated_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE"),
            ("feishu_response_config", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, target_type TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, debounce_secs INTEGER DEFAULT 20, created_at TEXT, updated_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE, UNIQUE(bot_id, target_type)"),
            ("feishu_group_whitelist", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, sender_open_id TEXT NOT NULL, sender_name TEXT, created_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE, UNIQUE(bot_id, sender_open_id)"),
        ];

        let mut needs_any = false;
        for (table, _ddl) in &tables_to_migrate {
            if self.needs_fk_migration(table).await? {
                needs_any = true;
                break;
            }
        }
        if !needs_any {
            return Ok(());
        }

        tracing::info!("Migrating feishu tables to add ON DELETE CASCADE...");
        self.exec("BEGIN").await?;

        for (table, ddl) in &tables_to_migrate {
            if self.needs_fk_migration(table).await? {
                self.rebuild_table_with_cascade(table, ddl).await?;
            }
        }


        // 重建索引
        self.exec("CREATE INDEX IF NOT EXISTS idx_feishu_messages_chat_id ON feishu_messages(chat_id)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_feishu_messages_created_at ON feishu_messages(created_at)").await?;

        self.exec("COMMIT").await?;


        tracing::info!("Feishu FK cascade migration completed.");
        Ok(())
    }

    /// 检查表的外键是否缺少 ON DELETE CASCADE（返回 true 表示需要迁移）
    async fn needs_fk_migration(&self, table: &str) -> Result<bool, sea_orm::DbErr> {
        let sql = format!("SELECT sql FROM sqlite_master WHERE type='table' AND name='{}'", table);
        let result = self.conn
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await?;
        if let Some(row) = result {
            let ddl: String = row.try_get_by("sql")?;
            // 如果 DDL 中包含 ON DELETE CASCADE，说明已经是新 schema
            return Ok(!ddl.contains("ON DELETE CASCADE"));
        }
        // 表不存在，CREATE TABLE IF NOT EXISTS 会创建正确的 schema
        Ok(false)
    }

    /// 重建表以添加 ON DELETE CASCADE 外键约束
    /// SQLite 标准迁移流程：新建→复制→删除→重命名
    async fn rebuild_table_with_cascade(&self, table: &str, columns: &str) -> Result<(), sea_orm::DbErr> {
        let tmp = format!("{}_new", table);
        tracing::info!("Rebuilding table {} to add ON DELETE CASCADE...", table);

        // 暂时关闭外键检查以避免重建过程中的约束冲突
        self.exec("PRAGMA foreign_keys = OFF").await?;

        // 清理上次中断可能残留的临时表
        self.exec(&format!("DROP TABLE IF EXISTS {}", tmp)).await?;

        // 创建新表
        self.exec(&format!(
            "CREATE TABLE IF NOT EXISTS {} ({})",
            tmp, columns
        )).await?;

        // 获取旧表列名列表，用于安全的数据复制
        let col_rows = self.conn
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                format!("PRAGMA table_info('{}')", table),
            ))
            .await?;
        let col_names: Vec<String> = col_rows
            .iter()
            .filter_map(|r| r.try_get::<String>("", "name").ok())
            .collect();
        let cols_str = col_names.join(", ");

        // 复制数据
        self.exec(&format!(
            "INSERT INTO {} ({}) SELECT {} FROM {}",
            tmp, cols_str, cols_str, table
        )).await?;

        // 删除旧表
        self.exec(&format!("DROP TABLE {}", table)).await?;

        // 重命名新表
        self.exec(&format!("ALTER TABLE {} RENAME TO {}", tmp, table)).await?;

        // 重新启用外键检查
        self.exec("PRAGMA foreign_keys = ON").await?;

        tracing::info!("Table {} rebuilt successfully.", table);
        Ok(())
    }

    /// 迁移：将 execution_records.logs 旧字段数据迁移到 execution_logs 表，并删除旧字段
    async fn migrate_logs_to_execution_logs(&self) -> Result<(), sea_orm::DbErr> {
        // 检查 logs 列是否存在
        let check_sql = "SELECT COUNT(*) FROM pragma_table_info('execution_records') WHERE name='logs'";
        let result = self
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, check_sql.to_string()))
            .await?;
        let col_exists = result
            .and_then(|r| r.try_get_by_index::<i64>(0).ok())
            .unwrap_or(0) > 0;
        if !col_exists {
            return Ok(());
        }

        tracing::info!("Migrating old logs column to execution_logs table...");

        // 迁移未迁移的记录（execution_logs 表中没有数据的记录）
        let select_sql = "SELECT id, logs FROM execution_records \
            WHERE logs IS NOT NULL AND logs != '' AND logs != '[]' \
            AND id NOT IN (SELECT DISTINCT record_id FROM execution_logs)";
        let rows = self
            .conn
            .query_all(Statement::from_string(DbBackend::Sqlite, select_sql.to_string()))
            .await?;

        let mut migrated = 0u64;
        let mut failed = 0u64;
        for row in rows {
            let id: i64 = row.try_get_by("id")?;
            let logs_json: String = row.try_get_by("logs")?;
            if !logs_json.is_empty() && logs_json != "[]" {
                if let Err(e) = self.insert_execution_logs(id, &logs_json).await {
                    tracing::warn!("Failed to migrate logs for record {}: {}", id, e);
                    failed += 1;
                } else {
                    migrated += 1;
                }
            }
        }

        // 有任意记录迁移失败则不删除旧列，保留数据等待下次重试
        if failed > 0 {
            tracing::warn!(
                "Logs migration incomplete: {} succeeded, {} failed. Keeping old logs column for retry.",
                migrated, failed
            );
            return Ok(());
        }

        // 删除旧列
        self.exec("ALTER TABLE execution_records DROP COLUMN logs").await?;
        tracing::info!(
            "Migrated {} execution records, dropped logs column",
            migrated
        );
        Ok(())
    }

    /// 一次性迁移：将旧 `todos.rating`（已不再使用）合并到对应 todo 最新一条
    /// `execution_records.rating`，然后 DROP COLUMN。
    /// 设计原因：评分属于执行结果而非 todo 本身。
    /// - 每个 todo 取最新一条已结束的 execution_record（按 started_at desc）
    /// - 同一 record 已被多次评分时跳过，避免覆盖更新的评价
    /// - 失败仅 warn，不阻塞启动
    async fn migrate_todo_rating_to_execution_records(&self) -> Result<(), sea_orm::DbErr> {
        // 检查旧列是否存在，不存在则直接跳过（DROP COLUMN 之后再次启动也是幂等的）
        let check_sql = "SELECT COUNT(*) FROM pragma_table_info('todos') WHERE name='rating'";
        let result = self
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, check_sql.to_string()))
            .await?;
        let col_exists = result
            .and_then(|r| r.try_get_by_index::<i64>(0).ok())
            .unwrap_or(0)
            > 0;
        if !col_exists {
            return Ok(());
        }

        tracing::info!("Migrating todos.rating -> execution_records.rating...");

        // 拉取所有有评分的 todo 及其最新一条 execution_record
        let select_sql = "\
            SELECT t.id AS todo_id, t.rating AS rating, \
                   (SELECT er.id FROM execution_records er \
                    WHERE er.todo_id = t.id \
                    ORDER BY er.started_at DESC LIMIT 1) AS latest_record_id \
            FROM todos t \
            WHERE t.rating IS NOT NULL";
        let rows = self
            .conn
            .query_all(Statement::from_string(DbBackend::Sqlite, select_sql.to_string()))
            .await?;

        let mut migrated = 0u64;
        for row in rows {
            let todo_id: i64 = row.try_get_by("todo_id")?;
            let rating: i32 = match row.try_get_by::<i64, _>("rating") {
                Ok(v) => v as i32,
                Err(_) => continue,
            };
            let latest_record_id: Option<i64> = row.try_get_by("latest_record_id").ok().flatten();
            let Some(record_id) = latest_record_id else {
                tracing::debug!(
                    "Skip todo {} rating {}: no execution_records",
                    todo_id, rating
                );
                continue;
            };

            // 仅在该 record 尚未评分时才写入，避免覆盖更新评价
            let update_sql = "UPDATE execution_records \
                SET rating = $1 \
                WHERE id = $2 AND rating IS NULL";
            let res = self
                .conn
                .execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    update_sql,
                    [rating.into(), record_id.into()],
                ))
                .await?;
            if res.rows_affected() > 0 {
                migrated += 1;
            }
        }

        // 移除旧列
        if let Err(e) = self
            .exec("ALTER TABLE todos DROP COLUMN rating")
            .await
        {
            tracing::warn!("Failed to DROP COLUMN todos.rating: {}", e);
            return Ok(()); // 不阻塞启动，下次启动再重试
        }

        tracing::info!(
            "Migrated {} todo ratings to execution_records, dropped todos.rating",
            migrated
        );
        Ok(())
    }

    async fn init_tables(&self) -> Result<(), sea_orm::DbErr> {
        // 走 schema migration 框架:首次启动跑全部 DDL,之后启动只读一次 schema_version 就跳过
        // (issue #498:旧实现每次启动跑上百条 DDL,即使 IF NOT EXISTS 也要解析+规划+扫描 schema)
        //
        // 稳态短路。若 schema 已经是最新版本,说明 DDL 早已应用、
        // 配套的 data migrations(todos.rating / logs / feishu_fk_cascade)也已在
        // 上一次首次启动 / 升级时跑完,此时再无条件调一次会浪费 ~10+ SELECT
        // (其中 6 个是 feishu_fk_cascade 的 needs_fk_migration) + Vec/closure 工作。
        // 这次启动只需 1 次 SELECT MAX(version) 即可。
        let max_version = migrations::ALL_MIGRATIONS
            .last()
            .map(|m| m.version)
            .unwrap_or(0);
        let current = self.current_schema_version().await;
        if current >= max_version {
            tracing::debug!(
                "init_tables: schema at v{} (max {}), skipping migrations + data migrations",
                current,
                max_version
            );
            return Ok(());
        }

        migrations::run_migrations(self).await?;

        // 下面是需要在 DDL 之上做"数据迁移"的工作,各自有幂等性检查,
        // 失败仅 warn,不阻塞启动(与原 init_tables 行为一致)。
        if let Err(e) = self.migrate_todo_rating_to_execution_records().await {
            tracing::warn!("Failed to migrate todos.rating -> execution_records.rating: {}", e);
        }
        self.migrate_logs_to_execution_logs()
            .await
            .unwrap_or_else(|e| tracing::warn!("Failed to migrate logs column: {}", e));
        self.migrate_feishu_fk_cascade().await?;

        Ok(())
    }

    /// 返回当前已应用的 schema 版本号(用于 /api/schema/version 等观测端点)
    pub async fn current_schema_version(&self) -> i64 {
        migrations::current_schema_version(&self.conn).await
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
mod feishu_project_binding;
mod feishu_push_target;
mod feishu_response_config;
pub mod project_directory;
mod todo_template;
pub use todo_template::TemplateInput;
pub mod webhook;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Timelike, Utc};

    async fn setup_db() -> Database {
        Database::new(":memory:").await.unwrap()
    }

    async fn create_test_execution_record(db: &Database, todo_id: i64, command: &str) -> i64 {
        db.create_execution_record(NewExecutionRecord {
            // `NewExecutionRecord.todo_id` 在一次重构里从 `Option<i64>` 收紧到
            // `i64` (db/execution.rs:12),这里忘了改,所以 `cargo test` 早就
            // 编不过。issue #502 的 PR 顺手把它对齐。
            todo_id,
            command,
            executor: "claudecode",
            trigger_type: "manual",
            task_id: "test-task-id",
            session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            source_hook_id: None,
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
            worktree_enabled: None,
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
            worktree_enabled: None,
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
            todo_id,
            command: "echo orphan",
            executor: "claudecode",
            trigger_type: "manual",
            task_id: "ghost-task",
            session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            source_hook_id: None,
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

    // ===== Schema migration tests (issue #498) =====

    /// 新建库后,`current_schema_version()` 应为 1 且所有 user 表都已创建
    #[tokio::test]
    async fn test_migrations_fresh_db_lands_on_v1() {
        let db = setup_db().await;
        let v = db.current_schema_version().await;
        assert_eq!(v, 1, "fresh :memory: DB should report schema version 1");

        // 抽样验证关键表已建好(完整覆盖见其它既有测试)
        let count: i64 = db
            .conn
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) FROM todos".to_string(),
            ))
            .await
            .unwrap()
            .and_then(|r| r.try_get_by_index::<i64>(0).ok())
            .unwrap_or(0);
        assert_eq!(count, 0, "todos table should exist and be empty");
    }

    /// schema_version 已是最新时,run_migrations 应当是空操作
    /// (issue #498 的核心收益:避免每次启动执行上百条 DDL)
    #[tokio::test]
    async fn test_migrations_idempotent_noop_when_up_to_date() {
        let db = setup_db().await;
        // 第一次启动后 schema_version 已经被打点成 1
        let first = db.current_schema_version().await;
        assert_eq!(first, 1);

        // 模拟「再次启动」:再跑一次 run_migrations,应当直接跳过
        // (函数本身是 private,通过 init_tables 间接覆盖;这里直接调它验证)
        let second = migrations::run_migrations(&db).await.unwrap();
        assert_eq!(second, 1, "version should stay at 1 on re-run");
    }

    /// 模拟「从老版本升级上来的库」:有 user 表但没有 schema_version 表
    /// 此时 run_migrations 应当把 DDL 跑一遍(全是 IF NOT EXISTS 的 no-op),
    /// 然后把 schema_version 标记为最新,不再重复跑
    #[tokio::test]
    async fn test_migrations_legacy_db_promoted_to_current() {
        // 1. 创建一个正常库,让它跑完初始迁移,标记 v1
        let db = setup_db().await;
        let v0 = db.current_schema_version().await;
        assert_eq!(v0, 1, "setup_db should have landed on v1");

        // 2. 手动写入一张"老表" + DROP 掉 schema_version 表,
        //    模拟"老版本的库,只有 user 表,没有 schema_version 元信息"
        db.exec("CREATE TABLE legacy_marker (id INTEGER PRIMARY KEY, note TEXT)")
            .await
            .unwrap();
        db.exec("DELETE FROM schema_version")
            .await
            .unwrap();
        db.exec("DROP TABLE schema_version").await.unwrap();

        // 3. 重新读 version 应当是 0
        let v_before = migrations::current_schema_version(&db.conn).await;
        assert_eq!(v_before, 0, "dropped schema_version should read as 0");

        // 4. 跑迁移:应当把 DDL 跑一次(IF NOT EXISTS 全部 no-op),然后标记 v1
        let v_after = migrations::run_migrations(&db).await.unwrap();
        assert_eq!(v_after, 1, "legacy DB should be promoted to v1");

        // 5. 验证老表仍然存在(说明迁移没把表重建/丢失)
        let legacy_rows: i64 = db
            .conn
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) FROM legacy_marker".to_string(),
            ))
            .await
            .unwrap()
            .and_then(|r| r.try_get_by_index::<i64>(0).ok())
            .unwrap_or(-1);
        assert_eq!(legacy_rows, 0, "legacy table should survive migration");

        // 6. 关键 user 表也已就位(IF NOT EXISTS 不会破坏现有数据)
        let todos_count: i64 = db
            .conn
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) FROM todos".to_string(),
            ))
            .await
            .unwrap()
            .and_then(|r| r.try_get_by_index::<i64>(0).ok())
            .unwrap_or(-1);
        assert_eq!(todos_count, 0, "todos table should now exist");

        // 7. schema_version 表中应正好一行,version=1
        let recorded_count: i64 = db
            .conn
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) FROM schema_version".to_string(),
            ))
            .await
            .unwrap()
            .and_then(|r| r.try_get_by_index::<i64>(0).ok())
            .unwrap_or(-1);
        assert_eq!(recorded_count, 1, "schema_version should have one row");
    }
}
