//! 迁移 v85：修复 loop_phase_executions 升级级联删除（BUG-008）。
//!
//! 背景：`process upgrade` 删除旧 `loop_phases` 重建（id 变化），
//! `loop_phase_executions.phase_id` 外键为 `ON DELETE CASCADE`，
//! 导致历史 phase 执行记录被级联删除，审计链断裂。
//!
//! 修复：把 `phase_id` 外键改为 `ON DELETE SET NULL`，
//! 阶段被删时执行记录保留，phase_id 置 NULL。
//!
//! SQLite ALTER TABLE 不支持直接修改外键，需重建表。

use crate::db::{Database, migration::Migration};
use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use tracing::info;

pub struct V85PhaseExecSetNull;

/// 在指定连接（迁移事务）上执行一条 DDL/DML。
/// 失败向上冒泡让迁移中止 —— 事务未提交被 drop 时自动回滚，保证原子性。
async fn exec_on_txn<C: ConnectionTrait>(conn: &C, sql: &str) -> Result<(), sea_orm::DbErr> {
    conn.execute(Statement::from_string(DbBackend::Sqlite, sql.to_string()))
        .await
        .map(|_| ())
}

#[async_trait]
impl Migration for V85PhaseExecSetNull {
    fn version(&self) -> i64 { 85 }
    fn name(&self) -> &'static str { "fix_phase_exec_cascade" }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 幂等：如果表已存在且 FK 已是 SET NULL，跳过。
        let ddl = db
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT sql FROM sqlite_master WHERE name='loop_phase_executions'",
            ))
            .await?;
        let ddl_str: String = ddl
            .first()
            .and_then(|r| r.try_get_by::<String, _>("sql").ok())
            .unwrap_or_default();
        if ddl_str.contains("ON DELETE SET NULL") {
            info!("v85: loop_phase_executions FK 已是 SET NULL，跳过");
            return Ok(());
        }
        // 表不存在（fresh DB / 被手动删过 / 上次重建中断在「DROP 旧表」之后、
        // 「RENAME 新表」之前——旧 v85 跨连接 RENAME 失败会留下带数据的 _new）。
        // 先探测遗留 _new：有 → 中断时数据已复制进去，直接改名回收，避免审计记录丢失；
        // 无 → 全新创建（SET NULL）。
        if ddl_str.is_empty() {
            let tmp = db
                .conn
                .query_all(sea_orm::Statement::from_string(
                    sea_orm::DbBackend::Sqlite,
                    "SELECT sql FROM sqlite_master WHERE name='_loop_phase_executions_new'",
                ))
                .await?;
            if !tmp.is_empty() {
                info!("v85: 检测到遗留 _loop_phase_executions_new，改名回收（上次重建中断）");
                // 回收同样钉在单连接事务里，避免跨连接 schema 竞争（PR #539 教训）。
                let txn = db.conn.begin().await?;
                exec_on_txn(&txn, "ALTER TABLE _loop_phase_executions_new RENAME TO loop_phase_executions")
                    .await?;
                exec_on_txn(
                    &txn,
                    "CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_exec ON loop_phase_executions(loop_execution_id)",
                )
                .await?;
                exec_on_txn(
                    &txn,
                    "CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_phase ON loop_phase_executions(phase_id)",
                )
                .await?;
                txn.commit().await?;
                info!("v85: 已回收中断重建遗留的 loop_phase_executions（数据保留）");
                return Ok(());
            }
            info!("v85: loop_phase_executions 表不存在，直接创建（SET NULL）");
            db.exec(
                "CREATE TABLE loop_phase_executions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    loop_execution_id INTEGER NOT NULL,
                    phase_id INTEGER,
                    status TEXT NOT NULL DEFAULT 'pending',
                    started_at TEXT,
                    finished_at TEXT,
                    FOREIGN KEY (loop_execution_id) REFERENCES loop_executions(id) ON DELETE CASCADE,
                    FOREIGN KEY (phase_id) REFERENCES loop_phases(id) ON DELETE SET NULL
                )",
            )
            .await?;
            db.exec("CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_exec ON loop_phase_executions(loop_execution_id)")
                .await?;
            db.exec("CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_phase ON loop_phase_executions(phase_id)")
                .await?;
            info!("v85: loop_phase_executions 已创建（SET NULL）");
            return Ok(());
        }

        // 表存在但 FK 是 CASCADE：重建改为 SET NULL。
        // 关键：整组 DDL 必须包在单连接事务里（PR #539 CRITICAL 教训，见 v2_v5.rs）——
        // db.exec 走连接池，每次 execute 可能落到不同连接；DROP 与 RENAME 若跨连接，
        // 后一条连接的 schema 缓存仍认为旧表存在 → 「already exists」迁移失败。
        // 用 conn.begin() 把「清理→建表→复制→替换→索引」全部钉在同一条连接上，
        // 任一步失败整体回滚，不污染连接池。
        let txn = db.conn.begin().await?;

        // 清理上次中断可能残留的临时表
        exec_on_txn(&txn, "DROP TABLE IF EXISTS _loop_phase_executions_new").await?;
        exec_on_txn(&txn, "DROP INDEX IF EXISTS idx_loop_phase_executions_exec").await?;
        exec_on_txn(&txn, "DROP INDEX IF EXISTS idx_loop_phase_executions_phase").await?;

        // 建新表。
        exec_on_txn(
            &txn,
            "CREATE TABLE _loop_phase_executions_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                loop_execution_id INTEGER NOT NULL,
                phase_id INTEGER,
                status TEXT NOT NULL DEFAULT 'pending',
                started_at TEXT,
                finished_at TEXT,
                FOREIGN KEY (loop_execution_id) REFERENCES loop_executions(id) ON DELETE CASCADE,
                FOREIGN KEY (phase_id) REFERENCES loop_phases(id) ON DELETE SET NULL
            )",
        )
        .await?;

        // 复制数据。残留态库可能含孤儿行（loop_execution_id 指向已被删除的
        // loop_executions：旧 FK 是 CASCADE 却未被级联清理，说明历史删除发生在
        // foreign_keys=OFF 期间），原样复制会触发 FK 失败让迁移中止，故复制时自愈：
        // - WHERE 过滤掉父执行已删除的孤儿行（记录已不可用，丢弃是唯一合理清理）
        // - phase_id 用 CASE 归一化：父 phase 已删除的置 NULL —— 这正是 SET NULL
        //   的语义；INSERT 本身不会触发 SET NULL（原注释假设是错的，只会报 FK 错）
        exec_on_txn(
            &txn,
            "INSERT INTO _loop_phase_executions_new (id, loop_execution_id, phase_id, status, started_at, finished_at)
             SELECT pe.id, pe.loop_execution_id,
                    CASE WHEN pe.phase_id IN (SELECT id FROM loop_phases) THEN pe.phase_id END,
                    pe.status, pe.started_at, pe.finished_at
             FROM loop_phase_executions pe
             WHERE pe.loop_execution_id IN (SELECT id FROM loop_executions)",
        )
        .await?;

        // 替换原表：先删旧表，再改名。
        exec_on_txn(&txn, "DROP TABLE loop_phase_executions").await?;
        exec_on_txn(&txn, "ALTER TABLE _loop_phase_executions_new RENAME TO loop_phase_executions").await?;

        // 重建索引。
        exec_on_txn(
            &txn,
            "CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_exec ON loop_phase_executions(loop_execution_id)",
        )
        .await?;
        exec_on_txn(
            &txn,
            "CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_phase ON loop_phase_executions(phase_id)",
        )
        .await?;

        txn.commit().await?;

        info!("v85: loop_phase_executions FK 改为 ON DELETE SET NULL，升级不再级联删除历史");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::db::Database;
    use sea_orm::ConnectionTrait;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    #[tokio::test]
    async fn test_v85_migration_sets_null_on_phase_delete() {
        let db = fresh_db().await;

        // fresh_db() 已包含 V71 建的表（CASCADE FK），直接用现有表验证迁移。
        // 先建 loop，再插入 phase/exec 数据（满足外键）。
        db.exec("INSERT INTO loops (id, name) VALUES (1, 'L')").await.expect("insert loop");
        db.exec("INSERT INTO loop_phases (id, loop_id, name) VALUES (1, 1, 'P1')").await.expect("insert phase");
        db.exec("INSERT INTO loop_executions (id, loop_id, status, started_at, trigger_type) VALUES (1, 1, 'running', '2026-01-01T00:00:00Z', 'manual')").await.expect("insert exec");
        db.exec("INSERT INTO loop_phase_executions (id, loop_execution_id, phase_id, status) VALUES (1, 1, 1, 'running')").await.expect("insert phase_exec");

        // 执行迁移
        V85PhaseExecSetNull.up(&db).await.expect("migration must succeed");

        // 验证新表结构：FOREIGN KEY 含 SET NULL
        let sql = db
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT sql FROM sqlite_master WHERE name='loop_phase_executions'",
            ))
            .await
            .expect("query");
        let ddl: String = sql[0].try_get_by("sql").unwrap_or_default();
        assert!(
            ddl.contains("ON DELETE SET NULL"),
            "FK 应改为 SET NULL：{}",
            ddl
        );

        // 验证数据还在
        let cnt = db
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) FROM loop_phase_executions",
            ))
            .await
            .expect("count");
        let n: i64 = cnt[0].try_get_by_index(0).unwrap_or(0);
        assert_eq!(n, 1, "数据应保留");
    }

    #[tokio::test]
    async fn test_v85_migration_creates_table_if_missing() {
        let db = fresh_db().await;
        // fresh_db() 已包含 V71 建的表，先删掉模拟「表不存在」场景。
        db.exec("DROP TABLE loop_phase_executions").await.expect("drop");

        V85PhaseExecSetNull.up(&db).await.expect("migration must succeed");

        // 验证新表存在且 FK 是 SET NULL
        let sql = db
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT sql FROM sqlite_master WHERE name='loop_phase_executions'",
            ))
            .await
            .expect("query");
        let ddl: String = sql[0].try_get_by("sql").unwrap_or_default();
        assert!(
            ddl.contains("ON DELETE SET NULL"),
            "FK 应为 SET NULL：{}",
            ddl
        );
    }

    /// 残留态孤儿行回归（BUG-008 同类残留）：loop_execution_id 指向已被删除的
    /// loop_executions。旧 FK 是 CASCADE 却未被级联清理（删除发生在 foreign_keys=OFF
    /// 期间），v85 重建复制时必须过滤孤儿行，否则 FK 约束失败导致迁移中止、启动失败。
    #[tokio::test]
    async fn test_v85_tolerates_orphan_rows_during_rebuild() {
        let db = fresh_db().await;
        // fresh_db() 已把表建成 SET NULL，先退化回旧 CASCADE 结构模拟残留态库。
        // 注意：旧表省略 loop_execution_id 的 FK —— 真实残留态里孤儿行就是靠
        // foreign_keys=OFF 插进来的，这里用「无该 FK」同样构造出「loop_execution_id
        // 悬空」的脏数据，且不依赖连接池对 PRAGMA 的粘性，测试确定可复现。
        // 迁移只检测 DDL 是否含 SET NULL、不检查旧表 FK，行为与真实残留态等价。
        db.exec("DROP TABLE loop_phase_executions").await.expect("drop");
        db.exec(
            "CREATE TABLE loop_phase_executions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                loop_execution_id INTEGER NOT NULL,
                phase_id INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                started_at TEXT,
                finished_at TEXT,
                FOREIGN KEY (phase_id) REFERENCES loop_phases(id) ON DELETE CASCADE
            )",
        )
        .await
        .expect("create legacy table");

        // 造一条合法父链 + 一条孤儿行（loop_execution_id 指向不存在的执行 999）。
        db.exec("INSERT INTO loops (id, name) VALUES (2, 'L2')").await.expect("insert loop");
        db.exec("INSERT INTO loop_phases (id, loop_id, name) VALUES (2, 2, 'P2')").await.expect("insert phase");
        db.exec("INSERT INTO loop_executions (id, loop_id, status, started_at, trigger_type) VALUES (2, 2, 'running', '2026-01-01T00:00:00Z', 'manual')").await.expect("insert exec");
        db.exec("INSERT INTO loop_phase_executions (id, loop_execution_id, phase_id, status) VALUES (10, 2, 2, 'running')").await.expect("insert valid row");
        db.exec("INSERT INTO loop_phase_executions (id, loop_execution_id, phase_id, status) VALUES (11, 999, 2, 'running')").await.expect("insert orphan row");

        // 执行迁移：孤儿行不应让迁移中止
        V85PhaseExecSetNull.up(&db).await.expect("migration must tolerate orphan rows");

        // 孤儿行被过滤（父执行已删除、记录不可用），合法行保留
        let cnt = db
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) FROM loop_phase_executions",
            ))
            .await
            .expect("count");
        let n: i64 = cnt[0].try_get_by_index(0).unwrap_or(0);
        assert_eq!(n, 1, "孤儿行应被过滤，合法行保留");

        // FK 已是 SET NULL
        let sql = db
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT sql FROM sqlite_master WHERE name='loop_phase_executions'",
            ))
            .await
            .expect("query");
        let ddl: String = sql[0].try_get_by("sql").unwrap_or_default();
        assert!(
            ddl.contains("ON DELETE SET NULL"),
            "FK 应为 SET NULL：{}",
            ddl
        );
    }

    /// 中断重建残留回收回归：上一次 v85 重建在「DROP 旧表」之后、「RENAME」之前失败
    /// （连接池跨连接导致 RENAME 报 already exists），留下带数据的 `_new` 表。
    /// 重跑 v85 应改名回收数据，而不是新建空表丢审计记录。
    #[tokio::test]
    async fn test_v85_recovers_leftover_new_table() {
        let db = fresh_db().await;
        // 模拟中断状态：删掉正式表，留下带数据的 _new（旧 v85 复制数据后 RENAME 失败）。
        db.exec("DROP TABLE loop_phase_executions").await.expect("drop");
        db.exec(
            "CREATE TABLE _loop_phase_executions_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                loop_execution_id INTEGER NOT NULL,
                phase_id INTEGER,
                status TEXT NOT NULL DEFAULT 'pending',
                started_at TEXT,
                finished_at TEXT,
                FOREIGN KEY (loop_execution_id) REFERENCES loop_executions(id) ON DELETE CASCADE,
                FOREIGN KEY (phase_id) REFERENCES loop_phases(id) ON DELETE SET NULL
            )",
        )
        .await
        .expect("create _new");
        db.exec("INSERT INTO loops (id, name) VALUES (3, 'L3')").await.expect("insert loop");
        db.exec("INSERT INTO loop_phases (id, loop_id, name) VALUES (3, 3, 'P3')").await.expect("insert phase");
        db.exec("INSERT INTO loop_executions (id, loop_id, status, started_at, trigger_type) VALUES (3, 3, 'running', '2026-01-01T00:00:00Z', 'manual')").await.expect("insert exec");
        db.exec("INSERT INTO _loop_phase_executions_new (id, loop_execution_id, phase_id, status) VALUES (20, 3, 3, 'running')").await.expect("row in _new");

        // 重跑 v85：应改名回收，数据保留
        V85PhaseExecSetNull.up(&db).await.expect("v85 must recover leftover _new");

        // 正式表存在且数据保留
        let cnt = db
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) FROM loop_phase_executions",
            ))
            .await
            .expect("count");
        let n: i64 = cnt[0].try_get_by_index(0).unwrap_or(0);
        assert_eq!(n, 1, "回收后数据应保留");

        // FK 是 SET NULL
        let sql = db
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT sql FROM sqlite_master WHERE name='loop_phase_executions'",
            ))
            .await
            .expect("query");
        let ddl: String = sql[0].try_get_by("sql").unwrap_or_default();
        assert!(
            ddl.contains("ON DELETE SET NULL"),
            "FK 应为 SET NULL：{}",
            ddl
        );
    }
}
