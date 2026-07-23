//! 黑板（Blackboard）API Handler。
//!
//! 纯文件存储方案：
//! - `GET /api/workspaces/{workspace_id}/wiki/files`：获取文件列表（index/log + topics）
//! - `GET /api/workspaces/{workspace_id}/wiki/files/{slug}`：获取文件内容
//! - `GET /api/workspaces/{workspace_id}/blackboard`：获取配置（保留兼容）
//! - `PATCH /api/workspaces/{workspace_id}/blackboard/config`：更新配置

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::Router;

use crate::db::blackboard::BlackboardConfig;
use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::ApiResponse;
use crate::wiki::{delete_topic, list_topics, read_topic, read_log};

/// 黑板配置响应体（保留兼容，不含内容）
#[derive(Debug, serde::Serialize)]
pub struct BlackboardResponse {
    pub id: i64,
    pub workspace_id: i64,
    pub updated_at: Option<String>,
    /// 黑板更新防抖周期（秒）
    pub blackboard_debounce_secs: i64,
    /// 黑板更新防抖条数阈值
    pub blackboard_debounce_count: i64,
    /// Wiki 维护提示词模板（空字符串表示使用内置默认）
    pub wiki_prompt: String,
    /// Wiki 对话使用的执行器名称（None 表示使用默认值 "claudecode"）
    pub wiki_chat_executor: Option<String>,
    /// Wiki 执行超时（秒），控制 Wiki 任务与 Wiki 对话的最长存活时间
    pub wiki_timeout_secs: i64,
    /// 待处理的 execution_record_id 列表（JSON 数组字符串）
    pub pending_record_ids: String,
    /// 黑板功能总开关
    pub enabled: bool,
}

/// Wiki 文件列表项
#[derive(Debug, serde::Serialize)]
pub struct WikiFileItem {
    pub slug: String,
    pub file_type: String, // "index" / "log" / "topic"
}

/// Wiki 文件内容响应
#[derive(Debug, serde::Serialize)]
pub struct WikiFileContent {
    pub slug: String,
    pub content: String,
}

/// Wiki 文件删除响应
///
/// `deleted=false` 表示删除时文件本就不存在（幂等删除，仍算成功），
/// 前端据此区分「真正删了一篇」与「点了但文件已没了」。
#[derive(Debug, serde::Serialize)]
pub struct WikiFileDeleteResult {
    pub slug: String,
    pub deleted: bool,
}

/// 更新黑板配置的请求体（所有字段可选，None 保持原值不变）。
/// wiki_chat_executor 特殊语义：
///   - 字段缺失 / 为 null → 不修改
///   - 为空字符串 "" → 设为 NULL（使用默认执行器）
///   - 为非空字符串 → 设为指定执行器名
#[derive(Debug, serde::Deserialize)]
pub struct UpdateBlackboardConfigRequest {
    pub blackboard_debounce_secs: Option<i64>,
    pub blackboard_debounce_count: Option<i64>,
    pub wiki_prompt: Option<String>,
    pub wiki_chat_executor: Option<String>,
    /// Wiki 执行超时（秒）。None 不修改，Some(v) 会被后端钳制到合法区间。
    pub wiki_timeout_secs: Option<i64>,
    /// 黑板功能总开关。None 不修改，Some(true/false) 启用/禁用。
    pub enabled: Option<bool>,
}

/// `GET /api/workspaces/{workspace_id}/blackboard`
///
/// 获取指定工作空间的黑板配置（不含内容）。
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
            updated_at: model.updated_at,
            blackboard_debounce_secs: model.blackboard_debounce_secs,
            blackboard_debounce_count: model.blackboard_debounce_count,
            wiki_prompt: model.wiki_prompt,
            wiki_chat_executor: model.wiki_chat_executor,
            wiki_timeout_secs: model.wiki_timeout_secs,
            pending_record_ids: model.pending_record_ids,
            enabled: model.enabled != 0,
        })),
        None => Ok(ApiResponse::ok(BlackboardResponse {
            id: 0,
            workspace_id,
            updated_at: None,
            blackboard_debounce_secs: 600,
            blackboard_debounce_count: 10,
            wiki_prompt: String::new(),
            wiki_chat_executor: None,
            wiki_timeout_secs: crate::db::blackboard::DEFAULT_WIKI_TIMEOUT_SECS,
            pending_record_ids: String::from("[]"),
            enabled: true,
        })),
    }
}

