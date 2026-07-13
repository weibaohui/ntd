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
        owner_open_id: m.owner_open_id,
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
            bot_open_id: ActiveValue::Set(bot_open_id.clone()),
            // 扫码创建时 bot_open_id 存的是扫码人 open_id（历史字段，被错位当作 bot 自己），
            // 而扫码人即 bot 所有者，故同时用它初始化 owner_open_id；
            // 非扫码创建两者均为 None，owner_open_id 改由首次私聊兜底写入。
            owner_open_id: ActiveValue::Set(bot_open_id),
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

    /// 设置 bot 的 owner_open_id，仅当当前值为空（NULL 或空串）时才写入。
    ///
    /// 护栏意义：第一个被捕获的私聊用户锁定为推送目标所有者，
    /// 后到的其他私聊用户不会覆盖它，避免定时推送被劫持到错误的人。
    /// 返回是否实际写入。
    pub async fn set_owner_open_id_if_empty(
        &self,
        bot_id: i64,
        open_id: &str,
    ) -> Result<bool, sea_orm::DbErr> {
        // 单条条件 UPDATE：只有 owner_open_id 为空/NULL 时才写，靠 rows_affected 判断是否命中。
        // 用原子 UPDATE 而非「读-改-写」：并发首次私聊时两个用户可能都读到空值，读-改-写会让
        // 后写者覆盖先写者、把推送目标劫持成自己（owner_open_id 是推送目标权威来源）。
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let result = self
            .conn
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE agent_bots SET owner_open_id = ? WHERE id = ? AND (owner_open_id IS NULL OR owner_open_id = '')",
                [open_id.to_string().into(), bot_id.into()],
            ))
            .await?;
        // rows_affected == 1：命中空值并写入；0：bot 不存在或 owner_open_id 已有值（含幽灵 id no-op 语义）
        Ok(result.rows_affected() == 1)
    }

    /// 获取 bot 的 owner_open_id（推送目标权威来源）。
    /// NULL 与空串都视为「未设置」，统一返回 None，方便调用方用 Option 判断。
    pub async fn get_owner_open_id(&self, bot_id: i64) -> Result<Option<String>, sea_orm::DbErr> {
        let bot = agent_bots::Entity::find_by_id(bot_id).one(&self.conn).await?;
        Ok(bot.and_then(|b| b.owner_open_id).filter(|s| !s.is_empty()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod owner_open_id_tests {
    use super::*;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect(":memory: db must open")
    }

    #[tokio::test]
    async fn test_set_owner_open_id_if_empty_writes_when_unset() {
        // 非扫码创建的 bot 无 owner_open_id，兜底写入应成功
        let db = fresh_db().await;
        let bot_id = db
            .create_agent_bot("feishu", "t", "app", "secret", None, None, 1)
            .await
            .unwrap();
        let wrote = db.set_owner_open_id_if_empty(bot_id, "ou_aaa").await.unwrap();
        assert!(wrote, "未设置时应写入");
        assert_eq!(
            db.get_owner_open_id(bot_id).await.unwrap(),
            Some("ou_aaa".to_string())
        );
    }

    #[tokio::test]
    async fn test_set_owner_open_id_if_empty_refuses_overwrite() {
        // 扫码创建即带 owner_open_id（=bot_open_id），另一用户私聊不应覆盖
        let db = fresh_db().await;
        let bot_id = db
            .create_agent_bot(
                "feishu",
                "t",
                "app",
                "secret",
                Some("ou_owner".to_string()),
                None,
                1,
            )
            .await
            .unwrap();
        assert_eq!(
            db.get_owner_open_id(bot_id).await.unwrap(),
            Some("ou_owner".to_string()),
            "扫码创建应同步初始化 owner_open_id"
        );
        let wrote = db.set_owner_open_id_if_empty(bot_id, "ou_other").await.unwrap();
        assert!(!wrote, "已有值时不应覆盖");
        assert_eq!(
            db.get_owner_open_id(bot_id).await.unwrap(),
            Some("ou_owner".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_owner_open_id_none_for_missing_bot() {
        let db = fresh_db().await;
        assert_eq!(db.get_owner_open_id(9999).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_get_owner_open_id_treats_empty_string_as_unset() {
        // owner_open_id 为空串（脏数据）应与 NULL 同等视为未设置，返回 None
        let db = fresh_db().await;
        let bot_id = db
            .create_agent_bot("feishu", "t", "app", "secret", None, None, 1)
            .await
            .unwrap();
        // create_agent_bot 对非扫码 bot 写 NULL；手动置空串模拟脏数据
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        db.conn
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE agent_bots SET owner_open_id = '' WHERE id = ?",
                [bot_id.into()],
            ))
            .await
            .unwrap();
        assert_eq!(db.get_owner_open_id(bot_id).await.unwrap(), None);
    }
}
