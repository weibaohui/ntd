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
use tracing::info;

pub struct V85PhaseExecSetNull;

#[async_trait]
impl Migration for V85PhaseExecSetNull {
    fn version(&self) -> i64 { 85 }
    fn name(&self) -> &'static str { "fix_phase_exec_cascade" }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // SQLite 不支持 ALTER FOREIGN KEY，必须重建表。
        db.exec(
            "CREATE TABLE IF NOT EXISTS _loop_phase_executions_new (
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

        // 替换原表。
        db.exec("DROP TABLE IF EXISTS loop_phase_executions").await?;
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

        // 执行迁移：fresh_db 上没有旧表，v85 会创建新表
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
    }
}
