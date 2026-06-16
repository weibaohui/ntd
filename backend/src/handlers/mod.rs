use axum::{
    Router,
    extract::{FromRequest, Path, Request, State, WebSocketUpgrade},
    http::{self, Method, StatusCode, header},
    middleware::Next,
    response::{Html, IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;
use axum::extract::DefaultBodyLimit;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::service_context::ServiceContext;
use crate::adapters::ExecutorRegistry;
use crate::Assets;
use crate::config::Config;
use crate::db::Database;
use crate::hooks::HookService;
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
    /// In-memory copy of the persisted Config. Wrapped in `std::sync::RwLock`
    /// (not `tokio::sync::RwLock`) because the read path is on the hot
    /// request loop while writes are rare (only `PUT /api/config` /
    /// cloud-sync updates). Callers must drop the guard before any `.await`
    /// to keep the future `Send`. See `service_context::ServiceContext::config`
    /// for the same rationale.
    pub config: Arc<std::sync::RwLock<Config>>,
    pub feishu_listener: Arc<FeishuListener>,
    pub feishu_push_mutator: broadcast::Sender<crate::services::feishu_push::PushConfigUpdate>,
    pub hook_service: Arc<HookService>,
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
        /// Feishu bot_id to use for sending result directly to binding chat
        feishu_bot_id: Option<i64>,
        /// Feishu receive_id (user open_id for p2p, chat_id for group)
        feishu_receive_id: Option<String>,
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
    ReviewStatusChanged {
        record_id: i64,
        todo_id: i64,
        review_status: String,
    },
}

/// HTTP handler 统一错误类型。
///
/// 公开枚举保持稳定（issue #613 要求：仅重构实现方式，不改公开 API）。
/// 3 个变体对应 3 个语义层级：
/// - `NotFound`：资源缺失
/// - `BadRequest`：用户输入错误（caller 可修复）
/// - `Internal`：服务器侧故障（caller 无法直接修）
///
/// ## SchedulerError 分类规则（issue #499）
/// `From<SchedulerError>` 实现的语义：
/// - 用户输入错误（`InvalidCron` / `InvalidTimezone`）→ 400 BadRequest，
///   因为这些是 caller 可以修复的（换个合法 cron、换个合法时区）。
/// - 其它（数据库失败、scheduler 后端失败、内部错误）→ 500 Internal。
///
/// impl 放在 `handlers/mod.rs` 而非 `scheduler.rs`，是为了避免
/// `scheduler -> handlers` 的反向依赖：`scheduler` 不需要知道 `AppError` 的存在。
#[derive(Debug)]
pub enum AppError {
    NotFound,
    BadRequest(String),
    Internal(String),
}

impl AppError {
    /// 把错误拆成 HTTP 响应三件套：(status, code, message)。
    ///
    /// 抽离的目的是把 `IntoResponse::into_response` 收成 3 行骨架，
    /// 让 status/code/message 的映射集中在一处，便于测试与扩展新变体。
    fn error_response_parts(&self) -> (StatusCode, i32, String) {
        match self {
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                crate::models::codes::NOT_FOUND,
                "Not found".to_string(),
            ),
            Self::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                crate::models::codes::BAD_REQUEST,
                msg.clone(),
            ),
            Self::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::models::codes::INTERNAL,
                msg.clone(),
            ),
        }
    }

    /// 从 `sea_orm::DbErr` 构造：`RecordNotFound` 视为 404，其余归 500。
    #[allow(clippy::needless_pass_by_value)] // 工厂方法签名：与 From impl 对齐
    fn from_db_err(err: sea_orm::DbErr) -> Self {
        match &err {
            sea_orm::DbErr::RecordNotFound(_) => Self::NotFound,
            _ => Self::Internal(err.to_string()),
        }
    }

    /// 从 `std::io::Error` 构造：统一归 500。
    #[allow(clippy::needless_pass_by_value)] // 工厂方法签名：与 From impl 对齐
    fn from_io_err(err: std::io::Error) -> Self {
        Self::Internal(err.to_string())
    }

    /// 从 `SchedulerError` 构造：用户输入错误→400，其它→500。
    /// 分类细节见模块顶部 doc comment。
    #[allow(clippy::needless_pass_by_value)] // 工厂方法签名：与 From impl 对齐
    fn from_scheduler_error(err: crate::scheduler::SchedulerError) -> Self {
        match &err {
            crate::scheduler::SchedulerError::InvalidCron { .. }
            | crate::scheduler::SchedulerError::InvalidTimezone(_) => {
                Self::BadRequest(err.to_string())
            }
            crate::scheduler::SchedulerError::Database(_)
            | crate::scheduler::SchedulerError::SchedulerBackend(_)
            | crate::scheduler::SchedulerError::Internal(_) => {
                Self::Internal(err.to_string())
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // 整个 body 只剩"三件套 → ApiResponse → Response"的机械拼装，
        // 所有 status/code/message 决策都已收敛在 `error_response_parts`。
        let (status, code, message) = self.error_response_parts();
        let body = axum::Json(crate::models::ApiResponse::<()>::err(code, &message));
        (status, body).into_response()
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        Self::from_db_err(err)
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        // 裸 String 转 BadRequest 是项目内的约定（多为校验失败信息），
        // 保留直接构造以避免在调用点散落额外的工厂调用。
        Self::BadRequest(s)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::from_io_err(err)
    }
}

impl From<crate::scheduler::SchedulerError> for AppError {
    fn from(err: crate::scheduler::SchedulerError) -> Self {
        Self::from_scheduler_error(err)
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
mod feishu_binding;
mod feishu_history;
mod session;
pub mod project_directory;
pub(crate) mod todo_template;
pub mod custom_template;
pub mod webhook;
pub mod usage_stats;
pub mod sync;
pub mod sub_states;

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

        // 循环从 broadcast channel 读取事件并推到 WebSocket。
        //
        // 注意 `rx.recv()` 在 channel 容量耗尽时返回 `RecvError::Lagged(n)`:
        // ring buffer 已被覆盖,n 条旧事件丢失(包括可能错过的 Finished 等
        // channel 当前 head,自然跳过被覆盖的积压。虽然 receiver 在 Lagged 后
        // 本身可以继续收新事件（tokio broadcast 语义），但 resubscribe 能从最新
        // head 开始，完全跳过积压，避免旧日志涌入前端。
        // 不会断开连接。如果不处理 Lagged,原 `while let Ok(...)` 会立刻
        // 退出 → WS 断开 → 前端误判任务仍在执行。
        //
        // 对比 `services/feishu_push.rs:91-93`（warn-only，不重新订阅）；
        // 这里额外 resubscribe 以跳过 lag 期间的积压事件。
        loop {
            match rx.recv().await {
                Ok(event) => {
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
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        "[ws-events] client lagged, skipped {} events; resubscribing to skip backlog",
                        n
                    );
                    rx = state.tx.subscribe();
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("[ws-events] broadcast channel closed, closing WebSocket");
                    break;
                }
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
            // 提取纯函数推断逻辑，方便在不依赖 `Assets` 嵌入资源的前提下做单元测试。
            let mime_str = guess_mime(path);
            let cache_control = cache_control_for(path, mime_str);
            // mime_guess 返回的是 `&'static str`（编译期常量），但仍校验其合法性，
            // 防止未来 mime_guess 引入非 ASCII 字符等导致 `HeaderValue` 构造失败。
            let mime_value = match header::HeaderValue::from_str(mime_str) {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!(
                        "invalid mime derived for {}: {}; fallback to octet-stream",
                        path,
                        mime_str
                    );
                    header::HeaderValue::from_static("application/octet-stream")
                }
            };
            let cache_value = header::HeaderValue::from_static(cache_control);
            ([
                (header::CONTENT_TYPE, mime_value),
                (header::CACHE_CONTROL, cache_value),
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

/// 根据文件路径推断 MIME 类型。
///
/// 之前用 if-else 链手写判断，缺点是：
/// 1. 维护成本高，新增类型必须改代码；
/// 2. 覆盖不全（缺 .wasm/.mjs/.map/.mp4/.webm 等）；
/// 3. 大小写敏感（`.JS` 会落到 octet-stream）。
///
/// 这里使用 `mime_guess` crate 推断。`from_path` 内部对扩展名做小写化处理，
/// 解决了大小写问题；库本身覆盖范围广（含 wasm/mjs/map/mp4/webm 等）。
/// 匹配不到时降级为 `application/octet-stream`。
///
/// 返回 `&'static str` 直接借用 mime_guess 内部维护的字符串常量，避免分配。
fn guess_mime(path: &str) -> &'static str {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
}

/// 根据路径与 MIME 返回合适的 `Cache-Control` 头。
///
/// Vite 构建产物对带 hash 的资源（形如 `index-AbCd1234.js`）使用不可变长缓存：
/// 内容变更时文件名改变，URL 变化会绕过浏览器缓存。
/// 早期实现 `path.contains('-')` 过于宽松：`foo-bar.js` 这种巧合命名也会被缓存 1 年。
/// 这里要求文件名最后一段 `-` 之后存在 6 位及以上的字母数字 hash，
/// 且扩展名属于可哈希资源类型，才下发 `immutable`，否则保守地使用 `no-cache`。
fn cache_control_for(path: &str, mime: &str) -> &'static str {
    if is_vite_hashed_asset(path) && is_cacheable_mime(mime) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

/// 是否是 Vite 风格的带 hash 资源名（`<name>-<hash>.<ext>`）。
///
/// 仅做启发式判断：要求 `base-hash.ext` 中 hash 段为 6 位及以上 ASCII 字母数字。
/// 形如 `index-3aB12cD4.js` 会被认为带 hash；`foo-bar.js`（hash 仅 3 位）会落空。
fn is_vite_hashed_asset(path: &str) -> bool {
    // 拆分扩展名；无扩展名则直接判定为否
    let Some((base, ext)) = path.rsplit_once('.') else {
        return false;
    };
    // 仅考虑 Vite 实际会 hash 的资源类型，避免对任意二进制/JSON 等下发 immutable
    if !is_vite_hashed_extension(ext) {
        return false;
    }
    // 取最后一个 `-` 后的段作为 hash
    let Some((_name, hash)) = base.rsplit_once('-') else {
        return false;
    };
    // 至少 6 位且全部为 ASCII 字母数字，避免巧合命名误判
    hash.len() >= 6 && hash.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Vite 在生产构建中会带 hash 的扩展名集合。
/// 维护一个明确列表，而不是“任何带 `.` 的文件”，减少误判空间。
fn is_vite_hashed_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "js" | "mjs" | "css" | "woff" | "woff2" | "ttf" | "eot" | "svg" | "png" | "jpg"
            | "jpeg" | "gif" | "webp" | "ico" | "json" | "map" | "wasm"
    )
}

/// 该 MIME 是否适合下发 immutable 长缓存。
///
/// 仅放行 JS/CSS/字体类；其他 MIME（图片/JSON 等）虽然 Vite 也会 hash，
/// 但作为业务可独立更新的资源，下发 `immutable` 需要更严格的资产 manifest 配合，
/// 暂保持 no-cache，避免缓存策略与运维预期不一致。
///
/// MIME 值以 `mime_guess` 实际输出为准（见 `guess_mime` 测试）。
/// 同时保留 `application/javascript` / `font/woff` 兼容旧链路/外部调用方。
fn is_cacheable_mime(mime: &str) -> bool {
    matches!(
        mime,
        "text/javascript"
            | "application/javascript"
            | "text/css"
            | "font/woff2"
            | "font/woff"
            | "application/font-woff"
            | "font/ttf"
            | "application/vnd.ms-fontobject"
    )
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

/// 查询 npm 最新版本号，用于前端版本检查提示。
/// 调用 `npm view @weibaohui/nothing-todo version` 获取远程最新版本。
async fn version_latest_handler() -> impl IntoResponse {
    let output = std::process::Command::new("npm")
        .args(["view", "@weibaohui/nothing-todo", "version"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            // npm view 输出格式为 "x.y.z\n"，需要 trim 掉换行符
            let latest = String::from_utf8_lossy(&out.stdout).trim().to_string();
            ApiResponse::ok(serde_json::json!({ "latest": latest }))
        }
        Ok(out) => {
            let err_msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            tracing::warn!("npm view failed: {}", err_msg);
            ApiResponse::ok(serde_json::json!({ "latest": null, "error": err_msg }))
        }
        Err(e) => {
            tracing::warn!("Failed to run npm view: {}", e);
            ApiResponse::ok(serde_json::json!({ "latest": null, "error": e.to_string() }))
        }
    }
}

/// 给每个 HTTP 请求注入 `X-Request-Id` 并开启一个携带 request_id / method / uri 的 tracing span。
///
/// 目的：issue #513 要求的「trace_id 关联」。
/// - 入站请求如果已经带了 `X-Request-Id` 头，沿用（让上游网关/前端可以打通链路）；
/// - 否则生成一个 UUIDv4 写到 extensions，再回写到响应头，方便客户端日志和服务端对账。
/// - 这里用 `tracing::debug!` 直接附带结构化字段写入当前活跃 span（TraceLayer 默认 span
///   没有声明可记录字段，所以 `Span::current().record(...)` 在默认 span 上会静默失败，
///   这里直接用 debug! 输出，确保 request_id 一定进日志）。
///
/// 注意：本中间件必须**先于** TraceLayer 注册（axum layer 栈是「后注册先生效」，
/// 所以要在 .layer(TraceLayer) 之前 .layer(from_fn(propagate_request_id))）。
pub async fn propagate_request_id(mut req: Request, next: Next) -> Response {
    // 读取或生成 request_id：保留外部传入以便和上游打通；否则给一个 UUIDv4。
    let request_id = resolve_request_id(req.headers());

    // 写入 request extensions，方便 handler 通过 Extension<RequestId> 显式取出，
    // 也方便后面的中间件/TraceLayer 进一步关联。
    req.extensions_mut().insert(RequestId(request_id.clone()));

    // 用 debug 级别：production 默认 RUST_LOG=info 时这条日志不会刷屏；
    // 排查链路时设 RUST_LOG=ntd::handlers=debug 即可激活，且本中间件作用于全部
    // HTTP 路由（含静态资源），用 info 会让每个静态资源请求都写一条日志。
    tracing::debug!(
        request_id = %request_id,
        method = %req.method(),
        uri = %req.uri(),
        "http request received"
    );

    let mut response = next.run(req).await;

    // 把同一个 request_id 写回响应头，客户端可以贴到工单/日志里。
    if let Ok(value) = header::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }

    response
}

/// 纯函数：解析或生成 request_id。
///
/// 抽出独立的纯函数是为 `request_id_tests` 单元测试：HTTP 头里若已携带 `X-Request-Id`
/// 则沿用上游值（保留跨服务 trace 关联），否则生成 UUIDv4。
/// 之所以不在测试里直接构造 `Next`：axum 0.8 的 `Next` 没有公开构造函数，
/// 抽离成纯函数是更轻量的验证路径。
pub fn resolve_request_id(headers: &http::HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// CORS expose_headers 列表 —— 让浏览器侧 JS 能读到这些响应头。
///
/// `x-request-id` 由 `propagate_request_id` 中间件写回，前端日志需要贴到工单/对账上游网关。
/// dev 与 prod 共享同一份列表（避免单边漂移），dev 模式下允许 `Any` origin，
/// 所以同时 expose 也不会扩大攻击面（攻击者已经从 origin 通配中拿到了能力）。
fn cors_expose_headers() -> [http::HeaderName; 1] {
    [http::HeaderName::from_static("x-request-id")]
}

/// Extractor：handler 内部如需在响应体里附带 request_id，可用 `Extension<RequestId>` 取出。
#[derive(Debug, Clone)]
pub struct RequestId(pub String);



/// 返回 ntd.update 标记文件的路径（Unix 版）。
fn ntd_update_marker_path() -> String {
    "/tmp/ntd.update".to_string()
}

/// 返回子进程清理标记文件时使用的路径表达式。
/// Unix 用 `/tmp/ntd.update`，Windows 用 `%TEMP%\\ntd.update`，
/// 与 `ntd_update_marker_path()` 写入的路径保持一致。
fn ntd_update_marker_cleanup_path() -> String {
    #[cfg(unix)]
    {
        "/tmp/ntd.update".to_string()
    }
    #[cfg(windows)]
    {
        "%TEMP%\\ntd.update".to_string()
    }
}

/// sh -c 回退方案：在非 Linux 平台或 systemd-run 不可用时使用。
///
/// 使用 `(...) &` 语法让子进程在后台运行并脱离当前 shell 的 wait 链，
/// 主进程 exit(0) 后子进程不会收到 SIGHUP，会被 reparent 到 init 进程。
/// 输出重定向到日志文件方便排查。
///
/// # 参数
/// * `ntd_cmd` - ntd 可执行文件路径
/// * `marker_cleanup_path` - 标记文件清理路径
/// * `log_path` - 子进程 stdout/stderr 重定向的目标日志路径
#[cfg(not(windows))]
fn spawn_redeploy_sh_fallback(ntd_cmd: &str, marker_cleanup_path: &str, log_path: &str) {
    // 用单引号包裹 ntd 路径。调用方已通过 is_safe_ntd_path 校验，
    // 没有单引号/反斜杠等危险字符。单引号在 bash 里禁用所有 expansion。
    let quoted = crate::daemon::common::shell_quote_single(ntd_cmd);

    // stdin 设为 null 防止子进程意外读取父进程 stdin。
    // stdout/stderr 在 shell 命令内部已通过 `>> {log_path} 2>&1`
    // 重定向到日志文件，此处 .stdout/.stderr 设置被 shell 内部重定向覆盖。
    //
    // 使用 `;` 而非 `&&`：即使某步失败也继续后续步骤，保证标记被清理。
    std::process::Command::new("sh")
        .args(["-c", &format!(
            "(sleep 3; {quoted} daemon install --force; {quoted} daemon start; rm -f {marker}) >> {log} 2>&1 &",
            quoted = quoted,
            marker = marker_cleanup_path,
            log = log_path,
        )])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok();
}

/// 执行 npm 升级并采用分离式自更新方案重新部署 daemon 服务。
///
/// # 核心问题
/// `ntd daemon stop` 会杀掉当前 daemon 进程（即本 handler 所在进程），
/// 后续的 `install --force` 和 `start` 无法在当前进程中继续执行。
///
/// # 分离式自更新方案 (issue #569)
/// 1. 执行 `npm install -g @weibaohui/nothing-todo@latest` 升级 npm 包
/// 2. 写标记文件（Unix: `/tmp/ntd.update`，Windows: `%TEMP%\\ntd.update`），
///    用于 systemd 检测到退出时不自动重启旧版本
/// 3. fork 独立子进程：`sh -c "(sleep 3; ntd daemon install --force; ntd daemon start; rm -f /tmp/ntd.update) &"`
///    - sleep 3：等主进程完全退出，释放端口
///    - install --force：用新 binary 重新安装服务配置
///    - start：启动新版本服务
///    - rm -f：清理标记
/// 4. 先返回 HTTP 响应给前端，再 spawn 后台任务延迟 500ms 后 `exit(0)`，
///    让出端口和资源给新版本。响应先返回确保前端看到成功响应，
///    避免 exit(0) 时连接被突然中断。
///
/// # 前端配合
/// 前端首先收到 HTTP 成功响应（code=0），然后 5s 后自动 `location.reload()`
/// 刷新页面以访问新版本服务。即使后端 exit(0) 导致 TCP 断开，
/// 5s 定时器也会兜底刷新。
///
/// # 安全防线 (issue #476 CRITICAL #1)
/// `npm prefix -g` 返回的 prefix 来自用户 `~/.npmrc`，可被污染成
/// `/foo;rm -rf /;` 之类的攻击载荷。安全校验链路：
/// 1. `is_safe_ntd_path` 白名单校验：仅允许 `[A-Za-z0-9/_.-]`，且必须是绝对路径
/// 2. `shell_quote_single` 单引号包裹：禁止所有 shell expansion
async fn version_upgrade_handler() -> impl IntoResponse {
    // 检测 npm 全局目录写权限，获取安全的安装 prefix
    let prefix = crate::npm_utils::get_npm_global_prefix();

    // 执行 npm 升级，捕获输出以便返回给前端展示
    let npm_result = std::process::Command::new("npm")
        .args([
            "install",
            "-g",
            // 指定 prefix 确保安装到有写权限的目录
            &format!("--prefix={}", prefix),
            "@weibaohui/nothing-todo@latest",
        ])
        .output();

    match &npm_result {
        Ok(out) => {
            tracing::info!(
                "npm upgrade stdout: {}, stderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Err(e) => {
            tracing::error!("Failed to run npm: {}", e);
            // 显式指定类型参数，因为 err 返回 ApiResponse<T>（T 无法从 err 签名推断）。
            // 这里用 serde_json::Value 作为 T，与 handler 的返回类型 impl IntoResponse 兼容。
            let err_resp: ApiResponse<serde_json::Value> = ApiResponse::err(1, &format!("npm upgrade failed: {}", e));
            return err_resp;
        }
    }

    // npm 升级失败时返回错误信息
    if let Ok(out) = &npm_result {
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let err_msg = if stderr.is_empty() {
                "npm upgrade failed".to_string()
            } else {
                format!("npm upgrade failed: {}", stderr.trim())
            };
            // 显式指定类型参数，与 handler 返回类型保持一致
        let err_resp: ApiResponse<serde_json::Value> = ApiResponse::err(1, &err_msg);
        return err_resp;
        }
    }

    // npm 升级成功，查找新安装的 ntd 可执行文件路径
    let ntd_cmd = crate::npm_utils::find_ntd_binary(&prefix);

    // **安全校验**：检查 ntd 路径是否可安全嵌入 shell 脚本，
    // 防止 prefix 被 ~/.npmrc 污染成攻击载荷 (issue #476)
    // 注意：find_ntd_binary 最终 fallback 返回裸字符串 "ntd"（依赖 PATH 查找），
    // 它会被 is_safe_ntd_path 拒绝（不是绝对路径），此时给出明确的错误提示。
    if ntd_cmd == "ntd" {
        tracing::error!(
            "Self-update: ntd binary not found at {{prefix}}/bin/ntd or current exe path"
        );
        let err_resp: ApiResponse<serde_json::Value> = ApiResponse::err(1, "无法更新：未找到 ntd 可执行文件路径");
        return err_resp;
    }
    if !crate::daemon::common::is_safe_ntd_path(&ntd_cmd) {
        tracing::error!(
            "Refusing self-update: ntd path {:?} contains characters outside [A-Za-z0-9/_.-] \
             or is not absolute. Likely a poisoned npm prefix.",
            ntd_cmd,
        );
        let err_resp: ApiResponse<serde_json::Value> = ApiResponse::err(1, "无法更新：ntd 路径包含非法字符（可能 npm prefix 被污染）");
        return err_resp;
    }

    // 写标记文件，用于 Restart=always 场景：
    // 主进程 exit(0) 后 systemd 检测到标记存在则不自动重启旧版本，
    // 等待子进程完成 install --force + start 后清理标记。
    // 更新失败排查：标记残留 = 流程中断，可定位卡在哪一步。
    // Unix 用 /tmp/ntd.update, Windows 用 %TEMP%\\ntd.update，
    // 与子进程里的清理路径保持一致。
    std::fs::write(ntd_update_marker_path(), "").ok();

    // 子进程清理标记文件时的路径，与 ntd_update_marker_path() 保持一致。
    let marker_cleanup_path = ntd_update_marker_cleanup_path();

    // 分离式自更新方案 (issue #569)：fork 独立子进程在后台完成 install + start。
    //
    // 不同平台采用不同策略：
    // - Linux（systemd）：使用 systemd-run --scope --no-block 把脚本放在独立 cgroup，
    //   即使主进程 exit(0) 后 systemd 清理 ntd.service 的 cgroup 也不会杀掉子进程。
    // - macOS / 其他 Unix：使用 sh -c "(...) &" 语法让子进程脱离 shell wait 链，
    //   主进程 exit 后子进程被 reparent 到 init 进程。
    // - Windows：使用 cmd /C + CREATE_NO_WINDOW 隐藏黑窗。
    #[cfg(target_os = "linux")]
    {
        // Linux 上使用 systemd-run --scope --no-block，
        // 将 redeploy 脚本放入独立 cgroup，防止被 systemd 的 KillMode=mixed 牵连。
        // 详见 backend/src/daemon/redeploy.rs 模块注释。
        //
        // 步骤：
        // 1. sleep 3s：等主进程完全退出 + OS 释放 socket fd，避免端口冲突。
        // 2. install --force：用新 binary 重新注册服务
        // 3. start：启动新版本服务
        // 4. rm -f：清理更新标记
        let script = format!(
            "sleep 3; {} daemon install --force; {} daemon start; rm -f {}",
            ntd_cmd,
            ntd_cmd,
            marker_cleanup_path,
        );
        match crate::daemon::spawn_detached_redeploy_nonblocking(&script) {
            Ok(()) => {
                tracing::info!(
                    "Self-update (Linux): systemd-run redeploy spawned. ntd path: {}",
                    ntd_cmd,
                );
            }
            Err(e) => {
                // systemd-run 启动失败（可能是 Docker / WSL 没有 systemd），降级到 sh -c。
                // 这样在无 systemd 的容器环境也能工作。
                tracing::warn!(
                    "Self-update: systemd-run failed ({}), falling back to sh -c",
                    e,
                );
                // systemd-run 失败时 fallback 到 sh -c，日志使用 redeploy_log_path 统一路径
                let fallback_log = crate::daemon::redeploy_log_path().to_string_lossy().to_string();
                spawn_redeploy_sh_fallback(&ntd_cmd, &marker_cleanup_path, &fallback_log);
            }
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        // macOS 或其他 Unix 使用 sh -c 方案，因为 launchd 不会按 cgroup 杀进程。
        spawn_redeploy_sh_fallback(&ntd_cmd, &marker_cleanup_path, "/tmp/ntd-upgrade.log");
    }

    // Windows 用 cmd /C 加 CREATE_NO_WINDOW 隐藏黑窗
    #[cfg(windows)]
    {
        let quoted = crate::daemon::common::shell_quote_single(&ntd_cmd);
        std::process::Command::new("cmd")
            .args(["/C", &format!(
                "timeout /t 3 /nobreak >nul && {quoted} daemon install --force && {quoted} daemon start && del /f /q {marker}",
                quoted = quoted,
                marker = marker_cleanup_path,
            )])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .ok();
    }

    // 记录 fork 成功，便于运维查日志确认升级进程已启动
    tracing::info!(
        "Self-update: npm upgraded, forked child process. ntd path: {}",
        ntd_cmd,
    );

    // 先返回成功响应给前端，让 Axum 完成 HTTP 响应的发送。
    // 然后通过 spawn 后台任务延迟 500ms 后 exit(0)，
    // 给当前 in-flight 响应足够时间写回客户端。
    // 子进程已通过 systemd-run --scope（Linux）或 `(...) &`（其他 Unix）
    // 脱离当前进程树，主进程 exit 后子进程正常工作。
    //
    // 异步 handler 返回响应与 spawn 后台任务：
    // Axum 在 handler 返回后完成响应 body 的写入和 flush，
    // 500ms 窗口足够绝大多数情况。若极端情况下响应未发完，
    // exit(0) 会导致 TCP RST，但前端已有 5s 自动刷新兜底。
    //
    // 为什么不用 tokio::signal / with_graceful_shutdown：
    // 当前 main.rs 使用 axum::serve(listener, app).await 无 graceful
    // shutdown 信号通路。引入完整 shutdown 架构超出本 PR 范围，
    // 短期用 spawn + exit(0) 平衡正确性与改动量。
    let response = ApiResponse::ok(serde_json::json!({
        "status": "upgrade_started",
        "message": "升级流程已启动，服务即将重启",
    }));
    tokio::spawn(async move {
        // 给当前 HTTP 响应足够时间完成发送
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        // 记录日志后退出，便于运维确认升级流程正常
        tracing::info!("Self-update: main process exiting after response sent");
        std::process::exit(0);
    });
    return response;
}

// Build router
pub fn create_app(
    ctx: ServiceContext,
    scheduler: Arc<TodoScheduler>,
    // 由 main.rs 构造好透传进来的 hook_service 单例。create_app 不再自行 Arc::new，
    // 避免出现两份 HookService 各自管自己内部状态、彼此观察不到对方 (见 issue #509)。
    hook_service: Arc<HookService>,
) -> Router {
    let db = ctx.db.clone();
    let executor_registry = ctx.executor_registry.clone();
    let tx = ctx.tx.clone();
    let task_manager = ctx.task_manager.clone();
    let config = ctx.config.clone();

    // Create message debounce service (shared between listener and history fetcher)
    use crate::services::message_debounce::MessageDebounce;
    let debounce = Arc::new(MessageDebounce::new(ctx.clone(), hook_service.clone()));

    let feishu_listener = Arc::new(FeishuListener::new(
        ctx.clone(),
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

    // Background periodic cleanup: reset stale running bindings every 30 seconds
    // This handles edge cases where the executor crashes or daemon restarts,
    // leaving binding.status permanently stuck at "running".
    {
        let db = db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            // Skip the first tick immediately (give startup time to settle)
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(e) = db.cleanup_stale_running_bindings().await {
                    tracing::warn!("background cleanup_stale_running_bindings failed: {e}");
                }
            }
        });
    }

    // Create and start Feishu push service before AppState
    use crate::services::feishu_push::FeishuPushService;
    let (push_service, push_mutator) = FeishuPushService::new(db.clone(), feishu_listener.clone());
    push_service.start(tx.subscribe());

    // Start Feishu history fetcher with all required dependencies (before AppState to use moved values)
    use crate::services::feishu_history_fetcher::FeishuHistoryFetcher;
    let fetcher = Arc::new(FeishuHistoryFetcher::new(
        ctx,
        feishu_listener.token_manager.clone(),
        feishu_listener.bot_credentials.clone(),
        debounce.clone(),
    ));
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

    // hook_service 由调用方（main.rs）构造后透传进来，这里不再重复创建。
    // 这样 cron 触发的执行、手动 handler 触发的执行、自动评审触发的执行
    // 全部复用同一份 HookService 单例 (见 issue #509)。

    // Create AutoReviewService and ensure the reviewer template todo exists.
    // create_app 是 sync 函数, 不能直接 .await; 复用当前 tokio runtime 的 Handle
    // 同步跑 init (带 block_in_place 避免阻塞 reactor 线程).
    use crate::services::auto_review::{ensure_reviewer_template, DEFAULT_REVIEWER_PROMPT, REVIEWER_TEMPLATE_TITLE};
    {
        let db_for_init = db.clone();
        let init_result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(ensure_reviewer_template(
                &db_for_init,
                REVIEWER_TEMPLATE_TITLE,
                DEFAULT_REVIEWER_PROMPT,
            ))
        });
        if let Err(e) = init_result {
            tracing::warn!("Failed to ensure auto-review reviewer template: {}", e);
        }
    }

    let state = AppState {
        db,
        executor_registry,
        tx: tx.clone(),
        scheduler,
        task_manager,
        config,
        feishu_listener: feishu_listener.clone(),
        feishu_push_mutator: push_mutator,
        hook_service,
    };

    Router::new()
        .route("/", get(index_handler))
        .route("/api/todos", get(todo::get_todos).post(todo::create_todo))
        .route("/api/todos/{id}/force-status", put(todo::force_update_todo_status))
        .route("/api/todos/{id}/tags", put(todo::update_todo_tags))
        .route("/api/todos/{id}/summary", get(execution::get_execution_summary))
        .route("/api/todos/{id}/scheduler", put(scheduler::update_scheduler))
        .route("/api/todos/recent-completed", get(todo::get_recent_completed_todos))
        .route("/api/todos/{id}", get(todo::get_todo).put(todo::update_todo).delete(todo::delete_todo))
        .route("/api/tags", get(tag::get_tags).post(tag::create_tag))
        .route("/api/tags/{id}", delete(tag::delete_tag))
        .route("/api/execution-records", get(execution::get_execution_records))
        .route("/api/execution-records/running", get(execution::get_running_execution_records_handler))
        .route("/api/running-board", get(execution::get_running_board))
        .route("/api/execution-records/session/{session_id}", get(execution::get_execution_records_by_session))
        .route("/api/execution-records/{id}/logs", get(execution::get_execution_logs_handler))
        .route("/api/execution-records/{id}", get(execution::get_execution_record))
        .route("/api/execution-records/{id}/resume", post(execution::resume_execution_handler))
        .route("/api/execution-records/{id}/rating", put(execution::rate_execution_handler))
        .route("/api/dashboard-stats", get(execution::get_dashboard_stats))
        .route("/api/execute", post(execution::execute_handler))
        .route("/api/smart-create", post(execution::smart_create_handler))
        .route("/api/execute/stop", post(execution::stop_execution_handler))
        .route("/api/execute/force-fail", post(execution::force_fail_execution_handler))
        .route("/api/running-todos", get(execution::get_running_todos))
        .route("/api/events", get(events_handler))
        .route("/api/scheduler/todos", get(scheduler::get_scheduler_todos))
        .route("/api/backup/export", get(backup::export_backup))
        .route("/api/backup/export-selected", post(backup::export_selected))
        .route("/api/backup/import", post(backup::import_backup))
        .route("/api/backup/merge", post(backup::merge_backup))
        .route("/api/backup/database/download", get(backup::download_database))
        .route("/api/backup/database/status", get(backup::get_database_backup_status))
        .route("/api/backup/database/trigger", post(backup::trigger_local_backup))
        .route("/api/backup/database/auto", put(backup::update_auto_backup))
        .route("/api/backup/database/optimize", post(backup::database_optimize))
        .route("/api/backup/database/file", get(backup::download_backup_file).delete(backup::delete_backup_file))
        .route("/api/backup/todo/status", get(backup::get_todo_backup_status))
        .route("/api/backup/todo/trigger", post(backup::trigger_todo_backup))
        .route("/api/backup/todo/auto", put(backup::update_todo_auto_backup))
        .route("/api/backup/todo/file", get(backup::download_todo_backup_file).delete(backup::delete_todo_backup_file))
        .route("/api/backup/log-cleanup/status", get(backup::get_log_cleanup_status))
        .route("/api/backup/log-cleanup", put(backup::update_log_cleanup))
        .route("/api/backup/log-cleanup/trigger", post(backup::trigger_log_cleanup))
        .route("/api/backup/skills/status", get(backup::get_skill_backup_status))
        .route("/api/backup/skills/trigger", post(backup::trigger_skill_backup))
        .route("/api/backup/skills/auto", put(backup::update_skill_auto_backup))
        .route("/api/backup/skills/file", get(backup::download_skill_backup_file).delete(backup::delete_skill_backup_file))
        .route("/api/config", get(config::get_config).put(config::update_config))
        .route("/api/executors", get(executor_config::list_executors))
        .route("/api/executors/{name}", put(executor_config::update_executor))
        .route("/api/executors/{name}/detect", post(executor_config::detect_executor))
        .route("/api/executors/{name}/test", post(executor_config::test_executor))
        .route("/api/executors/detect-all", post(executor_config::detect_all_executors))
        .route("/api/executors/{name}/resolve", post(executor_config::resolve_executor_path))
        .route("/api/skills", get(skills::list_skills).delete(skills::delete_skill))
        .route("/api/skills/compare", get(skills::compare_skills))
        .route("/api/skills/sync", post(skills::sync_skill))
        .route("/api/skills/invocations", get(skills::list_invocations).post(skills::record_invocation))
        .route("/api/skills/content", get(skills::get_skill_content))
        .route("/api/skills/export", get(skills::export_skill))
        .route("/api/skills/import", post(skills::import_skill))
        .route("/api/agent-bots", get(agent_bot::list_agent_bots))
        .route("/api/agent-bots/feishu/init", post(agent_bot::feishu_init))
        .route("/api/agent-bots/feishu/begin", post(agent_bot::feishu_begin))
        .route("/api/agent-bots/feishu/poll-stream", get(agent_bot::feishu_poll_sse))
        .route("/api/agent-bots/feishu/push", get(agent_bot::get_feishu_push).put(agent_bot::update_feishu_push))
        .route("/api/agent-bots/feishu/group-whitelist", get(agent_bot::get_group_whitelist).post(agent_bot::add_group_whitelist))
        .route("/api/agent-bots/feishu/group-whitelist/{id}", delete(agent_bot::delete_group_whitelist))
        .route("/api/feishu/history-messages", get(feishu_history::get_history_messages))
        .route("/api/feishu/message-stats", get(feishu_history::get_message_stats))
        .route("/api/feishu/senders", get(feishu_history::get_distinct_senders))
        .route("/api/feishu/history-chats", get(feishu_history::get_history_chats).post(feishu_history::create_history_chat))
        .route("/api/feishu/history-chats/{id}", delete(feishu_history::delete_history_chat).put(feishu_history::update_history_chat))
        .route("/api/feishu/bindings", get(feishu_binding::list_bindings).post(feishu_binding::create_binding))
        .route("/api/feishu/bindings/by-chat", delete(feishu_binding::delete_binding_by_chat))
        .route("/api/feishu/bindings/{id}", delete(feishu_binding::delete_binding))
        .route("/api/feishu/bindings/{id}/enabled", patch(feishu_binding::update_binding_enabled))
        .route("/api/agent-bots/{id}", delete(agent_bot::delete_agent_bot))
        .route("/api/agent-bots/{id}/config", put(agent_bot::update_agent_bot_config))
        .route("/health", get(health_handler))
        // Webhook trigger endpoint (no /api/ prefix, accessible externally).
        // todo_id is required — explicit, deterministic, no "most recent enabled" race.
        .route("/webhook/trigger/{todo_id}", get(webhook::trigger_webhook_with_todo).post(webhook::trigger_webhook_with_todo_post_json))
        // Webhook management APIs
        .route("/api/webhooks", get(webhook::list_webhooks).post(webhook::create_webhook))
        .route("/api/webhooks/{id}", get(webhook::get_webhook).put(webhook::update_webhook).delete(webhook::delete_webhook))
        .route("/api/webhook-records", get(webhook::get_webhook_records))
        .route("/api/webhook-records/{id}", get(webhook::get_webhook_record))
        .route("/assets/{*path}", get(static_handler))
        .route("/api/version", get(version_handler))
        .route("/api/version/latest", get(version_latest_handler))
        .route("/api/version/upgrade", post(version_upgrade_handler))
        .route("/api/sessions", get(session::list_sessions))
        .route("/api/sessions/stats", get(session::get_session_stats))
        .route("/api/sessions/{id}", get(session::get_session_detail).delete(session::delete_session))
        .route("/api/usage-stats", get(usage_stats::get_usage_stats))
        .route("/api/usage-stats/refresh", post(usage_stats::refresh_usage_stats))
        .route("/api/usage-stats/settings", get(usage_stats::get_usage_stats_settings).put(usage_stats::update_usage_stats_settings))
        .merge(project_directory::routes())
        .route("/api/todo-templates", get(todo_template::get_templates).post(todo_template::create_template))
        .route("/api/todo-templates/{id}", put(todo_template::update_template).delete(todo_template::delete_template))
        .route("/api/todo-templates/{id}/copy", post(todo_template::copy_template))
        .route("/api/custom-templates/status", get(custom_template::get_custom_template_status))
        .route("/api/custom-templates/subscribe", post(custom_template::subscribe_custom_template))
        .route("/api/custom-templates/unsubscribe", post(custom_template::unsubscribe_custom_template))
        .route("/api/custom-templates/sync", post(custom_template::sync_custom_template))
        .route("/api/custom-templates/auto-sync", put(custom_template::update_auto_sync_config))
        // 云端同步路由
        .route("/api/cloud/config", get(sync::cloud_get_config).post(sync::cloud_save_config))
        .route("/api/cloud/sync/status", get(sync::cloud_sync_status))
        .route("/api/cloud/sync/records", get(sync::cloud_sync_records).delete(sync::cloud_clear_sync_records))
        .route("/api/cloud/sync/push", post(sync::cloud_sync_push))
        .route("/api/cloud/sync/pull", post(sync::cloud_sync_pull))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB

        .layer(CompressionLayer::new())
        .layer(
            if crate::config::Config::is_dev_mode() {
                // dev 模式 origin 用 `Any`,`expose_headers` 跟 prod 保持一致 (见 `cors_expose_headers`):
                // 前端在 Vite 5173 → 反代 /api/* 时是同源 (不走 CORS),但若有同学启用了直连跨端口
                // (CORS 触发),他们会观察到「服务端日志有 request_id,前端读不到」— 同一痛点只在 prod
                // 能修会让 dev 体验与 prod 不一致。把 expose 列表共享一份常量可避免后续漂移。
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
                    .expose_headers(cors_expose_headers())
            } else {
                // Production: restrict to methods and headers actually used by the API,
                // with configurable origin whitelist (empty = same-origin only).
                let methods = [
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::DELETE,
                    Method::PATCH,
                ];
                let headers = [header::CONTENT_TYPE, header::AUTHORIZATION];

                let origins: Vec<_> = Config::load()
                    .cors_allowed_origins
                    .iter()
                    .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
                    .collect();

                // `expose_headers`: 让浏览器能读到 `x-request-id` 响应头,配合
                // `propagate_request_id` 中间件写回的 request_id,前端日志可以贴到
                // 工单/与上游网关对账。生产环境若不走浏览器可忽略。
                let cors = CorsLayer::new()
                    .allow_methods(methods)
                    .allow_headers(headers)
                    .expose_headers(cors_expose_headers());

                if origins.is_empty() {
                    // No explicit origins = same-origin only (no allow_origin sent)
                    cors
                } else {
                    cors.allow_origin(origins)
                }
            },
        )
        .layer(
            TraceLayer::new_for_http()
                // 自定义 span：把 request_id / method / uri 三个字段直接挂在 span 上，
                // 这样 TraceLayer 默认的 on_request / on_response 日志、以及 handler 内部
                // 所有 tracing::info! / tracing::warn! 输出，都会自动带 request_id 字段。
                // request_id 来自 propagate_request_id 中间件写入的 extensions。
                .make_span_with(|req: &Request| {
                    let request_id = req
                        .extensions()
                        .get::<RequestId>()
                        .map(|r| r.0.clone())
                        .unwrap_or_else(|| "-".to_string());
                    tracing::info_span!(
                        "http_request",
                        request_id = %request_id,
                        method = %req.method(),
                        uri = %req.uri(),
                    )
                }),
        )
        // request_id 中间件放在 TraceLayer 之外（axum 的 layer 栈是后注册先生效），
        // 这样 TraceLayer 创建的 span 上下文里就能拿到 request_id 字段；同时响应头
        // 也由这一层写入 X-Request-Id，前端日志 / 上游网关可以和服务端对账。
        .layer(axum::middleware::from_fn(propagate_request_id))
        .with_state(state)
}

#[cfg(test)]
mod static_handler_tests {
    //! 覆盖 `static_handler` 内联的纯函数：
    //! 1. `guess_mime`：基于 mime_guess 的 MIME 推断
    //! 2. `is_vite_hashed_asset` / `is_vite_hashed_extension`：Vite hash 启发式判断
    //! 3. `is_cacheable_mime`：可下发 immutable 长缓存的 MIME 白名单
    //! 4. `cache_control_for`：综合前两个决定 `Cache-Control` 头
    //!
    //! 这些函数被 `static_handler` 使用，提取为纯函数的目的就是
    //! 不依赖 `Assets` 嵌入资源也能在单元测试里完整覆盖。

    use super::{cache_control_for, guess_mime, is_cacheable_mime, is_vite_hashed_asset, is_vite_hashed_extension};

    // === guess_mime ===

    #[test]
    fn guess_mime_recognises_common_static_types() {
        // mime_guess 的返回值与旧 if-else 链的差异（差异均在浏览器/CDN 接受范围内）：
        //   .js   ：旧 `application/javascript` → 新 `text/javascript`（RFC 9239 当代标准）
        //   .woff ：旧 `font/woff`              → 新 `application/font-woff`
        // 这里以 mime_guess 实际输出为准。
        assert_eq!(guess_mime("foo.js"), "text/javascript");
        assert_eq!(guess_mime("foo.css"), "text/css");
        assert_eq!(guess_mime("index.html"), "text/html");
        assert_eq!(guess_mime("foo.woff2"), "font/woff2");
        assert_eq!(guess_mime("foo.woff"), "application/font-woff");
        assert_eq!(guess_mime("foo.ttf"), "font/ttf");
        assert_eq!(guess_mime("foo.eot"), "application/vnd.ms-fontobject");
        assert_eq!(guess_mime("foo.svg"), "image/svg+xml");
        assert_eq!(guess_mime("foo.png"), "image/png");
        assert_eq!(guess_mime("foo.jpg"), "image/jpeg");
        assert_eq!(guess_mime("foo.jpeg"), "image/jpeg");
        assert_eq!(guess_mime("foo.ico"), "image/x-icon");
        assert_eq!(guess_mime("foo.json"), "application/json");
        assert_eq!(guess_mime("foo.webp"), "image/webp");
    }

    #[test]
    fn guess_mime_handles_extension_case_insensitively() {
        // 早期实现的痛点：`.JS` / `.CSS` 大小写会被错误地归为 octet-stream
        assert_eq!(guess_mime("foo.JS"), "text/javascript");
        assert_eq!(guess_mime("foo.CSS"), "text/css");
        assert_eq!(guess_mime("foo.PnG"), "image/png");
    }

    #[test]
    fn guess_mime_covers_types_missed_by_old_chain() {
        // issue 508 列举的「覆盖不全」类型，mime_guess 应能识别
        assert_eq!(guess_mime("foo.wasm"), "application/wasm");
        assert_eq!(guess_mime("foo.mjs"), "application/javascript");
        assert_eq!(guess_mime("foo.mp4"), "video/mp4");
        assert_eq!(guess_mime("foo.webm"), "video/webm");
    }

    #[test]
    fn guess_mime_falls_back_to_octet_stream_for_unknown_types() {
        // 无扩展名 / 未知扩展名都应回退到 octet-stream，而不是 panic
        assert_eq!(guess_mime("Makefile"), "application/octet-stream");
        assert_eq!(guess_mime("foo.unknownext"), "application/octet-stream");
    }

    #[test]
    fn guess_mime_handles_vite_hashed_paths() {
        // 真实场景：Vite 产物形如 `index-AbCd1234.js`，推断时基于扩展名
        assert_eq!(guess_mime("index-AbCd1234.js"), "text/javascript");
        assert_eq!(guess_mime("assets/index-3aB12cD4.css"), "text/css");
    }

    // === is_vite_hashed_extension ===

    #[test]
    fn is_vite_hashed_extension_matches_expected_set() {
        // Vite 在生产构建中实际会 hash 的扩展名
        for ext in [
            "js", "mjs", "css", "woff", "woff2", "ttf", "eot", "svg", "png", "jpg", "jpeg", "gif",
            "webp", "ico", "json", "map", "wasm",
        ] {
            assert!(is_vite_hashed_extension(ext), "expected true for .{}", ext);
        }
        // 不在白名单：避免对 txt、pdf 等下发 immutable
        for ext in ["txt", "pdf", "zip", "unknown", "exe"] {
            assert!(!is_vite_hashed_extension(ext), "expected false for .{}", ext);
        }
    }

    #[test]
    fn is_vite_hashed_extension_is_case_insensitive() {
        // 真实场景：偶尔会有 `.JS` / `.PNG` 大写命名
        assert!(is_vite_hashed_extension("JS"));
        assert!(is_vite_hashed_extension("PnG"));
    }

    // === is_vite_hashed_asset ===

    #[test]
    fn is_vite_hashed_asset_detects_typical_vite_hashes() {
        // Vite 默认 hash 为 8 位字母数字（[a-zA-Z0-9]）；这里至少 6 位即可
        assert!(is_vite_hashed_asset("index-AbCd1234.js"));
        assert!(is_vite_hashed_asset("assets/index-3aB12cD4.css"));
        assert!(is_vite_hashed_asset("chunk-ABCDef12.mjs"));
    }

    #[test]
    fn is_vite_hashed_asset_rejects_non_hashed_names() {
        // 没有 `-`：不可能是 Vite 产物
        assert!(!is_vite_hashed_asset("index.html"));
        assert!(!is_vite_hashed_asset("style.css"));
    }

    #[test]
    fn is_vite_hashed_asset_rejects_short_hashes() {
        // 早期实现 `path.contains('-')` 的问题：`foo-bar.js` 也会被误判为带 hash
        // 修复后要求 hash 段至少 6 位
        assert!(!is_vite_hashed_asset("foo-bar.js"));
        assert!(!is_vite_hashed_asset("foo-abcde.css"));
        assert!(is_vite_hashed_asset("foo-abcdef.js"));
    }

    #[test]
    fn is_vite_hashed_asset_rejects_non_alphanumeric_hash() {
        // 包含特殊字符的「hash」不算（如 `foo-bar-baz.js` 中 hash 是 `bar-baz`）
        assert!(!is_vite_hashed_asset("foo-bar-baz.js"));
        // 只有字母数字才认
        assert!(is_vite_hashed_asset("foo-123abc456.js"));
    }

    #[test]
    fn is_vite_hashed_asset_rejects_non_hashed_extensions() {
        // 扩展名不在白名单，即使文件名带 hash 也不下发 immutable
        assert!(!is_vite_hashed_asset("doc-AbCd1234.txt"));
        assert!(!is_vite_hashed_asset("data-AbCd1234.zip"));
    }

    // === is_cacheable_mime ===

    #[test]
    fn is_cacheable_mime_allows_js_css_and_fonts() {
        // 这些是 Vite hash 后真正可以下发 immutable 的资源
        // 同时覆盖 mime_guess 实际输出（text/javascript、application/font-woff）
        // 与旧 if-else 链用过的别名（application/javascript、font/woff）
        assert!(is_cacheable_mime("text/javascript"));
        assert!(is_cacheable_mime("application/javascript"));
        assert!(is_cacheable_mime("text/css"));
        assert!(is_cacheable_mime("font/woff2"));
        assert!(is_cacheable_mime("font/woff"));
        assert!(is_cacheable_mime("application/font-woff"));
        assert!(is_cacheable_mime("font/ttf"));
        assert!(is_cacheable_mime("application/vnd.ms-fontobject"));
    }

    #[test]
    fn is_cacheable_mime_rejects_image_and_json() {
        // 图片/JSON 等业务可独立更新的资源暂保持 no-cache
        assert!(!is_cacheable_mime("image/png"));
        assert!(!is_cacheable_mime("image/svg+xml"));
        assert!(!is_cacheable_mime("application/json"));
        assert!(!is_cacheable_mime("video/mp4"));
    }

    // === cache_control_for ===

    #[test]
    fn cache_control_for_vite_hashed_js_gets_immutable() {
        // mime 参数取自 guess_mime 的实际输出
        assert_eq!(
            cache_control_for("index-AbCd1234.js", "text/javascript"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            cache_control_for("assets/style-3aB12cD4.css", "text/css"),
            "public, max-age=31536000, immutable"
        );
        // 同时验证 woff 在 mime_guess 新输出下也能命中 immutable
        assert_eq!(
            cache_control_for("font-AbCd1234.woff", "application/font-woff"),
            "public, max-age=31536000, immutable"
        );
    }

    #[test]
    fn cache_control_for_non_hashed_path_is_no_cache() {
        // 早期 `path.contains('-')` 命中 `foo-bar.js` 的情况应被纠正
        assert_eq!(cache_control_for("foo-bar.js", "text/javascript"), "no-cache");
        assert_eq!(cache_control_for("index.html", "text/html"), "no-cache");
        assert_eq!(cache_control_for("style.css", "text/css"), "no-cache");
    }

    #[test]
    fn cache_control_for_hashed_image_stays_no_cache() {
        // 即使 Vite 也可能 hash 图片，policy 上 image 不下发 immutable
        assert_eq!(cache_control_for("logo-AbCd1234.png", "image/png"), "no-cache");
        assert_eq!(cache_control_for("hero-AbCd1234.svg", "image/svg+xml"), "no-cache");
    }
}

#[cfg(test)]
mod request_id_tests {
    //! 覆盖 `propagate_request_id` 中间件在 issue #513 中的核心行为：
    //! 1. 入站请求无 X-Request-Id 头时生成 UUID；
    //! 2. 入站请求带 X-Request-Id 头时沿用上游 id（避免上游网关/前端的链路被打断）。
    //!
    //! 之所以只测 `resolve_request_id` 这一个纯函数：axum 0.8 的 `Next` 没有公开
    //! 构造函数，无法在单元测试里手工拼出中间件调用栈。把核心逻辑抽成纯函数后既能
    //! 完整覆盖行为，又省去了构造 Router/ServiceExt 的样板代码。
    use super::resolve_request_id;
    use axum::http;

    #[test]
    fn generates_request_id_when_header_missing() {
        let headers = http::HeaderMap::new();
        let id = resolve_request_id(&headers);
        // UUIDv4 长度固定为 36（含 4 个连字符）。
        assert_eq!(id.len(), 36, "expected UUID v4 length, got {id}");
    }

    #[test]
    fn preserves_inbound_request_id() {
        let mut headers = http::HeaderMap::new();
        headers.insert("x-request-id", "trace-abc-123".parse().unwrap());
        let id = resolve_request_id(&headers);
        assert_eq!(id, "trace-abc-123");
    }
}

#[cfg(test)]
mod app_error_tests {
    //! 覆盖 issue #613 重构后的 `AppError`：
    //! 1. `error_response_parts` —— 三个变体的 (status, code, message) 三件套
    //! 2. `from_db_err` / `from_io_err` / `from_scheduler_error` —— 工厂方法
    //! 3. `IntoResponse` 公开行为 —— status code 与 body JSON 与重构前一致
    //!
    //! 验证点：拆分前后对调用方完全等价（status / JSON body 字节级一致）。
    //!
    //! `panic!` 在测试里是 assertion 失败的合理表现（test 失败机制依赖 panic），
    //! 这里把模块级 panic/expect_used 抑制掉，避免 clippy `panic` lint 噪音。
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

    use super::AppError;
    use crate::models::codes;
    use crate::scheduler::SchedulerError;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    // === error_response_parts ===

    #[test]
    fn test_error_response_parts_not_found() {
        let (status, code, message) = AppError::NotFound.error_response_parts();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(code, codes::NOT_FOUND);
        assert_eq!(message, "Not found");
    }

    #[test]
    fn test_error_response_parts_bad_request() {
        let err = AppError::BadRequest("invalid cron".to_string());
        let (status, code, message) = err.error_response_parts();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(code, codes::BAD_REQUEST);
        assert_eq!(message, "invalid cron");
    }

    #[test]
    fn test_error_response_parts_internal() {
        let err = AppError::Internal("db down".to_string());
        let (status, code, message) = err.error_response_parts();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(code, codes::INTERNAL);
        assert_eq!(message, "db down");
    }

    // === from_db_err ===

    #[test]
    fn test_from_db_err_record_not_found_maps_to_not_found() {
        // sea_orm 的 RecordNotFound 是 SeaORM「行不存在」信号，
        // 历史上一律被映射为 404（不是 500），保留该行为。
        let err = sea_orm::DbErr::RecordNotFound("todo 42".into());
        match AppError::from_db_err(err) {
            AppError::NotFound => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn test_from_db_err_other_variants_map_to_internal() {
        // 任何非 RecordNotFound 的数据库错误归 Internal；to_string 透传。
        // 用一个具体变体（Custom）而不是 RecordNotFound，覆盖非 RecordNotFound 分支。
        let err = sea_orm::DbErr::Custom("connection refused".to_string());
        match AppError::from_db_err(err) {
            AppError::Internal(msg) => assert!(msg.contains("connection refused")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // === from_io_err ===

    #[test]
    fn test_from_io_err_maps_to_internal() {
        // io::Error 来自文件系统 / 网络等基础设施层，全部归 500。
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        match AppError::from_io_err(err) {
            AppError::Internal(msg) => assert!(msg.contains("file missing")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // === from_scheduler_error ===

    #[test]
    fn test_from_scheduler_error_invalid_cron_maps_to_bad_request() {
        // 用户输入错误 → 400（issue #499 分类规则）
        let err = SchedulerError::InvalidCron {
            expr: "bad cron".to_string(),
            todo_id: 7,
        };
        match AppError::from_scheduler_error(err) {
            AppError::BadRequest(msg) => assert!(msg.contains("bad cron")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn test_from_scheduler_error_invalid_timezone_maps_to_bad_request() {
        // 用户输入错误 → 400
        let err = SchedulerError::InvalidTimezone("Mars/Olympus".to_string());
        match AppError::from_scheduler_error(err) {
            AppError::BadRequest(msg) => assert!(msg.contains("Mars/Olympus")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn test_from_scheduler_error_internal_maps_to_internal() {
        // 兜底变体 → 500
        let err = SchedulerError::Internal("scheduler panicked".to_string());
        match AppError::from_scheduler_error(err) {
            AppError::Internal(msg) => assert!(msg.contains("scheduler panicked")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // === From trait: 验证 From impl 也走同一工厂方法 ===

    #[test]
    fn test_from_db_err_via_from_trait() {
        let err: sea_orm::DbErr = sea_orm::DbErr::RecordNotFound("x".into());
        let app: AppError = err.into();
        // 通过 From trait 入口走同一工厂；断言变体一致（不写 assert! 的 matches!
        // 返回 bool 直接被丢弃，等于测试空跑——见 scheduler.rs:20 文档约定的写法）。
        assert!(matches!(app, AppError::NotFound));
    }

    #[test]
    fn test_from_io_err_via_from_trait() {
        let err: std::io::Error = std::io::Error::other("boom");
        let app: AppError = err.into();
        assert!(matches!(app, AppError::Internal(_)));
    }

    #[test]
    fn test_from_string_maps_to_bad_request() {
        // 裸 String 约定为 BadRequest（多为校验失败），保留原行为。
        let app: AppError = "bad input".to_string().into();
        match app {
            AppError::BadRequest(msg) => assert_eq!(msg, "bad input"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn test_from_scheduler_error_via_from_trait() {
        let err = SchedulerError::InvalidTimezone("bad/tz".to_string());
        let app: AppError = err.into();
        assert!(matches!(app, AppError::BadRequest(_)));
    }

    // === IntoResponse 公开行为：保证重构前后字节级一致 ===

    /// 把 Response 拆成 (status, body_bytes) 便于断言。
    async fn response_to_parts(
        resp: axum::response::Response,
    ) -> (StatusCode, Vec<u8>) {
        let (parts, body) = resp.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX)
            .await
            .expect("body should collect");
        (parts.status, bytes.to_vec())
    }

    #[tokio::test]
    async fn test_into_response_not_found_returns_404_with_err_code() {
        // 验证：NotFound → 404 + ApiResponse JSON 包含 code=NOT_FOUND & "Not found"
        let resp = AppError::NotFound.into_response();
        let (status, body) = response_to_parts(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let body_str = String::from_utf8_lossy(&body);
        assert!(
            body_str.contains(&format!("\"code\":{}", codes::NOT_FOUND)),
            "body should carry NOT_FOUND code, got: {body_str}"
        );
        assert!(
            body_str.contains("Not found"),
            "body should carry 'Not found' message, got: {body_str}"
        );
    }

    #[tokio::test]
    async fn test_into_response_bad_request_returns_400_with_message() {
        let resp = AppError::BadRequest("invalid cron".to_string()).into_response();
        let (status, body) = response_to_parts(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains(&format!("\"code\":{}", codes::BAD_REQUEST)));
        assert!(body_str.contains("invalid cron"));
    }

    #[tokio::test]
    async fn test_into_response_internal_returns_500_with_message() {
        let resp = AppError::Internal("db down".to_string()).into_response();
        let (status, body) = response_to_parts(resp).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains(&format!("\"code\":{}", codes::INTERNAL)));
        assert!(body_str.contains("db down"));
    }
}
