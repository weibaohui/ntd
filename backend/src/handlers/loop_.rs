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
    http::header,
    response::IntoResponse,
    body::Bytes,
};
use serde::{Deserialize, Serialize};

use crate::handlers::{workspace_guard, AppError, AppState};
use crate::models::{
    self,
    ApiResponse, BatchCopyLoopWorkspaceRequest, BatchUpdateLoopWorkspaceRequest,
    BatchWorkspaceResult, CreateLoopRequest, CreateLoopStepRequest, CreateTriggerRequest,
    LoopDetail, LoopDto, LoopExecutionDetail, LoopExecutionDto, LoopExecutionTokenSummary,
    LoopListItem, LoopStepDto, LoopStepExecutionDto, LoopTriggerDto, ReorderLoopStepsRequest,
    UpdateLoopRequest, UpdateLoopStatusRequest, UpdateLoopStepRequest,
    UpdateTriggerRequest, ApproveStepExecutionRequest, UpdateTagsRequest, TriggerLoopRequest,
    ExportLoopSelectedRequest,
    LoopExportData, TagExportItem, ReviewTemplateExportItem, TodoExportItem,
    LoopExportItem, LoopTriggerExportItem, LoopStepExportItem,
    generate_pseudo_id, validate_pseudo_id,
    LoopImportPreviewResponse, LoopImportPreviewLoop, LoopImportSummary, LoopImportWarning,
    LoopImportResponse, LoopImportCreatedCounts,
};
use crate::db::ReviewTemplateInput;

const DEFAULT_PAGE_LIMIT: u64 = 20;
const MAX_PAGE_LIMIT: u64 = 100;

// ====== Loop 主体 ======

/// GET /api/loops — 左栏列表,一次查询带 trigger/step/exec 计数
pub async fn list_loops(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    // 筛选约定：必须用 workspace_id（唯一键）。前端 listLoops 已切到 id 过滤。
    let workspace_id: Option<i64> = params
        .get("workspace_id")
        .and_then(|s| s.parse().ok());
    let rows = state.db.list_loops_with_counts(workspace_id).await?;
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

/// /api/loops/stats 的查询参数:hours 缺省(None)= 全时段。
/// 用强类型 Option<u32> 而非 HashMap<String,String>:axum/serde 反序列化时,
/// 非法值(abc/负数/溢出)会自动 400,而非静默降级为全时段——隐藏输入错误比报错更危险。
// pub:handler 是 pub,签名暴露的类型必须至少同等可见,否则 clippy 报「私有类型暴露在公开接口」。
#[derive(Deserialize)]
pub struct LoopStatsQuery {
    pub hours: Option<u32>,
}

/// GET /api/loops/stats?hours=N — 全 loop 聚合统计(dashboard「自动化」Tab)。
/// hours 缺省表示全时段。一次返回 loop 规模/成功率/触发器分布/Token。
pub async fn get_loop_stats(
    State(state): State<AppState>,
    Query(params): Query<LoopStatsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let stats = state.db.get_loop_stats(params.hours).await?;
    Ok(ApiResponse::ok(stats))
}

/// POST /api/loops — 新建 loop,status 强制为 paused
pub async fn create_loop(
    State(state): State<AppState>,
    Json(req): Json<CreateLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    // 工作空间必填且必须存在：handler 强制把 id 解析为 path 后再下传 DAO，
    // 避免 DAO 再做路径反查，也保证 workspace_id / workspace_path 双字段同步写入。
    let workspace = state
        .db
        .get_project_directory_by_id(req.workspace_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id)))?;
    // 创建环路前校验标签约束：环路只能选择一个标签
    if req.tag_ids.len() > 1 {
        return Err(AppError::BadRequest("环路只能选择一个标签".to_string()));
    }
    let created = state
        .db
        .create_loop(
            req.name.trim(),
            &req.description,
            Some(req.workspace_id),
            Some(workspace.path.as_str()),
            req.webhook_enabled,
            &req.icon,
            req.review_template_id,
            req.limits_config.as_deref(),
            req.abnormal_handler_todo_id,
            &req.abnormal_handler_trigger_on,
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
    // 工作空间切换是可选的；只有显式传 workspace_id 才查 path，保证 cwd 与筛选键同步更新。
    // 不传则保留原工作空间不变——避免误清空。
    let mut workspace_id: Option<i64> = req.workspace_id;
    let mut workspace_path: Option<String> = None;
    if let Some(wid) = workspace_id {
        let dir = state
            .db
            .get_project_directory_by_id(wid)
            .await?
            .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", wid)))?;
        workspace_path = Some(dir.path);
    }
    // 把外部传入 id 再带回去，DAO 内部用 id 写入并保持 cwd 字段同步
    let _ = workspace_id.take();
    let workspace_id_for_dao: Option<i64> = req.workspace_id;
    state
        .db
        .update_loop(
            id,
            req.name.trim(),
            &req.description,
            workspace_id_for_dao,
            workspace_path.as_deref(),
            req.webhook_enabled,
            &req.icon,
            req.review_template_id,
            req.limits_config.as_deref(),
            req.abnormal_handler_todo_id,
            &req.abnormal_handler_trigger_on,
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
    Json(req): Json<TriggerLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    let dispatcher = state
        .loop_trigger_dispatcher
        .as_ref()
        .ok_or_else(|| AppError::Internal("loop dispatcher not ready".to_string()))?;
    // 将 params 存入 trigger_meta，供后续 step prompt 替换使用
    let trigger_meta = serde_json::json!({
        "source": "manual",
        "params": req.params,
    });
    match dispatcher.dispatch_manual_with_meta(id, trigger_meta).await {
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
        .map(|(s, todo_title, todo_executor, todo_archived_at)| LoopStepDto {
            step: s.into(),
            todo_title,
            todo_executor,
            todo_archived_at,
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
    // Loop 编排的节点必须是 todo，按 todo_id 校验存在性。
    state
        .db
        .get_todo(req.todo_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("todo #{} 不存在", req.todo_id)))?;
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
            &req.on_success,
            req.success_goto_step_id,
            &req.on_rating_fail,
            req.fail_goto_step_id,
            &req.review_type,
        )
        .await?;
    let (_, todo_title, todo_executor, todo_archived_at) = state
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
            todo_archived_at,
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
    // 与 create_loop_step 一致: 切换 todo_id 时也必须指向有效的 todo。
    if req.todo_id != step.todo_id {
        state
            .db
            .get_todo(req.todo_id)
            .await?
            .ok_or_else(|| AppError::BadRequest(format!("todo #{} 不存在", req.todo_id)))?;
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
            req.todo_id,
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
    let (_, todo_title, todo_executor, todo_archived_at) = state
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
        todo_archived_at,
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
    // AppError::NotFound 是单元变体，不捕获变量——用 ok_or 直接构造更简洁
    let step_exec = step_execs
        .iter()
        .find(|se| se.id == step_execution_id)
        .ok_or(AppError::NotFound)?;

    // 2.5. 校验 execution 归属于指定 loop_id，防止路径参数伪造
    // 先获取 loop_execution 记录，确认其 loop_id 与 URL 路径中的 _loop_id 一致
    // AppError::NotFound 是单元变体，不捕获变量——用 ok_or 直接构造更简洁
    let loop_exec = state
        .db
        .get_loop_execution(execution_id)
        .await?
        .ok_or(AppError::NotFound)?;
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
    let _ = state.tx.send(crate::executor_service::ExecEvent::ReviewStatusChanged {
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
    /// 按最近 N 小时过滤（对 started_at 生效）；不传或 0 表示不过滤。
    #[serde(default)]
    pub hours: Option<u32>,
}

pub async fn list_executions(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    Query(q): Query<ExecutionPageQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(DEFAULT_PAGE_LIMIT).min(MAX_PAGE_LIMIT);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * limit;
    let records = state.db.list_loop_executions(loop_id, limit, offset, q.hours).await?;
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
        // step_id=-1 是异常处理步骤，没有对应的 loop_step，用 todo 标题代替
        if dto.step_id == -1 {
            if let Ok(Some(todo)) = state.db.get_todo(dto.todo_id).await {
                dto.step_name = Some(format!("[异常处理] {}", todo.title));
            }
        } else if let Ok(Some(ls)) = state.db.get_loop_step(dto.step_id).await {
            // 读取 loop_step 的名称（仅用于显示）
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

/// 通过执行 ID 直接获取执行详情（无需 loop_id），供消息历史中 "处理类型" 列跳转使用。
pub async fn get_execution_by_id(
    State(state): State<AppState>,
    Path(eid): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let exec = state
        .db
        .get_loop_execution(eid)
        .await?
        .ok_or(AppError::NotFound)?;
    let loop_id = exec.loop_id;
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
    let mut enriched: Vec<LoopStepExecutionDto> = vec![];
    for se in step_execs {
        let mut dto: LoopStepExecutionDto = se.into();
        if dto.step_id == -1 {
            if let Ok(Some(todo)) = state.db.get_todo(dto.todo_id).await {
                dto.step_name = Some(format!("[异常处理] {}", todo.title));
            }
        } else if let Ok(Some(ls)) = state.db.get_loop_step(dto.step_id).await {
            dto.step_name = Some(ls.name);
        }
        enrich_step_execution_with_usage(&state.db, &mut dto).await;
        enriched.push(dto);
    }
    let token_summary = aggregate_tokens_from_step_dtos(&enriched);
    Ok(ApiResponse::ok(LoopExecutionDetail {
        execution: exec.into(),
        step_executions: enriched,
        loop_name,
        token_summary,
    }))
}

// ====== 批量 workspace 操作 ======

/// PUT /api/loops/batch-workspace — 批量移动环路到其他工作空间
pub async fn batch_move_loops_workspace(
    State(state): State<AppState>,
    Json(req): Json<BatchUpdateLoopWorkspaceRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    // 工作空间 id 必填且必须存在；handler 在此层把 id 解析为 path，
    // DAO 一次写入 workspace_id + workspace_path 两列保证双字段同步。
    let dir = state
        .db
        .get_project_directory_by_id(req.workspace_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id)))?;
    let rows_affected = state
        .db
        .batch_update_loops_workspace(&req.ids, req.workspace_id, &dir.path)
        .await?;
    Ok(ApiResponse::ok(BatchWorkspaceResult {
        updated_count: rows_affected as i64,
        total: req.ids.len() as i64,
    }))
}

/// POST /api/loops/batch-copy-workspace — 批量复制环路到其他工作空间
pub async fn batch_copy_loops_workspace(
    State(state): State<AppState>,
    Json(req): Json<BatchCopyLoopWorkspaceRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    let dir = state
        .db
        .get_project_directory_by_id(req.workspace_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id)))?;
    let created_ids = state
        .db
        .batch_copy_loops_to_workspace(&req.ids, req.workspace_id, &dir.path)
        .await?;
    Ok(ApiResponse::ok(BatchWorkspaceResult {
        updated_count: created_ids.len() as i64,
        total: req.ids.len() as i64,
    }))
}

// ====== Loop 导入导出 ======

/// GET /api/loops/{id}/export — 导出单个环路为 YAML 文件
pub async fn export_loop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let loops = state.db.list_loops_with_counts(None).await?;
    let loop_row = loops.into_iter().find(|l| l.loop_.id == id);

    // 检查环路是否存在，用 ok_or 替代 is_none + unwrap 的两步写法
    let loop_ = loop_row.ok_or(AppError::NotFound)?.loop_;
    let yaml = build_loop_export_yaml(&state, &[id]).await?;
    let filename = format!("{}-{}.loop.yaml",
        loop_.name.replace(' ', "-"),
        chrono::Utc::now().format("%Y%m%d-%H%M%S"));

    let disposition = format!("attachment; filename=\"{}\"", filename);
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-yaml; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        yaml,
    ))
}

/// POST /api/loops/export-selected — 批量导出选中的环路
pub async fn export_selected_loops(
    State(state): State<AppState>,
    Json(req): Json<ExportLoopSelectedRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.loop_ids.is_empty() {
        return Err(AppError::BadRequest("loop_ids 不能为空".to_string()));
    }
    let yaml = build_loop_export_yaml(&state, &req.loop_ids).await?;
    let filename = format!("loops-export-{}.loop.yaml",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"));

    let disposition = format!("attachment; filename=\"{}\"", filename);
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-yaml; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        yaml,
    ))
}

