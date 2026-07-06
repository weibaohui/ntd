//! 数据库迁移 V54：为 blackboards 表新增 wiki_chat_executor 字段。
//!
//! 该字段用于配置黑板 wiki 对话功能使用的执行器（如 claudecode / codex / kimi 等），
//! 与 workspace_settings.default_response_executor 互相独立。
//!
//! 幂等设计：使用 `add_column_if_missing` 辅助函数，列已存在时静默跳过。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V54AddWikiChatExecutor;

#[async_trait]
impl Migration for V54AddWikiChatExecutor {
    fn version(&self) -> i64 {
        54
    }

    fn name(&self) -> &'static str {
        "add_blackboards_wiki_chat_executor"
    }

    /// 执行迁移：为 blackboards 表添加 wiki_chat_executor 列。
    ///
    /// 使用 add_column_if_missing 保证幂等，列已存在时不报错。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "blackboards",
            "wiki_chat_executor",
            "ALTER TABLE blackboards ADD COLUMN wiki_chat_executor TEXT",
        )
        .await?;
        tracing::info!("V54: blackboards.wiki_chat_executor 列已添加");
        Ok(())
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::table_has_column;
    use crate::db::Database;

    /// 验证 V54 迁移成功添加 wiki_chat_executor 列。
    #[tokio::test]
    async fn test_v54_adds_wiki_chat_executor_column() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        // 先执行 V47 建表（不含 wiki_chat_executor 也行，但走完整流程更贴近真实场景）
        let v47 = crate::db::migration::v47_v53::V47ConsolidatedBlackboardFeatures;
        v47.up(&db).await.expect("V47 migration must succeed");

        // 执行 V54
        let migration = V54AddWikiChatExecutor;
        migration.up(&db).await.expect("V54 migration must succeed");

        // 验证列存在
        assert!(
            table_has_column(&db, "blackboards", "wiki_chat_executor")
                .await
                .unwrap(),
            "wiki_chat_executor column must exist after V54"
        );
    }

    /// 验证 V54 迁移是幂等的（重复执行不报错）。
    #[tokio::test]
    async fn test_v54_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let v47 = crate::db::migration::v47_v53::V47ConsolidatedBlackboardFeatures;
        v47.up(&db).await.expect("V47 migration must succeed");

        let migration = V54AddWikiChatExecutor;
        migration.up(&db).await.expect("First run must succeed");
        migration.up(&db).await.expect("Second run must succeed (idempotent)");
    }
}