/// `GET /api/workspaces/{workspace_id}/blackboard/config`
///
/// 仅获取指定工作空间的黑板配置（防抖阈值、提示词）。
pub async fn get_blackboard_config(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<BlackboardConfig>, AppError> {
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
            wiki_prompt: String::new(),
            wiki_chat_executor: None,
            wiki_chat_sessions: None,
            wiki_timeout_secs: crate::db::blackboard::DEFAULT_WIKI_TIMEOUT_SECS,
            enabled: true,
        })),
    }
}

/// `PATCH /api/workspaces/{workspace_id}/blackboard/config`
///
/// 更新指定工作空间的黑板配置（防抖阈值、提示词）。
pub async fn update_blackboard_config(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
    ApiJson(req): ApiJson<UpdateBlackboardConfigRequest>,
) -> Result<ApiResponse<BlackboardConfig>, AppError> {
    if let Err(e) = state.db.create_blackboard(workspace_id).await {
        tracing::warn!("update_blackboard_config: create_blackboard 幂等创建失败: {:?}", e);
    }
    state.db.update_blackboard_config(
        workspace_id,
        req.blackboard_debounce_secs,
        req.blackboard_debounce_count,
        req.wiki_prompt.clone(),
        // wiki_chat_executor: Option<String> → Option<Option<String>>
        //   - 字段不存在 → None（不修改）
        //   - 字段为 "" → Some(None)（设为 NULL，用默认执行器）
        //   - 字段为非空 → Some(Some(s))（设为指定执行器）
        req.wiki_chat_executor.map(|s| if s.is_empty() { None } else { Some(s) }),
        // wiki_timeout_secs: None 不修改，Some(v) 由 db 层钳制到合法区间
        req.wiki_timeout_secs,
        // enabled: None 不修改，Some(true/false) 启用/禁用黑板功能
        req.enabled,
    ).await.map_err(|e| AppError::Internal(format!("更新黑板配置失败: {}", e)))?;

    // 配置变更后同步到该 workspace 已存在的 Wiki Todo 的 prompt 字段：
    // - 用户改了提示词 → 立即生效到 todo.prompt，下次执行直接用最新值
    // - 用户清空提示词 → 用内置默认覆盖 todo.prompt
    // 失败仅 warn：配置已保存成功，同步 todo 失败不应阻断配置保存流程
    if let Err(e) = crate::services::blackboard::apply_wiki_prompt_to_todo(
        &state.db,
        workspace_id,
    ).await {
        tracing::warn!(
            "apply_wiki_prompt_to_todo 失败: workspace_id={}, error={:?}",
            workspace_id,
            e
        );
    }

    // enabled 变更为 false 时，取消已调度的防抖 timer，确保黑板彻底停止工作。
    // 已在队列中的 pending_record_ids 保留不清理（用户重新启用后继续处理）；
    // timer 逻辑取消（清除状态 + 标记未运行）后，即使 timer task 自然到期发送 flush 消息，
    // handle_flush_msg 的 enabled 检查也会拦截，不会派生 worker 执行 wiki 维护。
    if let Some(false) = req.enabled {
        crate::services::blackboard_debouncer::cancel_timer(workspace_id).await;
    }

    // debounce_secs 变更时，根据已计时长决定：超则立即触发 flush，未超则继续用新阈值计时
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
    // update 后立即查询，cfg 必定存在——upsert 保证了行的存在
    #[allow(clippy::unwrap_used)]
    Ok(ApiResponse::ok(cfg.unwrap()))
}

/// `GET /api/workspaces/{workspace_id}/wiki/files`
///
/// 获取 wiki 文件列表（不含 index）。
pub async fn list_wiki_files(
    State(_state): State<AppState>,
    Path(workspace_id): Path<i64>,
) -> Result<ApiResponse<Vec<WikiFileItem>>, AppError> {
    let mut items = Vec::new();

    // log.md
    items.push(WikiFileItem {
        slug: "log".to_string(),
        file_type: "log".to_string(),
    });

    // topics/*.md
    let topics = list_topics(workspace_id).map_err(|e| {
        AppError::Internal(format!("列出 topics 失败: {:?}", e))
    })?;

    for slug in topics {
        items.push(WikiFileItem {
            slug,
            file_type: "topic".to_string(),
        });
    }

    Ok(ApiResponse::ok(items))
}

