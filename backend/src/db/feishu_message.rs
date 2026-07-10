use std::collections::HashMap;

use crate::db::entity::feishu_messages;
use crate::db::Database;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Statement,
};

#[derive(Debug, Clone)]
pub struct FeishuMessageRecord {
    pub id: i64,
    pub bot_id: i64,
    pub message_id: String,
    pub chat_id: String,
    pub chat_type: String,
    pub sender_open_id: String,
    pub sender_nickname: Option<String>,
    pub sender_type: Option<String>,
    pub content: Option<String>,
    pub msg_type: String,
    pub is_mention: bool,
    pub processed: bool,
    pub execution_record_id: Option<i64>,
    pub is_history: bool,
    pub fetch_time: Option<String>,
    pub created_at: Option<String>,
    pub workspace_id: Option<i64>,
    pub processed_type: Option<String>,
    pub processed_id: Option<i64>,
    pub error: Option<String>,
}

pub struct NewFeishuMessage<'a> {
    pub bot_id: i64,
    pub message_id: &'a str,
    pub chat_id: &'a str,
    pub chat_type: &'a str,
    pub sender_open_id: &'a str,
    pub sender_type: Option<&'a str>,
    pub content: Option<&'a str>,
    pub msg_type: &'a str,
    pub is_mention: bool,
    /// 消息接收时，智能体所属的工作空间 ID
    pub workspace_id: Option<i64>,
}

pub struct NewFeishuHistoryMessage<'a> {
    pub bot_id: i64,
    pub message_id: &'a str,
    pub chat_id: &'a str,
    pub chat_type: &'a str,
    pub sender_open_id: &'a str,
    pub sender_nickname: Option<&'a str>,
    pub sender_type: Option<&'a str>,
    pub content: Option<&'a str>,
    pub msg_type: &'a str,
    pub created_at: &'a str,
    /// 消息接收时，智能体所属的工作空间 ID
    pub workspace_id: Option<i64>,
}

