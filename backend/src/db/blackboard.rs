//! 黑板（Blackboard）数据库层方法。
//!
//! 提供黑板的 CRUD 操作，每个工作空间最多一条黑板记录（由 UNIQUE 约束保证）。

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter,
};

use super::entity::blackboards;
use super::Database;

impl Database {
    /// 根据 workspace_id 获取黑板内容。
    ///
    /// 返回 Option<blackboards::Model>，None 表示该工作空间还没有黑板记录。
    /// 新工作空间首次访问时返回 None，由 Service 层的 find_or_create 方法处理初始化。
    pub async fn get_blackboard(
        &self,
        workspace_id: i64,
    ) -> Result<Option<blackboards::Model>, sea_orm::DbErr> {
        blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await
    }

    /// 为指定工作空间创建一条空的黑板记录。
    ///
    /// 如果该工作空间已有黑板记录，会因 UNIQUE 约束失败返回 Err。
    /// 调用方应先调用 get_blackboard 检查是否存在，或使用 Service 层的 find_or_create 封装方法。
    pub async fn create_blackboard(
        &self,
        workspace_id: i64,
    ) -> Result<blackboards::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let model = blackboards::ActiveModel {
            workspace_id: ActiveValue::Set(workspace_id),
            content: ActiveValue::Set(String::new()),
            updated_at: ActiveValue::Set(Some(now.clone())),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        model.insert(&self.conn).await
    }

    /// 更新指定工作空间的黑板内容。
    ///
    /// 如果该工作空间没有黑板记录，会因 Foreign Key 约束失败返回 Err。
    /// 调用方应先确保黑板记录存在（get_blackboard 返回 Some），
    /// 或使用 Service 层的 find_or_create_blackboard 封装方法。
    pub async fn update_blackboard_content(
        &self,
        workspace_id: i64,
        content: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let model = blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await?;

        let Some(model) = model else {
            // 记录不存在时返回 RecordNotFound 错误
            return Err(sea_orm::DbErr::RecordNotFound(format!(
                "blackboard for workspace {} not found",
                workspace_id
            )));
        };

        let mut am: blackboards::ActiveModel = model.into();
        am.content = ActiveValue::Set(content.to_string());
        am.updated_at = ActiveValue::Set(Some(now));
        am.update(&self.conn).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 创建一个测试用工作空间（project_directories），返回其 id。
    async fn create_test_workspace(db: &Database) -> i64 {
        db.create_project_directory("/tmp/test-blackboard-workspace", None, false, false)
            .await
            .expect("create workspace must succeed")
    }

    /// 验证 get_blackboard 在无记录时返回 None。
    #[tokio::test]
    async fn test_get_blackboard_returns_none_when_empty() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        // 不存在的 workspace_id 应返回 None
        let result = db.get_blackboard(999).await.unwrap();
        assert!(result.is_none());
    }

    /// 验证 create_blackboard 成功创建一条空黑板记录。
    #[tokio::test]
    async fn test_create_blackboard_success() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let ws_id = create_test_workspace(&db).await;

        let board = db.create_blackboard(ws_id).await.unwrap();
        assert_eq!(board.workspace_id, ws_id);
        assert_eq!(board.content, "");
        assert!(board.created_at.is_some());
        assert!(board.updated_at.is_some());

        // 验证可通过 get 查到
        let fetched = db.get_blackboard(ws_id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, board.id);
    }

    /// 验证 update_blackboard_content 更新成功。
    #[tokio::test]
    async fn test_update_blackboard_content_success() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let ws_id = create_test_workspace(&db).await;
        let _ = db.create_blackboard(ws_id).await.unwrap();

        db.update_blackboard_content(ws_id, "# 更新后的内容")
            .await
            .unwrap();

        let fetched = db.get_blackboard(ws_id).await.unwrap().unwrap();
        assert_eq!(fetched.content, "# 更新后的内容");
    }

    /// 验证 update_blackboard_content 在不存在的 workspace 上返回 RecordNotFound。
    #[tokio::test]
    async fn test_update_blackboard_content_record_not_found() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let result = db.update_blackboard_content(999, "# test").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            sea_orm::DbErr::RecordNotFound(_) => {} // 期望的错误类型
            other => panic!("expected RecordNotFound, got: {:?}", other),
        }
    }
}
