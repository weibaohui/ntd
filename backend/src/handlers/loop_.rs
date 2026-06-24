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
    LoopDetail, LoopDto, LoopExecutionDetail, LoopExecutionDto, LoopExecutionTokenSummary,
    LoopListItem, LoopStepDto, LoopStepExecutionDto, LoopTriggerDto, ReorderLoopStepsRequest,
    UpdateLoopRequest, UpdateLoopStatusRequest, UpdateLoopStepRequest,
    UpdateTriggerRequest, ApproveStepExecutionRequest, UpdateTagsRequest,
};

const DEFAULT_PAGE_LIMIT: u64 = 20;
const MAX_PAGE_LIMIT: u64 = 100;

// ====== Loop 主体 ======

/// GET /api/loops — 左栏列表,一次查询带 trigger/step/exec 计数
pub async fn list_loops(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let workspace = params.get("workspace").map(|s| s.as_str());
    let rows = state.db.list_loops_with_counts(workspace).await?;
    let items: Vec<LoopListItem> = rows.into_iter().map(Into::into).collect();
    // 批量查询所有 loop 的标签映射，避免逐条 N+1 查询
    let loop_ids: Vec<i64> = items.iter().map(|item| item.loop_.id).collect();
    let tag_map = state.db.get_loop_tag_ids_batch(&loop_ids).await?;
    let results: Vec<LoopListItem> = items
        .into_iter()
        .map(|item| {
            let tag_ids = tag_map.get(&item.loop_.id).cloned().unwrap_or_default();
            item.with_tags(tag_ids)
        })
        .collect();
    Ok(ApiResponse::ok(results))
}

/// POST /api/loops — 新建 loop,status 强制为 paused
pub async fn create_loop(
    State(state): State<AppState>,
    Json(req): Json<CreateLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    // 创建环路前校验工作空间必填
    if req.workspace.trim().is_empty() {
        return Err(AppError::BadRequest("workspace 不能为空".to_string()));
    }
    // 创建环路前校验标签约束：环路只能选择一个标签
    if req.tag_ids.len() > 1 {
        return Err(AppError::BadRequest("环路只能选择一个标签".to_string()));
    }
    let created = state
        .db
        .create_loop(
            req.name.trim(),
            &req.description,
            Some(req.workspace.trim()),
            &req.icon,
            req.review_template_id,
        )
        .await?;
    // 如果创建请求携带了 tag_ids，则持久化标签关联；否则新建环路从空标签开始
    if !req.tag_ids.is_empty() {
        state.db.set_loop_tags(created.id, &req.tag_ids).await?;
    }
    let tag_ids = state.db.get_loop_tag_ids(created.id).await?;
    Ok((StatusCode::CREATED, ApiResponse::ok(LoopDto::from(created).with_tags(tag_ids))))
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
    // 加载环路关联的标签 ID（复用模型层 LoopDetail::from 转换，只注入 tags）
    let tag_ids = state.db.get_loop_tag_ids(id).await?;
    let mut detail = LoopDetail::from(view);
    detail.loop_ = detail.loop_.with_tags(tag_ids);
    Ok(ApiResponse::ok(detail))
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
            &req.icon,
            req.review_template_id,
            req.limits_config.as_deref(),
        )
        .await?;
    // 如果请求携带了 tag_ids，则更新标签关联；
    // 合并到同一个 handler 中避免前端分两次保存导致的部分提交风险
    if let Some(ref tag_ids) = req.tag_ids {
        if tag_ids.len() > 1 {
            return Err(AppError::BadRequest("环路只能选择一个标签".to_string()));
        }
        state.db.set_loop_tags(id, tag_ids).await?;
    }
    let updated = state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    let tag_ids = state.db.get_loop_tag_ids(id).await?;
    Ok(ApiResponse::ok(LoopDto::from(updated).with_tags(tag_ids)))
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

