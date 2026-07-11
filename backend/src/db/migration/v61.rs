//! 数据库迁移 V61：为 project_directories 增加 executor_sessions 字段。
//!
//! ## 背景
//! 私聊默认响应执行器时，需要持久化各执行器的 session_id，
//! 使后续对话能继续同一会话（resume），类似 Claude Code 的多轮对话体验。
//! 使用 JSON 对象存储，key 为执行器类型，value 为 session_id。
//!
//! ## 幂等
//! `add_column_if_missing` 已存在则静默跳过。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V61AddProjectDirectoriesExecutorSessions;

#[async_trait]
impl Migration for V61AddProjectDirectoriesExecutorSessions {
    fn version(&self) -> i64 {
        61
    }

    fn name(&self) -> &'static str {
        "add_project_directories_executor_sessions"
    }

    /// 为 project_directories 表添加 executor_sessions 列，
    /// 存储各执行器的 session_id（JSON 对象格式）。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "project_directories",
            "executor_sessions",
            "ALTER TABLE project_directories ADD COLUMN executor_sessions TEXT",
        )
        .await?;
        tracing::info!("V61: project_directories.executor_sessions 列已添加");
        Ok(())
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::table_has_column;
    use crate::db::Database;

    /// 验证 V61 添加 executor_sessions 列。
    #[tokio::test]
    async fn test_v61_adds_executor_sessions_column() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V61AddProjectDirectoriesExecutorSessions;
        migration.up(&db).await.expect("V61 migration must succeed");

        assert!(
            table_has_column(&db, "project_directories", "executor_sessions").await.unwrap(),
            "project_directories.executor_sessions 列应存在"
        );
    }
}
