use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuInitResponse {
    pub supported: bool,
    pub auth_methods: Vec<String>,
}

pub async fn feishu_init() -> Result<impl IntoResponse, AppError> {
    let client = Client::new();
    let res = client
        .post("https://accounts.feishu.cn/oauth/v1/app/registration")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[("action", "init")])
        .send()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let body: serde_json::Value = res.json().await.map_err(|e| AppError::Internal(e.to_string()))?;

    let supported_auth_methods: Vec<String> = body
        .get("supported_auth_methods")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let supported = supported_auth_methods.contains(&"client_secret".to_string());

    let response = FeishuInitResponse {
        supported,
        auth_methods: supported_auth_methods,
    };
    Ok(ApiResponse::ok(response))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuBeginResponse {
    pub device_code: String,
    pub qr_url: String,
    pub user_code: String,
    pub interval: u64,
    pub expire_in: u64,
}

pub async fn feishu_begin() -> Result<impl IntoResponse, AppError> {
    let client = Client::new();
    let form = [
        ("action", "begin"),
        ("archetype", "PersonalAgent"),
        ("auth_method", "client_secret"),
        ("request_user_info", "open_id"),
    ];
    let res = client
        .post("https://accounts.feishu.cn/oauth/v1/app/registration")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let body: serde_json::Value = res.json().await.map_err(|e| AppError::Internal(e.to_string()))?;

    let device_code = body
        .get("device_code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal("Missing device_code".to_string()))?
        .to_string();

    let qr_url = body
        .get("verification_uri_complete")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal("Missing verification_uri_complete".to_string()))?
        .to_string();

    if !qr_url.starts_with("https://accounts.feishu.cn/") && !qr_url.starts_with("https://accounts.larksuite.com/") && !qr_url.starts_with("https://open.feishu.cn/") && !qr_url.starts_with("https://open.larksuite.com/") {
        return Err(AppError::Internal(format!("Invalid verification URI domain: {}", qr_url)));
    }

    let user_code = body
        .get("user_code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let interval = body
        .get("interval")
        .and_then(|v| v.as_u64())
        .unwrap_or(5);

    let _feishu_expire_in = body
        .get("expire_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(600);

    // 覆盖为 30 分钟，避免二维码太快过期
    let expire_in: u64 = 1800;

    let response = FeishuBeginResponse {
        device_code,
        qr_url,
        user_code,
        interval,
        expire_in,
    };
    Ok(ApiResponse::ok(response))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct FeishuPollResponse {
    pub success: bool,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub domain: Option<String>,
    pub open_id: Option<String>,
    pub bot_name: Option<String>,
    pub bot_id: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeishuPollRequest {
    pub device_code: String,
    pub interval: Option<u64>,
    pub expire_in: Option<u64>,
}

pub async fn feishu_poll(
    State(state): State<AppState>,
    Json(req): Json<FeishuPollRequest>,
) -> Result<impl IntoResponse, AppError> {
    let client = Client::new();
    let expire_in = req.expire_in.unwrap_or(600).clamp(60, 1800);
    let interval = Duration::from_secs(req.interval.unwrap_or(5).clamp(1, 30));
    let deadline = std::time::Instant::now() + Duration::from_secs(expire_in);

    loop {
        if std::time::Instant::now() > deadline {
            return Ok(ApiResponse::ok(FeishuPollResponse {
                success: false,
                error: Some("timeout".to_string()),
                ..Default::default()
            }));
        }

        let res = client
            .post("https://accounts.feishu.cn/oauth/v1/app/registration")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&[
                ("action", "poll"),
                ("device_code", &req.device_code),
            ])
            .send()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let body: serde_json::Value = res.json().await.map_err(|e| AppError::Internal(e.to_string()))?;

        // 授权成功
        if let (Some(app_id), Some(app_secret)) = (
            body.get("client_id").and_then(|v| v.as_str()),
            body.get("client_secret").and_then(|v| v.as_str()),
        ) {
            let user_info = body.get("user_info");
            let tenant_brand = user_info.and_then(|v| v.get("tenant_brand")).and_then(|v| v.as_str());
            let open_id = user_info.and_then(|v| v.get("open_id")).and_then(|v| v.as_str());

            let domain = if tenant_brand == Some("lark") {
                Some("lark".to_string())
            } else {
                Some("feishu".to_string())
            };

            let bot_name = match probe_bot(app_id, app_secret).await {
                Ok(name) => Some(name),
                Err(e) => {
                    tracing::warn!("probe_bot failed for app_id {}: {}", app_id, e);
                    None
                }
            };

            let bot_id = state
                .db
                .create_agent_bot("feishu", bot_name.as_deref().unwrap_or("Feishu Bot"), app_id, app_secret, open_id.map(String::from), domain.clone())
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            // 启动新创建的 bot listener
            if let Ok(Some(bot)) = state.db.get_agent_bot(bot_id).await {
                if bot.enabled {
                    let listener = state.feishu_listener.clone();
                    tokio::spawn(async move {
                        if let Err(e) = listener.start_bot(&bot).await {
                            tracing::error!("failed to start feishu bot {}: {e}", bot.id);
                        }
                    });
                }
            }

            return Ok(ApiResponse::ok(FeishuPollResponse {
                success: true,
                app_id: Some(app_id.to_string()),
                app_secret: None,
                domain,
                open_id: open_id.map(String::from),
                bot_name,
                bot_id: Some(bot_id),
                error: None,
            }));
        }

        // 终端错误
        if let Some(err) = body.get("error").and_then(|v| v.as_str()) {
            if err == "access_denied" || err == "expired_token" {
                return Ok(ApiResponse::ok(FeishuPollResponse {
                    success: false,
                    error: Some(err.to_string()),
                    ..Default::default()
                }));
            }
            if err == "slow_down" {
                sleep(interval + Duration::from_secs(5)).await;
                continue;
            }
        }

        // authorization_pending，等待后重试
        sleep(interval).await;
    }
}


async fn probe_bot(app_id: &str, app_secret: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::new();

    let token_res = client
        .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
        .json(&serde_json::json!({
            "app_id": app_id,
            "app_secret": app_secret
        }))
        .send()
        .await?;

    let token_body: serde_json::Value = token_res.json().await?;
    let token = token_body
        .get("tenant_access_token")
        .and_then(|v| v.as_str())
        .ok_or("Missing tenant_access_token")?;

    let bot_res = client
        .get("https://open.feishu.cn/open-apis/bot/v3/info")
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    let bot_body: serde_json::Value = bot_res.json().await?;
    let bot_name = bot_body
        .get("bot")
        .and_then(|v| v.get("app_name"))
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(bot_name.unwrap_or_else(|| "Feishu Bot".to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBotResponse {
    pub id: i64,
    pub bot_type: String,
    pub bot_name: String,
    pub app_id: String,
    pub bot_open_id: Option<String>,
    pub domain: Option<String>,
    pub enabled: bool,
    pub config: String,
    pub created_at: String,
}

pub async fn list_agent_bots(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let bots = state.db.get_agent_bots().await.map_err(|e| AppError::Internal(e.to_string()))?;

    let response: Vec<AgentBotResponse> = bots
        .into_iter()
        .map(|b| AgentBotResponse {
            id: b.id,
            bot_type: b.bot_type,
            bot_name: b.bot_name,
            app_id: b.app_id,
            bot_open_id: b.bot_open_id,
            domain: b.domain,
            enabled: b.enabled,
            config: b.config,
            created_at: b.created_at,
        })
        .collect();

    Ok(ApiResponse::ok(response))
}

pub async fn delete_agent_bot(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.db.delete_agent_bot(id).await.map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(serde_json::json!({"success": true})))
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBotConfigRequest {
    pub config: String,
}

pub async fn update_agent_bot_config(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateBotConfigRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate config JSON
    let _: serde_json::Value = serde_json::from_str(&req.config)
        .map_err(|e| AppError::BadRequest(format!("Invalid config JSON: {e}")))?;

    state
        .db
        .update_agent_bot_config(id, &req.config)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Restart the bot listener if it's running
    if state.feishu_listener.has_bot(id) {
        if let Ok(Some(bot)) = state.db.get_agent_bot(id).await {
            if bot.enabled {
                let listener = state.feishu_listener.clone();
                tokio::spawn(async move {
                    if let Err(e) = listener.start_bot(&bot).await {
                        tracing::error!("failed to restart feishu bot {}: {e}", bot.id);
                    }
                });
            }
        }
    }

    Ok(ApiResponse::ok(serde_json::json!({"success": true})))
}

#[derive(Debug, Clone, Serialize)]
pub struct FeishuPushStatus {
    pub bot_id: i64,
    pub push_level: String,
    pub p2p_receive_id: String,
    pub group_chat_id: String,
    pub receive_id_type: String,
    pub p2p_response_enabled: bool,
    pub group_response_enabled: bool,
    pub p2p_debounce_secs: i64,
    pub group_debounce_secs: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateFeishuPushRequest {
    pub bot_id: i64,
    pub push_level: Option<String>,
    pub p2p_receive_id: Option<String>,
    pub group_chat_id: Option<String>,
    pub receive_id_type: Option<String>,
    pub p2p_response_enabled: Option<bool>,
    pub group_response_enabled: Option<bool>,
    pub p2p_debounce_secs: Option<i64>,
    pub group_debounce_secs: Option<i64>,
}

pub async fn get_feishu_push(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let bots = state.db.get_agent_bots().await.map_err(|e| AppError::Internal(e.to_string()))?;
    let mut statuses = Vec::new();

    for bot in bots.into_iter().filter(|b| b.bot_type == "feishu") {
        let p2p_enabled = state.db.get_feishu_response_enabled(bot.id, "p2p").await.unwrap_or(false);
        let group_enabled = state.db.get_feishu_response_enabled(bot.id, "group").await.unwrap_or(false);
        let target = state.db.get_feishu_push_target(bot.id).await.ok().flatten();

        let p2p_debounce = state.db.get_debounce_secs(bot.id, "p2p").await.unwrap_or(20);
        let group_debounce = state.db.get_debounce_secs(bot.id, "group").await.unwrap_or(20);

        statuses.push(FeishuPushStatus {
            bot_id: bot.id,
            push_level: target.as_ref().map(|t| t.push_level.clone()).unwrap_or_else(|| "disabled".to_string()),
            p2p_receive_id: target.as_ref().map(|t| t.p2p_receive_id.clone()).unwrap_or_default(),
            group_chat_id: target.as_ref().map(|t| t.group_chat_id.clone()).unwrap_or_default(),
            receive_id_type: target.as_ref().map(|t| t.receive_id_type.clone()).unwrap_or_else(|| "open_id".to_string()),
            p2p_response_enabled: p2p_enabled,
            group_response_enabled: group_enabled,
            p2p_debounce_secs: p2p_debounce,
            group_debounce_secs: group_debounce,
        });
    }

    Ok(ApiResponse::ok(statuses))
}

pub async fn update_feishu_push(
    State(state): State<AppState>,
    Json(req): Json<UpdateFeishuPushRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(level) = &req.push_level {
        state.db.update_feishu_push_level(req.bot_id, level).await.map_err(|e| AppError::Internal(e.to_string()))?;
    }
    if let Some(p2p_id) = &req.p2p_receive_id {
        state.db.set_p2p_receive_id(req.bot_id, p2p_id).await.map_err(|e| AppError::Internal(e.to_string()))?;
    }
    if let Some(group_id) = &req.group_chat_id {
        state.db.set_group_chat_id(req.bot_id, group_id).await.map_err(|e| AppError::Internal(e.to_string()))?;
    }
    if let Some(rid_type) = &req.receive_id_type {
        state.db.update_receive_id_type(req.bot_id, rid_type).await.map_err(|e| AppError::Internal(e.to_string()))?;
    }

    if let Some(p2p_enabled) = req.p2p_response_enabled {
        state.db.set_feishu_response_enabled(req.bot_id, "p2p", p2p_enabled).await.map_err(|e| AppError::Internal(e.to_string()))?;
    }
    if let Some(group_enabled) = req.group_response_enabled {
        state.db.set_feishu_response_enabled(req.bot_id, "group", group_enabled).await.map_err(|e| AppError::Internal(e.to_string()))?;
    }
    if let Some(p2p_debounce) = req.p2p_debounce_secs {
        state.db.set_debounce_secs(req.bot_id, "p2p", p2p_debounce).await.map_err(|e| AppError::Internal(e.to_string()))?;
    }
    if let Some(group_debounce) = req.group_debounce_secs {
        state.db.set_debounce_secs(req.bot_id, "group", group_debounce).await.map_err(|e| AppError::Internal(e.to_string()))?;
    }

    let _ = state.feishu_push_mutator.send(crate::services::feishu_push::PushConfigUpdate::Refresh);

    let updated = state.db.get_feishu_push_target(req.bot_id).await.map_err(|e| AppError::Internal(e.to_string()))?;
    let p2p_enabled = state.db.get_feishu_response_enabled(req.bot_id, "p2p").await.unwrap_or(false);
    let group_enabled = state.db.get_feishu_response_enabled(req.bot_id, "group").await.unwrap_or(false);
    let p2p_debounce = state.db.get_debounce_secs(req.bot_id, "p2p").await.unwrap_or(20);
    let group_debounce = state.db.get_debounce_secs(req.bot_id, "group").await.unwrap_or(20);

    Ok(ApiResponse::ok(FeishuPushStatus {
        bot_id: req.bot_id,
        push_level: updated.as_ref().map(|t| t.push_level.clone()).unwrap_or_default(),
        p2p_receive_id: updated.as_ref().map(|t| t.p2p_receive_id.clone()).unwrap_or_default(),
        group_chat_id: updated.as_ref().map(|t| t.group_chat_id.clone()).unwrap_or_default(),
        receive_id_type: updated.as_ref().map(|t| t.receive_id_type.clone()).unwrap_or_else(|| "open_id".to_string()),
        p2p_response_enabled: p2p_enabled,
        group_response_enabled: group_enabled,
        p2p_debounce_secs: p2p_debounce,
        group_debounce_secs: group_debounce,
    }))
}

// Group Whitelist APIs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhitelistEntry {
    pub id: i64,
    pub bot_id: i64,
    pub sender_open_id: String,
    pub sender_name: Option<String>,
    pub created_at: Option<String>,
}

pub async fn get_group_whitelist(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let bot_id: i64 = params.get("bot_id")
        .ok_or(AppError::BadRequest("bot_id required".into()))?
        .parse()
        .map_err(|_| AppError::BadRequest("invalid bot_id".into()))?;

    let list = state.db.get_group_whitelist(bot_id).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let entries: Vec<WhitelistEntry> = list.into_iter().map(|w| WhitelistEntry {
        id: w.id,
        bot_id: w.bot_id,
        sender_open_id: w.sender_open_id,
        sender_name: w.sender_name,
        created_at: w.created_at,
    }).collect();

    Ok(ApiResponse::ok(entries))
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddWhitelistRequest {
    pub bot_id: i64,
    pub sender_open_id: String,
    pub sender_name: Option<String>,
}

pub async fn add_group_whitelist(
    State(state): State<AppState>,
    Json(req): Json<AddWhitelistRequest>,
) -> Result<impl IntoResponse, AppError> {
    let entry = state.db.add_group_whitelist(req.bot_id, &req.sender_open_id, req.sender_name.as_deref()).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(WhitelistEntry {
        id: entry.id,
        bot_id: entry.bot_id,
        sender_open_id: entry.sender_open_id,
        sender_name: entry.sender_name,
        created_at: entry.created_at,
    }))
}

pub async fn delete_group_whitelist(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.db.remove_group_whitelist(id).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(serde_json::json!({"success": true})))
}
