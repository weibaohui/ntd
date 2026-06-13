use axum::{
    Router,
    extract::{FromRequest, FromRequestParts, Path, Request, State, WebSocketUpgrade},
    http::{self, StatusCode, header},
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
use tokio::sync::{broadcast, oneshot};

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
    pub config: Arc<tokio::sync::RwLock<Config>>,
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
mod feishu_binding;
mod feishu_history;
mod session;
pub mod project_directory;
pub(crate) mod todo_template;
pub mod custom_template;
pub mod webhook;
pub mod usage_stats;
pub mod sync;

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
        // 关键事件)。这里选择 **重新订阅** —— `subscribe()` 拿到的 rx 指向
        // channel 当前 head,自然跳过被覆盖的积压;前端只会看到日志断流
        // 不会断开连接。如果不处理 Lagged,原 `while let Ok(...)` 会立刻
        // 退出 → WS 断开 → 前端误判任务仍在执行。
        //
        // 对比 `services/feishu_push.rs:91-93` 的同模式实现。
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

/// 查询最新版本号，用于前端版本检查提示。
/// 委托给 `updater::check_latest_version()`，由其根据配置决定查询方式。
async fn version_latest_handler() -> impl IntoResponse {
    match crate::updater::check_latest_version().await {
        Ok(latest) => ApiResponse::ok(serde_json::json!({ "latest": latest })),
        Err(e) => {
            tracing::warn!("Failed to check latest version: {}", e);
            ApiResponse::ok(serde_json::json!({ "latest": null, "error": e.to_string() }))
        }
    }
}

/// 给 handler 一个"响应 body 即将被写出"的信号点。
///
/// **用法**:
/// - 在 handler 里加 extractor `ResponseDone`,在 std::thread 里
///   `rx.blocking_recv()` 等信号
/// - 适用场景: handler 同步返回了响应,但仍要起后台线程做"会杀当前进程"的事
///   (典型如 `ntd daemon stop`)。如果不等响应真正落到 wire 就 stop,
///   客户端会看到连接重置,而不是预期的 JSON
///
/// **精度说明**:
/// `drop(tx)` 发生在 `next.run(req).await` 返回时 —— 此时 `Response` 对象
/// 已构造完,但 body 还在内存里,接下来由 axum runtime 异步 flush 到 socket。
/// 从 drop(tx) 到数据真正送达对端,延迟是 µs 级别,远小于
/// `ntd daemon stop` 进程退出 + OS 清理 socket fd 的时间窗口,实际不会丢响应。
///
/// **为什么不进一步精确到"body 字节落到 wire"**:
/// 那样需要包装 response body 实现 `axum::body::MessageBody`,
/// 在 `poll_frame` 返回 `Ready` 时发信号,代码量 +50 行。
/// 当前精度对 `daemon stop` 这种秒级操作已经足够,真要"绝对精确"再加。
///
/// **对其它路由的开销**:
/// 每次请求多一对 oneshot channel (零分配原语 + 一个 wait queue),
/// 没有 receiver 端订阅就直接 drop,基本可忽略。
pub async fn track_response_done(mut req: Request, next: Next) -> Response {
    let (tx, rx) = oneshot::channel();
    // 包装成 `Arc<Mutex<Option<Receiver<()>>>>` 才能塞进 extensions:
    // - axum 和 http 两边的 Extensions::insert 都要求 T: Clone
    // - oneshot::Receiver 是单消费者,本身不可能 Clone
    // - Arc<Mutex<Option<_>>> 是 Clone + Send + Sync,Option 让 handler
    //   端能 .take() 把 receiver 拿走
    let signal: ResponseSignal = Arc::new(std::sync::Mutex::new(Some(rx)));
    req.extensions_mut().insert(signal);
    let resp = next.run(req).await;
    // response 拿出来了,axum 接下来会异步把 body 写到 socket。
    // 在这个点 drop tx,后台线程的 blocking_recv 立刻返回,
    // 继续往下做会杀掉当前进程的工作。
    drop(tx);
    resp
}

/// `track_response_done` 中间件塞进 extensions 的信号句柄。
/// 内部用 Mutex 保护 oneshot::Receiver(后者不是 Sync,
/// 必须在 Mutex 里才能跨线程共享)。
pub type ResponseSignal = Arc<std::sync::Mutex<Option<oneshot::Receiver<()>>>>;

/// 自定义 extractor: 从 request extensions 拿走中间件塞进来的 oneshot receiver。
///
/// 之所以不走 `Extension<oneshot::Receiver<()>>`,是因为 `Extension<T>`
/// 要求 `T: Clone + Send + Sync`,而 `oneshot::Receiver` 是单消费者的,
/// 不可能 Clone。我们包一层 `Arc<Mutex<Option<_>>>` 满足 Clone bound,
/// 在 extractor 里 `.take()` 把 receiver 拿走。
pub struct ResponseDone(pub oneshot::Receiver<()>);

impl<S> FromRequestParts<S> for ResponseDone
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let signal = parts
            .extensions
            .remove::<ResponseSignal>()
            .expect("track_response_done middleware must be installed before any handler that uses ResponseDone");
        let rx = signal
            .lock()
            .expect("ResponseSignal mutex poisoned")
            .take()
            .expect("ResponseDone extractor can only be used once per request");
        Ok(ResponseDone(rx))
    }
}