/// GET /api/loops/export — 导出全库所有环路为单个 YAML，对齐 Todo `GET /api/backup/export`。
pub async fn export_all_loops(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    // list_loops_with_counts(None) 不按工作空间过滤，取全库 loop
    let loops = state.db.list_loops_with_counts(None).await?;
    let ids: Vec<i64> = loops.into_iter().map(|l| l.loop_.id).collect();
    if ids.is_empty() {
        return Err(AppError::BadRequest("当前没有任何环路可导出".to_string()));
    }
    let yaml = build_loop_export_yaml(&state, &ids).await?;
    let filename = format!("loops-export-{}.loop.yaml",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    let disposition = format!("attachment; filename=\"{}\"", filename);
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-yaml; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        yaml,
    ))
}

// ====== 导入工作空间逐 loop 解析 helpers ======
// 镜像 db/todo.rs 的 merge_backup 解析模式：override > 全局 > 导出 id（重新解析）> 哨兵 0。
// loop 导入 handler 不在事务内执行，无法复用 db/todo.rs 的事务版 resolve_workspace_pair，
// 这里走非事务连接（state.db.get_project_directory_by_id）。

/// 单条 loop 解析后的工作空间归属（id + path）；ws_id == 0 表示未匹配（哨兵）。
struct LoopResolved {
    name: String,
    ws_id: i64,
    ws_path: Option<String>,
}

/// 按 id 在当前库解析工作空间 (id, path, name)；None = id 不存在或入参为 None。
async fn resolve_loop_workspace(
    db: &crate::db::Database,
    id: Option<i64>,
) -> Result<Option<(i64, String, Option<String>)>, AppError> {
    // 入参 None：备份里就没有工作空间信息，直接返回 None
    let Some(id) = id else { return Ok(None) };
    let dir = db.get_project_directory_by_id(id).await
        .map_err(|e| AppError::Internal(format!("lookup workspace {}: {}", id, e)))?;
    Ok(dir.map(|d| (d.id, d.path, d.name)))
}

/// 解析单条 loop 的 (id, path)，优先级：override > 全局 > 导出 id > 哨兵 0。
/// path 与 id 成对写出，避免「id 指向 B、cwd 仍是备份源路径」的错配。
async fn resolve_loop_ws_pair(
    db: &crate::db::Database,
    loop_export: &LoopExportItem,
    global_ws: Option<&(i64, String)>,
    override_id: Option<i64>,
) -> Result<(i64, Option<String>), AppError> {
    // 1) 用户 per-loop 覆盖优先
    if let Some((id, path, _)) = resolve_loop_workspace(db, override_id).await? {
        return Ok((id, Some(path)));
    }
    // 2) 全局覆盖（向后兼容，req.workspace_id）
    if let Some((id, path)) = global_ws {
        return Ok((*id, Some(path.clone())));
    }
    // 3) 导出文件里的 workspace_id 重新解析（跨环境更可靠）
    if let Some((id, path, _)) = resolve_loop_workspace(db, loop_export.workspace_id).await? {
        return Ok((id, Some(path)));
    }
    // 4) 悬空 → 哨兵 0（workspace_id 列 NOT NULL DEFAULT 0，0=未分配）
    Ok((0, None))
}

/// 解析所有 loop 的工作空间，返回每条 (name, id, path)，供 todo 映射与 loop 创建复用。
async fn resolve_all_loops(
    db: &crate::db::Database,
    data: &LoopExportData,
    global_ws: Option<&(i64, String)>,
    overrides: &std::collections::HashMap<String, i64>,
) -> Result<Vec<LoopResolved>, AppError> {
    let mut out = Vec::with_capacity(data.loops.len());
    for l in &data.loops {
        let ov = overrides.get(&l.name).copied();
        let (id, path) = resolve_loop_ws_pair(db, l, global_ws, ov).await?;
        out.push(LoopResolved { name: l.name.clone(), ws_id: id, ws_path: path });
    }
    Ok(out)
}

/// gate：任一 loop 解析为 0（未匹配）→ 报错列出未匹配 loop 名，阻止导入。
fn gate_unmatched_loops(resolved: &[LoopResolved]) -> Result<(), AppError> {
    let unmatched: Vec<&str> = resolved.iter()
        .filter(|r| r.ws_id == 0)
        .map(|r| r.name.as_str())
        .collect();
    if unmatched.is_empty() {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "以下环路未匹配工作空间，请逐个指定: {}", unmatched.join(", ")
        )))
    }
}

/// data.todos 的 pseudo id → title 映射，供共享 todo warning 用。
fn todo_title_map(data: &LoopExportData) -> std::collections::HashMap<String, String> {
    data.todos.iter().map(|t| (t.id.clone(), t.title.clone())).collect()
}

/// todo→工作空间归属的累积器：记录每条 todo 首次归属的 loop，并对跨 loop 共享打 warning。
struct TodoWsAccumulator {
    todo_ws: std::collections::HashMap<String, (i64, Option<String>)>,
    first_loop: std::collections::HashMap<String, String>,
    warned: std::collections::HashSet<String>,
}

impl TodoWsAccumulator {
    fn new() -> Self {
        Self {
            todo_ws: std::collections::HashMap::new(),
            first_loop: std::collections::HashMap::new(),
            warned: std::collections::HashSet::new(),
        }
    }

    /// 记录一条 todo 的归属；返回 true 表示它已被更早的 loop 引用（跨 loop 共享）。
    fn record(&mut self, todo_id: &str, loop_name: &str, pair: (i64, Option<String>)) -> bool {
        use std::collections::hash_map::Entry;
        match self.todo_ws.entry(todo_id.to_string()) {
            // 空缺：首次归属，记下首个 loop 名
            Entry::Vacant(e) => {
                e.insert(pair);
                self.first_loop.insert(todo_id.to_string(), loop_name.to_string());
                false
            }
            // 已被更早 loop 归属 → 共享，不覆盖原归属
            Entry::Occupied(_) => true,
        }
    }

    /// 对跨 loop 共享的 todo 打一次 warning（同一条 todo 只打一次）。
    fn warn_shared(
        &mut self,
        warnings: &mut Vec<LoopImportWarning>,
        titles: &std::collections::HashMap<String, String>,
        todo_id: &str,
        fallback_title: &str,
    ) {
        // warned 只记一次，避免同一共享 todo 被多个后续 loop 重复 warning
        if self.warned.insert(todo_id.to_string()) {
            let title = titles.get(todo_id).map(String::as_str).unwrap_or(fallback_title);
            let first = self.first_loop.get(todo_id).cloned().unwrap_or_default();
            warnings.push(LoopImportWarning {
                warning_type: "shared_todo_workspace".to_string(),
                message: format!("Todo「{}」被多个环路引用，归到首个环路「{}」的工作空间", title, first),
            });
        }
    }
}

/// 为每条 todo pseudo 决定工作空间 (id, path)：取首个引用它的 loop 的工作空间。
fn build_todo_workspace_map(
    data: &LoopExportData,
    resolved_loops: &[LoopResolved],
    warnings: &mut Vec<LoopImportWarning>,
) -> std::collections::HashMap<String, (i64, Option<String>)> {
    let mut acc = TodoWsAccumulator::new();
    let titles = todo_title_map(data);
    for (i, l) in data.loops.iter().enumerate() {
        // pair 取自该 loop 的解析结果；step-todo 与 abnormal-handler 都跟随所属 loop
        let pair = (resolved_loops[i].ws_id, resolved_loops[i].ws_path.clone());
        for step in &l.steps {
            if acc.record(&step.todo_id, &l.name, pair.clone()) {
                acc.warn_shared(warnings, &titles, &step.todo_id, &step.todo_title);
            }
        }
        if let Some(hid) = &l.abnormal_handler_todo_id {
            let title = l.abnormal_handler_todo_title.as_deref().unwrap_or("");
            if acc.record(hid, &l.name, pair.clone()) {
                acc.warn_shared(warnings, &titles, hid, title);
            }
        }
    }
    acc.todo_ws
}

/// 预览：按导出原 id 解析每条 loop 的默认匹配情况（不接收用户 override）。
async fn build_preview_loops(
    db: &crate::db::Database,
    data: &LoopExportData,
) -> Result<Vec<LoopImportPreviewLoop>, AppError> {
    let mut out = Vec::with_capacity(data.loops.len());
    for l in &data.loops {
        let resolved = resolve_loop_workspace(db, l.workspace_id).await?;
        // 先取 matched 再 match 消费 resolved，避免部分移动后无法 is_some()
        let source_matched = resolved.is_some();
        let (rid, rname) = match resolved {
            Some((id, _, name)) => (id, name),
            None => (0, None),
        };
        out.push(LoopImportPreviewLoop {
            name: l.name.clone(),
            workspace_id: l.workspace_id,
            workspace_path: l.workspace_path.clone(),
            resolved_workspace_id: rid,
            resolved_workspace_name: rname,
            source_matched,
        });
    }
    Ok(out)
}

/// POST /api/loops/import/preview — 预览导入数据
pub async fn import_preview(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let yaml_str = String::from_utf8(body.to_vec())
        .map_err(|_| AppError::BadRequest("Invalid UTF-8 in request body".to_string()))?;

    // 解析 YAML
    let data: LoopExportData = serde_yaml::from_str(&yaml_str)
        .map_err(|e| AppError::BadRequest(format!("Invalid YAML: {}", e)))?;

    // 基本校验
    if data.loops.is_empty() {
        return Err(AppError::BadRequest("No loops in export file".to_string()));
    }

    // 收集所有伪ID并检查格式和唯一性
    let mut pseudo_ids: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // 收集标签伪ID
    for tag in &data.tags {
        if !validate_pseudo_id(&tag.id) {
            errors.push(format!("Invalid pseudo-ID format: {}", tag.id));
        }
        pseudo_ids.push(tag.id.clone());
    }

    // 收集评审模板伪ID
    for tmpl in &data.review_templates {
        if !validate_pseudo_id(&tmpl.id) {
            errors.push(format!("Invalid pseudo-ID format: {}", tmpl.id));
        }
        pseudo_ids.push(tmpl.id.clone());
    }

    // 收集Todo伪ID
    for todo in &data.todos {
        if !validate_pseudo_id(&todo.id) {
            errors.push(format!("Invalid pseudo-ID format: {}", todo.id));
        }
        pseudo_ids.push(todo.id.clone());
    }

    // 收集环路伪ID
    for loop_ in &data.loops {
        if !validate_pseudo_id(&loop_.id) {
            errors.push(format!("Invalid pseudo-ID format: {}", loop_.id));
        }
        pseudo_ids.push(loop_.id.clone());

        // 收集触发器伪ID
        for trigger in &loop_.triggers {
            if !validate_pseudo_id(&trigger.id) {
                errors.push(format!("Invalid pseudo-ID format: {}", trigger.id));
            }
            pseudo_ids.push(trigger.id.clone());
        }

        // 收集步骤伪ID
        for step in &loop_.steps {
            if !validate_pseudo_id(&step.id) {
                errors.push(format!("Invalid pseudo-ID format: {}", step.id));
            }
            pseudo_ids.push(step.id.clone());
        }
    }

    // 检查伪ID唯一性
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for id in &pseudo_ids {
        if !seen.insert(id.clone()) {
            errors.push(format!("Duplicate pseudo-ID: {}", id));
        }
    }

    // 检查引用完整性
    let all_tag_ids: std::collections::HashSet<_> = data.tags.iter().map(|t| t.id.clone()).collect();
    let all_template_ids: std::collections::HashSet<_> = data.review_templates.iter().map(|t| t.id.clone()).collect();
    let all_todo_ids: std::collections::HashSet<_> = data.todos.iter().map(|t| t.id.clone()).collect();
    let all_step_ids: std::collections::HashSet<_> = data.loops.iter()
        .flat_map(|l| l.steps.iter().map(|s| s.id.clone()))
        .collect();

    for loop_ in &data.loops {
        // 检查环路引用的评审模板
        if let Some(ref tid) = loop_.review_template_id {
            if !all_template_ids.contains(tid) && !tid.is_empty() {
                errors.push(format!("Loop '{}' references non-existent template: {}", loop_.name, tid));
            }
        }

        // 检查异常处理Todo
        if let Some(ref tid) = loop_.abnormal_handler_todo_id {
            if !all_todo_ids.contains(tid) && !tid.is_empty() {
                errors.push(format!("Loop '{}' references non-existent abnormal handler: {}", loop_.name, tid));
            }
        }

        // 检查标签
        for tid in &loop_.tag_ids {
            if !all_tag_ids.contains(tid) && !tid.is_empty() {
                errors.push(format!("Loop '{}' references non-existent tag: {}", loop_.name, tid));
            }
        }

        for step in &loop_.steps {
            // 检查步骤引用的Todo
            if !all_todo_ids.contains(&step.todo_id) {
                errors.push(format!("Step '{}' in loop '{}' references non-existent todo: {}", step.name, loop_.name, step.todo_id));
            }

            // 检查goto跳转
            if let Some(ref sid) = step.success_goto_step_id {
                if !all_step_ids.contains(sid) && !sid.is_empty() {
                    errors.push(format!("Step '{}' success_goto references non-existent step: {}", step.name, sid));
                }
            }
            if let Some(ref sid) = step.fail_goto_step_id {
                if !all_step_ids.contains(sid) && !sid.is_empty() {
                    errors.push(format!("Step '{}' fail_goto references non-existent step: {}", step.name, sid));
                }
            }
        }
    }

    // 检查Todo引用的评审模板
    for todo in &data.todos {
        if let Some(ref tid) = todo.review_template_id {
            if !all_template_ids.contains(tid) && !tid.is_empty() {
                errors.push(format!("Todo '{}' references non-existent template: {}", todo.title, tid));
            }
        }
        for tid in &todo.tag_ids {
            if !all_tag_ids.contains(tid) && !tid.is_empty() {
                errors.push(format!("Todo '{}' references non-existent tag: {}", todo.title, tid));
            }
        }
    }

    let valid = errors.is_empty();

    // 构建摘要
    let summary = LoopImportSummary {
        loops: data.loops.len(),
        steps: data.loops.iter().map(|l| l.steps.len()).sum(),
        todos: data.todos.len(),
        review_templates: data.review_templates.len(),
        tags: data.tags.len(),
        triggers: data.loops.iter().map(|l| l.triggers.len()).sum(),
    };

    // 生成警告（根据方案，cron触发器将被禁用）
    let mut warnings: Vec<LoopImportWarning> = Vec::new();
    for loop_ in &data.loops {
        for trigger in &loop_.triggers {
            if trigger.trigger_type == "cron" && trigger.enabled {
                warnings.push(LoopImportWarning {
                    warning_type: "cron_trigger_disabled".to_string(),
                    message: format!("Loop '{}' cron trigger will be disabled after import", loop_.name),
                });
            }
        }
    }

    let loops = build_preview_loops(state.db.as_ref(), &data).await?;

    let response = LoopImportPreviewResponse {
        valid,
        pseudo_ids,
        summary,
        conflicts: vec![],  // 新建模式无冲突
        warnings,
        loops,
    };

    Ok(ApiResponse::ok(response))
}

