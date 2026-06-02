use axum::extract::State;
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::{Deserialize, Serialize};

use super::{ApiJson, AppState};
use crate::models::ApiResponse;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectDirectoryRequest {
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProjectDirectoryRequest {
    pub name: Option<String>,
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
    if req.path.trim().is_empty() {
        return Ok(ApiResponse::err(crate::models::codes::BAD_REQUEST, "Path is required"));
    }
    let directory = state
        .db
        .get_or_create_project_directory(&req.path)
        .await?;
    Ok(ApiResponse::ok(directory))
}

pub async fn update_project_directory(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    ApiJson(req): ApiJson<UpdateProjectDirectoryRequest>,
) -> Result<ApiResponse<()>, super::AppError> {
    state
        .db
        .update_project_directory(id, req.name.as_deref())
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