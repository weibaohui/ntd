//! 任务管理 API。
use axum::extract::{Path, Query, State};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;
use crate::services::process::installer::install_process_template;

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub requirement: String,
    pub workspace_id: i64,
    pub template_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub status: String,
    pub workspace_id: Option<i64>,
    pub template_id: Option<i64>,
    pub loop_id: Option<i64>,
    pub template_name: Option<String>,
    pub complexity: Option<String>,
    pub latest_execution_status: Option<String>,
    pub latest_execution_requirement: Option<String>,
    pub created_at: Option<String>,
}

/// POST /api/v1/tasks
pub async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 1. 选模板。
    let template_name = if let Some(ref name) = req.template_name { name.clone() } else {
        let templates = state.db.list_process_templates().await?;
        let rec = crate::services::process::recommender::recommend(&templates,
            &crate::services::process::recommender::RecommendRequest { description: req.requirement.clone() });
        rec.recommendations.first().map(|r| r.template_name.clone())
            .ok_or_else(|| AppError::BadRequest("无可推荐的工艺模板".into()))?
    };
    let template = state.db.get_process_template_by_name(&template_name).await?
        .ok_or_else(|| AppError::BadRequest(format!("模板 {} 不存在", template_name)))?;
    let ws = state.db.get_project_directory_by_id(req.workspace_id).await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id)))?;
    // 2. 创建 task。
    let title = if req.requirement.len() > 60 { format!("{}…", &req.requirement[..60]) } else { req.requirement.clone() };
    let task = state.db.create_task(&title, req.workspace_id, template.id, None).await?;
    // 3. 复用或创建 Loop。
    let loop_id = if let Some(existing) = state.db.find_task_loop(template.id, req.workspace_id).await? {
        existing
    } else {
        let result = install_process_template(&state.db, &template, req.workspace_id, &ws.path)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        result.loop_id
    };
    state.db.update_task_loop_id(task.id, loop_id).await?;
    // 4. 创建执行。
    let meta = serde_json::json!({"requirement": req.requirement}).to_string();
    let exec = state.db.create_loop_execution_with_task(loop_id, task.id, "manual", &meta, 0).await?;
    Ok((axum::http::StatusCode::CREATED, ApiResponse::ok(serde_json::json!({
        "task_id": task.id, "loop_id": loop_id, "execution_id": exec.id, "template_name": template_name,
    }))))
}

/// GET /api/v1/tasks
pub async fn list_tasks(
    State(state): State<AppState>,
    Query(q): Query<ListTasksQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let tasks = state.db.list_tasks(q.status.as_deref()).await?;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let items: Vec<TaskItem> = {
        let mut result = Vec::new();
        for t in tasks {
            let pt = if let Some(tid) = t.template_id { state.db.get_process_template_by_id(tid).await? } else { None };
            let exec_sql = format!(
                "SELECT le.status, le.trigger_meta FROM loop_executions le WHERE le.task_id={} ORDER BY le.started_at DESC LIMIT 1", t.id);
            let exec = state.db.conn.query_all(Statement::from_string(DbBackend::Sqlite, exec_sql)).await.ok();
            let (exec_status, exec_req) = exec.and_then(|rows| rows.first().map(|r| {
                let s = r.try_get_by::<Option<String>,_>("status").ok().flatten();
                let m = r.try_get_by::<Option<String>,_>("trigger_meta").ok().flatten()
                    .and_then(|meta| serde_json::from_str::<serde_json::Value>(&meta).ok())
                    .and_then(|v| v.get("requirement").and_then(|r| r.as_str().map(|s| s.to_string())));
                (s, m)
            })).unwrap_or((None, None));
            result.push(TaskItem {
                id: t.id, title: t.title.clone(), description: t.description.clone(), status: t.status.clone(),
                workspace_id: t.workspace_id, template_id: t.template_id, loop_id: t.loop_id,
                template_name: pt.as_ref().map(|p| p.name.clone()),
                complexity: pt.as_ref().map(|p| p.complexity.clone()),
                latest_execution_status: exec_status,
                latest_execution_requirement: exec_req,
                created_at: t.created_at.clone(),
            });
        }
        result
    };
    Ok(ApiResponse::ok(items))
}

