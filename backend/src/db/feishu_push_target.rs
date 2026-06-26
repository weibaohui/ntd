use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use crate::db::Database;
use crate::db::entity::feishu_push_targets;

impl Database {
    /// Get or create a push target row for a bot. Returns the active model for mutation.
    async fn get_or_create_push_target(
        &self,
        bot_id: i64,
    ) -> Result<feishu_push_targets::ActiveModel, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = feishu_push_targets::Entity::find()
            .filter(feishu_push_targets::Column::BotId.eq(bot_id))
            .one(&self.conn)
            .await?;

        Ok(match existing {
            Some(m) => m.into(),
            None => feishu_push_targets::ActiveModel {
                bot_id: ActiveValue::Set(bot_id),
                p2p_receive_id: ActiveValue::Set(String::new()),
                group_chat_id: ActiveValue::Set(String::new()),
                receive_id_type: ActiveValue::Set("open_id".to_string()),
                push_level: ActiveValue::Set("result_only".to_string()),
                p2p_response_enabled: ActiveValue::Set(true),
                group_response_enabled: ActiveValue::Set(true),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            },
        })
    }

    /// Set the p2p (single chat) receive ID for a bot. Only touches p2p_receive_id.
    pub async fn set_p2p_receive_id(
        &self,
        bot_id: i64,
        p2p_receive_id: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let mut am = self.get_or_create_push_target(bot_id).await?;
        am.p2p_receive_id = ActiveValue::Set(p2p_receive_id.to_string());
        am.updated_at = ActiveValue::Set(Some(now));
        am.save(&self.conn).await?;
        Ok(())
    }

    /// Set the group chat ID for a bot. Only touches group_chat_id.
    pub async fn set_group_chat_id(
        &self,
        bot_id: i64,
        group_chat_id: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let mut am = self.get_or_create_push_target(bot_id).await?;
        am.group_chat_id = ActiveValue::Set(group_chat_id.to_string());
        am.updated_at = ActiveValue::Set(Some(now));
        am.save(&self.conn).await?;
        Ok(())
    }

    /// Update push level for a bot.
    pub async fn update_feishu_push_level(
        &self,
        bot_id: i64,
        push_level: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let mut am = self.get_or_create_push_target(bot_id).await?;
        am.push_level = ActiveValue::Set(push_level.to_string());
        am.updated_at = ActiveValue::Set(Some(now));
        am.save(&self.conn).await?;
        Ok(())
    }

    /// Update receive_id_type (send type) for a bot.
    pub async fn update_receive_id_type(
        &self,
        bot_id: i64,
        receive_id_type: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let mut am = self.get_or_create_push_target(bot_id).await?;
        am.receive_id_type = ActiveValue::Set(receive_id_type.to_string());
        am.updated_at = ActiveValue::Set(Some(now));
        am.save(&self.conn).await?;
        Ok(())
    }

    /// Get the push target for a bot.
    pub async fn get_feishu_push_target(
        &self,
        bot_id: i64,
    ) -> Result<Option<feishu_push_targets::Model>, sea_orm::DbErr> {
        feishu_push_targets::Entity::find()
            .filter(feishu_push_targets::Column::BotId.eq(bot_id))
            .one(&self.conn)
            .await
    }

    /// Get all push targets with push_level != "disabled", grouped by workspace_id.
    /// Returns (workspace_id, targets). workspace_id = None means bots without workspace.
    pub async fn get_all_push_targets_by_workspace(
        &self,
    ) -> Result<std::collections::HashMap<Option<i64>, Vec<(i64, String, String, String)>>, sea_orm::DbErr> {
        use std::collections::HashMap;
        use sea_orm::EntityTrait;

        let targets = feishu_push_targets::Entity::find()
            .all(&self.conn)
            .await?;

        let mut map: HashMap<Option<i64>, Vec<(i64, String, String, String)>> = HashMap::new();

        for t in targets.into_iter().filter(|t| t.push_level != "disabled") {
            let receive_id = match t.receive_id_type.as_str() {
                "chat_id" => t.group_chat_id.clone(),
                _ => t.p2p_receive_id.clone(),
            };
            if receive_id.is_empty() {
                continue;
            }

            // Get bot's workspace_id directly from agent_bots table
            let workspace_id = self.get_agent_bot_workspace_id(t.bot_id).await?;

            let entry = map.entry(workspace_id).or_default();
            entry.push((t.bot_id, receive_id, t.receive_id_type.clone(), t.push_level.clone()));
        }

        Ok(map)
    }



    /// Get all (bot_id, group_chat_id) pairs where group_chat_id is set.
    pub async fn get_group_chat_ids(
        &self,
    ) -> Result<Vec<(i64, String)>, sea_orm::DbErr> {
        let targets = feishu_push_targets::Entity::find()
            .all(&self.conn)
            .await?;
        Ok(targets
            .into_iter()
            .filter(|t| !t.group_chat_id.is_empty())
            .map(|t| (t.bot_id, t.group_chat_id))
            .collect())
    }
}
