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
    // 关键：先把所有需要从 config 读的值一次性拷出来，然后立刻释放读锁。
    // 之后再去打云端 HTTP，否则在网络抖动期间会一直持读锁，阻塞其他写者。
    let (connected, authenticated, server_url, token, last_sync_at_fallback) = {
        let cfg = state.config.read().await;
        let server_url = cfg.cloud_sync.server_url.clone();
        let token = cfg.cloud_sync.sync_token.clone();
        let last_sync_at = cfg.cloud_sync.last_sync_at.clone();
        let connected = !server_url.is_empty();
        let authenticated = token.is_some();
        (connected, authenticated, server_url, token, last_sync_at)
    };

    // 如果已配置 token，尝试从云端获取真实同步状态
    let last_sync_at = if let Some(token) = &token {
        if !server_url.is_empty() {
            match reqwest::Client::new()
                .get(format!("{}/api/v1/sync/status?data_type=todos", server_url))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    match resp.text().await {
                        Ok(body) => match serde_yaml::from_str::<CloudSyncStatusResponse>(&body) {
                            Ok(data) => data.last_sync_at,
                            Err(_) => last_sync_at_fallback,
                        },
                        Err(_) => last_sync_at_fallback,
                    }
                }
                Ok(_) => last_sync_at_fallback,
                Err(_) => last_sync_at_fallback,
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

/// 将云端 todos 合并到本地数据库
/// 冲突策略（按 title 匹配）：
/// - overwrite: 覆盖本地同名 todo 的 prompt/status
/// - skip:      跳过云端这条，保留本地
/// - rename:    将云端 todo 重命名后插入（追加 "(云端)" 后缀）
/// 返回成功插入/更新的条数
async fn merge_cloud_todos_to_local(
    db: &Database,
    cloud_todos: &[CloudTodoItem],
    conflict_mode: &str,
) -> Result<i64, String> {
    let mut affected = 0i64;
    let local_todos = db.get_todos().await.map_err(|e| e.to_string())?;
    // 用 title 作为冲突判断键（小写比较避免大小写差异）
    let local_by_title: HashMap<String, i64> = local_todos
        .iter()
        .map(|t| (t.title.trim().to_lowercase(), t.id))
        .collect();

    for item in cloud_todos {
        let title = item.title.trim();
        if title.is_empty() {
            continue;
        }
        let key = title.to_lowercase();
        match conflict_mode {
            "skip" => {
                // 跳过：本地已有同名 todo 就忽略
                if local_by_title.contains_key(&key) {
                    continue;
                }
                let new_id = db.create_todo(title, &item.prompt).await.map_err(|e| e.to_string())?;
                affected += 1;
                tracing::info!("pull: 新增 todo id={} title={}", new_id, title);
            }
            "overwrite" => {
                if let Some(&id) = local_by_title.get(&key) {
                    // 覆盖：用云端数据更新本地
                    let status = item.status.parse::<TodoStatus>().unwrap_or(TodoStatus::Pending);
                    let executor = item.executor.as_deref();
                    db.update_todo_full(crate::db::TodoUpdate {
                        id,
                        title,
                        prompt: &item.prompt,
                        status,
                        executor,
                        scheduler_enabled: None,
                        scheduler_config: None,
                        scheduler_timezone: None,
                        workspace: item.workspace.as_deref(),
                        worktree_enabled: None,
                        acceptance_criteria: None,
                        auto_review_enabled: None,
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                    affected += 1;
                    tracing::info!("pull: 覆盖 todo id={} title={}", id, title);
                } else {
                    let new_id = db.create_todo(title, &item.prompt).await.map_err(|e| e.to_string())?;
                    affected += 1;
                    tracing::info!("pull: 新增 todo id={} title={}", new_id, title);
                }
            }
            "rename" => {
                // 重命名：本地已有同名就改个名字再插入
                let final_title = if local_by_title.contains_key(&key) {
                    format!("{} (云端)", title)
                } else {
                    title.to_string()
                };
                let new_id = db.create_todo(&final_title, &item.prompt).await.map_err(|e| e.to_string())?;
                affected += 1;
                tracing::info!("pull: 新增 todo id={} title={}", new_id, final_title);
            }
            _ => {
                // 未知策略：当作 skip 处理
                if !local_by_title.contains_key(&key) {
                    let new_id = db.create_todo(title, &item.prompt).await.map_err(|e| e.to_string())?;
                    affected += 1;
                }
            }
        }
    }
    Ok(affected)
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
    // 关键：先把所有需要从 config 读的值一次性拷出来，然后立刻释放读锁。
    // 之前这里把读锁一路持有到函数末尾，并随后在同一任务里取写锁，
    // tokio::sync::RwLock 不允许同一任务持读锁时再等待写锁，会自我死锁——
    // 云端早就返回了，本端 HTTP 响应却永远不返回。修法：缩短临界区。
    let dry_run = query.dry_run.unwrap_or(false);
    let (server_url, token, conflict_mode) = {
        let cfg = state.config.read().await;
        let token = cfg
            .cloud_sync
            .sync_token
            .clone()
            .ok_or_else(|| AppError::BadRequest("请先配置同步 Token".to_string()))?;
        let server_url = cfg.cloud_sync.server_url.clone();
        let conflict_mode = query
            .conflict_mode
            .clone()
            .unwrap_or_else(|| cfg.cloud_sync.default_conflict_mode.clone());
        (server_url, token, conflict_mode)
    };

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
    // 同步可能慢（拉全量 + 远端往返），给 reqwest 显式 5 分钟上限；
    // 0 是「无超时」，本场景下会让前端超时后本端继续挂起，徒增风险。
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| AppError::Internal(format!("构造 HTTP 客户端失败: {}", e)))?;
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

    // 必须以云端业务字段 `success` 为准，HTTP 2x 不代表同步成功。
    // 同时兼容 4xx/5xx：把响应体作为错误信息透传给用户。
    //
    // 云端 push 响应是 YAML（不是 JSON），且 `merged_data` 是 literal block 字符串，
    // 与 CloudSyncResponse 的 Option<CloudSyncData> 不匹配。只需要 success 字段，
    // 拆出一个最小结构避免误把 `merged_data` 当对象解析而炸掉。
    #[derive(Deserialize)]
    struct CloudPushStatusOnly {
        success: bool,
    }

    let (success, errors) = if !status.is_success() {
        (
            false,
            vec![format!("HTTP {}: {}", status.as_u16(), cloud_resp_text)],
        )
    } else {
        match serde_yaml::from_str::<CloudPushStatusOnly>(&cloud_resp_text) {
            Ok(parsed) if parsed.success => (true, vec![]),
            Ok(_) => {
                // 业务失败：HTTP 200 但 success=false，尝试提取 summary 字段给用户看。
                let summary = serde_yaml::from_str::<CloudSyncResponse>(&cloud_resp_text)
                    .ok()
                    .and_then(|r| r.summary)
                    .map(|s| {
                        format!(
                            "total={} new={} overwrite={} skip={} rename={}",
                            s.total_client_items, s.new_items, s.overwritten, s.skipped, s.renamed
                        )
                    })
                    .unwrap_or_default();
                let msg = if summary.is_empty() {
                    "云端未说明失败原因".to_string()
                } else {
                    format!("云端业务失败：{}", summary)
                };
                (false, vec![msg])
            }
            Err(e) => (
                false,
                vec![format!("解析云端响应失败: {} (body: {})", e, cloud_resp_text)],
            ),
        }
    };
    let pushed_count = if success { cloud_data.todos.len() as i64 } else { 0 };
    let error_message = if !success {
        Some(errors.join("\n"))
    } else {
        None
    };

    // 保存同步记录
    let _ = state
        .db
        .create_sync_record(
            "push",
            &conflict_mode,
            if success {
                if dry_run { "dry_run" } else { "success" }
            } else {
                "failed"
            },
            "todos",
            Some(&serde_json::json!({
                "pushed_count": pushed_count,
                "dry_run": dry_run
            }).to_string()),
            error_message.as_deref(),
        )
        .await;

    // 更新最后同步时间：现在没有别的读锁阻塞，可以安全取写锁。
    if success && !dry_run {
        let mut cfg = state.config.write().await;
        cfg.cloud_sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        let _ = cfg.save();
    }

    Ok(ApiResponse::ok(SyncResult {
        success,
        direction: "push".to_string(),
        conflict_mode,
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
    // 关键：先把所有需要从 config 读的值一次性拷出来，然后立刻释放读锁。
    // 详见 cloud_sync_push 的注释。
    let dry_run = query.dry_run.unwrap_or(false);
    let (server_url, token, conflict_mode) = {
        let cfg = state.config.read().await;
        let token = cfg
            .cloud_sync
            .sync_token
            .clone()
            .ok_or_else(|| AppError::BadRequest("请先配置同步 Token".to_string()))?;
        let server_url = cfg.cloud_sync.server_url.clone();
        let conflict_mode = query
            .conflict_mode
            .clone()
            .unwrap_or_else(|| cfg.cloud_sync.default_conflict_mode.clone());
        (server_url, token, conflict_mode)
    };

    // 调用云端 API 获取数据
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| AppError::Internal(format!("构造 HTTP 客户端失败: {}", e)))?;
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
                &conflict_mode,
                "failed",
                "todos",
                None,
                Some(&err_text),
            )
            .await;

        return Err(AppError::BadRequest(format!("拉取失败: {}", err_text)));
    }

    // 云端返回 YAML（text/yaml），不是 JSON。
    let body = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("读取响应失败: {}", e)))?;
    let cloud_resp: CloudPullResponse = serde_yaml::from_str(&body)
        .map_err(|e| AppError::Internal(format!("解析云端响应失败: {} (body: {})", e, body)))?;

    let pulled_count = if let Some(data_str) = &cloud_resp.data {
        if let Ok(cloud_data) = serde_yaml::from_str::<CloudSyncData>(data_str) {
            if !dry_run {
                // 实际合并云端 todos 到本地数据库
                match merge_cloud_todos_to_local(&state.db, &cloud_data.todos, &conflict_mode).await {
                    Ok(n) => n,
                    Err(e) => {
                        return Err(AppError::Internal(format!("合并数据失败: {}", e)));
                    }
                }
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
            &conflict_mode,
            if dry_run { "dry_run" } else { "success" },
            "todos",
            Some(&serde_json::json!({
                "pulled_count": pulled_count,
                "dry_run": dry_run
            }).to_string()),
            None,
        )
        .await;

    // 更新最后同步时间：现在没有别的读锁阻塞，可以安全取写锁。
    if !dry_run {
        let mut cfg = state.config.write().await;
        cfg.cloud_sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        let _ = cfg.save();
    }

    Ok(ApiResponse::ok(SyncResult {
        success: true,
        direction: "pull".to_string(),
        conflict_mode,
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

/// 同步历史分页响应：records 是当前页数据，total 是全部记录数
#[derive(Serialize)]
pub struct SyncRecordsResponse {
    pub records: Vec<SyncRecord>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// GET /api/cloud/sync/records - 获取同步历史记录
pub async fn cloud_sync_records(
    State(state): State<AppState>,
    Query(query): Query<SyncRecordsQuery>,
) -> Result<ApiResponse<SyncRecordsResponse>, AppError> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    // 并行获取当前页 + 总数，避免分页信息错乱
    let (records, total) = tokio::try_join!(
        state.db.get_sync_records(limit, offset),
        state.db.count_sync_records(),
    )
    .map_err(|e: sea_orm::DbErr| AppError::Internal(format!("获取同步记录失败: {}", e)))?;

    let response = SyncRecordsResponse {
        records: records
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
            .collect(),
        total,
        limit,
        offset,
    };

    Ok(ApiResponse::ok(response))
}

#[derive(Serialize)]
pub struct ClearSyncRecordsResponse {
    pub deleted: u64,
}

/// DELETE /api/cloud/sync/records - 清空全部同步历史
pub async fn cloud_clear_sync_records(
    State(state): State<AppState>,
) -> Result<ApiResponse<ClearSyncRecordsResponse>, AppError> {
    let deleted = state
        .db
        .clear_sync_records()
        .await
        .map_err(|e| AppError::Internal(format!("清空同步历史失败: {}", e)))?;
    Ok(ApiResponse::ok(ClearSyncRecordsResponse { deleted }))
}
