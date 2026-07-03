//! 黑板（Blackboard）数据库层方法。
//!
//! 提供黑板的 CRUD 操作，每个工作空间最多一条黑板记录（由 UNIQUE 约束保证）。

use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait,
    QueryFilter, UpdateResult,
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
        // 用 utc_timestamp() 统一时间源，避免不同 DB driver 时区差异
        let now = crate::models::utc_timestamp();
        // 构造 ActiveModel：除主键外的字段显式赋值，主键交由 SQLite 自增
        let model = blackboards::ActiveModel {
            workspace_id: ActiveValue::Set(workspace_id),
            // 初始内容为空：创建时的黑板无内容，由后续 LLM 更新填充
            content: ActiveValue::Set(String::new()),
            updated_at: ActiveValue::Set(Some(now.clone())),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        model.insert(&self.conn).await
    }

    /// 更新指定工作空间的黑板内容（记录必须已存在）。
    ///
    /// 性能取舍：单条 `UPDATE ... WHERE workspace_id = ?`，避免原先 SELECT-then-UPDATE
    /// 的两次往返 + TOCTOU 窗口。如果记录不存在，rows_affected = 0，
    /// 返回 `RecordNotFound` 让调用方能识别这种情况。
    pub async fn update_blackboard_content(
        &self,
        workspace_id: i64,
        content: &str,
    ) -> Result<(), sea_orm::DbErr> {
        // 时间戳：单独变量确保 created_at / updated_at 用同一时刻
        let now = crate::models::utc_timestamp();
        // 单语句 UPDATE：workspace_id 是 UNIQUE 索引，命中后只更新一行
        let res: UpdateResult = blackboards::Entity::update_many()
            .col_expr(blackboards::Column::Content, content.into())
            .col_expr(blackboards::Column::UpdatedAt, now.into())
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .exec(&self.conn)
            .await?;
        // rows_affected == 0 表示记录不存在（区别于"存在但内容相同"的 0 变更）
        if res.rows_affected == 0 {
            return Err(sea_orm::DbErr::RecordNotFound(format!(
                "blackboard for workspace {} not found",
                workspace_id
            )));
        }
        Ok(())
    }

    /// Upsert 黑板内容：记录不存在则创建，存在则更新。
    ///
    /// 通过 `INSERT ... ON CONFLICT(workspace_id) DO UPDATE` 一次往返完成
    /// 创建/更新判断 + 写入，避免 service 层先 get 再 create 再 update 的 3 次往返。
    /// 用 workspace_id 唯一约束做冲突判定，与 schema UNIQUE 保持一致。
    pub async fn upsert_blackboard_content(
        &self,
        workspace_id: i64,
        content: &str,
    ) -> Result<(), sea_orm::DbErr> {
        // 同一时刻填充 created_at 和 updated_at：upsert 时两个字段语义一致
        let now = crate::models::utc_timestamp();
        // 构造 ActiveModel：与 create_blackboard 保持一致的初始结构
        let am = blackboards::ActiveModel {
            workspace_id: ActiveValue::Set(workspace_id),
            content: ActiveValue::Set(content.to_string()),
            updated_at: ActiveValue::Set(Some(now.clone())),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        // ON CONFLICT(workspace_id)：命中后只覆盖 content/updated_at，保留 created_at
        blackboards::Entity::insert(am)
            .on_conflict(
                OnConflict::column(blackboards::Column::WorkspaceId)
                    .update_columns([blackboards::Column::Content, blackboards::Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.conn)
            .await?;
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

    /// 验证 upsert_blackboard_content 在记录不存在时直接创建。
    #[tokio::test]
    async fn test_upsert_creates_when_missing() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let ws_id = create_test_workspace(&db).await;

        // 首次 upsert：记录不存在，应当走 INSERT 分支
        db.upsert_blackboard_content(ws_id, "# 初始内容")
            .await
            .unwrap();

        let fetched = db.get_blackboard(ws_id).await.unwrap().unwrap();
        assert_eq!(fetched.content, "# 初始内容");
    }

    /// 验证 upsert_blackboard_content 在记录已存在时更新内容并保留 created_at。
    #[tokio::test]
    async fn test_upsert_updates_when_exists() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let ws_id = create_test_workspace(&db).await;

        // 先 upsert 一次拿到初始记录
        db.upsert_blackboard_content(ws_id, "# 第一次")
            .await
            .unwrap();
        let first = db.get_blackboard(ws_id).await.unwrap().unwrap();
        let first_created = first.created_at.clone();
        let first_id = first.id;

        // 二次 upsert：ON CONFLICT 分支，应当覆盖 content 但保留 id/created_at
        db.upsert_blackboard_content(ws_id, "# 第二次")
            .await
            .unwrap();
        let second = db.get_blackboard(ws_id).await.unwrap().unwrap();

        assert_eq!(second.id, first_id, "upsert 不应改变主键");
        assert_eq!(second.content, "# 第二次", "content 应当被覆盖");
        assert_eq!(second.created_at, first_created, "created_at 应当保留");
    }
}
