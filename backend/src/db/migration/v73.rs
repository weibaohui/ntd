//! V73 迁移：任务管理 — 新建 tasks 表 + loops/loop_executions 新增 task_id。
use sea_orm::{ConnectionTrait, Statement};
use crate::db::migration::Migration;
use crate::db::Database;

pub(super) struct V73TaskManagement;

#[async_trait::async_trait]
impl Migration for V73TaskManagement {
    fn version(&self) -> i64 { 73 }
    fn name(&self) -> &'static str { "V73TaskManagement" }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        create_tasks_table(db).await?;
        add_column_if_missing(db, "loop_executions", "task_id", "INTEGER").await?;
        Ok(())
    }
}

async fn create_tasks_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.conn.execute(Statement::from_string(sea_orm::DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'pending',
            workspace_id INTEGER,
            template_id INTEGER,
            loop_id INTEGER,
            created_by TEXT DEFAULT '',
            created_at TEXT,
            updated_at TEXT
        )"
    )).await?;
    Ok(())
}

async fn add_column_if_missing(db: &Database, table: &str, col: &str, col_type: &str) -> Result<(), sea_orm::DbErr> {
    let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name='{}'", table, col);
    let rows = db.conn.query_all(Statement::from_string(sea_orm::DbBackend::Sqlite, sql)).await?;
    let exists = rows.first().and_then(|r| r.try_get_by::<i64,_>("n").ok()).map(|n| n>0).unwrap_or(false);
    if !exists {
        db.conn.execute(Statement::from_string(sea_orm::DbBackend::Sqlite,
            format!("ALTER TABLE {} ADD COLUMN {} {}", table, col, col_type)
        )).await?;
    }
    Ok(())
}
