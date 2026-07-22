//! 快捷话术按钮的 HTTP handler（全局 CRUD，无 workspace 维度）。
//!
//! 对应路由 `/api/quick-buttons`。点按钮只在前端填入回复输入框，
//! 真正发送走原有 resume 链路，本模块不涉及执行逻辑。

use axum::{
    Router,
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, put},
    Json,
};
use serde::Deserialize;

use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;

/// 列出全部快捷按钮（按创建时间升序，先加的排前面）。
pub async fn list_quick_buttons(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let buttons = crate::db::quick_button::get_quick_buttons(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(buttons))
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateQuickButtonRequest {
    pub button_name: String,
    pub prompt_text: String,
}

/// 创建快捷按钮：校验非空 + 重名预检（给前端友好 400，而非靠 DB 约束冒泡成 500）。
pub async fn create_quick_button(
    State(state): State<AppState>,
    Json(req): Json<CreateQuickButtonRequest>,
) -> Result<impl IntoResponse, AppError> {
    let name = req.button_name.trim();
    let prompt = req.prompt_text.trim();
    if name.is_empty() || prompt.is_empty() {
        return Err(AppError::BadRequest("按钮名称和话术不能为空".to_string()));
    }
    // 重名预检：DB 虽有 UNIQUE 兜底，但提前拦截能返回 400 而非 500
    if crate::db::quick_button::get_quick_button_by_name(&state.db, name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .is_some()
    {
        return Err(AppError::BadRequest("按钮名称已存在".to_string()));
    }

    let id = crate::db::quick_button::create_quick_button(&state.db, name, prompt)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(serde_json::json!({ "id": id })))
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateQuickButtonRequest {
    pub button_name: Option<String>,
    pub prompt_text: Option<String>,
}

/// 更新快捷按钮：非空校验 + 改名时排除自身的重名检查，再落库。
pub async fn update_quick_button(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateQuickButtonRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_quick_button_update(&state.db, id, &req).await?;
    crate::db::quick_button::update_quick_button(
        &state.db,
        id,
        req.button_name.as_deref().map(str::trim),
        req.prompt_text.as_deref().map(str::trim),
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(serde_json::json!({"success": true})))
}

/// 校验更新请求：提供字段需非空；改名时检查是否与其他按钮重名（排除自身 id）。
async fn validate_quick_button_update(
    db: &crate::db::Database,
    id: i64,
    req: &UpdateQuickButtonRequest,
) -> Result<(), AppError> {
    if let Some(ref name) = req.button_name {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest("按钮名称不能为空".to_string()));
        }
        // 改名冲突判定：同名记录存在且不是自己，才报错
        if let Some(existing) = crate::db::quick_button::get_quick_button_by_name(db, name)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
        {
            if existing.id != id {
                return Err(AppError::BadRequest("按钮名称已存在".to_string()));
            }
        }
    }
    if let Some(ref prompt) = req.prompt_text {
        if prompt.trim().is_empty() {
            return Err(AppError::BadRequest("话术不能为空".to_string()));
        }
    }
    Ok(())
}

/// 删除快捷按钮。
pub async fn delete_quick_button(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    crate::db::quick_button::delete_quick_button(&state.db, id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(serde_json::json!({"success": true})))
}

/// V1 API 路由：为快捷按钮注册 GET/POST/PUT/DELETE 端点，
/// 路径前缀 /api/v1/quick-buttons（全局资源扁平化，不嵌套 workspace）。
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        // 集合操作：列出全部按钮 / 创建新按钮
        .route(
            "/api/v1/quick-buttons",
            get(list_quick_buttons).post(create_quick_button),
        )
        // 单资源操作：按 id 更新 / 删除
        .route(
            "/api/v1/quick-buttons/{id}",
            put(update_quick_button).delete(delete_quick_button),
        )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod quick_button_handler_tests {
    use super::*;
    use crate::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 构造更新请求的辅助：None 表示不改该字段。
    fn update_req(name: Option<&str>, prompt: Option<&str>) -> UpdateQuickButtonRequest {
        UpdateQuickButtonRequest {
            button_name: name.map(str::to_string),
            prompt_text: prompt.map(str::to_string),
        }
    }

    /// validate：改成他人已占用的名称应 BadRequest（重名预检的核心）。
    #[tokio::test]
    async fn test_validate_quick_button_update_rejects_duplicate_name() {
        let db = fresh_db().await;
        let a = crate::db::quick_button::create_quick_button(&db, "A", "a").await.unwrap();
        let _b = crate::db::quick_button::create_quick_button(&db, "B", "b").await.unwrap();
        // 把 A 改名为已被 B 占用的 "B" → 应报错
        let err = validate_quick_button_update(&db, a, &update_req(Some("B"), None)).await;
        assert!(err.is_err(), "改成他人已用名应 BadRequest");
    }

    /// validate：改成自己的原名不算冲突（排除自身的判定）。
    #[tokio::test]
    async fn test_validate_quick_button_update_allows_own_name() {
        let db = fresh_db().await;
        let a = crate::db::quick_button::create_quick_button(&db, "A", "a").await.unwrap();
        let res = validate_quick_button_update(&db, a, &update_req(Some("A"), None)).await;
        assert!(res.is_ok(), "改成自己的原名不应报错");
    }

    /// validate：空白名称应 BadRequest。
    #[tokio::test]
    async fn test_validate_quick_button_update_rejects_empty_name() {
        let db = fresh_db().await;
        let res = validate_quick_button_update(&db, 1, &update_req(Some("   "), None)).await;
        assert!(res.is_err(), "空白名称应 BadRequest");
    }

    /// validate：空白话术应 BadRequest。
    #[tokio::test]
    async fn test_validate_quick_button_update_rejects_empty_prompt() {
        let db = fresh_db().await;
        let res = validate_quick_button_update(&db, 1, &update_req(None, Some("  "))).await;
        assert!(res.is_err(), "空白话术应 BadRequest");
    }
}
