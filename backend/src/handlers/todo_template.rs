use axum::extract::{Path, State};
use axum::Json;

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, CreateTemplateRequest, TodoTemplate, UpdateTemplateRequest};

pub async fn get_templates(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<TodoTemplate>>>, AppError> {
    let templates = state.db.get_templates().await?;
    Ok(Json(ApiResponse::ok(templates)))
}

pub async fn create_template(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateTemplateRequest>,
) -> Result<Json<ApiResponse<TodoTemplate>>, AppError> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("Title is required".to_string()));
    }

    let category = req.category.trim();
    if category.is_empty() {
        return Err(AppError::BadRequest("Category is required".to_string()));
    }

    let id = state.db
        .create_template(crate::db::TemplateInput { title, prompt: req.prompt.as_deref(), category, sort_order: req.sort_order }, false)
        .await?;

    let template = state.db
        .get_template_by_id(id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to get created template".to_string()))?;

    Ok(Json(ApiResponse::ok(template)))
}

pub async fn update_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateTemplateRequest>,
) -> Result<Json<ApiResponse<TodoTemplate>>, AppError> {
    // AppError::NotFound 是单元变体，不捕获变量——用 ok_or 直接构造更简洁
    let existing = state.db
        .get_template_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    // System templates cannot be modified
    if existing.is_system {
        return Err(AppError::BadRequest("Cannot modify system template".to_string()));
    }

    let title = req.title.unwrap_or_else(|| existing.title.clone());
    let prompt = req.prompt.or(existing.prompt);
    let category = req.category.unwrap_or_else(|| existing.category.clone());
    let sort_order = req.sort_order.or(Some(existing.sort_order));

    state.db
        .update_template(id, crate::db::TemplateInput { title: &title, prompt: prompt.as_deref(), category: &category, sort_order })
        .await?;

    let template = state.db
        .get_template_by_id(id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to get updated template".to_string()))?;

    Ok(Json(ApiResponse::ok(template)))
}

pub async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    // AppError::NotFound 是单元变体，不捕获变量——用 ok_or 直接构造更简洁
    let existing = state.db
        .get_template_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    // System templates cannot be deleted
    if existing.is_system {
        return Err(AppError::BadRequest("Cannot delete system template".to_string()));
    }

    state.db.delete_template(id).await?;
    Ok(Json(ApiResponse::ok(())))
}

pub async fn copy_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<TodoTemplate>>, AppError> {
    // AppError::NotFound 是单元变体，不捕获变量——用 ok_or 直接构造更简洁
    let existing = state.db
        .get_template_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Create a copy as user template
    let new_title = format!("{} (副本)", existing.title);
    let id = state.db
        .create_template(crate::db::TemplateInput { title: &new_title, prompt: existing.prompt.as_deref(), category: &existing.category, sort_order: Some(existing.sort_order) }, false)
        .await?;

    let template = state.db
        .get_template_by_id(id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to get copied template".to_string()))?;

    Ok(Json(ApiResponse::ok(template)))
}