/// PUT /api/loops/{id}/status — 切换 enabled/paused
pub async fn update_loop_status(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateLoopStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    models::validate_loop_status(&req.status)
        .map_err(AppError::BadRequest)?;
    state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    state.db.update_loop_status(id, &req.status).await?;
    // status 变化时刷新 cron 调度：paused 不应继续触发
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.reload_all().await;
    }
    let updated = state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    let tag_ids = state.db.get_loop_tag_ids(id).await?;
    Ok(ApiResponse::ok(LoopDto::from(updated).with_tags(tag_ids)))
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
    // 复制后的 loop 是 paused,如包含 cron trigger,要 reload scheduler
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.reload_all().await;
    }
    // 复制时标签不复制，新 loop 从空标签开始
    Ok((StatusCode::CREATED, ApiResponse::ok(LoopDto::from(new_loop).with_tags(vec![]))))
}

/// PUT /api/loops/{id}/tags — 更新环路标签（全量替换）
pub async fn update_loop_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateTagsRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    // 强制单选标签约束：前端是 TagCheckCardGroup 单选，后端防御多于 1 个标签的非法请求
    if req.tag_ids.len() > 1 {
        return Err(AppError::BadRequest("环路只能选择一个标签".to_string()));
    }
    state.db.set_loop_tags(id, &req.tag_ids).await?;
    let updated = state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    let tag_ids = state.db.get_loop_tag_ids(id).await?;
    Ok(ApiResponse::ok(LoopDto::from(updated).with_tags(tag_ids)))
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

// ====== Steps ======

