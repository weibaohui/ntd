//! 数据库迁移 V66：新建 quick_buttons 表（用户自定义快捷话术按钮）
//!
//! ## 背景
//! 帖子流回复框上方提供用户自定义的快捷按钮：每个按钮存「名称 + 预设话术」，
//! 点击把话术填入回复输入框。全局共享，无 workspace 维度。
//!
//! ## 幂等
//! `CREATE TABLE IF NOT EXISTS` 天然幂等，从任意中间状态重启都能安全重入。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V66AddQuickButtonsTable;

#[async_trait]
impl Migration for V66AddQuickButtonsTable {
    fn version(&self) -> i64 {
        66
    }

    fn name(&self) -> &'static str {
        "V66AddQuickButtonsTable"
    }

    /// 建 quick_buttons 表。button_name 全局唯一，避免用户加出同名按钮。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        db.exec(
            "CREATE TABLE IF NOT EXISTS quick_buttons (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                button_name TEXT NOT NULL,
                prompt_text TEXT NOT NULL,
                created_at TEXT,
                updated_at TEXT,
                UNIQUE(button_name)
            )",
        )
        .await?;
        tracing::info!("V66: quick_buttons 表已创建");
        Ok(())
    }
}
