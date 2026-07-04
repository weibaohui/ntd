//! 数据库迁移 V53：创建 blackboard_pages 表。
//!
//! 黑板从单文件模式（blackboards.content）演进为多页面 Wiki 架构。
//! 每个工作空间维护一组页面（index / topic / log），由 LLM 和后端协作维护。
//!
//! 设计要点：
//! - (workspace_id, slug) 联合唯一：同一 workspace 内 slug 不重复
//! - page_type 用 TEXT 而非枚举：为后期 analysis 类型预留扩展空间
//! - source_refs 存 JSON 数组字符串：记录本页面整合了哪些执行结论

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, table_exists};

pub(super) struct V53CreateBlackboardPagesTable;

#[async_trait]
impl Migration for V53CreateBlackboardPagesTable {
    fn version(&self) -> i64 {
        53
    }

    fn name(&self) -> &'static str {
        "create_blackboard_pages_table"
    }

    /// 创建 blackboard_pages 表。
    ///
    /// 使用 CREATE TABLE IF NOT EXISTS 保证幂等性。
    /// (workspace_id, slug) 联合唯一约束确保同一 workspace 内 slug 不重复。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 先检查表是否已存在，避免重复执行 CREATE TABLE 的日志噪声
        if table_exists(db, "blackboard_pages").await? {
            return Ok(());
        }

        let sql = r#"
CREATE TABLE IF NOT EXISTS blackboard_pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id INTEGER NOT NULL,
    page_type TEXT NOT NULL,
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    source_refs TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT,
    created_at TEXT,
    FOREIGN KEY (workspace_id) REFERENCES project_directories(id) ON DELETE CASCADE,
    UNIQUE (workspace_id, slug)
);
"#;
        db.exec(sql).await?;

        tracing::info!("V53: blackboard_pages 表已创建");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 验证 V53 迁移可成功创建 blackboard_pages 表。
    #[tokio::test]
    async fn test_v53_creates_blackboard_pages_table() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V53CreateBlackboardPagesTable;
        migration.up(&db).await.expect("V53 migration must succeed");

        // 验证表已存在
        assert!(table_exists(&db, "blackboard_pages").await.unwrap());
    }

    /// 验证 V53 迁移是幂等的（重复执行不会报错）。
    #[tokio::test]
    async fn test_v53_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V53CreateBlackboardPagesTable;
        migration.up(&db).await.expect("First run must succeed");
        migration.up(&db).await.expect("Second run must succeed (idempotent)");
    }
}
