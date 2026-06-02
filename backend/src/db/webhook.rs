use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};
use crate::db::Database;
use crate::db::entity::webhooks;
use crate::db::entity::webhook_records;
use crate::db::entity::todos;

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

    /// Get multiple webhooks by ids (batch query).
    pub async fn get_webhooks_by_ids(&self, ids: &[i64]) -> Result<Vec<webhooks::Model>, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        webhooks::Entity::find()
            .filter(webhooks::Column::Id.is_in(ids.iter().cloned()))
            .all(&self.conn)
            .await
    }

    /// Get multiple todos by ids (batch query).
    pub async fn get_todos_by_ids(&self, ids: &[i64]) -> Result<Vec<todos::Model>, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        todos::Entity::find()
            .filter(todos::Column::Id.is_in(ids.iter().cloned()))
            .all(&self.conn)
            .await
    }

    /// Get an enabled webhook that has the given todo_id as its default_todo_id.
    pub async fn get_webhook_by_default_todo(&self, todo_id: i64) -> Result<Option<webhooks::Model>, sea_orm::DbErr> {
        webhooks::Entity::find()
            .filter(webhooks::Column::Enabled.eq(true))
            .filter(webhooks::Column::DefaultTodoId.eq(todo_id))
            .one(&self.conn)
            .await
    }

    /// Create a new webhook.
    pub async fn create_webhook(
        &self,
        name: &str,
        enabled: bool,
        default_todo_id: Option<i64>,
    ) -> Result<webhooks::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = webhooks::ActiveModel {
            name: ActiveValue::Set(name.to_string()),
            enabled: ActiveValue::Set(enabled),
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

    /// Delete webhook records older than the specified number of days.
    /// Returns the number of records deleted.
    pub async fn cleanup_old_webhook_records(&self, days: i64) -> Result<u64, sea_orm::DbErr> {
        let cutoff = format!(
            "'{}'",
            chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::days(days))
                .unwrap()
                .format("%Y-%m-%dT%H:%M:%SZ")
        );
        let deleted = webhook_records::Entity::delete_many()
            .filter(webhook_records::Column::CreatedAt.lt(cutoff))
            .exec(&self.conn)
            .await?;
        Ok(deleted.rows_affected)
    }
}
