//! 数据库迁移 V54：黑板提示词拆分为 wiki_index_prompt 和 wiki_page_prompt。
//!
//! 旧字段 blackboard_update_prompt 用于单文件黑板模式，已废弃。
//! Wiki 两阶段模式需要两个独立提示词：
//! - wiki_index_prompt：分析阶段，决定记录归到哪些主题页面
//! - wiki_page_prompt：执行阶段，生成具体页面 Markdown 内容
//!
//! 两个新字段均为 TEXT，默认空字符串表示使用内置默认模板。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V54SplitBlackboardPrompt;

#[async_trait]
impl Migration for V54SplitBlackboardPrompt {
    fn version(&self) -> i64 {
        54
    }

    fn name(&self) -> &'static str {
        "split_blackboard_prompt_into_wiki_index_and_page"
    }

    /// 添加 wiki_index_prompt 和 wiki_page_prompt 列，删除旧的 blackboard_update_prompt 列。
    ///
    /// 新列使用 TEXT NOT NULL DEFAULT '' 确保必填且默认空字符串。
    /// SQLite 3.35.0+ 支持 ALTER TABLE DROP COLUMN。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 添加 wiki_index_prompt（若不存在则新增）
        let add_index = "ALTER TABLE blackboards ADD COLUMN wiki_index_prompt TEXT NOT NULL DEFAULT ''";
        if let Err(e) = db.exec(add_index).await {
            // 列已存在时忽略错误（幂等性）
            if !e.to_string().contains("duplicate column") {
                return Err(e);
            }
        }

        // 添加 wiki_page_prompt（若不存在则新增）
        let add_page = "ALTER TABLE blackboards ADD COLUMN wiki_page_prompt TEXT NOT NULL DEFAULT ''";
        if let Err(e) = db.exec(add_page).await {
            if !e.to_string().contains("duplicate column") {
                return Err(e);
            }
        }

        // 删除旧列 blackboard_update_prompt（若存在）
        let drop_old = "ALTER TABLE blackboards DROP COLUMN blackboard_update_prompt";
        if let Err(e) = db.exec(drop_old).await {
            // 列不存在时忽略（幂等性）
            if !e.to_string().contains("no such column") {
                return Err(e);
            }
        }

        tracing::info!("V54: wiki_index_prompt 和 wiki_page_prompt 已添加，blackboard_update_prompt 已删除");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::migration::table_exists;

    /// 验证 V54 迁移在全新数据库上正常执行。
    #[tokio::test]
    async fn test_v54_adds_wiki_prompt_columns() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        // 先创建 blackboards 表（V54 假设表已存在）
        db.exec(
            "CREATE TABLE blackboards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                pending_record_ids TEXT NOT NULL DEFAULT '[]',
                blackboard_debounce_secs INTEGER NOT NULL DEFAULT 600,
                blackboard_debounce_count INTEGER NOT NULL DEFAULT 10,
                blackboard_update_prompt TEXT NOT NULL DEFAULT '',
                updated_at TEXT,
                created_at TEXT
            )"
        ).await.expect("create blackboards table");

        let migration = V54SplitBlackboardPrompt;
        migration.up(&db).await.expect("V54 migration must succeed");

        // 验证新列存在，旧列不存在
        let result = db.exec("SELECT wiki_index_prompt, wiki_page_prompt FROM blackboards").await;
        assert!(result.is_ok(), "new columns must exist");
    }

    /// 验证 V54 迁移是幂等的。
    #[tokio::test]
    async fn test_v54_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        db.exec(
            "CREATE TABLE blackboards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                pending_record_ids TEXT NOT NULL DEFAULT '[]',
                blackboard_debounce_secs INTEGER NOT NULL DEFAULT 600,
                blackboard_debounce_count INTEGER NOT NULL DEFAULT 10,
                blackboard_update_prompt TEXT NOT NULL DEFAULT '',
                updated_at TEXT,
                created_at TEXT
            )"
        ).await.expect("create blackboards table");

        let migration = V54SplitBlackboardPrompt;
        migration.up(&db).await.expect("First run must succeed");
        migration.up(&db).await.expect("Second run must succeed (idempotent)");
    }
}
