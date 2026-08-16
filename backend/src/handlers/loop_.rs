//! Loop Studio HTTP handlers（044 瘦身后）。
//!
//! 环路降级为「工艺的运行时承载」后，只剩只读查询与运行态操作：
//! - `GET /`                              列表
//! - `GET /{id}`                          详情
//! - `DELETE /{id}`                       删除（级联清环节/执行记录）
//! - `PUT /{id}/status`                   启停
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
    LoopExecutionTokenSummary, GateResultDto, LoopListItem, LoopStepExecutionDto,
    UpdateLoopStatusRequest,
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
    // 列表「工艺名称」列需要来源模板的 display_name/name/guid；版本快照已在 loops 行自动带出。
    // 批量查 + 注入：去重后一次取回，避免逐 loop N+1。
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
            // 无 process_template_id 或模板已被删除时传 None：字段保持缺省不序列化。
            let tpl = item
                .loop_
                .process_template_id
                .and_then(|tid| template_map.get(&tid).cloned());
            // with_process_template 定义在 LoopDto 上（注入 display_name/name/guid），
            // LoopListItem 只是 flatten 包装，直接就地改 loop_ 字段。
            let mut tagged = item;
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
    let mut detail = LoopDetail::from(view);
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
    Ok(ApiResponse::ok(LoopDto::from(updated)))
}

/// 批量把 usage token 字段填进多个 step DTO。
/// 一次性取所有 execution_record 的 usage（按 record id 分组），替代逐 step 的
/// `get_execution_record`，消除执行历史列表/详情的 N+1（091 性能优化）。
async fn enrich_step_executions_usage_batch(
    db: &crate::db::Database,
    dtos: &mut [LoopStepExecutionDto],
) {
    // 只有关联了 execution_record 的 step 才有 usage 数据；空集直接返回避免非法 IN()。
    let record_ids: Vec<i64> = dtos.iter().filter_map(|d| d.execution_record_id).collect();
    if record_ids.is_empty() {
        return;
    }
    let records = match db.get_execution_records_by_ids(&record_ids).await {
        Ok(m) => m,
        // 批量取失败不阻塞展示，token 字段保持缺省；warn 留排查线索（NTD-009）。
        Err(e) => {
            tracing::warn!("批量获取 execution_records 失败 (ids={record_ids:?}): {e}");
            return;
        }
    };
    for dto in dtos.iter_mut() {
        let Some(rid) = dto.execution_record_id else { continue; };
        let Some(record) = records.get(&rid) else { continue; };
        let Some(usage) = record.usage.as_ref() else { continue; };
        // usage 字段是 u64，转 i64 送入 DTO 避免前端大数溢出。
        dto.input_tokens = Some(usage.input_tokens as i64);
        dto.output_tokens = Some(usage.output_tokens as i64);
        dto.cache_read_input_tokens = usage.cache_read_input_tokens.map(|v| v as i64);
        dto.cache_creation_input_tokens = usage.cache_creation_input_tokens.map(|v| v as i64);
        dto.total_cost_usd = usage.total_cost_usd;
    }
}

