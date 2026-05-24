use axum::{
    Router,
    extract::{FromRequest, Path, Request, State, WebSocketUpgrade},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post, put},
};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;
use axum::extract::DefaultBodyLimit;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::adapters::ExecutorRegistry;
use crate::Assets;
use crate::config::Config;
use crate::db::Database;
use crate::models::{ApiResponse, ParsedLogEntry};
use crate::scheduler::TodoScheduler;
use crate::services::feishu_listener::FeishuListener;
use crate::task_manager::{TaskManager, TaskInfo};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub executor_registry: Arc<ExecutorRegistry>,
    pub tx: broadcast::Sender<ExecEvent>,
    pub scheduler: Arc<TodoScheduler>,
    pub task_manager: Arc<TaskManager>,
    pub config: Arc<tokio::sync::RwLock<Config>>,
    pub feishu_listener: Arc<FeishuListener>,
    pub feishu_push_mutator: broadcast::Sender<crate::services::feishu_push::PushConfigUpdate>,
}

impl AppState {
    /// 根据 id 获取 todo，不存在时返回 NotFound 错误
    pub async fn require_todo(&self, id: i64) -> Result<crate::models::Todo, AppError> {
        self.db.get_todo(id).await?.ok_or(AppError::NotFound)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ExecEvent {
    Started {
        task_id: String,
        todo_id: i64,
        todo_title: String,
        executor: String,
    },
    Output {
        task_id: String,
        entry: ParsedLogEntry,
    },
    Finished {
        task_id: String,
        todo_id: i64,
        todo_title: String,
        executor: String,
        success: bool,
        result: Option<String>,
    },
    /// 同步事件：连接时发送当前实际运行的任务列表
    /// 前端收到此事件后应清空 runningTasks 并用此列表初始化
    Sync {
        tasks: Vec<TaskInfo>,
    },
    TodoProgress {
        task_id: String,
        progress: Vec<crate::models::TodoItem>,
    },
    ExecutionStats {
        task_id: String,
        stats: crate::models::ExecutionStats,
    },
}

#[derive(Debug)]
pub enum AppError {
    NotFound,
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            Self::NotFound => (StatusCode::NOT_FOUND, crate::models::codes::NOT_FOUND, "Not found".to_string()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, crate::models::codes::BAD_REQUEST, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, crate::models::codes::INTERNAL, msg.clone()),
        };
        let body = axum::Json(crate::models::ApiResponse::<()>::err(code, &message));
        (status, body).into_response()
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        match &err {
            sea_orm::DbErr::RecordNotFound(_) => AppError::NotFound,
            _ => AppError::Internal(err.to_string()),
        }
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::BadRequest(s)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl<T: Serialize> IntoResponse for crate::models::ApiResponse<T> {
    fn into_response(self) -> Response {
        axum::Json(self).into_response()
    }
}

/// 自定义 JSON 提取器，将解析错误转换为统一的 ApiResponse 错误格式
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Json::<T>::from_request(req, state).await {
            Ok(axum::extract::Json(value)) => Ok(ApiJson(value)),
            Err(rejection) => Err(AppError::BadRequest(rejection.to_string())),
        }
    }
}

mod todo;
mod tag;
pub(crate) mod execution;
mod scheduler;
pub mod backup;
mod config;
pub mod skills;
pub mod agent_bot;
pub mod executor_config;
mod feishu_history;
mod session;
pub mod project_directory;
pub(crate) mod todo_template;
pub mod custom_template;
pub mod webhook;

