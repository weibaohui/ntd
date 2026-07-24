//! 工艺模板 API 处理器。
//!
//! 提供工艺市场列表、详情查看、安装为 Loop 三个核心接口。

use axum::extract::{Path, State};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;
use crate::services::process::installer::install_process_template;

/// 工艺模板列表项。
#[derive(Debug, Serialize)]
pub struct ProcessTemplateListItem {
    pub id: i64,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub complexity: String,
    pub version: String,
    pub source_path: Option<String>,
    pub is_system: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// 工艺模板详情。
#[derive(Debug, Serialize)]
pub struct ProcessTemplateDetail {
    #[serde(flatten)]
    pub item: ProcessTemplateListItem,
    /// 原始 YAML/JSON 定义文本
    pub definition: String,
}

/// 安装工艺模板请求。
#[derive(Debug, Deserialize)]
pub struct InstallProcessRequest {
    pub workspace_id: i64,
}

/// 安装工艺模板响应。
#[derive(Debug, Serialize)]
pub struct InstallProcessResponse {
    pub loop_id: i64,
    pub loop_name: String,
    pub phase_count: usize,
    pub step_count: usize,
}

/// GET /api/bundled/processes — 列出所有工艺模板。
pub async fn list_process_templates(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let templates = state.db.list_process_templates().await?;
    let items: Vec<ProcessTemplateListItem> = templates
        .into_iter()
        .map(|t| ProcessTemplateListItem {
            id: t.id,
            name: t.name,
            display_name: t.display_name,
            description: t.description,
            category: t.category,
            complexity: t.complexity,
            version: t.version,
            source_path: t.source_path,
            is_system: t.is_system,
            created_at: t.created_at,
            updated_at: t.updated_at,
        })
        .collect();
    Ok(ApiResponse::ok(items))
}

/// GET /api/bundled/processes/{name} — 查看单个工艺模板详情。
pub async fn get_process_template(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let template = state
        .db
        .get_process_template_by_name(&name)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(ProcessTemplateDetail {
        item: ProcessTemplateListItem {
            id: template.id,
            name: template.name.clone(),
            display_name: template.display_name,
            description: template.description,
            category: template.category,
            complexity: template.complexity,
            version: template.version,
            source_path: template.source_path,
            is_system: template.is_system,
            created_at: template.created_at,
            updated_at: template.updated_at,
        },
        definition: template.definition,
    }))
}

/// POST /api/bundled/processes/{name}/install — 安装工艺模板为 Loop。
pub async fn install_process(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<InstallProcessRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let template = state
        .db
        .get_process_template_by_name(&name)
        .await?
        .ok_or(AppError::NotFound)?;

    let workspace = state
        .db
        .get_project_directory_by_id(req.workspace_id)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id))
        })?;

    let result = install_process_template(
        &state.db,
        &template,
        req.workspace_id,
        &workspace.path,
    )
    .await
    .map_err(|e| {
        tracing::error!("安装工艺模板 {} 失败: {}", name, e);
        AppError::Internal(e.to_string())
    })?;

    Ok((
        axum::http::StatusCode::CREATED,
        ApiResponse::ok(InstallProcessResponse {
            loop_id: result.loop_id,
            loop_name: result.loop_name,
            phase_count: result.phase_count,
            step_count: result.step_count,
        }),
    ))
}

/// 工艺模板相关路由（/api/bundled/processes）。
pub fn process_routes() -> Router<AppState> {
    Router::new()
        .route("/api/bundled/processes", axum::routing::get(list_process_templates))
        .route("/api/bundled/processes/{name}", axum::routing::get(get_process_template))
        .route(
            "/api/bundled/processes/{name}/install",
            axum::routing::post(install_process),
        )
}

/// 工艺模板相关路由（/api/v1/bundled/processes）。
pub fn v1_process_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/bundled/processes", axum::routing::get(list_process_templates))
        .route("/api/v1/bundled/processes/{name}", axum::routing::get(get_process_template))
        .route(
            "/api/v1/bundled/processes/{name}/install",
            axum::routing::post(install_process),
        )
}
