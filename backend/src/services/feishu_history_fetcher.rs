use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use std::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::service_context::ServiceContext;
use crate::config::Config as AppConfig;
use crate::db::NewFeishuHistoryMessage;
use crate::feishu::sdk::config::{Config as FeishuSdkConfig, CONTENT_TYPE_JSON};
use crate::feishu::sdk::token_manager::TokenManager;
use crate::models::build_trigger_params;
use crate::services::message_debounce::{MessageDebounce, PendingMessage};

const IM_V1_LIST_MESSAGES: &str = "/open-apis/im/v1/messages";

pub struct FeishuHistoryFetcher {
    ctx: ServiceContext,
    token_manager: Arc<TokenManager>,
    bot_credentials: Arc<DashMap<i64, (String, String, String)>>,
    debounce: Arc<MessageDebounce>,
}

#[derive(Debug, Deserialize)]
struct ListMessagesResponse {
    code: i32,
    msg: String,
    data: Option<ListMessagesData>,
}

#[derive(Debug, Deserialize)]
struct ListMessagesData {
    has_more: bool,
    page_token: Option<String>,
    items: Option<Vec<MessageItem>>,
}

#[derive(Debug, Deserialize)]
struct MessageItem {
    message_id: String,
    msg_type: String,
    chat_id: String,
    #[allow(dead_code)]
    chat_type: Option<String>,
    sender: Option<Sender>,
    body: Option<MessageBody>,
    create_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Sender {
    id: Option<String>,
    #[allow(dead_code)]
    id_type: Option<String>,
    #[allow(dead_code)]
    sender_type: Option<String>,
    #[allow(dead_code)]
    tenant_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageBody {
    content: Option<String>,
}

#[derive(Clone, Debug)]
struct ChatToFetch {
    bot_id: i64,
    chat_id: String,
}

impl FeishuHistoryFetcher {
    pub fn new(
        ctx: ServiceContext,
        token_manager: Arc<TokenManager>,
        bot_credentials: Arc<DashMap<i64, (String, String, String)>>,
        debounce: Arc<MessageDebounce>,
    ) -> Self {
        Self {
            ctx,
            token_manager,
            bot_credentials,
            debounce,
        }
    }

    pub fn start(self: Arc<Self>, bots: Vec<(i64, String, String)>) {
        if bots.is_empty() {
            info!("[feishu-history-fetcher] no bots configured, skipping");
            return;
        }

        tokio::spawn(async move {
            info!("[feishu-history-fetcher] started");
            let mut ticker = interval(Duration::from_secs(60));

            loop {
                ticker.tick().await;

                // Collect all chats to fetch from both sources
                let mut chats_to_fetch: Vec<ChatToFetch> = Vec::new();

                // 1. Get chats from feishu_history_chats table
                for (bot_id, _, _) in &bots {
                    if let Ok(history_chats) = self.ctx.db.get_enabled_feishu_history_chats(*bot_id).await {
                        for chat in history_chats {
                            chats_to_fetch.push(ChatToFetch {
                                bot_id: *bot_id,
                                chat_id: chat.chat_id,
                            });
                        }
                    }
                }

                // 2. Get chats from feishu_push_targets table (group chat only)
                if let Ok(push_targets) = self.ctx.db.get_group_chat_ids().await {
                    for (bot_id, chat_id) in push_targets {
                        // Avoid duplicates
                        if !chats_to_fetch
                            .iter()
                            .any(|c| c.bot_id == bot_id && c.chat_id == chat_id)
                        {
                            chats_to_fetch.push(ChatToFetch { bot_id, chat_id });
                        }
                    }
                }

                if chats_to_fetch.is_empty() {
                    debug!("[feishu-history-fetcher] no chats to fetch");
                    continue;
                }

                for (bot_id, app_id, app_secret) in &bots {
                    // Filter chats for this bot
                    let bot_chats: Vec<_> = chats_to_fetch
                        .iter()
                        .filter(|c| c.bot_id == *bot_id)
                        .cloned()
                        .collect();

                    if bot_chats.is_empty() {
                        continue;
                    }

                    if let Err(e) = self
                        .fetch_for_bot(*bot_id, app_id, app_secret, &bot_chats)
                        .await
                    {
                        warn!(
                            "[feishu-history-fetcher] error fetching for bot {}: {}",
                            bot_id, e
                        );
                    }
                }
            }
        });
    }

    async fn fetch_for_bot(
        &self,
        bot_id: i64,
        app_id: &str,
        app_secret: &str,
        chats: &[ChatToFetch],
    ) -> Result<(), String> {
        if chats.is_empty() {
            debug!("[feishu-history-fetcher] no chats for bot {}", bot_id);
            return Ok(());
        }

        let feishu_config = FeishuSdkConfig::builder()
            .app_id(app_id)
            .app_secret(app_secret)
            .build();

        let token = self
            .token_manager
            .get_tenant_access_token(&feishu_config)
            .await
            .map_err(|e| format!("failed to get token: {}", e))?;

        for chat in chats {
            match self.fetch_chat_history(bot_id, &chat.chat_id, &token).await {
                Ok(count) => {
                    if count > 0 {
                        info!(
                            "[feishu-history-fetcher] fetched {} new messages from chat {}",
                            count, chat.chat_id
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "[feishu-history-fetcher] error fetching chat {}: {}",
                        chat.chat_id, e
                    );
                }
            }
        }

        Ok(())
    }

    async fn fetch_chat_history(
        &self,
        bot_id: i64,
        chat_id: &str,
        token: &str,
    ) -> Result<usize, String> {
        // Get the latest message time from DB for incremental fetching
        let start_time = match self.ctx.db.get_latest_history_message_time(bot_id, chat_id).await {
            Ok(Some(time)) => {
                // Parse the time and convert to Unix timestamp in seconds (Feishu API expects seconds)
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&time) {
                    Some(dt.timestamp().to_string())
                } else {
                    None
                }
            }
            _ => None,
        };

        // 获取 bot 所属的 workspace_id，保存到消息记录中
        let workspace_id = self.ctx.db.get_agent_bot_workspace_id(bot_id).await.unwrap_or(None);

        let mut total_fetched = 0;
        let mut page_token: Option<String> = None;
        let mut has_more = true;

        while has_more {
            let mut query_params: HashMap<String, String> = HashMap::new();
            query_params.insert("container_id_type".to_string(), "chat".to_string());
            query_params.insert("container_id".to_string(), chat_id.to_string());
            query_params.insert("sort_type".to_string(), "ByCreateTimeAsc".to_string());
            query_params.insert("page_size".to_string(), "50".to_string());

            // Use start_time for incremental fetching (messages created after this time)
            if let Some(ref st) = start_time {
                query_params.insert("start_time".to_string(), st.clone());
            }

            if let Some(ref pt) = page_token {
                query_params.insert("page_token".to_string(), pt.clone());
            }

            let resp = Self::list_messages(&reqwest::Client::new(), token, &query_params).await?;

            if resp.code != 0 {
                return Err(format!("API error: {} ({})", resp.msg, resp.code));
            }

            let data = resp.data.ok_or("no data in response")?;
            has_more = data.has_more;
            page_token = data.page_token;

            if let Some(items) = data.items {
                for item in items {
                    if self.ctx.db
                        .feishu_message_exists(&item.message_id)
                        .await
                        .map_err(|e| format!("db error: {}", e))?
                    {
                        // Message already exists, skip it but continue fetching
                        // (API returns ascending order, so there may be newer messages)
                        continue;
                    }

                    // Extract sender info from API response
                    let sender_id = item
                        .sender
                        .as_ref()
                        .and_then(|s| s.id.clone())
                        .unwrap_or_default();
                    let sender_type = item.sender.as_ref().and_then(|s| s.sender_type.clone());

                    // sender_open_id is actually the sender's ID (open_id for users, app_id for bots)
                    let sender_open_id = sender_id.as_str();

                    // Skip if sender is OUR registered bot (using OR logic)
                    // This prevents circular triggering: bot sends message -> history fetcher picks it up -> bot processes it again
                    let is_our_bot = Self::is_our_bot_message(
                        &sender_id,
                        sender_type.as_deref(),
                        &self.bot_credentials,
                        &self.token_manager,
                        bot_id,
                    )
                    .await;

                    if is_our_bot {
                        tracing::debug!(
                            "[feishu-history-fetcher] skip our own bot message from sender_id={}, chat {}",
                            sender_id, chat_id
                        );
                        continue;
                    }

                    let sender_nickname: Option<&str> = None;

                    let content = item.body.as_ref().and_then(|b| b.content.clone());

                    // create_time from API is in milliseconds, convert to seconds
                    let created_at = item
                        .create_time
                        .and_then(|t| t.parse::<i64>().ok().map(|ms| ms / 1000))
                        .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
                        .unwrap_or_else(crate::models::utc_timestamp);

                    if let Err(e) = self.ctx.db
                        .save_feishu_history_message(NewFeishuHistoryMessage {
                            bot_id,
                            message_id: &item.message_id,
                            chat_id: &item.chat_id,
                            chat_type: item.chat_type.as_deref().unwrap_or(""),
                            sender_open_id,
                            sender_nickname,
                            sender_type: sender_type.as_deref(),
                            content: content.as_deref(),
                            msg_type: &item.msg_type,
                            created_at: &created_at,
                            workspace_id,
                        })
                        .await
                    {
                        warn!(
                            "[feishu-history-fetcher] failed to save message {}: {}",
                            item.message_id, e
                        );
                    } else {
                        total_fetched += 1;

                        // Check message age: skip processing if too old
                        let max_age_secs = {
                            // config.read() 可能因线程 panic 导致 PoisonError；回退到获取内部数据而非 panic
                            let cfg = match self.ctx.config.read() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            cfg.history_message_max_age_secs
                        };
                        let msg_time = chrono::DateTime::parse_from_rfc3339(&created_at)
                            .map(|dt| dt.with_timezone(&chrono::Utc).timestamp())
                            .unwrap_or(0);
                        let now_secs = chrono::Utc::now().timestamp();
                        let age_secs = now_secs.saturating_sub(msg_time);

                        if age_secs > max_age_secs as i64 {
                            debug!(
                                "[feishu-history-fetcher] skip old message {} (age={}s > max={}s)",
                                item.message_id, age_secs, max_age_secs
                            );
                            continue;
                        }

                        // Process the message through debounce pipeline
                        if let Some(ref msg_content) = content {
                            // Check group whitelist
                            let in_whitelist = match self.ctx.db
                                .is_sender_in_whitelist(bot_id, sender_open_id)
                                .await
                            {
                                Ok(allowed) => allowed,
                                Err(e) => {
                                    tracing::warn!("[feishu-history-fetcher] whitelist check failed for sender {}, denying: {}", sender_open_id, e);
                                    false
                                }
                            };
                            if !in_whitelist {
                                tracing::debug!(
                                    "[feishu-history-fetcher] sender {} not in group whitelist, skipping",
                                    sender_open_id
                                );
                                continue;
                            }

                            // Push to debounce buffer instead of executing directly
                            if let Some(todo_id) = Self::resolve_todo_id(&self.ctx.config, msg_content).await
                            {
                                let (trigger_type, params) =
                                    build_trigger_params(msg_content);
                                let todo_prompt = match self.ctx.db.get_todo(todo_id).await {
                                    Ok(Some(t)) => Some(t.prompt.clone()),
                                    Ok(None) => None,
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to fetch todo {} for feishu history: {}",
                                            todo_id,
                                            e
                                        );
                                        None
                                    }
                                }
                                .unwrap_or_default();
                                self.debounce.push(PendingMessage {
                                    bot_id,
                                    chat_id: chat_id.to_string(),
                                    chat_type: "group".to_string(),
                                    sender: sender_open_id.to_string(),
                                    content: msg_content.clone(),
                                    todo_id,
                                    todo_prompt,
                                    executor: None,
                                    trigger_type,
                                    params: Some(params),
                                    message_id: Some(item.message_id.clone()),
                                    resume_session_id: None,
                                    resume_message: None,
                                    binding_id: None,
                                    workspace_id: None,
                                });
                                // Debounce timer will mark message as processed with execution_record_id
                            }
                        }
                    }
                }
            }
        }

        Ok(total_fetched)
    }

