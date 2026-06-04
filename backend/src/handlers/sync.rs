//! 云端同步 handlers
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::Database;
use crate::handlers::{ApiResponse, AppError, AppState};

// ============ Types ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    pub server_url: String,
    pub token: Option<String>,
    pub device_id: Option<i64>,
    pub last_sync_at: Option<String>,
    pub default_conflict_mode: String,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            token: None,
            device_id: None,
            last_sync_at: None,
            default_conflict_mode: "overwrite".to_string(),
        }
    }
}

// ============ Device Handlers ============

#[derive(Deserialize)]
pub struct DeviceRequest {
    pub device_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct DeviceResponse {
    pub id: i64,
    pub device_name: String,
    pub last_seen_at: Option<String>,
    pub created_at: Option<String>,
}

/// POST /api/cloud/devices - 创建设备
pub async fn cloud_create_device(
    State(state): State<AppState>,
    Json(req): Json<DeviceRequest>,
) -> Result<ApiResponse<DeviceResponse>, AppError> {
    let (token, server_url) = {
        let cfg = state.config.read().await;
        (cfg.cloud_sync.token.clone(), cfg.cloud_sync.server_url.clone())
    };

    let token = token.ok_or_else(|| AppError::BadRequest("请先配置 Token".to_string()))?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/devices", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({ "device_name": req.device_name }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("网络错误: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::BadRequest(format!("设备注册失败 ({}): {}", status, text)));
    }

    let device_resp: DeviceResponse = resp.json().await
        .map_err(|e| AppError::Internal(format!("响应解析失败: {}", e)))?;

    // 保存 device_id 到配置
    {
        let mut cfg = state.config.write().await;
        cfg.cloud_sync.device_id = Some(device_resp.id);
        cfg.save().map_err(|e| AppError::Internal(e.to_string()))?;
    }

    Ok(ApiResponse::ok(device_resp))
}

// ============ Sync Status Handlers ============

#[derive(Serialize)]
pub struct SyncStatusResponse {
    pub connected: bool,
    pub authenticated: bool,
    pub device_id: Option<i64>,
    pub last_sync_at: Option<String>,
    pub server_url: String,
}

/// GET /api/cloud/sync/status - 获取同步状态
pub async fn cloud_sync_status(
    State(state): State<AppState>,
) -> Result<ApiResponse<SyncStatusResponse>, AppError> {
    let cfg = state.config.read().await;

    let connected = !cfg.cloud_sync.server_url.is_empty();
    let authenticated = cfg.cloud_sync.token.is_some();
    let device_id = cfg.cloud_sync.device_id;
    let last_sync_at = cfg.cloud_sync.last_sync_at.clone();
    let server_url = cfg.cloud_sync.server_url.clone();

    Ok(ApiResponse::ok(SyncStatusResponse {
        connected,
        authenticated,
        device_id,
        last_sync_at,
        server_url,
    }))
}

// ============ Config Handlers ============

#[derive(Deserialize)]
pub struct CloudConfigRequest {
    pub server_url: Option<String>,
    pub token: Option<String>,
    pub default_conflict_mode: Option<String>,
}

#[derive(Serialize)]
pub struct CloudConfigResponse {
    pub server_url: String,
    pub token: Option<String>,
    pub device_id: Option<i64>,
    pub last_sync_at: Option<String>,
    pub default_conflict_mode: String,
}

/// GET /api/cloud/config - 获取云端配置
pub async fn cloud_get_config(
    State(state): State<AppState>,
) -> Result<ApiResponse<CloudConfigResponse>, AppError> {
    let cfg = state.config.read().await;

    Ok(ApiResponse::ok(CloudConfigResponse {
        server_url: cfg.cloud_sync.server_url.clone(),
        token: cfg.cloud_sync.token.clone(),
        device_id: cfg.cloud_sync.device_id,
        last_sync_at: cfg.cloud_sync.last_sync_at.clone(),
        default_conflict_mode: cfg.cloud_sync.default_conflict_mode.clone(),
    }))
}

/// POST /api/cloud/config - 保存云端配置（包含 token）
pub async fn cloud_save_config(
    State(state): State<AppState>,
    Json(req): Json<CloudConfigRequest>,
) -> Result<ApiResponse<()>, AppError> {
    let mut cfg = state.config.write().await;
    if let Some(url) = req.server_url {
        cfg.cloud_sync.server_url = url.trim_end_matches('/').to_string();
    }
    if let Some(token) = req.token {
        cfg.cloud_sync.token = Some(token);
    }
    if let Some(mode) = req.default_conflict_mode {
        cfg.cloud_sync.default_conflict_mode = mode;
    }
    cfg.save().map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(()))
}

// ============ Sync Records Handlers ============

#[derive(Serialize)]
pub struct SyncRecord {
    pub id: i64,
    pub direction: String,
    pub conflict_mode: String,
    pub status: String,
    pub data_type: String,
    pub details: Option<String>,
    pub error_message: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Deserialize)]
pub struct SyncRecordsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /api/cloud/sync/records - 获取同步历史记录
pub async fn cloud_sync_records(
    State(state): State<AppState>,
    Query(query): Query<SyncRecordsQuery>,
) -> Result<ApiResponse<Vec<SyncRecord>>, AppError> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    let records = state.db.get_sync_records(limit, offset).await
        .map_err(|e| AppError::Internal(format!("获取同步记录失败: {}", e)))?;

    let response: Vec<SyncRecord> = records.into_iter().map(|r| SyncRecord {
        id: r.id,
        direction: r.direction,
        conflict_mode: r.conflict_mode,
        status: r.status,
        data_type: r.data_type,
        details: r.details,
        error_message: r.error_message,
        created_at: r.created_at,
    }).collect();

    Ok(ApiResponse::ok(response))
}

// ============ Logout Handler ============

/// POST /api/cloud/auth/logout - 清除 Token
pub async fn cloud_logout(
    State(state): State<AppState>,
) -> Result<ApiResponse<()>, AppError> {
    let mut cfg = state.config.write().await;
    cfg.cloud_sync.token = None;
    cfg.cloud_sync.device_id = None;
    cfg.cloud_sync.last_sync_at = None;
    cfg.save().map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(()))
}
