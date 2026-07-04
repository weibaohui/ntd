//! 黑板（Blackboard）API Handler。
//!
//! 提供以下端点：
//! - `GET /api/workspaces/{workspace_id}/blackboard`：获取当前黑板内容与配置（旧版单文件接口，保留兼容）
//! - `PATCH /api/workspaces/{workspace_id}/blackboard`：更新黑板配置
//! - `GET /api/workspaces/{workspace_id}/blackboard/config`：仅获取黑板配置
//! - `GET /api/workspaces/{workspace_id}/blackboard/pages`：获取所有页面列表（Wiki 化后）
//! - `GET /api/workspaces/{workspace_id}/blackboard/pages/{slug}`：获取单个页面内容（Wiki 化后）

use axum::extract::{Path, State};
use axum::routing::get;
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
    /// Wiki 索引页面维护提示词模板（空字符串表示使用内置默认）
    pub wiki_index_prompt: String,
    /// Wiki 主题页面生成提示词模板（空字符串表示使用内置默认）
    pub wiki_page_prompt: String,
}

/// 页面列表项：用于 GET /pages 接口，只返回摘要信息不返回完整 content。
///
/// 前端目录树用这个数据渲染，避免一次拉取所有页面的大文本。
#[derive(Debug, serde::Serialize)]
pub struct BlackboardPageListItem {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub page_type: String,
    /// 来源记录数量（source_refs 数组长度）
    pub source_count: usize,
    pub updated_at: Option<String>,
}

/// 单页详情：用于 GET /pages/{slug}，返回完整 Markdown 内容。
#[derive(Debug, serde::Serialize)]
pub struct BlackboardPageDetail {
    pub id: i64,
    pub workspace_id: i64,
    pub slug: String,
    pub title: String,
    pub page_type: String,
    pub content: String,
    pub source_refs: Vec<i64>,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
}

/// 更新黑板配置的请求体（所有字段可选，None 保持原值不变）。
#[derive(Debug, serde::Deserialize)]
pub struct UpdateBlackboardConfigRequest {
    pub blackboard_debounce_secs: Option<i64>,
    pub blackboard_debounce_count: Option<i64>,
    pub wiki_index_prompt: Option<String>,
    pub wiki_page_prompt: Option<String>,
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
            wiki_index_prompt: model.wiki_index_prompt,
            wiki_page_prompt: model.wiki_page_prompt,
        })),
        None => Ok(ApiResponse::ok(BlackboardResponse {
            id: 0,
            workspace_id,
            content: String::new(),
            updated_at: None,
            // 无记录时返回默认值；配置会在首次 create_blackboard 时写入
            blackboard_debounce_secs: 600,
            blackboard_debounce_count: 10,
            wiki_index_prompt: String::new(),
            wiki_page_prompt: String::new(),
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
            wiki_index_prompt: String::new(),
            wiki_page_prompt: String::new(),
        })),
    }
}

/// `PATCH /api/workspaces/{workspace_id}/blackboard/config`
///
/// 更新指定工作空间的黑板配置（防抖阈值、提示词）。
/// 若黑板记录不存在，先通过 create_blackboard 幂等创建（保证记录存在），再更新配置。
/// 返回更新后的完整配置（从 DB 重新查询）。
///
/// 当 debounce_secs 变更时，自动重置运行中的计时器，避免旧的计时状态与新的阈值不匹配
/// 导致前端显示负数秒数。
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
        req.wiki_index_prompt,
        req.wiki_page_prompt,
    ).await.map_err(|e| AppError::Internal(format!("更新黑板配置失败: {}", e)))?;

    // debounce_secs 变更时，根据已计时长决定：超则立即触发 flush，未超则继续用新阈值计时
    // 传入钳制后的值，与 DB update_blackboard_config 内部 v.max(10) 保持一致
    if let Some(new_secs) = req.blackboard_debounce_secs {
        let clamped = new_secs.max(10);
        crate::services::blackboard_debouncer::reconcile_timer_after_config_change(
            workspace_id,
            clamped,
        )
        .await;
    }

    let cfg = state.db.get_blackboard_config(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("更新后查询黑板配置失败: {}", e))
    })?;
    Ok(ApiResponse::ok(cfg.unwrap()))
}

/// `GET /api/workspaces/{workspace_id}/blackboard/pages`
///
/// 获取指定工作空间的所有黑板页面列表（含 index/topic/log）。
/// 返回摘要信息（不含完整 content），供前端目录树使用。
/// 按 page_type 分组排序：topic 在前（按 updated_at 倒序），然后 index，然后 log。
pub async fn list_blackboard_pages(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<Vec<BlackboardPageListItem>>, AppError> {
    let pages = state.db.list_blackboard_pages(workspace_id).await.map_err(|e| {
        AppError::Internal(format!("查询黑板页面列表失败: {}", e))
    })?;

    // 将 entity model 转成列表项（计算 source_count）
    let items: Vec<BlackboardPageListItem> = pages
        .into_iter()
        .map(|p| {
            let source_count = serde_json::from_str::<Vec<i64>>(&p.source_refs)
                .unwrap_or_default()
                .len();
            BlackboardPageListItem {
                id: p.id,
                slug: p.slug,
                title: p.title,
                page_type: p.page_type,
                source_count,
                updated_at: p.updated_at,
            }
        })
        .collect();

    Ok(ApiResponse::ok(items))
}

/// `GET /api/workspaces/{workspace_id}/blackboard/pages/{slug}`
///
/// 按 slug 获取单个黑板页面的完整内容。
/// 页面不存在返回 404。
pub async fn get_blackboard_page(
    State(state): State<AppState>,
    Path((workspace_id, slug)): Path<(i64, String)>,
) -> Result<ApiResponse<BlackboardPageDetail>, AppError> {
    let page = state.db.get_blackboard_page(workspace_id, &slug).await.map_err(|e| {
        AppError::Internal(format!("查询黑板页面失败: {}", e))
    })?;

    match page {
        Some(p) => {
            let source_refs: Vec<i64> = serde_json::from_str(&p.source_refs)
                .unwrap_or_default();
            Ok(ApiResponse::ok(BlackboardPageDetail {
                id: p.id,
                workspace_id: p.workspace_id,
                slug: p.slug,
                title: p.title,
                page_type: p.page_type,
                content: p.content,
                source_refs,
                updated_at: p.updated_at,
                created_at: p.created_at,
            }))
        }
        None => Err(AppError::NotFound),
    }
}

/// 返回黑板领域路由。
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
            "/api/workspaces/{workspace_id}/blackboard/pages",
            get(list_blackboard_pages),
        )
        .route(
            "/api/workspaces/{workspace_id}/blackboard/pages/{slug}",
            get(get_blackboard_page),
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
            wiki_index_prompt: "index prompt".to_string(),
            wiki_page_prompt: "page prompt".to_string(),
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
            wiki_index_prompt: String::new(),
            wiki_page_prompt: String::new(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 0);
        assert_eq!(json["workspace_id"], 42);
        assert_eq!(json["content"], "");
        assert!(json["updated_at"].is_null());
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
            wiki_index_prompt: board.wiki_index_prompt,
            wiki_page_prompt: board.wiki_page_prompt,
        });
        let json = serde_json::to_value(&resp).unwrap();
        // ApiResponse 包装格式：{"data": {...}}
        assert!(json["data"].is_object());
        assert_eq!(json["data"]["workspace_id"], ws_id);
        assert_eq!(json["data"]["content"], "# 工作空间进展\n\n- foo");
    }
}
