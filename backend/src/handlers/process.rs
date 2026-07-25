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

/// GET /api/v1/processes/stats — 工艺使用统计。
pub async fn get_process_stats(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    // 按模板聚合安装次数（loops.process_template_id GROUP BY）
    let sql = "SELECT pt.name, pt.display_name, pt.complexity, COUNT(l.id) AS loop_count
        FROM process_templates pt
        LEFT JOIN loops l ON l.process_template_id = pt.id
        GROUP BY pt.id ORDER BY loop_count DESC";
    let rows = state.db.conn.query_all(Statement::from_string(DbBackend::Sqlite, sql)).await?;
    let mut stats = Vec::new();
    for row in rows {
        let name: String = row.try_get_by("name").unwrap_or_default();
        let display_name: String = row.try_get_by("display_name").unwrap_or_default();
        let complexity: String = row.try_get_by("complexity").unwrap_or_default();
        let loop_count: i64 = row.try_get_by("loop_count").unwrap_or(0);
        stats.push(serde_json::json!({
            "name": name, "display_name": display_name, "complexity": complexity, "loop_count": loop_count
        }));
    }
    Ok(ApiResponse::ok(serde_json::json!({
        "template_stats": stats,
        "total_templates": stats.len()
    })))
}

/// POST /api/v1/processes/validate — 校验工艺定义 YAML。
#[derive(Debug, Deserialize)]
pub struct ValidateProcessRequest {
    pub definition: String,
}

pub async fn validate_process(
    State(_state): State<AppState>,
    Json(req): Json<ValidateProcessRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    match serde_yaml::from_str::<crate::services::process::ProcessDefinition>(&req.definition) {
        Ok(_def) => Ok(ApiResponse::ok(serde_json::json!({
            "valid": true,
            "errors": []
        }))),
        Err(e) => {
            // 提取行号信息（serde_yaml 错误消息含 line/column）。
            let msg = e.to_string();
            Ok(ApiResponse::ok(serde_json::json!({
                "valid": false,
                "errors": [msg]
            })))
        }
    }
}

/// POST /api/v1/processes/recommend — 工艺推荐。
pub async fn recommend_process(
    State(state): State<AppState>,
    Json(req): Json<crate::services::process::recommender::RecommendRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let templates = state.db.list_process_templates().await?;
    let result = crate::services::process::recommender::recommend(&templates, &req);
    Ok(ApiResponse::ok(result))
}

/// GET /api/workspaces/{ws}/loops/{id}/executions/{eid}/audit — 工艺实例审计链。
pub async fn get_loop_execution_audit(
    State(state): State<AppState>,
    Path((_ws_id, _loop_id, eid)): Path<(i64, i64, i64)>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let audit = state
        .db
        .get_loop_execution_audit(eid)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(axum::Json(crate::models::ApiResponse::ok(audit)))
}

/// POST .../gates/{gid}/approve 请求体。
#[derive(Debug, Deserialize)]
pub struct ApproveGateRequest {
    pub approved: bool,
    #[serde(default)]
    pub comment: Option<String>,
}

/// POST .../gates/{gid}/approve 响应体。
#[derive(Debug, Serialize)]
pub struct ApproveGateResponse {
    pub gate_id: i64,
    pub status: String,
}

/// 人工审批门禁：通过/拒绝。
pub async fn approve_gate(
    State(state): State<AppState>,
    Path((_ws_id, _loop_id, _eid, _seid, gid)): Path<(i64, i64, i64, i64, i64)>,
    Json(req): Json<ApproveGateRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let status = if req.approved { "passed" } else { "failed" };
    state
        .db
        .update_loop_step_execution_gate(gid, status, req.comment.as_deref(), Some("human"))
        .await?;
    Ok(ApiResponse::ok(ApproveGateResponse { gate_id: gid, status: status.to_string() }))
}

/// POST .../artifacts 请求体。
#[derive(Debug, Deserialize)]
pub struct AddArtifactRequest {
    pub name: String,
    pub artifact_type: String,
    pub locator: String,
    pub content_text: Option<String>,
}

/// 手动补充产物到指定环节执行记录。
pub async fn add_step_artifact(
    State(state): State<AppState>,
    Path((_ws_id, _loop_id, _eid, seid)): Path<(i64, i64, i64, i64)>,
    Json(req): Json<AddArtifactRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let model = state
        .db
        .create_loop_step_artifact(
            seid,
            &req.name,
            &req.artifact_type,
            &req.locator,
            req.content_text.as_deref(),
            Some("manual"),
        )
        .await?;
    Ok((axum::http::StatusCode::CREATED, ApiResponse::ok(model)))
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
        .route(
            "/api/v1/workspaces/{ws}/loops/{id}/executions/{eid}/audit",
            axum::routing::get(get_loop_execution_audit),
        )
        .route(
            "/api/v1/workspaces/{ws}/loops/{id}/executions/{eid}/steps/{seid}/gates/{gid}/approve",
            axum::routing::post(approve_gate),
        )
        .route(
            "/api/v1/workspaces/{ws}/loops/{id}/executions/{eid}/steps/{seid}/artifacts",
            axum::routing::post(add_step_artifact),
        )
        .route(
            "/api/v1/processes/recommend",
            axum::routing::post(recommend_process),
        )
        .route(
            "/api/v1/processes/stats",
            axum::routing::get(get_process_stats),
        )
        .route(
            "/api/v1/processes/validate",
            axum::routing::post(validate_process),
        )
}
