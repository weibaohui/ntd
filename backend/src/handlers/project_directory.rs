use axum::extract::State;
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::{Deserialize, Serialize};

use super::{ApiJson, AppState};
use crate::models::ApiResponse;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectDirectoryRequest {
    pub path: String,
    // 项目名称必填，Todo 选择目录时按名称展示
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateProjectDirectoryRequest {
    /// 修改后的名称（必填）。name="" 由 handler 拒绝。
    pub name: String,
    /// issue #643: 是否在该目录下执行 Todo 时由 ntd 托管 git worktree。
    /// `None` 表示不修改；`Some(bool)` 表示更新。
    /// 缺省走 `#[serde(default)]`，兼容老客户端只发 `{name}` 的请求。
    #[serde(default)]
    pub git_worktree_enabled: Option<bool>,
    /// issue #643: 执行结束（成功/失败/取消）后是否自动清理 worktree。
    /// 语义同上，None=不修改，Some=更新。
    #[serde(default)]
    pub auto_cleanup: Option<bool>,
}

pub async fn list_project_directories(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<crate::db::project_directory::ProjectDirectory>>, super::AppError> {
    let directories = state.db.get_project_directories().await?;
    Ok(ApiResponse::ok(directories))
}

pub async fn create_project_directory(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateProjectDirectoryRequest>,
) -> Result<ApiResponse<crate::db::project_directory::ProjectDirectory>, super::AppError> {
    // 路径和名称都必填：项目目录是用户按"项目"维度组织 Todo 的核心维度，
    // 缺一项就无法在 Todo 侧按名称识别目录。
    let path = req.path.trim();
    if path.is_empty() {
        return Ok(ApiResponse::err(crate::models::codes::BAD_REQUEST, "Path is required"));
    }
    let name = req.name.trim();
    if name.is_empty() {
        return Ok(ApiResponse::err(crate::models::codes::BAD_REQUEST, "Name is required"));
    }
    let directory = state
        .db
        .get_or_create_project_directory(&path, Some(name))
        .await?;
    Ok(ApiResponse::ok(directory))
}

pub async fn update_project_directory(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    ApiJson(req): ApiJson<UpdateProjectDirectoryRequest>,
) -> Result<ApiResponse<()>, super::AppError> {
    // 更新也要求名称非空，与新增保持一致约束，避免出现"无名项目"的历史脏数据
    let name = req.name.trim();
    if name.is_empty() {
        return Ok(ApiResponse::err(crate::models::codes::BAD_REQUEST, "Name is required"));
    }
    // issue #643: 即使前端没传 worktree 字段也允许请求继续，老客户端 PUT 不会 400。
    state
        .db
        .update_project_directory(id, Some(name), req.git_worktree_enabled, req.auto_cleanup)
        .await?;
    Ok(ApiResponse::ok(()))
}

pub async fn delete_project_directory(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<ApiResponse<()>, super::AppError> {
    state.db.delete_project_directory(id).await?;
    Ok(ApiResponse::ok(()))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/project-directories", get(list_project_directories))
        .route("/api/project-directories", post(create_project_directory))
        .route("/api/project-directories/{id}", put(update_project_directory))
        .route("/api/project-directories/{id}", delete(delete_project_directory))
}