/// `GET /api/workspaces/{workspace_id}/wiki/files/{slug}`
///
/// 获取 wiki 文件内容。
pub async fn get_wiki_file(
    State(_state): State<AppState>,
    Path((workspace_id, slug)): Path<(i64, String)>,
) -> Result<ApiResponse<WikiFileContent>, AppError> {
    let content = if slug == "log" {
        read_log(workspace_id).map_err(|e| {
            AppError::Internal(format!("读取 log 失败: {:?}", e))
        })?
    } else {
        // topic 文件
        read_topic(workspace_id, &slug).map_err(|e| {
            AppError::Internal(format!("读取 topic 失败: {:?}", e))
        })?
    };

    match content {
        Some(c) => Ok(ApiResponse::ok(WikiFileContent { slug, content: c })),
        None => Err(AppError::NotFound),
    }
}

/// `DELETE /api/workspaces/{workspace_id}/wiki/files/{slug}`
///
/// 删除指定 topic 文件。仅限 topic：log 由系统维护（禁止删，避免误清执行日志），
/// index 不在 topics/ 目录下、不可经此接口触及。文件本就不存在时返回 deleted=false（幂等）。
pub async fn delete_wiki_file(
    State(_state): State<AppState>,
    Path((workspace_id, slug)): Path<(i64, String)>,
) -> Result<ApiResponse<WikiFileDeleteResult>, AppError> {
    // log 是系统维护的执行日志页，禁止删除；其余 slug 一律按 topic 处理，
    // 由 delete_topic → topic_file 走 validate_slug 防路径遍历，再定位到 topics/<slug>.md。
    if slug == "log" {
        return Err(AppError::BadRequest("不允许删除执行日志".to_string()));
    }
    let deleted = delete_topic(workspace_id, &slug).map_err(|e| {
        AppError::Internal(format!("删除 topic 失败: {:?}", e))
    })?;
    Ok(ApiResponse::ok(WikiFileDeleteResult { slug, deleted }))
}

/// Wiki 对话请求体
#[derive(Debug, serde::Deserialize)]
pub struct WikiChatRequest {
    /// 用户发送的消息文本
    pub message: String,
    /// 可选：指定执行器；不传则使用黑板配置中的 wiki_chat_executor，再缺省用 "claudecode"
    pub executor: Option<String>,
    /// 可选：指定专家/专家团名称，用于注入专家角色定义到 prompt 前面
    pub expert_name: Option<String>,
}

/// `POST /api/workspaces/{workspace_id}/wiki/chat`
///
/// 用户通过自然语言与 Wiki 交流：后端直接 spawn 执行器在 wiki 目录运行，
/// 执行结果通过 HTTP 响应一次性返回（非流式、不创建 Todo、不持久化记录）。
/// 设计参考：feishu_listener 的「executor 默认响应」模式，把触发源从飞书消息换成 HTTP POST。
pub async fn chat_with_wiki(
    State(state): State<AppState>,
    Path(workspace_id): Path<i64>,
    ApiJson(req): ApiJson<WikiChatRequest>,
) -> Result<ApiResponse<crate::services::blackboard::WikiChatResponse>, AppError> {
    // 消息为空直接返回 400，避免 spawn 无意义的执行器进程
    if req.message.trim().is_empty() {
        return Err(AppError::BadRequest("消息不能为空".to_string()));
    }
    // 确保黑板记录存在（无则创建，保证 wiki_chat_executor 配置可读取）
    if let Err(e) = state.db.create_blackboard(workspace_id).await {
        tracing::warn!("chat_with_wiki: create_blackboard 幂等创建失败: {:?}", e);
    }
    // 调 service 层执行对话，传入专家名称用于上下文注入
    let resp = crate::services::blackboard::chat_with_wiki(
        &state.db,
        &state.executor_registry,
        &state.expert_manager,
        &state.tx,
        workspace_id,
        &req.message,
        req.executor.as_deref(),
        req.expert_name.as_deref(),
    )
    .await?;
    Ok(ApiResponse::ok(resp))
}

/// 黑板 API 路由。
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
            "/api/workspaces/{workspace_id}/wiki/files",
            get(list_wiki_files),
        )
        .route(
            "/api/workspaces/{workspace_id}/wiki/files/{slug}",
            get(get_wiki_file).delete(delete_wiki_file),
        )
        .route(
            "/api/workspaces/{workspace_id}/wiki/chat",
            post(chat_with_wiki),
        )
}

/// V1 黑板 API 路由（相对路径版本）。
///
/// 这些路由使用相对路径，期望被嵌套在 `/api/v1/workspaces/{ws}/blackboard` 下。
/// 所有 handler 复用现有函数签名 —— Path 提取器从嵌套路由的路径参数中获取 workspace_id。
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_blackboard).patch(update_blackboard_config))
        .route("/config", get(get_blackboard_config))
}