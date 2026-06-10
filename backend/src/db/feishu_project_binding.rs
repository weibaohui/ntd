use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Statement};
use crate::db::Database;
use crate::db::entity::feishu_project_bindings;

/// A binding between a Feishu chat and a project directory.
#[derive(Debug, Clone)]
pub struct FeishuProjectBinding {
    pub id: i64,
    pub bot_id: i64,
    pub chat_id: String,
    pub chat_type: String,
    pub project_dir_id: i64,
    pub todo_id: i64,
    pub session_id: Option<String>,
    pub latest_record_id: Option<i64>,
    pub status: String,
    /// 是否激活（true=参与路由，false=禁用但保留记录）
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Database {
    /// 创建飞书聊天与项目目录的绑定记录
    ///
    /// # 业务场景
    /// 1. Web UI 创建待绑定记录（chat_id="__pending__"），等待飞书侧 /bind 补齐
    /// 2. 飞书 /bind 命令直接创建完整绑定
    ///
    /// # 初始状态
    /// - status = "idle"（等待首次执行）
    /// - session_id/latest_record_id = None（首次执行时由 update_feishu_project_binding_session 填充）
    pub async fn create_feishu_project_binding(
        &self,
        bot_id: i64,
        chat_id: &str,
        chat_type: &str,
        project_dir_id: i64,
        todo_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = feishu_project_bindings::ActiveModel {
            bot_id: ActiveValue::Set(bot_id),
            chat_id: ActiveValue::Set(chat_id.to_string()),
            chat_type: ActiveValue::Set(chat_type.to_string()),
            project_dir_id: ActiveValue::Set(project_dir_id),
            todo_id: ActiveValue::Set(todo_id),
            session_id: ActiveValue::Set(None),
            latest_record_id: ActiveValue::Set(None),
            status: ActiveValue::Set(crate::models::binding_status::IDLE.to_string()),
            enabled: ActiveValue::Set(true),
            created_at: ActiveValue::Set(now.clone()),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// Get binding by bot_id + chat_id.
    /// Read-only — does NOT correct stale status. Callers should use
    /// `cleanup_stale_running_bindings` periodically or inline when routing.
    pub async fn get_feishu_project_binding(
        &self,
        bot_id: i64,
        chat_id: &str,
    ) -> Result<Option<FeishuProjectBinding>, sea_orm::DbErr> {
        let model = feishu_project_bindings::Entity::find()
            .filter(feishu_project_bindings::Column::BotId.eq(bot_id))
            .filter(feishu_project_bindings::Column::ChatId.eq(chat_id))
            .one(&self.conn)
            .await?;
        Ok(model.map(Self::binding_from_model))
    }

    /// Get binding by primary key id.
    pub async fn get_feishu_project_binding_by_id(
        &self,
        id: i64,
    ) -> Result<Option<FeishuProjectBinding>, sea_orm::DbErr> {
        let model = feishu_project_bindings::Entity::find_by_id(id)
            .one(&self.conn)
            .await?;
        Ok(model.map(Self::binding_from_model))
    }

    /// Get all bindings for a given bot.
    pub async fn get_feishu_project_bindings(
        &self,
        bot_id: i64,
    ) -> Result<Vec<FeishuProjectBinding>, sea_orm::DbErr> {
        let models = feishu_project_bindings::Entity::find()
            .filter(feishu_project_bindings::Column::BotId.eq(bot_id))
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(Self::binding_from_model).collect())
    }

    /// Get all bindings across all bots.
    pub async fn get_all_feishu_project_bindings(
        &self,
    ) -> Result<Vec<FeishuProjectBinding>, sea_orm::DbErr> {
        let models = feishu_project_bindings::Entity::find()
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(Self::binding_from_model).collect())
    }

    /// Update session_id (when Some), latest_record_id and status after starting an execution.
    /// Pass session_id=None to leave existing session_id unchanged (for first executions
    /// where the real Claude Code session_id is discovered later from stdout, not bound yet).
    pub async fn update_feishu_project_binding_session(
        &self,
        id: i64,
        session_id: Option<&str>,
        latest_record_id: i64,
        status: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let mut am = feishu_project_bindings::ActiveModel {
            id: ActiveValue::Unchanged(id),
            latest_record_id: ActiveValue::Set(Some(latest_record_id)),
            status: ActiveValue::Set(status.to_string()),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };
        if let Some(sid) = session_id {
            am.session_id = ActiveValue::Set(Some(sid.to_string()));
        }
        self.exec_update(am).await
    }

    /// 清除 binding 的 session_id 和 latest_record_id。
    /// 用于 /new 命令：强制开启新 session，不再 resume 之前的会话。
    pub async fn clear_feishu_binding_session(
        &self,
        id: i64,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = feishu_project_bindings::ActiveModel {
            id: ActiveValue::Unchanged(id),
            session_id: ActiveValue::Set(None),
            latest_record_id: ActiveValue::Set(None),
            status: ActiveValue::Set(crate::models::binding_status::IDLE.to_string()),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 将 pending 绑定（chat_id="__pending__"）连接到真实聊天
    ///
    /// 流程：Web UI 预先创建了 binding（含 Todo + project_dir），用户在飞书发送
    /// /bind <名称> 时，此方法将 pending binding 的 chat_id 从 "__pending__" 更新为
    /// 真实 chat_id，从而激活绑定。
    ///
    /// 前提：调用前需确保该 chat 尚未绑定（或已由调用方清理旧绑定）。
    pub async fn attach_feishu_project_binding(
        &self,
        id: i64,
        chat_id: &str,
        chat_type: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = feishu_project_bindings::ActiveModel {
            id: ActiveValue::Unchanged(id),
            chat_id: ActiveValue::Set(chat_id.to_string()),
            chat_type: ActiveValue::Set(chat_type.to_string()),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };
        self.exec_update(am).await?;
        Ok(())
    }

    /// Update binding status (e.g. back to idle after execution ends).
    pub async fn update_feishu_project_binding_status(
        &self,
        id: i64,
        status: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = feishu_project_bindings::ActiveModel {
            id: ActiveValue::Unchanged(id),
            status: ActiveValue::Set(status.to_string()),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 启用/禁用绑定（仅修改 enabled 状态，不删除记录）
    pub async fn update_feishu_project_binding_enabled(
        &self,
        id: i64,
        enabled: bool,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = feishu_project_bindings::ActiveModel {
            id: ActiveValue::Unchanged(id),
            enabled: ActiveValue::Set(enabled),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// Delete a binding.
    pub async fn delete_feishu_project_binding(
        &self,
        id: i64,
    ) -> Result<(), sea_orm::DbErr> {
        feishu_project_bindings::Entity::delete_by_id(id)
            .exec(&self.conn)
            .await
            .map(|_| ())
    }

    /// Delete binding by bot_id + chat_id.
    pub async fn delete_feishu_project_binding_by_chat(
        &self,
        bot_id: i64,
        chat_id: &str,
    ) -> Result<(), sea_orm::DbErr> {
        if let Some(binding) = self.get_feishu_project_binding(bot_id, chat_id).await? {
            self.delete_feishu_project_binding(binding.id).await?;
        }
        Ok(())
    }

    /// Atomically reset bindings whose latest_record is no longer running.
    /// Uses a single UPDATE … WHERE … IN (SELECT …) to avoid race between read+write.
    /// Call from a periodic task or inline when routing.
    pub async fn cleanup_stale_running_bindings(&self) -> Result<u64, sea_orm::DbErr> {
        let backend = self.conn.get_database_backend();
        let idle = crate::models::binding_status::IDLE;
        let running = crate::models::binding_status::RUNNING;
        let success = crate::models::ExecutionStatus::Success.as_str();
        let failed = crate::models::ExecutionStatus::Failed.as_str();
        let sql = format!(
            "UPDATE feishu_project_bindings \
            SET status = '{}', updated_at = ? \
            WHERE status = '{}' \
            AND latest_record_id IN (SELECT id FROM execution_records WHERE status IN ('{}', '{}'))",
            idle, running, success, failed
        );
        let now = crate::models::utc_timestamp();
        let res = self
            .conn
            .execute(Statement::from_sql_and_values(backend, sql, [now.into()]))
            .await?;
        Ok(res.rows_affected())
    }

    // --- helpers ---

    fn binding_from_model(m: feishu_project_bindings::Model) -> FeishuProjectBinding {
        FeishuProjectBinding {
            id: m.id,
            bot_id: m.bot_id,
            chat_id: m.chat_id,
            chat_type: m.chat_type,
            project_dir_id: m.project_dir_id,
            todo_id: m.todo_id,
            session_id: m.session_id,
            latest_record_id: m.latest_record_id,
            status: m.status,
            enabled: m.enabled,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
