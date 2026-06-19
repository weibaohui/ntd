//! Loop Studio HTTP handlers。
//!
//! 路由结构：
//! - `/api/loops`                          GET(列表) / POST(创建)
//! - `/api/loops/{id}`                     GET(详情) / PUT(全量更新) / DELETE
//! - `/api/loops/{id}/status`              PUT(切换 status)
//! - `/api/loops/{id}/duplicate`           POST(复制)
//! - `/api/loops/{id}/trigger`             POST(手动触发)
//! - `/api/loops/{id}/triggers`            GET(子资源) / POST
//! - `/api/loops/{id}/triggers/{tid}`      PUT / DELETE
//! - `/api/loops/{id}/steps`              GET / POST
//! - `/api/loops/{id}/steps/reorder`      POST(批量重排)
//! - `/api/loops/{id}/steps/{sid}`        PUT / DELETE
//! - `/api/loops/{id}/executions`          GET(运行历史,分页)
//! - `/api/loops/{id}/executions/{eid}`    GET(单次执行详情)
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::handlers::{AppError, AppState};
use crate::models::{
    self,
    ApiResponse, CreateLoopRequest, CreateLoopStepRequest, CreateTriggerRequest,
    LoopDetail, LoopDto, LoopExecutionDetail, LoopExecutionDto, LoopListItem,
    LoopStepDto, LoopTriggerDto, ReorderLoopStepsRequest,
    UpdateLoopRequest, UpdateLoopStatusRequest, UpdateLoopStepRequest,
    UpdateTriggerRequest,
};

const DEFAULT_PAGE_LIMIT: u64 = 20;
const MAX_PAGE_LIMIT: u64 = 100;

// ====== Loop 主体 ======

/// GET /api/loops — 左栏列表,一次查询带 trigger/step/exec 计数
pub async fn list_loops(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let rows = state.db.list_loops_with_counts().await?;
    let items: Vec<LoopListItem> = rows.into_iter().map(Into::into).collect();
    Ok(ApiResponse::ok(items))
}

/// POST /api/loops — 新建 loop,status 强制为 draft
pub async fn create_loop(
    State(state): State<AppState>,
    Json(req): Json<CreateLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    let created = state
        .db
        .create_loop(
            req.name.trim(),
            &req.description,
            req.workspace.as_deref(),
            &req.color,
            &req.icon,
        )
        .await?;
    Ok((StatusCode::CREATED, ApiResponse::ok(LoopDto::from(created))))
}

/// GET /api/loops/{id} — 完整详情(loop + triggers + steps + todos)
pub async fn get_loop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let view = state
        .db
        .load_loop_full(id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(LoopDetail::from(view)))
}

/// PUT /api/loops/{id} — 全量更新基本字段
pub async fn update_loop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    state
        .db
        .get_loop(id)
        .await?
        .ok_or(AppError::NotFound)?;
    state
        .db
        .update_loop(
            id,
            req.name.trim(),
            &req.description,
            req.workspace.as_deref(),
            &req.color,
            &req.icon,
        )
        .await?;
    let updated = state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(LoopDto::from(updated)))
}

/// DELETE /api/loops/{id} — 删 loop（CASCADE 删 triggers/steps）
pub async fn delete_loop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    // 先把 cron trigger 从 scheduler 移除
    let triggers = state.db.list_triggers_by_loop(id).await?;
    for t in triggers.iter().filter(|t| t.trigger_type == "cron") {
        if let Some(sched) = state.loop_scheduler.as_ref() {
            sched.remove_cron_trigger(t.id).await;
        }
    }
    state.db.delete_loop(id).await?;
    Ok(ApiResponse::ok(()))
}

/// PUT /api/loops/{id}/status — 切换 draft/enabled/paused
pub async fn update_loop_status(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateLoopStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    models::validate_loop_status(&req.status)
        .map_err(AppError::BadRequest)?;
    state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    state.db.update_loop_status(id, &req.status).await?;
    // status 变化时刷新 cron 调度：draft/paused 不应继续触发
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.reload_all().await;
    }
    let updated = state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(LoopDto::from(updated)))
}

/// POST /api/loops/{id}/duplicate — 复制 loop
pub async fn duplicate_loop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let new_loop = state
        .db
        .duplicate_loop(id)
        .await?
        .ok_or(AppError::NotFound)?;
    // 复制后的 loop 是 draft,如包含 cron trigger,要 reload scheduler
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.reload_all().await;
    }
    Ok((StatusCode::CREATED, ApiResponse::ok(LoopDto::from(new_loop))))
}

