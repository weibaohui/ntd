//! 数据库迁移 V60：为 feishu_messages 增加 error 字段。
//!
//! ## 背景
//! 当消息处理失败时（如环路暂停），需要记录失败原因以便前端显示。
//! 之前只有 processed=false 表示未处理，无法区分"真正未处理"和"环路暂停无法执行"。
//!
//! ## 幂等
//! `add_column_if_missing` 已存在则静默跳过。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V60AddFeishuMessagesError;

#[async_trait]
impl Migration for V60AddFeishuMessagesError {
    fn version(&self) -> i64 {
        60
    }

    fn name(&self) -> &'static str {
        "add_feishu_messages_error"
    }

    /// 为 feishu_messages 表添加 error 字段，记录处理失败原因。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "feishu_messages",
            "error",
            "ALTER TABLE feishu_messages ADD COLUMN error TEXT",
        )
        .await?;
        tracing::info!("V60: feishu_messages.error 列已添加");
        Ok(())
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::table_has_column;
    use crate::db::Database;

    /// 验证 V60 添加 error 列。
    #[tokio::test]
    async fn test_v60_adds_error_column() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V60AddFeishuMessagesError;
        migration.up(&db).await.expect("V60 migration must succeed");

        assert!(
            table_has_column(&db, "feishu_messages", "error").await.unwrap(),
            "feishu_messages.error 列应存在"
        );
    }

    /// 幂等：重复执行不报错。
    #[tokio::test]
    async fn test_v60_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V60AddFeishuMessagesError;
        migration.up(&db).await.expect("first run");
        migration.up(&db).await.expect("second run (idempotent)");
    }
}
