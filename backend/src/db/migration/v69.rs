//! 数据库迁移 V69：quick_buttons 增加 workspace_id 列，实现 workspace 隔离
//!
//! ## 背景
//! ADR-7 路由重构后 quick_buttons 按设计应嵌套在 `/api/v1/workspaces/{ws}` 下，
//! 因此表结构需要 workspace_id 列支撑按 workspace 查询与隔离。
//!
//! ## 幂等
//! 先通过 `table_has_column` 判断列是否存在，避免重复 ADD COLUMN 报错；
//! 存量数据默认归属到 id 最小的 project_directory（首个工作空间），保证旧数据不丢失。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V69AddQuickButtonsWorkspaceId;

#[async_trait]
impl Migration for V69AddQuickButtonsWorkspaceId {
    fn version(&self) -> i64 {
        69
    }

    fn name(&self) -> &'static str {
        "V69AddQuickButtonsWorkspaceId"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        if !super::table_has_column(db, "quick_buttons", "workspace_id").await? {
            db.exec("ALTER TABLE quick_buttons ADD COLUMN workspace_id INTEGER")
                .await?;

            // 把存量按钮默认归到第一个工作空间，避免旧数据变成「无主」。
            db.exec(
                "UPDATE quick_buttons SET workspace_id = (
                    SELECT id FROM project_directories ORDER BY id ASC LIMIT 1
                ) WHERE workspace_id IS NULL",
            )
            .await?;

            tracing::info!("V69: quick_buttons 已添加 workspace_id 列并完成存量迁移");
        } else {
            tracing::info!("V69: quick_buttons.workspace_id 列已存在，跳过");
        }
        Ok(())
    }
}