/// POST /api/loops/import — 执行导入（新建模式）
#[derive(Deserialize)]
pub struct ImportLoopRequest {
    pub yaml: String,
    /// 全局目标工作空间（向后兼容）；None 时按 per-loop 解析。
    #[serde(default)]
    pub workspace_id: Option<i64>,
    /// per-loop 覆盖：loop name → workspace_id（前端逐条选择）。
    #[serde(default)]
    pub workspace_overrides: Option<std::collections::HashMap<String, i64>>,
}

pub async fn import_loops(
    State(state): State<AppState>,
    Json(req): Json<ImportLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 解析 YAML
    let data: LoopExportData = serde_yaml::from_str(&req.yaml)
        .map_err(|e| AppError::BadRequest(format!("Invalid YAML: {}", e)))?;

    // 基本校验
    if data.loops.is_empty() {
        return Err(AppError::BadRequest("No loops in export file".to_string()));
    }

    // 全局目标工作空间（可选，向后兼容）；None 时按 per-loop 解析
    let global_ws: Option<(i64, String)> = match req.workspace_id {
        Some(id) => {
            let dir = state.db.get_project_directory_by_id(id).await?
                .ok_or_else(|| AppError::BadRequest(format!("Workspace {} not found", id)))?;
            Some((dir.id, dir.path))
        }
        None => None,
    };
    let overrides = req.workspace_overrides.unwrap_or_default();

    // 逐 loop 解析工作空间，gate 未匹配；建 todo→ws 映射（共享 todo 取首 loop ws）
    let resolved_loops = resolve_all_loops(state.db.as_ref(), &data, global_ws.as_ref(), &overrides).await?;
    gate_unmatched_loops(&resolved_loops)?;
    let mut warnings: Vec<LoopImportWarning> = Vec::new();
    let todo_ws = build_todo_workspace_map(&data, &resolved_loops, &mut warnings);

    // 评审模板按 name 匹配、跨 loop 复用，统一归到首个 loop 的工作空间（有意简化）
    let template_ws_id = global_ws.map(|(id, _)| id)
        .or_else(|| resolved_loops.first().map(|r| r.ws_id))
        .unwrap_or(0);

    // 构建伪ID -> 真实ID 的映射表
    let mut tag_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut template_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut todo_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut loop_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut step_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

    let mut created_counts = LoopImportCreatedCounts {
        loops: 0,
        todos: 0,
        review_templates: 0,
        tags: 0,
        triggers: 0,
        steps: 0,
    };

    // 阶段1: 导入标签
    for tag in &data.tags {
        let tag_id = state.db.create_tag(&tag.name, &tag.color).await?;
        tag_pseudo_to_real.insert(tag.id.clone(), tag_id);
        created_counts.tags += 1;
    }

    // 阶段2: 导入评审模板（统一归到首个 loop 的工作空间，见 template_ws_id）
    for tmpl in &data.review_templates {
        let input = ReviewTemplateInput {
            name: tmpl.name.clone(),
            description: tmpl.description.clone(),
            prompt: tmpl.prompt.clone(),
            workspace_id: Some(template_ws_id),
        };
        let new_tmpl = state.db.create_review_template(&input).await?;
        template_pseudo_to_real.insert(tmpl.id.clone(), new_tmpl);
        created_counts.review_templates += 1;
    }

    // 阶段3: 导入Todo模板（完整字段导入）；workspace 取 todo_ws 映射（跟随所属 loop）
    for todo in &data.todos {
        // 解析 kind 字符串 → i32（导出时存为字符串以便序列化）
        let kind: Option<i32> = todo.kind.parse().ok().or(Some(0));
        // 解析 review_template_id 伪ID → 真实ID
        let real_review_template_id = todo.review_template_id.as_ref()
            .and_then(|tid| template_pseudo_to_real.get(tid))
            .copied();
        // 该 todo 归属的工作空间 (id, path)；未在映射里（无 step/abnormal 引用）退化为哨兵 0
        let (todo_ws_id, todo_ws_path) = todo_ws.get(&todo.id).cloned().unwrap_or((0, None));
        let new_todo_id = state.db.create_todo_for_import(
            &todo.title,
            &todo.prompt,
            todo.executor.as_deref(),
            todo.acceptance_criteria.as_deref(),
            todo.webhook_enabled,
            todo_ws_id,
            todo_ws_path.as_deref().unwrap_or(""),
            Some(&todo.status),
            Some(todo.scheduler_enabled),
            Some(todo.auto_review_enabled),
            real_review_template_id,
            kind,
        ).await?;

        todo_pseudo_to_real.insert(todo.id.clone(), new_todo_id);
        created_counts.todos += 1;

        // 关联标签（异常处理Todo不关联标签）
        if !todo.is_abnormal_handler {
            for (pseudo_tag_id, _) in todo.tag_ids.iter().zip(todo.tag_names.iter()) {
                if let Some(&real_tag_id) = tag_pseudo_to_real.get(pseudo_tag_id) {
                    state.db.add_todo_tag(new_todo_id, real_tag_id).await?;
                }
            }
        }
    }

    // 阶段4: 导入环路主体；workspace 用该 loop 解析出的 (id, path)
    for (i, loop_export) in data.loops.iter().enumerate() {
        let resolved = &resolved_loops[i];
        let review_template_id = loop_export.review_template_id.as_ref()
            .and_then(|tid| template_pseudo_to_real.get(tid))
            .copied();

        let abnormal_handler_todo_id = loop_export.abnormal_handler_todo_id.as_ref()
            .and_then(|tid| todo_pseudo_to_real.get(tid))
            .copied();

        let limits_config = serde_json::to_string(&loop_export.limits_config)
            .unwrap_or_else(|_| "{}".to_string());
        let abnormal_handler_trigger_on = serde_json::to_string(&loop_export.abnormal_handler_trigger_on)
            .unwrap_or_else(|_| "[]".to_string());

        // 名称追加 "-导入" 后缀
        let loop_name = format!("{}-导入", loop_export.name);

        let new_loop = state.db.create_loop(
            &loop_name,
            &loop_export.description,
            Some(resolved.ws_id),
            resolved.ws_path.as_deref(),
            loop_export.webhook_enabled,
            &loop_export.icon,
            review_template_id,
            Some(&limits_config),
            abnormal_handler_todo_id,
            &abnormal_handler_trigger_on,
        ).await?;

        loop_pseudo_to_real.insert(loop_export.id.clone(), new_loop.id);
        created_counts.loops += 1;

        // 关联标签 - 先收集所有 tag_id，再一次性写入，避免 set_loop_tags 全量替换导致只保留最后一个
        let tag_ids: Vec<i64> = loop_export.tag_ids.iter()
            .filter_map(|pseudo_tag_id| tag_pseudo_to_real.get(pseudo_tag_id).copied())
            .collect();
        if !tag_ids.is_empty() {
            state.db.set_loop_tags(new_loop.id, &tag_ids).await?;
        }

        // 阶段5: 导入触发器
        for trigger in &loop_export.triggers {
            let _new_trigger = state.db.create_trigger(
                new_loop.id,
                &trigger.trigger_type,
                &serde_json::to_string(&trigger.config).unwrap_or_else(|_| "{}".to_string()),
                trigger.enabled,
                trigger.priority,
            ).await?;
            // 注意: cron触发器在导出时已被禁用，这里保持原状态
            created_counts.triggers += 1;
        }

        // 阶段6: 导入步骤（第一遍：创建所有步骤）
        for step in &loop_export.steps {
            let todo_id = todo_pseudo_to_real.get(&step.todo_id)
                .copied()
                .ok_or_else(|| AppError::BadRequest(format!("Step '{}' references unknown todo", step.name)))?;

            let new_step = state.db.create_loop_step(
                new_loop.id,
                &step.name,
                &step.description,
                todo_id,
                &step.run_mode,
                step.skip_on_source_failed,
                step.min_rating,
                &step.unrated_policy,
                step.enabled,
                &step.on_success,
                None,  // success_goto 第二遍再补
                &step.on_rating_fail,
                None,  // fail_goto 第二遍再补
                &step.review_type,
            ).await?;

            step_pseudo_to_real.insert(step.id.clone(), new_step.id);
            created_counts.steps += 1;
        }

        // 阶段7: 第二遍修复 goto 引用
        for step in &loop_export.steps {
            if let Some(&new_step_id) = step_pseudo_to_real.get(&step.id) {
                let success_goto = step.success_goto_step_id.as_ref()
                    .and_then(|sid| step_pseudo_to_real.get(sid))
                    .copied();
                let fail_goto = step.fail_goto_step_id.as_ref()
                    .and_then(|sid| step_pseudo_to_real.get(sid))
                    .copied();

                if success_goto.is_some() || fail_goto.is_some() {
                    state.db.update_loop_step_goto(new_step_id, success_goto, fail_goto).await?;
                }
            }
        }
    }

    // 刷新scheduler以加载新的cron触发器
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.reload_all().await;
    }

    let response = LoopImportResponse {
        success: true,
        created: created_counts,
        warnings,
    };

    Ok(ApiResponse::ok(response))
}

