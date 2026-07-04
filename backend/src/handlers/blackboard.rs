//! 黑板（Blackboard）API Handler。
//!
//! 提供四个端点：
//! - `GET /api/workspaces/{workspace_id}/blackboard`：获取当前黑板内容与配置
//! - `PATCH /api/workspaces/{workspace_id}/blackboard`：更新黑板配置
//! - `GET /api/workspaces/{workspace_id}/blackboard/config`：仅获取黑板配置
//! - `POST /api/workspaces/{workspace_id}/blackboard/refresh`：手动触发热刷新

use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::Router;

use crate::db::blackboard::BlackboardConfig;
use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::ApiResponse;

/// 黑板响应体（含内容与 per-workspace 配置）
#[derive(Debug, serde::Serialize)]
pub struct BlackboardResponse {
    pub id: i64,
    pub workspace_id: i64,
    pub content: String,
    pub updated_at: Option<String>,
    /// 黑板更新防抖周期（秒）
    pub blackboard_debounce_secs: i64,
    /// 黑板更新防抖条数阈值
    pub blackboard_debounce_count: i64,
    /// 黑板更新提示词模板（空字符串表示使用内置默认）
    pub blackboard_update_prompt: String,
}

/// 更新黑板配置的请求体（所有字段可选，None 保持原值不变）。
#[derive(Debug, serde::Deserialize)]
pub struct UpdateBlackboardConfigRequest {
    pub blackboard_debounce_secs: Option<i64>,
    pub blackboard_debounce_count: Option<i64>,
    pub blackboard_update_prompt: Option<String>,
}

/// `GET /api/workspaces/{workspace_id}/blackboard`
///
/// 获取指定工作空间的当前黑板内容与配置。
/// 如果该工作空间还没有黑板记录，返回空内容与默认配置（content=""）。
pub async fn get_blackboard(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<BlackboardResponse>, AppError> {
    let board = state.db.get_blackboard(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("查询黑板失败: {}", e))
    })?;

    match board {
        Some(model) => Ok(ApiResponse::ok(BlackboardResponse {
            id: model.id,
            workspace_id: model.workspace_id,
            content: model.content,
            updated_at: model.updated_at,
            blackboard_debounce_secs: model.blackboard_debounce_secs,
            blackboard_debounce_count: model.blackboard_debounce_count,
            blackboard_update_prompt: model.blackboard_update_prompt,
        })),
        None => Ok(ApiResponse::ok(BlackboardResponse {
            id: 0,
            workspace_id,
            content: String::new(),
            updated_at: None,
            // 无记录时返回默认值；配置会在首次 create_blackboard 时写入
            blackboard_debounce_secs: 600,
            blackboard_debounce_count: 10,
            blackboard_update_prompt: String::new(),
        })),
    }
}

/// `GET /api/workspaces/{workspace_id}/blackboard/config`
///
/// 仅获取指定工作空间的黑板配置（防抖阈值、提示词）。
/// 若黑板记录不存在，返回默认配置。
pub async fn get_blackboard_config(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<BlackboardConfig>, AppError> {
    // 先确保黑板记录存在（幂等），避免首次访问时 get_blackboard_config 返回 None
    if let Err(e) = state.db.create_blackboard(workspace_id).await {
        tracing::warn!("get_blackboard_config: create_blackboard 幂等创建失败: {:?}", e);
    }
    let cfg = state.db.get_blackboard_config(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("查询黑板配置失败: {}", e))
    })?;
    match cfg {
        Some(c) => Ok(ApiResponse::ok(c)),
        None => Ok(ApiResponse::ok(BlackboardConfig {
            debounce_secs: 600,
            debounce_count: 10,
            update_prompt: String::new(),
        })),
    }
}

/// `PATCH /api/workspaces/{workspace_id}/blackboard/config`
///
/// 更新指定工作空间的黑板配置（防抖阈值、提示词）。
/// 若黑板记录不存在，先创建再更新。
pub async fn update_blackboard_config(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
    ApiJson(req): ApiJson<UpdateBlackboardConfigRequest>,
) -> Result<ApiResponse<BlackboardConfig>, AppError> {
    // 先确保黑板记录已存在，避免在不存在的记录上更新
    if let Err(e) = state.db.create_blackboard(workspace_id).await {
        tracing::warn!("update_blackboard_config: create_blackboard 幂等创建失败: {:?}", e);
    }
    state.db.update_blackboard_config(
        workspace_id,
        req.blackboard_debounce_secs,
        req.blackboard_debounce_count,
        req.blackboard_update_prompt,
    ).await.map_err(|e| AppError::Internal(format!("更新黑板配置失败: {}", e)))?;
    let cfg = state.db.get_blackboard_config(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("更新后查询黑板配置失败: {}", e))
    })?;
    Ok(ApiResponse::ok(cfg.unwrap()))
}

/// 刷新请求体（预留，当前为空）
#[derive(Debug, serde::Deserialize)]
pub struct RefreshRequest {}

/// 刷新响应体
#[derive(Debug, serde::Serialize)]
pub struct RefreshResponse {
    pub success: bool,
    pub message: String,
}

