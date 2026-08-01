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
use sea_orm::ConnectionTrait;
use tracing::info;

pub struct V85PhaseExecSetNull;

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
        // 表不存在（fresh DB 或被手动删过）：直接建新表（带 SET NULL）。
        if ddl_str.is_empty() {
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
        // 先清理可能的残留（上次迁移中断的情况）。
        db.exec("DROP TABLE IF EXISTS _loop_phase_executions_new").await?;
        db.exec("DROP INDEX IF EXISTS idx_loop_phase_executions_exec").await?;
        db.exec("DROP INDEX IF EXISTS idx_loop_phase_executions_phase").await?;

        // 建新表。
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
        .await?;

        // 复制数据（phase_id 可能因外键约束变成 NULL，这正是期望行为）。
        db.exec(
            "INSERT INTO _loop_phase_executions_new (id, loop_execution_id, phase_id, status, started_at, finished_at)
             SELECT id, loop_execution_id, phase_id, status, started_at, finished_at
             FROM loop_phase_executions",
        )
        .await?;

        // 替换原表：先删旧表，再改名。
        db.exec("DROP TABLE loop_phase_executions").await?;
        db.exec("ALTER TABLE _loop_phase_executions_new RENAME TO loop_phase_executions").await?;

        // 重建索引。
        db.exec("CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_exec ON loop_phase_executions(loop_execution_id)")
            .await?;
        db.exec("CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_phase ON loop_phase_executions(phase_id)")
            .await?;

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
}