/// POST /api/loops/merge — 执行合并导入
#[derive(Deserialize)]
pub struct MergeLoopRequest {
    pub yaml: String,
    /// 全局目标工作空间（向后兼容）；None 时按 per-loop 解析。
    #[serde(default)]
    pub workspace_id: Option<i64>,
    /// per-loop 覆盖：loop name → workspace_id（前端逐条选择）。
    #[serde(default)]
    pub workspace_overrides: Option<std::collections::HashMap<String, i64>>,
    /// 用户选择「跳过」的同名环路名集合：这些环路不会被创建/覆盖，同名保留原样。
    #[serde(default)]
    pub skip_names: Vec<String>,
    /// 冲突解决策略（已废弃：统一为覆盖语义，对齐 Todo；字段保留向后兼容，不再生效）。
    #[serde(default)]
    pub conflict_resolution: std::collections::HashMap<String, String>,
}

#[derive(Serialize)]
pub struct MergeLoopResponse {
    pub success: bool,
    pub created: LoopImportCreatedCounts,
    pub updated: LoopImportCreatedCounts,
    pub skipped: Vec<String>,
    pub warnings: Vec<LoopImportWarning>,
}

pub async fn merge_loops(
    State(state): State<AppState>,
    Json(req): Json<MergeLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 解析 YAML
    let mut data: LoopExportData = serde_yaml::from_str(&req.yaml)
        .map_err(|e| AppError::BadRequest(format!("Invalid YAML: {}", e)))?;

    // 基本校验
    if data.loops.is_empty() {
        return Err(AppError::BadRequest("No loops in export file".to_string()));
    }

    // 全局目标工作空间（可选，向后兼容）；None 时按 per-loop 解析
    let global_ws: Option<(i64, String)> = match req.workspace_id {
        Some(id) => {
            let dir = state.db.get_project_directory_by_id(id).await?
                .ok_or_else(|| AppError::BadRequest(format!("Workspace {} not found", id)))?;
            Some((dir.id, dir.path))
        }
        None => None,
    };
    let overrides = req.workspace_overrides.unwrap_or_default();

    // 用户选择「跳过」的同名环路：记录名字后从待处理列表移除——
    // 不 resolve、不 gate、不创建，同名保留原样。其引用的 todo/模板/标签仍按全局资源合并。
    let skip_set: std::collections::HashSet<&str> =
        req.skip_names.iter().map(|s| s.as_str()).collect();
    let skipped: Vec<String> = data.loops.iter()
        .filter(|l| skip_set.contains(l.name.as_str()))
        .map(|l| l.name.clone())
        .collect();
    if !skip_set.is_empty() {
        data.loops.retain(|l| !skip_set.contains(l.name.as_str()));
    }

    // 逐 loop 解析工作空间，gate 未匹配；建 todo→ws 映射（共享 todo 取首 loop ws）
    let resolved_loops = resolve_all_loops(state.db.as_ref(), &data, global_ws.as_ref(), &overrides).await?;
    gate_unmatched_loops(&resolved_loops)?;
    let mut warnings: Vec<LoopImportWarning> = Vec::new();
    let todo_ws = build_todo_workspace_map(&data, &resolved_loops, &mut warnings);

    // 评审模板按 name 匹配、跨 loop 复用，统一归到首个 loop 的工作空间（有意简化）
    let template_ws_id = global_ws.map(|(id, _)| id)
        .or_else(|| resolved_loops.first().map(|r| r.ws_id))
        .unwrap_or(0);

    // 构建伪ID -> 真实ID 的映射表
    let mut tag_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut template_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut todo_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut loop_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let _step_pseudo_to_real: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

    let mut created_counts = LoopImportCreatedCounts {
        loops: 0, todos: 0, review_templates: 0, tags: 0, triggers: 0, steps: 0,
    };
    let mut updated_counts = LoopImportCreatedCounts {
        loops: 0, todos: 0, review_templates: 0, tags: 0, triggers: 0, steps: 0,
    };
    // skipped 已在前部按用户选择「跳过」的同名环路名收集（见 skip_names 处理）

    // 阶段1: 合并标签（按name匹配，同名复用）
    for tag in &data.tags {
        // 查找同名标签
        if let Some(existing_id) = state.db.find_tag_by_name(&tag.name).await? {
            tag_pseudo_to_real.insert(tag.id.clone(), existing_id);
        } else {
            let tag_id = state.db.create_tag(&tag.name, &tag.color).await?;
            tag_pseudo_to_real.insert(tag.id.clone(), tag_id);
            created_counts.tags += 1;
        }
    }

    // 阶段2: 合并评审模板（按name匹配，存在则覆盖；统一归到首个 loop 的工作空间）
    for tmpl in &data.review_templates {
        if let Some(existing) = state.db.get_review_template_by_name(&tmpl.name).await? {
            // 存在则更新
            let input = ReviewTemplateInput {
                name: tmpl.name.clone(),
                description: tmpl.description.clone(),
                prompt: tmpl.prompt.clone(),
                workspace_id: Some(template_ws_id),
            };
            state.db.update_review_template(existing.id, &input).await?;
            template_pseudo_to_real.insert(tmpl.id.clone(), existing.id);
            updated_counts.review_templates += 1;
        } else {
            let input = ReviewTemplateInput {
                name: tmpl.name.clone(),
                description: tmpl.description.clone(),
                prompt: tmpl.prompt.clone(),
                workspace_id: Some(template_ws_id),
            };
            let new_id = state.db.create_review_template(&input).await?;
            template_pseudo_to_real.insert(tmpl.id.clone(), new_id);
            created_counts.review_templates += 1;
        }
    }

    // 阶段3: 合并Todo（按 title+prompt+该 todo 归属 workspace 匹配，避免跨工作空间误覆盖）
    for todo in &data.todos {
        // 该 todo 归属的工作空间 (id, path)；未在映射里退化为哨兵 0
        let (todo_ws_id, todo_ws_path) = todo_ws.get(&todo.id).cloned().unwrap_or((0, None));
        // 尝试查找完全相同的 Todo（title + prompt + 同工作空间）
        if let Some(existing_todo) = state.db.get_todo_by_identity(&todo.title, &todo.prompt, todo_ws_id).await? {
            // 存在则复用，不重复创建
            todo_pseudo_to_real.insert(todo.id.clone(), existing_todo.id);
        } else {
            // 解析 kind 字符串 → i32
            let kind: Option<i32> = todo.kind.parse().ok().or(Some(0));
            // 解析 review_template_id 伪ID → 真实ID
            let real_review_template_id = todo.review_template_id.as_ref()
                .and_then(|tid| template_pseudo_to_real.get(tid))
                .copied();
            let new_todo_id = state.db.create_todo_for_import(
                &todo.title,
                &todo.prompt,
                todo.executor.as_deref(),
                todo.acceptance_criteria.as_deref(),
                todo.webhook_enabled,
                todo_ws_id,
                todo_ws_path.as_deref().unwrap_or(""),
                Some(&todo.status),
                Some(todo.scheduler_enabled),
                Some(todo.auto_review_enabled),
                real_review_template_id,
                kind,
            ).await?;
            todo_pseudo_to_real.insert(todo.id.clone(), new_todo_id);
            created_counts.todos += 1;
        }
    }

    // 阶段4: 合并环路——单一覆盖语义，对齐 Todo merge_backup：
    // 同名（在 resolved ws 内）→ 删旧 + 原名重建（覆盖）；不同名 → 原名新建。
    // 用户选择「跳过」的同名环路已在 resolve 前从 data.loops 移除，不会进入本阶段。
    for (i, loop_export) in data.loops.iter().enumerate() {
        let resolved = &resolved_loops[i];

        // 查找同名环路（只在该 loop 解析出的工作空间内查，避免跨工作空间误判同名）
        let existing_loop = state.db.list_loops_with_counts(Some(resolved.ws_id)).await?
            .into_iter()
            .find(|l| l.loop_.name == loop_export.name);

        // 同名存在 → 删除旧环路，随后用原名重建（= 覆盖）；不存在 → 直接原名新建
        let is_new = existing_loop.is_none();
        if let Some(existing) = existing_loop {
            state.db.delete_loop(existing.loop_.id).await?;
        }
        let new_loop_id = create_loop_from_export(
            &state, loop_export, resolved.ws_id, resolved.ws_path.as_deref(),
            &template_pseudo_to_real, &todo_pseudo_to_real, &tag_pseudo_to_real,
        ).await?;

        loop_pseudo_to_real.insert(loop_export.id.clone(), new_loop_id);
        if is_new {
            created_counts.loops += 1;
        } else {
            updated_counts.loops += 1;
        }
    }

    // 刷新scheduler以加载新的cron触发器
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.reload_all().await;
    }

    let response = MergeLoopResponse {
        success: true,
        created: created_counts,
        updated: updated_counts,
        skipped,
        warnings,
    };

    Ok(ApiResponse::ok(response))
}

/// 从导出数据创建环路（供新建模式和合并模式复用）
async fn create_loop_from_export(
    state: &AppState,
    loop_export: &LoopExportItem,
    workspace_id: i64,
    workspace_path: Option<&str>,
    template_pseudo_to_real: &std::collections::HashMap<String, i64>,
    todo_pseudo_to_real: &std::collections::HashMap<String, i64>,
    tag_pseudo_to_real: &std::collections::HashMap<String, i64>,
) -> Result<i64, AppError> {
    let review_template_id = loop_export.review_template_id.as_ref()
        .and_then(|tid| template_pseudo_to_real.get(tid))
        .copied();

    let abnormal_handler_todo_id = loop_export.abnormal_handler_todo_id.as_ref()
        .and_then(|tid| todo_pseudo_to_real.get(tid))
        .copied();

    let limits_config = serde_json::to_string(&loop_export.limits_config)
        .unwrap_or_else(|_| "{}".to_string());
    let abnormal_handler_trigger_on = serde_json::to_string(&loop_export.abnormal_handler_trigger_on)
        .unwrap_or_else(|_| "[]".to_string());

    // 统一覆盖语义：用原名（同名已在上游删除），不再追加 "-合并" 后缀，对齐 Todo
    let loop_name = loop_export.name.clone();

    let new_loop = state.db.create_loop(
        &loop_name,
        &loop_export.description,
        Some(workspace_id),
        workspace_path,
        loop_export.webhook_enabled,
        &loop_export.icon,
        review_template_id,
        Some(&limits_config),
        abnormal_handler_todo_id,
        &abnormal_handler_trigger_on,
    ).await?;

    // 关联标签 - 先收集所有 tag_id，再一次性写入，避免 set_loop_tags 全量替换导致只保留最后一个
    let tag_ids: Vec<i64> = loop_export.tag_ids.iter()
        .filter_map(|pseudo_tag_id| tag_pseudo_to_real.get(pseudo_tag_id).copied())
        .collect();
    if !tag_ids.is_empty() {
        state.db.set_loop_tags(new_loop.id, &tag_ids).await?;
    }

    // 导入触发器
    for trigger in &loop_export.triggers {
        state.db.create_trigger(
            new_loop.id,
            &trigger.trigger_type,
            &serde_json::to_string(&trigger.config).unwrap_or_else(|_| "{}".to_string()),
            trigger.enabled,
            trigger.priority,
        ).await?;
    }

    // 导入步骤（两遍）
    let mut step_map: Vec<(String, i64)> = Vec::new();
    for step in &loop_export.steps {
        let todo_id = todo_pseudo_to_real.get(&step.todo_id)
            .copied()
            .ok_or_else(|| AppError::BadRequest(format!("Step '{}' references unknown todo", step.name)))?;

        let new_step = state.db.create_loop_step(
            new_loop.id,
            &step.name,
            &step.description,
            todo_id,
            &step.run_mode,
            step.skip_on_source_failed,
            step.min_rating,
            &step.unrated_policy,
            step.enabled,
            &step.on_success,
            None,  // goto 第二遍
            &step.on_rating_fail,
            None,  // goto 第二遍
            &step.review_type,
        ).await?;

        step_map.push((step.id.clone(), new_step.id));
    }

    // 第二遍：修复goto引用
    for step in &loop_export.steps {
        if let Some(&new_step_id) = step_map.iter().find(|(id, _)| id == &step.id).map(|(_, id)| id) {
            let success_goto = step.success_goto_step_id.as_ref()
                .and_then(|sid| step_map.iter().find(|(sid2, _)| sid2 == sid).map(|(_, id)| *id));
            let fail_goto = step.fail_goto_step_id.as_ref()
                .and_then(|sid| step_map.iter().find(|(sid2, _)| sid2 == sid).map(|(_, id)| *id));

            if success_goto.is_some() || fail_goto.is_some() {
                state.db.update_loop_step_goto(new_step_id, success_goto, fail_goto).await?;
            }
        }
    }

    Ok(new_loop.id)
}

