//! 云端同步 handlers
use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::db::Database;
use crate::handlers::{ApiResponse, AppError, AppState};
use crate::models::{Todo, TodoStatus};

// ============ Types ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    pub server_url: String,
    /// 同步 Token (ntd_xxx 格式)
    pub sync_token: Option<String>,
    /// 最后同步时间
    pub last_sync_at: Option<String>,
    /// 默认冲突解决模式
    pub default_conflict_mode: String,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            sync_token: None,
            last_sync_at: None,
            default_conflict_mode: "overwrite".to_string(),
        }
    }
}

// 云端同步的 TodoItem 格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudTodoItem {
    pub title: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub executor: Option<String>,
    #[serde(default)]
    pub scheduler_enabled: bool,
    #[serde(default)]
    pub scheduler_config: Option<String>,
    #[serde(default)]
    pub tag_names: Vec<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub worktree: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncData {
    pub version: String,
    #[serde(default)]
    pub todos: Vec<CloudTodoItem>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncResponse {
    pub success: bool,
    #[serde(default)]
    pub merged_data: Option<CloudSyncData>,
    #[serde(default)]
    pub preview: Option<bool>,
    #[serde(default)]
    pub conflicts: Option<Vec<CloudConflict>>,
    #[serde(default)]
    pub summary: Option<CloudSyncSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConflict {
    pub title: String,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub server_item: Option<CloudTodoItem>,
    #[serde(default)]
    pub client_item: Option<CloudTodoItem>,
    #[serde(default)]
    pub new_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncSummary {
    #[serde(default)]
    pub total_client_items: i64,
    #[serde(default)]
    pub new_items: i64,
    #[serde(default)]
    pub overwritten: i64,
    #[serde(default)]
    pub skipped: i64,
    #[serde(default)]
    pub renamed: i64,
    #[serde(default)]
    pub final_total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudPullResponse {
    #[serde(default)]
    pub data_type: String,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

// ============ Sync Status Handlers ============

#[derive(Serialize)]
pub struct SyncStatusResponse {
    pub connected: bool,
    pub authenticated: bool,
    pub last_sync_at: Option<String>,
    pub server_url: String,
}

#[derive(Deserialize)]
struct CloudSyncStatusResponse {
    last_sync_at: Option<String>,
}

/// GET /api/cloud/sync/status - 获取同步状态
pub async fn cloud_sync_status(
    State(state): State<AppState>,
) -> Result<ApiResponse<SyncStatusResponse>, AppError> {
    let cfg = state.config.read().await;

    let connected = !cfg.cloud_sync.server_url.is_empty();
    let authenticated = cfg.cloud_sync.sync_token.is_some();
    let server_url = cfg.cloud_sync.server_url.clone();

    // 如果已配置 token，尝试从云端获取真实同步状态
    let last_sync_at = if let Some(token) = &cfg.cloud_sync.sync_token {
        if !cfg.cloud_sync.server_url.is_empty() {
            match reqwest::Client::new()
                .get(format!("{}/api/v1/sync/status?data_type=todos", cfg.cloud_sync.server_url))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<CloudSyncStatusResponse>().await {
                        Ok(data) => data.last_sync_at,
                        Err(_) => cfg.cloud_sync.last_sync_at.clone(),
                    }
                }
                Ok(_) => cfg.cloud_sync.last_sync_at.clone(),
                Err(_) => cfg.cloud_sync.last_sync_at.clone(),
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(ApiResponse::ok(SyncStatusResponse {
        connected,
        authenticated,
        last_sync_at,
        server_url,
    }))
}

// ============ Config Handlers ============

#[derive(Deserialize)]
pub struct CloudConfigRequest {
    pub server_url: Option<String>,
    /// 同步 Token (ntd_xxx 格式)
    pub sync_token: Option<String>,
    pub default_conflict_mode: Option<String>,
}

#[derive(Serialize)]
pub struct CloudConfigResponse {
    pub server_url: String,
    /// 是否已配置 Token (不返回实际 token)
    pub has_token: bool,
    pub last_sync_at: Option<String>,
    pub default_conflict_mode: String,
}

#[derive(Serialize)]
pub struct SaveResponse {
    pub saved: bool,
}

/// GET /api/cloud/config - 获取云端配置
pub async fn cloud_get_config(
    State(state): State<AppState>,
) -> Result<ApiResponse<CloudConfigResponse>, AppError> {
    let cfg = state.config.read().await;

    Ok(ApiResponse::ok(CloudConfigResponse {
        server_url: cfg.cloud_sync.server_url.clone(),
        has_token: cfg.cloud_sync.sync_token.is_some(),
        last_sync_at: cfg.cloud_sync.last_sync_at.clone(),
        default_conflict_mode: cfg.cloud_sync.default_conflict_mode.clone(),
    }))
}

/// POST /api/cloud/config - 保存云端配置
pub async fn cloud_save_config(
    State(state): State<AppState>,
    Json(req): Json<CloudConfigRequest>,
) -> Result<ApiResponse<SaveResponse>, AppError> {
    let mut cfg = state.config.write().await;
    if let Some(url) = req.server_url {
        cfg.cloud_sync.server_url = url.trim_end_matches('/').to_string();
    }
    if let Some(token) = req.sync_token {
        cfg.cloud_sync.sync_token = Some(token);
    }
    if let Some(mode) = req.default_conflict_mode {
        cfg.cloud_sync.default_conflict_mode = mode;
    }
    cfg.save().map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(SaveResponse { saved: true }))
}

// ============ Sync Handlers ============

#[derive(Deserialize)]
pub struct SyncQuery {
    pub conflict_mode: Option<String>,
    pub dry_run: Option<bool>,
}

#[derive(Serialize)]
pub struct SyncResult {
    pub success: bool,
    pub direction: String,
    pub conflict_mode: String,
    pub dry_run: bool,
    pub pushed_count: i64,
    pub pulled_count: i64,
    pub conflicts_count: i64,
    pub errors: Vec<String>,
}

fn local_todos_to_cloud(todos: Vec<Todo>, tag_map: HashMap<i64, String>) -> CloudSyncData {
    let cloud_todos: Vec<CloudTodoItem> = todos
        .into_iter()
        .map(|t| {
            let tag_names: Vec<String> = t
                .tag_ids
                .iter()
                .filter_map(|id| tag_map.get(id).cloned())
                .collect();

            CloudTodoItem {
                title: t.title,
                prompt: t.prompt,
                status: t.status.to_string(),
                executor: t.executor,
                scheduler_enabled: t.scheduler_enabled,
                scheduler_config: t.scheduler_config,
                tag_names,
                workspace: t.workspace,
                worktree: t.worktree_enabled.then(|| "auto".to_string()),
            }
        })
        .collect();

    CloudSyncData {
        version: "1.0".to_string(),
        todos: cloud_todos,
        tags: vec![],
        skills: vec![],
    }
}

/// POST /api/cloud/sync/push - 向上同步（上传本地 todos 到云端）
pub async fn cloud_sync_push(
    State(state): State<AppState>,
    Query(query): Query<SyncQuery>,
) -> Result<ApiResponse<SyncResult>, AppError> {
    let cfg = state.config.read().await;

    let token = cfg
        .cloud_sync
        .sync_token
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("请先配置同步 Token".to_string()))?;

    let server_url = cfg.cloud_sync.server_url.clone();
    let conflict_mode = query
        .conflict_mode
        .as_deref()
        .unwrap_or(&cfg.cloud_sync.default_conflict_mode);
    let dry_run = query.dry_run.unwrap_or(false);

    // 获取本地 todos
    let todos = state.db.get_todos().await.map_err(|e| {
        AppError::Internal(format!("获取本地 todos 失败: {}", e))
    })?;

    // 获取标签映射
    let tags = state.db.get_tags().await.map_err(|e| {
        AppError::Internal(format!("获取本地标签失败: {}", e))
    })?;
    let tag_map: HashMap<i64, String> = tags.into_iter().map(|t| (t.id, t.name)).collect();

    // 转换为云端格式
    let cloud_data = local_todos_to_cloud(todos, tag_map);
    let yaml_content = serde_yaml::to_string(&cloud_data)
        .map_err(|e| AppError::Internal(format!("序列化失败: {}", e)))?;

    // 调用云端 API
    let client = reqwest::Client::new();
    let body = format!(
        "data_type: todos\nconflict_mode: {}\ndry_run: {}\ndata: |\n{}",
        conflict_mode,
        dry_run,
        yaml_content.lines().map(|l| format!("  {}", l)).collect::<Vec<_>>().join("\n")
    );

    let resp = client
        .post(format!("{}/api/v1/sync/push", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/yaml")
        .body(body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("网络请求失败: {}", e)))?;

    let status = resp.status();
    let cloud_resp_text = resp.text().await.unwrap_or_default();

    // 记录同步结果
    let success = status.is_success();
    let pushed_count = if success { cloud_data.todos.len() as i64 } else { 0 };
    let errors = if !success {
        vec![cloud_resp_text.clone()]
    } else {
        vec![]
    };

    // 保存同步记录
    let _ = state
        .db
        .create_sync_record(
            "push",
            conflict_mode,
            if success { "success" } else { "failed" },
            "todos",
            Some(&serde_json::json!({
                "pushed_count": pushed_count,
                "dry_run": dry_run
            }).to_string()),
            if !success { Some(&cloud_resp_text) } else { None },
        )
        .await;

    // 更新最后同步时间
    if success && !dry_run {
        let mut cfg = state.config.write().await;
        cfg.cloud_sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        let _ = cfg.save();
    }

    Ok(ApiResponse::ok(SyncResult {
        success,
        direction: "push".to_string(),
        conflict_mode: conflict_mode.to_string(),
        dry_run,
        pushed_count,
        pulled_count: 0,
        conflicts_count: 0,
        errors,
    }))
}

/// GET /api/cloud/sync/pull - 向下同步（从云端拉取 todos）
pub async fn cloud_sync_pull(
    State(state): State<AppState>,
    Query(query): Query<SyncQuery>,
) -> Result<ApiResponse<SyncResult>, AppError> {
    let cfg = state.config.read().await;

    let token = cfg
        .cloud_sync
        .sync_token
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("请先配置同步 Token".to_string()))?;

    let server_url = cfg.cloud_sync.server_url.clone();
    let conflict_mode = query
        .conflict_mode
        .as_deref()
        .unwrap_or(&cfg.cloud_sync.default_conflict_mode);
    let dry_run = query.dry_run.unwrap_or(false);

    // 调用云端 API 获取数据
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/v1/sync/pull?data_type=todos", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("网络请求失败: {}", e)))?;

    let status = resp.status();

    if !status.is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        // 记录失败
        let _ = state
            .db
            .create_sync_record(
                "pull",
                conflict_mode,
                "failed",
                "todos",
                None,
                Some(&err_text),
            )
            .await;

        return Err(AppError::BadRequest(format!("拉取失败: {}", err_text)));
    }

    let cloud_resp: CloudPullResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("解析响应失败: {}", e)))?;

    let pulled_count = if let Some(data_str) = &cloud_resp.data {
        if let Ok(cloud_data) = serde_yaml::from_str::<CloudSyncData>(data_str) {
            if !dry_run {
                // TODO: 根据 conflict_mode 合并 todos 到本地数据库
                cloud_data.todos.len() as i64
            } else {
                cloud_data.todos.len() as i64
            }
        } else {
            0
        }
    } else {
        0
    };

    // 记录同步结果
    let _ = state
        .db
        .create_sync_record(
            "pull",
            conflict_mode,
            if dry_run { "dry_run" } else { "success" },
            "todos",
            Some(&serde_json::json!({
                "pulled_count": pulled_count,
                "dry_run": dry_run
            }).to_string()),
            None,
        )
        .await;

    // 更新最后同步时间
    if !dry_run {
        let mut cfg = state.config.write().await;
        cfg.cloud_sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        let _ = cfg.save();
    }

    Ok(ApiResponse::ok(SyncResult {
        success: true,
        direction: "pull".to_string(),
        conflict_mode: conflict_mode.to_string(),
        dry_run,
        pushed_count: 0,
        pulled_count,
        conflicts_count: 0,
        errors: vec![],
    }))
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

    let response: Vec<SyncRecord> = records
        .into_iter()
        .map(|r| SyncRecord {
            id: r.id,
            direction: r.direction,
            conflict_mode: r.conflict_mode,
            status: r.status,
            data_type: r.data_type,
            details: r.details,
            error_message: r.error_message,
            created_at: r.created_at,
        })
        .collect();

    Ok(ApiResponse::ok(response))
}
