//! Schema migration system.
//!
//! Why this exists (issue #498):
//! - Previously `db::init_tables()` re-ran 100+ DDL statements on every daemon start.
//! - Even with `IF NOT EXISTS`, SQLite still has to parse, plan, and check schema metadata
//!   for each statement, producing visible cold-start latency.
//! - Errors from `ALTER TABLE ... ADD COLUMN` (column already exists) were silently
//!   swallowed via `.ok()`, hiding real migration failures.
//!
//! How it works:
//! - A `schema_version(version INTEGER PRIMARY KEY, name TEXT, applied_at TEXT)` table
//!   tracks the highest applied migration version.
//! - On startup we read the current version and only run DDL for migrations with
//!   `version > current`. This means subsequent startups pay ~1 SELECT instead of
//!   100+ statements.
//! - Each migration's statements are still idempotent (`IF NOT EXISTS`, etc.) so a
//!   legacy database upgraded to this code will run the DDL once as no-ops and
//!   then be marked as up-to-date.
//! - Per-statement errors are logged at `debug` level (idempotent re-runs on
//!   legacy DBs are expected to fail-and-skip), but uncaught errors during the
//!   version-record insert will fail the migration.
//!
//! Adding a new migration:
//! 1. Bump `ALL_MIGRATIONS` with a new entry whose `version` is greater than the
//!    current maximum.
//! 2. The runner will apply it on next startup and record the new version.

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

use crate::db::Database;

/// Name of the meta table that records the highest applied migration version.
pub const SCHEMA_VERSION_TABLE: &str = "schema_version";

/// A versioned, named bundle of DDL statements.
///
/// Each statement should be idempotent (use `IF NOT EXISTS`, etc.) so that
/// re-running on a partially-upgraded database is safe.
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    /// Human-readable description used in startup logs.
    pub description: &'static str,
    /// Ordered DDL statements to apply for this migration.
    pub statements: &'static [&'static str],
}

/// All schema migrations in ascending version order.
///
/// The runner applies every entry whose `version` is greater than the value
/// stored in `schema_version`. New entries must be APPENDED (never reorder,
/// never reuse a version number).
pub const ALL_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        description:
            "Initial schema: todos, tags, todo_tags, execution_records, execution_logs, \
             skill_invocations, agent_bots, feishu_*, executors, project_directories, \
             todo_templates, webhooks, webhook_records, usage_*, sync_records + all \
             indexes/triggers and backward-compat ALTER TABLE migrations",
        statements: INITIAL_SCHEMA_STATEMENTS,
    },
    Migration {
        version: 2,
        name: "loop_studio",
        description:
            "Loop Studio: loops, loop_triggers, loop_stages, \
             loop_executions, loop_stage_executions + indexes/triggers",
        statements: LOOP_STUDIO_STATEMENTS,
    },
    Migration {
        version: 3,
        name: "todo_kind",
        description:
            "区分事项 vs 环节: todos 加 kind 列 ('item'|'step'), 默认 'item'; \
             回填 loop_stages 引用的 todo 为 'step'。这是底层抽象, 不破坏现有数据。",
        statements: TODO_KIND_STATEMENTS,
    },
    Migration {
        version: 9,
        name: "independent_steps",
        description:
            "环节独立为 steps 表: 创建新表, 从 todos 复制数据; \
             后续 promote 写入 steps 表而非改 kind。",
        statements: INDEPENDENT_STEPS_STATEMENTS,
    },
];

