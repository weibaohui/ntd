//! 迁移 v86：新增 process_template_versions 版本快照表（BUG-005）。
//!
//! 背景：`process versions` / `process diff` 按 guid 过滤 `process_templates`，
//! 但 process_templates 一行一 guid（当前版本），没有历史快照，
//! 导致 versions 恒 1 条、跨版本 diff 恒 404。
//!
//! 修复：保存工艺时把当前版本写入 `process_template_versions`（guid + version + definition），
//! versions/diff 从快照表读取。

use crate::db::{Database, migration::Migration};
use async_trait::async_trait;
use tracing::info;

pub struct V86ProcessTemplateVersions;

#[async_trait]
impl Migration for V86ProcessTemplateVersions {
    fn version(&self) -> i64 { 86 }
    fn name(&self) -> &'static str { "process_template_versions" }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        db.exec(
            "CREATE TABLE IF NOT EXISTS process_template_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                guid TEXT NOT NULL,
                version TEXT NOT NULL,
                definition TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(guid, version)
            )",
        )
        .await?;
        db.exec(
            "CREATE INDEX IF NOT EXISTS idx_process_template_versions_guid
             ON process_template_versions(guid)",
        )
        .await?;

        info!("v86: process_template_versions 版本快照表已创建");
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
    async fn test_v86_creates_versions_table() {
        let db = fresh_db().await;
        V86ProcessTemplateVersions.up(&db).await.expect("migration must succeed");

        // 验证表存在且有正确列
        let sql = db
            .conn
            .query_all(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT sql FROM sqlite_master WHERE name='process_template_versions'",
            ))
            .await
            .expect("query");
        let ddl: String = sql[0].try_get_by("sql").unwrap_or_default();
        assert!(ddl.contains("guid"));
        assert!(ddl.contains("version"));
        assert!(ddl.contains("definition"));
        assert!(ddl.contains("UNIQUE(guid, version)"));
    }
}
