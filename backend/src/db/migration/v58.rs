//! 数据库迁移 V58：为 todos 表新增 archived_at 字段。
//!
//! ## 背景
//! 事项中心改版（PR #875）引入「已归档」分类：用户希望把不再日常关注的事项
//! 从默认视图隐藏，但不删除数据、不破坏执行记录与 Loop 引用关系。
//! 现状只有软删除 `deleted_at`（会连带清理 scheduler/在跑 task），无法表达
//! 「仅隐藏、可恢复」的归档语义，因此新增独立的 `archived_at` 列。
//!
//! ## 语义
//! - `archived_at = NULL`：未归档，参与事项中心日常分类。
//! - `archived_at != NULL`：已归档，进入「已归档」分类，从日常视图隐藏。
//!
//! 归档只表达「用户希望隐藏」的时间点，不等于删除、停用或解除 Loop 引用。
//!
//! ## 幂等设计
//! 复用 `add_column_if_missing`，列已存在则静默跳过；新装库跑本迁移也只是 noop。
//! 新列允许 NULL：存量行自动为 NULL（即「未归档」），迁移后行为零变化。
//!
//! ## 不加索引
//! 第一版数据量有限，`archived_at IS NULL` 过滤走全表扫描即可；
//! 复合索引 `(workspace_id, archived_at)` 待实际查询性能有压力再加。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V58AddTodosArchivedAt;

#[async_trait]
impl Migration for V58AddTodosArchivedAt {
    fn version(&self) -> i64 {
        58
    }

    fn name(&self) -> &'static str {
        "add_todos_archived_at"
    }

    /// 为 todos 表追加 archived_at 列。
    ///
    /// 允许 NULL：存量行默认 NULL（未归档），保证迁移后日常视图行为不变。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "todos",
            "archived_at",
            // TEXT 存 UTC 时间戳字符串，与 created_at/updated_at/deleted_at 一致
            "ALTER TABLE todos ADD COLUMN archived_at TEXT",
        )
        .await?;
        tracing::info!("V58: todos.archived_at 列已添加");
        Ok(())
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::table_has_column;
    use crate::db::Database;

    /// 验证 V58 迁移成功添加 archived_at 列。
    /// 目的：确保列在全新库上被正确追加。
    #[tokio::test]
    async fn test_v58_adds_archived_at_column() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V58AddTodosArchivedAt;
        migration.up(&db).await.expect("V58 migration must succeed");

        assert!(
            table_has_column(&db, "todos", "archived_at")
                .await
                .unwrap(),
            "archived_at column must exist after V58"
        );
    }

    /// 验证 V58 迁移是幂等的（重复执行不报错）。
    /// 防止 schema_version 与 m.up 不一致时下次启动重跑失败。
    #[tokio::test]
    async fn test_v58_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V58AddTodosArchivedAt;
        migration.up(&db).await.expect("First run must succeed");
        migration.up(&db).await.expect("Second run must succeed (idempotent)");
    }

    /// 验证 V58 新列默认 NULL（即存量行「未归档」），行为零变化。
    /// 这是归档语义的基石：迁移不能把存量事项悄悄变成已归档。
    #[tokio::test]
    async fn test_v58_legacy_rows_default_null() {
        use sea_orm::ConnectionTrait;
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        // 先插一条 todo（此时还没有 archived_at 列），模拟存量数据
        db.exec(
            r#"INSERT INTO todos (title, prompt, status) VALUES ('legacy', 'p', 'pending')"#,
        )
        .await
        .expect("legacy row must be inserted");

        // 跑 V58：追加列
        let migration = V58AddTodosArchivedAt;
        migration.up(&db).await.expect("V58 migration must succeed");

        // 存量行 archived_at 必须为 NULL，而非被回填成某个时间戳
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT archived_at FROM todos WHERE title = 'legacy'".to_string(),
            ))
            .await
            .expect("row must be readable")
            .expect("legacy row must still exist");
        let archived: Option<String> = row
            .try_get_by_index(0)
            .expect("archived_at must be readable");
        assert!(
            archived.is_none(),
            "legacy row must default to NULL (not archived), got {archived:?}"
        );
    }
}
