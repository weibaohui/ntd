use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};
use crate::db::Database;
use crate::db::entity::webhooks;
use crate::db::entity::webhook_records;

#[derive(Debug, Clone)]
pub struct NewWebhookRecord {
    pub webhook_id: Option<i64>,
    pub method: String,
    pub path: String,
    pub query_params: Option<String>,
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub triggered_todo_id: Option<i64>,
    pub status_code: Option<i32>,
    pub response_body: Option<String>,
}

impl Database {
    /// Get all webhooks.
    pub async fn get_webhooks(&self) -> Result<Vec<webhooks::Model>, sea_orm::DbErr> {
        webhooks::Entity::find()
            .all(&self.conn)
            .await
    }

    /// Get webhook by id.
    pub async fn get_webhook(&self, id: i64) -> Result<Option<webhooks::Model>, sea_orm::DbErr> {
        webhooks::Entity::find_by_id(id)
            .one(&self.conn)
            .await
    }

    /// Get the first enabled webhook (default webhook).
    pub async fn get_default_webhook(&self) -> Result<Option<webhooks::Model>, sea_orm::DbErr> {
        webhooks::Entity::find()
            .filter(webhooks::Column::Enabled.eq(true))
            .one(&self.conn)
            .await
    }

    /// Create a new webhook.
    pub async fn create_webhook(
        &self,
        name: &str,
        default_todo_id: Option<i64>,
    ) -> Result<webhooks::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = webhooks::ActiveModel {
            name: ActiveValue::Set(name.to_string()),
            enabled: ActiveValue::Set(true),
            default_todo_id: ActiveValue::Set(default_todo_id),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// Update a webhook.
    pub async fn update_webhook(
        &self,
        id: i64,
        name: &str,
        enabled: bool,
        default_todo_id: Option<i64>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = webhooks::Entity::find_by_id(id)
            .one(&self.conn)
            .await?;

        if let Some(c) = existing {
            let mut am: webhooks::ActiveModel = c.into();
            am.name = ActiveValue::Set(name.to_string());
            am.enabled = ActiveValue::Set(enabled);
            am.default_todo_id = ActiveValue::Set(default_todo_id);
            am.updated_at = ActiveValue::Set(Some(now));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// Delete a webhook.
    pub async fn delete_webhook(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        webhooks::Entity::delete_by_id(id)
            .exec(&self.conn)
            .await?;
        Ok(())
    }

    /// Create a new webhook record.
    pub async fn create_webhook_record(
        &self,
        record: NewWebhookRecord,
    ) -> Result<webhook_records::Model, sea_orm::DbErr> {
        let am = webhook_records::ActiveModel {
            webhook_id: ActiveValue::Set(record.webhook_id),
            method: ActiveValue::Set(record.method),
            path: ActiveValue::Set(record.path),
            query_params: ActiveValue::Set(record.query_params),
            body: ActiveValue::Set(record.body),
            content_type: ActiveValue::Set(record.content_type),
            triggered_todo_id: ActiveValue::Set(record.triggered_todo_id),
            status_code: ActiveValue::Set(record.status_code),
            response_body: ActiveValue::Set(record.response_body),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// Get webhook records with optional pagination.
    pub async fn get_webhook_records(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<webhook_records::Model>, sea_orm::DbErr> {
        webhook_records::Entity::find()
            .order_by_desc(webhook_records::Column::Id)
            .limit(limit as u64)
            .offset(offset as u64)
            .all(&self.conn)
            .await
    }

    /// Get webhook record by id.
    pub async fn get_webhook_record(&self, id: i64) -> Result<Option<webhook_records::Model>, sea_orm::DbErr> {
        webhook_records::Entity::find_by_id(id)
            .one(&self.conn)
            .await
    }

    /// Get total count of webhook records.
    pub async fn get_webhook_records_count(&self) -> Result<i64, sea_orm::DbErr> {
        webhook_records::Entity::find()
            .count(&self.conn)
            .await
            .map(|c| c as i64)
    }
}
