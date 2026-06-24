use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;

/// 创建飞书项目绑定的请求体。
///
/// executor 和 todo_id 为互斥字段，不能同时提供：
/// - executor（执行器名）：创建新 Todo 时使用，仅支持继续对话的执行器（claudecode/kimi/opencode/mobilecoder/hermes/codewhale）。
///   为 None 时默认为 claudecode。
/// - todo_id（已有 Todo ID）：绑定到已有 Todo 时使用，复用该 Todo 的历史会话记录。
///   两者都传或都不传时行为由业务层决定（当前为互斥校验）。
#[derive(Debug, Deserialize)]
pub struct CreateBindingRequest {
    pub bot_id: i64,
    pub chat_id: String,
    pub chat_type: String,
    pub project_dir_id: i64,
    /// 指定执行器（可选），仅在新建 Todo 时有效。
    /// 仅支持继续对话的执行器（claudecode/kimi/opencode/mobilecoder/hermes/codewhale）。
    /// 为 None 或空串时默认为 claudecode。
    pub executor: Option<String>,
    /// 绑定到已有 Todo（可选），与 executor 互斥。
    /// 提供后复用该 Todo 及其历史会话，不提供则新建 Todo。
    pub todo_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct BindingResponse {
    pub id: i64,
    pub bot_id: i64,
    pub chat_id: String,
    pub chat_type: String,
    pub project_dir_id: i64,
    pub todo_id: i64,
    pub session_id: Option<String>,
    pub latest_record_id: Option<i64>,
    pub status: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    /// Resolved from project_dir_id
    pub project_name: Option<String>,
    pub project_path: Option<String>,
}

/// GET /api/feishu/bindings?bot_id=1
pub async fn list_bindings(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListBindingsQuery>,
) -> Result<Json<ApiResponse<Vec<BindingResponse>>>, AppError> {
    let bindings = if let Some(bot_id) = query.bot_id {
        state.db.get_feishu_project_bindings(bot_id).await?
    } else {
        state.db.get_all_feishu_project_bindings().await?
    };

    // 批量加载所有项目目录到 HashMap，避免 N+1 查询
    let dirs: std::collections::HashMap<i64, (Option<String>, Option<String>)> = state.db
        .get_project_directories()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|d| (d.id, (d.name, Some(d.path))))
        .collect();

    let mut results = Vec::new();
    for b in bindings {
        let (project_name, project_path) = dirs
            .get(&b.project_dir_id)
            .cloned()
            .unwrap_or((None, None));

        results.push(BindingResponse {
            id: b.id,
            bot_id: b.bot_id,
            chat_id: b.chat_id,
            chat_type: b.chat_type,
            project_dir_id: b.project_dir_id,
            todo_id: b.todo_id,
            session_id: b.session_id,
            latest_record_id: b.latest_record_id,
            status: b.status,
            enabled: b.enabled,
            created_at: b.created_at,
            updated_at: b.updated_at,
            project_name,
            project_path,
        });
    }

    Ok(Json(ApiResponse::ok(results)))
}

#[derive(Debug, Deserialize)]
pub struct ListBindingsQuery {
    pub bot_id: Option<i64>,
}