    /// Resolve the target todo_id for a message: slash command match or default response.
    /// TODO: 需要改为从 workspace 设置查询
    async fn resolve_todo_id(_config: &Arc<RwLock<AppConfig>>, _content: &str) -> Option<i64> {
        // 当前实现已移除，后续需要通过 workspace 设置查询
        None
    }

    #[allow(dead_code)]
    /// 发送文本消息
    async fn send_text(
        bot_credentials: &Arc<DashMap<i64, (String, String, String)>>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        receive_id: &str,
        receive_id_type: &str,
        text: &str,
    ) {
        let base_url = Self::base_url(bot_credentials, bot_id);
        let Some(base_url) = base_url else { return };
        let token = match Self::get_tenant_token(bot_credentials, token_manager, bot_id).await {
            Some(t) => t,
            None => return,
        };

        let client = reqwest::Client::new();
        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type={}",
            base_url, receive_id_type
        );
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": serde_json::to_string(&serde_json::json!({ "text": text })).unwrap_or_default()
        });

        match client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(res) => {
                let status = res.status();
                if !status.is_success() {
                    tracing::error!("[feishu-history] send_text failed: status={}", status);
                }
            }
            Err(e) => {
                tracing::error!("[feishu-history] send_text request failed: {}", e);
            }
        }
    }

    fn base_url(
        bot_credentials: &Arc<DashMap<i64, (String, String, String)>>,
        bot_id: i64,
    ) -> Option<String> {
        let domain = bot_credentials.get(&bot_id)?.2.clone();
        Some(if domain == "lark" {
            "https://open.larksuite.com".to_string()
        } else {
            "https://open.feishu.cn".to_string()
        })
    }

    async fn get_tenant_token(
        bot_credentials: &Arc<DashMap<i64, (String, String, String)>>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
    ) -> Option<String> {
        let sdk_config = Self::build_sdk_config(bot_credentials, bot_id)?;
        match token_manager.get_tenant_access_token(&sdk_config).await {
            Ok(token) => Some(token),
            Err(err) => {
                tracing::warn!("[feishu-history] 获取 tenant_access_token 失败: {}", err);
                None
            }
        }
    }

    /// Resolve the bot's own open_id from the Feishu API.
    /// Used to filter out self-sent messages and prevent circular triggering.
    async fn resolve_bot_open_id(
        bot_credentials: &Arc<DashMap<i64, (String, String, String)>>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
    ) -> Option<String> {
        let token = Self::get_tenant_token(bot_credentials, token_manager, bot_id).await?;
        let base_url = Self::base_url(bot_credentials, bot_id)?;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/open-apis/bot/v3/info"))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .ok()?;

        let body: serde_json::Value = res.json().await.ok()?;
        body.get("bot")
            .and_then(|b| b.get("open_id"))
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    /// Check if a message was sent by our own bot using OR logic.
    /// Matches against: app_id OR open_id of the registered bot.
    /// Returns true if sender matches ANY of our bot's identifiers.
    async fn is_our_bot_message(
        sender_id: &str,
        sender_type: Option<&str>,
        bot_credentials: &Arc<DashMap<i64, (String, String, String)>>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
    ) -> bool {
        if sender_id.is_empty() {
            return false;
        }

        // Check 1: If sender_type is "app", sender_id is the app_id - check if it matches our app_id
        if sender_type == Some("app") {
            if let Some(ref_val) = bot_credentials.get(&bot_id) {
                if sender_id == ref_val.0 {
                    tracing::debug!(
                        "[feishu-history-fetcher] matched our bot by app_id: {}",
                        sender_id
                    );
                    return true;
                }
            }
        }

        // Check 2: Resolve bot's open_id and compare
        if sender_type != Some("user") {
            let bot_open_id =
                Self::resolve_bot_open_id(bot_credentials, token_manager, bot_id).await;
            if let Some(open_id) = bot_open_id {
                if sender_id == open_id {
                    tracing::debug!(
                        "[feishu-history-fetcher] matched our bot by open_id: {}",
                        sender_id
                    );
                    return true;
                }
            }
        }

        false
    }

    fn build_sdk_config(
        bot_credentials: &Arc<DashMap<i64, (String, String, String)>>,
        bot_id: i64,
    ) -> Option<FeishuSdkConfig> {
        let ref_val = bot_credentials.get(&bot_id)?;
        let (app_id, app_secret, domain) =
            (ref_val.0.clone(), ref_val.1.clone(), ref_val.2.clone());
        let base_url = if domain == "lark" {
            "https://open.larksuite.com"
        } else {
            "https://open.feishu.cn"
        };

        Some(
            FeishuSdkConfig::builder()
                .app_id(app_id)
                .app_secret(app_secret)
                .base_url(base_url)
                .enable_token_cache(true)
                .http_client(reqwest::Client::new())
                .build(),
        )
    }

    async fn list_messages(
        client: &reqwest::Client,
        token: &str,
        query_params: &HashMap<String, String>,
    ) -> Result<ListMessagesResponse, String> {
        let url = format!("https://open.feishu.cn{}", IM_V1_LIST_MESSAGES);

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE_JSON));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|e| format!("invalid auth header: {}", e))?,
        );

        let mut builder = client.get(&url).headers(headers);
        for (key, value) in query_params {
            builder = builder.query(&[(key.as_str(), value.as_str())]);
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| format!("request failed: {}", e))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| format!("failed to read body: {}", e))?;

        debug!(
            "[feishu-history-fetcher] API response status: {}, body (first 500): {}",
            status,
            &body[..body.len().min(500)]
        );

        let result: ListMessagesResponse =
            serde_json::from_str(&body).map_err(|e| format!("json parse failed: {}", e))?;

        Ok(result)
    }
}
