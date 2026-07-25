//! 任务管理 API。
//!
//! 提供任务创建（推荐→安装→注入需求）与任务列表（按模板创建的 Loop）两个接口。

use axum::extract::State;
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};

use axum::extract::Path;
use crate::db::Database;
use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;
use crate::services::process::installer::install_process_template;

/// 遍历 Loop 的 steps，把每个 step 关联 todo 的 prompt 追加需求文本。
async fn inject_requirement_to_steps(db: &Database, loop_id: i64, requirement: &str) -> Result<(), AppError> {
    use sea_orm::ConnectionTrait;
    let steps = db.list_loop_steps_by_loop(loop_id).await?;
    for step in &steps {
        // 使用参数化 SQL 防止需求中特殊字符导致 SQL 注入。
        let append = format!("\n\n## 任务需求\n{}", requirement);
        let sql = "UPDATE todos SET prompt = prompt || ?1 WHERE id = ?2";
        db.conn
            .execute(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite, sql,
                [append.into(), step.todo_id.into()],
            ))
            .await?;
    }
    Ok(())
}

/// 创建任务请求体。
#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub requirement: String,
    pub workspace_id: i64,
    pub template_name: Option<String>,
}

/// 创建任务：推荐→安装→注入需求。
pub async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::services::process::recommender::{RecommendRequest, recommend};
    // 1. 选择模板。
    let template_name = if let Some(ref name) = req.template_name {
        name.clone()
    } else {
        let templates = state.db.list_process_templates().await?;
        let rec_req = RecommendRequest { description: req.requirement.clone() };
        let resp = recommend(&templates, &rec_req);
        resp.recommendations
            .first()
            .map(|r| r.template_name.clone())
            .ok_or_else(|| AppError::BadRequest("无可推荐的工艺模板，请先同步 bundled 工艺库".into()))?
    };
    // 2. 加载模板、校验工作空间。
    let template = state.db.get_process_template_by_name(&template_name).await?
        .ok_or_else(|| AppError::BadRequest(format!("工艺模板 {} 不存在", template_name)))?;
    let workspace = state.db.get_project_directory_by_id(req.workspace_id).await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id)))?;
    // 3. 安装为 Loop。
    let result = install_process_template(&state.db, &template, req.workspace_id, &workspace.path)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    // 4. 把需求文本写入 loops.description。
    state.db.set_loop_description(result.loop_id, &req.requirement).await?;
    // 5. 注入需求到所有 step todo 的 prompt 末尾。
    inject_requirement_to_steps(&state.db, result.loop_id, &req.requirement).await?;
    Ok((axum::http::StatusCode::CREATED, ApiResponse::ok(serde_json::json!({
        "task_id": result.loop_id,
        "loop_name": result.loop_name,
        "template_name": template_name,
        "phase_count": result.phase_count,
        "step_count": result.step_count,
    }))))
}

/// 任务列表响应项。
#[derive(Debug, Serialize)]
pub struct TaskListItem {
    pub loop_id: i64,
    pub name: String,
    pub description: String,
    pub template_name: Option<String>,
    pub complexity: Option<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub workspace_id: Option<i64>,
    pub latest_execution_id: Option<i64>,
    pub latest_execution_status: Option<String>,
}

/// 列出所有由工艺模板创建的任务。
pub async fn list_tasks(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let sql = "SELECT l.id, l.name, l.description, l.status, l.created_at, \
               l.workspace_id, \
               pt.name AS template_name, pt.complexity, \
               (SELECT le.id FROM loop_executions le WHERE le.loop_id=l.id ORDER BY le.started_at DESC LIMIT 1) AS latest_exec_id, \
               (SELECT le.status FROM loop_executions le WHERE le.loop_id=l.id ORDER BY le.started_at DESC LIMIT 1) AS latest_exec_status \
               FROM loops l \
               JOIN process_templates pt ON pt.id = l.process_template_id \
               WHERE l.process_template_id IS NOT NULL \
               ORDER BY l.updated_at DESC LIMIT 50";
    let rows = state.db.conn.query_all(Statement::from_string(DbBackend::Sqlite, sql)).await?;
    let items: Vec<TaskListItem> = rows.iter().map(|r| TaskListItem {
        loop_id: r.try_get_by("id").unwrap_or(0),
        name: r.try_get_by("name").unwrap_or_default(),
        description: r.try_get_by("description").unwrap_or_default(),
        template_name: r.try_get_by::<Option<String>, _>("template_name").ok().flatten(),
        complexity: r.try_get_by::<Option<String>, _>("complexity").ok().flatten(),
        status: r.try_get_by("status").unwrap_or_default(),
        created_at: r.try_get_by("created_at").ok(),
        workspace_id: r.try_get_by::<Option<i64>, _>("workspace_id").ok().flatten(),
        latest_execution_id: r.try_get_by::<Option<i64>, _>("latest_exec_id").ok().flatten(),
        latest_execution_status: r.try_get_by::<Option<String>, _>("latest_exec_status").ok().flatten(),
    }).collect();
    Ok(ApiResponse::ok(items))
}

