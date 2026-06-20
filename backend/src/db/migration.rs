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
        Box::new(V5ProjectDirectoryWorktree),
        Box::new(V6TodoKind),
        Box::new(V7LoopStudio),
        Box::new(V8LoopWorkspace),
        Box::new(V9IndependentSteps),
        Box::new(V10StepColor),
        Box::new(V11LoopFlowControl),
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

// ---------------------------------------------------------------------------
// v1 initial schema 拆分（issue #675）
// ---------------------------------------------------------------------------
//
// 把原来堆在一个函数体里的 ~30 张表 / ~40 个索引 / ~10 个触发器 / ~30 条 ALTER,
// 按"领域/用途"拆分为职责单一的子函数(每个 ≤ 30 行)。v1_initial_schema()
// 只负责调用顺序,不掺杂具体 DDL;新增表只追加子函数、不改主函数。
//
// 拆分原则:
//   1. 一张表 / 一个域 = 一个子函数,便于阅读、修改和单元测试。
//   2. 不改 DDL、不改执行顺序、不改事务边界 —— 拆分纯结构性,行为完全等价。
//   3. ALTER TABLE ADD COLUMN 的「重复 unwrap_or_else(warn)」模式抽成 helper,
//      兼容列按所属表分组调用,避免 26 条 ALTER 把单个函数撑爆。
//
// 执行顺序(保持与重构前完全一致):
//   - 先建稳定核心表 (todos / tags / execution_records / skill_invocations)
//   - 建高频过滤索引(依赖以上表已存在)
//   - 建 UTC 触发器(依赖 todos / tags 表已存在)
//   - 建功能模块表(agent_bots / feishu_* / executors / project_directories /
//     todo_templates / webhooks / usage_* / sync_records)
//   - 最后追加历史兼容列(仅在旧库缺列时才生效)

/// v1 初始 schema 的总编排入口。每个子函数职责单一、≤ 30 行。
async fn v1_initial_schema(db: &Database) -> Result<(), sea_orm::DbErr> {
    create_todos_table(db).await?;
    create_tags_tables(db).await?;
    create_execution_tables(db).await?;
    create_execution_logs_table(db).await?;
    create_skill_invocations_table(db).await?;
    create_high_frequency_indexes(db).await?;
    create_utc_triggers(db).await?;
    create_agent_bots_table(db).await?;
    create_feishu_homes(db).await?;
    create_feishu_messages(db).await?;
    create_feishu_history_chats(db).await?;
    create_feishu_push_targets(db).await?;
    create_feishu_response_config(db).await?;
    create_feishu_group_whitelist(db).await?;
    create_feishu_project_bindings(db).await?;
    create_feishu_indexes(db).await?;
    create_executors_table(db).await?;
    create_project_directories_table(db).await?;
    create_todo_templates_table(db).await?;
    create_webhooks_table(db).await?;
    create_webhook_records_table(db).await?;
    create_usage_daily_stats_table(db).await?;
    create_usage_daily_stats_trigger(db).await?;
    create_usage_model_breakdowns_table(db).await?;
    create_usage_executor_daily_stats_table(db).await?;
    create_sync_records_table(db).await?;
    add_legacy_columns(db).await?;
    create_auto_review_indexes(db).await?;
    Ok(())
}

// ---------------- 表创建 helpers(每个 ≤ 30 行) ----------------

/// todos 主表:所有 todo 的根表。`executor` / `scheduler_*` 等字段最初都在这里。
///
/// `kind` 列(事项 vs 环节)由 v6 迁移加进来; 但为了 fresh DB 一建表就拥有
/// 该列(避免空库启动后还要跑一次 v6 兼容 ALTER), 把它直接写进 v1 DDL。
/// 重复列错误被 v6 的 `add_column_warn` 静默吞掉, 与历史 add_legacy_*_columns
/// 同一处理风格。
async fn create_todos_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
            workspace TEXT,
            kind TEXT NOT NULL DEFAULT 'item'
        )",
    )
    .await
}

