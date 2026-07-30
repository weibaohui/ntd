//! 任务管理 API。
use axum::extract::{Path, Query, State};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;

// 设计原则：step todo 的 prompt 是只读模板，由 loop_runner 在内存中做占位符替换后
// 传给执行器，绝不写回数据库。需求文本通过 trigger_meta.requirement 传递给 LoopRunner，
// 由 LoopRunner 在运行时替换 {{requirement}} 占位符或兜底追加到 enhanced_prompt 末尾。
// 之前这里有一个 inject_requirement_to_steps 函数直接 UPDATE todos.prompt，
// 会随执行次数累加多段「## 任务需求」污染模板，已删除。

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub requirement: String,
    pub loop_id: i64,
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
    Path(_ws): Path<i64>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let lp = state.db.get_loop(req.loop_id).await?.ok_or(AppError::NotFound)?;
    let title = req.requirement.lines().next().unwrap_or(&req.requirement).trim();
    let title = if title.len() > 60 { format!("{}…", &title[..60]) } else { title.to_string() };
    let task = state.db.create_task(&title, lp.workspace_id.unwrap_or(1), lp.process_template_id.unwrap_or(0), Some(req.loop_id)).await?;
    state.db.update_task_description(task.id, &req.requirement).await?;
    // 需求不写入 step todo 的 prompt（避免污染模板），通过 trigger_meta 传递给 LoopRunner。
    let _ = state.db.update_loop_status(req.loop_id, "enabled").await;
    let dispatcher = state.loop_trigger_dispatcher.as_ref()
        .ok_or_else(|| AppError::Internal("loop dispatcher not ready".to_string()))?;
    let meta = serde_json::json!({"requirement": req.requirement, "source": "task"});
    match dispatcher.dispatch_manual_with_meta(req.loop_id, meta).await {
        Some(exec_id) => {
            state.db.update_loop_execution_task_id(exec_id, task.id).await?;
            Ok((axum::http::StatusCode::CREATED, ApiResponse::ok(serde_json::json!({
                "task_id": task.id, "loop_id": req.loop_id, "execution_id": exec_id,
            }))))
        }
        None => Err(AppError::BadRequest("无法触发执行".to_string())),
    }
}