/// All DDL for the initial schema (extracted from the previous monolithic
/// `init_tables()` function).
///
/// Per-statement comments preserve the rationale from the original code; for
/// the high-level design see the module-level docs.
const INITIAL_SCHEMA_STATEMENTS: &[&str] = &[
    // ===== Todos =====
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
    // ===== Tags =====
    "CREATE TABLE IF NOT EXISTS tags (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE,
        color TEXT DEFAULT '#1890ff',
        created_at TEXT
    )",
    // ===== Todo <-> Tag join =====
    "CREATE TABLE IF NOT EXISTS todo_tags (
        todo_id INTEGER,
        tag_id INTEGER,
        PRIMARY KEY (todo_id, tag_id),
        FOREIGN KEY (todo_id) REFERENCES todos(id) ON DELETE CASCADE,
        FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
    )",
    // ===== Execution records =====
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
    // ===== Execution logs (per-line, supports paginated loading) =====
    "CREATE TABLE IF NOT EXISTS execution_logs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        record_id INTEGER NOT NULL,
        timestamp TEXT NOT NULL,
        log_type TEXT NOT NULL DEFAULT 'info',
        content TEXT NOT NULL DEFAULT '',
        metadata TEXT DEFAULT '{}',
        FOREIGN KEY (record_id) REFERENCES execution_records(id) ON DELETE CASCADE
    )",
    "CREATE INDEX IF NOT EXISTS idx_execution_logs_record ON execution_logs(record_id)",
    // ===== Skill invocations =====
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
    // ===== Frequent-filter indexes =====
    "CREATE INDEX IF NOT EXISTS idx_todos_deleted_at ON todos(deleted_at)",
    "CREATE INDEX IF NOT EXISTS idx_todos_status ON todos(status)",
    "CREATE INDEX IF NOT EXISTS idx_todos_task_id ON todos(task_id)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_todo_id ON execution_records(todo_id)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_task_id ON execution_records(task_id)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_pid ON execution_records(pid)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_session_id ON execution_records(session_id)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_status ON execution_records(status)",
    "CREATE INDEX IF NOT EXISTS idx_todo_tags_todo_id ON todo_tags(todo_id)",
    "CREATE INDEX IF NOT EXISTS idx_skill_invocations_skill_name ON skill_invocations(skill_name)",
    "CREATE INDEX IF NOT EXISTS idx_skill_invocations_executor ON skill_invocations(executor)",
    "CREATE INDEX IF NOT EXISTS idx_skill_invocations_todo_id ON skill_invocations(todo_id)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_started_at ON execution_records(started_at)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_executor ON execution_records(executor)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_model ON execution_records(model)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_todo_finished ON execution_records(todo_id, finished_at DESC)",
    // ===== Triggers for created_at/updated_at (UTC) =====
    "CREATE TRIGGER IF NOT EXISTS set_todos_created_at_utc AFTER INSERT ON todos
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE todos SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // BEFORE UPDATE so application-set values are not overwritten; only auto-fill
    // when value is NULL/empty.
    "CREATE TRIGGER IF NOT EXISTS set_todos_updated_at_utc BEFORE UPDATE OF updated_at ON todos
     WHEN new.updated_at IS NULL OR new.updated_at = ''
     BEGIN
         UPDATE todos SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    "CREATE TRIGGER IF NOT EXISTS set_tags_created_at_utc AFTER INSERT ON tags
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE tags SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // ===== Agent bots (e.g., Feishu) =====
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
    // ===== Feishu homes =====
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
    // ===== Feishu messages =====
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
    "CREATE INDEX IF NOT EXISTS idx_feishu_messages_chat_id ON feishu_messages(chat_id)",
    "CREATE INDEX IF NOT EXISTS idx_feishu_messages_created_at ON feishu_messages(created_at)",
    // ===== Feishu history chats =====
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
    // ===== Feishu push targets =====
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
    // ===== Feishu response config =====
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
    // ===== Feishu group whitelist =====
    "CREATE TABLE IF NOT EXISTS feishu_group_whitelist (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        bot_id INTEGER NOT NULL,
        sender_open_id TEXT NOT NULL,
        sender_name TEXT,
        created_at TEXT,
        FOREIGN KEY (bot_id) REFERENCES agent_bots(id) ON DELETE CASCADE,
        UNIQUE(bot_id, sender_open_id)
    )",
    // ===== Feishu project bindings =====
    // 活跃绑定 (chat_id != '__pending__') 通过 partial unique index 保证 (bot_id, chat_id) 唯一
    // '__pending__' 是 Web UI 创建的待绑定记录,等待飞书侧 /bind 补齐
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
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_feishu_bindings_active ON feishu_project_bindings(bot_id, chat_id) WHERE chat_id != '__pending__' AND enabled = 1",
    "CREATE INDEX IF NOT EXISTS idx_feishu_bindings_record_id ON feishu_project_bindings(latest_record_id)",
    "CREATE INDEX IF NOT EXISTS idx_feishu_bindings_bot_id ON feishu_project_bindings(bot_id)",
    // ===== Executors (config moved from yaml to db) =====
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
    "CREATE INDEX IF NOT EXISTS idx_executors_name ON executors(name)",
    // ===== Project directories =====
    "CREATE TABLE IF NOT EXISTS project_directories (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        path TEXT NOT NULL UNIQUE,
        name TEXT,
        created_at TEXT,
        updated_at TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_project_directories_path ON project_directories(path)",
    "CREATE TRIGGER IF NOT EXISTS set_project_directories_created_at_utc AFTER INSERT ON project_directories
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE project_directories SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // ===== Todo templates =====
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
    "CREATE TRIGGER IF NOT EXISTS set_todo_templates_created_at_utc AFTER INSERT ON todo_templates
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE todo_templates SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    "CREATE TRIGGER IF NOT EXISTS set_project_directories_updated_at_utc BEFORE UPDATE ON project_directories
     WHEN new.updated_at IS NULL OR new.updated_at = ''
     BEGIN
         SELECT raise(IGNORE);
     END",
    "CREATE TRIGGER IF NOT EXISTS set_executors_created_at_utc AFTER INSERT ON executors
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE executors SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    "CREATE TRIGGER IF NOT EXISTS set_executors_updated_at_utc BEFORE UPDATE ON executors
     WHEN new.updated_at IS NULL OR new.updated_at = ''
     BEGIN
         SELECT raise(IGNORE);
     END",
    // ===== Webhooks =====
    "CREATE TABLE IF NOT EXISTS webhooks (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        default_todo_id INTEGER,
        created_at TEXT,
        updated_at TEXT
    )",
    "CREATE TRIGGER IF NOT EXISTS set_webhooks_created_at_utc AFTER INSERT ON webhooks
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE webhooks SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    "CREATE TRIGGER IF NOT EXISTS set_webhooks_updated_at_utc BEFORE UPDATE ON webhooks
     WHEN new.updated_at IS NULL OR new.updated_at = ''
     BEGIN
         SELECT raise(IGNORE);
     END",
    // ===== Webhook records =====
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
    "CREATE INDEX IF NOT EXISTS idx_webhook_records_webhook_id ON webhook_records(webhook_id)",
    "CREATE INDEX IF NOT EXISTS idx_webhook_records_triggered_todo_id ON webhook_records(triggered_todo_id)",
    "CREATE INDEX IF NOT EXISTS idx_webhook_records_created_at ON webhook_records(created_at)",
    "CREATE TRIGGER IF NOT EXISTS set_webhook_records_created_at_utc AFTER INSERT ON webhook_records
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE webhook_records SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // ===== Usage stats =====
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
    "CREATE INDEX IF NOT EXISTS idx_usage_daily_stats_date ON usage_daily_stats(date)",
    "CREATE INDEX IF NOT EXISTS idx_usage_daily_stats_stats_type ON usage_daily_stats(stats_type)",
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
    "CREATE INDEX IF NOT EXISTS idx_usage_model_breakdowns_daily_stat_id ON usage_model_breakdowns(daily_stat_id)",
    "CREATE TRIGGER IF NOT EXISTS set_usage_daily_stats_created_at_utc AFTER INSERT ON usage_daily_stats
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE usage_daily_stats SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
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
    "CREATE INDEX IF NOT EXISTS idx_usage_executor_daily_stats_date ON usage_executor_daily_stats(date)",
    "CREATE INDEX IF NOT EXISTS idx_usage_executor_daily_stats_executor ON usage_executor_daily_stats(executor)",
    "CREATE TRIGGER IF NOT EXISTS set_usage_executor_daily_stats_created_at_utc AFTER INSERT ON usage_executor_daily_stats
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE usage_executor_daily_stats SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // ===== Sync records =====
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
    "CREATE INDEX IF NOT EXISTS idx_sync_records_created_at ON sync_records(created_at DESC)",
    // ===== Backward-compat ALTER TABLE migrations (issue #498) =====
    // The previous code used `.ok()` to silently swallow "column already exists" errors.
    // We keep the same idempotency contract (errors are now logged at debug level
    // rather than swallowed) so legacy DBs that pre-date these columns still work.
    "ALTER TABLE execution_records ADD COLUMN pid INTEGER",
    "ALTER TABLE execution_records ADD COLUMN task_id TEXT",
    "ALTER TABLE execution_records ADD COLUMN session_id TEXT",
    "ALTER TABLE todos ADD COLUMN workspace TEXT",
    "ALTER TABLE todos ADD COLUMN worktree_enabled INTEGER DEFAULT 0",
    "ALTER TABLE execution_records ADD COLUMN todo_progress TEXT",
    "ALTER TABLE execution_records ADD COLUMN execution_stats TEXT",
    "ALTER TABLE execution_records ADD COLUMN resume_message TEXT",
    "ALTER TABLE execution_records ADD COLUMN source_todo_id INTEGER",
    "ALTER TABLE execution_records ADD COLUMN source_todo_title TEXT",
    "ALTER TABLE execution_records ADD COLUMN source_hook_id INTEGER",
    "ALTER TABLE todos ADD COLUMN scheduler_timezone TEXT",
    "ALTER TABLE todos ADD COLUMN hooks TEXT",
    "ALTER TABLE todos ADD COLUMN acceptance_criteria TEXT",
    "ALTER TABLE execution_records ADD COLUMN rating INTEGER",
    "ALTER TABLE todos ADD COLUMN todo_type INTEGER DEFAULT 0",
    "ALTER TABLE todos ADD COLUMN parent_todo_id INTEGER",
    "ALTER TABLE todos ADD COLUMN auto_review_enabled INTEGER DEFAULT 1",
    "ALTER TABLE execution_records ADD COLUMN source_execution_record_id INTEGER",
    "ALTER TABLE execution_records ADD COLUMN last_review_status TEXT",
    "ALTER TABLE execution_records ADD COLUMN last_reviewed_at TEXT",
    "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS sender_nickname TEXT",
    "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS sender_type TEXT",
    "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS is_history INTEGER DEFAULT 0",
    "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS fetch_time TEXT",
    "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS processed_todo_id INTEGER",
    "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS execution_record_id INTEGER",
    "ALTER TABLE feishu_response_config ADD COLUMN debounce_secs INTEGER DEFAULT 20",
    "ALTER TABLE feishu_project_bindings ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1",
    "ALTER TABLE executors ADD COLUMN session_dir TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE todo_templates ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE todo_templates ADD COLUMN source_url TEXT",
    "ALTER TABLE todo_templates ADD COLUMN last_sync_at TEXT",
    "ALTER TABLE agent_bots ADD COLUMN config TEXT DEFAULT '{}'",
    "CREATE INDEX IF NOT EXISTS idx_todos_parent_todo_id ON todos(parent_todo_id)",
    "CREATE INDEX IF NOT EXISTS idx_todos_todo_type ON todos(todo_type)",
    "CREATE INDEX IF NOT EXISTS idx_execution_records_source_record_id ON execution_records(source_execution_record_id)",
];

