//! 数据库迁移 V93：为 todos.updated_at 增加排序索引。
//!
//! ## 背景
//! Todo 列表/看板是最高频读路径：`get_todos_page_by_workspace` 与 `get_todo_briefs`
//! 都按 `ORDER BY updated_at DESC` 分页，但 todos 表现有 10 个索引均无 updated_at
//! （V90 补高频过滤列时独漏排序键；loops 表早有 `idx_loops_updated_at` 先例）。
//! 每页查询全表扫 + filesort，随数据量增长线性变慢。
//! 配套改动（同 PR）：hours 过滤从列上套 REPLACE 函数改为参数化裸列比较，
//! 使本索引同时覆盖「排序 + 时间窗过滤」两条路径。
//!
//! ## 幂等
//! `CREATE INDEX IF NOT EXISTS`，已存在则静默跳过。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V93AddTodosUpdatedAtIndex;

#[async_trait]
impl Migration for V93AddTodosUpdatedAtIndex {
    fn version(&self) -> i64 {
        93
    }

    fn name(&self) -> &'static str {
        "add_todos_updated_at_index"
    }

    /// 在 updated_at 上建 DESC 索引，与 loops 表 `idx_loops_updated_at` 先例对齐
    /// （SQLite 对 ASC 索引也能反向扫，功能等价，统一风格优先）。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        db.exec(
            "CREATE INDEX IF NOT EXISTS idx_todos_updated_at ON todos(updated_at DESC)",
        )
        .await?;
        tracing::info!("V93: todos.updated_at 索引已创建");
        Ok(())
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::table_has_column;
    use crate::db::Database;

    /// 验证 V93 创建索引（通过查询 sqlite_master 间接确认）。
    #[tokio::test]
    async fn test_v93_creates_updated_at_index() {
        use sea_orm::ConnectionTrait;
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V93AddTodosUpdatedAtIndex;
        migration.up(&db).await.expect("V93 migration must succeed");

        // 确认索引存在
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) AS cnt FROM sqlite_master \
                 WHERE type='index' AND name='idx_todos_updated_at'"
                    .to_string(),
            ))
            .await
            .expect("query must succeed")
            .expect("row must exist");
        let cnt: i64 = row.try_get_by("cnt").expect("cnt readable");
        assert_eq!(cnt, 1, "idx_todos_updated_at 索引应存在");
    }

    /// 幂等：重复执行不报错。
    #[tokio::test]
    async fn test_v93_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let migration = V93AddTodosUpdatedAtIndex;
        migration.up(&db).await.expect("first run");
        migration.up(&db).await.expect("second run (idempotent)");
        // table_has_column 仅作占位断言，确认库仍可用
        assert!(table_has_column(&db, "todos", "updated_at").await.unwrap());
    }
}
