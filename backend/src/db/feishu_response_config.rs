use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use crate::db::Database;
use crate::db::entity::feishu_response_config;

impl Database {
    /// Get response enabled status for a specific bot and target type.
    pub async fn get_feishu_response_enabled(
        &self,
        bot_id: i64,
        target_type: &str,
    ) -> Result<bool, sea_orm::DbErr> {
        let config = feishu_response_config::Entity::find()
            .filter(feishu_response_config::Column::BotId.eq(bot_id))
            .filter(feishu_response_config::Column::TargetType.eq(target_type))
            .one(&self.conn)
            .await?;

        Ok(config.map(|c| c.enabled).unwrap_or(false))
    }

    /// Set response enabled status for a specific bot and target type.
    pub async fn set_feishu_response_enabled(
        &self,
        bot_id: i64,
        target_type: &str,
        enabled: bool,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();

        let existing = feishu_response_config::Entity::find()
            .filter(feishu_response_config::Column::BotId.eq(bot_id))
            .filter(feishu_response_config::Column::TargetType.eq(target_type))
            .one(&self.conn)
            .await?;

        if let Some(c) = existing {
            let mut am: feishu_response_config::ActiveModel = c.into();
            am.enabled = ActiveValue::Set(enabled);
            am.updated_at = ActiveValue::Set(Some(now));
            am.update(&self.conn).await?;
        } else {
            let am = feishu_response_config::ActiveModel {
                bot_id: ActiveValue::Set(bot_id),
                target_type: ActiveValue::Set(target_type.to_string()),
                enabled: ActiveValue::Set(enabled),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            };
            am.insert(&self.conn).await?;
        }

        Ok(())
    }

    /// Get debounce seconds for a specific bot and target type. Default 20.
    pub async fn get_debounce_secs(
        &self,
        bot_id: i64,
        target_type: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        let config = feishu_response_config::Entity::find()
            .filter(feishu_response_config::Column::BotId.eq(bot_id))
            .filter(feishu_response_config::Column::TargetType.eq(target_type))
            .one(&self.conn)
            .await?;

        Ok(config.and_then(|c| c.debounce_secs).unwrap_or(20))
    }

    /// Set debounce seconds for a specific bot and target type.
    pub async fn set_debounce_secs(
        &self,
        bot_id: i64,
        target_type: &str,
        debounce_secs: i64,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();

        let existing = feishu_response_config::Entity::find()
            .filter(feishu_response_config::Column::BotId.eq(bot_id))
            .filter(feishu_response_config::Column::TargetType.eq(target_type))
            .one(&self.conn)
            .await?;

        if let Some(c) = existing {
            let mut am: feishu_response_config::ActiveModel = c.into();
            am.debounce_secs = ActiveValue::Set(Some(debounce_secs));
            am.updated_at = ActiveValue::Set(Some(now));
            am.update(&self.conn).await?;
        } else {
            let am = feishu_response_config::ActiveModel {
                bot_id: ActiveValue::Set(bot_id),
                target_type: ActiveValue::Set(target_type.to_string()),
                enabled: ActiveValue::Set(true),
                debounce_secs: ActiveValue::Set(Some(debounce_secs)),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            };
            am.insert(&self.conn).await?;
        }

        Ok(())
    }
}