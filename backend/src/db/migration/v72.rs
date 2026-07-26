//! V72 迁移：M3 — 工艺版本管理 + 四流闭预留表。
//!
//! 变更：
//! 1. `process_templates` 新增 `previous_version_id`（版本链）。
//! 2. 新建 `insight_events` 表（洞察流，AI 生成的改进建议）。
//! 3. 新建 `governance_rules` 表（治理流，组织级规则）。
//! 4. 新建 `asset_evolution` 表（资产流，skill/模板版本进化链）。

use sea_orm::{ConnectionTrait, Statement};

use crate::db::migration::Migration;
use crate::db::Database;

/// V72：工艺管理 M3 — 版本链 + 四流闭预留表。
pub(super) struct V72ProcessManagementV2;

#[async_trait::async_trait]
impl Migration for V72ProcessManagementV2 {
    fn version(&self) -> i64 {
        72
    }

    fn name(&self) -> &'static str {
        "V72ProcessManagementV2"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        migrate(db).await
    }
}

async fn migrate(db: &Database) -> Result<(), sea_orm::DbErr> {
    add_previous_version_id(db).await?;
    // V71 遗漏：process_step_templates 需要 category 列（spec §8.1 数据模型定义）。
    add_column_if_missing(db, "process_step_templates", "category", "TEXT NOT NULL DEFAULT 'general'").await?;
    create_insight_events_table(db).await?;
    create_governance_rules_table(db).await?;
    create_asset_evolution_table(db).await?;
    Ok(())
}

/// 为 `process_templates` 新增版本链字段。
async fn add_previous_version_id(db: &Database) -> Result<(), sea_orm::DbErr> {
    add_column_if_missing(
        db,
        "process_templates",
        "previous_version_id",
        "INTEGER",
    )
    .await
}

/// 创建 `insight_events` 表（洞察流）。
async fn create_insight_events_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.conn
        .execute(Statement::from_string(
            sea_orm::DbBackend::Sqlite,
            "CREATE TABLE IF NOT EXISTS insight_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                loop_execution_id INTEGER NOT NULL,
                phase_id INTEGER,
                step_id INTEGER,
                insight_type TEXT DEFAULT 'improvement',
                content TEXT NOT NULL DEFAULT '',
                generated_by TEXT DEFAULT 'ai',
                created_at TEXT
            )",
        ))
        .await?;
    Ok(())
}

/// 创建 `governance_rules` 表（治理流）。
async fn create_governance_rules_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.conn
        .execute(Statement::from_string(
            sea_orm::DbBackend::Sqlite,
            "CREATE TABLE IF NOT EXISTS governance_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL DEFAULT '',
                rule_type TEXT DEFAULT 'rework_threshold',
                config TEXT NOT NULL DEFAULT '{}',
                enabled INTEGER DEFAULT 1,
                created_at TEXT,
                updated_at TEXT
            )",
        ))
        .await?;
    Ok(())
}

/// 创建 `asset_evolution` 表（资产流）。
async fn create_asset_evolution_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.conn
        .execute(Statement::from_string(
            sea_orm::DbBackend::Sqlite,
            "CREATE TABLE IF NOT EXISTS asset_evolution (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                asset_type TEXT NOT NULL DEFAULT 'process_template',
                asset_name TEXT NOT NULL DEFAULT '',
                from_version TEXT,
                to_version TEXT NOT NULL DEFAULT '',
                change_summary TEXT DEFAULT '',
                created_at TEXT
            )",
        ))
        .await?;
    Ok(())
}

/// 如果列不存在则添加（幂等）。
async fn add_column_if_missing(
    db: &Database,
    table: &str,
    column: &str,
    col_type: &str,
) -> Result<(), sea_orm::DbErr> {
    let sql = format!(
        "SELECT COUNT(*) AS n FROM pragma_table_info('{}') WHERE name = '{}'",
        table, column
    );
    let rows = db
        .conn
        .query_all(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
        .await?;
    let exists = rows
        .first()
        .and_then(|r| r.try_get_by::<i64, _>("n").ok())
        .map(|n| n > 0)
        .unwrap_or(false);

    if !exists {
        let ddl = format!(
            "ALTER TABLE {} ADD COLUMN {} {}",
            table, column, col_type
        );
        db.conn
            .execute(Statement::from_string(sea_orm::DbBackend::Sqlite, ddl))
            .await?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
mod tests {
    use super::*;
    use crate::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    #[tokio::test]
    async fn test_migrate_v72_idempotent() {
        let db = fresh_db().await;
        // 1. 先建 process_templates 表（通常由过往 migration 创建）。
        db.conn
            .execute(Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "CREATE TABLE IF NOT EXISTS process_templates (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL DEFAULT '',
                    display_name TEXT NOT NULL DEFAULT '',
                    description TEXT NOT NULL DEFAULT '',
                    category TEXT NOT NULL DEFAULT '',
                    complexity TEXT NOT NULL DEFAULT 'standard',
                    version TEXT NOT NULL DEFAULT '1.0.0',
                    definition TEXT NOT NULL DEFAULT '',
                    source_path TEXT,
                    workspace_id INTEGER,
                    is_system INTEGER DEFAULT 0,
                    created_at TEXT,
                    updated_at TEXT
                )",
            ))
            .await
            .unwrap();

        // 2. 首次迁移。
        V72ProcessManagementV2.up(&db).await.unwrap();

        // 验证列已添加。
        let row = db
            .conn
            .query_one(Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) AS n FROM pragma_table_info('process_templates') WHERE name = 'previous_version_id'",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get_by::<i64, _>("n").unwrap(), 1);

        // 验证新表已创建。
        let tables = db
            .conn
            .query_all(Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('insight_events','governance_rules','asset_evolution')",
            ))
            .await
            .unwrap();
        assert_eq!(tables.len(), 3);

        // 3. 二次迁移幂等（不报错）。
        V72ProcessManagementV2.up(&db).await.unwrap();
    }
}
