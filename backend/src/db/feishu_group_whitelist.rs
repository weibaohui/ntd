use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use crate::db::Database;
use crate::db::entity::feishu_group_whitelist;

impl Database {
    /// Check if a sender is in the whitelist for a bot.
    /// Returns true if whitelist is empty (no restriction) or sender is in the list.
    pub async fn is_sender_in_whitelist(
        &self,
        bot_id: i64,
        sender_open_id: &str,
    ) -> Result<bool, sea_orm::DbErr> {
        let list = feishu_group_whitelist::Entity::find()
            .filter(feishu_group_whitelist::Column::BotId.eq(bot_id))
            .all(&self.conn)
            .await?;

        // Empty whitelist means no restriction (allow all)
        if list.is_empty() {
            return Ok(true);
        }

        // Fail Closed: if any entry has empty sender_open_id, treat as misconfiguration
        // and deny access (rather than allowing all). Log a warning for investigation.
        if list.iter().any(|w| w.sender_open_id.is_empty()) {
            tracing::warn!("[whitelist] bot_id={} has entries with empty sender_open_id, treating as misconfiguration (deny all)", bot_id);
            return Ok(false);
        }

        Ok(list.iter().any(|w| w.sender_open_id == sender_open_id))
    }

    /// Get all whitelist entries for a bot.
    pub async fn get_group_whitelist(
        &self,
        bot_id: i64,
    ) -> Result<Vec<feishu_group_whitelist::Model>, sea_orm::DbErr> {
        feishu_group_whitelist::Entity::find()
            .filter(feishu_group_whitelist::Column::BotId.eq(bot_id))
            .all(&self.conn)
            .await
    }

    /// Add a sender to the whitelist. Returns existing entry if duplicate.
    /// Returns error if sender_open_id is empty (empty entries would bypass whitelist).
    pub async fn add_group_whitelist(
        &self,
        bot_id: i64,
        sender_open_id: &str,
        sender_name: Option<&str>,
    ) -> Result<feishu_group_whitelist::Model, sea_orm::DbErr> {
        // Validate sender_open_id is non-empty to prevent accidental whitelist bypass
        if sender_open_id.is_empty() {
            return Err(sea_orm::DbErr::Custom("sender_open_id cannot be empty".to_string()));
        }

        // Check if already exists
        if let Some(existing) = feishu_group_whitelist::Entity::find()
            .filter(feishu_group_whitelist::Column::BotId.eq(bot_id))
            .filter(feishu_group_whitelist::Column::SenderOpenId.eq(sender_open_id))
            .one(&self.conn)
            .await?
        {
            return Ok(existing);
        }

        let now = crate::models::utc_timestamp();
        let am = feishu_group_whitelist::ActiveModel {
            bot_id: ActiveValue::Set(bot_id),
            sender_open_id: ActiveValue::Set(sender_open_id.to_string()),
            sender_name: ActiveValue::Set(sender_name.map(String::from)),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// Remove a sender from the whitelist.
    pub async fn remove_group_whitelist(
        &self,
        id: i64,
    ) -> Result<(), sea_orm::DbErr> {
        feishu_group_whitelist::Entity::delete_by_id(id)
            .exec(&self.conn)
            .await?;
        Ok(())
    }
}