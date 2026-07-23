use axum::{
    Router,
    extract::{Path, State},
    routing::{delete, get},
};

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, CreateTagRequest, Tag, utc_timestamp};

pub async fn get_tags(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<Tag>>, AppError> {
    Ok(ApiResponse::ok(state.db.get_tags().await?))
}

pub async fn create_tag(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateTagRequest>,
) -> Result<ApiResponse<Tag>, AppError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Tag name is required".to_string()));
    }
    let now = utc_timestamp();
    let id = state.db.create_tag(name, &req.color).await?;
    Ok(ApiResponse::ok(Tag {
        id,
        name: name.to_string(),
        color: req.color,
        created_at: now,
    }))
}

pub async fn delete_tag(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<()>, AppError> {
    state.db.delete_tag(id).await?;
    Ok(ApiResponse::ok(()))
}

/// v1 API 路由：标签为全局资源（tags 表无 workspace_id 列），嵌套在 `/api/v1/tags` 下。
/// 所有路径都是相对路径（`/`、`/{id}`），由 action.rs 的 `.nest("/api/v1/tags", v1_routes())` 提供前缀。
/// handler 不提取 workspace_id（全局资源）。
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_tags).post(create_tag))
        .route("/{id}", delete(delete_tag))
}
