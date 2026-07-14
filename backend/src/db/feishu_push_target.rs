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

        // 复杂 tuple 类型提取为 type alias，提升可读性
        type TargetEntry = (i64, String, String, String);
        let mut map: HashMap<Option<i64>, Vec<TargetEntry>> = HashMap::new();

        for t in targets.into_iter().filter(|t| t.push_level != "disabled") {
            // 推送目标恒为 bot 所有者私聊（owner_open_id），不再走群聊/私聊二选一分支。
            // receive_id_type 固定为 open_id，从根本上消除了旧的 receive_id_type 开关 bug。
            let Some(owner) = self.get_owner_open_id(t.bot_id).await? else {
                // owner_open_id 未设置：定时/Web 触发的任务无处可推，记 warn 跳过（决策点5）。
                // 提示与 bot 私聊一次以触发兜底捕获，而非静默丢失。
                tracing::warn!(
                    "[feishu-push] bot {} 的 owner_open_id 未设置，跳过推送（请先与 bot 私聊一次）",
                    t.bot_id
                );
                continue;
            };
            // workspace_id 直接取自 agent_bots 表
            let workspace_id = self.get_agent_bot_workspace_id(t.bot_id).await?;
            map.entry(workspace_id).or_default().push((
                t.bot_id,
                owner,
                "open_id".to_string(),
                t.push_level.clone(),
            ));
        }

        Ok(map)
    }
}
