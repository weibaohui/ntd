//! 数据库迁移 V50：为 blackboards 表添加 per-workspace 配置字段。
//!
//! 背景：黑板的防抖阈值和提示词模板原本存储在全局 Config 中，
//! 导致所有工作空间共享同一份配置，无法按工作空间独立定制。
//!
//! 新增字段（均有默认值，迁移前已存在的记录自动获得初始值）：
//! - blackboard_debounce_secs: i64，默认 600
//! - blackboard_debounce_count: i64，默认 10
//! - blackboard_update_prompt: TEXT，空字符串（表示使用内置默认）
//!
//! 数据兼容性：已有记录这些字段自动获得默认值，不影响现有逻辑。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V50AddBlackboardWorkspaceConfig;

#[async_trait]
impl Migration for V50AddBlackboardWorkspaceConfig {
    fn version(&self) -> i64 {
        50
    }

    fn name(&self) -> &'static str {
        "add_blackboard_workspace_config"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 1. blackboard_debounce_secs: INTEGER NOT NULL DEFAULT 600
        if !super::table_has_column(db, "blackboards", "blackboard_debounce_secs").await? {
            db.exec("ALTER TABLE blackboards ADD COLUMN blackboard_debounce_secs INTEGER NOT NULL DEFAULT 600")
                .await?;
            tracing::info!("V50: blackboards.blackboard_debounce_secs 字段已添加");
        }
        // 2. blackboard_debounce_count: INTEGER NOT NULL DEFAULT 10
        if !super::table_has_column(db, "blackboards", "blackboard_debounce_count").await? {
            db.exec("ALTER TABLE blackboards ADD COLUMN blackboard_debounce_count INTEGER NOT NULL DEFAULT 10")
                .await?;
            tracing::info!("V50: blackboards.blackboard_debounce_count 字段已添加");
        }
        // 3. blackboard_update_prompt: TEXT NOT NULL DEFAULT ''
        if !super::table_has_column(db, "blackboards", "blackboard_update_prompt").await? {
            db.exec("ALTER TABLE blackboards ADD COLUMN blackboard_update_prompt TEXT NOT NULL DEFAULT ''")
                .await?;
            tracing::info!("V50: blackboards.blackboard_update_prompt 字段已添加");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::v47;

    #[tokio::test]
    async fn test_v50_migration_succeeds() {
        let db = Database::new(":memory:").await.unwrap();
        // V47 创建了 blackboards 表
        v47::V47CreateBlackboardsTable.up(&db).await.unwrap();
        // V49 添加了 pending_todo_ids
        crate::db::migration::v49::V49AddBlackboardPendingTodoIds
            .up(&db)
            .await
            .unwrap();
        // V50 应当成功执行
        V50AddBlackboardWorkspaceConfig
            .up(&db)
            .await
            .expect("V50 must succeed");
    }
}