/// `POST /api/workspaces/{workspace_id}/blackboard/refresh`
///
/// 手动触发黑板刷新：重新执行 blackboard update todo，
/// 让 LLM 根据当前黑板内容重新组织生成。
///
/// 这是一个异步操作，请求返回后黑板内容会在几秒到几十秒后更新。
/// 前端需要间隔轮询或等待 WebSocket 推送来获取最新内容。
pub async fn refresh_blackboard(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<RefreshResponse>, AppError> {
    // 异步触发黑板刷新，不阻塞请求
    // tokio::spawn 确保刷新任务在后台运行，即使 HTTP 连接已断开
    let db = state.db.clone();
    let executor_registry = state.executor_registry.clone();
    let tx = state.tx.clone();
    let task_manager = state.task_manager.clone();
    let config = state.config.clone();

    tokio::spawn(async move {
        if let Err(e) = crate::services::blackboard::refresh_blackboard(
            db,
            executor_registry,
            tx,
            task_manager,
            config,
            workspace_id,
        )
        .await
        {
            tracing::warn!("黑板刷新失败: workspace_id={}, error={:?}", workspace_id, e);
        }
    });

    Ok(ApiResponse::ok(RefreshResponse {
        success: true,
        message: "黑板刷新已触发".to_string(),
    }))
}

/// 返回黑板领域路由。
///
/// 路由设计遵循 RESTful 风格，以 workspace 为作用域：
/// - `/api/workspaces/{workspace_id}/blackboard`
pub fn blackboard_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/workspaces/{workspace_id}/blackboard",
            get(get_blackboard).patch(update_blackboard_config),
        )
        .route(
            "/api/workspaces/{workspace_id}/blackboard/config",
            get(get_blackboard_config),
        )
        .route(
            "/api/workspaces/{workspace_id}/blackboard/refresh",
            post(refresh_blackboard),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 验证 BlackboardResponse 在有记录时正确序列化为前端期望的 JSON 结构。
    /// 字段名（snake_case）必须稳定：前端 BlackboardData 接口与之对应。
    /// 缺字段或命名漂移会导致前端拿不到数据。
    #[test]
    fn test_blackboard_response_serialization_with_data() {
        let resp = BlackboardResponse {
            id: 7,
            workspace_id: 3,
            content: "# 工作空间进展".to_string(),
            updated_at: Some("2026-07-03T10:00:00Z".to_string()),
            blackboard_debounce_secs: 600,
            blackboard_debounce_count: 10,
            blackboard_update_prompt: "custom prompt".to_string(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 7);
        assert_eq!(json["workspace_id"], 3);
        assert_eq!(json["content"], "# 工作空间进展");
        assert_eq!(json["updated_at"], "2026-07-03T10:00:00Z");
    }

    /// 验证 BlackboardResponse 在空记录时（id=0, content=""）的序列化为空骨架。
    /// 前端看到 `id=0 + content=""` 判定"工作空间尚无黑板"，渲染空状态。
    #[test]
    fn test_blackboard_response_serialization_empty() {
        let resp = BlackboardResponse {
            id: 0,
            workspace_id: 42,
            content: String::new(),
            updated_at: None,
            blackboard_debounce_secs: 600,
            blackboard_debounce_count: 10,
            blackboard_update_prompt: String::new(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 0);
        assert_eq!(json["workspace_id"], 42);
        assert_eq!(json["content"], "");
        assert!(json["updated_at"].is_null());
    }

    /// 验证 RefreshRequest 接受空 body。
    /// 刷新端点不要求 body 参数，但 axum 默认要求 Content-Length 与 Content-Type 头；
    /// 序列化反序列化的空结构体对应 `{}` 或空 body。
    #[test]
    fn test_refresh_request_accepts_empty_object() {
        let req: RefreshRequest = serde_json::from_str("{}").unwrap();
        // 反序列化无字段：结构体为空，只要类型正确即可
        let _ = req;
    }

    /// 验证 RefreshResponse 序列化字段稳定。
    /// 字段名必须与前端 `RefreshResponse` 接口一致。
    #[test]
    fn test_refresh_response_serialization() {
        let resp = RefreshResponse {
            success: true,
            message: "黑板刷新已触发".to_string(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["message"], "黑板刷新已触发");
    }

    /// 集成测试：通过 DB 写入后用 ApiResponse 包装，验证完整 JSON 包络。
    /// 这条用例模拟"GET 端点返 ApiResponse 包装"的数据结构，
    /// 防止 ApiResponse 的 data 字段命名（snake_case）漂移导致前端解析失败。
    #[tokio::test]
    async fn test_get_blackboard_returns_wrapped_response() {
        // 准备数据：建 workspace + 写黑板
        let db = Database::new(":memory:").await.unwrap();
        let ws_id = db
            .create_project_directory("/tmp/test-blackboard-handler", None, false, false)
            .await
            .unwrap();
        db.upsert_blackboard_content(ws_id, "# 工作空间进展\n\n- foo")
            .await
            .unwrap();
        // 直接调 DB 层（绕过 handler 的 State<AppState>，因为它依赖完整 AppState 装配）
        let board = db.get_blackboard(ws_id).await.unwrap().unwrap();
        // 模拟 handler 行为：构造 BlackboardResponse 并包到 ApiResponse
        let resp = ApiResponse::ok(BlackboardResponse {
            id: board.id,
            workspace_id: board.workspace_id,
            content: board.content,
            updated_at: board.updated_at,
            blackboard_debounce_secs: board.blackboard_debounce_secs,
            blackboard_debounce_count: board.blackboard_debounce_count,
            blackboard_update_prompt: board.blackboard_update_prompt,
        });
        let json = serde_json::to_value(&resp).unwrap();
        // ApiResponse 包装格式：{"data": {...}}
        assert!(json["data"].is_object());
        assert_eq!(json["data"]["workspace_id"], ws_id);
        assert_eq!(json["data"]["content"], "# 工作空间进展\n\n- foo");
    }
}
