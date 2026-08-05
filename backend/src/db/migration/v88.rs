//! 迁移 v88：新建 task_posts 表（任务讨论区 / 论坛跟帖，需求 060）。
//!
//! ## 背景
//! 任务（tasks）此前只有「需求文本 + 工艺环路 + 执行历史」的单向链路，缺少围绕
//! 一个任务的多轮协作讨论。需求 060 在任务上增加论坛式跟帖流：人帖 + 智能体帖
//! （由 @专家 / @执行器 触发执行后自动回写结论）。每条帖子挂在一个 task 上，
//! 智能体帖通过 `source_execution_id` 关联既有 execution_records，不重复存储执行明细。
//!
//! ## 幂等
//! `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` 天然幂等，
//! 从任意中间状态重启都能安全重入（与 v66 quick_buttons 同模式）。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

/// v88：任务讨论帖表。
pub struct V88TaskDiscussionPosts;

#[async_trait]
impl Migration for V88TaskDiscussionPosts {
    fn version(&self) -> i64 {
        88
    }

    fn name(&self) -> &'static str {
        "V88TaskDiscussionPosts"
    }

    /// 建 task_posts 表 + task_id 索引。
    ///
    /// - `parent_post_id` 自引用实现「楼中楼」（应用层限制深度 ≤1，只允许指向主楼层）。
    /// - `mentions` 用 JSON 字符串存结构化提及，触发与徽标渲染都依赖它，
    ///   不靠解析正文（可靠）；正文里的 `@token` 仅作展示。
    /// - 外键均 ON DELETE CASCADE：删任务 / 删父帖时连带清理，避免孤儿帖。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        db.exec(
            "CREATE TABLE IF NOT EXISTS task_posts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id INTEGER NOT NULL,
                parent_post_id INTEGER,
                kind TEXT NOT NULL,
                author_name TEXT NOT NULL,
                executor TEXT,
                expert_name TEXT,
                content TEXT NOT NULL DEFAULT '',
                mentions TEXT NOT NULL DEFAULT '[]',
                status TEXT NOT NULL DEFAULT 'sent',
                source_execution_id INTEGER,
                source_todo_id INTEGER,
                created_at TEXT,
                updated_at TEXT,
                FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
                FOREIGN KEY (parent_post_id) REFERENCES task_posts(id) ON DELETE CASCADE
            )",
        )
        .await?;
        // task_id 索引：讨论 Tab 按 task 取帖子流是高频点查，必须走索引。
        db.exec(
            "CREATE INDEX IF NOT EXISTS idx_task_posts_task_id ON task_posts(task_id)",
        )
        .await?;
        tracing::info!("V88: task_posts 表已创建");
        Ok(())
    }
}
