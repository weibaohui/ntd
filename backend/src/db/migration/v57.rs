//! 数据库迁移 V57：为 blackboards 表新增 wiki_timeout_secs 字段。
//!
//! ## 背景
//! 黑板的 Wiki 自动维护（`update_blackboard_wiki`）和 Wiki 对话
//! （`spawn_executor_for_chat_streaming`）此前都把超时写死成 300 秒，
//! 用户无法在设置界面调整。一旦 Wiki 维护任务因模型慢、上下文大而超过 5 分钟，
//! 就会被强制超时失败（日志：`Wiki 执行超时（5 分钟）`）。
//!
//! 本迁移把超时做成 per-workspace 可配置项，默认值仍为 300 秒（与旧行为一致），
//! 用户可在黑板设置界面按需调大/调小。
//!
//! ## 幂等设计
//! 复用 `add_column_if_missing`，列已存在则静默跳过；新装 DB（V47 已建齐表）
//! 跑本迁移也只是 noop，不会破坏已有数据。
//!
//! ## 默认值
//! - wiki_timeout_secs: 300（与原写死的 5 分钟一致，保证存量行为不变）
//!
//! 注意 SQLite 的 ADD COLUMN 带 DEFAULT 会回填旧行，避免存量行变 NULL 导致
//! 后续业务层读取时拿到 0 误判成「无超时」。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V57AddWikiTimeoutSecs;

#[async_trait]
impl Migration for V57AddWikiTimeoutSecs {
    fn version(&self) -> i64 {
        57
    }

    fn name(&self) -> &'static str {
        "add_blackboards_wiki_timeout_secs"
    }

    /// 为 blackboards 表追加 wiki_timeout_secs 列。
    ///
    /// 用 NOT NULL DEFAULT 300 回填旧行，保证存量工作空间立即拥有与原写死值
    /// 一致的 5 分钟超时，迁移后行为零变化。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "blackboards",
            "wiki_timeout_secs",
            // DEFAULT 300 让旧行回填，避免业务层读到 NULL/0 误判
            "ALTER TABLE blackboards ADD COLUMN wiki_timeout_secs INTEGER NOT NULL DEFAULT 300",
        )
        .await?;
        tracing::info!("V57: blackboards.wiki_timeout_secs 列已添加");
        Ok(())
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::table_has_column;
    use crate::db::Database;

    /// 验证 V57 迁移成功添加 wiki_timeout_secs 列。
    /// 目的：确保列在全新库上被正确追加，且不破坏 V47 已建的表结构。
    #[tokio::test]
    async fn test_v57_adds_wiki_timeout_secs_column() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        // 先走 V47 建表，模拟一个已有 blackboards 表的环境
        let v47 = crate::db::migration::v47_v53::V47ConsolidatedBlackboardFeatures;
        v47.up(&db).await.expect("V47 migration must succeed");

        // 执行 V57
        let migration = V57AddWikiTimeoutSecs;
        migration.up(&db).await.expect("V57 migration must succeed");

        // 关键断言：列已存在
        assert!(
            table_has_column(&db, "blackboards", "wiki_timeout_secs")
                .await
                .unwrap(),
            "wiki_timeout_secs column must exist after V57"
        );
    }

    /// 验证 V57 迁移是幂等的（重复执行不报错）。
    /// 防止 schema_version 与 m.up 不一致时下次启动重跑失败。
    #[tokio::test]
    async fn test_v57_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let v47 = crate::db::migration::v47_v53::V47ConsolidatedBlackboardFeatures;
        v47.up(&db).await.expect("V47 migration must succeed");

        let migration = V57AddWikiTimeoutSecs;
        migration.up(&db).await.expect("First run must succeed");
        migration.up(&db).await.expect("Second run must succeed (idempotent)");
    }

    /// 验证 V57 给旧行回填默认值 300，而非 NULL/0。
    /// 这是 NOT NULL DEFAULT 子句的关键作用：存量工作空间的超时行为在迁移后
    /// 必须与原写死的 5 分钟完全一致，不能因为迁移悄悄变成「无超时」或「0 秒」。
    #[tokio::test]
    async fn test_v57_backfills_default_300_to_legacy_rows() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let v47 = crate::db::migration::v47_v53::V47ConsolidatedBlackboardFeatures;
        v47.up(&db).await.expect("V47 migration must succeed");

        // 插一条旧行（迁移前没有 wiki_timeout_secs 列，这里模拟迁移前的存量数据）
        // workspace_id=1 对应的 project_directories 行可能不存在，但 SQLite 默认不强制外键
        db.exec(
            r#"INSERT INTO blackboards (workspace_id, content) VALUES (1, 'legacy')"#,
        )
        .await
        .expect("legacy row must be inserted");

        // 跑 V57：应追加列并回填默认值
        let migration = V57AddWikiTimeoutSecs;
        migration.up(&db).await.expect("V57 migration must succeed");

        // 读取旧行，确认 wiki_timeout_secs 被回填成 300，而非 NULL 或 0
        use sea_orm::ConnectionTrait;
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT wiki_timeout_secs FROM blackboards WHERE workspace_id = 1".to_string(),
            ))
            .await
            .expect("row must be readable")
            .expect("legacy row must still exist");
        let timeout: i64 = row
            .try_get_by_index(0)
            .expect("wiki_timeout_secs must be readable");
        assert_eq!(
            timeout, 300,
            "legacy row must backfill to 300 (5 min), matching old hardcoded behavior"
        );
    }
}
