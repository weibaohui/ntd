//! 数据库迁移框架
//!
//! 通过 `schema_version` 表追踪已应用的迁移版本号，启动时只执行尚未应用的迁移，
//! 把冷启动成本从 O(全部 DDL) 降到 O(待执行迁移)。
//!
//! 设计取舍：
//! - 手写 runner 而非引入 `sea-orm-migration` / `refinery`：项目历史 DDL 全是
//!   在 `mod.rs::init_tables()` 中手写的 `Statement::from_string`，没有 cli 工作流，
//!   引入外部库会带来一次大改造但收益有限。Issue #498 选择了相同路线。
//! - 版本号用 `i64`，单调递增即可；不在此提供 rollback（issue 未要求）。
//! - 每个迁移是一个独立的 struct，原因：
//!   1. 单元测试可单独运行/验证每个迁移；
//!   2. 启动日志能精确指出哪个迁移失败，方便排查。
//! - 启动幂等性依赖 SQLite `CREATE ... IF NOT EXISTS` 与各迁移内部的「已应用？」判断。

use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use std::collections::HashSet;

use super::Database;

/// 一个数据库迁移。每个迁移是「版本号 + 名字 + 升级函数」的不可变组合。
#[async_trait]
pub(super) trait Migration: Send + Sync {
    /// 单调递增的版本号。新迁移必须严格大于已有版本。
    fn version(&self) -> i64;

    /// 简短的可读名字，用于日志与 `schema_version.name` 列。
    fn name(&self) -> &'static str;

    /// 执行迁移。失败时返回 `Err` 让 runner 中止启动（区别于「无害失败」）。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr>;
}

/// 按版本号升序返回所有已注册的迁移。
///
/// 新增迁移：在末尾追加一行即可，runner 会自动跳过已应用的并执行新版本。
pub(super) fn all_migrations() -> Vec<Box<dyn Migration>> {
    vec![
        Box::new(V1InitialSchema),
        Box::new(V2TodoRatingDropColumn),
        Box::new(V3LogsToExecutionLogs),
        Box::new(V4FeishuFkCascade),
    ]
}

// ---------------------------------------------------------------------------
// v1: 首次建库 / 兼容旧库
// ---------------------------------------------------------------------------

/// v1 迁移：所有初始表、索引、触发器，以及历史上为兼容旧库加过的列。
///
/// 所有语句都设计成幂等的（`IF NOT EXISTS` 或迁移内部检查），因此可以在已有数据
/// 的旧库上反复执行而不破坏数据。`.ok()` 历史上用来静默吞掉"列已存在"错误，
/// 这里改为 `unwrap_or_else(|e| tracing::warn!(...))`，让真实迁移失败也能从日志发现。
pub(super) struct V1InitialSchema;

#[async_trait]
impl Migration for V1InitialSchema {
    fn version(&self) -> i64 {
        1
    }
    fn name(&self) -> &'static str {
        "initial_schema"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        v1_initial_schema(db).await
    }
}