/// 构建环路导出的 YAML 内容
/// 收集所有依赖实体（标签、评审模板、Todo）并生成伪ID
async fn build_loop_export_yaml(
    state: &AppState,
    loop_ids: &[i64],
) -> Result<String, AppError> {
    let mut all_tags: std::collections::HashMap<i64, TagExportItem> = std::collections::HashMap::new();
    let mut all_templates: std::collections::HashMap<i64, ReviewTemplateExportItem> = std::collections::HashMap::new();
    let mut all_todos: std::collections::HashMap<i64, TodoExportItem> = std::collections::HashMap::new();
    let mut exported_loops: Vec<LoopExportItem> = Vec::new();

    // 全局计数器，避免批量导出时多个环路的 trigger/step pseudo-ID 冲突
    let mut global_trigger_idx = 0;
    let mut global_step_idx = 0;

    for (idx, &loop_id) in loop_ids.iter().enumerate() {
        // AppError::NotFound 是单元变体，不捕获变量——用 ok_or 直接构造更简洁
        let view = state.db.load_loop_full(loop_id).await?
            .ok_or(AppError::NotFound)?;

        // 收集环路关联的标签
        let loop_tag_ids = state.db.get_loop_tag_ids(loop_id).await?;
        for tag_id in &loop_tag_ids {
            if !all_tags.contains_key(tag_id) {
                // 获取标签详情
                if let Some(tag) = state.db.get_tag(*tag_id).await? {
                    all_tags.insert(*tag_id, TagExportItem {
                        id: generate_pseudo_id("tag", all_tags.len() + 1),
                        name: tag.name,
                        color: tag.color,
                    });
                }
            }
        }

        // 收集环路引用的评审模板
        if let Some(tpl_id) = view.loop_.review_template_id {
            if !all_templates.contains_key(&tpl_id) {
                if let Some(tpl) = state.db.get_review_template(tpl_id).await? {
                    all_templates.insert(tpl_id, ReviewTemplateExportItem {
                        id: generate_pseudo_id("template", all_templates.len() + 1),
                        name: tpl.name,
                        description: tpl.description,
                        prompt: tpl.prompt,
                    });
                }
            }
        }

        // 遍历步骤收集 Todo 和标签
        for (step, _todo_title, _, _) in &view.steps_meta {
            if !all_todos.contains_key(&step.todo_id) {
                if let Some(todo) = state.db.get_todo_entity(step.todo_id).await? {
                    // 收集 Todo 的标签
                    let todo_tag_ids = state.db.get_todo_tag_ids(step.todo_id).await?;
                    let mut todo_tag_items: Vec<TagExportItem> = Vec::new();
                    for tag_id in &todo_tag_ids {
                        if !all_tags.contains_key(tag_id) {
                            if let Some(tag) = state.db.get_tag(*tag_id).await? {
                                let pseudo_id = generate_pseudo_id("tag", all_tags.len() + 1);
                                let color = tag.color.clone();
                                all_tags.insert(*tag_id, TagExportItem {
                                    id: pseudo_id.clone(),
                                    name: tag.name.clone(),
                                    color: color.clone(),
                                });
                                todo_tag_items.push(TagExportItem {
                                    id: pseudo_id,
                                    name: tag.name,
                                    color,
                                });
                            }
                        } else if let Some(existing) = all_tags.get(tag_id) {
                            todo_tag_items.push(existing.clone());
                        }
                    }

                    // 收集 Todo 引用的评审模板
                    let mut review_template_id: Option<String> = None;
                    let mut review_template_name: Option<String> = None;
                    if let Some(rt_id) = todo.review_template_id {
                        if !all_templates.contains_key(&rt_id) {
                            if let Some(tpl) = state.db.get_review_template(rt_id).await? {
                                // 先计算 pseudo id，再分别赋给 review_template_id 和结构体字段，
                                // 避免 clone().unwrap() 的生产代码反模式
                                let pseudo_id = generate_pseudo_id("template", all_templates.len() + 1);
                                review_template_id = Some(pseudo_id.clone());
                                review_template_name = Some(tpl.name.clone());
                                all_templates.insert(rt_id, ReviewTemplateExportItem {
                                    id: pseudo_id,
                                    name: tpl.name,
                                    description: tpl.description,
                                    prompt: tpl.prompt,
                                });
                            }
                        } else if let Some(existing) = all_templates.get(&rt_id) {
                            review_template_id = Some(existing.id.clone());
                            review_template_name = Some(existing.name.clone());
                        }
                    }

                    let tag_ids: Vec<String> = todo_tag_items.iter().map(|t| t.id.clone()).collect();
                    let tag_names: Vec<String> = todo_tag_items.iter().map(|t| t.name.clone()).collect();

                    // 实体模型 status 是 Option<String>，需要 unwrap_or_default
                    // kind 是 Option<String>，需要 unwrap_or_default
                    // 导出时保留工作空间信息，让导入方知道该 todo 原本属于哪个工作空间
                    let todo_workspace_id = todo.workspace_id;
                    let todo_workspace_path = todo.workspace_path.clone();
                    all_todos.insert(step.todo_id, TodoExportItem {
                        id: generate_pseudo_id("todo", all_todos.len() + 1),
                        title: todo.title.clone(),
                        prompt: todo.prompt.clone().unwrap_or_default(),
                        status: todo.status.clone().unwrap_or_else(|| "pending".to_string()),
                        executor: todo.executor.clone(),
                        scheduler_enabled: todo.scheduler_enabled.unwrap_or(false),
                        webhook_enabled: todo.webhook_enabled.unwrap_or(false),
                        acceptance_criteria: todo.acceptance_criteria.clone(),
                        auto_review_enabled: todo.auto_review_enabled.unwrap_or(false),
                        review_template_id,
                        review_template_name,
                        kind: todo.kind.clone().unwrap_or_else(|| "item".to_string()),
                        tag_ids,
                        tag_names,
                        is_abnormal_handler: false,
                        action_type: todo.action_type.clone(),
                        action_key: todo.action_key.clone(),
                        workspace_id: todo_workspace_id,
                        workspace_path: todo_workspace_path,
                    });
                }
            }
        }

        // 收集异常处理 Todo（如果有）
        let mut abnormal_handler_todo_id: Option<String> = None;
        let mut abnormal_handler_todo_title: Option<String> = None;
        if let Some(handler_todo_id) = view.loop_.abnormal_handler_todo_id {
            if !all_todos.contains_key(&handler_todo_id) {
                if let Some(todo) = state.db.get_todo_entity(handler_todo_id).await? {
                    // 异常处理 Todo 不导出标签
                    // 异常处理 Todo 也保留工作空间信息，确保导入后能正确关联
                    let handler_workspace_id = todo.workspace_id;
                    let handler_workspace_path = todo.workspace_path.clone();
                    all_todos.insert(handler_todo_id, TodoExportItem {
                        id: generate_pseudo_id("todo", all_todos.len() + 1),
                        title: todo.title.clone(),
                        prompt: todo.prompt.clone().unwrap_or_default(),
                        status: todo.status.clone().unwrap_or_else(|| "pending".to_string()),
                        executor: todo.executor.clone(),
                        scheduler_enabled: todo.scheduler_enabled.unwrap_or(false),
                        webhook_enabled: todo.webhook_enabled.unwrap_or(false),
                        acceptance_criteria: todo.acceptance_criteria.clone(),
                        auto_review_enabled: todo.auto_review_enabled.unwrap_or(false),
                        review_template_id: None,
                        review_template_name: None,
                        kind: todo.kind.clone().unwrap_or_else(|| "item".to_string()),
                        tag_ids: vec![],
                        tag_names: vec![],
                        is_abnormal_handler: true,
                        action_type: todo.action_type.clone(),
                        action_key: todo.action_key.clone(),
                        workspace_id: handler_workspace_id,
                        workspace_path: handler_workspace_path,
                    });
                }
            }
            if let Some(td) = all_todos.get(&handler_todo_id) {
                abnormal_handler_todo_id = Some(td.id.clone());
                abnormal_handler_todo_title = Some(td.title.clone());
            }
        }

        // 构建环路导出项
        let mut triggers: Vec<LoopTriggerExportItem> = Vec::new();
        for t in &view.triggers {
            // 只导出 manual 和 cron 触发器
            if t.trigger_type != "manual" && t.trigger_type != "cron" {
                continue;
            }
            global_trigger_idx += 1;
            let mut enabled = t.enabled != 0;
            // cron 触发器导出时强制禁用
            if t.trigger_type == "cron" {
                enabled = false;
            }
            triggers.push(LoopTriggerExportItem {
                id: generate_pseudo_id("trigger", global_trigger_idx),
                trigger_type: t.trigger_type.clone(),
                config: serde_json::from_str(&t.config).unwrap_or_default(),
                enabled,
                priority: t.priority,
            });
        }

        let mut steps: Vec<LoopStepExportItem> = Vec::new();
        // 本地位置 → 全局 step pseudo-ID 映射（用于解析 goto 引用）
        let mut step_pos_to_global: Vec<i32> = Vec::new();
        for (step, _todo_title, _, _) in &view.steps_meta {
            global_step_idx += 1;
            step_pos_to_global.push(global_step_idx);
            let todo_pseudo_id = all_todos.get(&step.todo_id)
                .map(|t| t.id.clone())
                .unwrap_or_else(|| generate_pseudo_id("todo", 999));

            let success_goto_step_id = step.success_goto_step_id
                .and_then(|gid| view.steps.iter().position(|s| s.id == gid))
                .and_then(|pos| step_pos_to_global.get(pos).copied())
                .map(|idx| generate_pseudo_id("step", idx as usize));
            let fail_goto_step_id = step.fail_goto_step_id
                .and_then(|gid| view.steps.iter().position(|s| s.id == gid))
                .and_then(|pos| step_pos_to_global.get(pos).copied())
                .map(|idx| generate_pseudo_id("step", idx as usize));

            steps.push(LoopStepExportItem {
                id: generate_pseudo_id("step", global_step_idx as usize),
                name: step.name.clone(),
                description: step.description.clone(),
                todo_id: todo_pseudo_id,
                todo_title: all_todos.get(&step.todo_id).map(|t| t.title.clone()).unwrap_or_default(),
                order_index: step.order_index,
                run_mode: step.run_mode.clone(),
                skip_on_source_failed: step.skip_on_source_failed != 0,
                min_rating: step.min_rating,
                unrated_policy: step.unrated_policy.clone(),
                on_success: step.on_success.clone(),
                success_goto_step_id,
                success_goto_step_name: step.success_goto_step_id
                    .and_then(|gid| view.steps.iter().find(|s| s.id == gid))
                    .map(|s| s.name.clone()),
                on_rating_fail: step.on_rating_fail.clone(),
                fail_goto_step_id,
                fail_goto_step_name: step.fail_goto_step_id
                    .and_then(|gid| view.steps.iter().find(|s| s.id == gid))
                    .map(|s| s.name.clone()),
                review_type: step.review_type.clone(),
                enabled: step.enabled != 0,
            });
        }

        // 环路级别的评审模板
        let mut loop_review_template_id: Option<String> = None;
        let mut loop_review_template_name: Option<String> = None;
        if let Some(rt_id) = view.loop_.review_template_id {
            if let Some(existing) = all_templates.get(&rt_id) {
                loop_review_template_id = Some(existing.id.clone());
                loop_review_template_name = Some(existing.name.clone());
            }
        }

        let loop_tag_ids = state.db.get_loop_tag_ids(loop_id).await?;
        let loop_tag_pseudo_ids: Vec<String> = loop_tag_ids.iter()
            .filter_map(|id| all_tags.get(id).map(|t| t.id.clone()))
            .collect();
        let loop_tag_names: Vec<String> = loop_tag_ids.iter()
            .filter_map(|id| all_tags.get(id).map(|t| t.name.clone()))
            .collect();

        let limits_config: serde_json::Value = serde_json::from_str(&view.loop_.limits_config)
            .unwrap_or_default();

        let abnormal_handler_trigger_on: Vec<String> = serde_json::from_str(&view.loop_.abnormal_handler_trigger_on)
            .unwrap_or_default();

        // 导出时保留环路的工作空间信息，让导入方知道原本属于哪个工作空间
        let loop_workspace_id = view.loop_.workspace_id;
        let loop_workspace_path = view.loop_.workspace_path.clone();
        exported_loops.push(LoopExportItem {
            id: generate_pseudo_id("loop", idx + 1),
            name: view.loop_.name.clone(),
            description: view.loop_.description.clone(),
            icon: view.loop_.icon.clone(),
            color: view.loop_.color.clone(),
            status: "paused".to_string(), // 导出时统一为 paused
            webhook_enabled: view.loop_.webhook_enabled,
            limits_config,
            review_template_id: loop_review_template_id,
            review_template_name: loop_review_template_name,
            abnormal_handler_todo_id,
            abnormal_handler_todo_title,
            abnormal_handler_trigger_on,
            tag_ids: loop_tag_pseudo_ids,
            tag_names: loop_tag_names,
            triggers,
            steps,
            workspace_id: loop_workspace_id,
            workspace_path: loop_workspace_path,
        });
    }

    let export_data = LoopExportData {
        version: "1.0".to_string(),
        export_type: "loop".to_string(),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        source: "nothing-todo".to_string(),
        schema_version: 1,
        tags: all_tags.into_values().collect(),
        review_templates: all_templates.into_values().collect(),
        todos: all_todos.into_values().collect(),
        loops: exported_loops,
    };

    serde_yaml::to_string(&export_data).map_err(|e| AppError::Internal(e.to_string()))
}

