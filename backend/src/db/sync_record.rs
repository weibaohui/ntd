//! Sync records database operations
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait, QueryOrder, QuerySelect};
use crate::db::Database;
use crate::db::entity::sync_records;

impl Database {
    /// 创建同步记录
    pub async fn create_sync_record(
        &self,
        direction: &str,
        conflict_mode: &str,
        status: &str,
        data_type: &str,
        details: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<i64, sea_orm::DbErr> {
        let am = sync_records::ActiveModel {
            direction: ActiveValue::Set(direction.to_string()),
            conflict_mode: ActiveValue::Set(conflict_mode.to_string()),
            status: ActiveValue::Set(status.to_string()),
            data_type: ActiveValue::Set(data_type.to_string()),
            details: ActiveValue::Set(details.map(|s| s.to_string())),
            error_message: ActiveValue::Set(error_message.map(|s| s.to_string())),
            ..Default::default()
        };
        let result = am.insert(&self.conn).await?;
        Ok(result.id)
    }

    /// 获取同步记录列表
    pub async fn get_sync_records(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<sync_records::Model>, sea_orm::DbErr> {
        sync_records::Entity::find()
            .order_by_desc(sync_records::Column::CreatedAt)
            .limit(limit as u64)
            .offset(offset as u64)
            .all(&self.conn)
            .await
    }
}
