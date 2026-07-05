//! 黑板（Blackboard）API Handler。
//!
//! 纯文件存储方案：
//! - `GET /api/workspaces/{workspace_id}/wiki/files`：获取文件列表（index/log + topics）
//! - `GET /api/workspaces/{workspace_id}/wiki/files/{slug}`：获取文件内容
//! - `GET /api/workspaces/{workspace_id}/blackboard`：获取配置（保留兼容）
//! - `PATCH /api/workspaces/{workspace_id}/blackboard/config`：更新配置

use axum::extract::{Path, State};
use axum::routing::get;
use axum::Router;

use crate::db::blackboard::BlackboardConfig;
use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::ApiResponse;
use crate::wiki::{list_topics, read_topic, read_index, read_log};

/// 黑板配置响应体（保留兼容，不含内容）
#[derive(Debug, serde::Serialize)]
pub struct BlackboardResponse {
    pub id: i64,
    pub workspace_id: i64,
    pub updated_at: Option<String>,
    /// 黑板更新防抖周期（秒）
    pub blackboard_debounce_secs: i64,
    /// 黑板更新防抖条数阈值
    pub blackboard_debounce_count: i64,
    /// Wiki 维护提示词模板（空字符串表示使用内置默认）
    pub wiki_prompt: String,
    /// 待处理的 execution_record_id 列表（JSON 数组字符串）
    pub pending_record_ids: String,
}

/// Wiki 文件列表项
#[derive(Debug, serde::Serialize)]
pub struct WikiFileItem {
    pub slug: String,
    pub file_type: String, // "index" / "log" / "topic"
}

/// Wiki 文件内容响应
#[derive(Debug, serde::Serialize)]
pub struct WikiFileContent {
    pub slug: String,
    pub content: String,
}

/// 更新黑板配置的请求体（所有字段可选，None 保持原值不变）。
#[derive(Debug, serde::Deserialize)]
pub struct UpdateBlackboardConfigRequest {
    pub blackboard_debounce_secs: Option<i64>,
    pub blackboard_debounce_count: Option<i64>,
    pub wiki_prompt: Option<String>,
}

/// `GET /api/workspaces/{workspace_id}/blackboard`
///
/// 获取指定工作空间的黑板配置（不含内容）。
pub async fn get_blackboard(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<BlackboardResponse>, AppError> {
    let board = state.db.get_blackboard(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("查询黑板失败: {}", e))
    })?;

    match board {
        Some(model) => Ok(ApiResponse::ok(BlackboardResponse {
            id: model.id,
            workspace_id: model.workspace_id,
            updated_at: model.updated_at,
            blackboard_debounce_secs: model.blackboard_debounce_secs,
            blackboard_debounce_count: model.blackboard_debounce_count,
            wiki_prompt: model.wiki_prompt,
            pending_record_ids: model.pending_record_ids,
        })),
        None => Ok(ApiResponse::ok(BlackboardResponse {
            id: 0,
            workspace_id,
            updated_at: None,
            blackboard_debounce_secs: 600,
            blackboard_debounce_count: 10,
            wiki_prompt: String::new(),
            pending_record_ids: String::from("[]"),
        })),
    }
}

/// `GET /api/workspaces/{workspace_id}/blackboard/config`
///
/// 仅获取指定工作空间的黑板配置（防抖阈值、提示词）。
pub async fn get_blackboard_config(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<BlackboardConfig>, AppError> {
    if let Err(e) = state.db.create_blackboard(workspace_id).await {
        tracing::warn!("get_blackboard_config: create_blackboard 幂等创建失败: {:?}", e);
    }
    let cfg = state.db.get_blackboard_config(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("查询黑板配置失败: {}", e))
    })?;
    match cfg {
        Some(c) => Ok(ApiResponse::ok(c)),
        None => Ok(ApiResponse::ok(BlackboardConfig {
            debounce_secs: 600,
            debounce_count: 10,
            wiki_prompt: String::new(),
        })),
    }
}

/// `PATCH /api/workspaces/{workspace_id}/blackboard/config`
///
/// 更新指定工作空间的黑板配置（防抖阈值、提示词）。
pub async fn update_blackboard_config(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
    ApiJson(req): ApiJson<UpdateBlackboardConfigRequest>,
) -> Result<ApiResponse<BlackboardConfig>, AppError> {
    if let Err(e) = state.db.create_blackboard(workspace_id).await {
        tracing::warn!("update_blackboard_config: create_blackboard 幂等创建失败: {:?}", e);
    }
    state.db.update_blackboard_config(
        workspace_id,
        req.blackboard_debounce_secs,
        req.blackboard_debounce_count,
        req.wiki_prompt.clone(),
    ).await.map_err(|e| AppError::Internal(format!("更新黑板配置失败: {}", e)))?;

    // debounce_secs 变更时，根据已计时长决定：超则立即触发 flush，未超则继续用新阈值计时
    if let Some(new_secs) = req.blackboard_debounce_secs {
        let clamped = new_secs.max(10);
        crate::services::blackboard_debouncer::reconcile_timer_after_config_change(
            workspace_id,
            clamped,
        )
        .await;
    }

    let cfg = state.db.get_blackboard_config(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("更新后查询黑板配置失败: {}", e))
    })?;
    Ok(ApiResponse::ok(cfg.unwrap()))
}

/// `GET /api/workspaces/{workspace_id}/wiki/files`
///
/// 获取 wiki 文件列表。
pub async fn list_wiki_files(
    State(_state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<Vec<WikiFileItem>>, AppError> {
    let mut items = Vec::new();

    // index.md
    items.push(WikiFileItem {
        slug: "index".to_string(),
        file_type: "index".to_string(),
    });

    // log.md
    items.push(WikiFileItem {
        slug: "log".to_string(),
        file_type: "log".to_string(),
    });

    // topics/*.md
    let topics = list_topics(workspace_id).map_err(|e| {
        AppError::Internal(format!("列出 topics 失败: {:?}", e))
    })?;

    for slug in topics {
        items.push(WikiFileItem {
            slug,
            file_type: "topic".to_string(),
        });
    }

    Ok(ApiResponse::ok(items))
}

/// `GET /api/workspaces/{workspace_id}/wiki/files/{slug}`
///
/// 获取 wiki 文件内容。
pub async fn get_wiki_file(
    State(_state): State<AppState>,
    Path((workspace_id, slug)): Path<(i64, String)>,
) -> Result<ApiResponse<WikiFileContent>, AppError> {
    let content = if slug == "index" {
        read_index(workspace_id).map_err(|e| {
            AppError::Internal(format!("读取 index 失败: {:?}", e))
        })?
    } else if slug == "log" {
        read_log(workspace_id).map_err(|e| {
            AppError::Internal(format!("读取 log 失败: {:?}", e))
        })?
    } else {
        read_topic(workspace_id, &slug).map_err(|e| {
            AppError::Internal(format!("读取 topic 失败: {:?}", e))
        })?
    };

    match content {
        Some(c) => Ok(ApiResponse::ok(WikiFileContent { slug, content: c })),
        None => Err(AppError::NotFound),
    }
}

/// 黑板 API 路由。
pub fn blackboard_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/workspaces/{workspace_id}/blackboard",
            get(get_blackboard).patch(update_blackboard_config),
        )
        .route(
            "/api/workspaces/{workspace_id}/blackboard/config",
            get(get_blackboard_config),
        )
        .route(
            "/api/workspaces/{workspace_id}/wiki/files",
            get(list_wiki_files),
        )
        .route(
            "/api/workspaces/{workspace_id}/wiki/files/{slug}",
            get(get_wiki_file),
        )
}