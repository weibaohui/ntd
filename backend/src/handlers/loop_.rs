//! Loop Studio HTTP handlers（044 瘦身后）。
//!
//! 环路降级为「工艺的运行时承载」后，只剩只读查询与运行态操作：
//! - `GET /`                              列表
//! - `GET /{id}`                          详情
//! - `DELETE /{id}`                       删除（级联清环节/执行记录）
//! - `PUT /{id}/status`                   启停
//! - `PUT /{id}/tags`                     标签
//! - `GET /{id}/executions`               运行历史（分页）
//! - `GET /{id}/executions/{eid}`         单次执行详情（含待审批门禁 id）
//! - `GET /stats`                         workspace 聚合统计
//! - `POST /batch-delete` `/batch/workspace` `/batch/copy-workspace`
//!
//! 已下线：创建/更新/复制/触发、触发器 CRUD、环节 CRUD/重排、导入导出/merge、
//! 旧评分制审批（改由 process.rs 的门禁制 approve_gate 承担）。
//! v0 路由组（loop_routes）整体删除——原本已无挂载点，是迁移遗留死代码。
use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::handlers::{workspace_guard, AppError, AppState};
use crate::models::{
    self,
    ApiResponse, BatchCopyLoopWorkspaceRequest, BatchUpdateLoopWorkspaceRequest,
    BatchWorkspaceResult, LoopDetail, LoopDto, LoopExecutionDetail, LoopExecutionDto,
    LoopExecutionTokenSummary, LoopListItem, LoopStepExecutionDto,
    UpdateLoopStatusRequest, UpdateTagsRequest,
};

// 默认分页：列表类接口未显式传 limit 时的兜底值，与历史行为一致。
const DEFAULT_PAGE_LIMIT: u64 = 20;
// 分页上限：防止客户端一次拉取过多数据拖垮 DB。
const MAX_PAGE_LIMIT: u64 = 100;

/// 批量删除环路请求体（v1 batch-delete 复用）。
#[derive(Deserialize)]
pub struct BatchDeleteLoopsRequest {
    pub ids: Vec<i64>,
}

/// GET /stats 的查询参数：hours 缺省或 0 表示全时段统计。
#[derive(Deserialize)]
pub struct LoopStatsQuery {
    pub hours: Option<i64>,
}

/// 执行历史分页查询参数。
#[derive(Debug, Deserialize)]
pub struct ExecutionPageQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    /// 按最近 N 小时过滤
    pub hours: Option<i64>,
}

// ====== V1 API handlers (workspace-scoped paths, nested under /api/v1/workspaces/{ws}/loops) ======
// V1 handlers 从路径参数取 ws_id，并通过 workspace_guard 校验资源归属，保证 workspace 隔离。

/// GET / (nested) — list loops filtered by workspace from path
pub async fn list_loops_v1(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = state.db.list_loops_with_counts(Some(ws_id)).await?;
    let items: Vec<LoopListItem> = rows.into_iter().map(Into::into).collect();
    let loop_ids: Vec<i64> = items.iter().map(|item| item.loop_.id).collect();
    let tag_map = state.db.get_loop_tag_ids_batch(&loop_ids).await?;
    // 列表「工艺名称」列需要来源模板的 display_name/name/guid；版本快照已在 loops 行自动带出。
    // 与 tags 同走「批量查 + 注入」：去重后一次取回，避免逐 loop N+1。
    let template_ids: Vec<i64> = items
        .iter()
        .filter_map(|item| item.loop_.process_template_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let template_map = state
        .db
        .get_process_templates_by_ids(&template_ids)
        .await?
        .into_iter()
        .map(|t| (t.id, t))
        .collect::<std::collections::HashMap<i64, _>>();
    let results: Vec<LoopListItem> = items
        .into_iter()
        .map(|item| {
            let tag_ids = tag_map.get(&item.loop_.id).cloned().unwrap_or_default();
            // 无 process_template_id 或模板已被删除时传 None：字段保持缺省不序列化。
            let tpl = item
                .loop_
                .process_template_id
                .and_then(|tid| template_map.get(&tid).cloned());
            // with_process_template 定义在 LoopDto 上（注入 display_name/name/guid），
            // LoopListItem 只是 flatten 包装，因此先 with_tags 再就地改 loop_ 字段。
            let mut tagged = item.with_tags(tag_ids);
            tagged.loop_ = tagged.loop_.with_process_template(tpl);
            tagged
        })
        .collect();
    Ok(ApiResponse::ok(results))
}

/// GET /{id} (nested) — loop 详情
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
    // 注入来源工艺模板名称：详情页「来源工艺」面包屑需要显示名，
    // 仅当环路是工艺实例化产物时才查一次模板表，普通环路零额外查询。
    let process_meta = match detail.loop_.process_template_id {
        Some(template_id) => state.db.get_process_template_by_id(template_id).await?,
        None => None,
    };
    detail.loop_ = detail.loop_.with_process_template(process_meta);
    Ok(ApiResponse::ok(detail))
}

