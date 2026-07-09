//! 数据库迁移 V59：为 todos.archived_at 增加索引。
//!
//! ## 背景
//! 事项中心日常视图（`get_todos_by_workspace_id`）过滤 `archived_at IS NULL`，
//! 随归档量增长该过滤走全表扫描会变慢。设计文档建议加 `idx_todos_archived_at`，
//! 本迁移补上该索引（V58 仅加了列，未加索引）。
//!
//! ## 幂等
//! `CREATE INDEX IF NOT EXISTS`，已存在则静默跳过。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V59AddTodosArchivedAtIndex;

#[async_trait]
impl Migration for V59AddTodosArchivedAtIndex {
    fn version(&self) -> i64 {
        59
    }

    fn name(&self) -> &'static str {
        "add_todos_archived_at_index"
    }

    /// 在 archived_at 上建索引，加速「未归档事项」日常过滤。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        db.exec(
            "CREATE INDEX IF NOT EXISTS idx_todos_archived_at ON todos(archived_at)",
        )
        .await?;
        tracing::info!("V59: todos.archived_at 索引已创建");
        Ok(())
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::table_has_column;
    use crate::db::Database;

    /// 验证 V59 创建索引（通过查询 sqlite_master 间接确认）。
    #[tokio::test]
    async fn test_v59_creates_archived_at_index() {
        use sea_orm::ConnectionTrait;
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V59AddTodosArchivedAtIndex;
        migration.up(&db).await.expect("V59 migration must succeed");

        // 确认索引存在
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) AS cnt FROM sqlite_master \
                 WHERE type='index' AND name='idx_todos_archived_at'"
                    .to_string(),
            ))
            .await
            .expect("query must succeed")
            .expect("row must exist");
        let cnt: i64 = row.try_get_by("cnt").expect("cnt readable");
        assert_eq!(cnt, 1, "idx_todos_archived_at 索引应存在");
    }

    /// 幂等：重复执行不报错。
    #[tokio::test]
    async fn test_v59_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let migration = V59AddTodosArchivedAtIndex;
        migration.up(&db).await.expect("first run");
        migration.up(&db).await.expect("second run (idempotent)");
        // table_has_column 仅作占位断言，确认库仍可用
        assert!(table_has_column(&db, "todos", "archived_at").await.unwrap());
    }
}
