use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing, add_column_warn, add_column_with_fallback};

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
//     todo_templates / usage_* / sync_records)
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
            webhook_enabled INTEGER NOT NULL DEFAULT 0,
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
            execution_record_id INTEGER,
            workspace_id INTEGER,
            processed_id INTEGER,
            processed_type TEXT,
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
/// 3 条(processed_todo_id / execution_record_id / workspace_id)在 IF NOT EXISTS 失败时回退。
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
    .await?;
    add_column_with_fallback(
        db,
        "ALTER TABLE feishu_messages ADD COLUMN IF NOT EXISTS workspace_id INTEGER",
        "ALTER TABLE feishu_messages ADD COLUMN workspace_id INTEGER",
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
// ---------------- 内部 helpers (已移至 mod.rs) ----------------