/// 批量填充门禁评价摘要 + 待审批门禁 id（需求 047）。
/// 一次性取所有 step_execution 的门禁（按 step_execution id 分组），替代逐 step 的
/// `list_loop_step_execution_gates`，消除 N+1（091 性能优化）。
async fn populate_gate_results_batch(
    db: &crate::db::Database,
    dtos: &mut [LoopStepExecutionDto],
) {
    let step_ids: Vec<i64> = dtos.iter().map(|d| d.id).collect();
    if step_ids.is_empty() {
        return;
    }
    let gates_by_step = match db.list_loop_step_execution_gates_by_step_ids(&step_ids).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("批量查询门禁失败 (step_ids={step_ids:?}): {e}");
            return;
        }
    };
    for dto in dtos.iter_mut() {
        let gates = gates_by_step.get(&dto.id);
        // pending_gate_id：同时认旧评分路径（approval_status=pending）与 phase_driver 路径（status=pending_approval），
        // 与 count_pending_approvals 一致，避免漏掉 phase_driver 导致前端审批框拿不到 gate id。
        if dto.status == "pending_approval" || dto.approval_status.as_deref() == Some("pending") {
            dto.pending_gate_id = gates
                .and_then(|gs| {
                    gs.iter()
                        .find(|g| g.gate_type == "human_approval" && g.status == "pending")
                })
                .map(|g| g.id);
        }
        // gate_results：全部门禁摘要，供前端展示通过/失败/失败原因。
        dto.gate_results = gates
            .map(|gs| {
                gs.iter()
                    .map(|g| GateResultDto {
                        id: g.id,
                        gate_type: g.gate_type.clone(),
                        gate_name: g.gate_name.clone(),
                        status: g.status.clone(),
                        result: g.result.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
}

/// 批量填充评分来源评审 record id（需求 047）。
/// 一次性反查所有原 step record 的评审实例，替代逐 step 的
/// `find_review_record_id_by_source`，消除 N+1（091 性能优化）。语义与单条版一致。
async fn populate_review_record_id_batch(
    db: &crate::db::Database,
    dtos: &mut [LoopStepExecutionDto],
) {
    let source_ids: Vec<i64> = dtos.iter().filter_map(|d| d.execution_record_id).collect();
    if source_ids.is_empty() {
        return;
    }
    let review_by_source = match db.find_review_record_id_by_source_batch(&source_ids).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("批量查询评分来源失败 (source_ids={source_ids:?}): {e}");
            return;
        }
    };
    for dto in dtos.iter_mut() {
        let Some(source_id) = dto.execution_record_id else { continue; };
        // map 未命中即该 step 未被评审过，保持 None。
        dto.review_record_id = review_by_source.get(&source_id).copied();
    }
}

/// 从已 enrich 的 LoopStepExecutionDto 字段直接聚合 Token 消耗汇总，
/// 不再重复查询数据库（原有的 aggregate_step_execution_tokens 存在 N+1 问题）。
/// 前置条件：调用方必须先通过 `enrich_step_executions_usage_batch` 填充 DTO token 字段。
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
    // 一次批量取全部 exec 的 step_executions，按 exec_id 分组（替代逐 exec 查询，消除 N+1）。
    let steps_by_exec = state.db.list_loop_step_executions_by_exec_ids(&exec_ids).await?;
    let mut items: Vec<LoopExecutionDto> = records.into_iter().map(Into::into).collect();
    for item in &mut items {
        item.pending_approval_count = pending_counts.get(&item.id).copied().unwrap_or(0);
    }
    // 列表响应（LoopExecutionDto）不含 step 明细，只需 token 汇总。
    // 091：把全页所有 execution 的 step DTO 扁平收集后「一次」批量 enrich usage，
    // 再按 loop_execution_id 分组聚合——避免在 for item 循环内逐 execution 调 enrich：
    // enrich 内部一次 get_execution_records_by_ids，逐 exec 调用 = 每页 N 次往返（N+1 未消除）。
    let mut flat_steps: Vec<LoopStepExecutionDto> = items
        .iter()
        .filter_map(|item| {
            steps_by_exec
                .get(&item.id)
                .map(|v| v.iter().cloned().map(Into::into).collect::<Vec<_>>())
        })
        .flatten()
        .collect();
    enrich_step_executions_usage_batch(&state.db, &mut flat_steps).await;
    // DTO 自带 loop_execution_id，据此分组（move，无额外 clone），逐 exec 聚合 token。
    let mut tokens_by_exec: std::collections::HashMap<i64, Vec<LoopStepExecutionDto>> =
        std::collections::HashMap::new();
    for dto in flat_steps {
        tokens_by_exec.entry(dto.loop_execution_id).or_default().push(dto);
    }
    for item in &mut items {
        let group = tokens_by_exec.remove(&item.id).unwrap_or_default();
        item.token_summary = Some(aggregate_tokens_from_step_dtos(&group));
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
    let mut enriched: Vec<LoopStepExecutionDto> = step_execs.into_iter().map(Into::into).collect();
    // 三项 enrich 一次性批量取回（usage / 门禁 / 评审来源），消除逐 step 的 N+1（091 性能优化）。
    enrich_step_executions_usage_batch(&state.db, &mut enriched).await;
    populate_gate_results_batch(&state.db, &mut enriched).await;
    populate_review_record_id_batch(&state.db, &mut enriched).await;
    // step_name：单次执行环节数很少（通常 ≤7），逐环节查可接受，保留原 step_id=-1 异常处理兜底。
    for dto in &mut enriched {
        // step_id=-1 是异常处理步骤，没有对应的 loop_step，用 todo 标题代替
        if dto.step_id == -1 {
            if let Ok(Some(todo)) = state.db.get_todo(dto.todo_id).await {
                dto.step_name = Some(format!("[异常处理] {}", todo.title));
            }
        } else if let Ok(Some(ls)) = state.db.get_loop_step(dto.step_id).await {
            // 读取 loop_step 的名称（仅用于显示）
            dto.step_name = Some(ls.name);
        }
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
        .get_workspace_by_id(req.workspace_id)
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
        .get_workspace_by_id(req.workspace_id)
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
        .route("/{id}/executions", get(list_executions_v1))
        .route("/{id}/executions/{eid}", get(get_execution_v1))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    // ====== aggregate_tokens_from_step_dtos ======

    /// 空列表应返回全零汇总。
    #[test]
    fn test_aggregate_tokens_from_step_dtos_empty() {
        let result = aggregate_tokens_from_step_dtos(&[]);
        assert_eq!(result.total_input_tokens, 0);
        assert_eq!(result.total_output_tokens, 0);
        assert_eq!(result.total_cache_read_input_tokens, 0);
        assert_eq!(result.total_cache_creation_input_tokens, 0);
        assert_eq!(result.total_cost_usd, 0.0);
    }

    /// 单条 DTO：正确读取各 token 字段。
    #[test]
    fn test_aggregate_tokens_from_step_dtos_single() {
        let dtos = vec![LoopStepExecutionDto {
            input_tokens: Some(100),
            output_tokens: Some(50),
            cache_read_input_tokens: Some(10),
            cache_creation_input_tokens: Some(5),
            total_cost_usd: Some(0.002),
            ..Default::default()
        }];
        let result = aggregate_tokens_from_step_dtos(&dtos);
        assert_eq!(result.total_input_tokens, 100);
        assert_eq!(result.total_output_tokens, 50);
        assert_eq!(result.total_cache_read_input_tokens, 10);
        assert_eq!(result.total_cache_creation_input_tokens, 5);
        assert!((result.total_cost_usd - 0.002).abs() < f64::EPSILON);
    }

    /// 多条 DTO：验证正确累加。
    #[test]
    fn test_aggregate_tokens_from_step_dtos_multi() {
        let dtos = vec![
            LoopStepExecutionDto {
                input_tokens: Some(100),
                output_tokens: Some(50),
                total_cost_usd: Some(0.001),
                ..Default::default()
            },
            LoopStepExecutionDto {
                input_tokens: Some(200),
                output_tokens: Some(30),
                total_cost_usd: Some(0.002),
                ..Default::default()
            },
        ];
        let result = aggregate_tokens_from_step_dtos(&dtos);
        assert_eq!(result.total_input_tokens, 300);
        assert_eq!(result.total_output_tokens, 80);
        assert!((result.total_cost_usd - 0.003).abs() < f64::EPSILON);
    }

    /// 部分字段为 None 时跳过，不影响其他字段求和。
    #[test]
    fn test_aggregate_tokens_from_step_dtos_partial_none() {
        let dtos = vec![
            LoopStepExecutionDto {
                input_tokens: Some(100),
                output_tokens: None,
                ..Default::default()
            },
            LoopStepExecutionDto {
                input_tokens: None,
                output_tokens: Some(50),
                ..Default::default()
            },
        ];
        let result = aggregate_tokens_from_step_dtos(&dtos);
        assert_eq!(result.total_input_tokens, 100);
        assert_eq!(result.total_output_tokens, 50);
    }

    // ====== BatchDeleteLoopsRequest deserialization ======

    #[test]
    fn test_batch_delete_loops_request_deserialize() {
        let json = serde_json::json!({"ids": [1, 2, 3]});
        let req: BatchDeleteLoopsRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_batch_delete_loops_request_empty_ids() {
        let json = serde_json::json!({"ids": []});
        let req: BatchDeleteLoopsRequest = serde_json::from_value(json).unwrap();
        assert!(req.ids.is_empty());
    }

    // ====== LoopStatsQuery deserialization ======

    #[test]
    fn test_loop_stats_query_with_hours() {
        let json = serde_json::json!({"hours": 24});
        let q: LoopStatsQuery = serde_json::from_value(json).unwrap();
        assert_eq!(q.hours, Some(24));
    }

    #[test]
    fn test_loop_stats_query_hours_none() {
        let json = serde_json::json!({});
        let q: LoopStatsQuery = serde_json::from_value(json).unwrap();
        assert_eq!(q.hours, None);
    }

    // ====== ExecutionPageQuery deserialization ======

    #[test]
    fn test_execution_page_query_all_fields() {
        let json = serde_json::json!({"page": 2, "limit": 50, "hours": 48});
        let q: ExecutionPageQuery = serde_json::from_value(json).unwrap();
        assert_eq!(q.page, Some(2));
        assert_eq!(q.limit, Some(50));
        assert_eq!(q.hours, Some(48));
    }

    #[test]
    fn test_execution_page_query_defaults() {
        let json = serde_json::json!({});
        let q: ExecutionPageQuery = serde_json::from_value(json).unwrap();
        assert_eq!(q.page, None);
        assert_eq!(q.limit, None);
        assert_eq!(q.hours, None);
    }
}