/// POST /api/feishu/bindings
pub async fn create_binding(
    State(state): State<AppState>,
    Json(req): Json<CreateBindingRequest>,
) -> Result<Json<ApiResponse<BindingResponse>>, AppError> {
    // 验证项目目录存在
    let dir = state.db
        .get_project_directory_by_id(req.project_dir_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Project directory not found".to_string()))?;

    // executor 与 todo_id 互斥：提供 executor 表示新建 Todo，提供 todo_id 表示绑定已有 Todo。
    // 后者要求 executor 必须为 None（否则可能两者都传，产生歧义）。
    if req.executor.is_some() && req.todo_id.is_some() {
        return Err(AppError::BadRequest("executor 和 todo_id 互斥，不能同时提供".to_string()));
    }

    // 校验 executor 是否在允许列表中（仅新建 Todo 时需要校验）
    // 使用 parse_executor_type 做大小写不敏感 + 别名解析，再检查是否支持继续对话。
    if let Some(ref exec) = req.executor {
        let parsed = crate::adapters::parse_executor_type(exec)
            .ok_or_else(|| AppError::BadRequest(format!("不支持的执行器: {exec}")))?;
        if !crate::adapters::RESUMABLE_EXECUTORS.contains(&parsed.as_str()) {
            return Err(AppError::BadRequest(format!("执行器 {exec} 不支持继续对话")));
        }
    }

    // Step 1: 确定 todo_id — 有指定则绑定到已有 Todo，否则新建
    let todo_id = if let Some(tid) = req.todo_id {
        // 绑定到已有 Todo：验证存在并检查 workspace 一致性。
        // 若 Todo 已有 workspace 且与目标目录不一致，说明用户可能误选，强制覆写会导致历史上下文错位，
        // 所以先警告式更新（而非静默覆盖），让用户知道旧上下文已脱离。
        let existing = state.db.get_todo(tid).await?
            .ok_or_else(|| AppError::BadRequest("指定的 Todo 不存在".to_string()))?;

        let workspace_changed = existing.workspace.as_ref().map(|w| w.as_str()) != Some(&dir.path);
        if workspace_changed && existing.workspace.is_some() {
            tracing::warn!(
                "[binding] binding todo {} to a different workspace (was: {:?}, now: {}), session history may be misaligned",
                tid, existing.workspace, dir.path
            );
        }

        if let Err(e) = state.db.update_todo_workspace(tid, Some(&dir.path)).await {
            tracing::warn!("[binding] failed to update todo {} workspace: {e}", tid);
        }
        tid
    } else {
        // 新建 Todo，title/prompt 模板与 feishu_listener.rs 保持一致。
        let todo_title = format!("飞书-{}", dir.name.as_deref().unwrap_or(&dir.path));
        let todo_prompt = format!(
            "你是飞书Bot的AI助手，正在项目「{name}」({path})中工作。\n\
             用户通过飞书与你交流，请根据用户的需求在项目目录中完成开发任务。\n\
             你可以读取、修改项目文件，运行命令等。\n\n\
             用户诉求：{{message}}\n\
             项目目录：{path}",
            name = dir.name.as_deref().unwrap_or("unknown"),
            path = dir.path,
        );
        let new_todo_id = state.db.create_todo_with_executor(
            &todo_title,
            &todo_prompt,
            req.executor.as_deref().filter(|s| !s.is_empty()),
        ).await?;
        if let Err(e) = state.db.update_todo_workspace(new_todo_id, Some(&dir.path)).await {
            tracing::warn!("[binding] failed to set todo workspace: {e}");
        }
        new_todo_id
    };

    // Step 2: 在事务中执行删除旧 binding + 创建新 binding，
    // 避免 create 失败时留下不一致状态（旧 binding 已删但新 binding 未建）。
    use sea_orm::TransactionTrait;
    let binding = state.db.conn.begin().await?;
    // 删除旧 binding 失败时记录警告但继续（可能本来就没有旧 binding）
    if let Err(e) = state.db
        .delete_feishu_project_binding_by_chat(req.bot_id, &req.chat_id)
        .await
    {
        tracing::warn!("[binding] failed to delete old binding for bot {} chat {}: {e}", req.bot_id, req.chat_id);
    }
    let _binding_id = state.db
        .create_feishu_project_binding(
            req.bot_id,
            &req.chat_id,
            &req.chat_type,
            req.project_dir_id,
            todo_id,
        )
        .await?;
    binding.commit().await?;

    let binding = state.db.get_feishu_project_binding(req.bot_id, &req.chat_id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to retrieve created binding".to_string()))?;

    Ok(Json(ApiResponse::ok(BindingResponse {
        id: binding.id,
        bot_id: binding.bot_id,
        chat_id: binding.chat_id,
        chat_type: binding.chat_type,
        project_dir_id: binding.project_dir_id,
        todo_id: binding.todo_id,
        session_id: binding.session_id,
        latest_record_id: binding.latest_record_id,
        status: binding.status,
        enabled: binding.enabled,
        created_at: binding.created_at,
        updated_at: binding.updated_at,
        project_name: dir.name,
        project_path: Some(dir.path),
    })))
}

/// DELETE /api/feishu/bindings/{id}
pub async fn delete_binding(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    state.db.delete_feishu_project_binding(id).await?;
    Ok(Json(ApiResponse::ok(())))
}

/// DELETE /api/feishu/bindings/by-chat?bot_id=1&chat_id=xxx
pub async fn delete_binding_by_chat(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<DeleteByChatQuery>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    state.db
        .delete_feishu_project_binding_by_chat(query.bot_id, &query.chat_id)
        .await?;
    Ok(Json(ApiResponse::ok(())))
}

#[derive(Debug, Deserialize)]
pub struct DeleteByChatQuery {
    pub bot_id: i64,
    pub chat_id: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBindingEnabledRequest {
    pub enabled: bool,
}

/// PATCH /api/feishu/bindings/{id}/enabled
pub async fn update_binding_enabled(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateBindingEnabledRequest>,
) -> Result<Json<ApiResponse<BindingResponse>>, AppError> {
    state.db
        .update_feishu_project_binding_enabled(id, req.enabled)
        .await?;

    let binding = state.db
        .get_feishu_project_binding_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Load project directory info for response
    let (project_name, project_path) = state.db
        .get_project_directory_by_id(binding.project_dir_id)
        .await
        .ok()
        .flatten()
        .map(|d| (d.name, Some(d.path)))
        .unwrap_or((None, None));

    Ok(Json(ApiResponse::ok(BindingResponse {
        id: binding.id,
        bot_id: binding.bot_id,
        chat_id: binding.chat_id,
        chat_type: binding.chat_type,
        project_dir_id: binding.project_dir_id,
        todo_id: binding.todo_id,
        session_id: binding.session_id,
        latest_record_id: binding.latest_record_id,
        status: binding.status,
        enabled: binding.enabled,
        created_at: binding.created_at,
        updated_at: binding.updated_at,
        project_name,
        project_path,
    })))
}
