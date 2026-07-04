//! 数据库迁移 V52：删除 blackboards 表的 pending_todo_ids 列。
//!
//! 背景：V51 引入了 pending_record_ids 代替 pending_todo_ids，
//! 本迁移清理不再使用的旧列。
//!
//! 安全删除：SQLite 3.35.0+ 支持 ALTER TABLE DROP COLUMN，
//! drop_column_if_exists 内置存在性检查，重复执行幂等。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V52DropBlackboardPendingTodoIds;

#[async_trait]
impl Migration for V52DropBlackboardPendingTodoIds {
    fn version(&self) -> i64 {
        52
    }

    fn name(&self) -> &'static str {
        "drop_blackboard_pending_todo_ids"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 使用带存在性检查的 drop 工具函数，幂等安全
        crate::db::migration::v2_v5::drop_column_if_exists(
            db,
            "blackboards",
            "pending_todo_ids",
        )
        .await?;

        tracing::info!("V52: blackboards.pending_todo_ids 列已删除");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[tokio::test]
    async fn test_v52_migration_succeeds() {
        let db = Database::new(":memory:").await.unwrap();
        // V47 创建了 blackboards 表
        crate::db::migration::v47::V47CreateBlackboardsTable
            .up(&db)
            .await
            .unwrap();
        // V49 添加了 pending_todo_ids 列
        crate::db::migration::v49::V49AddBlackboardPendingTodoIds
            .up(&db)
            .await
            .unwrap();
        // V52 应当成功删除该列
        V52DropBlackboardPendingTodoIds
            .up(&db)
            .await
            .expect("V52 must succeed");
    }
}