/// 任务详情。
pub async fn get_task_detail(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let loop_ = state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
    let template = if let Some(tid) = loop_.process_template_id {
        state.db.get_process_template_by_id(tid).await?.ok_or(AppError::NotFound)?
    } else {
        return Err(AppError::NotFound);
    };
    // 步骤配置（含 skill/artifact/gate）。
    let steps: Vec<_> = state.db.list_loop_steps_by_loop(loop_id).await?
        .into_iter().map(|s| serde_json::json!({
            "id": s.id, "name": s.name, "order_index": s.order_index,
            "skill_names": serde_json::from_str::<serde_json::Value>(&s.skill_names).unwrap_or_default(),
            "expected_artifacts": serde_json::from_str::<serde_json::Value>(&s.expected_artifacts).unwrap_or_default(),
            "gate_config": serde_json::from_str::<serde_json::Value>(&s.gate_config).unwrap_or_default(),
        })).collect();
    // 执行历史。
    let execs: Vec<_> = state.db.list_loop_executions(loop_id, 20, 0, None).await?
        .into_iter().map(|e| serde_json::json!({
            "id": e.id, "status": e.status,
            "started_at": e.started_at, "finished_at": e.finished_at,
            "total_steps": e.total_steps, "completed_steps": e.completed_steps,
            "failed_steps": e.failed_steps,
        })).collect();
    Ok(ApiResponse::ok(serde_json::json!({
        "loop": { "id": loop_.id, "name": loop_.name, "description": loop_.description, "status": loop_.status, "workspace_id": loop_.workspace_id },
        "template": { "name": template.name, "display_name": template.display_name, "complexity": template.complexity, "version": template.version },
        "steps": steps,
        "executions": execs,
    })))
}

/// 读取产物内容。
/// - text/url/json 类型：直接返回 DB 快照。
/// - file 类型：优先用 DB 快照；若为空则尝试从工作目录读取实际文件。
pub async fn get_artifact_content(
    State(state): State<AppState>,
    Path(aid): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use axum::response::Response;
    let artifact = state.db.get_loop_step_artifact(aid).await?.ok_or(AppError::NotFound)?;
    // 链式查 workspace_path：artifact → step_execution → loop_execution → loop
    let ws_path = resolve_artifact_workspace(&state.db, &artifact).await.unwrap_or_default();
    let content = if artifact.artifact_type == "file" {
        // file 类产物始终从磁盘读，DB 只存路径。
        read_workspace_file(&ws_path, &artifact.locator).await
    } else {
        artifact.content_text.unwrap_or_else(|| format!("({}: {})", artifact.artifact_type, artifact.locator))
    };
    let resp = Response::builder()
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(axum::body::Body::from(content))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(resp)
}

/// 链式解析 artifact 所属的工作空间路径。
async fn resolve_artifact_workspace(
    db: &Database,
    artifact: &crate::db::entity::loop_step_artifacts::Model,
) -> Result<String, sea_orm::DbErr> {
    use sea_orm::EntityTrait;
    let se = crate::db::entity::loop_step_executions::Entity::find_by_id(artifact.loop_step_execution_id)
        .one(&db.conn).await?.ok_or(sea_orm::DbErr::RecordNotFound("step_exec not found".into()))?;
    let le = crate::db::entity::loop_executions::Entity::find_by_id(se.loop_execution_id)
        .one(&db.conn).await?.ok_or(sea_orm::DbErr::RecordNotFound("loop_exec not found".into()))?;
    let lp = crate::db::entity::loops::Entity::find_by_id(le.loop_id)
        .one(&db.conn).await?.ok_or(sea_orm::DbErr::RecordNotFound("loop not found".into()))?;
    Ok(lp.workspace_path.unwrap_or_default())
}

/// 从工作目录读取文件内容（限 128KB）。
async fn read_workspace_file(ws_path: &str, rel_path: &str) -> String {
    let full = std::path::Path::new(ws_path).join(rel_path);
    match tokio::fs::read_to_string(&full).await {
        Ok(s) if s.len() <= 128 * 1024 => s,
        Ok(s) => format!("{}...\n(文件过大，仅显示前 128KB)", &s[..128 * 1024]),
        Err(e) => format!("无法读取文件: {}\n路径: {}", e, full.display()),
    }
}

/// 任务路由。
pub fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/tasks", axum::routing::get(list_tasks).post(create_task))
        .route("/api/v1/tasks/{loop_id}", axum::routing::get(get_task_detail))
        .route("/api/v1/artifacts/{aid}/content", axum::routing::get(get_artifact_content))
}
