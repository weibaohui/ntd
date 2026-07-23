use axum::{
    extract::{Path, Query, State},
    response::{sse::{Event, Sse}, IntoResponse},
    routing::{delete, get, post, put},
    Json, Router,
};
use futures_util::stream::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

use crate::handlers::{workspace_guard, AppError, AppState};
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
    /// 创建 bot 时归属的工作空间 ID
    pub workspace_id: Option<i64>,
}

// SSE 轮询飞书授权结果，支持页面关闭后继续执行
// 整体流程：通过 mpsc channel 将后台轮询任务的结果传递给 SSE 流，
// 前端建立 SSE 连接后，后台任务循环轮询飞书 API直至授权成功/超时/出错。
pub async fn feishu_poll_sse(
    State(state): State<AppState>,
    Query(params): Query<FeishuPollRequest>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let expire_in = params.expire_in.unwrap_or(600).clamp(60, 1800);
    let interval = Duration::from_secs(params.interval.unwrap_or(5).clamp(1, 30));
    let deadline = std::time::Instant::now() + Duration::from_secs(expire_in);
    let device_code = params.device_code.clone();

    // 使用 channel 在后台轮询任务和 SSE 流之间传递结果
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(1);

    // 将需要 move 进 tokio::spawn 的值提取出来
    let db = state.db.clone();
    // feishu_listener 是 Arc，直接 move 进 async 块，无需 clone
    let listener = state.feishu_listener;
    let workspace_id = params.workspace_id.unwrap_or(0);

    // 启动后台轮询任务：独立于请求处理线程，持续轮询飞书 API
    tokio::spawn(async move {
        // 为 HTTP 请求设置 30 秒超时，防止网络问题导致轮询挂起
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        // 辅助函数：发送 SSE 事件，成功返回 true，channel 关闭时返回 false
        async fn send_sse_event(
            tx: &tokio::sync::mpsc::Sender<Result<Event, std::convert::Infallible>>,
            response: &FeishuPollResponse,
            event_name: &str,
        ) -> bool {
            let payload = serde_json::to_string(response);
            let event = match payload {
                Ok(data) => Event::default().event(event_name).data(data),
                Err(e) => {
                    tracing::error!("failed to serialize SSE event: {}", e);
                    return true; // 序列化失败也继续，不中断流程
                }
            };
            // #8: 使用 tokio::select! 检测 channel 关闭并退出
            tokio::select! {
                _ = tx.closed() => return false,
                result = tx.send(Ok(event)) => {
                    if result.is_err() {
                        return false;
                    }
                }
            };
            true
        }

        loop {
            // #8: 检查 channel 是否已关闭（客户端断开）
            if tx.is_closed() {
                break;
            }

            // 每次循环检查是否超过总期限(expire_in)，超过则返回 timeout
            if std::time::Instant::now() > deadline {
                let response = FeishuPollResponse {
                    success: false,
                    error: Some("timeout".to_string()),
                    ..Default::default()
                };
                let _ = send_sse_event(&tx, &response, "result").await;
                break;
            }

            // 向飞书授权服务器发送 poll 请求，检查设备授权状态
            let res = match client
                .post("https://accounts.feishu.cn/oauth/v1/app/registration")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .form(&[
                    ("action", "poll"),
                    ("device_code", &device_code),
                ])
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    // HTTP 请求失败，发送 fail 事件并退出
                    let response = FeishuPollResponse {
                        success: false,
                        error: Some(e.to_string()),
                        ..Default::default()
                    };
                    let _ = send_sse_event(&tx, &response, "fail").await;
                    break;
                }
            };

            let body: serde_json::Value = match res.json().await {
                Ok(b) => b,
                Err(e) => {
                    // 响应解析失败，发送 fail 事件并退出
                    let response = FeishuPollResponse {
                        success: false,
                        error: Some(format!("failed to parse response: {}", e)),
                        ..Default::default()
                    };
                    let _ = send_sse_event(&tx, &response, "fail").await;
                    break;
                }
            };

            // 授权成功：提取 client_id/client_secret，创建 bot 并启动 listener
            if let (Some(app_id), Some(app_secret)) = (
                body.get("client_id").and_then(|v| v.as_str()),
                body.get("client_secret").and_then(|v| v.as_str()),
            ) {
                // 获取用户信息确定是飞书还是 Lark
                let user_info = body.get("user_info");
                let tenant_brand = user_info.and_then(|v| v.get("tenant_brand")).and_then(|v| v.as_str());
                let open_id = user_info.and_then(|v| v.get("open_id")).and_then(|v| v.as_str());

                let domain = if tenant_brand == Some("lark") {
                    Some("lark".to_string())
                } else {
                    Some("feishu".to_string())
                };

                // 查询 bot 信息验证凭证有效性
                let bot_name = match probe_bot(app_id, app_secret).await {
                    Ok(name) => Some(name),
                    Err(e) => {
                        tracing::warn!("probe_bot failed for app_id {}: {}", app_id, e);
                        None
                    }
                };

                // 在数据库中创建飞书 bot 记录，使用 SSE URL 中传入的 workspace_id
                let bot_id = match db
                    .create_agent_bot("feishu", bot_name.as_deref().unwrap_or("Feishu Bot"), app_id, app_secret, open_id.map(String::from), domain.clone(), workspace_id)
                    .await
                {
                    Ok(id) => Some(id),
                    Err(e) => {
                        tracing::error!("failed to create feishu bot: {}", e);
                        None
                    }
                };

                // bot 创建失败时发送错误事件
                if bot_id.is_none() {
                    let response = FeishuPollResponse {
                        success: false,
                        error: Some("failed to create bot in database".to_string()),
                        ..Default::default()
                    };
                    let _ = send_sse_event(&tx, &response, "fail").await;
                    break;
                }

                // 仅当 bot 创建成功时启动 listener（监听飞书消息）
                // bot_id 在上方 is_ok() 分支中已确定为 Some，直接 unwrap 安全
                #[allow(clippy::unwrap_used)]
                let bot_id = bot_id.unwrap();
                if let Ok(Some(bot)) = db.get_agent_bot(bot_id).await {
                    if bot.enabled {
                        // listener 是 Arc，loop 中多次 spawn 需要 clone 保留 owned 值
                        let listener_clone = listener.clone();
                        tokio::spawn(async move {
                            if let Err(e) = listener_clone.start_bot(&bot).await {
                                tracing::error!("failed to start feishu bot {}: {e}", bot.id);
                            }
                        });
                    }
                }

                // 发送成功结果事件
                let response = FeishuPollResponse {
                    success: true,
                    app_id: Some(app_id.to_string()),
                    app_secret: None,
                    domain,
                    open_id: open_id.map(String::from),
                    bot_name,
                    bot_id: Some(bot_id),
                    error: None,
                };
                let _ = send_sse_event(&tx, &response, "result").await;
                break;
            }

            // 终端错误：access_denied（用户拒绝）或 expired_token（二维码过期），不可重试
            if let Some(err) = body.get("error").and_then(|v| v.as_str()) {
                if err == "access_denied" || err == "expired_token" {
                    let response = FeishuPollResponse {
                        success: false,
                        error: Some(err.to_string()),
                        ..Default::default()
                    };
                    let _ = send_sse_event(&tx, &response, "result").await;
                    break;
                }
                // #9: slow_down：服务端要求降低请求频率，等待额外 5 秒后直接 continue
                if err == "slow_down" {
                    sleep(interval + Duration::from_secs(5)).await;
                    continue;
                }
            }

            // authorization_pending：用户还未扫码，发送 ping 心跳并等待 interval 后重试
            let ping_data = r#"{"status":"waiting"}"#;
            let ping_event = Event::default().event("ping").data(ping_data);
            tokio::select! {
                _ = tx.closed() => break,
                result = tx.send(Ok(ping_event)) => {
                    if result.is_err() {
                        break;
                    }
                }
            };
            sleep(interval).await;
        }
    });

    // 将 channel 转为 Stream 返回给前端
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
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
    /// 所有者 open_id（推送目标），扫码/首次私聊自动捕获；前端列表页展示
    pub owner_open_id: Option<String>,
    pub domain: Option<String>,
    pub enabled: bool,
    pub config: String,
    pub created_at: String,
    /// Bot 所属的工作空间 ID
    pub workspace_id: i64,
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
            owner_open_id: b.owner_open_id,
            domain: b.domain,
            enabled: b.enabled,
            config: b.config,
            created_at: b.created_at,
            workspace_id: b.workspace_id,
        })
        .collect();

    Ok(ApiResponse::ok(response))
}

