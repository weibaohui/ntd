//! 环节管理（steps 表）处理器。
//!
//! 步骤独立于 todo，放在此文件而非 todo.rs，明确职责边界。

use axum::extract::{Path, State};

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, StepDto, UpdateStepRequest};

// ====== 环节管理（kind=step）======
//
// 路由：
// - GET    /api/steps                    列出所有环节 + 各自的 loop 引用计数
// - GET    /api/steps/:id                单个环节详情
// - GET    /api/steps/candidates         loop 编辑器选环节用的精简候选列表

/// GET /api/steps — 列出所有环节,带"被哪些 loop 用"复用度计数
pub async fn list_steps(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<StepDto>>, AppError> {
    let rows = state.db.list_steps_with_usage_pure().await?;
    let items = rows
        .into_iter()
        .map(|(s, count)| StepDto::from(s).with_usage(count))
        .collect();
    Ok(ApiResponse::ok(items))
}

/// GET /api/steps/candidates — loop 编辑器选环节用
pub async fn list_step_candidates(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<StepDto>>, AppError> {
    let rows = state.db.list_steps_with_usage_pure().await?;
    let items = rows
        .into_iter()
        .map(|(s, count)| StepDto::from(s).with_usage(count))
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
    Ok(ApiResponse::ok(StepDto::from(s).with_usage(used_by_loop_step_count)))
}

/// PUT /api/steps/:id — 更新环节基本信息
pub async fn update_step(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateStepRequest>,
) -> Result<ApiResponse<StepDto>, AppError> {
    if req.title.trim().is_empty() {
        return Err(AppError::BadRequest("title 不能为空".to_string()));
    }
    // 校验环节存在
    state.db.get_step(id).await?.ok_or(AppError::NotFound)?;
    state
        .db
        .update_step(
            id,
            req.title.trim(),
            &req.prompt,
            req.executor.as_deref(),
            req.acceptance_criteria.as_deref(),
            req.color.as_deref(),
        )
        .await?;
    // 查回最新数据
    let s = state.db.get_step(id).await?.ok_or(AppError::NotFound)?;
    let used_by_loop_step_count = state.db.count_loop_steps_using_step(id).await?;
    Ok(ApiResponse::ok(StepDto::from(s).with_usage(used_by_loop_step_count)))
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
