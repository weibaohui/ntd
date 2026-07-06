//! 数据库迁移 V55：为 blackboards 表新增 wiki_chat_sessions 字段。
//!
//! 该字段存储每个执行器的 Wiki 对话 session ID（JSON 格式），例如：
//! {"claudecode": "uuid-session-1", "hermes": "uuid-session-2", "opencode": null}
//!
//! 幂等设计：使用 `add_column_if_missing` 辅助函数，列已存在时静默跳过。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V55AddWikiChatSessions;

#[async_trait]
impl Migration for V55AddWikiChatSessions {
    fn version(&self) -> i64 {
        55
    }

    fn name(&self) -> &'static str {
        "add_blackboards_wiki_chat_sessions"
    }

    /// 执行迁移：为 blackboards 表添加 wiki_chat_sessions 列。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "blackboards",
            "wiki_chat_sessions",
            "ALTER TABLE blackboards ADD COLUMN wiki_chat_sessions TEXT",
        )
        .await?;
        tracing::info!("V55: blackboards.wiki_chat_sessions 列已添加");
        Ok(())
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::table_has_column;
    use crate::db::Database;

    #[tokio::test]
    async fn test_v55_adds_wiki_chat_sessions_column() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let v47 = crate::db::migration::v47_v53::V47ConsolidatedBlackboardFeatures;
        v47.up(&db).await.expect("V47 migration must succeed");

        let migration = V55AddWikiChatSessions;
        migration.up(&db).await.expect("V55 migration must succeed");

        assert!(
            table_has_column(&db, "blackboards", "wiki_chat_sessions")
                .await
                .unwrap(),
            "wiki_chat_sessions column must exist after V55"
        );
    }

    #[tokio::test]
    async fn test_v55_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let v47 = crate::db::migration::v47_v53::V47ConsolidatedBlackboardFeatures;
        v47.up(&db).await.expect("V47 migration must succeed");

        let migration = V55AddWikiChatSessions;
        migration.up(&db).await.expect("First run must succeed");
        migration.up(&db).await.expect("Second run must succeed (idempotent)");
    }
}