async fn v1_initial_schema(db: &Database) -> Result<(), sea_orm::DbErr> {
    // ---- todos ----
    db.exec(
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

    // ---- tags / todo_tags ----
    db.exec(
        "CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            color TEXT DEFAULT '#1890ff',
            created_at TEXT
        )",
    )
    .await?;
    db.exec(
        "CREATE TABLE IF NOT EXISTS todo_tags (
            todo_id INTEGER,
            tag_id INTEGER,
            PRIMARY KEY (todo_id, tag_id),
            FOREIGN KEY (todo_id) REFERENCES todos(id) ON DELETE CASCADE,
            FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
        )",
    )
    .await?;

    // ---- execution_records / execution_logs ----
    db.exec(
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
    db.exec(
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_logs_record ON execution_logs(record_id)")
        .await?;

    // ---- 向后兼容旧库：execution_records / todos 上历史追加的列 ----
    // 失败仅 warn（旧库上「列已存在」是预期情况），不阻塞启动。
    db.exec("ALTER TABLE execution_records ADD COLUMN pid INTEGER")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN pid: {} (column likely already exists)",
                e
            );
        });
    db.exec("ALTER TABLE execution_records ADD COLUMN task_id TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN task_id: {}",
                e
            );
        });
    db.exec("ALTER TABLE execution_records ADD COLUMN session_id TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN session_id: {}",
                e
            );
        });
    db.exec("ALTER TABLE todos ADD COLUMN workspace TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE todos ADD COLUMN workspace: {}",
                e
            );
        });
    db.exec("ALTER TABLE todos ADD COLUMN worktree_enabled INTEGER DEFAULT 0")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE todos ADD COLUMN worktree_enabled: {}",
                e
            );
        });
    db.exec("ALTER TABLE execution_records ADD COLUMN todo_progress TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN todo_progress: {}",
                e
            );
        });
    db.exec("ALTER TABLE execution_records ADD COLUMN execution_stats TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN execution_stats: {}",
                e
            );
        });
    db.exec("ALTER TABLE execution_records ADD COLUMN resume_message TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN resume_message: {}",
                e
            );
        });
    // hook 触发起源字段：在目标 todo 的执行记录里回显「被 #X 标题 的 '触发时机' hook 触发」，
    // 避免列表里 hook 触发记录与手动/cron 触发无法区分。
    db.exec("ALTER TABLE execution_records ADD COLUMN source_todo_id INTEGER")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN source_todo_id: {}",
                e
            );
        });
    db.exec("ALTER TABLE execution_records ADD COLUMN source_todo_title TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN source_todo_title: {}",
                e
            );
        });
    db.exec("ALTER TABLE execution_records ADD COLUMN source_hook_id INTEGER")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN source_hook_id: {}",
                e
            );
        });
    db.exec("ALTER TABLE todos ADD COLUMN scheduler_timezone TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE todos ADD COLUMN scheduler_timezone: {}",
                e
            );
        });
    db.exec("ALTER TABLE todos ADD COLUMN hooks TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE todos ADD COLUMN hooks: {}",
                e
            );
        });
    db.exec("ALTER TABLE todos ADD COLUMN acceptance_criteria TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE todos ADD COLUMN acceptance_criteria: {}",
                e
            );
        });
    db.exec("ALTER TABLE execution_records ADD COLUMN rating INTEGER")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN rating: {}",
                e
            );
        });

    // ---- 自动评审（auto-review）字段 ----
    // todos.todo_type: 0=normal, 1=reviewer_template(系统专用), 2=review_instance(评审实例)
    db.exec("ALTER TABLE todos ADD COLUMN todo_type INTEGER DEFAULT 0")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE todos ADD COLUMN todo_type: {}",
                e
            );
        });
    // todos.parent_todo_id: review_instance 关联到被评审的原 todo
    db.exec("ALTER TABLE todos ADD COLUMN parent_todo_id INTEGER")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE todos ADD COLUMN parent_todo_id: {}",
                e
            );
        });
    // todos.auto_review_enabled: 原 todo 是否在完成后自动 spawn 评审 (默认开)
    db.exec("ALTER TABLE todos ADD COLUMN auto_review_enabled INTEGER DEFAULT 1")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE todos ADD COLUMN auto_review_enabled: {}",
                e
            );
        });
    // execution_records.source_execution_record_id: 评审记录精确回填到「原那条」执行记录
    db.exec("ALTER TABLE execution_records ADD COLUMN source_execution_record_id INTEGER")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN source_execution_record_id: {}",
                e
            );
        });
    // execution_records.last_review_status: pending/success/failed/interrupted/skipped
    db.exec("ALTER TABLE execution_records ADD COLUMN last_review_status TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN last_review_status: {}",
                e
            );
        });
    // execution_records.last_reviewed_at: 最近一次评审 spawn 时间
    db.exec("ALTER TABLE execution_records ADD COLUMN last_reviewed_at TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE execution_records ADD COLUMN last_reviewed_at: {}",
                e
            );
        });

    // ---- 索引：加速「按 parent_todo_id 查评审实例」等查询 ----
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_parent_todo_id ON todos(parent_todo_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_todo_type ON todos(todo_type)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_source_record_id ON execution_records(source_execution_record_id)")
        .await?;

    // ---- skill_invocations ----
    db.exec(
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

    // ---- 高频过滤索引 ----
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_deleted_at ON todos(deleted_at)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_status ON todos(status)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_task_id ON todos(task_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_todo_id ON execution_records(todo_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_task_id ON execution_records(task_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_pid ON execution_records(pid)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_session_id ON execution_records(session_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_status ON execution_records(status)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_todo_tags_todo_id ON todo_tags(todo_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_skill_invocations_skill_name ON skill_invocations(skill_name)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_skill_invocations_executor ON skill_invocations(executor)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_skill_invocations_todo_id ON skill_invocations(todo_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_started_at ON execution_records(started_at)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_executor ON execution_records(executor)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_model ON execution_records(model)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_todo_finished ON execution_records(todo_id, finished_at DESC)")
        .await?;

    // ---- 触发器：created_at / updated_at 自动填充为 UTC ----
    // 用 BEFORE UPDATE 而非 AFTER UPDATE：应用层显式写入 updated_at 时不要被触发器覆盖；
    // 只在 NULL/空时自动填充。
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_todos_created_at_utc AFTER INSERT ON todos
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE todos SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_todos_updated_at_utc BEFORE UPDATE OF updated_at ON todos
         WHEN new.updated_at IS NULL OR new.updated_at = ''
         BEGIN
             UPDATE todos SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_tags_created_at_utc AFTER INSERT ON tags
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE tags SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;

    // ---- agent_bots ----
    db.exec(
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

    // Migration: add config column if missing (existing databases)
    let cols = db
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
        db.exec("ALTER TABLE agent_bots ADD COLUMN config TEXT DEFAULT '{}'")
            .await?;
    }

    // ---- feishu 子表 ----
    db.exec(
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

    db.exec(
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

    // ---- feishu_messages 向后兼容列 ----
    db.exec("ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS sender_nickname TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE feishu_messages ADD COLUMN sender_nickname: {}",
                e
            );
        });
    db.exec("ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS sender_type TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE feishu_messages ADD COLUMN sender_type: {}",
                e
            );
        });
    db.exec(
        "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS is_history INTEGER DEFAULT 0",
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(
            "migration v1: ALTER TABLE feishu_messages ADD COLUMN is_history: {}",
            e
        );
    });
    db.exec("ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS fetch_time TEXT")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE feishu_messages ADD COLUMN fetch_time: {}",
                e
            );
        });
    // processed_todo_id: SQLite 3.39.0+ 支持 IF NOT EXISTS，旧版本不支持
    let add_result = db
        .exec("ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS processed_todo_id INTEGER")
        .await;
    if add_result.is_err() {
        db.exec("ALTER TABLE feishu_messages ADD COLUMN processed_todo_id INTEGER")
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "migration v1: ALTER TABLE feishu_messages ADD COLUMN processed_todo_id: {}",
                    e
                );
            });
    }
    let add_exec_result = db
        .exec(
            "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS execution_record_id INTEGER",
        )
        .await;
    if add_exec_result.is_err() {
        db.exec("ALTER TABLE feishu_messages ADD COLUMN execution_record_id INTEGER")
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "migration v1: ALTER TABLE feishu_messages ADD COLUMN execution_record_id: {}",
                    e
                );
            });
    }

    // ---- feishu_history_chats ----
    db.exec(
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

    db.exec("CREATE INDEX IF NOT EXISTS idx_feishu_messages_chat_id ON feishu_messages(chat_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_feishu_messages_created_at ON feishu_messages(created_at)")
        .await?;

    // ---- feishu_push_targets / feishu_response_config / feishu_group_whitelist ----
    db.exec(
        "CREATE TABLE IF NOT EXISTS feishu_push_targets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            bot_id INTEGER NOT NULL,
            p2p_receive_id TEXT NOT NULL DEFAULT '',
            group_chat_id TEXT NOT NULL DEFAULT '',
            receive_id_type TEXT NOT NULL DEFAULT 'open_id',
            push_level TEXT DEFAULT 'result_only',
            p2p_response_enabled INTEGER NOT NULL DEFAULT 1,
            group_response_enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT,
            updated_at TEXT,
            FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE
        )",
    )
    .await?;

    db.exec(
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
    let has_debounce: i64 = db
        .conn
        .query_one(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) FROM pragma_table_info('feishu_response_config') WHERE name='debounce_secs'",
        ))
        .await?
        .map(|r| r.try_get::<i64>("", "COUNT(*)").unwrap_or(0))
        .unwrap_or(0);
    if has_debounce == 0 {
        db.exec("ALTER TABLE feishu_response_config ADD COLUMN debounce_secs INTEGER DEFAULT 20")
            .await?;
    }

    db.exec(
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

    // ---- feishu_project_bindings ----
    // - 活跃绑定（chat_id != '__pending__'）通过 partial unique index 保证 (bot_id, chat_id) 唯一
    // - status 默认 'idle'，执行任务时更新为 'running'（执行完成后清理脚本重置为 idle）
    // - session_id：Claude Code 的会话 ID，首次执行时填充，resume 时保持不变
    // - latest_record_id：最近一次 execution_record.id，用于判断是否可 resume
    // - chat_id 特殊值 "__pending__"：Web UI 创建的待绑定记录，等待飞书侧 /bind 补齐
    // - created_at/updated_at 为 NOT NULL，业务层写入（非触发器）
    db.exec(
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
            updated_at TEXT NOT NULL
        )",
    )
    .await?;
    // 添加 enabled 字段（支持禁用而非删除绑定）
    db.exec("ALTER TABLE feishu_project_bindings ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE feishu_project_bindings ADD COLUMN enabled: {}",
                e
            );
        });
    // Partial unique index: active bindings (non-pending) must be unique per (bot_id, chat_id)
    // Pending bindings (chat_id='__pending__') excluded so one bot can have multiple pending
    db.exec("CREATE UNIQUE INDEX IF NOT EXISTS idx_feishu_bindings_active ON feishu_project_bindings(bot_id, chat_id) WHERE chat_id != '__pending__' AND enabled = 1")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_feishu_bindings_record_id ON feishu_project_bindings(latest_record_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_feishu_bindings_bot_id ON feishu_project_bindings(bot_id)")
        .await?;

    // ---- executors ----
    db.exec(
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_executors_name ON executors(name)")
        .await?;

    // Migration: add session_dir column if missing (existing databases)
    // 旧库可能没有此列；首次执行成功时这里不会报错，因为 SQLite 在 ADD COLUMN 重复列时会失败但被忽略
    db.exec("ALTER TABLE executors ADD COLUMN session_dir TEXT NOT NULL DEFAULT ''")
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "migration v1: ALTER TABLE executors ADD COLUMN session_dir: {} (column likely already exists)",
                e
            );
        });

    // ---- project_directories ----
    db.exec(
        "CREATE TABLE IF NOT EXISTS project_directories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE,
            name TEXT,
            created_at TEXT,
            updated_at TEXT
        )",
    )
    .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_project_directories_path ON project_directories(path)")
        .await?;

    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_project_directories_created_at_utc AFTER INSERT ON project_directories
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE project_directories SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;

    // ---- todo_templates ----
    db.exec(
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
    let has_is_system: i64 = db
        .conn
        .query_one(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) FROM pragma_table_info('todo_templates') WHERE name='is_system'".to_string(),
        ))
        .await?
        .map(|r| r.try_get::<i64>("", "COUNT(*)").unwrap_or(0))
        .unwrap_or(0);
    if has_is_system == 0 {
        db.exec("ALTER TABLE todo_templates ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0")
            .await?;
    }

    // Migration: add source_url and last_sync_at columns if missing (custom template subscription)
    let has_source_url: i64 = db
        .conn
        .query_one(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) FROM pragma_table_info('todo_templates') WHERE name='source_url'".to_string(),
        ))
        .await?
        .map(|r| r.try_get::<i64>("", "COUNT(*)").unwrap_or(0))
        .unwrap_or(0);
    if has_source_url == 0 {
        db.exec("ALTER TABLE todo_templates ADD COLUMN source_url TEXT")
            .await?;
    }

    let has_last_sync_at: i64 = db
        .conn
        .query_one(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) FROM pragma_table_info('todo_templates') WHERE name='last_sync_at'".to_string(),
        ))
        .await?
        .map(|r| r.try_get::<i64>("", "COUNT(*)").unwrap_or(0))
        .unwrap_or(0);
    if has_last_sync_at == 0 {
        db.exec("ALTER TABLE todo_templates ADD COLUMN last_sync_at TEXT")
            .await?;
    }

    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_todo_templates_created_at_utc AFTER INSERT ON todo_templates
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE todo_templates SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_project_directories_updated_at_utc BEFORE UPDATE ON project_directories
         WHEN new.updated_at IS NULL OR new.updated_at = ''
         BEGIN
             SELECT raise(IGNORE);
         END",
    )
    .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_executors_created_at_utc AFTER INSERT ON executors
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE executors SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_executors_updated_at_utc BEFORE UPDATE ON executors
         WHEN new.updated_at IS NULL OR new.updated_at = ''
         BEGIN
             SELECT raise(IGNORE);
         END",
    )
    .await?;

    // ---- webhooks ----
    db.exec(
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
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_webhooks_created_at_utc AFTER INSERT ON webhooks
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE webhooks SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_webhooks_updated_at_utc BEFORE UPDATE ON webhooks
         WHEN new.updated_at IS NULL OR new.updated_at = ''
         BEGIN
             SELECT raise(IGNORE);
         END",
    )
    .await?;

    db.exec(
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_webhook_records_webhook_id ON webhook_records(webhook_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_webhook_records_triggered_todo_id ON webhook_records(triggered_todo_id)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_webhook_records_created_at ON webhook_records(created_at)")
        .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_webhook_records_created_at_utc AFTER INSERT ON webhook_records
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE webhook_records SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;

    // ===== Hook System (inline on todos.hooks, no separate tables) =====
    // ---- usage_daily_stats / usage_model_breakdowns / usage_executor_daily_stats ----
    db.exec(
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_daily_stats_date ON usage_daily_stats(date)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_daily_stats_stats_type ON usage_daily_stats(stats_type)")
        .await?;

    db.exec(
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_model_breakdowns_daily_stat_id ON usage_model_breakdowns(daily_stat_id)")
        .await?;

    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_usage_daily_stats_created_at_utc AFTER INSERT ON usage_daily_stats
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE usage_daily_stats SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;

    db.exec(
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_executor_daily_stats_date ON usage_executor_daily_stats(date)")
        .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_executor_daily_stats_executor ON usage_executor_daily_stats(executor)")
        .await?;

    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_usage_executor_daily_stats_created_at_utc AFTER INSERT ON usage_executor_daily_stats
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE usage_executor_daily_stats SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;

    // ---- sync_records ----
    db.exec(
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_sync_records_created_at ON sync_records(created_at DESC)")
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// v2: 旧 todos.rating 数据合并到 execution_records.rating
// ---------------------------------------------------------------------------

/// v2 迁移：把历史 `todos.rating`（已不再使用）合并到对应 todo 最新一条
/// `execution_records.rating`，然后 DROP COLUMN。
///
/// 设计原因：评分属于执行结果而非 todo 本身。
/// - 每个 todo 取最新一条已结束的 execution_record（按 started_at desc）
/// - 同一 record 已被多次评分时跳过，避免覆盖更新的评价
/// - 失败仅 warn，不阻塞启动
pub(super) struct V2TodoRatingDropColumn;

#[async_trait]
impl Migration for V2TodoRatingDropColumn {
    fn version(&self) -> i64 {
        2
    }
    fn name(&self) -> &'static str {
        "todo_rating_to_execution_records"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        migrate_todo_rating_to_execution_records(db).await
    }
}

async fn migrate_todo_rating_to_execution_records(db: &Database) -> Result<(), sea_orm::DbErr> {
    // 检查旧列是否存在，不存在则直接跳过（DROP COLUMN 之后再次启动也是幂等的）
    let check_sql = "SELECT COUNT(*) FROM pragma_table_info('todos') WHERE name='rating'";
    let result = db
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

    let select_sql = "\
        SELECT t.id AS todo_id, t.rating AS rating, \
               (SELECT er.id FROM execution_records er \
                WHERE er.todo_id = t.id \
                ORDER BY er.started_at DESC LIMIT 1) AS latest_record_id \
        FROM todos t \
        WHERE t.rating IS NOT NULL";
    let rows = db
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
                todo_id,
                rating
            );
            continue;
        };

        // 仅在该 record 尚未评分时才写入，避免覆盖更新评价
        let update_sql = "UPDATE execution_records \
            SET rating = $1 \
            WHERE id = $2 AND rating IS NULL";
        let res = db
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

    // 移除旧列。注意：必须把错误冒泡给 runner —— 旧实现 `if let Err ... return Ok(())`
    // 会被 runner 记录为「已应用」，但其实数据已迁移、列未删，schema 处于不一致状态。
    // 用 `?` 让 daemon 启动失败，下次启动时 `run_migrations` 会跳过已迁移的数据行
    // （`SELECT ... WHERE rating IS NOT NULL` 找不到记录，但 `todos.rating` 列还在，
    // 这时 v2 的 UPDATE 会再次执行、空跑），最终 DROP COLUMN 也会再次尝试。
    //
    // 用 `map_err` 在冒泡前先记一条 `tracing::error!`，把「在 V2 DROP COLUMN todos.rating
    // 时失败」这个上下文带上 —— 否则 operator 只看到 sea_orm 序列化出来的 "Failed to
    // execute statement: ..."，排查时不知道是哪条 DDL 失败。
    db.exec("ALTER TABLE todos DROP COLUMN rating")
        .await
        .map_err(|e| {
            tracing::error!("V2 DROP COLUMN todos.rating failed: {}", e);
            e
        })?;

    tracing::info!(
        "Migrated {} todo ratings to execution_records, dropped todos.rating",
        migrated
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// v3: execution_records.logs -> execution_logs 表
// ---------------------------------------------------------------------------

/// v3 迁移：把 `execution_records.logs` 旧字段数据转移到 `execution_logs` 表，
/// 并 DROP 旧字段。
///
/// 设计原因：logs 单独成表后支持分页加载，避免单条 record 的 logs TEXT 字段过大。
pub(super) struct V3LogsToExecutionLogs;

#[async_trait]
impl Migration for V3LogsToExecutionLogs {
    fn version(&self) -> i64 {
        3
    }
    fn name(&self) -> &'static str {
        "logs_to_execution_logs"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        migrate_logs_to_execution_logs(db).await
    }
}

async fn migrate_logs_to_execution_logs(db: &Database) -> Result<(), sea_orm::DbErr> {
    // 检查旧列是否存在，不存在则直接跳过（DROP COLUMN 之后再次启动也是幂等的）
    let check_sql = "SELECT COUNT(*) FROM pragma_table_info('execution_records') WHERE name='logs'";
    let result = db
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

    tracing::info!("Migrating old logs column to execution_logs table...");

    let select_sql = "SELECT id, logs FROM execution_records \
        WHERE logs IS NOT NULL AND logs != '' AND logs != '[]' \
        AND id NOT IN (SELECT DISTINCT record_id FROM execution_logs)";
    let rows = db
        .conn
        .query_all(Statement::from_string(DbBackend::Sqlite, select_sql.to_string()))
        .await?;

    let mut migrated = 0u64;
    let mut failed = 0u64;
    for row in rows {
        let id: i64 = row.try_get_by("id")?;
        let logs_json: String = row.try_get_by("logs")?;
        if !logs_json.is_empty() && logs_json != "[]" {
            if let Err(e) = db.insert_execution_logs(id, &logs_json).await {
                tracing::warn!("Failed to migrate logs for record {}: {}", id, e);
                failed += 1;
            } else {
                migrated += 1;
            }
        }
    }

    // 有任意记录迁移失败则不删除旧列，保留数据等待下次重试
    // 注意：必须返回 Err 让 runner 不要把本次标记为已应用 —— 旧实现 `return Ok(())`
    // 会让 schema_version 记录 v3 已应用，下次启动跳过，但 `logs` 列仍存在、数据不完整。
    if failed > 0 {
        tracing::warn!(
            "Logs migration incomplete: {} succeeded, {} failed. Will retry next start.",
            migrated,
            failed
        );
        return Err(sea_orm::DbErr::Custom(format!(
            "V3 logs migration partial: {}/{} failed",
            failed,
            migrated + failed
        )));
    }

    db.exec("ALTER TABLE execution_records DROP COLUMN logs").await?;
    tracing::info!(
        "Migrated {} execution records, dropped logs column",
        migrated
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// v4: 飞书子表添加 ON DELETE CASCADE
// ---------------------------------------------------------------------------

/// v4 迁移：为飞书子表添加 ON DELETE CASCADE 外键约束。
///
/// SQLite 不支持 ALTER TABLE 修改外键约束，需要重建表
/// （创建新表→复制数据→删除旧表→重命名）。每张表独立检查，只有自身缺少 CASCADE
/// 才重建；整个迁移包在事务中。
pub(super) struct V4FeishuFkCascade;

#[async_trait]
impl Migration for V4FeishuFkCascade {
    fn version(&self) -> i64 {
        4
    }
    fn name(&self) -> &'static str {
        "feishu_fk_cascade"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        migrate_feishu_fk_cascade(db).await
    }
}

/// 检查表的外键是否缺少 ON DELETE CASCADE（返回 true 表示需要迁移）
///
/// 接受 `&impl ConnectionTrait` 而非 `&Database`，这样可以被 `DatabaseConnection`
/// 或 `DatabaseTransaction` 共同使用 — V4 迁移需要把整组 rebuild 放在一个事务里。
///
/// 使用 `PRAGMA foreign_key_list(table)` 精确解析外键元组，**避免**在 `sqlite_master.sql`
/// 文本上做 `contains("ON DELETE CASCADE")` 子串匹配 —— 后者会把
/// `CHECK (col != 'ON DELETE CASCADE')`、注释、视图 DDL 等字符串误判为已迁移，
/// 且无法区分「多个外键中只有一个缺 CASCADE」的情况。
async fn needs_fk_migration<C: ConnectionTrait>(
    conn: &C,
    table: &str,
) -> Result<bool, sea_orm::DbErr> {
    // 表名白名单校验：函数签名是 `&str`，目前唯一调用方传的是 hardcoded 数组，但
    // `format!` 直接拼接进 SQL/PRAGMA 字符串，存在注入风险面。用 `debug_assert!`
    // 在 debug build 立刻拒绝任何非 `[A-Za-z0-9_]` 的字符 —— 与 PR #476 daemon-redeploy
    // 的 whitelist 模式一致。release build 下保持零开销（assertion 被消除）。
    debug_assert!(
        !table.is_empty()
            && table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "needs_fk_migration: invalid table name {table:?} (must match [A-Za-z0-9_]+)"
    );
    let sql = format!("SELECT sql FROM sqlite_master WHERE type='table' AND name='{}'", table);
    let result = conn
        .query_one(Statement::from_string(DbBackend::Sqlite, sql))
        .await?;
    if result.is_none() {
        // 表不存在，CREATE TABLE IF NOT EXISTS 会创建正确的 schema
        return Ok(false);
    }
    // 解析 foreign_key_list：每行对应一个 FK 列定义。
    // 至少有一个 FK 的 `on_delete` 不是 CASCADE，就视为需要迁移。
    let fk_sql = format!("PRAGMA foreign_key_list('{}')", table);
    let fk_rows = conn
        .query_all(Statement::from_string(DbBackend::Sqlite, fk_sql))
        .await?;
    if fk_rows.is_empty() {
        // 表上没有外键，无需迁移
        return Ok(false);
    }
    for row in fk_rows {
        // foreign_key_list 列：id, seq, table, from, to, on_update, on_delete, match
        let on_delete: String = row.try_get_by("on_delete")?;
        if on_delete != "CASCADE" {
            return Ok(true);
        }
    }
    // 全部 FK 都是 CASCADE，已经是新 schema
    Ok(false)
}

/// 在指定连接上执行 raw SQL（包成 Result<(), DbErr>）。
///
/// 之所以不直接调用 `Database::exec` — 它的实现是 `&self.conn.execute(...)`，
/// 走的是连接池（max_connections=10）。在事务里必须把每个 DDL 钉在同一条连接上，
/// 否则 BEGIN/ALTER/COMMIT 会落在 3 条不同连接上，事务根本不原子。
async fn exec_on_conn<C: ConnectionTrait>(conn: &C, sql: &str) -> Result<(), sea_orm::DbErr> {
    conn.execute(Statement::from_string(DbBackend::Sqlite, sql.to_string()))
        .await
        .map(|_| ())
}

/// 重建表以添加 ON DELETE CASCADE 外键约束
/// SQLite 标准迁移流程：新建→复制→删除→重命名
///
/// 所有 DDL 必须在调用方传入的同一条连接上执行（通常是事务），否则 PRAGMA 与
/// ALTER 之间会因连接池切换而失去原子性。
async fn rebuild_table_with_cascade<C: ConnectionTrait>(
    conn: &C,
    table: &str,
    columns: &str,
) -> Result<(), sea_orm::DbErr> {
    let tmp = format!("{}_new", table);
    tracing::info!("Rebuilding table {} to add ON DELETE CASCADE...", table);

    // 暂时关闭外键检查以避免重建过程中的约束冲突
    exec_on_conn(conn, "PRAGMA foreign_keys = OFF").await?;

    // 清理上次中断可能残留的临时表
    exec_on_conn(conn, &format!("DROP TABLE IF EXISTS {}", tmp)).await?;

    // 创建新表
    exec_on_conn(conn, &format!("CREATE TABLE IF NOT EXISTS {} ({})", tmp, columns)).await?;

    // 获取旧表列名列表，用于安全的数据复制
    let col_rows = conn
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
    exec_on_conn(
        conn,
        &format!(
            "INSERT INTO {} ({}) SELECT {} FROM {}",
            tmp, cols_str, cols_str, table
        ),
    )
    .await?;

    // 删除旧表
    exec_on_conn(conn, &format!("DROP TABLE {}", table)).await?;

    // 重命名新表
    exec_on_conn(conn, &format!("ALTER TABLE {} RENAME TO {}", tmp, table)).await?;

    // 恢复外键检查
    exec_on_conn(conn, "PRAGMA foreign_keys = ON").await?;
    Ok(())
}

async fn migrate_feishu_fk_cascade(db: &Database) -> Result<(), sea_orm::DbErr> {
    use sea_orm::TransactionTrait;

    // 收集需要迁移的表
    let tables_to_migrate = [
        ("feishu_homes", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, user_open_id TEXT NOT NULL, chat_id TEXT, receive_id TEXT NOT NULL, receive_id_type TEXT NOT NULL, created_at TEXT, updated_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE, UNIQUE(bot_id, user_open_id)"),
        ("feishu_messages", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, message_id TEXT NOT NULL UNIQUE, chat_id TEXT NOT NULL, chat_type TEXT NOT NULL, sender_open_id TEXT NOT NULL, sender_nickname TEXT, sender_type TEXT, content TEXT, msg_type TEXT NOT NULL DEFAULT 'text', is_mention INTEGER DEFAULT 0, processed INTEGER DEFAULT 0, is_history INTEGER DEFAULT 0, fetch_time TEXT, created_at TEXT, processed_todo_id INTEGER, execution_record_id INTEGER, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE"),
        ("feishu_history_chats", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, chat_id TEXT NOT NULL, chat_name TEXT, enabled INTEGER DEFAULT 1, last_fetch_time TEXT, polling_interval_secs INTEGER DEFAULT 60, created_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE, UNIQUE(bot_id, chat_id)"),
        ("feishu_push_targets", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, p2p_receive_id TEXT NOT NULL DEFAULT '', group_chat_id TEXT NOT NULL DEFAULT '', receive_id_type TEXT NOT NULL DEFAULT 'open_id', push_level TEXT DEFAULT 'result_only', p2p_response_enabled INTEGER NOT NULL DEFAULT 1, group_response_enabled INTEGER NOT NULL DEFAULT 1, created_at TEXT, updated_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE"),
        ("feishu_response_config", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, target_type TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, debounce_secs INTEGER DEFAULT 20, created_at TEXT, updated_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE, UNIQUE(bot_id, target_type)"),
        ("feishu_group_whitelist", "id INTEGER PRIMARY KEY AUTOINCREMENT, bot_id INTEGER NOT NULL, sender_open_id TEXT NOT NULL, sender_name TEXT, created_at TEXT, FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE, UNIQUE(bot_id, sender_open_id)"),
    ];

    // 探测阶段：先在主连接上确定是否真要迁移（避免无谓地开事务）。
    //
    // 设计取舍 (PR #539 push-3 review LOW-3): 理论上 probe 与后续 `db.conn.begin()`
    // 会从连接池各拿一条连接，构成 TOCTOU 窗口（probe 后另一连接上对 schema 的修改
    // 可能让 probe 结果过期）。但 V4 是「schema rebuild」类迁移，daemon 启动早期 + 几乎
    // 无并发写入 + SQLite 单写者串行化，实际不可触发。把 probe-then-txn 拆成两步是
    // 有意的 (probe 失败 → 不开事务 → 不污染 connection pool)，不是疏漏。如果未来
    // 出现真并发修改 schema 的场景，应该把 probe 也搬到 txn 上做，而不是去掉「无谓不开
    // 事务」的早退优化。
    let mut needs_any = false;
    for (table, _ddl) in &tables_to_migrate {
        if needs_fk_migration(&db.conn, table).await? {
            needs_any = true;
            break;
        }
    }
    if !needs_any {
        return Ok(());
    }

    tracing::info!("Migrating feishu tables to add ON DELETE CASCADE...");

    // 关键：必须把整组 rebuild 包在一条连接的事务里。
    // 旧实现用 raw `BEGIN` / `COMMIT` 是错的 — `Database::exec` 走的是 sqlx 连接池
    // （max_connections=10，PR #497 调整后），每次 execute 都可能拿到不同的连接，
    // BEGIN/ALTER/COMMIT 落在 3 条不同连接上 → 事务完全失去原子性。
    // 用 `conn.begin()` 把整组 DDL 钉在同一条连接上，任一步失败都能回滚。
    let txn = db.conn.begin().await?;

    for (table, ddl) in &tables_to_migrate {
        if needs_fk_migration(&txn, table).await? {
            rebuild_table_with_cascade(&txn, table, ddl).await?;
        }
    }

    // 重建索引
    exec_on_conn(
        &txn,
        "CREATE INDEX IF NOT EXISTS idx_feishu_messages_chat_id ON feishu_messages(chat_id)",
    )
    .await?;
    exec_on_conn(
        &txn,
        "CREATE INDEX IF NOT EXISTS idx_feishu_messages_created_at ON feishu_messages(created_at)",
    )
    .await?;

    txn.commit().await?;

    tracing::info!("Feishu FK cascade migration completed.");
    Ok(())
}

// ---------------------------------------------------------------------------
// 工具函数（被 mod.rs 中的 run_migrations 调用）
// ---------------------------------------------------------------------------

/// 已应用迁移的版本号集合，从 `schema_version` 表读取。
pub(super) async fn read_applied_versions(
    db: &Database,
) -> Result<HashSet<i64>, sea_orm::DbErr> {
    let stmt = Statement::from_string(
        DbBackend::Sqlite,
        "SELECT version FROM schema_version".to_string(),
    );
    let rows = db.conn.query_all(stmt).await?;
    let mut set = HashSet::new();
    for row in rows {
        if let Ok(v) = row.try_get_by_index::<i64>(0) {
            set.insert(v);
        }
    }
    Ok(set)
}

// ---------------------------------------------------------------------------
// Unit tests for `needs_fk_migration` (V4 feishu_fk_cascade)
// ---------------------------------------------------------------------------
//
// `needs_fk_migration` 之前的 4 个分支（表不存在 / 无 FK / 全部 CASCADE / 任意非 CASCADE /
// 混合）原本 0 个测试覆盖 —— 下次有人想换回 `sqlite_master.sql.contains(...)` 时没有回归网。
// 这里的 5 个 fixture-driven test 把这 4 个分支全部钉死，且最后一个 test 用「混合 FK」复现
// 旧实现 `contains()` 根本区分不了的场景，确保 PR #539 push 3 的 `PRAGMA foreign_key_list`
// 改写不会被无声地回退。

#[cfg(test)]
mod needs_fk_migration_tests {
    use super::*;

    async fn fresh_db() -> Database {
        // `Database::new(":memory:")` 会跑 v1 init + seed_default_templates，
        // 但每张表用唯一名字避免冲突；`:memory:` 模式每个测试一个独立 ephemeral store。
        Database::new(":memory:")
            .await
            .expect(":memory: db must open")
    }

    async fn exec(db: &Database, sql: &str) {
        db.exec(sql).await.expect("test DDL must succeed");
    }

    /// 分支 1: 表不存在 → `false`
    /// (CREATE TABLE IF NOT EXISTS 阶段会建出正确 schema,无需迁移)
    #[tokio::test]
    async fn needs_fk_migration_returns_false_when_table_missing() {
        let db = fresh_db().await;
        let needs = needs_fk_migration(&db.conn, "no_such_table_for_needs_fk")
            .await
            .expect("probe must succeed");
        assert!(
            !needs,
            "non-existent table must not require FK migration (CREATE TABLE IF NOT EXISTS will set the correct schema)"
        );
    }

    /// 分支 2: 表存在但无 FK → `false`
    #[tokio::test]
    async fn needs_fk_migration_returns_false_when_no_foreign_keys() {
        let db = fresh_db().await;
        exec(
            &db,
            "CREATE TABLE nfm_plain (id INTEGER PRIMARY KEY, name TEXT)",
        )
        .await;
        let needs = needs_fk_migration(&db.conn, "nfm_plain")
            .await
            .expect("probe must succeed");
        assert!(
            !needs,
            "table without any FK must not require FK migration"
        );
    }

    /// 分支 3: 全部 FK 都是 CASCADE → `false` (已经是新 schema)
    #[tokio::test]
    async fn needs_fk_migration_returns_false_when_all_fks_cascade() {
        let db = fresh_db().await;
        exec(
            &db,
            "CREATE TABLE nfm_parent_all (id INTEGER PRIMARY KEY)",
        )
        .await;
        exec(
            &db,
            "CREATE TABLE nfm_child_all (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER NOT NULL,
                FOREIGN KEY (parent_id) REFERENCES nfm_parent_all(id) ON DELETE CASCADE
            )",
        )
        .await;
        let needs = needs_fk_migration(&db.conn, "nfm_child_all")
            .await
            .expect("probe must succeed");
        assert!(
            !needs,
            "all FKs already ON DELETE CASCADE → migration not required"
        );
    }

    /// 分支 4: 至少一个 FK `on_delete != "CASCADE"` → `true`
    /// 用 `NO ACTION` (SQLite 默认) 这个最常见的非 CASCADE 形式。
    #[tokio::test]
    async fn needs_fk_migration_returns_true_when_one_fk_not_cascade() {
        let db = fresh_db().await;
        exec(
            &db,
            "CREATE TABLE nfm_parent_one (id INTEGER PRIMARY KEY)",
        )
        .await;
        exec(
            &db,
            "CREATE TABLE nfm_child_one (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER NOT NULL,
                FOREIGN KEY (parent_id) REFERENCES nfm_parent_one(id) ON DELETE NO ACTION
            )",
        )
        .await;
        let needs = needs_fk_migration(&db.conn, "nfm_child_one")
            .await
            .expect("probe must succeed");
        assert!(
            needs,
            "single non-CASCADE FK (NO ACTION) must require migration"
        );
    }

    /// 分支 5: 多个 FK 混合 (部分 CASCADE + 部分非 CASCADE) → `true`
    /// 这是旧 `sqlite_master.sql.contains("ON DELETE CASCADE")` 子串匹配**根本区分不了**的场景：
    /// `contains` 看到 "ON DELETE CASCADE" 字符串就直接判 false，但表里还有 RESTRICT FK 没改。
    /// 现在的 `PRAGMA foreign_key_list` 逐行解析能正确返回 `true`。
    #[tokio::test]
    async fn needs_fk_migration_returns_true_when_fks_mixed() {
        let db = fresh_db().await;
        exec(&db, "CREATE TABLE nfm_parent_a (id INTEGER PRIMARY KEY)").await;
        exec(&db, "CREATE TABLE nfm_parent_b (id INTEGER PRIMARY KEY)").await;
        exec(
            &db,
            "CREATE TABLE nfm_child_mixed (
                id INTEGER PRIMARY KEY,
                a_id INTEGER NOT NULL,
                b_id INTEGER NOT NULL,
                FOREIGN KEY (a_id) REFERENCES nfm_parent_a(id) ON DELETE CASCADE,
                FOREIGN KEY (b_id) REFERENCES nfm_parent_b(id) ON DELETE RESTRICT
            )",
        )
        .await;
        let needs = needs_fk_migration(&db.conn, "nfm_child_mixed")
            .await
            .expect("probe must succeed");
        assert!(
            needs,
            "mixed FKs (CASCADE + RESTRICT) → at least one needs migration, must return true"
        );
    }

    /// 安全网: `debug_assert!` 白名单拒绝非 `[A-Za-z0-9_]+` 的表名,
    /// 防止 `format!` 拼接 SQL 时被注入 (虽然当前唯一调用方传的是 hardcoded 数组,
    /// 但 `pub(super)` 函数签名不约束调用方)。
    /// 注意 `debug_assert!` 只在 debug build 触发 — `cargo test` 默认 debug,所以这里有效。
    #[tokio::test]
    #[should_panic(expected = "invalid table name")]
    async fn needs_fk_migration_rejects_non_whitelisted_table_name() {
        let db = fresh_db().await;
        // 单引号 + SQL 注释符的经典注入 payload
        let _ = needs_fk_migration(&db.conn, "evil'; DROP TABLE x; --").await;
    }
}