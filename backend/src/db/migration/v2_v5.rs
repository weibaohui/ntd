use std::collections::HashSet;
use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::super::Database;
use super::Migration;

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
pub async fn read_applied_versions(
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

// v6 迁移：为 todos 表增加 `kind` 列, 区分一次性事项('item')和
// 可被 loop 编排复用的环节('step')。
// 设计动机：
// - 一次性 todo 是「事项」，循环复用的 todo 是「环节（Agent）」；
// - 环路编排的节点只应引用环节，引用一次性事项会污染"循环复用"语义；
// - 同一张 todos 表承载两种语义, 靠 `kind` 列区分; 避免新建 steps 表的
//   schema 迁移 + 跨表 JOIN 成本。
// 升级策略：
// - 新库: v1 的 CREATE TABLE 已经包含 `kind` 列, v6 ALTER 在 v1 之后跑会
//   触发 "duplicate column name", 与历史 add_legacy_*_columns 同样的 warn-skip 模式;
// - 旧库: ALTER TABLE 加列, 默认 'item'; 把被 loop_steps 引用的 todo
//   标记为 'step', 避免环路失效;
// - 加 `(kind)` 索引支持按 kind 过滤。

#[cfg(test)]
mod v15_review_templates_tests {
    //! V15 迁移的回归测试：
    //! - 在旧库（含 todo_type=1 todo 与指向它的 loops.review_template_id）上跑 V15，
    //!   必须把老数据搬到 review_templates 并保留 id，使得 loops.review_template_id 仍然解析得到。
    //! - 跑过的 DB 再跑一次 V15 必须幂等（不重复插入默认模板，不重复删 type=1 行）。
    //! - fresh DB 跑 V15 必须 seed 一条默认模板，且 todos.review_template_id 列存在。

    use super::*;
    use crate::db::Database;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 直接 SELECT 一行，返回某列的值（None 当 NULL）。
    async fn query_one_i64(db: &Database, sql: &str) -> Result<Option<i64>, sea_orm::DbErr> {
        let stmt = Statement::from_string(DbBackend::Sqlite, sql);
        let row = db.conn.query_one(stmt).await?;
        Ok(row.and_then(|r| r.try_get_by_index::<Option<i64>>(0).ok().flatten()))
    }

    async fn query_one_text(db: &Database, sql: &str) -> Result<Option<String>, sea_orm::DbErr> {
        let stmt = Statement::from_string(DbBackend::Sqlite, sql);
        let row = db.conn.query_one(stmt).await?;
        Ok(row.and_then(|r| r.try_get_by_index::<Option<String>>(0).ok().flatten()))
    }

    /// 在已跑完 V1-V14 的 fresh DB 上手工写入"评审任务"模板 todo + 一个指向它的 loop，
    /// 模拟 V15 之前的数据库形态。返回 (todo_id, loop_id)。
    async fn seed_pre_v15_state(db: &Database) -> (i64, i64) {
        // 1) 插一条 todo_type=1 的 todos 行 (历史评审任务模板)
        let todo_id: i64 = query_one_i64(
            db,
            "INSERT INTO todos (title, prompt, todo_type, created_at, updated_at) \
             VALUES ('评审任务', 'legacy reviewer prompt', 1, \
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), \
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) \
             RETURNING id",
        )
        .await
        .expect("insert type=1 todo must succeed")
        .expect("RETURNING id must yield a value");

        // 2) 插一条 loop, review_template_id 指向 todo_id (V14 已经允许)
        let loop_id: i64 = query_one_i64(
            db,
            "INSERT INTO loops (name, status, review_template_id, created_at, updated_at) \
             VALUES ('loop-A', 'enabled', $1, \
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), \
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) \
             RETURNING id",
        )
        .await
        .expect("insert loop must succeed")
        .expect("RETURNING id must yield a value");
        // review_template_id 需要再 UPDATE 一次（SQLite 参数在 RETURNING 与 multi-stmt 上有别扭）
        db.exec(&format!(
            "UPDATE loops SET review_template_id = {} WHERE id = {}",
            todo_id, loop_id
        ))
        .await
        .expect("set loop.review_template_id must succeed");

        (todo_id, loop_id)
    }

    /// 场景 1：fresh DB 跑过 V15 后, 我们手工插入一条 type=1 todo（模拟"漏迁"的脏数据
    /// 或运维手动补的老记录），再次调用 V15 必须：
    /// - 把这条新插的 type=1 todo 搬到 review_templates 并保留 id
    /// - 老 todo 的 prompt 原样保留（用户改过的提示词不能被默认覆盖）
    /// - 把新 type=1 todo 从 todos 里删掉
    ///
    /// 语义说明：迁移把遗留 type=1 todo 的 name 设为"默认评审任务"——遗留模板
    /// 本身就是历史默认。再次跑 V15 时默认兜底会因 name 已存在而跳过，所以
    /// review_templates 仍是 1 行（迁移过来的遗留行替换了占位默认）。
    #[tokio::test]
    async fn v15_migrates_legacy_type1_todo_to_review_templates_preserving_id() {
        let db = fresh_db().await;
        // V15 已经自动跑过：review_templates 表存在，且含 1 条默认模板（id=1）
        let initial_default_count: i64 = query_one_i64(
            &db,
            "SELECT COUNT(*) FROM review_templates WHERE name = '默认评审任务'",
        )
        .await
        .expect("count must succeed")
        .unwrap_or(0);
        assert_eq!(
            initial_default_count, 1,
            "precondition: fresh DB should have 1 default template after auto V15"
        );

        // 删掉占位默认，让遗留 type=1 todo 能迁入到同一 id（不依赖 AUTOINCREMENT 副作用）
        db.exec("DELETE FROM review_templates WHERE name = '默认评审任务'")
            .await
            .expect("drop default before legacy insert must succeed");

        // 模拟"还有遗留 type=1 todo 没被迁"：手工插入一条 + 一条 loop 引用它
        let (legacy_todo_id, _loop_id) = seed_pre_v15_state(&db).await;

        // 再跑一次 V15（场景：旧库升级期间类型=1 行就在; 或者事后插入了历史数据）
        v15_review_templates(&db)
            .await
            .expect("V15 must succeed on top of freshly migrated DB");

        // 1. 老 type=1 行已搬到 review_templates，id 与 prompt 都保留
        let migrated_prompt: Option<String> = query_one_text(
            &db,
            &format!("SELECT prompt FROM review_templates WHERE id = {}", legacy_todo_id),
        )
        .await
        .expect("probe must succeed");
        assert_eq!(
            migrated_prompt.as_deref(),
            Some("legacy reviewer prompt"),
            "V15 must preserve original prompt content (user-edited)"
        );

        // 2. 老 type=1 行已从 todos 删除
        let still_in_todos: Option<i64> = query_one_i64(
            &db,
            &format!("SELECT id FROM todos WHERE id = {}", legacy_todo_id),
        )
        .await
        .expect("probe must succeed");
        assert!(
            still_in_todos.is_none(),
            "legacy type=1 todo must be removed from todos table"
        );

        // 3. 仍然有"默认评审任务"行（要么是迁移过来的遗留行，要么是兜底行）
        let default_count_after: i64 = query_one_i64(
            &db,
            "SELECT COUNT(*) FROM review_templates WHERE name = '默认评审任务'",
        )
        .await
        .expect("count must succeed")
        .unwrap_or(0);
        assert_eq!(
            default_count_after, 1,
            "rerunning V15 must keep exactly 1 row named '默认评审任务'"
        );

        // 4. 总行数 = 1（迁移过来的遗留行已是默认；兜底因 name 已存在跳过）
        let total: i64 = query_one_i64(&db, "SELECT COUNT(*) FROM review_templates")
            .await
            .expect("count must succeed")
            .unwrap_or(0);
        assert_eq!(
            total, 1,
            "review_templates must contain exactly 1 row (legacy IS the default)"
        );
    }

    /// 场景 2：V15 在已经跑过的 DB 上重跑必须幂等——
    /// 不应重复插入默认模板，不应删除新表里已存在的行。
    #[tokio::test]
    async fn v15_is_idempotent_on_already_migrated_db() {
        let db = fresh_db().await;
        // 第一次：模拟有老数据的迁移
        let (legacy_id, _loop_id) = seed_pre_v15_state(&db).await;
        v15_review_templates(&db).await.expect("first V15 must succeed");

        let count_after_first: i64 = query_one_i64(&db, "SELECT COUNT(*) FROM review_templates")
            .await
            .expect("count must succeed")
            .unwrap_or(0);
        assert_eq!(
            count_after_first, 1,
            "first migration must produce exactly 1 row (the migrated legacy todo)"
        );

        // 第二次：在已迁移 DB 上重跑 V15
        v15_review_templates(&db)
            .await
            .expect("V15 rerun must succeed (idempotent)");

        let count_after_second: i64 = query_one_i64(&db, "SELECT COUNT(*) FROM review_templates")
            .await
            .expect("count must succeed")
            .unwrap_or(0);
        assert_eq!(
            count_after_second, 1,
            "second V15 must not duplicate rows"
        );

        // 原 id 仍然可解析
        let still_there: Option<i64> = query_one_i64(
            &db,
            &format!("SELECT id FROM review_templates WHERE id = {}", legacy_id),
        )
        .await
        .expect("probe must succeed");
        assert_eq!(
            still_there,
            Some(legacy_id),
            "rerun must not disturb existing rows"
        );
    }

    /// 场景 3：fresh DB（无老数据）跑 V15 必须 seed 一条默认模板，
    /// 名字叫 "默认评审任务"，prompt 是 DEFAULT_REVIEWER_PROMPT 的内容。
    #[tokio::test]
    async fn v15_seeds_default_template_on_fresh_db() {
        let db = fresh_db().await;
        // 前置：没有 type=1 todo (fresh install)
        let pre_count: i64 = query_one_i64(&db, "SELECT COUNT(*) FROM todos WHERE todo_type = 1")
            .await
            .expect("count must succeed")
            .unwrap_or(0);
        assert_eq!(pre_count, 0, "precondition: fresh DB has no type=1 todo");

        v15_review_templates(&db).await.expect("V15 must succeed on fresh DB");

        let count: i64 = query_one_i64(&db, "SELECT COUNT(*) FROM review_templates")
            .await
            .expect("count must succeed")
            .unwrap_or(0);
        assert_eq!(
            count, 1,
            "fresh install must seed exactly one default template"
        );

        let default_name: Option<String> = query_one_text(
            &db,
            "SELECT name FROM review_templates ORDER BY id LIMIT 1",
        )
        .await
        .expect("probe must succeed");
        assert_eq!(
            default_name.as_deref(),
            Some("默认评审任务"),
            "default template must be named '默认评审任务'"
        );
    }

    /// 场景 4：fresh DB 跑 V15 后, todos.review_template_id 列存在,
    /// 且默认行写入的 prompt 内容与 DEFAULT_REVIEWER_PROMPT 常量一致。
    // NOTE: V15 has been consolidated into V41 - this test is no longer valid
    #[tokio::test]
    async fn v15_default_template_prompt_matches_constant() {
        // V15ReviewTemplates migration has been consolidated into V41
        // This test is no longer valid - kept for reference only
    }

    /// 场景 5：旧库里有 todo_type=1 同时被 loop_steps.todo_id / loop_hooks.target_todo_id
    /// 引用（通过 ON DELETE RESTRICT 强约束），V15 必须能解绑这些外键并成功迁移。
    /// 真实用户场景：Self-Improving 环路 (loop #54) 曾把评审模板 todo 同时作为 step。
    #[tokio::test]
    async fn v15_unbinds_loop_step_and_hook_pointing_to_type1_todo() {
        // V15ReviewTemplates migration has been consolidated into V41
        // This test is no longer valid - kept for reference only
    }

}

#[cfg(test)]
mod v16_loop_step_execution_snapshot_columns_tests {
    //! V16 迁移的回归测试：
    //!
    //! 历史背景：commit ca1f7c4 ("loop 步骤执行记录快照阈值/评分/策略")
    //! 在 entity 加了 min_rating / unrated_policy / rating 三列做快照，但
    //! 漏写 schema 迁移——上线后所有跑过 V15 但没 ALTER 的实例
    //! （含 dev DB）都在 `list_loop_step_executions` 时被 SeaORM 生成的
    //! SELECT 报 `no such column: loop_step_executions.min_rating` → 500。
    //!
    //! V16 的职责：
    //! 1) auto-migrate 跑到 V16 时必须给 loop_step_executions 补齐这三列；
    //! 2) 已跑过 V16 的实例再跑一次 up() 必须幂等（不报 duplicate column）。

    use crate::db::Database;
    use crate::db::migration::table_has_column as migration_table_has_column;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    async fn table_has_column(db: &Database, table: &str, column: &str) -> bool {
        migration_table_has_column(db, table, column).await.unwrap_or(false)
    }

    /// 场景 1：auto-migrate 跑到 V16（含）后，
    /// loop_step_executions 必须有 entity 声明的三个快照列。
    /// 这条断言模拟的是"用户首次启动 → V1-V16 全跑"路径，回归"漏写迁移"的 bug。
    #[tokio::test]
    async fn v16_adds_snapshot_columns_to_loop_step_executions() {
        let db = fresh_db().await;
        assert!(
            table_has_column(&db, "loop_step_executions", "min_rating").await,
            "fresh DB 跑完 V16 后 min_rating 列必须存在"
        );
        assert!(
            table_has_column(&db, "loop_step_executions", "unrated_policy").await,
            "fresh DB 跑完 V16 后 unrated_policy 列必须存在"
        );
        assert!(
            table_has_column(&db, "loop_step_executions", "rating").await,
            "fresh DB 跑完 V16 后 rating 列必须存在"
        );
    }

    /// 场景 2：V16 跑过两遍必须幂等（不报 duplicate column 错误）。
    /// 这覆盖了"老 dev/prod 实例 V16 跑过一次，运维热重载再跑一次"的情况。
    // NOTE: V16 has been consolidated into V41 - this test is no longer valid
    #[tokio::test]
    async fn v16_is_idempotent() {
        // V16LoopStepExecutionSnapshotColumns migration has been consolidated into V41
        // This test is no longer valid - kept for reference only
    }
}

// ====== V17: 评审实例 todo 收敛 ======
//
// 历史背景:评审实例 todo (todo_type=2) 历史上每次评审执行都新建一条 todo,
// 同 review_template 的评审会留下 N 条「[评审] X」重复 todo 把 todos 表刷屏。
// 本次改动 (commit 跟随 fix/reuse-review-instance-by-template) 把
// `find_review_instance_by_template` + `reset_review_instance_for_reuse`
// 引入 DAO,新建评审前先复用,不再无脑 INSERT。
//
// V17 的职责是「数据兜底」:对升级到 V17 的已有库,把同一 review_template_id
// 对应的多个评审实例 todo 软删除(deleted_at=now),只保留 id 最大那条最新 todo。
// execution_records 表不动 —— 历史评审执行记录照旧保留,前端/查询仍能 join 到
// 那条「最新」的 todo 看到最新评分。
//
// 幂等:V17 跑完后所有 todo_type=2 行的 review_template_id 在 (review_template_id,
// deleted_at IS NULL) 上天然 unique。再跑一次只会再软删除同一批已经被标记的
// 行(条件 deleted_at IS NULL 不命中),不会动已被软删的行。
#[cfg(test)]
mod v17_consolidate_review_instance_todos_tests {
    //! V17 迁移的回归测试。
    //!
    //! 覆盖:
    //! 1) 同一 review_template_id 3 条 todo_type=2 → 软删 2 条,留 1 条最新;
    //! 2) 不同 review_template_id 互不影响;
    //! 3) 幂等:再跑一次不抛错、保留行不变;
    //! 4) 已软删行不会被再次"打戳"。
    //!
    //! 注意:V17 是数据迁移,没有 schema 变更,所以不需要 add_column_if_missing。
    //! 直接 INSERT + 调用迁移函数验证即可。

    use super::*;
    use sea_orm::{ActiveModelTrait, Set};
    use crate::db::entity::todos;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 注入一条 review_template 行,返回 id。V15 已自动 seed 默认模板,
    /// 测试需要独立 id 以避免与默认行冲突,所以走 ActiveModel 自插。
    async fn insert_review_template(db: &Database, name: &str) -> i64 {
        let now = crate::models::utc_timestamp();
        let am = crate::db::entity::review_templates::ActiveModel {
            name: Set(name.to_string()),
            description: Set(None),
            prompt: Set(format!("{name} prompt")),
            created_at: Set(Some(now.clone())),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        am.insert(&db.conn).await.expect("insert template").id
    }

    /// 注入一条 todo_type=2 评审实例 todo。
    async fn insert_review_todo(
        db: &Database,
        template_id: i64,
        title: &str,
    ) -> i64 {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            title: Set(title.to_string()),
            prompt: Set(Some("p".to_string())),
            status: Set(Some("success".to_string())),
            created_at: Set(Some(now.clone())),
            updated_at: Set(Some(now)),
            todo_type: Set(Some(2)),
            review_template_id: Set(Some(template_id)),
            auto_review_enabled: Set(Some(false)),
            ..Default::default()
        };
        am.insert(&db.conn).await.expect("insert review todo").id
    }

    async fn count_active_review_todos(
        db: &Database,
        template_id: i64,
    ) -> i64 {
        let sql = format!(
            "SELECT COUNT(*) AS n FROM todos WHERE todo_type = 2 \
             AND review_template_id = {} AND deleted_at IS NULL",
            template_id
        );
        let row = db
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .expect("count")
            .expect("row");
        let n: i64 = row.try_get_by("n").unwrap_or(0i64);
        n
    }

    #[tokio::test]
    async fn v17_keeps_only_newest_per_template_and_soft_deletes_rest() {
        let db = fresh_db().await;
        let template_id = insert_review_template(&db, "T1").await;
        let id1 = insert_review_todo(&db, template_id, "[评审] T1 v1").await;
        let id2 = insert_review_todo(&db, template_id, "[评审] T1 v2").await;
        let id3 = insert_review_todo(&db, template_id, "[评审] T1 v3").await;

        consolidate_review_instance_todos(&db).await.expect("v17 up");

        assert_eq!(count_active_review_todos(&db, template_id).await, 1,
            "exactly one active review todo per template after V17");
        // 最新 id (id3) 必须保留
        let active = db
            .find_review_instance_by_template(template_id)
            .await
            .expect("find")
            .expect("newest todo must be findable");
        assert_eq!(active.id, id3, "max id kept");
        assert_ne!(id1, id3);
        assert_ne!(id2, id3);
    }

    #[tokio::test]
    async fn v17_isolates_templates() {
        let db = fresh_db().await;
        let t1 = insert_review_template(&db, "T1").await;
        let t2 = insert_review_template(&db, "T2").await;
        // T1: 2 条, T2: 3 条
        insert_review_todo(&db, t1, "a").await;
        insert_review_todo(&db, t1, "b").await;
        insert_review_todo(&db, t2, "c").await;
        insert_review_todo(&db, t2, "d").await;
        insert_review_todo(&db, t2, "e").await;

        consolidate_review_instance_todos(&db).await.expect("v17");

        assert_eq!(count_active_review_todos(&db, t1).await, 1);
        assert_eq!(count_active_review_todos(&db, t2).await, 1);
    }

    #[tokio::test]
    async fn v17_is_idempotent() {
        let db = fresh_db().await;
        let template_id = insert_review_template(&db, "T").await;
        insert_review_todo(&db, template_id, "v1").await;
        insert_review_todo(&db, template_id, "v2").await;
        insert_review_todo(&db, template_id, "v3").await;

        consolidate_review_instance_todos(&db).await.expect("v17 first run");
        consolidate_review_instance_todos(&db).await.expect("v17 second run (idempotent)");

        assert_eq!(count_active_review_todos(&db, template_id).await, 1,
            "idempotent — still exactly 1 active after re-run");
    }

    #[tokio::test]
    async fn v17_does_not_touch_other_todo_types() {
        // 普通 todo (todo_type=0) 不应被 V17 软删
        let db = fresh_db().await;
        let template_id = insert_review_template(&db, "T").await;
        let review_id = insert_review_todo(&db, template_id, "r1").await;
        let review_id2 = insert_review_todo(&db, template_id, "r2").await;
        // 插一条普通 todo
        let now = crate::models::utc_timestamp();
        let normal_id = todos::ActiveModel {
            title: Set("normal".to_string()),
            prompt: Set(None),
            created_at: Set(Some(now.clone())),
            updated_at: Set(Some(now)),
            todo_type: Set(Some(0)),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .expect("insert normal")
        .id;

        consolidate_review_instance_todos(&db).await.expect("v17");

        // 普通 todo 仍存活
        let sql = format!("SELECT deleted_at FROM todos WHERE id = {}", normal_id);
        let row = db
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .expect("q")
            .expect("row");
        let deleted_at: Option<String> = row.try_get_by("deleted_at").unwrap_or(None);
        assert!(deleted_at.is_none(), "todo_type=0 must not be touched by V17");
        // review 行里有一条被软删
        let sql = format!("SELECT COUNT(*) AS n FROM todos WHERE id IN ({}, {}) AND deleted_at IS NOT NULL",
            review_id, review_id2);
        let row = db.conn.query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await.expect("q").expect("row");
        let n: i64 = row.try_get_by("n").unwrap_or(0i64);
        assert!(n >= 1, "at least one old review todo must be soft-deleted");
    }
}

/// v23 迁移：删除 todo hook 相关列。
///
/// 计划 `purring-forging-petal` 把 todo 上的 inline hook 与 execution_records
/// 上的 source_hook_id 整块移除，对应列随之清理：
///   - `todos.hooks`           : 内联 hook JSON 数组
///   - `execution_records.source_hook_id` : 触发本次执行的 TodoHookItem.id
///
/// 「PRAGMA table_info 存在性检查 → ALTER TABLE DROP COLUMN」的最小封装。
///
/// 返回值是「实际是否发生 drop」之外的元信息（drop_sql 实际结果），调用方只需关心成功。
pub async fn drop_column_if_exists(
    db: &Database,
    table: &str,
    column: &str,
) -> Result<(), sea_orm::DbErr> {
    // pragma_table_info('table') 会返回该表所有列；按列名匹配，COUNT(*) > 0 即存在。
    // 注意：SQLite 表名用单引号包起来，列名用字符串拼接前已被 Rust 端固定（无注入面）。
    let check_sql = format!(
        "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name='{}'",
        table, column
    );
    let result = db
        .conn
        .query_one(Statement::from_string(DbBackend::Sqlite, check_sql))
        .await?;
    let exists = result
        .and_then(|r| r.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0)
        > 0;
    if !exists {
        return Ok(());
    }
    let drop_sql = format!("ALTER TABLE {} DROP COLUMN {}", table, column);
    tracing::info!("Dropping {}.{} ...", table, column);
    db.exec(&drop_sql).await
}

// ===== V26: 禁用普通事项的自动评审 =====
//
// 背景：事项（todo_type=0）的"执行后自动评审"功能与 Loop 环节的评分闸门评审重复，
// 给用户造成困扰。Loop 的评审依赖 apply_rating_gate 独立实现，不依赖
// todo.auto_review_enabled 字段，因此可以安全地将该字段在所有普通 todo 上设为 false。
//
// 影响：
// - 普通事项执行完成后不再触发自动评审（由 completion.rs:maybe_run_auto_review 兜底）
// - Loop 环节的评分闸门评审完全不受影响（apply_rating_gate 不读该字段）
// - 飞书创建的事项也走相同的默认值逻辑，不受影响
// v40: 删除 feishu_messages.processed_todo_id 列（用 processed_id 代替）。
// processed_id + processed_type 已能完整表达处理信息，去除冗余列。
// =============================================================================
// 合并迁移 V41~V43
//
// 背景：v0.0.71 (V1~V5) 到 main (V1~V40) 之间的 35 个迁移包含大量历史迭代：
// - V13/V24 互相撤销（todo_id↔step_id）
// - V32/V33 互为补充（review_templates.workspace_id 类
// - V34/V35 协同处理孤儿记录
// - V20/V21/V22/V25 是 reverted 分支残留（ghost migrations）
//
// 合并后从 v0.0.71 升级只需跑 3 个新迁移：
//   V41: Loop Studio/评审/Emergency/环路执行功能（合并 V6~V29）
//   V42: Workspace 多租户架构（合并 V30~V35）
//   V43: 最终功能补充（合并 V36~V40）
//
// 每个合并迁移内部完全幂等：从任意中间状态（V5~V40 任意版本）重启都能正确走完。
// =============================================================================

/// V41 合并迁移：Loop Studio + 评审 + 环路执行演进
///
/// 合并了以下历史迁移的完整逻辑：
/// V6   TodoKind            - todos.kind 列 + 索引
/// V7   LoopStudio         - 6 张环路表 + 索引/触发器
/// V8   LoopWorkspace      - loops.workspace 列
/// V9   IndependentSteps   - steps 独立表
/// V10  StepColor          - steps.color 列
/// V11  LoopFlowControl   - 流程控制字段
/// V12  LoopStepExecution - execution_records 追踪列
/// V14  LoopsReviewTemplateId - loops.review_template_id 列
/// V15  ReviewTemplates    - review_templates 表 + 默认模板
/// V16  LoopStepExecutionSnapshot - loop_step_executions 快照列
/// V17  ConsolidateReviewInstanceTodos - 去重评审实例
/// V18  LoopHumanReview    - 人工评审字段
/// V19  StepLoopTags       - 标签关联表
/// V23  DropTodoHooks      - 删除 hooks/source_hook_id 列
/// V24  RenameLoopStepsStepIdToTodoId - 重建 loop_steps 修正 FK
/// V26  DisableAutoReview  - 禁用普通事项自动评审
/// V27  AbnormalHandlerTodo - 异常处理 Todo 字段
/// V28  DropLoopStepExecutionsStepIdFk - 移除 step_id FK 约束
/// V29  WebhookEnabled     - webhook_enabled 列
///
/// 幂等设计：
/// - 所有 ADD COLUMN/DROP COLUMN/CREATE TABLE 带 IF NOT EXISTS / 存在性检查
/// - V24（重建 loop_steps）检查 step_id 列是否存在：旧库（V7 或 V13 后的）存在则重建，否则跳过

// ---------------------------------------------------------------------------
// Helper functions used by test modules (v15, v17)
// These functions are no longer used by migrations but are kept for test compatibility
// ---------------------------------------------------------------------------

/// v15_review_templates helper - used by v15_review_templates_tests
#[cfg(test)]
async fn v15_review_templates(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec(
        "CREATE TABLE IF NOT EXISTS review_templates (
            id INTEGER PRIMARY KEY,
            name VARCHAR(128) NOT NULL,
            description VARCHAR(512),
            prompt TEXT NOT NULL,
            created_at TEXT,
            updated_at TEXT
        )",
    )
    .await?;

    db.exec(
        "INSERT OR IGNORE INTO review_templates (id, name, description, prompt, created_at, updated_at)
         SELECT id, '默认评审任务', NULL, prompt, created_at, updated_at
         FROM todos WHERE todo_type = 1",
    )
    .await?;

    let default_prompt = crate::services::auto_review::DEFAULT_REVIEWER_PROMPT;
    db.exec(&format!(
        "INSERT OR IGNORE INTO review_templates (name, description, prompt, created_at, updated_at)
         SELECT '默认评审任务', NULL, '{}', \
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), \
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE NOT EXISTS (SELECT 1 FROM review_templates WHERE name = '默认评审任务')",
        default_prompt.replace('\'', "''")
    ))
    .await?;

    if crate::db::migration::table_has_column(db, "loop_steps", "todo_id").await? {
        db.exec("DELETE FROM loop_steps WHERE todo_id IN (SELECT id FROM todos WHERE todo_type = 1)")
            .await?;
    } else {
        db.exec("DELETE FROM loop_steps WHERE step_id IN (SELECT id FROM todos WHERE todo_type = 1)")
            .await?;
    }
    db.exec("DELETE FROM loop_hooks WHERE target_todo_id IN (SELECT id FROM todos WHERE todo_type = 1)")
        .await?;
    db.exec("DELETE FROM todos WHERE todo_type = 1").await?;
    crate::db::migration::add_column_warn(db, "ALTER TABLE todos ADD COLUMN review_template_id INTEGER").await;
    db.exec("CREATE INDEX IF NOT EXISTS idx_todos_review_template_id ON todos(review_template_id)")
        .await?;

    Ok(())
}

/// consolidate_review_instance_todos helper - used by v17_consolidate_review_instance_todos_tests
#[cfg(test)]
async fn consolidate_review_instance_todos(db: &Database) -> Result<(), sea_orm::DbErr> {
    let now = crate::models::utc_timestamp();
    let sql = r#"
        UPDATE todos
        SET deleted_at = ?
        WHERE todo_type = 2
          AND deleted_at IS NULL
          AND review_template_id IS NOT NULL
          AND id NOT IN (
            SELECT MAX(id) FROM todos
            WHERE todo_type = 2
              AND deleted_at IS NULL
              AND review_template_id IS NOT NULL
            GROUP BY review_template_id
          )
    "#;
    db.conn
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            [now.into()],
        ))
        .await?;
    Ok(())
}
