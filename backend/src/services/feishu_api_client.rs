//! 飞书 HTTP 发送 + 凭证/token + reaction 族（096-W3-PR4：从 feishu_listener.rs 拆分，函数体逐字搬迁零改动）。
//!
//! 设计依据：docs/design/100-FeishuListener拆分-设计.md（拆分边界与字段归属）。
//! 本族函数均为无 self 关联函数（接 context 参数对象或显式传参），
//! unit struct 仅作命名空间承载，不持有状态。

use dashmap::DashMap;
use std::sync::{Arc, OnceLock};

use crate::feishu::sdk::config::{build_client_with_timeout, Config as FeishuSdkConfig, DEFAULT_REQ_TIMEOUT};
use crate::feishu::sdk::token_manager::TokenManager;

/// 全模块共享的带超时 HTTP Client（连接池复用）。
///
/// 106 体检：此前每个函数每次调用 `reqwest::Client::new()`——无超时（TCP 半开
/// 挂死串行推送循环）、无连接复用（繁忙时打爆临时端口/触发飞书限流）。
/// 15s 超时与 SDK 默认（DEFAULT_REQ_TIMEOUT）保持同一策略。
fn shared_http_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| build_client_with_timeout(DEFAULT_REQ_TIMEOUT))
        .clone()
}

/// 见模块头注释。unit struct：仅作命名空间（函数全部关联函数形态）。
pub(crate) struct FeishuApiClient;