/// GET /api/v1/tasks/{id}
pub async fn get_task_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let task = state.db.get_task(id).await?.ok_or(AppError::NotFound)?;
    let template = if let Some(tid) = task.template_id { state.db.get_process_template_by_id(tid).await? } else { None };
    let loop_ = if let Some(lid) = task.loop_id { state.db.get_loop(lid).await? } else { None };
    // steps
    let steps: Vec<_> = if let Some(ref lp) = loop_ {
        state.db.list_loop_steps_by_loop(lp.id).await?.into_iter().map(|s| serde_json::json!({
            "id":s.id,"name":s.name,"order_index":s.order_index,
            "skill_names": serde_json::from_str::<serde_json::Value>(&s.skill_names).unwrap_or_default(),
            "expected_artifacts": serde_json::from_str::<serde_json::Value>(&s.expected_artifacts).unwrap_or_default(),
            "gate_config": serde_json::from_str::<serde_json::Value>(&s.gate_config).unwrap_or_default(),
        })).collect()
    } else { vec![] };
    // executions
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let exec_rows = state.db.conn.query_all(Statement::from_string(DbBackend::Sqlite,
        format!("SELECT id, status, started_at, finished_at, total_steps, completed_steps, failed_steps, trigger_meta \
                 FROM loop_executions WHERE task_id={} ORDER BY started_at DESC LIMIT 20", id)
    )).await?;
    let executions: Vec<_> = exec_rows.iter().map(|r| {
        let meta = r.try_get_by::<Option<String>,_>("trigger_meta").ok().flatten()
            .and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok());
        let requirement = meta.as_ref().and_then(|v| v.get("requirement").and_then(|r| r.as_str().map(|s| s.to_string())));
        serde_json::json!({
            "id": r.try_get_by::<i64,_>("id").unwrap_or(0),
            "status": r.try_get_by::<String,_>("status").unwrap_or_default(),
            "started_at": r.try_get_by::<Option<String>,_>("started_at").ok().flatten(),
            "finished_at": r.try_get_by::<Option<String>,_>("finished_at").ok().flatten(),
            "total_steps": r.try_get_by::<i32,_>("total_steps").unwrap_or(0),
            "completed_steps": r.try_get_by::<i32,_>("completed_steps").unwrap_or(0),
            "failed_steps": r.try_get_by::<i32,_>("failed_steps").unwrap_or(0),
            "requirement": requirement,
        })
    }).collect();
    Ok(ApiResponse::ok(serde_json::json!({
        "task": { "id": task.id, "title": task.title, "status": task.status, "workspace_id": task.workspace_id, "loop_id": task.loop_id },
        "template": template.map(|t| serde_json::json!({"name":t.name,"display_name":t.display_name,"complexity":t.complexity,"version":t.version})),
        "loop": loop_.map(|l| serde_json::json!({"id":l.id,"name":l.name,"status":l.status,"workspace_id":l.workspace_id,"workspace_path":l.workspace_path})),
        "steps": steps,
        "executions": executions,
    })))
}

/// 管理 artifact 内容（略，同之前）
pub async fn get_artifact_content(
    State(state): State<AppState>, Path(aid): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let artifact = state.db.get_loop_step_artifact(aid).await?.ok_or(AppError::NotFound)?;
    let ws_path = resolve_artifact_workspace(&state.db, &artifact).await.unwrap_or_default();
    let content = if artifact.artifact_type == "file" {
        read_workspace_file(&ws_path, &artifact.locator).await
    } else {
        artifact.content_text.unwrap_or_else(|| format!("({}: {})", artifact.artifact_type, artifact.locator))
    };
    let resp = axum::response::Response::builder()
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(axum::body::Body::from(content))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(resp)
}

async fn resolve_artifact_workspace(
    db: &crate::db::Database,
    art: &crate::db::entity::loop_step_artifacts::Model,
) -> Result<String, sea_orm::DbErr> {
    use sea_orm::EntityTrait;
    let se = crate::db::entity::loop_step_executions::Entity::find_by_id(art.loop_step_execution_id).one(&db.conn).await?
        .ok_or(sea_orm::DbErr::RecordNotFound("step_exec not found".into()))?;
    let le = crate::db::entity::loop_executions::Entity::find_by_id(se.loop_execution_id).one(&db.conn).await?
        .ok_or(sea_orm::DbErr::RecordNotFound("loop_exec not found".into()))?;
    let lp = crate::db::entity::loops::Entity::find_by_id(le.loop_id).one(&db.conn).await?
        .ok_or(sea_orm::DbErr::RecordNotFound("loop not found".into()))?;
    Ok(lp.workspace_path.unwrap_or_default())
}

async fn read_workspace_file(ws: &str, rel: &str) -> String {
    let full = std::path::Path::new(ws).join(rel);
    match tokio::fs::read_to_string(&full).await {
        Ok(s) if s.len() <= 128*1024 => s,
        Ok(s) => format!("{}…(仅显示前128KB)", &s[..128*1024]),
        Err(e) => format!("无法读取: {} ({})", e, full.display()),
    }
}

pub fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/tasks", axum::routing::get(list_tasks).post(create_task))
        .route("/api/v1/tasks/{id}", axum::routing::get(get_task_detail))
        .route("/api/v1/artifacts/{aid}/content", axum::routing::get(get_artifact_content))
}