/// POST /api/loops/{id}/trigger — 手动触发
pub async fn trigger_loop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let dispatcher = state
        .loop_trigger_dispatcher
        .as_ref()
        .ok_or_else(|| AppError::Internal("loop dispatcher not ready".to_string()))?;
    match dispatcher.dispatch_manual(id).await {
        Some(exec_id) => Ok(ApiResponse::ok(serde_json::json!({
            "execution_id": exec_id,
        }))),
        None => Err(AppError::BadRequest(
            "loop 不存在或未启用".to_string(),
        )),
    }
}

// ====== Triggers ======

pub async fn list_triggers(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let triggers = state.db.list_triggers_by_loop(loop_id).await?;
    let dtos: Vec<LoopTriggerDto> = triggers.into_iter().map(Into::into).collect();
    Ok(ApiResponse::ok(dtos))
}

pub async fn create_trigger(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    Json(req): Json<CreateTriggerRequest>,
) -> Result<impl IntoResponse, AppError> {
    models::validate_trigger_type(&req.trigger_type)
        .map_err(AppError::BadRequest)?;
    state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
    let created = state
        .db
        .create_trigger(
            loop_id,
            &req.trigger_type,
            &req.config,
            req.enabled,
            req.priority,
        )
        .await?;
    // 若是 cron trigger 且启用,注册到 scheduler
    if created.trigger_type == "cron" && created.enabled == 1 {
        if let Some(sched) = state.loop_scheduler.as_ref() {
            let _ = sched.upsert_cron_trigger(created.id).await;
        }
    }
    Ok((
        StatusCode::CREATED,
        ApiResponse::ok(LoopTriggerDto::from(created)),
    ))
}

pub async fn update_trigger(
    State(state): State<AppState>,
    Path((loop_id, tid)): Path<(i64, i64)>,
    Json(req): Json<UpdateTriggerRequest>,
) -> Result<impl IntoResponse, AppError> {
    models::validate_trigger_type(&req.trigger_type)
        .map_err(AppError::BadRequest)?;
    state.db.get_trigger(tid).await?.ok_or(AppError::NotFound)?;
    state
        .db
        .update_trigger(tid, &req.trigger_type, &req.config, req.enabled, req.priority)
        .await?;
    // 重新同步 cron scheduler
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.upsert_cron_trigger(tid).await;
    }
    let updated = state
        .db
        .get_trigger(tid)
        .await?
        .ok_or(AppError::NotFound)?;
    if updated.loop_id != loop_id {
        return Err(AppError::BadRequest(
            "trigger 不属于该 loop".to_string(),
        ));
    }
    Ok(ApiResponse::ok(LoopTriggerDto::from(updated)))
}

