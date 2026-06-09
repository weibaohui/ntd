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
    Order, QueryFilter, QueryOrder, QuerySelect, Statement,
};

pub mod entity;
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

        let mut opt = ConnectOptions::new(url);
        opt.max_connections(1)
            .min_connections(1)
            .connect_timeout(Duration::from_secs(5))
            .sqlx_logging(false);

        let conn = SeaDatabase::connect(opt).await?;
        let db = Self { conn };

        // Optimize SQLite for concurrent read / write performance
        db.exec("PRAGMA busy_timeout = 5000").await?;
        // Enable foreign key enforcement (SQLite default is OFF; CASCADE depends on this)
        db.exec("PRAGMA foreign_keys = ON").await?;
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

    async fn init_tables(&self) -> Result<(), sea_orm::DbErr> {
        self.exec(
            "CREATE TABLE IF NOT EXISTS todos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                prompt TEXT DEFAULT '',
                status TEXT DEFAULT 'pending',
                created_at TEXT,
                updated_at TEXT,
                deleted_at TEXT,
                executor TEXT DEFAULT 'claudecode',
                scheduler_enabled INTEGER DEFAULT 0,
                scheduler_config TEXT,
                task_id TEXT,
                workspace TEXT
            )",
        )
        .await?;

        self.exec(
            "CREATE TABLE IF NOT EXISTS tags (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                color TEXT DEFAULT '#1890ff',
                created_at TEXT
            )",
        )
        .await?;

        self.exec(
            "CREATE TABLE IF NOT EXISTS todo_tags (
                todo_id INTEGER,
                tag_id INTEGER,
                PRIMARY KEY (todo_id, tag_id),
                FOREIGN KEY (todo_id) REFERENCES todos(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )",
        )
        .await?;

        self.exec(
            "CREATE TABLE IF NOT EXISTS execution_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                todo_id INTEGER,
                status TEXT DEFAULT 'running',
                command TEXT,
                stdout TEXT DEFAULT '',
                stderr TEXT DEFAULT '',
                result TEXT,
                usage TEXT,
                executor TEXT,
                model TEXT,
                started_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                finished_at TEXT,
                trigger_type TEXT DEFAULT 'manual',
                pid INTEGER,
                task_id TEXT,
                session_id TEXT,
                FOREIGN KEY (todo_id) REFERENCES todos(id) ON DELETE CASCADE
            )",
        )
        .await?;

        // 执行日志表（每条日志一行，支持分页加载）
        self.exec(
            "CREATE TABLE IF NOT EXISTS execution_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                record_id INTEGER NOT NULL,
                timestamp TEXT NOT NULL,
                log_type TEXT NOT NULL DEFAULT 'info',
                content TEXT NOT NULL DEFAULT '',
                metadata TEXT DEFAULT '{}',
                FOREIGN KEY (record_id) REFERENCES execution_records(id) ON DELETE CASCADE
            )",
        )
        .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_execution_logs_record ON execution_logs(record_id)")
            .await?;

        // 添加 pid 字段的迁移（向后兼容）
        self.exec("ALTER TABLE execution_records ADD COLUMN pid INTEGER")
            .await
            .ok(); // 忽略错误，因为字段可能已存在

        // 添加 task_id 字段的迁移（向后兼容）
        self.exec("ALTER TABLE execution_records ADD COLUMN task_id TEXT")
            .await
            .ok(); // 忽略错误，因为字段可能已存在

        // 添加 session_id 字段的迁移（向后兼容）
        self.exec("ALTER TABLE execution_records ADD COLUMN session_id TEXT")
            .await
            .ok(); // 忽略错误，因为字段可能已存在

        // 添加 workspace 字段的迁移（向后兼容）
        self.exec("ALTER TABLE todos ADD COLUMN workspace TEXT")
            .await
            .ok(); // 忽略错误，因为字段可能已存在

        // 添加 worktree_enabled 字段的迁移（向后兼容）
        self.exec("ALTER TABLE todos ADD COLUMN worktree_enabled INTEGER DEFAULT 0")
            .await
            .ok(); // 忽略错误，因为字段可能已存在

        // 添加 todo_progress 字段的迁移（向后兼容）
        self.exec("ALTER TABLE execution_records ADD COLUMN todo_progress TEXT")
            .await
            .ok(); // 忽略错误，因为字段可能已存在

        // 添加 execution_stats 字段的迁移（向后兼容）
        self.exec("ALTER TABLE execution_records ADD COLUMN execution_stats TEXT")
            .await
            .ok(); // 忽略错误，因为字段可能已存在

        // 添加 resume_message 字段的迁移（向后兼容）
        self.exec("ALTER TABLE execution_records ADD COLUMN resume_message TEXT")
            .await
            .ok();

        // 添加 hook 触发起源字段（向后兼容）—— 用于在目标 todo 的执行记录里
        // 回显"被 #X 标题 的 '触发时机' hook 触发"，避免列表里 hook 触发记录
        // 与手动/cron 触发无法区分。
        self.exec("ALTER TABLE execution_records ADD COLUMN source_todo_id INTEGER")
            .await
            .ok();
        self.exec("ALTER TABLE execution_records ADD COLUMN source_todo_title TEXT")
            .await
            .ok();
        self.exec("ALTER TABLE execution_records ADD COLUMN source_hook_id INTEGER")
            .await
            .ok();

        // 添加 scheduler_timezone 字段的迁移（向后兼容）
        self.exec("ALTER TABLE todos ADD COLUMN scheduler_timezone TEXT")
            .await
            .ok(); // 忽略错误，因为字段可能已存在

        // 添加 hooks 字段的迁移（向后兼容）—— 内联 hook 列表存为 JSON 数组
        self.exec("ALTER TABLE todos ADD COLUMN hooks TEXT")
            .await
            .ok(); // 忽略错误，因为字段可能已存在

        // 迁移：将 execution_records.logs 旧字段数据转移到 execution_logs 表，并删除旧字段
        self.migrate_logs_to_execution_logs().await
            .unwrap_or_else(|e| tracing::warn!("Failed to migrate logs column: {}", e));

        // Skill invocations tracking table
        self.exec(
            "CREATE TABLE IF NOT EXISTS skill_invocations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                skill_name TEXT NOT NULL,
                executor TEXT NOT NULL,
                todo_id INTEGER,
                status TEXT DEFAULT 'invoked',
                duration_ms INTEGER,
                invoked_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc')),
                FOREIGN KEY (todo_id) REFERENCES todos(id) ON DELETE CASCADE
            )",
        )
        .await?;

        // --- Indexes for frequently-filtered columns ---
        self.exec("CREATE INDEX IF NOT EXISTS idx_todos_deleted_at ON todos(deleted_at)")
            .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_todos_status ON todos(status)")
            .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_todos_task_id ON todos(task_id)")
            .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_todo_id ON execution_records(todo_id)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_task_id ON execution_records(task_id)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_pid ON execution_records(pid)")
            .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_session_id ON execution_records(session_id)").await?;
        self.exec(
            "CREATE INDEX IF NOT EXISTS idx_execution_records_status ON execution_records(status)",
        )
        .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_todo_tags_todo_id ON todo_tags(todo_id)")
            .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_skill_invocations_skill_name ON skill_invocations(skill_name)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_skill_invocations_executor ON skill_invocations(executor)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_skill_invocations_todo_id ON skill_invocations(todo_id)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_started_at ON execution_records(started_at)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_executor ON execution_records(executor)").await?;
        self.exec(
            "CREATE INDEX IF NOT EXISTS idx_execution_records_model ON execution_records(model)",
        )
        .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_todo_finished ON execution_records(todo_id, finished_at DESC)").await?;

        // Trigger: fill created_at with UTC time on INSERT if not set
        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_todos_created_at_utc AFTER INSERT ON todos
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN
                 UPDATE todos SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        // Use BEFORE UPDATE trigger so that if the application already sets updated_at,
        // the trigger doesn't overwrite it with the wrong timezone. Only auto-fill when
        // the value is NULL or empty.
        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_todos_updated_at_utc BEFORE UPDATE OF updated_at ON todos
             WHEN new.updated_at IS NULL OR new.updated_at = ''
             BEGIN
                 UPDATE todos SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_tags_created_at_utc AFTER INSERT ON tags
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN
                 UPDATE tags SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        // Agent Bots table
        self.exec(
            "CREATE TABLE IF NOT EXISTS agent_bots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bot_type TEXT NOT NULL,
                bot_name TEXT NOT NULL,
                app_id TEXT NOT NULL,
                app_secret TEXT NOT NULL,
                bot_open_id TEXT,
                domain TEXT,
                enabled INTEGER DEFAULT 1,
                config TEXT DEFAULT '{}',
                created_at TEXT,
                updated_at TEXT
            )",
        )
        .await?;

        // Migrate: add config column if missing (existing databases)
        let cols = self
            .conn
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA table_info(agent_bots)".to_string(),
            ))
            .await
            .unwrap_or_default();
        let has_config = cols.iter().any(|row| {
            row.try_get::<String>("", "name")
                .map(|n| n == "config")
                .unwrap_or(false)
        });
        if !has_config {
            self.exec("ALTER TABLE agent_bots ADD COLUMN config TEXT DEFAULT '{}'")
                .await?;
        }

        // Feishu Homes table
        self.exec(
            "CREATE TABLE IF NOT EXISTS feishu_homes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bot_id INTEGER NOT NULL,
                user_open_id TEXT NOT NULL,
                chat_id TEXT,
                receive_id TEXT NOT NULL,
                receive_id_type TEXT NOT NULL,
                created_at TEXT,
                updated_at TEXT,
                FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE,
                UNIQUE(bot_id, user_open_id)
            )",
        )
        .await?;

        // Feishu Messages table
        self.exec(
            "CREATE TABLE IF NOT EXISTS feishu_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bot_id INTEGER NOT NULL,
                message_id TEXT NOT NULL UNIQUE,
                chat_id TEXT NOT NULL,
                chat_type TEXT NOT NULL,
                sender_open_id TEXT NOT NULL,
                sender_nickname TEXT,
                sender_type TEXT,
                content TEXT,
                msg_type TEXT NOT NULL DEFAULT 'text',
                is_mention INTEGER DEFAULT 0,
                processed INTEGER DEFAULT 0,
                is_history INTEGER DEFAULT 0,
                fetch_time TEXT,
                created_at TEXT,
                FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE
            )",
        )
        .await?;

        // 添加 sender_nickname 字段的迁移（向后兼容）
        self.exec("ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS sender_nickname TEXT")
            .await
            .ok();

        // 添加 sender_type 字段的迁移（向后兼容）
        self.exec("ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS sender_type TEXT")
            .await
            .ok();

        // 添加 is_history 字段的迁移（向后兼容）
        self.exec(
            "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS is_history INTEGER DEFAULT 0",
        )
        .await
        .ok();

        // 添加 fetch_time 字段的迁移（向后兼容）
        self.exec("ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS fetch_time TEXT")
            .await
            .ok();

        // 添加 processed_todo_id 字段的迁移（向后兼容）
        // 注意：SQLite 3.39.0+ 支持 IF NOT EXISTS，但旧版本不支持此语法
        // 先尝试带 IF NOT EXISTS 的版本，失败后再尝试不带 IF NOT EXISTS 的版本
        let add_result = self
            .exec("ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS processed_todo_id INTEGER")
            .await;
        if add_result.is_err() {
            // 尝试不带 IF NOT EXISTS 的版本（如果列已存在会报错，被 .ok() 忽略）
            self.exec("ALTER TABLE feishu_messages ADD COLUMN processed_todo_id INTEGER")
                .await
                .ok();
        }

        // 添加 execution_record_id 字段的迁移
        let add_exec_result = self
            .exec(
                "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS execution_record_id INTEGER",
            )
            .await;
        if add_exec_result.is_err() {
            self.exec("ALTER TABLE feishu_messages ADD COLUMN execution_record_id INTEGER")
                .await
                .ok();
        }

        // Feishu History Chats table
        self.exec(
            "CREATE TABLE IF NOT EXISTS feishu_history_chats (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bot_id INTEGER NOT NULL,
                chat_id TEXT NOT NULL,
                chat_name TEXT,
                enabled INTEGER DEFAULT 1,
                last_fetch_time TEXT,
                polling_interval_secs INTEGER DEFAULT 60,
                created_at TEXT,
                FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE,
                UNIQUE(bot_id, chat_id)
            )",
        )
        .await?;

        // Feishu Messages table indexes (created after table to ensure table exists)
        self.exec("CREATE INDEX IF NOT EXISTS idx_feishu_messages_chat_id ON feishu_messages(chat_id)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_feishu_messages_created_at ON feishu_messages(created_at)").await?;

        // Feishu Push Targets — one row per bot, p2p and group IDs as separate fields
        self.exec(
            "CREATE TABLE IF NOT EXISTS feishu_push_targets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bot_id INTEGER NOT NULL,
                p2p_receive_id TEXT NOT NULL DEFAULT '',
                group_chat_id TEXT NOT NULL DEFAULT '',
                receive_id_type TEXT NOT NULL DEFAULT 'open_id',
                push_level TEXT DEFAULT 'result_only',
                p2p_response_enabled INTEGER DEFAULT 1,
                group_response_enabled INTEGER DEFAULT 1,
                created_at TEXT,
                updated_at TEXT,
                FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE
            )",
        )
        .await?;

        // feishu_response_config 表（响应开关独立配置）
        self.exec(
            "CREATE TABLE IF NOT EXISTS feishu_response_config (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bot_id INTEGER NOT NULL,
                target_type TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                debounce_secs INTEGER DEFAULT 20,
                created_at TEXT,
                updated_at TEXT,
                FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE,
                UNIQUE(bot_id, target_type)
            )",
        )
        .await?;

        // Migrate: add debounce_secs column if missing (for existing tables created before this column)
        let has_debounce: i64 = self.conn
            .query_one(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM pragma_table_info('feishu_response_config') WHERE name='debounce_secs'",
            ))
            .await?
            .map(|r| r.try_get::<i64>("", "COUNT(*)").unwrap_or(0))
            .unwrap_or(0);
        if has_debounce == 0 {
            self.exec(
                "ALTER TABLE feishu_response_config ADD COLUMN debounce_secs INTEGER DEFAULT 20",
            )
            .await?;
        }

        // feishu_group_whitelist 表（群聊响应白名单）
        self.exec(
            "CREATE TABLE IF NOT EXISTS feishu_group_whitelist (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bot_id INTEGER NOT NULL,
                sender_open_id TEXT NOT NULL,
                sender_name TEXT,
                created_at TEXT,
                FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE,
                UNIQUE(bot_id, sender_open_id)
            )",
        )
        .await?;

        // 飞书项目绑定表 — 将飞书聊天会话绑定到项目目录
        // - UNIQUE(bot_id, chat_id)：同一 Bot 下的同一聊天只能绑定一个项目
        // - status 默认 'idle'，执行任务时更新为 'running'（执行完成后清理脚本重置为 idle）
        // - session_id：Claude Code 的会话 ID，首次执行时填充，resume 时保持不变
        // - latest_record_id：最近一次 execution_record.id，用于判断是否可 resume
        // - chat_id 特殊值 "__pending__"：Web UI 创建的待绑定记录，等待飞书侧 /bind 补齐
        // - created_at/updated_at 为 NOT NULL，业务层写入（非触发器）
        self.exec(
            "CREATE TABLE IF NOT EXISTS feishu_project_bindings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bot_id INTEGER NOT NULL,
                chat_id TEXT NOT NULL,
                chat_type TEXT NOT NULL,
                project_dir_id INTEGER NOT NULL,
                todo_id INTEGER NOT NULL,
                session_id TEXT,
                latest_record_id INTEGER,
                status TEXT NOT NULL DEFAULT 'idle',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(bot_id, chat_id)
            )",
        )
        .await?;
        // Index for latest_record_id lookups (hot path in resume routing & cleanup)
        self.exec("CREATE INDEX IF NOT EXISTS idx_feishu_bindings_record_id ON feishu_project_bindings(latest_record_id)")
            .await?;
        // Index for bot_id lookups in /list and cleanup
        self.exec("CREATE INDEX IF NOT EXISTS idx_feishu_bindings_bot_id ON feishu_project_bindings(bot_id)")
            .await?;

        // Executors table (executor config moved from config.yaml to database)
        self.exec(
            "CREATE TABLE IF NOT EXISTS executors (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                path TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                display_name TEXT NOT NULL DEFAULT '',
                session_dir TEXT NOT NULL DEFAULT '',
                created_at TEXT,
                updated_at TEXT
            )",
        )
        .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_executors_name ON executors(name)")
            .await?;

        // Migration: add session_dir column if missing (existing databases)
        let _ = self
            .exec("ALTER TABLE executors ADD COLUMN session_dir TEXT NOT NULL DEFAULT ''")
            .await;

        // Project directories table
        self.exec(
            "CREATE TABLE IF NOT EXISTS project_directories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                name TEXT,
                created_at TEXT,
                updated_at TEXT
            )",
        )
        .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_project_directories_path ON project_directories(path)")
            .await?;

        // Project directories timestamps triggers
        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_project_directories_created_at_utc AFTER INSERT ON project_directories
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN
                 UPDATE project_directories SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        // Todo templates table
        self.exec(
            "CREATE TABLE IF NOT EXISTS todo_templates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                prompt TEXT,
                category TEXT NOT NULL DEFAULT '',
                sort_order INTEGER,
                is_system INTEGER NOT NULL DEFAULT 0,
                created_at TEXT,
                updated_at TEXT
            )",
        )
        .await?;

        // Migration: add is_system column if missing (existing databases)
        let has_is_system: i64 = self.conn
            .query_one(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM pragma_table_info('todo_templates') WHERE name='is_system'".to_string(),
            ))
            .await?
            .map(|r| r.try_get::<i64>("", "COUNT(*)").unwrap_or(0))
            .unwrap_or(0);
        if has_is_system == 0 {
            self.exec("ALTER TABLE todo_templates ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0")
                .await?;
        }

        // Migration: add source_url and last_sync_at columns if missing (custom template subscription)
        let has_source_url: i64 = self.conn
            .query_one(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM pragma_table_info('todo_templates') WHERE name='source_url'".to_string(),
            ))
            .await?
            .map(|r| r.try_get::<i64>("", "COUNT(*)").unwrap_or(0))
            .unwrap_or(0);
        if has_source_url == 0 {
            self.exec("ALTER TABLE todo_templates ADD COLUMN source_url TEXT")
                .await?;
        }

        let has_last_sync_at: i64 = self.conn
            .query_one(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM pragma_table_info('todo_templates') WHERE name='last_sync_at'".to_string(),
            ))
            .await?
            .map(|r| r.try_get::<i64>("", "COUNT(*)").unwrap_or(0))
            .unwrap_or(0);
        if has_last_sync_at == 0 {
            self.exec("ALTER TABLE todo_templates ADD COLUMN last_sync_at TEXT")
                .await?;
        }

        // Todo templates timestamps triggers
        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_todo_templates_created_at_utc AFTER INSERT ON todo_templates
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN
                 UPDATE todo_templates SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_project_directories_updated_at_utc BEFORE UPDATE ON project_directories
             WHEN new.updated_at IS NULL OR new.updated_at = ''
             BEGIN
                 SELECT raise(IGNORE);
             END",
        )
        .await?;

        // Executors timestamps triggers
        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_executors_created_at_utc AFTER INSERT ON executors
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN
                 UPDATE executors SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_executors_updated_at_utc BEFORE UPDATE ON executors
             WHEN new.updated_at IS NULL OR new.updated_at = ''
             BEGIN
                 SELECT raise(IGNORE);
             END",
        )
        .await?;

        // Webhooks table
        self.exec(
            "CREATE TABLE IF NOT EXISTS webhooks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                default_todo_id INTEGER,
                created_at TEXT,
                updated_at TEXT
            )",
        )
        .await?;

        // Webhooks timestamps triggers
        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_webhooks_created_at_utc AFTER INSERT ON webhooks
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN
                 UPDATE webhooks SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_webhooks_updated_at_utc BEFORE UPDATE ON webhooks
             WHEN new.updated_at IS NULL OR new.updated_at = ''
             BEGIN
                 SELECT raise(IGNORE);
             END",
        )
        .await?;

        // Webhook records table
        self.exec(
            "CREATE TABLE IF NOT EXISTS webhook_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                webhook_id INTEGER,
                method TEXT NOT NULL,
                path TEXT NOT NULL,
                query_params TEXT,
                body TEXT,
                content_type TEXT,
                triggered_todo_id INTEGER,
                status_code INTEGER,
                response_body TEXT,
                created_at TEXT
            )",
        )
        .await?;

        // Indexes for webhook_records (improve N+1 query performance)
        self.exec("CREATE INDEX IF NOT EXISTS idx_webhook_records_webhook_id ON webhook_records(webhook_id)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_webhook_records_triggered_todo_id ON webhook_records(triggered_todo_id)").await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_webhook_records_created_at ON webhook_records(created_at)").await?;

        // Webhook records timestamps triggers
        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_webhook_records_created_at_utc AFTER INSERT ON webhook_records
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN
                 UPDATE webhook_records SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        // ===== Hook System (inline on todos.hooks, no separate tables) =====
        // Usage daily stats table (stores aggregated usage statistics)
        self.exec(
            "CREATE TABLE IF NOT EXISTS usage_daily_stats (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                project_path TEXT,
                session_id TEXT,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                extra_total_tokens INTEGER NOT NULL DEFAULT 0,
                total_cost REAL NOT NULL DEFAULT 0.0,
                credits REAL,
                message_count INTEGER,
                models_used TEXT NOT NULL DEFAULT '[]',
                project TEXT,
                versions TEXT,
                last_activity TEXT,
                stats_type TEXT NOT NULL DEFAULT 'daily',
                created_at TEXT
            )",
        )
        .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_usage_daily_stats_date ON usage_daily_stats(date)")
            .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_usage_daily_stats_stats_type ON usage_daily_stats(stats_type)")
            .await?;

        // Usage model breakdowns table (stores per-model breakdown for each daily stat)
        self.exec(
            "CREATE TABLE IF NOT EXISTS usage_model_breakdowns (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                daily_stat_id INTEGER NOT NULL,
                model_name TEXT NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                extra_total_tokens INTEGER NOT NULL DEFAULT 0,
                cost REAL NOT NULL DEFAULT 0.0,
                FOREIGN KEY (daily_stat_id) REFERENCES usage_daily_stats(id) ON DELETE CASCADE
            )",
        )
        .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_usage_model_breakdowns_daily_stat_id ON usage_model_breakdowns(daily_stat_id)")
            .await?;

        // Usage stats timestamps triggers
        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_usage_daily_stats_created_at_utc AFTER INSERT ON usage_daily_stats
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN
                 UPDATE usage_daily_stats SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        // Usage executor daily stats table (stores per-executor daily token usage)
        self.exec(
            "CREATE TABLE IF NOT EXISTS usage_executor_daily_stats (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                executor TEXT NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                extra_total_tokens INTEGER NOT NULL DEFAULT 0,
                total_cost REAL NOT NULL DEFAULT 0.0,
                credits REAL,
                message_count INTEGER,
                model TEXT,
                execution_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT,
                UNIQUE(date, executor)
            )",
        )
        .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_usage_executor_daily_stats_date ON usage_executor_daily_stats(date)")
            .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_usage_executor_daily_stats_executor ON usage_executor_daily_stats(executor)")
            .await?;

        // Usage executor daily stats timestamps triggers
        self.exec(
            "CREATE TRIGGER IF NOT EXISTS set_usage_executor_daily_stats_created_at_utc AFTER INSERT ON usage_executor_daily_stats
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN
                 UPDATE usage_executor_daily_stats SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        // 同步记录表
        self.exec(
            "CREATE TABLE IF NOT EXISTS sync_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                direction TEXT NOT NULL,
                conflict_mode TEXT NOT NULL,
                status TEXT NOT NULL,
                data_type TEXT NOT NULL,
                details TEXT,
                error_message TEXT,
                created_at TEXT
            )",
        )
        .await?;
        self.exec("CREATE INDEX IF NOT EXISTS idx_sync_records_created_at ON sync_records(created_at DESC)")
            .await?;

        // 迁移：为飞书子表添加 ON DELETE CASCADE 外键约束
        self.migrate_feishu_fk_cascade().await?;

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
            todo_id,
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
        })
        .await
        .unwrap();
        let after = truncate_seconds(Utc::now());

        let (records, _) = db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id,
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
        db.update_todo_executor(id, "joinai").await.unwrap();
        let todo = db.get_todo(id).await.unwrap().unwrap();
        assert_eq!(todo.executor, Some("joinai".to_string()));
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
            todo_id,
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
            todo_id,
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
            todo_id,
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
        })
        .await
        .unwrap();

        let (running, total_running) =
            db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
                todo_id,
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
                todo_id,
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
                todo_id,
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
            todo_id,
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
                todo_id,
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
        })
        .await
        .unwrap();
        let (records, _) = db.get_execution_records(crate::db::execution::ExecutionRecordQuery {
            todo_id,
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
}
