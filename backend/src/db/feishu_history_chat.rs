use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use crate::db::Database;
use crate::db::entity::feishu_history_chats;

#[derive(Debug, Clone)]
pub struct FeishuHistoryChatRecord {
    pub id: i64,
    pub bot_id: i64,
    pub chat_id: String,
    pub chat_name: Option<String>,
    pub enabled: bool,
    pub last_fetch_time: Option<String>,
    pub polling_interval_secs: i32,
    pub created_at: Option<String>,
}

impl Database {
    pub async fn create_feishu_history_chat(
        &self,
        bot_id: i64,
        chat_id: &str,
        chat_name: Option<&str>,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = feishu_history_chats::ActiveModel {
            bot_id: ActiveValue::Set(bot_id),
            chat_id: ActiveValue::Set(chat_id.to_string()),
            chat_name: ActiveValue::Set(chat_name.map(String::from)),
            enabled: ActiveValue::Set(Some(true)),
            last_fetch_time: ActiveValue::Set(None),
            polling_interval_secs: ActiveValue::Set(Some(60)),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    pub async fn get_feishu_history_chats(
        &self,
        bot_id: i64,
    ) -> Result<Vec<FeishuHistoryChatRecord>, sea_orm::DbErr> {
        let models = feishu_history_chats::Entity::find()
            .filter(feishu_history_chats::Column::BotId.eq(bot_id))
            .all(&self.conn)
            .await?;

        Ok(models
            .into_iter()
            .map(|m| FeishuHistoryChatRecord {
                id: m.id,
                bot_id: m.bot_id,
                chat_id: m.chat_id,
                chat_name: m.chat_name,
                enabled: m.enabled.unwrap_or(true),
                last_fetch_time: m.last_fetch_time,
                polling_interval_secs: m.polling_interval_secs.unwrap_or(60),
                created_at: m.created_at,
            })
            .collect())
    }

    pub async fn get_enabled_feishu_history_chats(
        &self,
        bot_id: i64,
    ) -> Result<Vec<FeishuHistoryChatRecord>, sea_orm::DbErr> {
        let models = feishu_history_chats::Entity::find()
            .filter(feishu_history_chats::Column::BotId.eq(bot_id))
            .filter(feishu_history_chats::Column::Enabled.eq(Some(true)))
            .all(&self.conn)
            .await?;

        Ok(models
            .into_iter()
            .map(|m| FeishuHistoryChatRecord {
                id: m.id,
                bot_id: m.bot_id,
                chat_id: m.chat_id,
                chat_name: m.chat_name,
                enabled: m.enabled.unwrap_or(true),
                last_fetch_time: m.last_fetch_time,
                polling_interval_secs: m.polling_interval_secs.unwrap_or(60),
                created_at: m.created_at,
            })
            .collect())
    }

    pub async fn update_feishu_history_chat(
        &self,
        id: i64,
        chat_name: Option<&str>,
        enabled: Option<bool>,
        polling_interval_secs: Option<i32>,
    ) -> Result<(), sea_orm::DbErr> {
        let model = feishu_history_chats::Entity::find_by_id(id)
            .one(&self.conn)
            .await?
            .ok_or(sea_orm::DbErr::RecordNotFound("Record not found".to_string()))?;

        let mut am: feishu_history_chats::ActiveModel = model.into();
        if let Some(name) = chat_name {
            am.chat_name = ActiveValue::Set(Some(name.to_string()));
        }
        if let Some(en) = enabled {
            am.enabled = ActiveValue::Set(Some(en));
        }
        if let Some(interval) = polling_interval_secs {
            am.polling_interval_secs = ActiveValue::Set(Some(interval));
        }

        am.update(&self.conn).await?;
        Ok(())
    }

    pub async fn delete_feishu_history_chat(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        let model = feishu_history_chats::Entity::find_by_id(id)
            .one(&self.conn)
            .await?
            .ok_or(sea_orm::DbErr::RecordNotFound("Record not found".to_string()))?;

        let am: feishu_history_chats::ActiveModel = model.into();
        am.delete(&self.conn).await?;
        Ok(())
    }
}