/// GET /api/v1/workspaces/{ws}/tasks
/// 按工作空间列出任务，可选按 status 过滤。
/// ws 来自 URL path，用于按 workspace_id 过滤（修复之前忽略 ws 导致跨工作空间数据相同的 bug）。
pub async fn list_tasks(
    State(state): State<AppState>,
    Path(ws): Path<i64>,
    Query(q): Query<ListTasksQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let tasks = state.db.list_tasks(ws, q.status.as_deref()).await?;
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
            // 模板展示名：优先用中文 display_name，空时回退英文唯一名 name，
            // 与 services/process/recommender.rs 的展示名降级策略保持一致。
            // 前端任务列表/卡片/详情三处均从此字段取展示文本，统一为中文名。
            template_name: pt.as_ref().map(|p| {
                if p.display_name.is_empty() { p.name.clone() } else { p.display_name.clone() }
            }),
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
    Path((_ws, id)): Path<(i64, i64)>,
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
    // 统计每次执行的待审批环节数，前端据此在执行历史行上显示「待审批」引导标记（NTD-004）。
    let exec_ids: Vec<i64> = exec_rows.iter().map(|r| r.try_get_by::<i64,_>("id").unwrap_or(0)).collect();
    let pending_counts = state.db.count_pending_approvals_by_execution_ids(&exec_ids).await?;
    let executions: Vec<_> = exec_rows.iter().map(|r| {
        let meta = r.try_get_by::<Option<String>,_>("trigger_meta").ok().flatten()
            .and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok());
        let requirement = meta.as_ref().and_then(|v| v.get("requirement").and_then(|r| r.as_str().map(|s| s.to_string())));
        let exec_id = r.try_get_by::<i64,_>("id").unwrap_or(0);
        serde_json::json!({
            "id": exec_id,
            "status": r.try_get_by::<String,_>("status").unwrap_or_default(),
            "started_at": r.try_get_by::<Option<String>,_>("started_at").ok().flatten(),
            "finished_at": r.try_get_by::<Option<String>,_>("finished_at").ok().flatten(),
            "total_steps": r.try_get_by::<i32,_>("total_steps").unwrap_or(0),
            "completed_steps": r.try_get_by::<i32,_>("completed_steps").unwrap_or(0),
            "failed_steps": r.try_get_by::<i32,_>("failed_steps").unwrap_or(0),
            "requirement": requirement,
            "pending_approval_count": pending_counts.get(&exec_id).copied().unwrap_or(0),
        })
    }).collect();
    Ok(ApiResponse::ok(serde_json::json!({
        "task": { "id": task.id, "title": task.title, "status": task.status, "workspace_id": task.workspace_id, "loop_id": task.loop_id },
        "template": template.map(|t| {
            // 版本优先取环路的 process_template_version（执行时的快照），
            // 而非工艺模板表的最新版本——用户关注的是「执行当时的工艺版本」。
            let version = loop_.as_ref().and_then(|l| l.process_template_version.clone()).unwrap_or(t.version.clone());
            serde_json::json!({"name":t.name,"display_name":t.display_name,"complexity":t.complexity,"version":version})
        }),
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

/// POST /api/v1/tasks/{id}/executions — 为已有任务创建新执行（复用 task_id + loop）。
pub async fn create_task_execution(
    State(state): State<AppState>,
    Path((_ws, id)): Path<(i64, i64)>,
    Json(req): Json<NewExecutionRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let task = state.db.get_task(id).await?.ok_or(AppError::NotFound)?;
    let loop_id = task.loop_id.ok_or_else(|| AppError::BadRequest("任务未关联 Loop".to_string()))?;
    state.db.update_task_description(id, &req.requirement).await?;
    // 需求不写入 step todo 的 prompt（避免污染模板），通过 trigger_meta 传递给 LoopRunner。
    let dispatcher = state.loop_trigger_dispatcher.as_ref()
        .ok_or_else(|| AppError::Internal("dispatcher not ready".to_string()))?;
    let meta = serde_json::json!({"requirement": req.requirement, "source": "task"});
    match dispatcher.dispatch_manual_with_meta(loop_id, meta).await {
        Some(exec_id) => {
            state.db.update_loop_execution_task_id(exec_id, id).await?;
            Ok(ApiResponse::ok(serde_json::json!({"execution_id": exec_id})))
        }
        None => Err(AppError::BadRequest("无法触发执行".to_string())),
    }
}

#[derive(Deserialize)]
pub struct NewExecutionRequest { pub requirement: String }

/// DELETE /api/v1/workspaces/{ws}/tasks/{id} — 删除单个任务。
pub async fn delete_task(
    State(state): State<AppState>,
    Path((_ws, id)): Path<(i64, i64)>,
) -> Result<ApiResponse<()>, AppError> {
    state.db.delete_task(id).await.map_err(AppError::from)?;
    Ok(ApiResponse::ok(()))
}

/// POST /api/v1/workspaces/{ws}/tasks/batch-delete — 批量删除任务。
#[derive(Deserialize)]
pub struct BatchDeleteTasksRequest { pub ids: Vec<i64> }
pub async fn batch_delete_tasks(
    State(state): State<AppState>,
    Path(_ws): Path<i64>,
    Json(req): Json<BatchDeleteTasksRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let deleted = state.db.batch_delete_tasks(&req.ids).await?;
    Ok(ApiResponse::ok(serde_json::json!({
        "deleted": deleted, "total": req.ids.len(),
    })))
}

pub fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/workspaces/{ws}/tasks", axum::routing::get(list_tasks).post(create_task))
        .route("/api/v1/workspaces/{ws}/tasks/{id}", axum::routing::get(get_task_detail).delete(delete_task))
        .route("/api/v1/workspaces/{ws}/tasks/batch-delete", axum::routing::post(batch_delete_tasks))
        .route("/api/v1/workspaces/{ws}/tasks/{id}/executions", axum::routing::post(create_task_execution))
        .route("/api/v1/artifacts/{aid}/content", axum::routing::get(get_artifact_content))
}
