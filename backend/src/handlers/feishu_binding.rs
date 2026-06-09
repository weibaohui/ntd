use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;

#[derive(Debug, Deserialize)]
pub struct CreateBindingRequest {
    pub bot_id: i64,
    pub chat_id: String,
    pub chat_type: String,
    pub project_dir_id: i64,
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

    let mut results = Vec::new();
    for b in bindings {
        let (project_name, project_path) = state.db
            .get_project_directory_by_id(b.project_dir_id)
            .await
            .ok()
            .flatten()
            .map(|d| (d.name, Some(d.path)))
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
    // Verify project directory exists
    let dir = state.db
        .get_project_directory_by_id(req.project_dir_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Project directory not found".to_string()))?;

    // Step 1: Create Todo first — if this fails, nothing is lost.
    let todo_title = format!("飞书-{}", dir.name.as_deref().unwrap_or(&dir.path));
    let todo_prompt = format!(
        "你是飞书Bot的AI助手，正在项目「{name}」({path})中工作。\n\
         用户通过飞书与你交流，请根据用户的需求在项目目录中完成开发任务。\n\
         你可以读取、修改项目文件，运行命令等。\n\n\
         项目目录：{path}",
        name = dir.name.as_deref().unwrap_or("unknown"),
        path = dir.path,
    );

    let todo_id = state.db.create_todo(&todo_title, &todo_prompt).await?;

    // Step 2: Update workspace/worktree — warn on failure but don't abort
    if let Err(e) = state.db.update_todo_workspace(todo_id, Some(&dir.path)).await {
        tracing::warn!("[binding] failed to set todo workspace: {e}");
    }
    if let Err(e) = state.db.update_todo_worktree_enabled(todo_id, true).await {
        tracing::warn!("[binding] failed to set worktree_enabled: {e}");
    }

    // Step 3: Delete old binding + create new binding (close together to minimize window)
    let _ = state.db
        .delete_feishu_project_binding_by_chat(req.bot_id, &req.chat_id)
        .await;

    let binding_id = state.db
        .create_feishu_project_binding(
            req.bot_id,
            &req.chat_id,
            &req.chat_type,
            req.project_dir_id,
            todo_id,
        )
        .await?;

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
