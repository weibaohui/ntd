//! 黑板（Blackboard）API Handler。
//!
//! 提供两个端点：
//! - `GET /api/workspaces/{workspace_id}/blackboard`：获取当前黑板内容
//! - `POST /api/workspaces/{workspace_id}/blackboard/refresh`：手动触发热刷新

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::Router;

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::ApiResponse;

/// 黑板响应体
#[derive(Debug, serde::Serialize)]
pub struct BlackboardResponse {
    pub id: i64,
    pub workspace_id: i64,
    pub content: String,
    pub updated_at: Option<String>,
}

/// `GET /api/workspaces/{workspace_id}/blackboard`
///
/// 获取指定工作空间的当前黑板内容。
/// 如果该工作空间还没有黑板记录，返回空内容（content=""）。
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
            content: model.content,
            updated_at: model.updated_at,
        })),
        None => Ok(ApiResponse::ok(BlackboardResponse {
            id: 0,
            workspace_id,
            content: String::new(),
            updated_at: None,
        })),
    }
}

/// 刷新请求体（预留，当前为空）
#[derive(Debug, serde::Deserialize)]
pub struct RefreshRequest {}

/// 刷新响应体
#[derive(Debug, serde::Serialize)]
pub struct RefreshResponse {
    pub success: bool,
    pub message: String,
}

/// `POST /api/workspaces/{workspace_id}/blackboard/refresh`
///
/// 手动触发黑板刷新：重新执行 blackboard update todo，
/// 让 LLM 根据当前黑板内容重新组织生成。
///
/// 这是一个异步操作，请求返回后黑板内容会在几秒到几十秒后更新。
/// 前端需要间隔轮询或等待 WebSocket 推送来获取最新内容。
pub async fn refresh_blackboard(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<RefreshResponse>, AppError> {
    // 异步触发黑板刷新，不阻塞请求
    // tokio::spawn 确保刷新任务在后台运行，即使 HTTP 连接已断开
    let db = state.db.clone();
    let executor_registry = state.executor_registry.clone();
    let tx = state.tx.clone();
    let task_manager = state.task_manager.clone();
    let config = state.config.clone();

    tokio::spawn(async move {
        if let Err(e) = crate::services::blackboard::refresh_blackboard(
            db,
            executor_registry,
            tx,
            task_manager,
            config,
            workspace_id,
        )
        .await
        {
            tracing::warn!("黑板刷新失败: workspace_id={}, error={:?}", workspace_id, e);
        }
    });

    Ok(ApiResponse::ok(RefreshResponse {
        success: true,
        message: "黑板刷新已触发".to_string(),
    }))
}

/// 返回黑板领域路由。
///
/// 路由设计遵循 RESTful 风格，以 workspace 为作用域：
/// - `/api/workspaces/{workspace_id}/blackboard`
pub fn blackboard_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/workspaces/{workspace_id}/blackboard",
            get(get_blackboard),
        )
        .route(
            "/api/workspaces/{workspace_id}/blackboard/refresh",
            post(refresh_blackboard),
        )
}