/// 标签表 + 多对多关联表(todo_tags)。
async fn create_tags_tables(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// execution_records 主表(每条记录对应一次执行任务的结果/日志/状态)。
async fn create_execution_tables(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// execution_logs 表(每条日志一行,支持分页加载)+ record_id 索引。
async fn create_execution_logs_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
        .await
}

/// skill_invocations 表:记录每个 todo 调用 skill 的轨迹。
async fn create_skill_invocations_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// 高频过滤索引(todos / todo_tags / execution_records / skill_invocations)。
async fn create_high_frequency_indexes(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_deleted_at ON todos(deleted_at)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_status ON todos(status)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_task_id ON todos(task_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_todo_tags_todo_id ON todo_tags(todo_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_todo_id ON execution_records(todo_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_task_id ON execution_records(task_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_pid ON execution_records(pid)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_session_id ON execution_records(session_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_status ON execution_records(status)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_started_at ON execution_records(started_at)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_executor ON execution_records(executor)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_model ON execution_records(model)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_todo_finished ON execution_records(todo_id, finished_at DESC)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_skill_invocations_skill_name ON skill_invocations(skill_name)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_skill_invocations_executor ON skill_invocations(executor)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_skill_invocations_todo_id ON skill_invocations(todo_id)").await
}

/// `created_at / updated_at` 自动填充 UTC 的触发器。
/// BEFORE UPDATE 让应用层显式写入时不被覆盖;只在 NULL/空时自动填充。
async fn create_utc_triggers(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// agent_bots 表 + 旧库缺 `config` 列时追加。
async fn create_agent_bots_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    // 旧库可能没有 config 列;用独立探测+追加,避免依赖 ADD COLUMN 的兼容性 hack
    add_column_if_missing(db, "agent_bots", "config", "ALTER TABLE agent_bots ADD COLUMN config TEXT DEFAULT '{}'").await?;
    Ok(())
}

/// feishu_homes 表:每个 (bot, user) 一行,记录当前 home view 信息。
async fn create_feishu_homes(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// feishu_messages 表。
async fn create_feishu_messages(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// feishu_history_chats 表:用户开启自动拉取历史的群聊配置。
async fn create_feishu_history_chats(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// feishu_push_targets 表:每个 bot 的私聊/群聊推送配置。
async fn create_feishu_push_targets(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// feishu_response_config 表:bot 响应触发配置,debounce_secs 默认 20。
async fn create_feishu_response_config(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// feishu_group_whitelist 表:bot 可响应的群成员白名单。
async fn create_feishu_group_whitelist(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// feishu_project_bindings 表:飞书会话 ↔ 项目目录 ↔ todo 三方绑定。
/// `chat_id='__pending__'` 是 Web UI 创建的待绑定记录,等待飞书侧 /bind 补齐。
///
/// 注意:`enabled` 列虽然历史上是 hotfix 追加的兼容列,但被后续 partial unique
/// index `idx_feishu_bindings_active` 引用(`WHERE ... AND enabled = 1`),所以必须
/// 在建索引之前存在 —— 这里用 add_column_if_missing() 在建表后立即追加,
/// 重复调用幂等无副作用。
async fn create_feishu_project_bindings(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    // 必须在 create_feishu_indexes() 之前:partial unique index 引用此列
    add_column_if_missing(
        db,
        "feishu_project_bindings",
        "enabled",
        "ALTER TABLE feishu_project_bindings ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1",
    )
    .await
}

/// 飞书相关全部索引集中管理(便于一眼看全)。
async fn create_feishu_indexes(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec("CREATE INDEX IF NOT EXISTS idx_feishu_messages_chat_id ON feishu_messages(chat_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_feishu_messages_created_at ON feishu_messages(created_at)").await?;
    db.exec("CREATE UNIQUE INDEX IF NOT EXISTS idx_feishu_bindings_active ON feishu_project_bindings(bot_id, chat_id) WHERE chat_id != '__pending__' AND enabled = 1").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_feishu_bindings_record_id ON feishu_project_bindings(latest_record_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_feishu_bindings_bot_id ON feishu_project_bindings(bot_id)").await
}

/// executors 表 + name 索引;旧库缺 session_dir 列时自动追加。
async fn create_executors_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_executors_name ON executors(name)").await?;
    // 旧库可能没有此列;独立探测+追加
    add_column_if_missing(db, "executors", "session_dir", "ALTER TABLE executors ADD COLUMN session_dir TEXT NOT NULL DEFAULT ''").await?;
    Ok(())
}

/// project_directories 表 + path 索引 + UTC created_at 触发器。
async fn create_project_directories_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_project_directories_path ON project_directories(path)").await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_project_directories_created_at_utc AFTER INSERT ON project_directories
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE project_directories SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await
}

/// todo_templates 表 + 兼容列(is_system/source_url/last_sync_at) + UTC 触发器。
async fn create_todo_templates_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    // 旧库依次补 3 列(按兼容性顺序;可幂等反复执行)
    add_column_if_missing(db, "todo_templates", "is_system", "ALTER TABLE todo_templates ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0").await?;
    add_column_if_missing(db, "todo_templates", "source_url", "ALTER TABLE todo_templates ADD COLUMN source_url TEXT").await?;
    add_column_if_missing(db, "todo_templates", "last_sync_at", "ALTER TABLE todo_templates ADD COLUMN last_sync_at TEXT").await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_todo_templates_created_at_utc AFTER INSERT ON todo_templates
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE todo_templates SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await
}

/// webhooks 主表 + UTC created_at 触发器。
async fn create_webhooks_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    .await
}

/// webhook_records 表 + 3 个索引 + UTC created_at 触发器。
async fn create_webhook_records_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_webhook_records_webhook_id ON webhook_records(webhook_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_webhook_records_triggered_todo_id ON webhook_records(triggered_todo_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_webhook_records_created_at ON webhook_records(created_at)").await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_webhook_records_created_at_utc AFTER INSERT ON webhook_records
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE webhook_records SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await
}

/// usage_daily_stats 表 + 索引(UTC 触发器拆到独立函数,避免本函数超 30 行)。
async fn create_usage_daily_stats_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_daily_stats_date ON usage_daily_stats(date)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_daily_stats_stats_type ON usage_daily_stats(stats_type)").await?;
    create_usage_daily_stats_trigger(db).await
}

/// usage_daily_stats 表的 UTC created_at 触发器。
async fn create_usage_daily_stats_trigger(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_usage_daily_stats_created_at_utc AFTER INSERT ON usage_daily_stats
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE usage_daily_stats SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await
}

/// usage_model_breakdowns 表 + daily_stat_id 索引。
async fn create_usage_model_breakdowns_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_model_breakdowns_daily_stat_id ON usage_model_breakdowns(daily_stat_id)").await
}

/// usage_executor_daily_stats 表 + 索引 + UTC 触发器。
async fn create_usage_executor_daily_stats_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_executor_daily_stats_date ON usage_executor_daily_stats(date)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_usage_executor_daily_stats_executor ON usage_executor_daily_stats(executor)").await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_usage_executor_daily_stats_created_at_utc AFTER INSERT ON usage_executor_daily_stats
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE usage_executor_daily_stats SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await
}

/// sync_records 表 + created_at 索引。
async fn create_sync_records_table(db: &Database) -> Result<(), sea_orm::DbErr> {
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
        .await
}

/// 自动评审相关索引(todos.parent_todo_id / todos.todo_type / execution_records.source_execution_record_id)。
async fn create_auto_review_indexes(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_parent_todo_id ON todos(parent_todo_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_todo_type ON todos(todo_type)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_execution_records_source_record_id ON execution_records(source_execution_record_id)").await
}

// ---------------- 历史兼容列追加(按表分组) ----------------

/// 全部向后兼容 ALTER TABLE 集合。按表分组,避免 26 条 ALTER 把单个函数撑爆。
/// 重复列名错误(旧库上列已存在)属预期,仅记 warn;真实错误冒泡。
async fn add_legacy_columns(db: &Database) -> Result<(), sea_orm::DbErr> {
    add_legacy_execution_record_columns(db).await?;
    add_legacy_todos_columns(db).await?;
    add_legacy_feishu_messages_columns(db).await?;
    add_legacy_misc_columns(db).await?;
    Ok(())
}

/// execution_records 历史追加列(13 条,含自动评审与 hook 触发起源字段)。
async fn add_legacy_execution_record_columns(db: &Database) -> Result<(), sea_orm::DbErr> {
    const COLS: &[&str] = &[
        "ALTER TABLE execution_records ADD COLUMN pid INTEGER",
        "ALTER TABLE execution_records ADD COLUMN task_id TEXT",
        "ALTER TABLE execution_records ADD COLUMN session_id TEXT",
        "ALTER TABLE execution_records ADD COLUMN todo_progress TEXT",
        "ALTER TABLE execution_records ADD COLUMN execution_stats TEXT",
        "ALTER TABLE execution_records ADD COLUMN resume_message TEXT",
        "ALTER TABLE execution_records ADD COLUMN source_todo_id INTEGER",
        "ALTER TABLE execution_records ADD COLUMN source_todo_title TEXT",
        "ALTER TABLE execution_records ADD COLUMN source_hook_id INTEGER",
        "ALTER TABLE execution_records ADD COLUMN rating INTEGER",
        "ALTER TABLE execution_records ADD COLUMN source_execution_record_id INTEGER",
        "ALTER TABLE execution_records ADD COLUMN last_review_status TEXT",
        "ALTER TABLE execution_records ADD COLUMN last_reviewed_at TEXT",
    ];
    for sql in COLS {
        add_column_warn(db, sql).await;
    }
    Ok(())
}

/// todos 历史追加列(8 条,含自动评审与 worktree/scheduler 字段)。
async fn add_legacy_todos_columns(db: &Database) -> Result<(), sea_orm::DbErr> {
    const COLS: &[&str] = &[
        "ALTER TABLE todos ADD COLUMN workspace TEXT",
        "ALTER TABLE todos ADD COLUMN worktree_enabled INTEGER DEFAULT 0",
        "ALTER TABLE todos ADD COLUMN scheduler_timezone TEXT",
        "ALTER TABLE todos ADD COLUMN hooks TEXT",
        "ALTER TABLE todos ADD COLUMN acceptance_criteria TEXT",
        "ALTER TABLE todos ADD COLUMN todo_type INTEGER DEFAULT 0",
        "ALTER TABLE todos ADD COLUMN parent_todo_id INTEGER",
        "ALTER TABLE todos ADD COLUMN auto_review_enabled INTEGER DEFAULT 1",
    ];
    for sql in COLS {
        add_column_warn(db, sql).await;
    }
    Ok(())
}

/// feishu_messages 历史追加列:4 条走 `ADD COLUMN IF NOT EXISTS`(SQLite 3.35+),
/// 2 条(processed_todo_id / execution_record_id)在 IF NOT EXISTS 失败时回退。
async fn add_legacy_feishu_messages_columns(db: &Database) -> Result<(), sea_orm::DbErr> {
    const IF_NOT_EXISTS_COLS: &[&str] = &[
        "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS sender_nickname TEXT",
        "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS sender_type TEXT",
        "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS is_history INTEGER DEFAULT 0",
        "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS fetch_time TEXT",
    ];
    for sql in IF_NOT_EXISTS_COLS {
        add_column_warn(db, sql).await;
    }
    // IF NOT EXISTS 在老版本 SQLite 失败时回退到普通 ADD COLUMN
    add_column_with_fallback(
        db,
        "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS processed_todo_id INTEGER",
        "ALTER TABLE feishu_messages ADD COLUMN processed_todo_id INTEGER",
    )
    .await?;
    add_column_with_fallback(
        db,
        "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS execution_record_id INTEGER",
        "ALTER TABLE feishu_messages ADD COLUMN execution_record_id INTEGER",
    )
    .await
}

/// 散落各表的杂项历史列(7 条):debounce_secs / enabled / session_dir / 3 列 templates / config。
/// 散落各表的杂项历史列(6 条):debounce_secs / session_dir / 3 列 templates / config。
/// 注:`feishu_project_bindings.enabled` 已在 create_feishu_project_bindings() 内
/// 追加(因为 partial unique index 依赖它),此处不再重复。
async fn add_legacy_misc_columns(db: &Database) -> Result<(), sea_orm::DbErr> {
    const COLS: &[&str] = &[
        "ALTER TABLE feishu_response_config ADD COLUMN debounce_secs INTEGER DEFAULT 20",
        "ALTER TABLE executors ADD COLUMN session_dir TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE todo_templates ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE todo_templates ADD COLUMN source_url TEXT",
        "ALTER TABLE todo_templates ADD COLUMN last_sync_at TEXT",
        "ALTER TABLE agent_bots ADD COLUMN config TEXT DEFAULT '{}'",
    ];
    for sql in COLS {
        add_column_warn(db, sql).await;
    }
    Ok(())
}

// ---------------- 内部 helpers ----------------

/// 用 `PRAGMA table_info` 判断某列是否存在。返回 `Ok(true)` 表示表+列都存在。
async fn table_has_column(db: &Database, table: &str, column: &str) -> Result<bool, sea_orm::DbErr> {
    let sql = format!(
        "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name='{}'",
        table, column
    );
    let row = db
        .conn
        .query_one(Statement::from_string(DbBackend::Sqlite, sql))
        .await?;
    Ok(row
        .and_then(|r| r.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0)
        > 0)
}

/// 「探测列存在性 → 缺则 ALTER 追加」。把 6 处相同的探测+ALTER 模式收敛到一个 helper。
async fn add_column_if_missing(
    db: &Database,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<(), sea_orm::DbErr> {
    if !table_has_column(db, table, column).await? {
        db.exec(alter_sql).await?;
    }
    Ok(())
}

/// 「执行一条 ALTER TABLE ADD COLUMN,失败仅 warn」。
/// 重复列名错误(旧库上列已存在)属预期,仅记 warn;真实错误理论上不会发生。
async fn add_column_warn(db: &Database, sql: &str) {
    if let Err(e) = db.exec(sql).await {
        tracing::warn!("migration v1: {}: {} (column likely already exists)", sql, e);
    }
}

/// 「先试 IF NOT EXISTS 版本,失败则回退到普通 ADD COLUMN」。
/// 用于旧版 SQLite(<3.35)不支持 `ADD COLUMN IF NOT EXISTS` 的场景。
async fn add_column_with_fallback(
    db: &Database,
    if_not_exists_sql: &str,
    fallback_sql: &str,
) -> Result<(), sea_orm::DbErr> {
    if let Err(e) = db.exec(if_not_exists_sql).await {
        tracing::debug!(
            "migration v1: IF NOT EXISTS ADD COLUMN failed ({}), falling back",
            e
        );
        add_column_warn(db, fallback_sql).await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests for v1 split helpers (issue #675)
// ---------------------------------------------------------------------------
//
// 新引入的 helper(`table_has_column` / `add_column_if_missing` / `add_column_warn`)
// 是 v1_initial_schema 拆分后的复用基石,任何 helper 行为变化都会让
// 整个 v1 在旧库上的兼容分支出错。这里加 5 个 fixture-driven test 把
// 它们的 3 个核心分支钉死:
//   1. table_has_column: 列存在 → true / 列不存在 → false / 表不存在 → false
//   2. add_column_if_missing: 列已存在 → 跳过 / 列不存在 → ALTER 追加
//   3. add_column_with_fallback: IF NOT EXISTS 失败 → 回退 ALTER

#[cfg(test)]
mod v1_helpers_tests {
    use super::*;

    /// 复用与 needs_fk_migration_tests 同款的 fresh_db helper,确保走完整 v1 init
    async fn fresh_db() -> Database {
        Database::new(":memory:")
            .await
            .expect(":memory: db must open")
    }

    /// 分支 1: 列存在 → true。v1 init 已经把 todos.workspace 加进去。
    #[tokio::test]
    async fn table_has_column_returns_true_when_column_exists() {
        let db = fresh_db().await;
        assert!(
            table_has_column(&db, "todos", "workspace")
                .await
                .expect("probe must succeed"),
            "todos.workspace is added by v1, must be detected"
        );
    }

    /// 分支 2: 表上没这列 → false。用一个肯定不在 v1 表上的列名。
    #[tokio::test]
    async fn table_has_column_returns_false_when_column_missing() {
        let db = fresh_db().await;
        assert!(
            !table_has_column(&db, "todos", "definitely_not_a_real_column_xyz")
                .await
                .expect("probe must succeed"),
            "non-existent column must report false"
        );
    }

    /// 分支 3: 表不存在 → false(PRAGMA table_info 对不存在的表返回 0 行)。
    /// 这一点很关键:add_column_if_missing() 依赖它能优雅处理新建表前的探测场景。
    #[tokio::test]
    async fn table_has_column_returns_false_when_table_missing() {
        let db = fresh_db().await;
        assert!(
            !table_has_column(&db, "no_such_table_xyz", "anything")
                .await
                .expect("probe must succeed"),
            "non-existent table must report false (not panic)"
        );
    }

    /// 分支 4: add_column_if_missing → 列已存在则跳过(幂等无副作用)。
    /// 这是 v1 在已迁移库上反复启动不爆的关键。
    #[tokio::test]
    async fn add_column_if_missing_skips_when_column_exists() {
        let db = fresh_db().await;
        // todos.workspace 已经被 v1 添加;重复调用必须 no-op
        add_column_if_missing(
            &db,
            "todos",
            "workspace",
            "ALTER TABLE todos ADD COLUMN workspace TEXT",
        )
        .await
        .expect("skip must succeed");
        // workspace 必须仍是单列(没有重复)
        assert!(
            table_has_column(&db, "todos", "workspace").await.unwrap(),
            "workspace must still exist after no-op skip"
        );
    }

    /// 分支 5: add_column_if_missing → 列不存在则追加。
    /// 用临时表隔离,确保只在测试自身建的表上操作,不污染 v1 schema。
    #[tokio::test]
    async fn add_column_if_missing_adds_when_column_missing() {
        let db = fresh_db().await;
        db.exec("CREATE TABLE acim_probe (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .expect("table must create");
        assert!(
            !table_has_column(&db, "acim_probe", "nickname").await.unwrap(),
            "precondition: column must be absent"
        );
        add_column_if_missing(
            &db,
            "acim_probe",
            "nickname",
            "ALTER TABLE acim_probe ADD COLUMN nickname TEXT",
        )
        .await
        .expect("add must succeed");
        assert!(
            table_has_column(&db, "acim_probe", "nickname").await.unwrap(),
            "column must be added"
        );
        // 再次调用必须幂等 — 不会因为 ALTER 重复列失败(否则会 panic/Err)
        add_column_if_missing(
            &db,
            "acim_probe",
            "nickname",
            "ALTER TABLE acim_probe ADD COLUMN nickname TEXT",
        )
        .await
        .expect("second call must also succeed (idempotent)");
    }
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
                WHERE er.todo_id = t.id AND er.finished_at IS NOT NULL \
                ORDER BY er.started_at DESC, er.id DESC LIMIT 1) AS latest_record_id \
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

    // 注意 (PR #539 push-4 review CRITICAL): PRAGMA foreign_keys 不能在事务
    // 内设置（SQLite 直接禁止：no-op / SQLITE_ERROR）。该 PRAGMA 必须由调用方
    // （migrate_feishu_fk_cascade）在事务**外**统一管理。本函数不再操作 FK 设置。

    // 清理上次中断可能残留的临时表
    exec_on_conn(conn, &format!("DROP TABLE IF EXISTS {}", tmp)).await?;

    // 创建新表
    exec_on_conn(conn, &format!("CREATE TABLE IF NOT EXISTS {} ({})", tmp, columns)).await?;

    // 列名取交集：用新表（DDL 定义的 schema）为权威，避免旧表存在「已被 hotfix
    // 加进、但当前 DDL 没包含」的列导致 INSERT ... SELECT 报 "no such column"。
    // 旧表缺新表有 → 跳过该列（旧数据无值，新列 DEFAULT NULL）。
    // 新表缺旧表有 → 跳过该列（旧数据不被复制）。
    let old_col_rows = conn
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            format!("PRAGMA table_info('{}')", table),
        ))
        .await?;
    let new_col_rows = conn
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            format!("PRAGMA table_info('{}')", tmp),
        ))
        .await?;
    let old_col_names: std::collections::HashSet<String> = old_col_rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();
    let cols_str: String = new_col_rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .filter(|name| old_col_names.contains(name))
        .collect::<Vec<_>>()
        .join(", ");

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
    //
    // 重要 (PR #539 push-4 review CRITICAL)：`PRAGMA foreign_keys = OFF` /
    // `= ON` **必须在事务外**执行。SQLite 文档明确规定：
    //   "This pragma is a no-op within a transaction; foreign key constraint
    //    enforcement may only be enabled or disabled when there is no pending
    //    BEGIN or SAVEPOINT."
    // 在事务内执行会得到 SQLITE_ERROR "cannot change foreign key enforcement
    // inside of a transaction"，整个 migration runner 失败、daemon 拒绝启动。
    // 注意：必须在 begin() **之前** OFF、commit() **之后** ON 才有效果。
    db.exec("PRAGMA foreign_keys = OFF").await?;
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

    // 事务提交后再开 FK 检查（必须在事务外才生效）。
    db.exec("PRAGMA foreign_keys = ON").await?;

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

// ---------------------------------------------------------------------------
// v5: 项目目录级 git worktree 支持 (issue #643)
// ---------------------------------------------------------------------------

/// v5 迁移：增加 3 个字段
///   - project_directories.git_worktree_enabled (NOT NULL DEFAULT 0)
///   - project_directories.auto_cleanup         (NOT NULL DEFAULT 0)
///   - execution_records.worktree_path          (NULL)
///
/// 全部使用 `ADD COLUMN IF NOT EXISTS` / `unwrap_or_else` 兼容旧库：
/// 字段在 IF NOT EXISTS 不被 SQLite 支持时（旧版 < 3.35）回退到忽略"已存在"错误。
pub(super) struct V5ProjectDirectoryWorktree;

#[async_trait]
impl Migration for V5ProjectDirectoryWorktree {
    fn version(&self) -> i64 {
        5
    }
    fn name(&self) -> &'static str {
        "project_directory_worktree"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        v5_project_directory_worktree(db).await
    }
}

/// 把 v5 三条 ALTER 串成一条：只在 "duplicate column name" 类型的错误上吞掉并 warn，
/// 其它真实错误（表不存在、SQL 语法错误等）必须传播出去——否则迁移被错误地标记为已应用，
/// 后续运行会因为缺列而炸在更难定位的位置。
///
/// SQLite 错误信息中 "duplicate column name" 由原生接口直接产出，未走 i18n；按子串匹配即可。
async fn run_v5_alter(db: &Database, sql: &str, label: &str) -> Result<(), sea_orm::DbErr> {
    if let Err(e) = db.exec(sql).await {
        // 仅在「列已存在」这一类幂等错误上跳过，其它错误必须向上抛
        let msg = e.to_string();
        if msg.contains("duplicate column name") {
            tracing::warn!(
                "migration v5: {} column may already exist, skipping: {}",
                label,
                msg
            );
            Ok(())
        } else {
            Err(e)
        }
    } else {
        Ok(())
    }
}

async fn v5_project_directory_worktree(db: &Database) -> Result<(), sea_orm::DbErr> {
    // 加列失败时只在「duplicate column name」语义下吞掉并 warn：老库可能已经手工补过这些列。
    // 其它错误（如表不存在、SQL 语法错误）必须传播，避免迁移被错误标记为已应用后留下隐患。
    run_v5_alter(
        db,
        "ALTER TABLE project_directories ADD COLUMN git_worktree_enabled INTEGER NOT NULL DEFAULT 0",
        "git_worktree_enabled",
    )
    .await?;
    run_v5_alter(
        db,
        "ALTER TABLE project_directories ADD COLUMN auto_cleanup INTEGER NOT NULL DEFAULT 0",
        "auto_cleanup",
    )
    .await?;
    run_v5_alter(
        db,
        "ALTER TABLE execution_records ADD COLUMN worktree_path TEXT",
        "worktree_path",
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// v6: todos.kind 列 (issue #674: 事项 vs 环节区分)
// ---------------------------------------------------------------------------

/// v6 迁移：为 todos 表增加 `kind` 列, 区分一次性事项('item')和
/// 可被 loop 编排复用的环节('step')。
///
/// 设计动机：
/// - 一次性 todo 是「事项」，循环复用的 todo 是「环节（Agent）」；
/// - 环路编排的节点只应引用环节，引用一次性事项会污染"循环复用"语义；
/// - 同一张 todos 表承载两种语义, 靠 `kind` 列区分; 避免新建 steps 表的
///   schema 迁移 + 跨表 JOIN 成本。
///
/// 升级策略：
/// - 新库: v1 的 CREATE TABLE 已经包含 `kind` 列, v6 ALTER 在 v1 之后跑会
///   触发 "duplicate column name", 与历史 add_legacy_*_columns 同样的 warn-skip 模式;
/// - 旧库: ALTER TABLE 加列, 默认 'item'; 把被 loop_steps 引用的 todo
///   标记为 'step', 避免环路失效;
/// - 加 `(kind)` 索引支持按 kind 过滤。
pub(super) struct V6TodoKind;

#[async_trait]
impl Migration for V6TodoKind {
    fn version(&self) -> i64 {
        6
    }
    fn name(&self) -> &'static str {
        "todo_kind"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        v6_todo_kind(db).await
    }
}

async fn v6_todo_kind(db: &Database) -> Result<(), sea_orm::DbErr> {
    // 1) 加列, 旧库上没有 kind 列时生效; 新库已由 v1 CREATE TABLE 包含, 静默跳过
    add_column_warn(db, "ALTER TABLE todos ADD COLUMN kind TEXT NOT NULL DEFAULT 'item'").await;
    // 2) 回填: 被 loop_steps 引用的 todo 升级为 step
    // loop_steps 表不一定存在 (旧库, 或 fresh 跑 v1 没建), 探测一下避免 UPDATE 失败
    if table_has_column(db, "todos", "kind").await?
        && table_exists(db, "loop_steps").await?
    {
        db.exec(
            "UPDATE todos SET kind = 'step' \
             WHERE id IN (SELECT DISTINCT todo_id FROM loop_steps)",
        )
        .await?;
    }
    // 3) 加 kind 索引
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_kind ON todos(kind)").await?;
    Ok(())
}

/// 检测 sqlite_master 上是否有该表, 用于 v6 等「表可能不存在」场景的探测。
async fn table_exists(db: &Database, table: &str) -> Result<bool, sea_orm::DbErr> {
    let sql = format!(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
        table
    );
    // 同 needs_fk_migration 的注入防护: 表名走白名单 (调用方都是 hardcoded 字符串).
    debug_assert!(
        !table.is_empty() && table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "table_exists: invalid table name {table:?}"
    );
    let row = db
        .conn
        .query_one(Statement::from_string(DbBackend::Sqlite, sql))
        .await?;
    Ok(row
        .and_then(|r| r.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0)
        > 0)
}

// ---------------------------------------------------------------------------
// v7: Loop Studio (issue #670: 把 Loop Studio DDL 迁到 runner 系统)
// ---------------------------------------------------------------------------

/// v7 迁移: 把 Loop Studio 的 6 张表 + 索引/触发器从 `db/migrations.rs`
/// (旧版本化迁移) 搬到 runner 系统, 让所有新建内存库 (测试用) 都能跑出
/// 完整 schema.
///
/// 设计动机:
/// - 旧 `migrations.rs::run_migrations` 没被 `Database::new` 调用
///   (issue #498 引入 runner 后取代了它), 导致测试内存库缺失
///   loops/loop_steps/loop_hooks/loop_triggers/loop_executions/
///   loop_step_executions 这 6 张表, 测试不得不手工建表或绕开;
/// - 把 DDL 集中到 runner 系统后, 内存测试和真实生产 DB 走同一条
///   迁移路径, 避免「测试通过, 生产报错」的分裂.
///
/// 幂等性: 所有 DDL 都带 `IF NOT EXISTS`, 重跑无害.
pub(super) struct V7LoopStudio;

#[async_trait]
impl Migration for V7LoopStudio {
    fn version(&self) -> i64 {
        7
    }
    fn name(&self) -> &'static str {
        "loop_studio"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        v7_loop_studio(db).await
    }
}

/// 6 张表 DDL + 索引 + 触发器. 顺序按 (loops, loop_triggers, loop_steps,
/// loop_hooks, loop_executions, loop_step_executions), 外键引用
/// 关系保证后续表能成功建出.
async fn v7_loop_studio(db: &Database) -> Result<(), sea_orm::DbErr> {
    for stmt in LOOP_STUDIO_DDL {
        db.exec(stmt).await?;
    }
    Ok(())
}

/// 集中放置的 Loop Studio DDL. 之所以写成模块级 const slice 而非内联
/// 在 v7_loop_studio 函数体里, 是为了 (1) 测试可直接复用, (2) DDL 列表
/// 不会污染函数体长度 (CLAUDE.md 单函数 30 行限制).
const LOOP_STUDIO_DDL: &[&str] = &[
    // ===== loops: 环路主表 =====
    "CREATE TABLE IF NOT EXISTS loops (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        description TEXT DEFAULT '',
        workspace TEXT,
        status TEXT NOT NULL DEFAULT 'draft',
        color TEXT DEFAULT '#722ed1',
        icon TEXT DEFAULT 'loop',
        created_at TEXT,
        updated_at TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_loops_status ON loops(status)",
    "CREATE INDEX IF NOT EXISTS idx_loops_updated_at ON loops(updated_at DESC)",
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
    // ===== loop_steps: 有序阶段 =====
    "CREATE TABLE IF NOT EXISTS loop_steps (
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
    "CREATE INDEX IF NOT EXISTS idx_loop_steps_loop_id ON loop_steps(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_steps_loop_order ON loop_steps(loop_id, order_index)",
    "CREATE TRIGGER IF NOT EXISTS set_loop_steps_created_at_utc AFTER INSERT ON loop_steps
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE loop_steps SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // ===== loop_hooks: 环路级 hook =====
    "CREATE TABLE IF NOT EXISTS loop_hooks (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_id INTEGER NOT NULL,
        hook_position TEXT NOT NULL,
        source_step_id INTEGER,
        target_todo_id INTEGER NOT NULL,
        skip_if_missing INTEGER NOT NULL DEFAULT 0,
        enabled INTEGER NOT NULL DEFAULT 1,
        min_rating INTEGER,
        unrated_policy TEXT NOT NULL DEFAULT 'skip',
        created_at TEXT,
        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE,
        FOREIGN KEY (source_step_id) REFERENCES loop_steps(id) ON DELETE CASCADE,
        FOREIGN KEY (target_todo_id) REFERENCES todos(id) ON DELETE RESTRICT
    )",
    "CREATE INDEX IF NOT EXISTS idx_loop_hooks_loop_id ON loop_hooks(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_hooks_source_stage ON loop_hooks(source_step_id)",
    "CREATE TRIGGER IF NOT EXISTS set_loop_hooks_created_at_utc AFTER INSERT ON loop_hooks
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN
         UPDATE loop_hooks SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
     END",
    // ===== loop_executions: 每次运行的顶层记录 =====
    "CREATE TABLE IF NOT EXISTS loop_executions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_id INTEGER NOT NULL,
        trigger_id INTEGER,
        trigger_type TEXT NOT NULL,
        trigger_meta TEXT DEFAULT '{}',
        started_at TEXT NOT NULL,
        finished_at TEXT,
        status TEXT NOT NULL DEFAULT 'running',
        total_steps INTEGER NOT NULL DEFAULT 0,
        completed_steps INTEGER NOT NULL DEFAULT 0,
        failed_steps INTEGER NOT NULL DEFAULT 0,
        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE,
        FOREIGN KEY (trigger_id) REFERENCES loop_triggers(id) ON DELETE SET NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_loop_executions_loop_id ON loop_executions(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_executions_started_at ON loop_executions(started_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_loop_executions_status ON loop_executions(status)",
    // ===== loop_step_executions: 每个阶段的执行 =====
    "CREATE TABLE IF NOT EXISTS loop_step_executions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_execution_id INTEGER NOT NULL,
        step_id INTEGER NOT NULL,
        todo_id INTEGER NOT NULL,
        execution_record_id INTEGER,
        status TEXT NOT NULL DEFAULT 'pending',
        started_at TEXT,
        finished_at TEXT,
        error_message TEXT,
        FOREIGN KEY (loop_execution_id) REFERENCES loop_executions(id) ON DELETE CASCADE,
        FOREIGN KEY (step_id) REFERENCES loop_steps(id) ON DELETE CASCADE,
        FOREIGN KEY (execution_record_id) REFERENCES execution_records(id) ON DELETE SET NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_loop_step_executions_loop_exec ON loop_step_executions(loop_execution_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_step_executions_record ON loop_step_executions(execution_record_id)",
];

#[cfg(test)]
mod v7_loop_studio_tests {
    //! 验证 v7 迁移建表完整, 6 张 Loop Studio 表 + 索引都到位.
    use super::*;

    #[tokio::test]
    async fn v7_creates_all_loop_studio_tables() {
        let db = Database::new(":memory:").await.unwrap();
        for table in [
            "loops",
            "loop_triggers",
            "loop_steps",
            "loop_hooks",
            "loop_executions",
            "loop_step_executions",
        ] {
            assert!(
                table_exists(&db, table).await.unwrap(),
                "v7 迁移后表 {table} 应当存在"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// v8: loops 表加 workspace 列 (用于关联工作空间)
// ---------------------------------------------------------------------------

/// v8 迁移：为 `loops` 表添加 `workspace` 列，替换原来的 product/repo/branch 字段。
///
/// 设计动机：
/// - Loop 不再需要独立的产品/仓库/分支字段，改为关联工作空间（与 todo 共用同一套 workspace 体系）。
/// - 旧字段 product/repo/branch 在 v7 建的表中仍存在，但 v8 不删它们（避免数据丢失）；
///   新库的 DDL 已直接使用 workspace 替代。
///
/// 升级策略：
/// - 新库: v7 DDL 已经直接定义 `workspace TEXT`，而非 product/repo/branch，v8 ALTER 会被静默跳过。
/// - 旧库: ALTER TABLE 加 workspace 列，保留旧列不动。
pub(super) struct V8LoopWorkspace;

#[async_trait]
impl Migration for V8LoopWorkspace {
    fn version(&self) -> i64 {
        8
    }
    fn name(&self) -> &'static str {
        "loop_workspace"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        v8_loop_workspace(db).await
    }
}

async fn v8_loop_workspace(db: &Database) -> Result<(), sea_orm::DbErr> {
    add_column_warn(db, "ALTER TABLE loops ADD COLUMN workspace TEXT").await;
    Ok(())
}

// ===== V9: 环节独立为 steps 表 =====

pub(super) struct V9IndependentSteps;

#[async_trait]
impl Migration for V9IndependentSteps {
    fn version(&self) -> i64 {
        9
    }
    fn name(&self) -> &'static str {
        "independent_steps"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        v9_independent_steps(db).await
    }
}

async fn v9_independent_steps(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec(
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
    )
    .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_steps_source_todo ON steps(source_todo_id)")
        .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_steps_created_at_utc AFTER INSERT ON steps
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN
             UPDATE steps SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_steps_updated_at_utc AFTER UPDATE ON steps
         WHEN new.updated_at IS NULL OR new.updated_at = ''
         BEGIN
             UPDATE steps SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
         END",
    )
    .await?;
    // 回填已有步骤：将 todos 表中 kind='step' 的数据复制到 steps 表
    db.exec(
        "INSERT INTO steps (title, prompt, executor, acceptance_criteria, source_todo_id, created_at, updated_at)
         SELECT title, COALESCE(prompt, ''), executor, acceptance_criteria, id, created_at, updated_at
         FROM todos WHERE kind = 'step' AND id NOT IN (SELECT source_todo_id FROM steps WHERE source_todo_id IS NOT NULL)",
    )
    .await?;
    Ok(())
}

// ===== V10: steps 表增加 color 列 =====

pub(super) struct V10StepColor;

#[async_trait]
impl Migration for V10StepColor {
    fn version(&self) -> i64 {
        10
    }
    fn name(&self) -> &'static str {
        "step_color"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_warn(db, "ALTER TABLE steps ADD COLUMN color TEXT NOT NULL DEFAULT '#722ed1'").await;
        Ok(())
    }
}

pub(super) struct V11LoopFlowControl;

#[async_trait]
impl Migration for V11LoopFlowControl {
    fn version(&self) -> i64 {
        11
    }
    fn name(&self) -> &'static str {
        "loop_flow_control"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // loop_steps: 控制流字段
        add_column_warn(db, "ALTER TABLE loop_steps ADD COLUMN on_success TEXT NOT NULL DEFAULT 'next'").await;
        add_column_warn(db, "ALTER TABLE loop_steps ADD COLUMN success_goto_step_id BIGINT").await;
        add_column_warn(db, "ALTER TABLE loop_steps ADD COLUMN on_rating_fail TEXT NOT NULL DEFAULT 'break'").await;
        add_column_warn(db, "ALTER TABLE loop_steps ADD COLUMN fail_goto_step_id BIGINT").await;

        // loops: 全局限制配置
        add_column_warn(db, "ALTER TABLE loops ADD COLUMN limits_config TEXT NOT NULL DEFAULT '{}'").await;

        // loop_executions: 累计执行步数
        add_column_warn(db, "ALTER TABLE loop_executions ADD COLUMN total_executed_steps INTEGER NOT NULL DEFAULT 0").await;

        // loop_step_executions: 黑板字段
        add_column_warn(db, "ALTER TABLE loop_step_executions ADD COLUMN sequence_index INTEGER NOT NULL DEFAULT 0").await;
        add_column_warn(db, "ALTER TABLE loop_step_executions ADD COLUMN conclusion TEXT").await;

        Ok(())
    }
}