// ====== 路由表 ======

pub fn loop_routes() -> axum::Router<AppState> {
    use axum::routing::{get, post, put};
    axum::Router::new()
        .route("/api/loops", get(list_loops).post(create_loop))
        .route("/api/loops/batch-workspace", put(batch_move_loops_workspace))
        .route("/api/loops/batch-copy-workspace", post(batch_copy_loops_workspace))
        .route("/api/loops/export-selected", post(export_selected_loops))
        .route("/api/loops/export", get(export_all_loops))
        // stats 必须在 {id} 之前注册:axum 字面量路由优先匹配,否则 "stats" 会被当成 loop id。
        .route("/api/loops/stats", get(get_loop_stats))
        .route("/api/loops/{id}", get(get_loop).put(update_loop).delete(delete_loop))
        .route("/api/loops/{id}/export", get(export_loop))
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
        // 通过执行 ID 直接获取执行详情（无需 loop_id），供消息历史中 "处理类型" 列跳转使用
        .route("/api/loop-executions/{eid}", get(get_execution_by_id))
        // 导入导出
        .route("/api/loops/import/preview", post(import_preview))
        .route("/api/loops/import", post(import_loops))
        .route("/api/loops/merge", post(merge_loops))
}

// ====== V1 API handlers (workspace-scoped paths, nested under /api/v1/workspaces/{ws}/loops) ======
// V1 handlers differ from the originals only in how they extract ws_id:
// - routes without ws_id in v0 (list, create) receive it via Path(ws_id)
// - routes with other Path params (id, loop_id, etc.) get ws_id prepended as a tuple prefix
// This file keeps both v0 and v1 code until the migration is complete.

/// GET / (nested) — list loops filtered by workspace from path
pub async fn list_loops_v1(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    // V1: workspace_id 从路径参数取，不再依赖查询参数中的 workspace_id
    let rows = state.db.list_loops_with_counts(Some(ws_id)).await?;
    let items: Vec<LoopListItem> = rows.into_iter().map(Into::into).collect();
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

/// POST / (nested) — create loop, force workspace_id from path
pub async fn create_loop_v1(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    Json(mut req): Json<CreateLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    // V1: workspace_id 强制从路径取，覆盖请求体中的值，避免路径与 body 不一致
    req.workspace_id = ws_id;
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    let workspace = state
        .db
        .get_project_directory_by_id(req.workspace_id)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id)))?;
    if req.tag_ids.len() > 1 {
        return Err(AppError::BadRequest("环路只能选择一个标签".to_string()));
    }
    let created = state
        .db
        .create_loop(
            req.name.trim(),
            &req.description,
            Some(req.workspace_id),
            Some(workspace.path.as_str()),
            req.webhook_enabled,
            &req.icon,
            req.review_template_id,
            req.limits_config.as_deref(),
            req.abnormal_handler_todo_id,
            &req.abnormal_handler_trigger_on,
        )
        .await?;
    if !req.tag_ids.is_empty() {
        state.db.set_loop_tags(created.id, &req.tag_ids).await?;
    }
    let tag_ids = state.db.get_loop_tag_ids(created.id).await?;
    Ok((StatusCode::CREATED, ApiResponse::ok(LoopDto::from(created).with_tags(tag_ids))))
}

