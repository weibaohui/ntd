//! 数据库迁移 V48：把 todos 上的 (action_type, action_key) 唯一索引改为
//! 包含 workspace_id 的复合唯一索引。
//!
//! 背景：V46 创建的 `idx_todos_action_type_key` 是全局唯一约束，
//! 但 `find_or_create_blackboard_todo` 按 (action_type, action_key, workspace_id)
//! 查找并为每个 workspace 单独创建黑板更新 todo。
//! 在多 workspace 场景下，全局唯一会让第二个 workspace 的创建语句
//! 触发 `UNIQUE constraint failed: todos.action_type, todos.action_key`。
//!
//! 修复方式：
//! 1. 删除 V46 创建的旧索引
//! 2. 新建 (action_type, action_key, workspace_id) 唯一索引，保持 WHERE 过滤
//!    NULL 列的语义
//!
//! 数据兼容性：新索引在已有数据上可能因历史脏数据而失败。
//! 黑板功能是本次 PR 引入，所以生产环境不应有 (blackboard, update) 跨 workspace
//! 的历史脏数据。其它 action_type/action_key 由调用方保证单一 workspace。

use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::super::Database;
use super::Migration;

pub(super) struct V48ScopeTodosActionKeyByWorkspace;

#[async_trait]
impl Migration for V48ScopeTodosActionKeyByWorkspace {
    fn version(&self) -> i64 {
        48
    }

    fn name(&self) -> &'static str {
        "scope_todos_action_key_by_workspace"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 1. 删除 V46 留下的旧唯一索引。索引不存在时忽略错误以保证迁移可重入。
        let _ = db
            .conn
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "DROP INDEX IF EXISTS idx_todos_action_type_key".to_string(),
            ))
            .await;

        // 2. 新建 (action_type, action_key, workspace_id) 唯一索引。
        //    保留 WHERE 过滤 NULL 列的语义，避免把未分类的 todo 也纳入唯一约束。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 验证 V48 迁移可成功执行（包含 DROP + CREATE）。
    /// 在干净的 :memory: 数据库上跑，验证幂等且不报错。
    #[tokio::test]
    async fn test_v48_migration_succeeds() {
        let db = Database::new(":memory:").await.unwrap();
        let migration = V48ScopeTodosActionKeyByWorkspace;
        migration.up(&db).await.expect("V48 must succeed");
        // 二次执行：旧索引已不在，DROP IF EXISTS 静默跳过，CREATE IF NOT EXISTS 跳过
        migration
            .up(&db)
            .await
            .expect("V48 second run must succeed (idempotent)");
    }

    /// 验证迁移后不同 workspace 可以拥有同 (action_type, action_key) 的 todo。
    /// 这是修复的核心断言：原先 V46 的全局唯一会让两条 INSERT 失败，
    /// V48 应当允许它们共存。
    /// V1 初始 schema 已经包含 action_type/action_key 列，直接用 create_todo_with_extras
    /// 即可触发 INSERT 路径；V48 在 up 时替换索引，所以测试只用 V48 的 up。
    #[tokio::test]
    async fn test_v48_allows_per_workspace_action_template() {
        let db = Database::new(":memory:").await.unwrap();
        // 关键：先跑 V48 创建 (action_type, action_key, workspace_id) 索引
        V48ScopeTodosActionKeyByWorkspace
            .up(&db)
            .await
            .unwrap();
        // 不同 workspace 各自建一个 todo
        let ws1 = db
            .create_project_directory("/tmp/v48-ws-1", None, false, false)
            .await
            .unwrap();
        let ws2 = db
            .create_project_directory("/tmp/v48-ws-2", None, false, false)
            .await
            .unwrap();
        // create_todo_with_extras 不会自动设 action_type/action_key，
        // 所以用 find_or_create_blackboard_todo 的等价路径：
        // 调 create_todo_with_extras 再 update_todo_full 设 action 字段。
        let todo1 = db
            .create_todo_with_extras("t1", "p1", None, None, false, ws1, "/tmp/v48-ws-1")
            .await
            .unwrap();
        let todo2 = db
            .create_todo_with_extras("t2", "p2", None, None, false, ws2, "/tmp/v48-ws-2")
            .await
            .unwrap();
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
        // 关键断言：V46 唯一索引会拒绝第二条；V48 应当通过
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
        .expect("per-workspace action template should be allowed after V48");
    }
}
