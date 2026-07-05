use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait, QueryOrder};
use crate::db::Database;
use crate::db::entity::agent_bots;
use crate::db::entity::feishu_response_config;
use crate::models::AgentBot;

fn map_bot(m: agent_bots::Model) -> AgentBot {
    AgentBot {
        id: m.id,
        bot_type: m.bot_type,
        bot_name: m.bot_name,
        app_id: m.app_id,
        app_secret: m.app_secret,
        bot_open_id: m.bot_open_id,
        domain: m.domain,
        enabled: m.enabled.unwrap_or(true),
        config: m.config.unwrap_or_else(|| "{}".to_string()),
        created_at: m.created_at.unwrap_or_default(),
        workspace_id: m.workspace_id,
    }
}

impl Database {
    pub async fn get_agent_bots(&self) -> Result<Vec<AgentBot>, sea_orm::DbErr> {
        let models = agent_bots::Entity::find()
            .order_by_desc(agent_bots::Column::Id)
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(map_bot).collect())
    }

    /// 参数数量由 agent_bots 表 schema 决定
    #[allow(clippy::too_many_arguments)]
    pub async fn create_agent_bot(
        &self,
        bot_type: &str,
        bot_name: &str,
        app_id: &str,
        app_secret: &str,
        bot_open_id: Option<String>,
        domain: Option<String>,
        workspace_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = agent_bots::ActiveModel {
            bot_type: ActiveValue::Set(bot_type.to_string()),
            bot_name: ActiveValue::Set(bot_name.to_string()),
            app_id: ActiveValue::Set(app_id.to_string()),
            app_secret: ActiveValue::Set(app_secret.to_string()),
            bot_open_id: ActiveValue::Set(bot_open_id),
            domain: ActiveValue::Set(domain),
            workspace_id: ActiveValue::Set(workspace_id),
            enabled: ActiveValue::Set(Some(true)),
            config: ActiveValue::Set(Some("{}".to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;

        // For Feishu bots, also create response config records for p2p and group
        if bot_type == "feishu" {
            for target_type in &["p2p", "group"] {
                let config_am = feishu_response_config::ActiveModel {
                    bot_id: ActiveValue::Set(inserted.id),
                    target_type: ActiveValue::Set(target_type.to_string()),
                    enabled: ActiveValue::Set(true),
                    debounce_secs: ActiveValue::Set(Some(20)),
                    created_at: ActiveValue::Set(Some(now.clone())),
                    updated_at: ActiveValue::Set(Some(now.clone())),
                    ..Default::default()
                };
                let _ = config_am.insert(&self.conn).await;
            }
        }

        Ok(inserted.id)
    }

    pub async fn delete_agent_bot(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        // Child rows in feishu_* tables are cleaned up by ON DELETE CASCADE.
        agent_bots::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }

    pub async fn get_agent_bot(&self, id: i64) -> Result<Option<AgentBot>, sea_orm::DbErr> {
        let model = agent_bots::Entity::find_by_id(id)
            .one(&self.conn)
            .await?;
        Ok(model.map(map_bot))
    }

    pub async fn update_agent_bot_config(&self, id: i64, config: &str) -> Result<(), sea_orm::DbErr> {
        let bot = agent_bots::Entity::find_by_id(id).one(&self.conn).await?;
        // 幽灵 id 静默 no-op 是历史契约；显式 warn 让上游 handler 传错 id 时
        // 能在日志里看到提示,避免配置错误被静默吞掉。
        let Some(bot) = bot else {
            tracing::warn!(
                bot_id = id,
                "update_agent_bot_config called with unknown bot id; no-op"
            );
            return Ok(());
        };
        let mut am: agent_bots::ActiveModel = bot.into();
        am.config = ActiveValue::Set(Some(config.to_string()));
        am.update(&self.conn).await?;
        Ok(())
    }

    /// 获取 bot 的 workspace_id
    pub async fn get_agent_bot_workspace_id(&self, bot_id: i64) -> Result<Option<i64>, sea_orm::DbErr> {
        let bot = agent_bots::Entity::find_by_id(bot_id).one(&self.conn).await?;
        Ok(bot.map(|b| b.workspace_id))
    }

    /// 更新 bot 的 workspace_id（仅变更 workspace 时调用）
    ///
    /// 注意：此方法仅更新 workspace_id 字段本身，不执行级联逻辑。
    /// 级联禁用 binding 由调用方在 handler 层负责。
    pub async fn update_agent_bot_workspace_id(&self, bot_id: i64, workspace_id: i64) -> Result<(), sea_orm::DbErr> {
        let bot = agent_bots::Entity::find_by_id(bot_id).one(&self.conn).await?;
        let Some(bot) = bot else {
            return Ok(());
        };
        let mut am: agent_bots::ActiveModel = bot.into();
        am.workspace_id = ActiveValue::Set(workspace_id);
        am.update(&self.conn).await?;
        Ok(())
    }

    /// 获取 workspace 名称（通过 workspace_id 查 project_directories 表）
    pub async fn get_workspace_name_by_id(&self, workspace_id: i64) -> Result<Option<String>, sea_orm::DbErr> {
        use crate::db::entity::project_directories;
        let ws = project_directories::Entity::find_by_id(workspace_id).one(&self.conn).await?;
        Ok(ws.and_then(|w| w.name))
    }
}