impl FeishuApiClient {
    /// Patch an existing interactive card message with new content.
    pub(crate) async fn patch_card(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        message_id: &str,
        card_json: &str,
    ) -> anyhow::Result<()> {
        let base_url = FeishuApiClient::base_url(credentials, bot_id)
            .ok_or_else(|| anyhow::anyhow!("no base_url for bot {}", bot_id))?;
        let token = FeishuApiClient::get_tenant_token(credentials, token_manager, bot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no token for bot {}", bot_id))?;

        let client = shared_http_client();
        let url = format!(
            "{}/open-apis/im/v1/messages/{}",
            base_url, message_id
        );

        let body = serde_json::json!({
            "msg_type": "interactive",
            "content": card_json
        });

        let res = client
            .patch(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("patch_card request failed: {}", e))?;

        let status = res.status();
        if !status.is_success() {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            return Err(anyhow::anyhow!("patch_card failed: {} {:?}", status, body));
        }

        tracing::debug!("[feishu:{}] patch_card ok for message {}", bot_id, message_id);
        Ok(())
    }

    /// Send a plain text message to a Feishu recipient.
    pub(crate) async fn send_text(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        receive_id: &str,
        receive_id_type: &str,
        text: &str,
    ) {
        let base_url = match FeishuApiClient::base_url(credentials, bot_id) {
            Some(u) => u,
            None => return,
        };
        let token = match FeishuApiClient::get_tenant_token(credentials, token_manager, bot_id).await {
            Some(t) => t,
            None => return,
        };

        let client = shared_http_client();
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
                    tracing::error!("[feishu:{}] send_text failed: status={}", bot_id, status);
                } else {
                    tracing::debug!(
                        "[feishu:{}] send_text ok to {} ({})",
                        bot_id,
                        receive_id,
                        receive_id_type
                    );
                }
            }
            Err(e) => {
                tracing::error!("[feishu:{}] send_text request failed: {e}", bot_id);
            }
        }
    }

    /// Send an interactive card message to a Feishu recipient.
    #[allow(dead_code)]
    pub(crate) async fn send_card(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        receive_id: &str,
        receive_id_type: &str,
        card_json: &str,
    ) -> anyhow::Result<()> {
        let base_url = FeishuApiClient::base_url(credentials, bot_id)
            .ok_or_else(|| anyhow::anyhow!("no base_url for bot {}", bot_id))?;
        let token = FeishuApiClient::get_tenant_token(credentials, token_manager, bot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no token for bot {}", bot_id))?;

        let client = shared_http_client();
        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type={}",
            base_url, receive_id_type
        );

        // 飞书 Interactive Card 的 content 直接是 JSON 字符串，不需要额外的嵌套
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "interactive",
            "content": card_json
        });

        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("send_card request failed: {}", e))?;

        let status = res.status();
        if !status.is_success() {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            return Err(anyhow::anyhow!("send_card failed: {} {:?}", status, body));
        }

        tracing::debug!(
            "[feishu:{}] send_card ok to {} ({})",
            bot_id, receive_id, receive_id_type
        );
        Ok(())
    }

    /// Reply to a message with an interactive card.
    pub(crate) async fn reply_card(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        message_id: &str,
        card_json: &str,
    ) -> anyhow::Result<()> {
        let base_url = FeishuApiClient::base_url(credentials, bot_id)
            .ok_or_else(|| anyhow::anyhow!("no base_url for bot {}", bot_id))?;
        let token = FeishuApiClient::get_tenant_token(credentials, token_manager, bot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no token for bot {}", bot_id))?;

        let client = shared_http_client();
        // 使用 reply API 而不是 create
        let url = format!(
            "{}/open-apis/im/v1/messages/{}/reply",
            base_url, message_id
        );

        let body = serde_json::json!({
            "msg_type": "interactive",
            "content": card_json
        });

        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("reply_card request failed: {}", e))?;

        let status = res.status();
        if !status.is_success() {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            return Err(anyhow::anyhow!("reply_card failed: {} {:?}", status, body));
        }

        tracing::debug!(
            "[feishu:{}] reply_card ok to message {}",
            bot_id, message_id
        );
        Ok(())
    }

    /// Send a raw text message using a specific receive_id_type.
    pub(crate) async fn send_raw(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        receive_id: &str,
        receive_id_type: &str,
        text: &str,
    ) -> anyhow::Result<()> {
        let base_url = FeishuApiClient::base_url(credentials, bot_id)
            .ok_or_else(|| anyhow::anyhow!("no credentials for bot {}", bot_id))?;
        let token = FeishuApiClient::get_tenant_token(credentials, token_manager, bot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no token for bot {}", bot_id))?;

        let client = shared_http_client();
        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type={}",
            base_url, receive_id_type
        );
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": serde_json::to_string(&serde_json::json!({ "text": text })).unwrap_or_default()
        });

        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = res.status();
        if !status.is_success() {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            return Err(anyhow::anyhow!("send_raw failed: {} {:?}", status, body));
        }

        Ok(())
    }

    /// Send a card message using a specific receive_id_type.
    pub(crate) async fn send_card_raw(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        receive_id: &str,
        receive_id_type: &str,
        card_json: &str,
    ) -> anyhow::Result<()> {
        let base_url = FeishuApiClient::base_url(credentials, bot_id)
            .ok_or_else(|| anyhow::anyhow!("no credentials for bot {}", bot_id))?;
        let token = FeishuApiClient::get_tenant_token(credentials, token_manager, bot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no token for bot {}", bot_id))?;

        let client = shared_http_client();
        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type={}",
            base_url, receive_id_type
        );
        // 飞书 API 要求 content 字段是字符串格式的 JSON
        // json! 宏会自动将 &str 转义为 JSON 字符串值，无需手动处理
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "interactive",
            "content": card_json
        });

        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = res.status();
        if !status.is_success() {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            return Err(anyhow::anyhow!("send_card_raw failed: {} {:?}", status, body));
        }

        Ok(())
    }

    pub(crate) fn base_url(
        credentials: &DashMap<i64, (String, String, String)>,
        bot_id: i64,
    ) -> Option<String> {
        let domain = credentials.get(&bot_id)?.2.clone();
        Some(if domain == "lark" {
            "https://open.larksuite.com".to_string()
        } else {
            "https://open.feishu.cn".to_string()
        })
    }

    pub(crate) fn build_sdk_config(
        credentials: &DashMap<i64, (String, String, String)>,
        bot_id: i64,
    ) -> Option<FeishuSdkConfig> {
        let ref_val = credentials.get(&bot_id)?;
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
                .http_client(shared_http_client())
                .build(),
        )
    }

    pub(crate) async fn get_tenant_token(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
    ) -> Option<String> {
        let sdk_config = FeishuApiClient::build_sdk_config(credentials, bot_id)?;
        match token_manager.get_tenant_access_token(&sdk_config).await {
            Ok(token) => Some(token),
            Err(err) => {
                tracing::warn!("[feishu:{}] 获取 tenant_access_token 失败: {}", bot_id, err);
                None
            }
        }
    }

    pub(crate) async fn resolve_bot_open_id(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
    ) -> Option<String> {
        let token = FeishuApiClient::get_tenant_token(credentials, token_manager, bot_id).await?;
        let base_url = FeishuApiClient::base_url(credentials, bot_id)?;

        let client = shared_http_client();
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

    /// Add reaction, returns reaction_id on success.
    pub(crate) async fn add_reaction(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        message_id: &str,
        emoji_type: &str,
    ) -> Option<String> {
        let token = FeishuApiClient::get_tenant_token(credentials, token_manager, bot_id).await?;
        let base_url = FeishuApiClient::base_url(credentials, bot_id)?;

        let client = shared_http_client();
        let url = format!("{base_url}/open-apis/im/v1/messages/{message_id}/reactions");
        let body_json = serde_json::json!({
            "reaction_type": {
                "emoji_type": emoji_type
            }
        });
        tracing::info!(
            "[feishu:{}] add_reaction POST {} token={}... body={}",
            bot_id,
            url,
            &token[..token.len().min(10)],
            body_json
        );
        let res = match client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&body_json)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[feishu:{}] add_reaction request failed: {e}", bot_id);
                return None;
            }
        };

        let status = res.status();
        let body: serde_json::Value = match res.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("[feishu:{}] add_reaction parse failed: {e}", bot_id);
                return None;
            }
        };

        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            tracing::error!(
                "[feishu:{}] add_reaction API error (status={}): {body}",
                bot_id,
                status
            );
            return None;
        }

        let reaction_id = body
            .get("data")
            .and_then(|d| d.get("reaction_id"))
            .and_then(|v| v.as_str())
            .map(String::from);

        tracing::info!(
            "[feishu:{}] add_reaction {} ok, reaction_id={:?}",
            bot_id,
            emoji_type,
            reaction_id
        );
        reaction_id
    }

    /// Delete reaction by reaction_id.
    pub(crate) async fn delete_reaction(
        credentials: &DashMap<i64, (String, String, String)>,
        token_manager: &Arc<TokenManager>,
        bot_id: i64,
        message_id: &str,
        reaction_id: &str,
    ) {
        let token = match FeishuApiClient::get_tenant_token(credentials, token_manager, bot_id).await {
            Some(t) => t,
            None => return,
        };
        let base_url = match FeishuApiClient::base_url(credentials, bot_id) {
            Some(u) => u,
            None => return,
        };

        let client = shared_http_client();
        match client
            .delete(format!(
                "{base_url}/open-apis/im/v1/messages/{message_id}/reactions/{reaction_id}"
            ))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
        {
            Ok(res) => {
                let body: serde_json::Value = res.json().await.unwrap_or_default();
                let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
                if code == 0 {
                    tracing::info!("[feishu:{}] delete_reaction ok", bot_id);
                } else {
                    tracing::error!("[feishu:{}] delete_reaction API error: {body}", bot_id);
                }
            }
            Err(e) => {
                tracing::error!("[feishu:{}] delete_reaction request failed: {e}", bot_id);
            }
        }
    }
}
