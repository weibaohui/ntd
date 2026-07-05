//! 数据库迁移 V47-V48：黑板功能合并迁移（blackboards 表 + todos 索引修复）。
//!
//! 合并了以下迁移的完整逻辑：
//! V47  CreateBlackboardsTable - 创建 blackboards 表（+ pending_record_ids / debounce / wiki 提示词）
//! V48  ScopeTodosActionKeyByWorkspace - 修复 todos (action_type, action_key) 索引为 workspace 级
//!
//! 幂等设计：
//! - 所有 CREATE TABLE 带 IF NOT EXISTS / table_exists 前置检查
//! - DROP INDEX IF EXISTS 静默跳过不存在的索引
//! - CREATE UNIQUE INDEX IF NOT EXISTS 跳过已存在的索引

use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::super::Database;
use super::{Migration, table_exists};

pub(super) struct V47ConsolidatedBlackboardFeatures;

#[async_trait]
impl Migration for V47ConsolidatedBlackboardFeatures {
    fn version(&self) -> i64 {
        47
    }

    fn name(&self) -> &'static str {
        "consolidated_blackboard_features"
    }

    /// 执行合并迁移：按 V47 → V48 → V53 的顺序依次执行。
    ///
    /// 每步通过 IF NOT EXISTS / table_exists 保证幂等性，
    /// 已执行过的步骤自动跳过。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // ---- V47: 创建 blackboards 表（一次性包含所有字段） ----
        // 每个工作空间维护一条黑板记录，用于 LLM 自动生成的 Markdown 知识库内容
        if !table_exists(db, "blackboards").await? {
            db.exec(CREATE_BLACKBOARDS_SQL).await?;
            tracing::info!("V47: blackboards 表已创建");
        }

        // ---- V48: 把 todos 上的唯一索引改为 workspace 级 ----
        // V46 创建的 (action_type, action_key) 是全局唯一，多 workspace 下
        // 第二个 workspace 创建同名 action template 会触发 UNIQUE 冲突。
        // 修复：改为 (action_type, action_key, workspace_id) 复合唯一索引。
        let _ = db
            .conn
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "DROP INDEX IF EXISTS idx_todos_action_type_key".to_string(),
            ))
            .await;
        db.conn
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_todos_action_type_key_workspace \
                 ON todos (action_type, action_key, workspace_id) \
                 WHERE action_type IS NOT NULL AND action_key IS NOT NULL"
                    .to_string(),
            ))
            .await?;
        tracing::info!("V48: todos (action_type, action_key, workspace_id) 唯一索引已建立");

        Ok(())
    }
}

/// blackboards 表 DDL：一次性包含所有演进字段。
const CREATE_BLACKBOARDS_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS blackboards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id INTEGER NOT NULL UNIQUE,
    content TEXT NOT NULL DEFAULT '',
    pending_record_ids TEXT NOT NULL DEFAULT '[]',
    blackboard_debounce_secs INTEGER NOT NULL DEFAULT 600,
    blackboard_debounce_count INTEGER NOT NULL DEFAULT 10,
    wiki_prompt TEXT NOT NULL DEFAULT '',
    updated_at TEXT,
    created_at TEXT,
    FOREIGN KEY (workspace_id) REFERENCES project_directories(id) ON DELETE CASCADE
);
"#;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 验证合并迁移可成功创建 blackboards 表（含所有业务字段）。
    #[tokio::test]
    async fn test_v47_creates_blackboards_table() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V47ConsolidatedBlackboardFeatures;
        migration.up(&db).await.expect("V47 migration must succeed");

        // 验证 blackboards 表已存在
        assert!(table_exists(&db, "blackboards").await.unwrap());

        // 验证所有业务字段一次性建好（V47 原本的测试）
        for col in &[
            "pending_record_ids",
            "blackboard_debounce_secs",
            "blackboard_debounce_count",
            "wiki_prompt",
        ] {
            assert!(
                super::super::table_has_column(&db, "blackboards", col)
                    .await
                    .unwrap(),
                "column {col} must exist in blackboards table"
            );
        }
    }

    /// 验证 V48 索引修复：不同 workspace 可以拥有同 (action_type, action_key) 的 todo。
    #[tokio::test]
    async fn test_v47_scope_todos_action_key_by_workspace() {
        let db = Database::new(":memory:").await.unwrap();
        // 执行合并迁移
        V47ConsolidatedBlackboardFeatures
            .up(&db)
            .await
            .unwrap();

        // 不同 workspace 各自建一个 todo
        let ws1 = db
            .create_project_directory("/tmp/v47-ws-1", None, false, false)
            .await
            .unwrap();
        let ws2 = db
            .create_project_directory("/tmp/v47-ws-2", None, false, false)
            .await
            .unwrap();
        let todo1 = db
            .create_todo_with_extras("t1", "p1", None, None, false, ws1, "/tmp/v47-ws-1")
            .await
            .unwrap();
        let todo2 = db
            .create_todo_with_extras("t2", "p2", None, None, false, ws2, "/tmp/v47-ws-2")
            .await
            .unwrap();
        // 设置 action_type/action_key，workspace 级索引应允许两者共存
        db.update_todo_full(crate::db::TodoUpdate {
            id: todo1,
            title: "t1",
            prompt: "p1",
            status: crate::models::TodoStatus::Pending,
            executor: None,
            scheduler_enabled: None,
            scheduler_config: None,
            scheduler_timezone: None,
            workspace_id: None,
            webhook_enabled: None,
            acceptance_criteria: None,
            auto_review_enabled: None,
            action_type: Some("blackboard"),
            action_key: Some("update"),
        })
        .await
        .unwrap();
        // 关键断言：V46 全局唯一索引会拒绝第二条；合并后的 V48 应允许
        db.update_todo_full(crate::db::TodoUpdate {
            id: todo2,
            title: "t2",
            prompt: "p2",
            status: crate::models::TodoStatus::Pending,
            executor: None,
            scheduler_enabled: None,
            scheduler_config: None,
            scheduler_timezone: None,
            workspace_id: None,
            webhook_enabled: None,
            acceptance_criteria: None,
            auto_review_enabled: None,
            action_type: Some("blackboard"),
            action_key: Some("update"),
        })
        .await
        .expect("per-workspace action template should be allowed after V47");
    }

    /// 验证合并迁移是幂等的（重复执行不会报错）。
    #[tokio::test]
    async fn test_v47_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let migration = V47ConsolidatedBlackboardFeatures;
        migration.up(&db).await.expect("First run must succeed");
        migration.up(&db).await.expect("Second run must succeed (idempotent)");
    }
}
