//! 数据库迁移 V51：为 blackboards 表添加 pending_record_ids 字段。
//!
//! 背景：黑板 pending 队列从存 todo_id 改为存 execution_record_id。
//! 这样 flusher 可以直接按 record_id 查结论，避免 get_latest_execution_record_for_todo
//! 在 debounce 窗口内同一 todo 多次执行时丢失中间结论的问题。
//!
//! 数据兼容性：已有 blackboards 记录的 pending_record_ids 初始化为空数组 "[]"。
//! 存量 pending_todo_ids 列由 V52 迁移清理删除。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V51AddBlackboardPendingRecordIds;

#[async_trait]
impl Migration for V51AddBlackboardPendingRecordIds {
    fn version(&self) -> i64 {
        51
    }

    fn name(&self) -> &'static str {
        "add_blackboard_pending_record_ids"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 1. 检查列是否已存在（幂等：重复执行不报错）
        if super::table_has_column(db, "blackboards", "pending_record_ids").await? {
            return Ok(());
        }

        // 2. 加列
        db.exec("ALTER TABLE blackboards ADD COLUMN pending_record_ids TEXT NOT NULL DEFAULT '[]'")
            .await?;

        tracing::info!("V51: blackboards.pending_record_ids 字段已添加");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[tokio::test]
    async fn test_v51_migration_succeeds() {
        let db = Database::new(":memory:").await.unwrap();
        // V47 创建了 blackboards 表
        crate::db::migration::v47::V47CreateBlackboardsTable
            .up(&db)
            .await
            .unwrap();
        // V51 应当成功执行
        V51AddBlackboardPendingRecordIds
            .up(&db)
            .await
            .expect("V51 must succeed");
    }
}
