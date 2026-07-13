//! 专家 API 路由处理函数
//!
//! 提供专家列表查询、详情查询、Agent MD 内容获取、头像资源访问等接口。

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Router;

use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;
use crate::expert::ExpertMetadata;

/// `GET /api/experts`：获取所有专家列表
///
/// 返回所有已加载的专家元数据，按分类分组。前端可用于专家选择面板。
pub async fn get_experts(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<ExpertMetadata>>, AppError> {
    let experts = state.expert_manager.get_all_experts();
    Ok(ApiResponse::ok(experts))
}

/// `GET /api/experts/:name`：获取单个专家详情
///
/// 返回指定名称的专家完整元数据。
pub async fn get_expert(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<ExpertMetadata>, AppError> {
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(expert))
}

/// `GET /api/experts/:name/agent-md`：获取专家的 Agent MD 内容
///
/// 根据专家类型自动定位：
/// - 单个专家：使用 agent_name 字段定位
/// - 专家团队：使用 lead_agent 字段定位
///
/// 返回完整的 MD 文件内容，用于执行时注入 prompt。
pub async fn get_expert_agent_md(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<String>, AppError> {
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;

    // 根据专家类型确定要加载的 Agent
    let target_agent = expert.lead_agent.or(expert.agent_name);
    let agent_name = target_agent.ok_or(AppError::NotFound)?;

    let md_content = state
        .expert_manager
        .get_agent_md_content(&agent_name)
        .map_err(|e| match e {
            crate::expert::ExpertError::AgentNotFound(_) => AppError::NotFound,
            _ => AppError::Internal("加载 Agent MD 内容失败".to_string()),
        })?;

    Ok(ApiResponse::ok(md_content))
}

/// `GET /api/experts/:name/skills`：获取专家关联的所有 Skill 元数据
///
/// 返回专家绑定的技能列表，用于前端展示可用技能。
pub async fn get_expert_skills(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<Vec<crate::expert::SkillMetadata>>, AppError> {
    let _ = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;

    let skills = state.expert_manager.get_expert_skills(&name);
    Ok(ApiResponse::ok(skills))
}

/// `GET /api/experts/:name/avatar`：获取专家头像
///
/// 根据专家的 avatar_path 字段定位头像文件并返回。
/// 如果头像不存在，返回 404。
pub async fn get_expert_avatar(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;

    let avatar_path = expert.avatar_path.ok_or(AppError::NotFound)?;
    let full_path = std::path::Path::new(&expert.definition_dir).join(avatar_path);

    if !full_path.exists() {
        return Err(AppError::NotFound);
    }

    let content = std::fs::read(&full_path).map_err(|e| AppError::Internal(format!("读取头像文件失败: {}", e)))?;

    // 根据文件扩展名推断 MIME 类型
    let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let mime_type = match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime_type)],
        content,
    ))
}

/// `POST /api/experts/reload`：重新加载所有专家定义
///
/// 清空现有索引，重新扫描 ~/.ntd/experts/ 目录加载专家定义。
/// 返回加载结果（成功数量和错误列表）。
pub async fn reload_experts(
    State(state): State<AppState>,
) -> Result<ApiResponse<crate::expert::LoadResult>, AppError> {
    if let Some(experts_dir) = crate::expert::experts_dir() {
        if experts_dir.exists() {
            state.expert_manager.clear();
            let load_result = crate::expert::load_experts_from_directory(&experts_dir, &state.expert_manager);
            Ok(ApiResponse::ok(load_result))
        } else {
            Ok(ApiResponse::err(400, "专家定义目录不存在"))
        }
    } else {
        Ok(ApiResponse::err(400, "无法获取 home 目录"))
    }
}

/// 专家 API 路由定义
pub fn expert_routes() -> axum::Router<AppState> {
    use axum::routing::{get, post};

    Router::new()
        .route("/api/experts", get(get_experts))
        .route("/api/experts/{name}", get(get_expert))
        .route("/api/experts/{name}/agent-md", get(get_expert_agent_md))
        .route("/api/experts/{name}/skills", get(get_expert_skills))
        .route("/api/experts/{name}/avatar", get(get_expert_avatar))
        .route("/api/experts/reload", post(reload_experts))
}
