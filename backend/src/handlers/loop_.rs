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
//! - `/api/loops/{id}/stages`              GET / POST
//! - `/api/loops/{id}/stages/reorder`      POST(批量重排)
//! - `/api/loops/{id}/stages/{sid}`        PUT / DELETE
//! - `/api/loops/{id}/hooks`               GET / POST
//! - `/api/loops/{id}/hooks/{hid}`         DELETE
//! - `/api/loops/{id}/executions`          GET(运行历史,分页)
//! - `/api/loops/{id}/executions/{eid}`    GET(单次执行详情)
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::db::entity::{loop_executions, loop_hooks, loop_stage_executions, loop_stages, loop_triggers};
use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{
    self,
    ApiResponse, CreateHookRequest, CreateLoopRequest, CreateStageRequest, CreateTriggerRequest,
    LoopDetail, LoopDto, LoopExecutionDetail, LoopExecutionDto, LoopHookDto, LoopListItem,
    LoopStageDto, LoopStageExecutionDto, LoopTriggerDto, ReorderStagesRequest,
    UpdateHookRequest, UpdateLoopRequest, UpdateLoopStatusRequest, UpdateStageRequest,
    UpdateTriggerRequest,
};
use crate::services::loop_scheduler::LoopScheduler;

const DEFAULT_PAGE_LIMIT: u64 = 20;
const MAX_PAGE_LIMIT: u64 = 100;

// ====== Loop 主体 ======

/// GET /api/loops — 左栏列表,一次查询带 trigger/stage/exec 计数
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
            &req.product,
            &req.repo,
            &req.branch,
            &req.color,
            &req.icon,
        )
        .await?;
    Ok((StatusCode::CREATED, ApiResponse::ok(LoopDto::from(created))))
}

/// GET /api/loops/{id} — 完整详情(loop + triggers + stages + hooks + todos)
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
            &req.product,
            &req.repo,
            &req.branch,
            &req.color,
            &req.icon,
        )
        .await?;
    let updated = state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(LoopDto::from(updated)))
}

/// DELETE /api/loops/{id} — 删 loop（CASCADE 删 triggers/stages/hooks）
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

pub async fn list_stages(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = state.db.list_stages_with_todo_meta(loop_id).await?;
    let dtos: Vec<LoopStageDto> = rows
        .into_iter()
        .map(|(s, todo_title, todo_executor, todo_status)| LoopStageDto {
            stage: s.into(),
            todo_title,
            todo_executor,
            todo_status,
        })
        .collect();
    Ok(ApiResponse::ok(dtos))
}

pub async fn create_stage(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    Json(req): Json<CreateStageRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
    // Loop 编排的「节点」必须是环节,不允许引用一次性事项。
    // 这是 v3 migration 引入 kind 列后的语义约束：loop 是"循环复用的编排"，
    // 一次性事项没有跨 loop 的复用价值,挂在 stage 里会导致引用方向混乱。
    let target = state
        .db
        .get_todo(req.todo_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("todo #{} 不存在", req.todo_id)))?;
    if target.kind != "expert" {
        return Err(AppError::BadRequest(format!(
            "todo #{} 不是环节(kind={}); 请先在环节页 promote,或在循环编辑器中创建环节",
            req.todo_id, target.kind
        )));
    }
    let created = state
        .db
        .create_stage(
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
        .list_stages_with_todo_meta(loop_id)
        .await?
        .into_iter()
        .find(|(s, _, _, _)| s.id == created.id)
        .ok_or_else(|| AppError::Internal("created stage missing".to_string()))?;
    Ok((
        StatusCode::CREATED,
        ApiResponse::ok(LoopStageDto {
            stage: created.into(),
            todo_title,
            todo_executor,
            todo_status,
        }),
    ))
}