/// GET /{id} (nested) — loop detail, 先校验 loop 属于路径 workspace 再取详情
pub async fn get_loop_v1(
    State(state): State<AppState>,
    Path((_ws_id, id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：loop id 全局唯一不等于「跨 ws 可见」，必须校验归属路径中的 workspace
    workspace_guard::verify_loop_belongs_to_ws(&state.db, id, _ws_id).await?;
    let view = state.db.load_loop_full(id).await?
        .ok_or(AppError::NotFound)?;
    let tag_ids = state.db.get_loop_tag_ids(id).await?;
    let mut detail = LoopDetail::from(view);
    detail.loop_ = detail.loop_.with_tags(tag_ids);
    Ok(ApiResponse::ok(detail))
}

/// PUT /{id} (nested) — full update, 先校验 loop 归属路径 workspace
pub async fn update_loop_v1(
    State(state): State<AppState>,
    Path((_ws_id, id)): Path<(i64, i64)>,
    Json(req): Json<UpdateLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    // V1 隔离：校验 loop 当前属于路径 workspace，防止越权改别人的 loop。
    // verify 已含存在性校验（NotFound），不再单独 get_loop 探测存在。
    workspace_guard::verify_loop_belongs_to_ws(&state.db, id, _ws_id).await?;
    // 若 body 指定 workspace_id（迁移到新 workspace），解析其 path 做双字段同步写入；
    // None 表示保持当前 workspace 不变。原 take()/重读 req 的死逻辑已移除。
    let workspace_path: Option<String> = if let Some(wid) = req.workspace_id {
        Some(state.db.get_project_directory_by_id(wid).await?
            .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", wid)))?.path)
    } else {
        None
    };
    state.db.update_loop(
        id, req.name.trim(), &req.description,
        req.workspace_id, workspace_path.as_deref(),
        req.webhook_enabled, &req.icon, req.review_template_id,
        req.limits_config.as_deref(), req.abnormal_handler_todo_id,
        &req.abnormal_handler_trigger_on,
    ).await?;
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

/// DELETE /{id} (nested) — delete loop, 先校验归属再删
pub async fn delete_loop_v1(
    State(state): State<AppState>,
    Path((_ws_id, id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验 loop 属于路径 workspace（verify 已含存在性校验，替换原 get_loop 探测）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, id, _ws_id).await?;
    let triggers = state.db.list_triggers_by_loop(id).await?;
    for t in triggers.iter().filter(|t| t.trigger_type == "cron") {
        if let Some(sched) = state.loop_scheduler.as_ref() {
            sched.remove_cron_trigger(t.id).await;
        }
    }
    state.db.delete_loop(id).await?;
    Ok(ApiResponse::ok(()))
}

/// PUT /{id}/status (nested) — toggle enabled/paused
pub async fn update_loop_status_v1(
    State(state): State<AppState>,
    Path((_ws_id, id)): Path<(i64, i64)>,
    Json(req): Json<UpdateLoopStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    models::validate_loop_status(&req.status)
        .map_err(AppError::BadRequest)?;
    // V1 隔离：校验 loop 属于路径 workspace（verify 已含存在性校验）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, id, _ws_id).await?;
    state.db.update_loop_status(id, &req.status).await?;
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.reload_all().await;
    }
    let updated = state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    let tag_ids = state.db.get_loop_tag_ids(id).await?;
    Ok(ApiResponse::ok(LoopDto::from(updated).with_tags(tag_ids)))
}

/// PUT /{id}/tags (nested) — replace tags
pub async fn update_loop_tags_v1(
    State(state): State<AppState>,
    Path((_ws_id, id)): Path<(i64, i64)>,
    Json(req): Json<UpdateTagsRequest>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验 loop 属于路径 workspace（verify 已含存在性校验）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, id, _ws_id).await?;
    if req.tag_ids.len() > 1 {
        return Err(AppError::BadRequest("环路只能选择一个标签".to_string()));
    }
    state.db.set_loop_tags(id, &req.tag_ids).await?;
    let updated = state.db.get_loop(id).await?.ok_or(AppError::NotFound)?;
    let tag_ids = state.db.get_loop_tag_ids(id).await?;
    Ok(ApiResponse::ok(LoopDto::from(updated).with_tags(tag_ids)))
}

/// POST /{id}/duplicate (nested) — duplicate loop
pub async fn duplicate_loop_v1(
    State(state): State<AppState>,
    Path((_ws_id, id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验源 loop 属于路径 workspace，复制产物继承同一 workspace
    workspace_guard::verify_loop_belongs_to_ws(&state.db, id, _ws_id).await?;
    let new_loop = state.db.duplicate_loop(id).await?
        .ok_or(AppError::NotFound)?;
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.reload_all().await;
    }
    Ok((StatusCode::CREATED, ApiResponse::ok(LoopDto::from(new_loop).with_tags(vec![]))))
}

/// POST /{id}/trigger (nested) — manual trigger
pub async fn trigger_loop_v1(
    State(state): State<AppState>,
    Path((_ws_id, id)): Path<(i64, i64)>,
    Json(req): Json<TriggerLoopRequest>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验 loop 属于路径 workspace，防止越权触发别人的 loop
    workspace_guard::verify_loop_belongs_to_ws(&state.db, id, _ws_id).await?;
    let dispatcher = state.loop_trigger_dispatcher.as_ref()
        .ok_or_else(|| AppError::Internal("loop dispatcher not ready".to_string()))?;
    let trigger_meta = serde_json::json!({
        "source": "manual",
        "params": req.params,
    });
    match dispatcher.dispatch_manual_with_meta(id, trigger_meta).await {
        Some(exec_id) => Ok(ApiResponse::ok(serde_json::json!({
            "execution_id": exec_id,
        }))),
        None => Err(AppError::BadRequest("loop 不存在或未启用".to_string())),
    }
}

// ====== V1 Triggers ======

/// GET /{id}/triggers (nested) — list triggers
pub async fn list_triggers_v1(
    State(state): State<AppState>,
    Path((_ws_id, loop_id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验父 loop 属于路径 workspace，再列其子资源
    workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, _ws_id).await?;
    let triggers = state.db.list_triggers_by_loop(loop_id).await?;
    let dtos: Vec<LoopTriggerDto> = triggers.into_iter().map(Into::into).collect();
    Ok(ApiResponse::ok(dtos))
}

/// POST /{id}/triggers (nested) — create trigger
pub async fn create_trigger_v1(
    State(state): State<AppState>,
    Path((_ws_id, loop_id)): Path<(i64, i64)>,
    Json(req): Json<CreateTriggerRequest>,
) -> Result<impl IntoResponse, AppError> {
    models::validate_trigger_type(&req.trigger_type)
        .map_err(AppError::BadRequest)?;
    // V1 隔离：校验父 loop 属于路径 workspace（verify 已含存在性校验）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, _ws_id).await?;
    let created = state.db.create_trigger(
        loop_id, &req.trigger_type, &req.config, req.enabled, req.priority,
    ).await?;
    if created.trigger_type == "cron" && created.enabled == 1 {
        if let Some(sched) = state.loop_scheduler.as_ref() {
            let _ = sched.upsert_cron_trigger(created.id).await;
        }
    }
    Ok((StatusCode::CREATED, ApiResponse::ok(LoopTriggerDto::from(created))))
}

/// PUT /{id}/triggers/{tid} (nested) — update trigger
pub async fn update_trigger_v1(
    State(state): State<AppState>,
    Path((_ws_id, loop_id, tid)): Path<(i64, i64, i64)>,
    Json(req): Json<UpdateTriggerRequest>,
) -> Result<impl IntoResponse, AppError> {
    models::validate_trigger_type(&req.trigger_type)
        .map_err(AppError::BadRequest)?;
    // V1 隔离：校验父 loop 属于路径 workspace（后续 updated.loop_id != loop_id 校验子资源归属）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, _ws_id).await?;
    state.db.get_trigger(tid).await?.ok_or(AppError::NotFound)?;
    state.db.update_trigger(tid, &req.trigger_type, &req.config, req.enabled, req.priority).await?;
    if let Some(sched) = state.loop_scheduler.as_ref() {
        let _ = sched.upsert_cron_trigger(tid).await;
    }
    let updated = state.db.get_trigger(tid).await?
        .ok_or(AppError::NotFound)?;
    if updated.loop_id != loop_id {
        return Err(AppError::BadRequest("trigger 不属于该 loop".to_string()));
    }
    Ok(ApiResponse::ok(LoopTriggerDto::from(updated)))
}

/// DELETE /{id}/triggers/{tid} (nested) — delete trigger
pub async fn delete_trigger_v1(
    State(state): State<AppState>,
    Path((_ws_id, _loop_id, tid)): Path<(i64, i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验父 loop 属于路径 workspace，再按 tid 删 trigger
    workspace_guard::verify_loop_belongs_to_ws(&state.db, _loop_id, _ws_id).await?;
    state.db.get_trigger(tid).await?.ok_or(AppError::NotFound)?;
    if let Some(sched) = state.loop_scheduler.as_ref() {
        sched.remove_cron_trigger(tid).await;
    }
    state.db.delete_trigger(tid).await?;
    Ok(ApiResponse::ok(()))
}

// ====== V1 Steps ======

/// GET /{id}/steps (nested) — list steps
pub async fn list_loop_steps_v1(
    State(state): State<AppState>,
    Path((_ws_id, loop_id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验父 loop 属于路径 workspace，再列其 steps
    workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, _ws_id).await?;
    let rows = state.db.list_loop_steps_with_todo_meta(loop_id).await?;
    let dtos: Vec<LoopStepDto> = rows.into_iter()
        .map(|(s, todo_title, todo_executor, todo_archived_at)| LoopStepDto {
            step: s.into(), todo_title, todo_executor, todo_archived_at,
        })
        .collect();
    Ok(ApiResponse::ok(dtos))
}

/// POST /{id}/steps (nested) — create step
pub async fn create_loop_step_v1(
    State(state): State<AppState>,
    Path((_ws_id, loop_id)): Path<(i64, i64)>,
    Json(req): Json<CreateLoopStepRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    // V1 隔离：校验父 loop 属于路径 workspace（verify 已含存在性校验）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, _ws_id).await?;
    state.db.get_todo(req.todo_id).await?
        .ok_or_else(|| AppError::BadRequest(format!("todo #{} 不存在", req.todo_id)))?;
    let created = state.db.create_loop_step(
        loop_id, req.name.trim(), &req.description, req.todo_id,
        &req.run_mode, req.skip_on_source_failed, req.min_rating,
        &req.unrated_policy, req.enabled, &req.on_success,
        req.success_goto_step_id, &req.on_rating_fail,
        req.fail_goto_step_id, &req.review_type,
    ).await?;
    let (_, todo_title, todo_executor, todo_archived_at) = state.db
        .list_loop_steps_with_todo_meta(loop_id).await?
        .into_iter()
        .find(|(s, _, _, _)| s.id == created.id)
        .ok_or_else(|| AppError::Internal("created step missing".to_string()))?;
    Ok((StatusCode::CREATED, ApiResponse::ok(LoopStepDto {
        step: created.into(), todo_title, todo_executor, todo_archived_at,
    })))
}

/// PUT /{id}/steps/{sid} (nested) — update step
pub async fn update_loop_step_v1(
    State(state): State<AppState>,
    Path((_ws_id, loop_id, sid)): Path<(i64, i64, i64)>,
    Json(req): Json<UpdateLoopStepRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".to_string()));
    }
    // V1 隔离：校验父 loop 属于路径 workspace（后续 step.loop_id != loop_id 校验子资源归属）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, _ws_id).await?;
    let step = state.db.get_loop_step(sid).await?.ok_or(AppError::NotFound)?;
    if step.loop_id != loop_id {
        return Err(AppError::BadRequest("step 不属于该 loop".to_string()));
    }
    if req.todo_id != step.todo_id {
        state.db.get_todo(req.todo_id).await?
            .ok_or_else(|| AppError::BadRequest(format!("todo #{} 不存在", req.todo_id)))?;
    }
    if req.on_rating_fail == "goto" && req.fail_goto_step_id == Some(sid) {
        let loop_ = state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
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
    state.db.update_loop_step(
        sid, req.name.trim(), &req.description, req.todo_id,
        &req.run_mode, req.skip_on_source_failed, req.min_rating,
        &req.unrated_policy, req.enabled, &req.on_success,
        req.success_goto_step_id, &req.on_rating_fail,
        req.fail_goto_step_id, &req.review_type,
    ).await?;
    let (_, todo_title, todo_executor, todo_archived_at) = state.db
        .list_loop_steps_with_todo_meta(loop_id).await?
        .into_iter()
        .find(|(s, _, _, _)| s.id == sid)
        .ok_or_else(|| AppError::Internal("updated step missing".to_string()))?;
    Ok(ApiResponse::ok(LoopStepDto {
        step: state.db.get_loop_step(sid).await?.ok_or(AppError::Internal("step missing".to_string()))?.into(),
        todo_title, todo_executor, todo_archived_at,
    }))
}

/// DELETE /{id}/steps/{sid} (nested) — delete step
pub async fn delete_loop_step_v1(
    State(state): State<AppState>,
    Path((_ws_id, _loop_id, sid)): Path<(i64, i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验父 loop 属于路径 workspace，再按 sid 删 step
    workspace_guard::verify_loop_belongs_to_ws(&state.db, _loop_id, _ws_id).await?;
    state.db.get_loop_step(sid).await?.ok_or(AppError::NotFound)?;
    state.db.delete_loop_step(sid).await?;
    Ok(ApiResponse::ok(()))
}

/// POST /{id}/steps/reorder (nested) — batch reorder steps
pub async fn reorder_loop_steps_v1(
    State(state): State<AppState>,
    Path((_ws_id, loop_id)): Path<(i64, i64)>,
    Json(req): Json<ReorderLoopStepsRequest>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验父 loop 属于路径 workspace（verify 已含存在性校验）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, _ws_id).await?;
    state.db.reorder_loop_steps(loop_id, &req.ordered_ids).await?;
    Ok(ApiResponse::ok(()))
}

// ====== V1 Executions ======

/// GET /{id}/executions (nested) — paginated execution history
pub async fn list_executions_v1(
    State(state): State<AppState>,
    Path((_ws_id, loop_id)): Path<(i64, i64)>,
    Query(q): Query<ExecutionPageQuery>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验父 loop 属于路径 workspace，再列其执行历史
    workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, _ws_id).await?;
    let limit = q.limit.unwrap_or(DEFAULT_PAGE_LIMIT).min(MAX_PAGE_LIMIT);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * limit;
    let records = state.db.list_loop_executions(loop_id, limit, offset, q.hours).await?;
    let total = state.db.count_loop_executions(loop_id).await?;
    let exec_ids: Vec<i64> = records.iter().map(|r| r.id).collect();
    let pending_counts = state.db.count_pending_approvals_by_execution_ids(&exec_ids).await?;
    let mut items: Vec<LoopExecutionDto> = records.into_iter().map(Into::into).collect();
    for item in &mut items {
        item.pending_approval_count = pending_counts.get(&item.id).copied().unwrap_or(0);
        let step_execs = state.db.list_loop_step_executions(item.id).await?;
        let mut enriched: Vec<LoopStepExecutionDto> = step_execs.into_iter().map(|se| se.into()).collect();
        for dto in &mut enriched {
            enrich_step_execution_with_usage(&state.db, dto).await;
        }
        item.token_summary = Some(aggregate_tokens_from_step_dtos(&enriched));
    }
    Ok(ApiResponse::ok(serde_json::json!({
        "items": items, "total": total, "page": page, "limit": limit,
    })))
}

/// GET /{id}/executions/{eid} (nested) — single execution detail
pub async fn get_execution_v1(
    State(state): State<AppState>,
    Path((_ws_id, loop_id, eid)): Path<(i64, i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验父 loop 属于路径 workspace（后续 exec.loop_id != loop_id 校验子资源归属）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, loop_id, _ws_id).await?;
    let exec = state.db.get_loop_execution(eid).await?
        .ok_or(AppError::NotFound)?;
    if exec.loop_id != loop_id {
        return Err(AppError::BadRequest("execution 不属于该 loop".to_string()));
    }
    let step_execs = state.db.list_loop_step_executions(eid).await?;
    let loop_name = state.db.get_loop(loop_id).await?
        .map(|l| l.name).unwrap_or_default();
    let mut enriched: Vec<LoopStepExecutionDto> = vec![];
    for se in step_execs {
        let mut dto: LoopStepExecutionDto = se.into();
        if dto.step_id == -1 {
            if let Ok(Some(todo)) = state.db.get_todo(dto.todo_id).await {
                dto.step_name = Some(format!("[异常处理] {}", todo.title));
            }
        } else if let Ok(Some(ls)) = state.db.get_loop_step(dto.step_id).await {
            dto.step_name = Some(ls.name);
        }
        enrich_step_execution_with_usage(&state.db, &mut dto).await;
        enriched.push(dto);
    }
    let token_summary = aggregate_tokens_from_step_dtos(&enriched);
    Ok(ApiResponse::ok(LoopExecutionDetail {
        execution: exec.into(), step_executions: enriched,
        loop_name, token_summary,
    }))
}

/// POST /{id}/executions/{eid}/steps/{seid}/approve (nested) — approve step execution
pub async fn approve_step_execution_v1(
    State(state): State<AppState>,
    Path((_ws_id, _loop_id, execution_id, step_execution_id)): Path<(i64, i64, i64, i64)>,
    Json(req): Json<ApproveStepExecutionRequest>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验父 loop 属于路径 workspace（后续 loop_exec.loop_id != _loop_id 校验归属）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, _loop_id, _ws_id).await?;
    if req.rating < 0 || req.rating > 100 {
        return Err(AppError::BadRequest("评分必须在 0-100 之间".to_string()));
    }
    let step_execs = state.db.list_loop_step_executions(execution_id).await?;
    let step_exec = step_execs.iter()
        .find(|se| se.id == step_execution_id)
        .ok_or(AppError::NotFound)?;
    let loop_exec = state.db.get_loop_execution(execution_id).await?
        .ok_or(AppError::NotFound)?;
    if loop_exec.loop_id != _loop_id {
        return Err(AppError::BadRequest("该 execution 不属于指定的 loop".to_string()));
    }
    if step_exec.status != "pending_approval" {
        return Err(AppError::BadRequest("该环节当前不需要审批".to_string()));
    }
    let min_rating = step_exec.min_rating.unwrap_or(0);
    let final_status = if req.rating >= min_rating { "success" } else { "failed" };
    state.db.approve_step_execution(
        step_execution_id, req.rating, final_status, req.comment.as_deref(),
    ).await?;
    let _ = state.tx.send(crate::executor_service::ExecEvent::ReviewStatusChanged {
        record_id: step_exec.execution_record_id.unwrap_or(0),
        todo_id: step_exec.todo_id,
        review_status: final_status.to_string(),
    });
    let runner = state.loop_runner.as_ref()
        .ok_or_else(|| AppError::Internal("loop runner not ready".to_string()))?;
    runner.resume_loop_execution(execution_id).await;
    Ok(ApiResponse::ok(serde_json::json!({
        "step_execution_id": step_execution_id,
        "rating": req.rating,
        "status": final_status,
    })))
}

// ====== V1 路由表 ======
// 所有路径为相对路径，外层已剥离 /api/v1/workspaces/{ws}/loops 前缀。
// 外层 nesting 时 axum 会将 {ws} 路径参数传递到本路由器的各 handler。

/// 返回 v1 路由表（相对路径），由外层嵌套在 /api/v1/workspaces/{ws}/loops 下。
pub fn v1_routes() -> axum::Router<AppState> {
    use axum::routing::{get, post, put};
    axum::Router::new()
        .route("/", get(list_loops_v1).post(create_loop_v1))
        // merge 必须注册在 /{id} 之前：axum 字面量路由优先匹配，否则 "merge" 会被当 loop id。
        // merge_loops / import_loops / import_preview handler 内部已按 workspace 隔离解析。
        .route("/merge", post(merge_loops))
        .route("/import-preview", post(import_preview))
        .route("/import", post(import_loops))
        .route("/{id}", get(get_loop_v1).put(update_loop_v1).delete(delete_loop_v1))
        .route("/{id}/status", put(update_loop_status_v1))
        .route("/{id}/tags", put(update_loop_tags_v1))
        .route("/{id}/duplicate", post(duplicate_loop_v1))
        .route("/{id}/trigger", post(trigger_loop_v1))
        .route("/{id}/triggers", get(list_triggers_v1).post(create_trigger_v1))
        .route("/{id}/triggers/{tid}", put(update_trigger_v1).delete(delete_trigger_v1))
        .route("/{id}/steps", get(list_loop_steps_v1).post(create_loop_step_v1))
        .route("/{id}/steps/reorder", post(reorder_loop_steps_v1))
        .route("/{id}/steps/{sid}", put(update_loop_step_v1).delete(delete_loop_step_v1))
        .route("/{id}/executions", get(list_executions_v1))
        .route("/{id}/executions/{eid}", get(get_execution_v1))
        .route("/{id}/executions/{eid}/steps/{seid}/approve", post(approve_step_execution_v1))
}

// ====== 导入工作空间逐 loop 解析的单元测试 ======
// 覆盖 resolve 优先级、gate、共享 todo 归属、preview 匹配。
// 用 in-memory SQLite，不依赖 AppState/Router，直接测私有 helper。
#[cfg(test)]
mod workspace_resolve_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value, clippy::bool_assert_comparison)]
    use super::*;
    use crate::db::Database;
    use crate::models::{LoopExportData, LoopExportItem, LoopStepExportItem, TodoExportItem};

    async fn setup_db() -> Database {
        Database::new(":memory:").await.unwrap()
    }

    // 构造一个最小可用 LoopExportItem，只关心 name/workspace_id/abnormal_handler，
    // 其余字段填零值——测试只验证工作空间解析逻辑，不跑完整导入。
    fn mk_loop(name: &str, ws_id: Option<i64>, handler_todo_id: Option<String>) -> LoopExportItem {
        LoopExportItem {
            id: format!("@loop_{}", name),
            name: name.to_string(),
            description: String::new(),
            icon: String::new(),
            color: String::new(),
            status: "paused".to_string(),
            webhook_enabled: false,
            limits_config: serde_json::json!({}),
            review_template_id: None,
            review_template_name: None,
            abnormal_handler_todo_id: handler_todo_id,
            abnormal_handler_todo_title: None,
            abnormal_handler_trigger_on: vec![],
            tag_ids: vec![],
            tag_names: vec![],
            triggers: vec![],
            steps: vec![],
            workspace_id: ws_id,
            workspace_path: None,
        }
    }

    // 带一个 step（引用 todo_id）的 loop，用于共享 todo 归属测试。
    fn mk_loop_with_step(name: &str, ws_id: Option<i64>, todo_id: &str, todo_title: &str) -> LoopExportItem {
        let mut l = mk_loop(name, ws_id, None);
        l.steps.push(LoopStepExportItem {
            id: format!("@step_{}_{}", name, todo_id),
            name: format!("step-{}", todo_id),
            description: String::new(),
            todo_id: todo_id.to_string(),
            todo_title: todo_title.to_string(),
            order_index: 0,
            run_mode: "auto".to_string(),
            skip_on_source_failed: false,
            min_rating: None,
            unrated_policy: "skip".to_string(),
            on_success: "continue".to_string(),
            success_goto_step_id: None,
            success_goto_step_name: None,
            on_rating_fail: "stop".to_string(),
            fail_goto_step_id: None,
            fail_goto_step_name: None,
            review_type: "none".to_string(),
            enabled: true,
        });
        l
    }

    fn mk_todo(id: &str, title: &str) -> TodoExportItem {
        TodoExportItem {
            id: id.to_string(),
            title: title.to_string(),
            prompt: String::new(),
            status: "pending".to_string(),
            executor: None,
            scheduler_enabled: false,
            webhook_enabled: false,
            acceptance_criteria: None,
            auto_review_enabled: false,
            review_template_id: None,
            review_template_name: None,
            kind: "0".to_string(),
            tag_ids: vec![],
            tag_names: vec![],
            is_abnormal_handler: false,
            action_type: None,
            action_key: None,
            workspace_id: None,
            workspace_path: None,
        }
    }

    #[tokio::test]
    async fn test_resolve_loop_workspace_found() {
        // 已登记的 id → 返回 (id, path, name)
        let db = setup_db().await;
        let id = db.create_project_directory("/tmp/ws-a", Some("A"), false, false).await.unwrap();
        let r = resolve_loop_workspace(&db, Some(id)).await.unwrap();
        let (rid, path, name) = r.expect("已存在的工作空间应能解析");
        assert_eq!(rid, id);
        assert_eq!(path, "/tmp/ws-a");
        assert_eq!(name.as_deref(), Some("A"));
    }

    #[tokio::test]
    async fn test_resolve_loop_workspace_missing_and_none() {
        // 不存在的 id → None；入参 None → None
        let db = setup_db().await;
        assert!(resolve_loop_workspace(&db, Some(99999)).await.unwrap().is_none());
        assert!(resolve_loop_workspace(&db, None).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_resolve_ws_pair_override_beats_global_and_exported() {
        // override 优先于全局与导出值
        let db = setup_db().await;
        let a = db.create_project_directory("/tmp/ws-a", Some("A"), false, false).await.unwrap();
        let b = db.create_project_directory("/tmp/ws-b", Some("B"), false, false).await.unwrap();
        let c = db.create_project_directory("/tmp/ws-c", Some("C"), false, false).await.unwrap();
        let l = mk_loop("L", Some(c), None);
        let global = (a, "/tmp/ws-a".to_string());
        let (id, path) = resolve_loop_ws_pair(&db, &l, Some(&global), Some(b)).await.unwrap();
        assert_eq!(id, b);
        assert_eq!(path.as_deref(), Some("/tmp/ws-b"));
    }

    #[tokio::test]
    async fn test_resolve_ws_pair_global_when_no_override() {
        // 无 override 时全局覆盖导出值
        let db = setup_db().await;
        let a = db.create_project_directory("/tmp/ws-a", Some("A"), false, false).await.unwrap();
        let c = db.create_project_directory("/tmp/ws-c", Some("C"), false, false).await.unwrap();
        let l = mk_loop("L", Some(c), None);
        let global = (a, "/tmp/ws-a".to_string());
        let (id, _) = resolve_loop_ws_pair(&db, &l, Some(&global), None).await.unwrap();
        assert_eq!(id, a);
    }

    #[tokio::test]
    async fn test_resolve_ws_pair_exported_when_no_global() {
        // 无 override 无全局 → 用导出 id 重新解析
        let db = setup_db().await;
        let c = db.create_project_directory("/tmp/ws-c", Some("C"), false, false).await.unwrap();
        let l = mk_loop("L", Some(c), None);
        let (id, path) = resolve_loop_ws_pair(&db, &l, None, None).await.unwrap();
        assert_eq!(id, c);
        assert_eq!(path.as_deref(), Some("/tmp/ws-c"));
    }

    #[tokio::test]
    async fn test_resolve_ws_pair_dangling_sentinel_zero() {
        // 导出 id 在当前库不存在且无全局无 override → 哨兵 (0, None)
        let db = setup_db().await;
        let l = mk_loop("L", Some(99999), None);
        let (id, path) = resolve_loop_ws_pair(&db, &l, None, None).await.unwrap();
        assert_eq!(id, 0);
        assert!(path.is_none());
    }

    #[test]
    fn test_gate_unmatched_loops() {
        // 全部已匹配 → Ok；任一为 0 → 报错列出名字
        let ok = vec![
            LoopResolved { name: "L1".into(), ws_id: 1, ws_path: None },
            LoopResolved { name: "L2".into(), ws_id: 2, ws_path: None },
        ];
        assert!(gate_unmatched_loops(&ok).is_ok());

        let bad = vec![
            LoopResolved { name: "L1".into(), ws_id: 1, ws_path: None },
            LoopResolved { name: "L2".into(), ws_id: 0, ws_path: None },
        ];
        let err = gate_unmatched_loops(&bad).unwrap_err();
        let msg = match err { AppError::BadRequest(m) => m, _ => String::new() };
        assert!(msg.contains("L2"), "错误信息应列出未匹配 loop 名，got: {}", msg);
    }

    #[tokio::test]
    async fn test_build_todo_workspace_map_shared_todo_first_loop_wins() {
        // 两个 loop 共享同一条 todo；todo 应归属首个引用它的 loop 的工作空间，并打一次 warning
        let db = setup_db().await;
        let a = db.create_project_directory("/tmp/ws-a", Some("A"), false, false).await.unwrap();
        let b = db.create_project_directory("/tmp/ws-b", Some("B"), false, false).await.unwrap();
        let l1 = mk_loop_with_step("L1", Some(a), "@todo_x", "TX");
        let l2 = mk_loop_with_step("L2", Some(b), "@todo_x", "TX");
        let data = LoopExportData {
            version: "1.0".into(),
            export_type: "loop".into(),
            created_at: String::new(),
            source: "test".into(),
            schema_version: 1,
            tags: vec![],
            review_templates: vec![],
            todos: vec![mk_todo("@todo_x", "TX")],
            loops: vec![l1, l2],
        };
        let resolved = vec![
            LoopResolved { name: "L1".into(), ws_id: a, ws_path: Some("/tmp/ws-a".into()) },
            LoopResolved { name: "L2".into(), ws_id: b, ws_path: Some("/tmp/ws-b".into()) },
        ];
        let mut warnings = Vec::new();
        let todo_ws = build_todo_workspace_map(&data, &resolved, &mut warnings);
        // 共享 todo 归首 loop L1 的工作空间 A
        let (wid, wpath) = todo_ws.get("@todo_x").expect("todo 应被映射");
        assert_eq!(*wid, a);
        assert_eq!(wpath.as_deref(), Some("/tmp/ws-a"));
        // 只打一次共享 warning
        assert_eq!(warnings.iter().filter(|w| w.warning_type == "shared_todo_workspace").count(), 1);
        assert!(warnings[0].message.contains("TX"));
        assert!(warnings[0].message.contains("L1"));
    }

    #[tokio::test]
    async fn test_build_preview_loops_matched_and_unmatched() {
        // 一条 loop 原 id 存在→matched + resolved=id；另一条不存在→unmatched + resolved=0
        let db = setup_db().await;
        let a = db.create_project_directory("/tmp/ws-a", Some("A"), false, false).await.unwrap();
        let data = LoopExportData {
            version: "1.0".into(),
            export_type: "loop".into(),
            created_at: String::new(),
            source: "test".into(),
            schema_version: 1,
            tags: vec![],
            review_templates: vec![],
            todos: vec![],
            loops: vec![
                mk_loop("L1", Some(a), None),
                mk_loop("L2", Some(99999), None),
            ],
        };
        let previews = build_preview_loops(&db, &data).await.unwrap();
        assert_eq!(previews.len(), 2);
        assert!(previews[0].source_matched);
        assert_eq!(previews[0].resolved_workspace_id, a);
        assert_eq!(previews[0].resolved_workspace_name.as_deref(), Some("A"));
        assert!(!previews[1].source_matched);
        assert_eq!(previews[1].resolved_workspace_id, 0);
        assert!(previews[1].resolved_workspace_name.is_none());
    }
}