pub async fn delete_trigger(
    State(state): State<AppState>,
    Path((_loop_id, tid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    state.db.get_trigger(tid).await?.ok_or(AppError::NotFound)?;
    if let Some(sched) = state.loop_scheduler.as_ref() {
        sched.remove_cron_trigger(tid).await;
    }
    state.db.delete_trigger(tid).await?;
    Ok(ApiResponse::ok(()))
}

// ====== Stages ======

pub async fn list_loop_steps(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = state.db.list_loop_steps_with_todo_meta(loop_id).await?;
    let dtos: Vec<LoopStepDto> = rows
        .into_iter()
        .map(|(s, todo_title, todo_executor, todo_status)| LoopStepDto {
            step: s.into(),
            todo_title,
            todo_executor,
            todo_status,
        })
        .collect();
    Ok(ApiResponse::ok(dtos))
}

pub async fn create_loop_step(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    Json(req): Json<CreateLoopStepRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
    // Loop 编排的「节点」必须是环节（来自 steps 表）。
    state
        .db
        .get_step(req.todo_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("step #{} 不存在", req.todo_id)))?;
    let created = state
        .db
        .create_loop_step(
            loop_id,
            req.name.trim(),
            &req.description,
            req.todo_id,
            &req.run_mode,
            req.skip_on_source_failed,
            req.min_rating,
            &req.unrated_policy,
            req.enabled,
        )
        .await?;
    let (_, todo_title, todo_executor, todo_status) = state
        .db
        .list_loop_steps_with_todo_meta(loop_id)
        .await?
        .into_iter()
        .find(|(s, _, _, _)| s.id == created.id)
        .ok_or_else(|| AppError::Internal("created step missing".to_string()))?;
    Ok((
        StatusCode::CREATED,
        ApiResponse::ok(LoopStepDto {
            step: created.into(),
            todo_title,
            todo_executor,
            todo_status,
        }),
    ))
}

pub async fn update_loop_step(
    State(state): State<AppState>,
    Path((loop_id, sid)): Path<(i64, i64)>,
    Json(req): Json<UpdateLoopStepRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    let step = state.db.get_loop_step(sid).await?.ok_or(AppError::NotFound)?;
    if step.loop_id != loop_id {
        return Err(AppError::BadRequest(
            "step 不属于该 loop".to_string(),
        ));
    }
    // 与 create_loop_step 一致: 切换 todo_id 时也必须指向有效的步骤。
    if req.todo_id != step.todo_id {
        state
            .db
            .get_step(req.todo_id)
            .await?
            .ok_or_else(|| AppError::BadRequest(format!("step #{} 不存在", req.todo_id)))?;
    }
    state
        .db
        .update_loop_step(
            sid,
            req.name.trim(),
            &req.description,
            req.todo_id,
            &req.run_mode,
            req.skip_on_source_failed,
            req.min_rating,
            &req.unrated_policy,
            req.enabled,
        )
        .await?;
    let (_, todo_title, todo_executor, todo_status) = state
        .db
        .list_loop_steps_with_todo_meta(loop_id)
        .await?
        .into_iter()
        .find(|(s, _, _, _)| s.id == sid)
        .ok_or_else(|| AppError::Internal("updated step missing".to_string()))?;
    Ok(ApiResponse::ok(LoopStepDto {
        step: state.db.get_loop_step(sid).await?.ok_or(AppError::Internal("step missing".to_string()))?.into(),
        todo_title,
        todo_executor,
        todo_status,
    }))
}

pub async fn delete_loop_step(
    State(state): State<AppState>,
    Path((_loop_id, sid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    state.db.get_loop_step(sid).await?.ok_or(AppError::NotFound)?;
    state.db.delete_loop_step(sid).await?;
    Ok(ApiResponse::ok(()))
}

pub async fn reorder_loop_steps(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    Json(req): Json<ReorderLoopStepsRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
    state.db.reorder_loop_steps(loop_id, &req.ordered_ids).await?;
    Ok(ApiResponse::ok(()))
}

// ====== Executions ======

#[derive(Debug, Deserialize)]
pub struct ExecutionPageQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

pub async fn list_executions(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    Query(q): Query<ExecutionPageQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(DEFAULT_PAGE_LIMIT).min(MAX_PAGE_LIMIT);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * limit;
    let records = state.db.list_loop_executions(loop_id, limit, offset).await?;
    let total = state.db.count_loop_executions(loop_id).await?;
    let items: Vec<LoopExecutionDto> = records.into_iter().map(Into::into).collect();
    Ok(ApiResponse::ok(serde_json::json!({
        "items": items,
        "total": total,
        "page": page,
        "limit": limit,
    })))
}

pub async fn get_execution(
    State(state): State<AppState>,
    Path((loop_id, eid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    let exec = state
        .db
        .get_loop_execution(eid)
        .await?
        .ok_or(AppError::NotFound)?;
    if exec.loop_id != loop_id {
        return Err(AppError::BadRequest(
            "execution 不属于该 loop".to_string(),
        ));
    }
    let step_execs = state
        .db
        .list_loop_step_executions(eid)
        .await?;
    let loop_name = state
        .db
        .get_loop(loop_id)
        .await?
        .map(|l| l.name)
        .unwrap_or_default();
    Ok(ApiResponse::ok(LoopExecutionDetail {
        execution: exec.into(),
        step_executions: step_execs.into_iter().map(Into::into).collect(),
        loop_name,
    }))
}

// ====== 路由表 ======

pub fn loop_routes() -> axum::Router<AppState> {
    use axum::routing::{get, post, put};
    axum::Router::new()
        .route("/api/loops", get(list_loops).post(create_loop))
        .route("/api/loops/{id}", get(get_loop).put(update_loop).delete(delete_loop))
        .route("/api/loops/{id}/status", put(update_loop_status))
        .route("/api/loops/{id}/duplicate", post(duplicate_loop))
        .route("/api/loops/{id}/trigger", post(trigger_loop))
        .route("/api/loops/{id}/triggers", get(list_triggers).post(create_trigger))
        .route(
            "/api/loops/{id}/triggers/{tid}",
            put(update_trigger).delete(delete_trigger),
        )
        .route("/api/loops/{id}/steps", get(list_loop_steps).post(create_loop_step))
        .route("/api/loops/{id}/steps/reorder", post(reorder_loop_steps))
        .route(
            "/api/loops/{id}/steps/{sid}",
            put(update_loop_step).delete(delete_loop_step),
        )
        .route("/api/loops/{id}/executions", get(list_executions))
        .route("/api/loops/{id}/executions/{eid}", get(get_execution))
}