pub async fn delete_agent_bot(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    // 先清理关联的项目绑定（feishu_project_bindings 无 FK CASCADE，需手动清理避免孤儿记录）。
    // 查询失败时记录错误但继续删除，因为 bot 本身的删除优先级更高。
    let bindings = state.db
        .get_feishu_project_bindings(id)
        .await
        .map_err(|e| {
            tracing::warn!("failed to query bindings for bot {} before deletion: {}", id, e);
            e
        })
        .unwrap_or_default();

    // 逐条删除绑定，记录失败但不中断（最坏情况是遗留孤儿绑定，由 cleanup_stale_running_bindings 兜底）。
    for b in bindings {
        if let Err(e) = state.db.delete_feishu_project_binding(b.id).await {
            tracing::error!("failed to delete binding {} for bot {}: {}", b.id, id, e);
        }
    }

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
    /// 推送目标（机器人所有者 open_id）：扫码创建或首次私聊时自动捕获，
    /// 是推送的权威来源。前端据此只读展示当前推送目标。
    pub owner_open_id: Option<String>,
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
    // 推送目标（owner_open_id）由系统自动捕获，不再接受前端手动填单聊/群聊 ID 与 receive_id_type
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
            // 推送目标改读 owner_open_id（自动捕获），不再依赖 p2p_receive_id
            owner_open_id: bot.owner_open_id.clone(),
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
    // 推送目标不再由此接口手动设置（改由 owner_open_id 自动捕获），故无 p2p/group/receive_id_type 分支

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
    // 推送目标（所有者）回读，供前端展示
    let owner_open_id = state.db.get_owner_open_id(req.bot_id).await.ok().flatten();

    Ok(ApiResponse::ok(FeishuPushStatus {
        bot_id: req.bot_id,
        push_level: updated.as_ref().map(|t| t.push_level.clone()).unwrap_or_default(),
        owner_open_id,
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
        .map_err(|e| match &e {
            sea_orm::DbErr::Custom(msg) if msg.contains("cannot be empty") => AppError::BadRequest(msg.clone()),
            _ => AppError::Internal(e.to_string()),
        })?;

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

// ============================================================================
// Workspace 相关的 API（阶段5）
// ============================================================================

/// 获取工作空间的斜杠命令列表
pub async fn list_workspace_slash_commands(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    // auto-deref 会自动将 Arc<Database> 解引用为 &Database，无需手动 *
    let commands = crate::db::workspace_slash_command::get_workspace_slash_commands(&state.db, workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(commands))
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkspaceSlashCommandRequest {
    pub slash_command: String,
    /// 命令类型：'todo' 或 'loop'，默认为 'todo'
    #[serde(default = "default_command_type")]
    pub command_type: String,
    /// 关联的 Todo ID（command_type='todo' 时使用）
    pub todo_id: i64,
    /// 关联的 Loop ID（command_type='loop' 时使用）
    pub loop_id: Option<i64>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_command_type() -> String {
    "todo".to_string()
}

/// 创建工作空间的斜杠命令
pub async fn create_workspace_slash_command(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
    Json(req): Json<CreateWorkspaceSlashCommandRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 校验 slash_command 格式（必须以 / 开头）
    if !req.slash_command.starts_with('/') {
        return Err(AppError::BadRequest("slash_command must start with /".to_string()));
    }
    // 校验 command_type
    if req.command_type != "todo" && req.command_type != "loop" {
        return Err(AppError::BadRequest("command_type must be 'todo' or 'loop'".to_string()));
    }

    let id = crate::db::workspace_slash_command::create_workspace_slash_command(
        &state.db,
        workspace_id,
        &req.slash_command,
        &req.command_type,
        req.todo_id,
        req.loop_id,
        req.enabled,
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(serde_json::json!({ "id": id })))
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateWorkspaceSlashCommandRequest {
    pub slash_command: Option<String>,
    /// 命令类型：'todo' 或 'loop'
    pub command_type: Option<String>,
    /// 关联的 Todo ID
    pub todo_id: Option<i64>,
    /// 关联的 Loop ID
    pub loop_id: Option<i64>,
    pub enabled: Option<bool>,
}

/// 更新工作空间的斜杠命令
pub async fn update_workspace_slash_command(
    State(state): State<AppState>,
    Path((_workspace_id, cmd_id)): Path<(i64, i64)>,
    Json(req): Json<UpdateWorkspaceSlashCommandRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 校验 slash_command 格式（如果提供）
    if let Some(ref cmd) = req.slash_command {
        if !cmd.starts_with('/') {
            return Err(AppError::BadRequest("slash_command must start with /".to_string()));
        }
    }
    // 校验 command_type（如果提供）
    if let Some(ref ct) = req.command_type {
        if ct != "todo" && ct != "loop" {
            return Err(AppError::BadRequest("command_type must be 'todo' or 'loop'".to_string()));
        }
    }

    crate::db::workspace_slash_command::update_workspace_slash_command(
        &state.db,
        cmd_id,
        req.slash_command.as_deref(),
        req.command_type.as_deref(),
        req.todo_id,
        req.loop_id,
        req.enabled,
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(serde_json::json!({"success": true})))
}

/// 删除工作空间的斜杠命令
pub async fn delete_workspace_slash_command(
    State(state): State<AppState>,
    Path((_workspace_id, cmd_id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    crate::db::workspace_slash_command::delete_workspace_slash_command(&state.db, cmd_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(serde_json::json!({"success": true})))
}

/// 获取工作空间的设置
pub async fn get_workspace_settings(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let settings = crate::db::workspace_setting::get_workspace_settings(&state.db, workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    match settings {
        Some(s) => Ok(ApiResponse::ok(serde_json::json!({
            "workspace_id": s.workspace_id,
            "default_response_type": s.default_response_type,
            "default_response_todo_id": s.default_response_todo_id,
            "default_response_loop_id": s.default_response_loop_id,
            "default_response_executor": s.default_response_executor,
            "updated_at": s.updated_at,
        }))),
        None => Ok(ApiResponse::ok(serde_json::json!({
            "workspace_id": workspace_id,
            "default_response_type": "todo",
            "default_response_todo_id": null,
            "default_response_loop_id": null,
            "default_response_executor": null,
            "updated_at": null,
        }))),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateWorkspaceSettingsRequest {
    pub default_response_type: Option<String>,
    pub default_response_todo_id: Option<i64>,
    pub default_response_loop_id: Option<i64>,
    pub default_response_executor: Option<String>,
}

/// 更新工作空间的设置
///
/// 若请求指定了默认响应 todo/loop，先校验其确实属于当前 workspace，防止通过设置
/// 间接引用其他 workspace 的资源（与 smart_create 的 workspace 隔离保持一致）。
pub async fn update_workspace_settings(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
    Json(req): Json<UpdateWorkspaceSettingsRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(todo_id) = req.default_response_todo_id {
        workspace_guard::verify_todo_belongs_to_ws(&state.db, todo_id, workspace_id).await?;
    }
    if let Some(loop_id) = req.default_response_loop_id {
        workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, workspace_id).await?;
    }

    crate::db::workspace_setting::upsert_workspace_settings(
        &state.db,
        workspace_id,
        req.default_response_type,
        req.default_response_todo_id,
        req.default_response_loop_id,
        req.default_response_executor,
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(serde_json::json!({"success": true})))
}

// ============================================================================
// Bot 变更 workspace 的级联逻辑（阶段6）
// ============================================================================

/// Bot 变更 workspace 请求
#[derive(Debug, Clone, Deserialize)]
pub struct MoveBotToWorkspaceRequest {
    pub workspace_id: i64,
}

/// 将 Bot 移动到另一个工作空间（阶段6：级联禁用原有 bindings）
///
/// 变更逻辑：
/// 1. pending binding（__pending__）直接删除
/// 2. 已生效 binding 设为 disabled（保留记录）
/// 3. 更新 bot.workspace_id
pub async fn move_bot_to_workspace(
    State(state): State<AppState>,
    Path(bot_id): Path<i64>,
    Json(req): Json<MoveBotToWorkspaceRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 1. 查询该 bot 的所有 project_bindings
    let bindings = state.db
        .get_feishu_project_bindings(bot_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 2. 级联处理 bindings
    for b in bindings {
        if b.chat_id == crate::models::PENDING_CHAT_ID {
            // pending binding 直接删除
            state.db.delete_feishu_project_binding(b.id).await
                .map_err(|e| AppError::Internal(e.to_string()))?;
        } else {
            // 已生效的 binding 设为 disabled
            state.db.update_feishu_project_binding_enabled(b.id, false).await
                .map_err(|e| AppError::Internal(e.to_string()))?;
        }
    }

    // 3. 更新 bot.workspace_id
    state.db.update_agent_bot_workspace_id(bot_id, req.workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 4. 如果 bot 正在运行，重启 listener 使其使用新的 workspace_id
    if state.feishu_listener.has_bot(bot_id) {
        if let Ok(Some(bot)) = state.db.get_agent_bot(bot_id).await {
            if bot.enabled {
                let listener = state.feishu_listener.clone();
                tokio::spawn(async move {
                    if let Err(e) = listener.start_bot(&bot).await {
                        tracing::error!("failed to restart feishu bot {} after workspace change: {e}", bot.id);
                    }
                });
            }
        }
    }

    Ok(ApiResponse::ok(serde_json::json!({
        "success": true,
        "message": format!("Bot moved to workspace {}, all existing bindings have been disabled", req.workspace_id)
    })))
}

// ============================================================================
// V1 路由：Bot 管理（非 workspace 作用域）
// ============================================================================

/// V1 变体：群白名单列表，bot_id 来自 Query 参数。
///
/// 与旧版保持一致：前端调用 `/api/v1/agent-bots/feishu/group-whitelist?bot_id=X`，
/// 路由注册在 `/feishu/group-whitelist`（无 `{bot_id}` 路径段），因此用 Query 提取。
pub async fn v1_get_group_whitelist(
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

/// V1 Bot 管理路由（非 workspace 作用域）。
///
/// 这些路由使用相对路径，期望被嵌套在 `/api/v1/agent-bots` 下。
/// Bot 管理路由不依赖 workspace 作用域，所有 handler 复用现有函数签名。
pub fn v1_bot_routes() -> Router<AppState> {
    Router::new()
        // GET / — 列出所有 agent bot
        .route("/", get(list_agent_bots))
        // DELETE /{id} — 删除指定 bot
        .route("/{id}", delete(delete_agent_bot))
        // PUT /{id}/config — 更新 bot 配置
        .route("/{id}/config", put(update_agent_bot_config))
        // PUT /{id}/workspace — 移动 bot 到另一个工作空间
        .route("/{id}/workspace", put(move_bot_to_workspace))
        // POST /feishu/init — 初始化飞书授权流程
        .route("/feishu/init", post(feishu_init))
        // POST /feishu/begin — 开始飞书设备授权
        .route("/feishu/begin", post(feishu_begin))
        // GET /feishu/poll-stream — SSE 轮询飞书授权结果
        .route("/feishu/poll-stream", get(feishu_poll_sse))
        // GET|PUT /feishu/push — 查询/更新推送配置
        .route("/feishu/push", get(get_feishu_push).put(update_feishu_push))
        // GET|POST /feishu/group-whitelist — 群白名单列表/添加（bot_id 来自 Query/body）
        .route("/feishu/group-whitelist", get(v1_get_group_whitelist).post(add_group_whitelist))
        // DELETE /feishu/group-whitelist/{id} — 删除群白名单条目
        .route("/feishu/group-whitelist/{id}", delete(delete_group_whitelist))
}

// ============================================================================
// V1 路由：Workspace 作用域（斜杠命令 + 设置）
// ============================================================================

/// V1 Workspace 路由（斜杠命令 + 设置），使用相对路径。
///
/// 这些路由期望被嵌套在 `/api/v1/workspaces/{ws}` 下，因此路径中不包含
/// workspace 前缀。所有 handler 已在签名中使用 `Path(workspace_id): Path<i64>`，
/// 嵌套后 axum 会自动将 `{ws}` 提取为 workspace_id 传给 handler。
pub fn v1_workspace_routes() -> Router<AppState> {
    Router::new()
        // GET|POST /slash-commands — 列出/创建斜杠命令
        .route("/slash-commands", get(list_workspace_slash_commands).post(create_workspace_slash_command))
        // PUT|DELETE /slash-commands/{cmd_id} — 更新/删除斜杠命令
        .route("/slash-commands/{cmd_id}", put(update_workspace_slash_command).delete(delete_workspace_slash_command))
        // GET|PUT /settings — 查询/更新工作空间设置
        .route("/settings", get(get_workspace_settings).put(update_workspace_settings))
}
