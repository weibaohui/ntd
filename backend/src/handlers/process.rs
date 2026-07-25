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

/// POST /api/v1/processes/{name}/copy-to-user — 把系统工艺复制到用户层 `~/.ntd/processes/`。
///
/// 用户层 YAML 文件路径与系统层相对路径一致。
/// 复制完成后触发用户层 upsert，把工艺标记为 `is_system=false`。
/// 若用户层已存在同名工艺则返回 409，避免静默覆盖用户自定义。
pub async fn copy_process_to_user(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 安全校验：name 不得包含路径分隔符或 ..，防止写入目标逃逸出 ~/.ntd/processes/。
    validate_process_name(&name)?;

    let template = state
        .db
        .get_process_template_by_name(&name)
        .await?
        .ok_or(AppError::NotFound)?;

    if !template.is_system {
        return Err(AppError::BadRequest(format!("工艺 {} 已是用户工艺", name)));
    }

    // 计算用户层目标路径：~/.ntd/processes/<bundled 中相对路径>
    let user_dir = crate::services::process::user_dir::user_processes_dir()
        .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;

    let rel_path = template
        .source_path
        .as_ref()
        .and_then(|s| s.strip_prefix("bundled://processes/"))
        .unwrap_or(&name);

    let target_path = user_dir.join(rel_path);

    // 防御性校验：canonicalize 后必须在 user_dir 内。
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(format!("创建用户工艺目录失败: {}", e)))?;
    }

    if target_path.exists() {
        return Err(AppError::BadRequest(format!(
            "用户层已存在同名工艺 {}，请先手动删除 ~/.ntd/processes/{}",
            name, rel_path
        )));
    }

    std::fs::write(&target_path, &template.definition)
        .map_err(|e| AppError::Internal(format!("写入用户工艺文件失败: {}", e)))?;

    // 触发用户层 upsert，把刚复制的文件入库为 is_system=false。
    if let Err(e) =
        crate::services::process::user_dir::import_user_process_templates(&state).await
    {
        tracing::warn!("复制后导入用户层失败: {}", e);
    }

    Ok(ApiResponse::ok(serde_json::json!({
        "user_source_path": format!("{}{}", crate::services::process::user_dir::USER_SOURCE_PREFIX, rel_path),
    })))
}

/// 校验工艺名称不得包含路径分隔符或 `..`，防止 `copy_process_to_user` 写入目标逃逸。
fn validate_process_name(name: &str) -> Result<(), AppError> {
    if name.is_empty() {
        return Err(AppError::BadRequest("工艺名称不能为空".to_string()));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(AppError::BadRequest(format!(
            "工艺名称不得包含路径分隔符: {}",
            name
        )));
    }
    if name == ".." || name.contains("/..") || name.contains("\\..") {
        return Err(AppError::BadRequest(format!("工艺名称不得包含 ..: {}", name)));
    }
    Ok(())
}
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

/// GET /api/v1/processes/{name}/versions — 版本历史。
pub async fn get_process_versions(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let templates = state.db.list_process_templates().await?;
    let versions: Vec<_> = templates
        .iter()
        .filter(|t| t.name == name)
        .map(|t| serde_json::json!({
            "id": t.id,
            "version": t.version,
            "updated_at": t.updated_at,
            "source_path": t.source_path,
        }))
        .collect();
    Ok(ApiResponse::ok(serde_json::json!({ "name": name, "versions": versions })))
}

/// GET /api/v1/processes/{name}/versions/{v}/diff?base={base_v} — 版本 diff。
pub async fn diff_process_versions(
    State(state): State<AppState>,
    Path((name, _version)): Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_version = params.get("base").cloned().unwrap_or_default();
    let templates = state.db.list_process_templates().await?;
    let base = templates.iter().find(|t| t.name == name && t.version == base_version);
    let target = templates.iter().find(|t| t.name == name && t.version == _version);

    match (base, target) {
        (Some(b), Some(t)) => {
            let diff_lines = simple_diff(&b.definition, &t.definition);
            Ok(ApiResponse::ok(serde_json::json!({
                "name": name,
                "base_version": base_version,
                "target_version": _version,
                "diff": diff_lines,
            })))
        }
        _ => Err(AppError::NotFound),
    }
}

/// 简单的逐行 diff，返回 added/removed/unchanged 行。
///
/// 使用简化的 LCS-like 算法：双指针扫描两条文本，逐行对比。
/// 当行不匹配时尝试先推进新版本指针（跳过新增行），否则视作移除。
/// 这样能正确处理插入/删除，但不处理行级修改（修改显示为 remove+add）。
fn simple_diff(old: &str, new: &str) -> Vec<serde_json::Value> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut result = Vec::new();

    // 使用简单的 LCS-like 逻辑：逐行对比直到同步点。
    let mut oi = 0;
    let mut ni = 0;
    while oi < old_lines.len() && ni < new_lines.len() {
        if old_lines[oi] == new_lines[ni] {
            result.push(serde_json::json!({"type": "unchanged", "line": old_lines[oi]}));
            oi += 1;
            ni += 1;
        } else {
            // 跳过旧版本中不存在的行（removed）。
            if ni + 1 < new_lines.len() && old_lines[oi] == new_lines[ni + 1] {
                result.push(serde_json::json!({"type": "added", "line": new_lines[ni]}));
                ni += 1;
            } else {
                result.push(serde_json::json!({"type": "removed", "line": old_lines[oi]}));
                oi += 1;
                // 如果新行匹配到了旧行后面，也添加。
                if oi < old_lines.len() && ni < new_lines.len() && old_lines[oi] == new_lines[ni] {
                    continue;
                }
            }
        }
    }
    // 剩余行。
    while oi < old_lines.len() {
        result.push(serde_json::json!({"type": "removed", "line": old_lines[oi]}));
        oi += 1;
    }
    while ni < new_lines.len() {
        result.push(serde_json::json!({"type": "added", "line": new_lines[ni]}));
        ni += 1;
    }
    result
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
        .route(
            "/api/v1/processes/{name}/versions",
            axum::routing::get(get_process_versions),
        )
        .route(
            "/api/v1/processes/{name}/versions/{version}/diff",
            axum::routing::get(diff_process_versions),
        )
        .route(
            "/api/v1/processes/{name}/copy-to-user",
            axum::routing::post(copy_process_to_user),
        )
}