// WebSocket handler
pub async fn events_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|mut ws| async move {
        let mut rx = state.tx.subscribe();

        // 连接时发送当前实际运行的任务列表
        // 批量获取执行记录，避免 N+1 查询
        let mut running_tasks = state.task_manager.get_all_task_infos().await;
        let task_ids: Vec<String> = running_tasks.iter().map(|t| t.task_id.clone()).collect();
        let records = state.db.get_execution_records_by_task_ids(&task_ids).await.unwrap_or_default();
        let record_map: std::collections::HashMap<String, _> = records
            .into_iter()
            .filter_map(|r| r.task_id.clone().map(|tid| (tid, r)))
            .collect();
        let record_ids: Vec<i64> = record_map.values().map(|r| r.id).collect();
        let logs_map = state.db.get_all_execution_logs_for_records(&record_ids).await.unwrap_or_default();
        for task in &mut running_tasks {
            if let Some(record) = record_map.get(&task.task_id) {
                let logs = logs_map.get(&record.id).cloned().unwrap_or_default();
                task.logs = serde_json::to_string(&logs).unwrap_or_default();
            }
        }
        let sync_event = ExecEvent::Sync { tasks: running_tasks };
        let sync_json = serde_json::to_string(&sync_event).unwrap_or_default();
        if !sync_json.is_empty() {
            let _ = ws
                .send(axum::extract::ws::Message::Text(sync_json.into()))
                .await;
        }

        while let Ok(event) = rx.recv().await {
            let json = serde_json::to_string(&event).unwrap_or_default();
            if json.is_empty() {
                continue;
            }
            if ws
                .send(axum::extract::ws::Message::Text(json.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

// Static file handler
pub async fn index_handler() -> Result<Html<String>, AppError> {
    let content = Assets::get("index.html")
        .ok_or_else(|| AppError::Internal("index.html not found in embedded assets".to_string()))?;
    Ok(Html(String::from_utf8_lossy(&content.data).to_string()))
}

pub async fn static_handler(Path(path): axum::extract::Path<String>) -> Response {
    let path = path.trim_start_matches('/');
    let full_path = if path.is_empty() {
        "index.html".to_string()
    } else {
        format!("assets/{}", path)
    };

    match Assets::get(&full_path) {
        Some(content) => {
            let mime = if path.ends_with(".js") {
                "application/javascript"
            } else if path.ends_with(".css") {
                "text/css"
            } else if path.ends_with(".html") {
                "text/html"
            } else if path.ends_with(".woff2") {
                "font/woff2"
            } else if path.ends_with(".woff") {
                "font/woff"
            } else if path.ends_with(".ttf") {
                "font/ttf"
            } else if path.ends_with(".eot") {
                "application/vnd.ms-fontobject"
            } else if path.ends_with(".svg") {
                "image/svg+xml"
            } else if path.ends_with(".png") {
                "image/png"
            } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
                "image/jpeg"
            } else if path.ends_with(".ico") {
                "image/x-icon"
            } else if path.ends_with(".json") {
                "application/json"
            } else if path.ends_with(".webp") {
                "image/webp"
            } else {
                "application/octet-stream"
            };
            // Vite hashed assets (e.g. index-AbCd1234.js) get immutable cache
            let cache_control = if path.contains('-')
                && matches!(mime, "application/javascript" | "text/css" | "font/woff2" | "font/woff" | "font/ttf")
            {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            ([
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, cache_control),
            ], content.data.to_vec()).into_response()
        }
        None => match Assets::get("index.html") {
            Some(content) => {
                Html(String::from_utf8_lossy(&content.data).to_string()).into_response()
            }
            None => (StatusCode::NOT_FOUND, "Not found").into_response(),
        },
    }
}

#[derive(serde::Serialize)]
struct VersionResponse {
    version: String,
    git_sha: String,
    git_describe: String,
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, axum::Json(serde_json::json!({"status": "ok"})))
}

async fn version_handler() -> impl IntoResponse {
    let version = option_env!("NTD_VERSION").unwrap_or("unknown");
    let git_sha = option_env!("NTD_GIT_SHA").unwrap_or("unknown");
    let git_describe = option_env!("NTD_VERSION_FULL").unwrap_or("unknown");
    let response = VersionResponse {
        version: version.to_string(),
        git_sha: git_sha.to_string(),
        git_describe: git_describe.to_string(),
    };
    ApiResponse::ok(response)
}

// Build router
pub fn create_app(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    scheduler: Arc<TodoScheduler>,
    task_manager: Arc<TaskManager>,
    config: Arc<tokio::sync::RwLock<Config>>,
) -> Router {
    // Create message debounce service (shared between listener and history fetcher)
    use crate::services::message_debounce::MessageDebounce;
    let debounce = Arc::new(MessageDebounce::new(
        db.clone(),
        executor_registry.clone(),
        tx.clone(),
        task_manager.clone(),
        config.clone(),
    ));

    let feishu_listener = Arc::new(FeishuListener::new(
        db.clone(),
        executor_registry.clone(),
        tx.clone(),
        task_manager.clone(),
        config.clone(),
        debounce.clone(),
    ));

    // Auto-start Feishu listeners for enabled bots
    let fl_clone = feishu_listener.clone();
    let db_clone = db.clone();
    tokio::spawn(async move {
        match db_clone.get_agent_bots().await {
            Ok(bots) => {
                for bot in bots.iter().filter(|b| b.bot_type == "feishu" && b.enabled) {
                    if let Err(e) = fl_clone.start_bot(bot).await {
                        tracing::error!("failed to start feishu bot {}: {e}", bot.id);
                    }
                }
            }
            Err(e) => tracing::error!("failed to load agent bots: {e}"),
        }
    });

    // Create and start Feishu push service before AppState
    use crate::services::feishu_push::FeishuPushService;
    let (push_service, push_mutator) = FeishuPushService::new(db.clone(), feishu_listener.clone());
    push_service.start(tx.subscribe());

    // Start Feishu history fetcher with all required dependencies (before AppState to use moved values)
    use crate::services::feishu_history_fetcher::FeishuHistoryFetcher;
    let fetcher = FeishuHistoryFetcher::new(
        db.clone(),
        executor_registry.clone(),
        tx.clone(),
        task_manager.clone(),
        config.clone(),
        feishu_listener.token_manager.clone(),
        feishu_listener.bot_credentials.clone(),
        debounce.clone(),
    );
    let db_for_fetcher = db.clone();
    tokio::spawn(async move {
        tracing::info!("[feishu-history-fetcher] starting initialization");
        let bots_for_fetcher: Vec<(i64, String, String)> = match db_for_fetcher.get_agent_bots().await {
            Ok(bots) => {
                let filtered: Vec<_> = bots.into_iter()
                    .filter(|b| b.bot_type == "feishu" && b.enabled)
                    .map(|b| (b.id, b.app_id.clone(), b.app_secret.clone()))
                    .collect();
                tracing::info!("[feishu-history-fetcher] found {} feishu bots", filtered.len());
                filtered
            }
            Err(e) => {
                tracing::error!("[feishu-history-fetcher] failed to get agent bots: {}", e);
                Vec::new()
            }
        };
        tracing::info!("[feishu-history-fetcher] starting with {} bots", bots_for_fetcher.len());
        fetcher.start(bots_for_fetcher);
    });

    let state = AppState {
        db,
        executor_registry,
        tx: tx.clone(),
        scheduler,
        task_manager,
        config,
        feishu_listener: feishu_listener.clone(),
        feishu_push_mutator: push_mutator,
    };

    Router::new()
        .route("/", get(index_handler))
        .route("/xyz/todos", get(todo::get_todos).post(todo::create_todo))
        .route("/xyz/todos/{id}/force-status", put(todo::force_update_todo_status))
        .route("/xyz/todos/{id}/tags", put(todo::update_todo_tags))
        .route("/xyz/todos/{id}/summary", get(execution::get_execution_summary))
        .route("/xyz/todos/{id}/scheduler", put(scheduler::update_scheduler))
        .route("/xyz/todos/recent-completed", get(todo::get_recent_completed_todos))
        .route("/xyz/todos/{id}", get(todo::get_todo).put(todo::update_todo).delete(todo::delete_todo))
        .route("/xyz/tags", get(tag::get_tags).post(tag::create_tag))
        .route("/xyz/tags/{id}", delete(tag::delete_tag))
        .route("/xyz/execution-records", get(execution::get_execution_records))
        .route("/xyz/execution-records/running", get(execution::get_running_execution_records_handler))
        .route("/xyz/execution-records/session/{session_id}", get(execution::get_execution_records_by_session))
        .route("/xyz/execution-records/{id}/logs", get(execution::get_execution_logs_handler))
        .route("/xyz/execution-records/{id}", get(execution::get_execution_record))
        .route("/xyz/execution-records/{id}/resume", post(execution::resume_execution_handler))
        .route("/xyz/dashboard-stats", get(execution::get_dashboard_stats))
        .route("/xyz/execute", post(execution::execute_handler))
        .route("/xyz/smart-create", post(execution::smart_create_handler))
        .route("/xyz/execute/stop", post(execution::stop_execution_handler))
        .route("/xyz/execute/force-fail", post(execution::force_fail_execution_handler))
        .route("/xyz/running-todos", get(execution::get_running_todos))
        .route("/xyz/events", get(events_handler))
        .route("/xyz/scheduler/todos", get(scheduler::get_scheduler_todos))
        .route("/xyz/backup/export", get(backup::export_backup))
        .route("/xyz/backup/export-selected", post(backup::export_selected))
        .route("/xyz/backup/import", post(backup::import_backup))
        .route("/xyz/backup/merge", post(backup::merge_backup))
        .route("/xyz/backup/database/download", get(backup::download_database))
        .route("/xyz/backup/database/status", get(backup::get_database_backup_status))
        .route("/xyz/backup/database/trigger", post(backup::trigger_local_backup))
        .route("/xyz/backup/database/auto", put(backup::update_auto_backup))
        .route("/xyz/backup/database/optimize", post(backup::database_optimize))
        .route("/xyz/backup/database/file", get(backup::download_backup_file).delete(backup::delete_backup_file))
        .route("/xyz/backup/todo/status", get(backup::get_todo_backup_status))
        .route("/xyz/backup/todo/trigger", post(backup::trigger_todo_backup))
        .route("/xyz/backup/todo/auto", put(backup::update_todo_auto_backup))
        .route("/xyz/backup/todo/file", get(backup::download_todo_backup_file).delete(backup::delete_todo_backup_file))
        .route("/xyz/backup/log-cleanup/status", get(backup::get_log_cleanup_status))
        .route("/xyz/backup/log-cleanup", put(backup::update_log_cleanup))
        .route("/xyz/backup/log-cleanup/trigger", post(backup::trigger_log_cleanup))
        .route("/xyz/backup/skills/status", get(backup::get_skill_backup_status))
        .route("/xyz/backup/skills/trigger", post(backup::trigger_skill_backup))
        .route("/xyz/backup/skills/auto", put(backup::update_skill_auto_backup))
        .route("/xyz/backup/skills/file", get(backup::download_skill_backup_file).delete(backup::delete_skill_backup_file))
        .route("/xyz/config", get(config::get_config).put(config::update_config))
        .route("/xyz/executors", get(executor_config::list_executors))
        .route("/xyz/executors/{name}", put(executor_config::update_executor))
        .route("/xyz/executors/{name}/detect", post(executor_config::detect_executor))
        .route("/xyz/executors/{name}/test", post(executor_config::test_executor))
        .route("/xyz/executors/detect-all", post(executor_config::detect_all_executors))
        .route("/xyz/skills", get(skills::list_skills))
        .route("/xyz/skills/compare", get(skills::compare_skills))
        .route("/xyz/skills/sync", post(skills::sync_skill))
        .route("/xyz/skills/invocations", get(skills::list_invocations).post(skills::record_invocation))
        .route("/xyz/skills/content", get(skills::get_skill_content))
        .route("/xyz/skills/export", get(skills::export_skill))
        .route("/xyz/skills/import", post(skills::import_skill))
        .route("/xyz/agent-bots", get(agent_bot::list_agent_bots))
        .route("/xyz/agent-bots/feishu/init", post(agent_bot::feishu_init))
        .route("/xyz/agent-bots/feishu/begin", post(agent_bot::feishu_begin))
        .route("/xyz/agent-bots/feishu/poll", post(agent_bot::feishu_poll))
        .route("/xyz/agent-bots/feishu/push", get(agent_bot::get_feishu_push).put(agent_bot::update_feishu_push))
        .route("/xyz/agent-bots/feishu/group-whitelist", get(agent_bot::get_group_whitelist).post(agent_bot::add_group_whitelist))
        .route("/xyz/agent-bots/feishu/group-whitelist/{id}", delete(agent_bot::delete_group_whitelist))
        .route("/xyz/feishu/history-messages", get(feishu_history::get_history_messages))
        .route("/xyz/feishu/message-stats", get(feishu_history::get_message_stats))
        .route("/xyz/feishu/senders", get(feishu_history::get_distinct_senders))
        .route("/xyz/feishu/history-chats", get(feishu_history::get_history_chats).post(feishu_history::create_history_chat))
        .route("/xyz/feishu/history-chats/{id}", delete(feishu_history::delete_history_chat).put(feishu_history::update_history_chat))
        .route("/xyz/agent-bots/{id}", delete(agent_bot::delete_agent_bot))
        .route("/xyz/agent-bots/{id}/config", put(agent_bot::update_agent_bot_config))
        .route("/health", get(health_handler))
        // Webhook trigger endpoints (no /xyz/ prefix, accessible externally)
        .route("/webhook/trigger", get(webhook::trigger_webhook_default).post(webhook::trigger_webhook_default_post_json))
        .route("/webhook/trigger/{todo_id}", get(webhook::trigger_webhook_with_todo).post(webhook::trigger_webhook_with_todo_post_json))
        // Webhook management APIs
        .route("/xyz/webhooks", get(webhook::list_webhooks).post(webhook::create_webhook))
        .route("/xyz/webhooks/{id}", get(webhook::get_webhook).put(webhook::update_webhook).delete(webhook::delete_webhook))
        .route("/xyz/webhook-records", get(webhook::get_webhook_records))
        .route("/xyz/webhook-records/{id}", get(webhook::get_webhook_record))
        .route("/assets/{*path}", get(static_handler))
        .route("/xyz/version", get(version_handler))
        .route("/xyz/sessions", get(session::list_sessions))
        .route("/xyz/sessions/stats", get(session::get_session_stats))
        .route("/xyz/sessions/{id}", get(session::get_session_detail).delete(session::delete_session))
        .merge(project_directory::routes())
        .route("/xyz/todo-templates", get(todo_template::get_templates).post(todo_template::create_template))
        .route("/xyz/todo-templates/{id}", put(todo_template::update_template).delete(todo_template::delete_template))
        .route("/xyz/todo-templates/{id}/copy", post(todo_template::copy_template))
        .route("/xyz/custom-templates/status", get(custom_template::get_custom_template_status))
        .route("/xyz/custom-templates/subscribe", post(custom_template::subscribe_custom_template))
        .route("/xyz/custom-templates/unsubscribe", post(custom_template::unsubscribe_custom_template))
        .route("/xyz/custom-templates/sync", post(custom_template::sync_custom_template))
        .route("/xyz/custom-templates/auto-sync", put(custom_template::update_auto_sync_config))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB
        .layer(CompressionLayer::new())
        .layer(
            if crate::config::Config::is_dev_mode() {
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
            } else {
                CorsLayer::new()
                    .allow_methods(Any)
                    .allow_headers(Any)
                    // Production: same-origin only (no explicit allow_origin)
            },
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
