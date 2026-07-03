//! 数据库迁移 V47：创建 blackboards 表。
//!
//! 每个工作空间维护一个黑板记录，用于存储 LLM 自动生成的 Markdown 知识库内容。
//! workspace_id 为 UNIQUE，确保每个工作空间最多一条黑板记录。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, table_exists};

pub(super) struct V47CreateBlackboardsTable;

#[async_trait]
impl Migration for V47CreateBlackboardsTable {
    fn version(&self) -> i64 {
        47
    }

    fn name(&self) -> &'static str {
        "create_blackboards_table"
    }

    /// 创建 blackboards 表。
    ///
    /// 使用 CREATE TABLE IF NOT EXISTS 保证幂等性。
    /// workspace_id 设为 UNIQUE，每个工作空间只能有一条黑板记录。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 先检查表是否已存在，避免重复执行 CREATE TABLE 的日志噪声
        if table_exists(db, "blackboards").await? {
            return Ok(());
        }

        let sql = r#"
CREATE TABLE IF NOT EXISTS blackboards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id INTEGER NOT NULL UNIQUE,
    content TEXT NOT NULL DEFAULT '',
    updated_at TEXT,
    created_at TEXT,
    FOREIGN KEY (workspace_id) REFERENCES project_directories(id) ON DELETE CASCADE
);
"#;
        db.exec(sql).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 验证 V47 迁移可成功创建 blackboards 表。
    #[tokio::test]
    async fn test_v47_creates_blackboards_table() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V47CreateBlackboardsTable;
        migration.up(&db).await.expect("V47 migration must succeed");

        // 验证表已存在
        assert!(table_exists(&db, "blackboards").await.unwrap());
    }

    /// 验证 V47 迁移是幂等的（重复执行不会报错）。
    #[tokio::test]
    async fn test_v47_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V47CreateBlackboardsTable;
        migration.up(&db).await.expect("First run must succeed");
        migration.up(&db).await.expect("Second run must succeed (idempotent)");
    }
}