/// 执行升级并重新部署 daemon 服务。
///
/// 根据配置中的 `update.source` 派发到对应安装方式：
/// - `npm`: 检测 npm 全局目录写权限 → 调用 `npm install -g <pkg>@latest` → 升级成功后将
///   daemon 重部署步骤（stop → uninstall → install --force → start）fork 到独立子进程执行
/// - `manual` / `cargo` / `apt`: 委托给 `UpdateSource::upgrade()`，由用户自行完成安装，
///   daemon 重部署仅对 npm 方式适用（其他方式不涉及可执行文件替换）
async fn version_upgrade_handler(
    State(state): State<AppState>,
    ResponseDone(redeploy_signal): ResponseDone,
) -> impl IntoResponse {
    // 从注入的 AppState 读取内存中的配置，避免重复磁盘 I/O。
    // 使用 RwLock 的 read() 获取共享只读锁，因为这里只需查询 update 配置段，不做写入。
    let cfg = state.config.read().await;
    let source = crate::updater::UpdateSource::from_config_ref(&cfg);
    // 显式释放读锁，使后续操作（如 npm 命令执行）不会在持有锁时阻塞其他线程。
    drop(cfg);

    // 将 InstallMethod 枚举映射为稳定的小写字符串，用于 API 响应中的 method 字段。
    // 避免泄漏 Debug 格式（如 "InstallMethod::Npm"），确保前端能根据固定字符串判断安装方式。
    let method_str = match source.method {
        crate::updater::InstallMethod::Npm => "npm",
        crate::updater::InstallMethod::Cargo => "cargo",
        crate::updater::InstallMethod::Apt => "apt",
        crate::updater::InstallMethod::Manual => "manual",
    };

    // 非 npm 安装方式（manual/cargo/apt）由用户手动升级，
    // 直接委托给 UpdateSource::upgrade() 处理，不再走 npm 硬编码路径。
    // 这些方式的升级不会替换当前可执行文件，因此不触发 daemon 自动重部署。
    if !matches!(source.method, crate::updater::InstallMethod::Npm) {
        // 为非 npm 方式生成对应的提示消息，引导用户完成手动操作步骤。
        let message = match source.method {
            crate::updater::InstallMethod::Cargo => {
                format!("请手动执行: cargo install {}@latest", source.package_name())
            }
            crate::updater::InstallMethod::Apt => {
                "请手动执行: sudo apt update && sudo apt upgrade ntd".to_string()
            }
            crate::updater::InstallMethod::Manual => {
                "请前往 https://github.com/weibaohui/nothing-todo/releases 下载最新版本".to_string()
            }
            _ => "升级完成（非 npm 安装方式，无需自动重部署 daemon）".to_string(),
        };

        match source.upgrade().await {
            Ok(_) => {
                // 统一响应格式：所有分支返回相同的字段名和类型，方便前端解析。
                // upgraded: 是否已完成升级操作（对非 npm 方式，此处为 true 表示提示已输出）
                // restarted: 是否触发了 daemon 自动重部署（非 npm 方式始终为 false）
                // method: 安装方式的稳定字符串标识
                // message: 用户可读的操作提示或结果说明
                return ApiResponse::ok(serde_json::json!({
                    "upgraded": true,
                    "restarted": false,
                    "method": method_str,
                    "message": message
                }));
            }
            Err(e) => {
                return ApiResponse::err(1, &format!("upgrade failed: {}", e));
            }
        }
    }

    // 检测 npm 全局目录写权限，获取安全的安装 prefix
    let prefix = crate::updater::get_npm_global_prefix();

    // 执行 npm 升级，捕获输出以便返回给前端展示
    let npm_result = std::process::Command::new("npm")
        .args([
            "install",
            "-g",
            // 指定 prefix 确保安装到有写权限的目录
            &format!("--prefix={}", prefix),
            // 使用配置中的包名，确保与 UpdateConfig.npm_package 一致
            &format!("{}@latest", source.package_name()),
        ])
        .output();

    let npm_stdout;
    let npm_stderr;
    let npm_success;

    match &npm_result {
        Ok(out) => {
            npm_stdout = String::from_utf8_lossy(&out.stdout).to_string();
            npm_stderr = String::from_utf8_lossy(&out.stderr).to_string();
            npm_success = out.status.success();
            tracing::info!("npm upgrade stdout: {}, stderr: {}", npm_stdout, npm_stderr);
        }
        Err(e) => {
            npm_stdout = String::new();
            npm_stderr = e.to_string();
            npm_success = false;
            tracing::error!("Failed to run npm: {}", e);
        }
    }

    if !npm_success {
        let err_msg = if npm_stderr.is_empty() {
            "npm upgrade failed".to_string()
        } else {
            format!("npm upgrade failed: {}", npm_stderr)
        };
        return ApiResponse::err(1, &err_msg);
    }

    // npm 升级成功，查找新安装的 ntd 可执行文件路径
    let ntd_cmd = crate::updater::find_ntd_binary(&prefix);

    // 关键：先返回响应给前端，再 fork 子进程执行 daemon 重部署。
    // 因为 stop 会终止当前 daemon 进程（即本 handler 所在进程），
    // 如果在当前进程中顺序执行 stop→uninstall→install→start，
    // stop 后后续步骤可能无法执行完成。
    //
    // 真正的 cgroup 脱离逻辑在 `daemon::spawn_detached_redeploy` 里实现：
    // 用 `systemd-run --scope` 把 redeploy 脚本放到独立 transient scope，
    // 避免 ntd.service stop 时 cgroup 清理把脚本一起杀掉。
    // macOS (launchd) 不走 systemd，直接 sh -c 即可。
    let ntd_cmd_clone = ntd_cmd.clone();
    std::thread::spawn(move || {
        // 等到响应 body 真正被 axum 准备送出(stop daemon 会杀掉
        // 当前进程,如果不等人可能响应就丢了)。
        // 信号由 `track_response_done` 中间件在 `next.run` 返回时触发,
        // 比"固定 sleep 1 秒"更精确,也消除了 magic number。
        // blocking_recv 因为我们在 std::thread 里,不是 tokio runtime。
        let _ = redeploy_signal.blocking_recv();

        // 将 daemon 重部署的四步操作合并成一条 shell 命令，用 && 连接。
        // stop 失败不阻断（服务可能已停止），但 uninstall/install/start 任一步
        // 失败都会导致整体失败，符合预期。
        let redeploy_script = format!(
            "{} daemon stop && {} daemon uninstall && {} daemon install --force && {} daemon start",
            ntd_cmd_clone, ntd_cmd_clone, ntd_cmd_clone, ntd_cmd_clone
        );

        #[cfg(target_os = "linux")]
        {
            // 委托给 daemon 模块,它会:
            // 1) 探测当前 install mode (system / user)
            // 2) 用 systemd-run --scope 把 sh 拉到独立 cgroup
            // 3) stdio 重定向到 /dev/null + 日志文件,失败可查
            match crate::daemon::spawn_detached_redeploy(&redeploy_script) {
                Ok(()) => tracing::info!("Daemon redeploy dispatched via systemd-run"),
                Err(e) => tracing::error!("Daemon redeploy dispatch failed: {e}"),
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            // macOS/Windows: launchd/Task Scheduler 不使用 cgroup,
            // sh -c 子进程会被 reparent 到 PID 1,daemon stop 不会牵连。
            // 保留原行为,不做 cgroup 隔离。
            let result = std::process::Command::new("sh")
                .args(["-c", &redeploy_script])
                .stdin(std::process::Stdio::null())
                .status();

            match &result {
                Ok(s) if s.success() => tracing::info!("Daemon redeployed successfully"),
                Ok(s) => tracing::error!("Daemon redeploy failed with exit code {}", s),
                Err(e) => tracing::error!("Daemon redeploy exec error: {}", e),
            }
        }
    });

    // 立即返回成功响应，daemon 重部署在后台子线程中执行。
    // 统一响应格式：与非 npm 分支保持一致的字段名和语义，
    // 确保前端能用统一逻辑解析所有安装方式的响应。
    // - upgraded: 是否已完成 npm 升级
    // - restarted: 是否已触发后台 daemon 重部署
    // - method: 安装方式（固定为 "npm"）
    // - message: 包含 npm 输出和重部署提示，供前端展示给用户
    ApiResponse::ok(serde_json::json!({
        "upgraded": true,
        "restarted": true,
        "method": method_str,
        "message": format!(
            "npm 升级成功，正在后台重新部署服务，请稍后刷新页面\n\nnpm 输出:\n{}",
            npm_stdout
        )
    }))
}

// Build router
pub fn create_app(
    ctx: ServiceContext,
    scheduler: Arc<TodoScheduler>,
) -> Router {
    let db = ctx.db.clone();
    let executor_registry = ctx.executor_registry.clone();
    let tx = ctx.tx.clone();
    let task_manager = ctx.task_manager.clone();
    let config = ctx.config.clone();

    // Create message debounce service (shared between listener and history fetcher)
    use crate::services::message_debounce::MessageDebounce;
    let debounce = Arc::new(MessageDebounce::new(ctx.clone()));

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

    // Create HookService with a ServiceContext so it can trigger target todos.
    // Reuse the live ServiceContext values rather than re-cloning its fields.
    let hook_ctx = ServiceContext {
        db: db.clone(),
        executor_registry: executor_registry.clone(),
        tx: tx.clone(),
        task_manager: task_manager.clone(),
        config: config.clone(),
    };
    let hook_service = Arc::new(HookService::new(hook_ctx));

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
        // 给 handler 一个"响应 body 即将送出"的信号点,用于
        // "返回响应后还要做会杀当前进程的事"的场景(典型如版本升级)。
        // 注释见 `track_response_done` 定义。
        .layer(axum::middleware::from_fn(track_response_done))
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
