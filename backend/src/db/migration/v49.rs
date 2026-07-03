//! 数据库迁移 V49：为 blackboards 表添加 pending_todo_ids 字段。
//!
//! 背景：黑板更新从"每次 todo 执行完毕立即触发"改为"debounce 周期汇总触发"。
//! pending_todo_ids 存储待处理的 todo_id 队列（JSON 数组），周期到点后统一处理。
//!
//! 数据兼容性：已有 blackboards 记录的 pending_todo_ids 初始化为空数组 "[]"。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V49AddBlackboardPendingTodoIds;

#[async_trait]
impl Migration for V49AddBlackboardPendingTodoIds {
    fn version(&self) -> i64 {
        49
    }

    fn name(&self) -> &'static str {
        "add_blackboard_pending_todo_ids"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 1. 检查列是否已存在（幂等：重复执行不报错）
        if super::table_has_column(db, "blackboards", "pending_todo_ids").await? {
            return Ok(());
        }

        // 2. 加列
        db.exec("ALTER TABLE blackboards ADD COLUMN pending_todo_ids TEXT NOT NULL DEFAULT '[]'")
            .await?;

        tracing::info!("V49: blackboards.pending_todo_ids 字段已添加");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[tokio::test]
    async fn test_v49_migration_succeeds() {
        let db = Database::new(":memory:").await.unwrap();
        // V47 创建了 blackboards 表
        crate::db::migration::v47::V47CreateBlackboardsTable
            .up(&db)
            .await
            .unwrap();
        // V49 应当成功执行
        V49AddBlackboardPendingTodoIds
            .up(&db)
            .await
            .expect("V49 must succeed");
    }
}
