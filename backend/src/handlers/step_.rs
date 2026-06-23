//! 环节管理（steps 表）处理器。
//!
//! 步骤独立于 todo，放在此文件而非 todo.rs，明确职责边界。

use axum::extract::{Path, State};

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, BatchUpdateStepExecutorRequest, BatchUpdateStepResult, CreateStepRequest, StepDto, UpdateStepRequest, UpdateTagsRequest};

// ====== 环节管理（kind=step）======
//
// 路由：
// - GET    /api/steps                    列出所有环节 + 各自的 loop 引用计数
// - POST   /api/steps                    直接创建环节（不走 createTodo+promote）
// - GET    /api/steps/:id                单个环节详情
// - GET    /api/steps/candidates         loop 编辑器选环节用的精简候选列表

/// GET /api/steps — 列出所有环节,带"被哪些 loop 用"复用度计数
pub async fn list_steps(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<StepDto>>, AppError> {
    let rows = state.db.list_steps_with_usage_pure().await?;
    // 批量查询所有环节的标签映射，消除逐条 N+1 查询
    let step_ids: Vec<i64> = rows.iter().map(|(s, _)| s.id).collect();
    let tag_map = state.db.get_step_tag_ids_batch(&step_ids).await?;
    let items: Vec<StepDto> = rows
        .into_iter()
        .map(|(s, count)| {
            let tag_ids = tag_map.get(&s.id).cloned().unwrap_or_default();
            StepDto::from(s).with_usage(count).with_tags(tag_ids)
        })
        .collect();
    Ok(ApiResponse::ok(items))
}

/// POST /api/steps — 直接创建环节。
///
/// 历史上 TodoList "新建环节" 走 createTodo + promoteTodoToStep 流程，
/// 留下孤儿 todo + 错位 id；现在 todo 与 step 彻底拆开，前端必须直建。
/// title 必填且非空，prompt/executor/acceptance_criteria 可空。
pub async fn create_step(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateStepRequest>,
) -> Result<ApiResponse<StepDto>, AppError> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title 不能为空".to_string()));
    }
    let prompt = req.prompt.unwrap_or_default();
    let step = state
        .db
        .create_step(
            title,
            &prompt,
            req.executor.as_deref(),
            req.acceptance_criteria.as_deref(),
            None, // 直建场景不绑定 source_todo_id（promote 链路才需要）
        )
        .await?;
    let tag_ids = state.db.get_step_tag_ids(step.id).await?;
    // 直建场景没有 loop 引用，usage 必为 0，但仍走 list 路径保证 DTO 字段齐全
    Ok(ApiResponse::ok(StepDto::from(step).with_usage(0).with_tags(tag_ids)))
}

/// GET /api/steps/candidates — loop 编辑器选环节用
pub async fn list_step_candidates(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<StepDto>>, AppError> {
    let rows = state.db.list_steps_with_usage_pure().await?;
    // 批量查询标签，避免 N+1
    let step_ids: Vec<i64> = rows.iter().map(|(s, _)| s.id).collect();
    let tag_map = state.db.get_step_tag_ids_batch(&step_ids).await?;
    let items: Vec<StepDto> = rows
        .into_iter()
        .map(|(s, count)| {
            let tag_ids = tag_map.get(&s.id).cloned().unwrap_or_default();
            StepDto::from(s).with_usage(count).with_tags(tag_ids)
        })
        .collect();
    Ok(ApiResponse::ok(items))
}

/// GET /api/steps/:id — 单个环节详情,带 loop 引用计数
pub async fn get_step(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<StepDto>, AppError> {
    let s = state
        .db
        .get_step(id)
        .await?
        .ok_or(AppError::NotFound)?;
    let used_by_loop_step_count = state.db.count_loop_steps_using_step(id).await?;
    let tag_ids = state.db.get_step_tag_ids(id).await?;
    Ok(ApiResponse::ok(StepDto::from(s).with_usage(used_by_loop_step_count).with_tags(tag_ids)))
}

/// PUT /api/steps/:id — 更新环节基本信息（部分更新，只传需要改的字段即可）
pub async fn update_step(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateStepRequest>,
) -> Result<ApiResponse<StepDto>, AppError> {
    // 校验环节存在，并取出当前值用于回填未传字段
    let existing = state
        .db
        .get_step(id)
        .await?
        .ok_or(AppError::NotFound)?;

    let title = req.title.unwrap_or_else(|| existing.title.clone());
    if title.trim().is_empty() {
        return Err(AppError::BadRequest("title 不能为空".to_string()));
    }
    let prompt = req.prompt.unwrap_or_else(|| existing.prompt.clone());

    state
        .db
        .update_step(
            id,
            title.trim(),
            &prompt,
            req.executor.as_deref(),
            req.acceptance_criteria.as_deref(),
        )
        .await?;
    // 查回最新数据
    let s = state.db.get_step(id).await?.ok_or(AppError::NotFound)?;
    let used_by_loop_step_count = state.db.count_loop_steps_using_step(id).await?;
    // 如果请求携带了 tag_ids，则更新标签关联；
    // 合并到同一个 handler 中避免前端分两次保存导致的部分提交风险
    if let Some(ref tag_ids) = req.tag_ids {
        if tag_ids.len() > 1 {
            return Err(AppError::BadRequest("环节只能选择一个标签".to_string()));
        }
        state.db.set_step_tags(id, tag_ids).await?;
    }
    let tag_ids = state.db.get_step_tag_ids(id).await?;
    Ok(ApiResponse::ok(StepDto::from(s).with_usage(used_by_loop_step_count).with_tags(tag_ids)))
}

/// PUT /api/steps/:id/tags — 更新环节标签（全量替换）
pub async fn update_step_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateTagsRequest>,
) -> Result<ApiResponse<StepDto>, AppError> {
    state.db.get_step(id).await?.ok_or(AppError::NotFound)?;
    // 强制单选标签约束：前端是 TagCheckCardGroup 单选，后端防御多于 1 个标签的非法请求
    if req.tag_ids.len() > 1 {
        return Err(AppError::BadRequest("环节只能选择一个标签".to_string()));
    }
    state.db.set_step_tags(id, &req.tag_ids).await?;
    let s = state.db.get_step(id).await?.ok_or(AppError::NotFound)?;
    let used_by_loop_step_count = state.db.count_loop_steps_using_step(id).await?;
    let tag_ids = state.db.get_step_tag_ids(id).await?;
    Ok(ApiResponse::ok(StepDto::from(s).with_usage(used_by_loop_step_count).with_tags(tag_ids)))
}

/// DELETE /api/steps/:id — 删除环节
pub async fn delete_step(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<()>, AppError> {
    // 环节可能被 loop 引用，由数据库外键约束保护（RESTRICT），
    // 被引用时后端返回外键冲突错误，前端会显示相应提示。
    state.db.delete_step(id).await?;
    Ok(ApiResponse::ok(()))
}

/// PUT /api/steps/batch-executor — 批量更新环节执行器
pub async fn batch_update_steps_executor(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<BatchUpdateStepExecutorRequest>,
) -> Result<ApiResponse<BatchUpdateStepResult>, AppError> {
    if req.ids.is_empty() {
        return Err(AppError::BadRequest("ids 不能为空".to_string()));
    }
    if req.executor.trim().is_empty() {
        return Err(AppError::BadRequest("executor 不能为空".to_string()));
    }

    let rows_affected = state
        .db
        .batch_update_steps_executor(&req.ids, req.executor.trim())
        .await?;
    Ok(ApiResponse::ok(BatchUpdateStepResult {
        updated_count: rows_affected as i64,
        total: req.ids.len() as i64,
    }))
}
