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
        // 显式设置 created_at，避免 SQLite 把 NULL 排到结果最前面
        let now = chrono::Utc::now().to_rfc3339();
        let am = sync_records::ActiveModel {
            direction: ActiveValue::Set(direction.to_string()),
            conflict_mode: ActiveValue::Set(conflict_mode.to_string()),
            status: ActiveValue::Set(status.to_string()),
            data_type: ActiveValue::Set(data_type.to_string()),
            details: ActiveValue::Set(details.map(|s| s.to_string())),
            error_message: ActiveValue::Set(error_message.map(|s| s.to_string())),
            created_at: ActiveValue::Set(Some(now)),
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
        // 按 id 倒序：id 是自增主键，等同于按插入时间倒序，
        // 而且避免了 created_at 为 NULL 时 SQLite 排序不稳定的问题
        sync_records::Entity::find()
            .order_by_desc(sync_records::Column::Id)
            .limit(limit as u64)
            .offset(offset as u64)
            .all(&self.conn)
            .await
    }

    /// 统计同步记录总数，给前端分页用
    pub async fn count_sync_records(&self) -> Result<i64, sea_orm::DbErr> {
        use sea_orm::PaginatorTrait;
        let n = sync_records::Entity::find()
            .count(&self.conn)
            .await?;
        Ok(n as i64)
    }

    /// 清空全部同步历史，返回实际删除条数
    pub async fn clear_sync_records(&self) -> Result<u64, sea_orm::DbErr> {
        let res = sync_records::Entity::delete_many()
            .exec(&self.conn)
            .await?;
        Ok(res.rows_affected)
    }
}