/// DELETE /{id} (nested) — delete loop, 先校验归属再级联删除
pub async fn delete_loop_v1(
    State(state): State<AppState>,
    Path((_ws_id, id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    // V1 隔离：校验 loop 属于路径 workspace（verify 已含存在性校验，替换原 get_loop 探测）
    workspace_guard::verify_loop_belongs_to_ws(&state.db, id, _ws_id).await?;
    // 044：触发器表已删，cron 调度器已下线，删除仅做级联清理子表（环节/执行记录）。
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
        // 记录不存在属正常情况（旧数据或已被清理），保持缺省返回。
        Ok(None) => return,
        // 查询出错则记一条 warn：token 用量字段会缺省，前端不显示用量数据，
        // 但服务端应留下可排查的线索（NTD-009：静默丢弃 DB 错误会导致问题难以追踪）。
        Err(e) => {
            tracing::warn!("获取 execution_record #{record_id} 失败，step_execution #{} token 用量将从 DTO 缺省: {e}", dto.id);
            return;
        }
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

/// 为 pending_approval 的环节执行注入待审批门禁 id。
/// 044 审批改门禁制：前端凭 pending_gate_id 直接调门禁审批接口，无需再查审计接口。
/// 仅当环节处于 pending_approval 且存在 pending 的 human_approval 门禁时写入，否则保持 None。
async fn populate_pending_gate_id(
    db: &crate::db::Database,
    dto: &mut LoopStepExecutionDto,
) {
    // 非待审批状态没有「待审批」门禁，直接跳过，避免无谓的门禁查询。
    // 必须同时认两条暂停路径（与 db::count_pending_approvals_by_execution_ids 的 OR 条件一致，NTD-004）：
    //   - 旧评分路径：暂停时写 approval_status='pending'；
    //   - 工艺 phase_driver 路径：暂停时只写 status='pending_approval'，不写 approval_status。
    // 只看 approval_status 会漏掉 phase_driver 路径，导致前端审批框出现却拿不到 gate id（报「未找到待审批门禁」）。
    if dto.status != "pending_approval" && dto.approval_status.as_deref() != Some("pending") {
        return;
    }
    match db.list_loop_step_execution_gates(dto.id).await {
        // 取首个 pending 的 human_approval 门禁：工艺定义里人工审批环节只有一个此类门禁
        Ok(gates) => {
            dto.pending_gate_id = gates
                .into_iter()
                .find(|g| g.gate_type == "human_approval" && g.status == "pending")
                .map(|g| g.id);
        }
        // 门禁查询失败会让前端失去审批入口（pending_gate_id 保留 None），但不阻塞执行记录展示；
        // 需记录 warn 让运维人员排查，避免用户报「审批按钮不显示」时无服务端日志可查（NTD-009）。
        Err(e) => tracing::warn!("查询 step_execution #{} 门禁列表失败，pending_gate_id 将保持 None: {e}", dto.id),
    }
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
    // q.hours 来自 query string 解析为 i64；DAO 按 u32 接收（负数无意义，折叠为 None = 全时段）。
    let hours = q.hours.and_then(|h| u32::try_from(h).ok());
    let records = state.db.list_loop_executions(loop_id, limit, offset, hours).await?;
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
            populate_pending_gate_id(&state.db, dto).await;
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
    // 为每个 step execution 补充 step_name（来自 loop_steps 表）、token 用量与待审批门禁 id
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
        enrich_step_execution_with_usage(&state.db, &mut dto).await;
        populate_pending_gate_id(&state.db, &mut dto).await;
        enriched.push(dto);
    }
    // 聚合 token 汇总：直接从已 enrich 的 DTO 字段聚合，避免重复查询数据库
    let token_summary = aggregate_tokens_from_step_dtos(&enriched);
    Ok(ApiResponse::ok(LoopExecutionDetail {
        execution: exec.into(), step_executions: enriched,
        loop_name, token_summary,
    }))
}

/// GET /stats — 当前 workspace 的 loop 聚合统计。
pub async fn get_loop_stats_v1(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    Query(params): Query<LoopStatsQuery>,
) -> Result<impl IntoResponse, AppError> {
    // params.hours 为 i64，DAO 按 u32 接收（负数折叠为 None = 全时段统计）。
    let hours = params.hours.and_then(|h| u32::try_from(h).ok());
    let stats = state.db.get_loop_stats_for_workspace(Some(ws_id), hours).await?;
    Ok(ApiResponse::ok(stats))
}

/// POST /batch/workspace — 批量移动 loop 到其他 workspace。
pub async fn batch_move_loops_workspace_v1(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    Json(req): Json<BatchUpdateLoopWorkspaceRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    workspace_guard::verify_loops_belong_to_ws(&state.db, &req.ids, ws_id).await?;
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

/// POST /batch/copy-workspace — 批量复制 loop 到其他 workspace。
pub async fn batch_copy_loops_workspace_v1(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    Json(req): Json<BatchCopyLoopWorkspaceRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    workspace_guard::verify_loops_belong_to_ws(&state.db, &req.ids, ws_id).await?;
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

/// POST /batch-delete (nested) - 批量删除环路（workspace 隔离）。
/// 044：触发器表与 cron 调度器已下线，删除仅做归属校验 + 级联清理。
pub async fn batch_delete_loops_v1(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    Json(req): Json<BatchDeleteLoopsRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    // V1 隔离：校验所有 loop 属于路径 workspace（verify 已含存在性校验）。
    workspace_guard::verify_loops_belong_to_ws(&state.db, &req.ids, ws_id).await?;
    // 使用 DB 层 batch_delete_loops（单 SQL 事务化批量 DELETE，级联清环节/执行记录），
    // 避免逐条 delete_loop 产生 N 次独立 SQL 调用、部分失败时无法回滚（NTD-008）。
    let deleted = state.db.batch_delete_loops(&req.ids).await?;
    // deleted 是实际删除行数，total 是请求的 ID 数（两者可能不等，如某些 ID 不存在）。
    Ok(ApiResponse::ok(serde_json::json!({
        "deleted": deleted,
        "total": req.ids.len(),
    })))
}

// ====== 路由表 ======
//
// 044：仅保留只读 + 运行态接口。创建/更新/复制/触发、触发器 CRUD、环节 CRUD/重排、
// 导入导出/merge、旧评分制审批均已删除（前端唯一执行入口是「创建任务选工艺环路」）。

pub fn v1_routes() -> axum::Router<AppState> {
    use axum::routing::{get, post, put};
    axum::Router::new()
        .route("/", get(list_loops_v1))
        // 静态段（如 /stats, /batch/*）与动态段（/{id}）同层注册。
        // Axum 0.8 底层 matchit 会按静态优先匹配，无需依赖注册先后顺序。
        .route("/stats", get(get_loop_stats_v1))
        .route("/batch/workspace", post(batch_move_loops_workspace_v1))
        .route("/batch/copy-workspace", post(batch_copy_loops_workspace_v1))
        .route("/batch-delete", post(batch_delete_loops_v1))
        .route("/{id}", get(get_loop_v1).delete(delete_loop_v1))
        .route("/{id}/status", put(update_loop_status_v1))
        .route("/{id}/tags", put(update_loop_tags_v1))
        .route("/{id}/executions", get(list_executions_v1))
        .route("/{id}/executions/{eid}", get(get_execution_v1))
}
