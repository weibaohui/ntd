//! 评审模板 HTTP handler。
//!
//! 端点（与 `db/review_template.rs` DAO 一一对应）：
//! - `GET    /api/review-templates?workspace=`   列出（含 prompt，可选按 workspace 过滤）
//! - `GET    /api/review-templates/options?workspace=`  轻量选项（可选按 workspace 过滤）
//! - `GET    /api/review-templates/{id}`     取单条
//! - `POST   /api/review-templates`          创建
//! - `PUT    /api/review-templates/{id}`     更新（PUT 全字段语义）
//! - `DELETE /api/review-templates/{id}`     删除
//!
//! 路由构建函数 `review_template_routes()` 在 `handlers/mod.rs` 内组装。

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Router;
use axum::Json;
use serde::Deserialize;

use crate::db::ReviewTemplateInput;
use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{
    ApiResponse, CreateReviewTemplateRequest, ReviewTemplate, ReviewTemplateOption,
    UpdateReviewTemplateRequest,
};

/// 查询参数：可选的 workspace_id 过滤。
#[derive(Debug, Default, Deserialize)]
pub struct ReviewTemplateQuery {
    pub workspace_id: Option<i64>,
}

/// 列表（完整模型，含 prompt）。
pub async fn list_review_templates(
    State(state): State<AppState>,
    Query(query): Query<ReviewTemplateQuery>,
) -> Result<Json<ApiResponse<Vec<ReviewTemplate>>>, AppError> {
    let templates = state.db.list_review_templates(query.workspace_id).await?;
    Ok(Json(ApiResponse::ok(templates)))
}

/// 选项列表（轻量，不含 prompt）。
pub async fn list_review_template_options(
    State(state): State<AppState>,
    Query(query): Query<ReviewTemplateQuery>,
) -> Result<Json<ApiResponse<Vec<ReviewTemplateOption>>>, AppError> {
    let opts = state.db.list_review_template_options(query.workspace_id).await?;
    Ok(Json(ApiResponse::ok(opts)))
}

/// 按 id 取单条。
pub async fn get_review_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<ReviewTemplate>>, AppError> {
    let t = state
        .db
        .get_review_template(id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(ApiResponse::ok(t)))
}

/// 创建模板。name 必填且非空，prompt 必填。
pub async fn create_review_template(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateReviewTemplateRequest>,
) -> Result<Json<ApiResponse<ReviewTemplate>>, AppError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".to_string()));
    }
    let prompt = req.prompt.trim();
    if prompt.is_empty() {
        return Err(AppError::BadRequest("prompt is required".to_string()));
    }
    let input = ReviewTemplateInput {
        name: name.to_string(),
        description: req.description.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        prompt: prompt.to_string(),
        workspace_id: req.workspace_id,
    };
    let id = state.db.create_review_template(&input).await?;
    let t = state
        .db
        .get_review_template(id)
        .await?
        .ok_or_else(|| AppError::Internal("failed to read created template".to_string()))?;
    Ok(Json(ApiResponse::ok(t)))
}

/// 更新模板（PUT 全字段语义）。name/prompt 必填且非空。
pub async fn update_review_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateReviewTemplateRequest>,
) -> Result<Json<ApiResponse<ReviewTemplate>>, AppError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".to_string()));
    }
    let prompt = req.prompt.trim();
    if prompt.is_empty() {
        return Err(AppError::BadRequest("prompt is required".to_string()));
    }
    let input = ReviewTemplateInput {
        name: name.to_string(),
        description: req.description.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        prompt: prompt.to_string(),
        workspace_id: None, // 更新时不修改 workspace_id
    };
    state.db.update_review_template(id, &input).await?;
    let t = state
        .db
        .get_review_template(id)
        .await?
        .ok_or_else(|| AppError::Internal("failed to read updated template".to_string()))?;
    Ok(Json(ApiResponse::ok(t)))
}

/// 删除模板。返回 204 No Content 语义由 Ok(()) 表达（包裹在 ApiResponse::ok 中）。
pub async fn delete_review_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<bool>>, AppError> {
    let deleted = state.db.delete_review_template(id).await?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(Json(ApiResponse::ok(true)))
}

/// v1 API 路由（/api/v1/review-templates/*）。
///
/// 与 `review_template_routes()`（mod.rs）并行，前缀全部改为 `/api/v1/`。
/// 等旧路由废弃后，`mount_domain_routes` 里从 `mod.rs` 的 `review_template_routes` 切到这个函数。
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        // 列表（含 prompt过滤） + 创建
        .route(
            "/api/v1/review-templates",
            get(list_review_templates).post(create_review_template),
        )
        // 选项列表（轻量，不含 prompt，必须在 {id} 之前注册以免被当成 id 捕获）
        .route(
            "/api/v1/review-templates/options",
            get(list_review_template_options),
        )
        // 按 id 取/更新/删除单条
        .route(
            "/api/v1/review-templates/{id}",
            get(get_review_template)
                .put(update_review_template)
                .delete(delete_review_template),
        )
}