// ===== Migration v2: Loop Studio =====
//
// Why: 用户希望在 todo + 内联 hooks 之上增加一个「环路」编排层，把"一组 todo +
// 触发条件 + 前后 hook + 定时"统一管理为一个场景级自动化能力。一个 Loop 包含:
// - 基本信息 (name/description/product/repo/branch/color/status)
// - 多个触发器 (manual/cron/webhook/feishu_message/feishu_command/todo_completed/tag_added)
// - 多个有序阶段 (每个阶段绑定一个 todo，顺序执行，含 rating 闸门)
// - 多个 hook (pre_loop / post_loop / pre_stage / post_stage)
// - 每次执行的运行记录 (loop_executions + loop_stage_executions)
//
// 与现有 todos.hooks 字段的关系：并存策略。todo 自己的内联 hooks 仍适用于
// 单 todo 的链式场景；loop_hooks 是 loop 级的编排视图，二者职责分离。
const LOOP_STUDIO_STATEMENTS: &[&str] = &[
    // ===== loops: 环路主表 =====
    // status 三态: draft(草稿)/enabled(启用)/paused(暂停),与 CODELOOP 视觉一致
    "CREATE TABLE IF NOT EXISTS loops (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        description TEXT DEFAULT '',
        product TEXT DEFAULT '',
        repo TEXT DEFAULT '',
        branch TEXT DEFAULT '',
        status TEXT NOT NULL DEFAULT 'draft',
        color TEXT DEFAULT '#722ed1',
        icon TEXT DEFAULT 'loop',
        created_at TEXT,
        updated_at TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_loops_status ON loops(status)",
    "CREATE INDEX IF NOT EXISTS idx_loops_updated_at ON loops(updated_at DESC)",
    // 自动维护 created_at / updated_at
    "CREATE TRIGGER IF NOT EXISTS set_loops_created_at_utc AFTER INSERT ON loops
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE loops SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    "CREATE TRIGGER IF NOT EXISTS set_loops_updated_at_utc BEFORE UPDATE ON loops
     WHEN new.updated_at IS NULL OR new.updated_at = ''
     BEGIN
         UPDATE loops SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // ===== loop_triggers: 多类型触发器 =====
    // trigger_type: manual / cron / webhook / feishu_message / feishu_command /
    //               todo_completed / todo_state_changed / tag_added
    // config 是 JSON,具体 schema 由 trigger_type 决定:
    //   cron:               {"cron":"0 0 9 * * *","timezone":"Asia/Shanghai"}
    //   webhook:            {"webhook_id":5}
    //   feishu_message:     {"bot_id":1,"chat_id":"oc_xxx","match":"keyword|regex","pattern":"...","match_type":"exact|regex|contains"}
    //   feishu_command:     {"bot_id":1,"command":"/run"}
    //   todo_completed:     {"todo_id":7}
    //   todo_state_changed: {"todo_id":7,"to_status":"completed"}
    //   tag_added:          {"tag_id":3}
    "CREATE TABLE IF NOT EXISTS loop_triggers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_id INTEGER NOT NULL,
        trigger_type TEXT NOT NULL,
        config TEXT DEFAULT '{}',
        enabled INTEGER NOT NULL DEFAULT 1,
        priority INTEGER NOT NULL DEFAULT 0,
        created_at TEXT,
        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE
    )",
    "CREATE INDEX IF NOT EXISTS idx_loop_triggers_loop_id ON loop_triggers(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_triggers_type_enabled ON loop_triggers(trigger_type, enabled)",
    "CREATE TRIGGER IF NOT EXISTS set_loop_triggers_created_at_utc AFTER INSERT ON loop_triggers
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE loop_triggers SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // ===== loop_stages: 有序阶段 =====
    // todo_id 引用现有 todos 表;首版 run_mode 固定 sequential,字段预留
    "CREATE TABLE IF NOT EXISTS loop_stages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_id INTEGER NOT NULL,
        name TEXT NOT NULL,
        description TEXT DEFAULT '',
        order_index INTEGER NOT NULL DEFAULT 0,
        todo_id INTEGER NOT NULL,
        run_mode TEXT NOT NULL DEFAULT 'sequential',
        skip_on_source_failed INTEGER NOT NULL DEFAULT 0,
        min_rating INTEGER,
        unrated_policy TEXT NOT NULL DEFAULT 'skip',
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at TEXT,
        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE,
        FOREIGN KEY (todo_id) REFERENCES todos(id) ON DELETE RESTRICT
    )",
    "CREATE INDEX IF NOT EXISTS idx_loop_stages_loop_id ON loop_stages(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_stages_loop_order ON loop_stages(loop_id, order_index)",
    "CREATE TRIGGER IF NOT EXISTS set_loop_stages_created_at_utc AFTER INSERT ON loop_stages
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE loop_stages SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // ===== loop_executions: 每次运行的顶层记录 =====
    // trigger_meta 是 JSON,记录是谁/什么触发的(例: feishu 消息原文、webhook body 等)
    "CREATE TABLE IF NOT EXISTS loop_executions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_id INTEGER NOT NULL,
        trigger_id INTEGER,
        trigger_type TEXT NOT NULL,
        trigger_meta TEXT DEFAULT '{}',
        started_at TEXT NOT NULL,
        finished_at TEXT,
        status TEXT NOT NULL DEFAULT 'running',
        total_stages INTEGER NOT NULL DEFAULT 0,
        completed_stages INTEGER NOT NULL DEFAULT 0,
        failed_stages INTEGER NOT NULL DEFAULT 0,
        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE,
        FOREIGN KEY (trigger_id) REFERENCES loop_triggers(id) ON DELETE SET NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_loop_executions_loop_id ON loop_executions(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_executions_started_at ON loop_executions(started_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_loop_executions_status ON loop_executions(status)",
    // ===== loop_stage_executions: 每个阶段的执行 =====
    "CREATE TABLE IF NOT EXISTS loop_stage_executions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_execution_id INTEGER NOT NULL,
        stage_id INTEGER NOT NULL,
        todo_id INTEGER NOT NULL,
        execution_record_id INTEGER,
        status TEXT NOT NULL DEFAULT 'pending',
        started_at TEXT,
        finished_at TEXT,
        error_message TEXT,
        FOREIGN KEY (loop_execution_id) REFERENCES loop_executions(id) ON DELETE CASCADE,
        FOREIGN KEY (stage_id) REFERENCES loop_stages(id) ON DELETE CASCADE,
        FOREIGN KEY (execution_record_id) REFERENCES execution_records(id) ON DELETE SET NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_loop_stage_executions_loop_exec ON loop_stage_executions(loop_execution_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_stage_executions_record ON loop_stage_executions(execution_record_id)",
];

/// v3 — todos.kind 列。
///
/// 设计动机：
/// - 一次性 todo 是「事项」，循环复用的 todo 是「环节（Agent）」；
/// - 环路编排应当只引用环节，不应误选一次性事项；
/// - 同一张 todos 表承载两种语义，靠 `kind` 列区分，避免新建 steps 表的迁移成本；
/// - `step` 与 `item` 在存储层完全等价（prompt/executor/tag_ids 都共用），只是用法不同。
///
/// 升级策略：
/// - 加列：`ALTER TABLE todos ADD COLUMN kind TEXT NOT NULL DEFAULT 'item'`
/// - 回填：把当前已经被 loop_stages 引用的 todo 标记为 'step'，
///   未被引用的保持默认 'item'；
/// - 加索引：`(kind)` 用于环节/事项过滤查询。
const TODO_KIND_STATEMENTS: &[&str] = &[
    "ALTER TABLE todos ADD COLUMN kind TEXT NOT NULL DEFAULT 'item'",
    "UPDATE todos SET kind = 'step' WHERE id IN (SELECT DISTINCT todo_id FROM loop_stages)",
    "CREATE INDEX IF NOT EXISTS idx_todos_kind ON todos(kind)",
];

/// v4 — 环节独立为 steps 表。
///
/// 环节不再作为 todo 的一个 kind，而是独立的 steps 表，
/// 仅保留标题、提示词、执行器、验收标准字段，去除 hook/定时/门禁等 todo 专属属性。
/// 从 todo 升级时复制数据到 steps 表，原 todo 保留。
///
/// 回填策略：将 todos 表中已有 kind='step' 的数据复制到 steps 表，
/// 确保升级前后环节列表不丢失。
const INDEPENDENT_STEPS_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS steps (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        title TEXT NOT NULL,
        prompt TEXT NOT NULL DEFAULT '',
        executor TEXT,
        acceptance_criteria TEXT,
        source_todo_id INTEGER,
        created_at TEXT,
        updated_at TEXT,
        FOREIGN KEY (source_todo_id) REFERENCES todos(id) ON DELETE SET NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_steps_source_todo ON steps(source_todo_id)",
    "CREATE TRIGGER IF NOT EXISTS set_steps_created_at_utc AFTER INSERT ON steps
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE steps SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    "CREATE TRIGGER IF NOT EXISTS set_steps_updated_at_utc AFTER UPDATE ON steps
     WHEN new.updated_at IS NULL OR new.updated_at = ''
     BEGIN
         UPDATE steps SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // 回填已有步骤：将 todos 表中 kind='step' 的数据复制到 steps 表
    "INSERT INTO steps (title, prompt, executor, acceptance_criteria, source_todo_id, created_at, updated_at)
     SELECT title, COALESCE(prompt, ''), executor, acceptance_criteria, id, created_at, updated_at
     FROM todos WHERE kind = 'step' AND id NOT IN (SELECT source_todo_id FROM steps WHERE source_todo_id IS NOT NULL)",
];

/// SQL to create the `schema_version` meta table. Idempotent.
const SCHEMA_VERSION_DDL: &str = "CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL
)";

/// Read the highest applied schema version, or 0 if the table is empty / missing.
///
/// Returns 0 (not an error) for legacy DBs that have user tables but no
/// `schema_version` table yet; the caller treats 0 as "needs initial migration".
pub async fn current_schema_version(conn: &DatabaseConnection) -> i64 {
    // 表缺失时 sqlite 会返回错误,我们把它当作 version=0 处理
    let stmt = Statement::from_string(
        DbBackend::Sqlite,
        format!("SELECT COALESCE(MAX(version), 0) FROM {}", SCHEMA_VERSION_TABLE),
    );
    match conn.query_one(stmt).await {
        Ok(Some(row)) => row
            .try_get_by::<i64, _>("COALESCE(MAX(version), 0)")
            .unwrap_or(0),
        // 表不存在 / 没有行,都当作 version=0
        Ok(None) => 0,
        Err(_) => 0,
    }
}

/// Run all unapplied migrations on the given database, recording the new
/// highest version. Returns the post-run schema version.
pub async fn run_migrations(db: &Database) -> Result<i64, sea_orm::DbErr> {
    // meta 表本身用 IF NOT EXISTS 创建,绝对幂等
    db.exec(SCHEMA_VERSION_DDL).await?;

    let current = current_schema_version(&db.conn).await;
    let max_version = ALL_MIGRATIONS.last().map(|m| m.version).unwrap_or(0);

    if current >= max_version {
        tracing::debug!(
            "Schema already at version {} (max {}); skipping {} migrations",
            current,
            max_version,
            ALL_MIGRATIONS.len()
        );
        return Ok(current);
    }

    let mut new_version = current;
    let total_skipped_ddl: usize = ALL_MIGRATIONS
        .iter()
        .filter(|m| m.version <= current)
        .map(|m| m.statements.len())
        .sum();

    if total_skipped_ddl > 0 {
        tracing::debug!(
            "Schema migration: skipping {} already-applied DDL statements",
            total_skipped_ddl
        );
    }

    for migration in ALL_MIGRATIONS {
        if migration.version > current {
            apply_migration(db, migration).await?;
            new_version = migration.version;
        }
    }

    tracing::info!("Schema migration complete: version {}", new_version);
    Ok(new_version)
}

/// Apply a single migration. Per-statement errors are logged at debug level
/// (idempotent DDL on a legacy DB is expected to fail-and-skip for the
/// `ALTER TABLE ADD COLUMN` cases). The version-record insert is the
/// authoritative "did this migration succeed" checkpoint.
async fn apply_migration(db: &Database, m: &Migration) -> Result<(), sea_orm::DbErr> {
    tracing::info!(
        "Applying schema migration v{} ({}): {}",
        m.version,
        m.name,
        m.description
    );
    let started = std::time::Instant::now();

    for stmt in m.statements {
        if let Err(e) = db.exec(stmt).await {
            // 大多数情况下这是预期的"列已存在/触发器已存在"幂等失败;
            // 真正致命的错误会在后面 INSERT schema_version 时再次暴露。
            tracing::debug!("DDL skipped (likely idempotent): {}: {}", e, one_line(stmt));
        }
    }

    // 记录迁移版本 - 这里失败才算真正失败
    // 用绑定参数而非 format! + 手动引号转义。表名仍是 const(format! 拼接),
    // 不存在注入风险;但 version/name/timestamp 走 `$1/$2/$3` 参数化,与代码库其余
    // 部分的 `Statement::from_sql_and_values` 风格一致,避免未来 m.name 改成
    // 动态来源(registry / config)时变成 SQL 注入。
    let now = chrono::Utc::now().to_rfc3339();
    let record_sql = format!(
        "INSERT OR REPLACE INTO {} (version, name, applied_at) VALUES ($1, $2, $3)",
        SCHEMA_VERSION_TABLE
    );
    db.exec_with_params(
        &record_sql,
        vec![m.version.into(), m.name.into(), now.into()],
    )
    .await?;

    tracing::info!(
        "Schema migration v{} applied in {:?} ({} statements)",
        m.version,
        started.elapsed(),
        m.statements.len()
    );
    Ok(())
}

/// Collapse a DDL statement to a single line for compact log output.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_ordered_and_unique() {
        // 版本号必须严格递增,否则运行时会跳过中间的迁移
        let mut last: Option<i64> = None;
        for m in ALL_MIGRATIONS {
            match last {
                None => assert!(m.version >= 1, "version must be >= 1: {}", m.version),
                Some(v) => assert!(
                    m.version > v,
                    "migration versions must be strictly ascending: {} after {}",
                    m.version,
                    v
                ),
            }
            last = Some(m.version);
        }
    }

    #[test]
    fn no_empty_migration() {
        // 空 migration 会让 startup log 显示 "applied 0 statements" 误导排查
        for m in ALL_MIGRATIONS {
            assert!(
                !m.statements.is_empty(),
                "migration {} ({}) has no statements",
                m.version,
                m.name
            );
        }
    }

    #[test]
    fn one_line_collapses_whitespace() {
        let s = "CREATE TABLE foo (\n  id INT\n)";
        assert_eq!(one_line(s), "CREATE TABLE foo ( id INT )");
    }
}