pub async fn list_loop_steps(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = state.db.list_loop_steps_with_todo_meta(loop_id).await?;
    let dtos: Vec<LoopStepDto> = rows
        .into_iter()
        .map(|(s, todo_title, todo_executor)| LoopStepDto {
            step: s.into(),
            todo_title,
            todo_executor,
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
    // Loop 编排的「节点」必须是环节（来自 steps 表），按 step_id 校验。
    state
        .db
        .get_step(req.step_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("step #{} 不存在", req.step_id)))?;
    let created = state
        .db
        .create_loop_step(
            loop_id,
            req.name.trim(),
            &req.description,
            req.step_id,
            &req.run_mode,
            req.skip_on_source_failed,
            req.min_rating,
            &req.unrated_policy,
            req.enabled,
            &req.on_success,
            req.success_goto_step_id,
            &req.on_rating_fail,
            req.fail_goto_step_id,
            &req.review_type,
        )
        .await?;
    let (_, todo_title, todo_executor) = state
        .db
        .list_loop_steps_with_todo_meta(loop_id)
        .await?
        .into_iter()
        .find(|(s, _, _)| s.id == created.id)
        .ok_or_else(|| AppError::Internal("created step missing".to_string()))?;
    Ok((
        StatusCode::CREATED,
        ApiResponse::ok(LoopStepDto {
            step: created.into(),
            todo_title,
            todo_executor,
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
    // 与 create_loop_step 一致: 切换 step_id 时也必须指向有效的步骤。
    if req.step_id != step.step_id {
        state
            .db
            .get_step(req.step_id)
            .await?
            .ok_or_else(|| AppError::BadRequest(format!("step #{} 不存在", req.step_id)))?;
    }
    // 评分不通过时跳转到自身（重试），需要 loop 有兜底限制
    if req.on_rating_fail == "goto" && req.fail_goto_step_id == Some(sid) {
        let loop_ = state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
        // 解析 limits_config，检查是否配置了步数或 Token 限制
        #[derive(serde::Deserialize, Default)]
        struct LimitsConfig {
            max_step_executions: Option<i32>,
            max_total_tokens: Option<i64>,
        }
        let limits: LimitsConfig = serde_json::from_str(&loop_.limits_config).unwrap_or_default();
        if limits.max_step_executions.is_none() && limits.max_total_tokens.is_none() {
            return Err(AppError::BadRequest(
                "评分不通过时跳转到自身需要配置「最大执行步数」或「最大 Token 数」兜底".to_string(),
            ));
        }
    }
    state
        .db
        .update_loop_step(
            sid,
            req.name.trim(),
            &req.description,
            req.step_id,
            &req.run_mode,
            req.skip_on_source_failed,
            req.min_rating,
            &req.unrated_policy,
            req.enabled,
            &req.on_success,
            req.success_goto_step_id,
            &req.on_rating_fail,
            req.fail_goto_step_id,
            &req.review_type,
        )
        .await?;
    let (_, todo_title, todo_executor) = state
        .db
        .list_loop_steps_with_todo_meta(loop_id)
        .await?
        .into_iter()
        .find(|(s, _, _)| s.id == sid)
        .ok_or_else(|| AppError::Internal("updated step missing".to_string()))?;
    Ok(ApiResponse::ok(LoopStepDto {
        step: state.db.get_loop_step(sid).await?.ok_or(AppError::Internal("step missing".to_string()))?.into(),
        todo_title,
        todo_executor,
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

/// POST /api/loops/{loop_id}/executions/{execution_id}/steps/{step_execution_id}/approve
/// 人工审批：对等待审批的环节执行记录打分并继续 loop 执行。
pub async fn approve_step_execution(
    State(state): State<AppState>,
    Path((_loop_id, execution_id, step_execution_id)): Path<(i64, i64, i64)>,
    Json(req): Json<ApproveStepExecutionRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 1. 校验评分范围
    if req.rating < 0 || req.rating > 100 {
        return Err(AppError::BadRequest("评分必须在 0-100 之间".to_string()));
    }

    // 2. 查询 step_execution 记录
    let step_execs = state.db.list_loop_step_executions(execution_id).await?;
    let step_exec = step_execs
        .iter()
        .find(|se| se.id == step_execution_id)
        .ok_or_else(|| AppError::NotFound)?;

    // 2.5. 校验 execution 归属于指定 loop_id，防止路径参数伪造
    // 先获取 loop_execution 记录，确认其 loop_id 与 URL 路径中的 _loop_id 一致
    let loop_exec = state
        .db
        .get_loop_execution(execution_id)
        .await?
        .ok_or_else(|| AppError::NotFound)?;
    if loop_exec.loop_id != _loop_id {
        return Err(AppError::BadRequest(
            "该 execution 不属于指定的 loop".to_string(),
        ));
    }

    // 3. 校验当前状态是 "pending_approval"
    if step_exec.status != "pending_approval" {
        return Err(AppError::BadRequest(
            "该环节当前不需要审批".to_string(),
        ));
    }

    // 4. 根据评分和阈值决定最终状态
    let min_rating = step_exec.min_rating.unwrap_or(0);
    let final_status = if req.rating >= min_rating { "success" } else { "failed" };

    // 5. 写入审批结果
    state
        .db
        .approve_step_execution(
            step_execution_id,
            req.rating,
            final_status,
            req.comment.as_deref(),
        )
        .await?;

    // 5b. 发送 WebSocket 事件触发前端刷新
    let _ = state.tx.send(crate::handlers::ExecEvent::ReviewStatusChanged {
        record_id: step_exec.execution_record_id.unwrap_or(0),
        todo_id: step_exec.todo_id,
        review_status: final_status.to_string(),
    });

    // 6. 尝试恢复 loop 执行
    let runner = state
        .loop_runner
        .as_ref()
        .ok_or_else(|| AppError::Internal("loop runner not ready".to_string()))?;
    runner.resume_loop_execution(execution_id).await;

    Ok(ApiResponse::ok(serde_json::json!({
        "step_execution_id": step_execution_id,
        "rating": req.rating,
        "status": final_status,
    })))
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
    // 批量查询各执行记录的待审批数量
    let exec_ids: Vec<i64> = records.iter().map(|r| r.id).collect();
    let pending_counts = state.db.count_pending_approvals_by_execution_ids(&exec_ids).await?;
    let mut items: Vec<LoopExecutionDto> = records.into_iter().map(Into::into).collect();
    // 填充 pending_approval_count 和 token_summary
    for item in &mut items {
        item.pending_approval_count = pending_counts.get(&item.id).copied().unwrap_or(0);
        // 加载 step_executions 并聚合 Token 消耗汇总
        let step_execs = state.db.list_loop_step_executions(item.id).await?;
        let mut enriched: Vec<LoopStepExecutionDto> = step_execs.into_iter().map(|se| {
            se.into()
        }).collect();
        // 从 execution_record.usage 填充 token 字段到 DTO
        for dto in &mut enriched {
            enrich_step_execution_with_usage(&state.db, dto).await;
        }
        // 直接从已 enrich 的 DTO 字段聚合，避免重复查询数据库
        item.token_summary = Some(aggregate_tokens_from_step_dtos(&enriched));
    }
    Ok(ApiResponse::ok(serde_json::json!({
        "items": items,
        "total": total,
        "page": page,
        "limit": limit,
    })))
}

/// 从 execution_record_id 读取 usage JSON 并解析为 LoopStepExecutionDto 的 token 字段。
/// usage 是 JSON 字符串，格式见 ExecutionUsage。
async fn enrich_step_execution_with_usage(
    db: &crate::db::Database,
    dto: &mut LoopStepExecutionDto,
) {
    // 只有关联了 execution_record 的 step 才有 usage 数据
    let record_id = match dto.execution_record_id {
        Some(id) => id,
        None => return,
    };
    let record = match db.get_execution_record(record_id).await {
        Ok(Some(r)) => r,
        _ => return,
    };
    // 从 execution_record.usage 字段解析 token 用量
    let usage = match record.usage {
        Some(u) => u,
        None => return,
    };
    // 转为 i64 送入 DTO（usage 字段是 u64），避免前端处理大数字溢出
    dto.input_tokens = Some(usage.input_tokens as i64);
    dto.output_tokens = Some(usage.output_tokens as i64);
    dto.cache_read_input_tokens = usage.cache_read_input_tokens.map(|v| v as i64);
    dto.cache_creation_input_tokens = usage.cache_creation_input_tokens.map(|v| v as i64);
    dto.total_cost_usd = usage.total_cost_usd;
}

/// 从已 enrich 的 LoopStepExecutionDto 字段直接聚合 Token 消耗汇总，
/// 不再重复查询数据库（原有的 aggregate_step_execution_tokens 存在 N+1 问题）。
/// 前置条件：调用方必须先通过 enrich_step_execution_with_usage 填充 DTO token 字段。
fn aggregate_tokens_from_step_dtos(step_execs: &[LoopStepExecutionDto]) -> LoopExecutionTokenSummary {
    let mut total_input_tokens: i64 = 0;
    let mut total_output_tokens: i64 = 0;
    let mut total_cache_read_input_tokens: i64 = 0;
    let mut total_cache_creation_input_tokens: i64 = 0;
    let mut total_cost_usd: f64 = 0.0;
    for se in step_execs {
        if let Some(v) = se.input_tokens {
            total_input_tokens += v;
        }
        if let Some(v) = se.output_tokens {
            total_output_tokens += v;
        }
        if let Some(v) = se.cache_read_input_tokens {
            total_cache_read_input_tokens += v;
        }
        if let Some(v) = se.cache_creation_input_tokens {
            total_cache_creation_input_tokens += v;
        }
        if let Some(v) = se.total_cost_usd {
            total_cost_usd += v;
        }
    }
    LoopExecutionTokenSummary {
        total_input_tokens,
        total_output_tokens,
        total_cache_read_input_tokens,
        total_cache_creation_input_tokens,
        total_cost_usd,
    }
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
    // 为每个 step execution 补充 step_name（来自 loop_steps 表）和 token 用量
    let mut enriched: Vec<LoopStepExecutionDto> = vec![];
    for se in step_execs {
        let mut dto: LoopStepExecutionDto = se.into();
        // 读取 loop_step 的名称（仅用于显示）
        if let Ok(Some(ls)) = state.db.get_loop_step(dto.step_id).await {
            dto.step_name = Some(ls.name);
        }
        // 从关联的 execution_record 读取 token 用量
        enrich_step_execution_with_usage(&state.db, &mut dto).await;
        enriched.push(dto);
    }
    // 聚合 token 汇总：直接从已 enrich 的 DTO 字段聚合，避免重复查询数据库
    let token_summary = aggregate_tokens_from_step_dtos(&enriched);
    Ok(ApiResponse::ok(LoopExecutionDetail {
        execution: exec.into(),
        step_executions: enriched,
        loop_name,
        token_summary,
    }))
}

// ====== 路由表 ======

pub fn loop_routes() -> axum::Router<AppState> {
    use axum::routing::{get, post, put};
    axum::Router::new()
        .route("/api/loops", get(list_loops).post(create_loop))
        .route("/api/loops/{id}", get(get_loop).put(update_loop).delete(delete_loop))
        .route("/api/loops/{id}/status", put(update_loop_status))
        .route("/api/loops/{id}/tags", put(update_loop_tags))
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
        .route("/api/loops/{id}/executions/{eid}/steps/{seid}/approve", post(approve_step_execution))
}