impl Database {
    pub async fn save_feishu_message(
        &self,
        message: NewFeishuMessage<'_>,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = feishu_messages::ActiveModel {
            bot_id: ActiveValue::Set(message.bot_id),
            message_id: ActiveValue::Set(message.message_id.to_string()),
            chat_id: ActiveValue::Set(message.chat_id.to_string()),
            chat_type: ActiveValue::Set(message.chat_type.to_string()),
            sender_open_id: ActiveValue::Set(message.sender_open_id.to_string()),
            sender_nickname: ActiveValue::Set(None),
            sender_type: ActiveValue::Set(message.sender_type.map(String::from)),
            content: ActiveValue::Set(message.content.map(String::from)),
            msg_type: ActiveValue::Set(message.msg_type.to_string()),
            is_mention: ActiveValue::Set(Some(message.is_mention)),
            processed: ActiveValue::Set(Some(false)),
            is_history: ActiveValue::Set(Some(false)),
            fetch_time: ActiveValue::Set(None),
            created_at: ActiveValue::Set(Some(now)),
            workspace_id: ActiveValue::Set(message.workspace_id),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    pub async fn save_feishu_history_message(
        &self,
        message: NewFeishuHistoryMessage<'_>,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = feishu_messages::ActiveModel {
            bot_id: ActiveValue::Set(message.bot_id),
            message_id: ActiveValue::Set(message.message_id.to_string()),
            chat_id: ActiveValue::Set(message.chat_id.to_string()),
            chat_type: ActiveValue::Set(message.chat_type.to_string()),
            sender_open_id: ActiveValue::Set(message.sender_open_id.to_string()),
            sender_nickname: ActiveValue::Set(message.sender_nickname.map(String::from)),
            sender_type: ActiveValue::Set(message.sender_type.map(String::from)),
            content: ActiveValue::Set(message.content.map(String::from)),
            msg_type: ActiveValue::Set(message.msg_type.to_string()),
            is_mention: ActiveValue::Set(Some(false)),
            processed: ActiveValue::Set(Some(false)),
            is_history: ActiveValue::Set(Some(true)),
            fetch_time: ActiveValue::Set(Some(now)),
            created_at: ActiveValue::Set(Some(message.created_at.to_string())),
            workspace_id: ActiveValue::Set(message.workspace_id),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    pub async fn get_feishu_messages(
        &self,
        bot_id: i64,
        limit: u64,
    ) -> Result<Vec<FeishuMessageRecord>, sea_orm::DbErr> {
        let models = feishu_messages::Entity::find()
            .order_by_desc(feishu_messages::Column::Id)
            .all(&self.conn)
            .await?;

        Ok(models
            .into_iter()
            .filter(|m| m.bot_id == bot_id)
            .take(limit as usize)
            .map(|m| FeishuMessageRecord {
                id: m.id,
                bot_id: m.bot_id,
                message_id: m.message_id,
                chat_id: m.chat_id,
                chat_type: m.chat_type,
                sender_open_id: m.sender_open_id,
                sender_nickname: m.sender_nickname,
                sender_type: m.sender_type,
                content: m.content,
                msg_type: m.msg_type,
                is_mention: m.is_mention.unwrap_or(false),
                processed: m.processed.unwrap_or(false),
                execution_record_id: m.execution_record_id,
                is_history: m.is_history.unwrap_or(false),
                fetch_time: m.fetch_time,
                created_at: m.created_at,
                workspace_id: m.workspace_id,
                processed_type: m.processed_type,
                processed_id: m.processed_id,
                error: m.error,
            })
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn get_feishu_history_messages(
        &self,
        chat_id: Option<&str>,
        sender_open_id: Option<&str>,
        is_history: Option<bool>,
        workspace_id: Option<i64>,
        bot_id: Option<i64>,
        page: u64,
        page_size: u64,
    ) -> Result<(Vec<FeishuMessageRecord>, i64), sea_orm::DbErr> {
        let mut query =
            feishu_messages::Entity::find().order_by_desc(feishu_messages::Column::CreatedAt);

        if let Some(sid) = sender_open_id {
            query = query.filter(feishu_messages::Column::SenderOpenId.eq(sid.to_string()));
        }

        if let Some(history) = is_history {
            query = query.filter(feishu_messages::Column::IsHistory.eq(Some(history)));
        }

        if let Some(cid) = chat_id {
            query = query.filter(feishu_messages::Column::ChatId.eq(cid.to_string()));
        }

        // 按工作空间筛选：只返回该工作空间下的消息
        if let Some(wid) = workspace_id {
            query = query.filter(feishu_messages::Column::WorkspaceId.eq(Some(wid)));
        }

        // 按智能体筛选：只返回该智能体的消息
        if let Some(bid) = bot_id {
            query = query.filter(feishu_messages::Column::BotId.eq(bid));
        }

        let total = query.clone().count(&self.conn).await? as i64;

        let offset = (page - 1) * page_size;
        let models = query
            .offset(offset)
            .limit(page_size)
            .all(&self.conn)
            .await?;

        let records = models
            .into_iter()
            .map(|m| FeishuMessageRecord {
                id: m.id,
                bot_id: m.bot_id,
                message_id: m.message_id,
                chat_id: m.chat_id,
                chat_type: m.chat_type,
                sender_open_id: m.sender_open_id,
                sender_nickname: m.sender_nickname,
                sender_type: m.sender_type,
                content: m.content,
                msg_type: m.msg_type,
                is_mention: m.is_mention.unwrap_or(false),
                processed: m.processed.unwrap_or(false),
                execution_record_id: m.execution_record_id,
                is_history: m.is_history.unwrap_or(false),
                fetch_time: m.fetch_time,
                created_at: m.created_at,
                workspace_id: m.workspace_id,
                processed_type: m.processed_type,
                processed_id: m.processed_id,
                error: m.error,
            })
            .collect();

        Ok((records, total))
    }

    pub async fn feishu_message_exists(&self, message_id: &str) -> Result<bool, sea_orm::DbErr> {
        let result = feishu_messages::Entity::find()
            .filter(feishu_messages::Column::MessageId.eq(message_id))
            .one(&self.conn)
            .await?;
        Ok(result.is_some())
    }

    pub async fn get_distinct_senders(
        &self,
    ) -> Result<Vec<(String, Option<String>, Option<String>, i64)>, sea_orm::DbErr> {
        // Returns distinct sender_open_ids with their message count, sender_type, and sender_nickname
        let models = feishu_messages::Entity::find()
            .order_by_desc(feishu_messages::Column::CreatedAt)
            .all(&self.conn)
            .await?;

        let mut sender_map: HashMap<String, (Option<String>, Option<String>, i64)> = HashMap::new();
        for model in models {
            let entry = sender_map.entry(model.sender_open_id.clone()).or_insert((
                model.sender_type.clone(),
                model.sender_nickname.clone(),
                0,
            ));
            // Fill non-null nickname and sender_type only when missing
            if entry.1.is_none() && model.sender_nickname.is_some() {
                entry.1 = model.sender_nickname.clone();
            }
            if entry.0.is_none() && model.sender_type.is_some() {
                entry.0 = model.sender_type.clone();
            }
            entry.2 += 1;
        }

        let result: Vec<(String, Option<String>, Option<String>, i64)> = sender_map
            .into_iter()
            .map(|(sender_open_id, (sender_type, sender_nickname, count))| {
                (sender_open_id, sender_type, sender_nickname, count)
            })
            .collect();

        Ok(result)
    }

    /// Get the latest message create_time for a specific chat (for incremental fetching)
    pub async fn get_latest_history_message_time(
        &self,
        bot_id: i64,
        chat_id: &str,
    ) -> Result<Option<String>, sea_orm::DbErr> {
        let result = feishu_messages::Entity::find()
            .filter(feishu_messages::Column::BotId.eq(bot_id))
            .filter(feishu_messages::Column::ChatId.eq(chat_id.to_string()))
            .filter(feishu_messages::Column::IsHistory.eq(Some(true)))
            .order_by_desc(feishu_messages::Column::CreatedAt)
            .one(&self.conn)
            .await?;
        Ok(result.and_then(|m| m.created_at))
    }

    /// Mark a message as processed with the triggered todo_id and execution_record_id
    pub async fn mark_feishu_message_processed(
        &self,
        message_id: &str,
        todo_id: i64,
        execution_record_id: Option<i64>,
        processed_type: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let result = feishu_messages::Entity::find()
            .filter(feishu_messages::Column::MessageId.eq(message_id))
            .one(&self.conn)
            .await?;

        if let Some(model) = result {
            let mut am: feishu_messages::ActiveModel = model.into();
            am.processed = ActiveValue::Set(Some(true));
            am.execution_record_id = ActiveValue::Set(execution_record_id);
            // processed_id 存 execution_record_id（有值）或 todo_id（回退）
            am.processed_id = ActiveValue::Set(execution_record_id.or(Some(todo_id)));
            am.processed_type = ActiveValue::Set(processed_type.map(|s| s.to_string()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// Mark a message as failed (processed=false) when execution fails.
    pub async fn mark_feishu_message_failed(
        &self,
        message_id: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let result = feishu_messages::Entity::find()
            .filter(feishu_messages::Column::MessageId.eq(message_id))
            .one(&self.conn)
            .await?;

        if let Some(model) = result {
            let mut am: feishu_messages::ActiveModel = model.into();
            am.processed = ActiveValue::Set(Some(false));
            am.processed_id = ActiveValue::Set(None);
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// Mark a message as processed with error (e.g., loop_paused).
    /// Message is considered processed but with an error condition.
    pub async fn mark_feishu_message_processed_with_error(
        &self,
        message_id: &str,
        todo_id: i64,
        processed_type: Option<&str>,
        error: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let result = feishu_messages::Entity::find()
            .filter(feishu_messages::Column::MessageId.eq(message_id))
            .one(&self.conn)
            .await?;

        if let Some(model) = result {
            let mut am: feishu_messages::ActiveModel = model.into();
            am.processed = ActiveValue::Set(Some(true));
            am.processed_id = ActiveValue::Set(Some(todo_id));
            am.processed_type = ActiveValue::Set(processed_type.map(|s| s.to_string()));
            am.error = ActiveValue::Set(Some(error.to_string()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn get_feishu_message_stats(&self, hours: Option<u32>, workspace_id: Option<i64>) -> Result<crate::models::FeishuMessageStats, sea_orm::DbErr> {
        let backend = self.conn.get_database_backend();
        let hours = hours.unwrap_or(720); // default 30 days = 720 hours (matches frontend)
        let time_filter = format!("datetime('now', '-{} hours')", hours);

        // 构建基础 SQL 和可选的 workspace_id 过滤条件
        let workspace_filter = if let Some(wid) = workspace_id {
            format!(" AND workspace_id = {}", wid)
        } else {
            String::new()
        };

        let stats_sql = format!(
            "SELECT \
            COUNT(*) as total, \
            COALESCE(SUM(CASE WHEN processed = 1 OR processed = 'true' THEN 1 ELSE 0 END), 0) as processed, \
            COALESCE(SUM(CASE WHEN processed IS NULL OR processed = 0 OR processed = 'false' THEN 1 ELSE 0 END), 0) as unprocessed, \
            COALESCE(SUM(CASE WHEN processed_id IS NOT NULL THEN 1 ELSE 0 END), 0) as triggered_todos, \
            COUNT(DISTINCT sender_open_id) as unique_senders, \
            COUNT(DISTINCT chat_id) as unique_chats \
            FROM feishu_messages \
            WHERE datetime(created_at) >= {}{}", time_filter, workspace_filter);

        let mut stats = if let Some(row) = self.conn.query_one(Statement::from_string(backend, stats_sql.to_string())).await? {
            crate::models::FeishuMessageStats {
                total_messages: row.try_get_by("total").unwrap_or(0),
                processed: row.try_get_by("processed").unwrap_or(0),
                unprocessed: row.try_get_by("unprocessed").unwrap_or(0),
                triggered_todos: row.try_get_by("triggered_todos").unwrap_or(0),
                unique_senders: row.try_get_by("unique_senders").unwrap_or(0),
                unique_chats: row.try_get_by("unique_chats").unwrap_or(0),
                last_24h_messages: 0,
            }
        } else {
            return Ok(crate::models::FeishuMessageStats {
                total_messages: 0,
                processed: 0,
                unprocessed: 0,
                triggered_todos: 0,
                unique_senders: 0,
                last_24h_messages: 0,
                unique_chats: 0,
            });
        };

        // Last 24h count - 同样需要按 workspace 过滤
        let recent_sql = format!(
            "SELECT COUNT(*) as cnt FROM feishu_messages WHERE \
            created_at IS NOT NULL AND datetime(created_at) >= datetime('now', '-1 day'){}", workspace_filter);
        if let Some(row) = self.conn.query_one(Statement::from_string(backend, recent_sql.to_string())).await? {
            stats.last_24h_messages = row.try_get_by("cnt").unwrap_or(0);
        }

        Ok(stats)
    }
}