pub async fn update_stage(
    State(state): State<AppState>,
    Path((loop_id, sid)): Path<(i64, i64)>,
    Json(req): Json<UpdateStageRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    let stage = state.db.get_stage(sid).await?.ok_or(AppError::NotFound)?;
    if stage.loop_id != loop_id {
        return Err(AppError::BadRequest(
            "stage 不属于该 loop".to_string(),
        ));
    }
    // 与 create_stage 一致: 切换 todo_id 时也必须指向环节。
    // 防御性: 即便前端已经在候选里筛了环节,后端再校验一次避免越权。
    if req.todo_id != stage.todo_id {
        let target = state
            .db
            .get_todo(req.todo_id)
            .await?
            .ok_or_else(|| AppError::BadRequest(format!("todo #{} 不存在", req.todo_id)))?;
        if target.kind != "expert" {
            return Err(AppError::BadRequest(format!(
                "todo #{} 不是环节(kind={}); 不能作为 stage 的目标",
                req.todo_id, target.kind
            )));
        }
    }
    state
        .db
        .update_stage(
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
        .list_stages_with_todo_meta(loop_id)
        .await?
        .into_iter()
        .find(|(s, _, _, _)| s.id == sid)
        .ok_or_else(|| AppError::Internal("updated stage missing".to_string()))?;
    Ok(ApiResponse::ok(LoopStageDto {
        stage: state.db.get_stage(sid).await?.ok_or(AppError::Internal("stage missing".to_string()))?.into(),
        todo_title,
        todo_executor,
        todo_status,
    }))
}

pub async fn delete_stage(
    State(state): State<AppState>,
    Path((_loop_id, sid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    state.db.get_stage(sid).await?.ok_or(AppError::NotFound)?;
    state.db.delete_stage(sid).await?;
    Ok(ApiResponse::ok(()))
}

pub async fn reorder_stages(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    Json(req): Json<ReorderStagesRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
    state.db.reorder_stages(loop_id, &req.ordered_ids).await?;
    Ok(ApiResponse::ok(()))
}

// ====== Hooks ======

pub async fn list_hooks(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let hooks = state.db.list_hooks_by_loop(loop_id).await?;
    let dtos: Vec<LoopHookDto> = hooks.into_iter().map(Into::into).collect();
    Ok(ApiResponse::ok(dtos))
}

pub async fn create_hook(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    Json(req): Json<CreateHookRequest>,
) -> Result<impl IntoResponse, AppError> {
    models::validate_hook_position(&req.hook_position)
        .map_err(AppError::BadRequest)?;
    state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
    state
        .db
        .get_todo(req.target_todo_id)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(format!("target todo #{} 不存在", req.target_todo_id))
        })?;
    // pre_stage / post_stage 必须有 source_stage_id
    if matches!(req.hook_position.as_str(), "pre_stage" | "post_stage")
        && req.source_stage_id.is_none()
    {
        return Err(AppError::BadRequest(
            "pre_stage / post_stage 必须指定 source_stage_id".to_string(),
        ));
    }
    if let Some(sid) = req.source_stage_id {
        let stage = state.db.get_stage(sid).await?.ok_or(AppError::NotFound)?;
        if stage.loop_id != loop_id {
            return Err(AppError::BadRequest(
                "source_stage 不属于该 loop".to_string(),
            ));
        }
    }
    let created = state
        .db
        .create_hook(
            loop_id,
            &req.hook_position,
            req.source_stage_id,
            req.target_todo_id,
            req.skip_if_missing,
            req.enabled,
            req.min_rating,
            &req.unrated_policy,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        ApiResponse::ok(LoopHookDto::from(created)),
    ))
}

pub async fn update_hook(
    State(state): State<AppState>,
    Path((loop_id, hid)): Path<(i64, i64)>,
    Json(req): Json<UpdateHookRequest>,
) -> Result<impl IntoResponse, AppError> {
    models::validate_hook_position(&req.hook_position)
        .map_err(AppError::BadRequest)?;
    let hook = state.db.get_hook(hid).await?.ok_or(AppError::NotFound)?;
    if hook.loop_id != loop_id {
        return Err(AppError::BadRequest(
            "hook 不属于该 loop".to_string(),
        ));
    }
    if matches!(req.hook_position.as_str(), "pre_stage" | "post_stage")
        && req.source_stage_id.is_none()
    {
        return Err(AppError::BadRequest(
            "pre_stage / post_stage 必须指定 source_stage_id".to_string(),
        ));
    }
    state
        .db
        .update_hook(
            hid,
            &req.hook_position,
            req.source_stage_id,
            req.target_todo_id,
            req.skip_if_missing,
            req.enabled,
            req.min_rating,
            &req.unrated_policy,
        )
        .await?;
    let updated = state.db.get_hook(hid).await?.ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(LoopHookDto::from(updated)))
}

pub async fn delete_hook(
    State(state): State<AppState>,
    Path((_loop_id, hid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    state.db.get_hook(hid).await?.ok_or(AppError::NotFound)?;
    state.db.delete_hook(hid).await?;
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
    let stage_execs = state
        .db
        .list_loop_stage_executions(eid)
        .await?;
    let loop_name = state
        .db
        .get_loop(loop_id)
        .await?
        .map(|l| l.name)
        .unwrap_or_default();
    Ok(ApiResponse::ok(LoopExecutionDetail {
        execution: exec.into(),
        stage_executions: stage_execs.into_iter().map(Into::into).collect(),
        loop_name,
    }))
}

// ====== 路由表 ======

pub fn loop_routes() -> axum::Router<AppState> {
    use axum::routing::{delete, get, post, put};
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
        .route("/api/loops/{id}/stages", get(list_stages).post(create_stage))
        .route("/api/loops/{id}/stages/reorder", post(reorder_stages))
        .route(
            "/api/loops/{id}/stages/{sid}",
            put(update_stage).delete(delete_stage),
        )
        .route("/api/loops/{id}/hooks", get(list_hooks).post(create_hook))
        .route(
            "/api/loops/{id}/hooks/{hid}",
            put(update_hook).delete(delete_hook),
        )
        .route("/api/loops/{id}/executions", get(list_executions))
        .route("/api/loops/{id}/executions/{eid}", get(get_execution))
}
