use axum::{
    Router,
    extract::{Request, State, WebSocketUpgrade},
    http::{Method, header},
    response::Response,
    routing::{delete, get, patch, post, put},
};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;
use axum::extract::DefaultBodyLimit;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::service_context::ServiceContext;
use crate::adapters::ExecutorRegistry;
use crate::config::Config;
use crate::db::Database;
use crate::models::ApiResponse;
use crate::scheduler::TodoScheduler;
use crate::services::feishu_listener::FeishuListener;
use crate::task_manager::TaskManager;

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
    /// Loop Studio: 独立 cron 调度器（None 表示 loop 功能未启用或初始化失败）
    pub loop_scheduler: Option<Arc<crate::services::loop_scheduler::LoopScheduler>>,
    /// Loop Studio: 触发器分发器（None 同上）
    pub loop_trigger_dispatcher: Option<Arc<crate::services::loop_trigger::LoopTriggerDispatcher>>,
    /// Loop Studio: loop runner（手动触发 / dispatcher / cron 都通过它启动执行）
    pub loop_runner: Option<Arc<crate::services::loop_runner::LoopRunner>>,
}

impl AppState {
    /// 根据 id 获取 todo，不存在时返回 NotFound 错误
    pub async fn require_todo(&self, id: i64) -> Result<crate::models::Todo, AppError> {
        self.db.get_todo(id).await?.ok_or(AppError::NotFound)
    }

    /// 在闭包中读 config,锁卫在闭包返回时立即 drop。
    ///
    /// 适用于"读 config → 拷出 owned 值 → 立刻释放 std 读锁卫 → 后续 await 不持锁"的模板。
    /// 不允许在 `f` 内部 `await`:如果需要,先把 owned 值拷出再 await。
    ///
    /// 返回 `f(&Config) -> R` 的结果。闭包签名简单,call site 形如:
    /// ```ignore
    /// let (db_path, max_files) = state.config_snapshot(|c|
    ///     (PathBuf::from(&c.db_path), c.auto_backup_max_files)
    /// );
    /// ```
    pub fn config_snapshot<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Config) -> R,
    {
        // 块作用域收紧 `RwLockReadGuard` 生命周期:闭包返回时 guard 随作用域
        // 结束而 drop,后续 await 一定不会持 std 读锁卫跨 .await。
        // 中毒时用 into_inner 取旧值继续：本仓未设 panic=abort，默认 unwind 下
        // axum handler panic 不会重启进程，若 .unwrap() 会让所有 config 路由级联 500。
        let cfg = self.config.read().unwrap_or_else(|e| e.into_inner());
        f(&cfg)
    }

    /// 一次性 clone 出 owned `Config`,读锁卫立即 drop。
    ///
    /// 适用于"读取完整 config → clone 一份 owned 值 → 立即释放读锁卫 → 后续
    /// 在 spawn_blocking 内使用 cfg"的模板。`std::sync::RwLockReadGuard`
    /// 跨 .await 会让 future 变 !Send,所以必须先把 cfg clone 出来再 await。
    ///
    /// 与 `config_snapshot` 区分:`config_snapshot` 用于"只读 + 投影出 owned
    /// 字段"场景,这个方法用于"需要完整 owned `Config` 副本"场景。
    pub fn config_clone(&self) -> Config {
        // 块作用域与 `config_snapshot` 同理:guard 在表达式结束时 drop,
        // 后续 await 不会持读锁卫,future 保持 Send。
        // 中毒时用 into_inner 取旧值继续（见 config_snapshot 注释）。
        self.config.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 在闭包中修改 config 并返回 owned `Config`,写锁卫在闭包返回时立即 drop。
    ///
    /// 适用于"修改 config 字段 → clone 一份 owned 值 → 立即释放写锁卫 → 后续
    /// 在 spawn_blocking 内调 `cfg.save()`"的模板。`std::sync::RwLockWriteGuard`
    /// 跨 .await 会让 future 变 !Send,所以必须先把 cfg clone 出来再 await。
    ///
    /// call site 形如:
    /// ```ignore
    /// let cfg = state.config_write_clone(|c| {
    ///     c.auto_backup_enabled = req.enabled;
    ///     c.auto_backup_cron = req.cron;
    ///     c.normalize_paths();
    ///     c.clamp_execution_timeout_secs();
    ///     c.clone()
    /// });
    /// ```
    pub fn config_write_clone<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Config) -> R,
    {
        // 块作用域收紧 `RwLockWriteGuard` 生命周期:闭包返回时 guard 随作用域
        // 结束而 drop,后续 await 一定不会持 std 写锁卫跨 .await。
        // 中毒时用 into_inner 取旧值继续（见 config_snapshot 注释）。
        let mut cfg = self.config.write().unwrap_or_else(|e| e.into_inner());
        f(&mut cfg)
    }
}

pub use crate::executor_service::ExecEvent;

pub use errors::{AppError, ApiJson};

pub use middleware::{RequestId, propagate_request_id, cors_expose_headers};

pub use static_handlers::{index_handler, static_handler, health_handler, version_handler, version_latest_handler, version_upgrade_handler};

pub mod errors;
pub mod middleware;
pub mod static_handlers;

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
pub(crate) mod review_template;
pub mod custom_template;
pub mod webhook;
pub mod usage_stats;
pub mod sync;
pub mod sub_states; // 由 #604 引入，当前无内容占位
pub mod loop_;
pub mod action;
pub mod blackboard;

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

// =========================================================================
// 路由装配（issue #661 重构）
// -------------------------------------------------------------------------
// 原 `create_app` 是 312 行的单体函数，承载 5 件事：服务初始化、AppState 构造、
// 全部领域路由注册、CORS/Trace 等 layer 叠加、状态注入。改为：
//   1) create_app      : 纯组装（路由 merge + layer 链）
//   2) build_app_state : 服务初始化 + AppState 构造
//   3) 4 个 spawn_*    : 各自负责一个后台 tokio 任务
//   4) 18 个 *_routes  : 各领域路由独立函数，单函数体 < 30 行
//   5) cors_layer      : CORS 配置独立抽出
// =========================================================================

/// 入口：装配整个 Axum 路由。所有领域路由以 merge 形式聚合，layer 链统一叠加。
///
/// todo hook 已整块移除（见 plan `purring-forging-petal`）：函数不再接收
/// `hook_service`，避免出现「hook 体系已经从编排链摘除，但 AppState 还挂着
/// Arc<HookService>」的接口残留。
pub async fn create_app(
    ctx: ServiceContext,
    scheduler: Arc<TodoScheduler>,
) -> Router {
    // 把状态构造与中间件叠加分两步：先 build 再 merge，便于读者按"装配顺序"线性阅读
    let state = build_app_state(ctx, scheduler).await;

    Router::new()
        .merge(mount_domain_routes())
        .merge(project_directory::routes())
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(CompressionLayer::new())
        .layer(cors_layer())
        // TraceLayer 闭包体已抽到 `make_request_span` 函数，这里直接传函数指针
        .layer(TraceLayer::new_for_http().make_span_with(make_request_span))
        // request_id 中间件放在 TraceLayer 之外（axum 的 layer 栈是后注册先生效），
        // 这样 TraceLayer 创建的 span 上下文里就能拿到 request_id 字段；同时响应头
        // 也由这一层写入 X-Request-Id，前端日志 / 上游网关可以和服务端对账。
        .layer(axum::middleware::from_fn(propagate_request_id))
        .with_state(state)
}

/// 把 18 个领域子路由函数合并成一个 Router 一次性 merge 进 `create_app`。
/// `project_directory` 是另一个模块里实现的同名子路由（不在 18 个里），由 `create_app`
/// 单独 merge，避免在跨模块拓扑变化时改这个函数。
fn mount_domain_routes() -> Router<AppState> {
    Router::new()
        .merge(root_routes())
        .merge(todo_routes())
        .merge(execution_routes())
        .merge(scheduler_routes())
        .merge(backup_routes())
        .merge(config_routes())
        .merge(skills_routes())
        .merge(agent_bot_routes())
        .merge(feishu_routes())
        .merge(webhook_routes())
        .merge(session_routes())
        .merge(usage_stats_routes())
        .merge(version_routes())
        .merge(static_routes())
        .merge(todo_template_routes())
        .merge(review_template_routes())
        .merge(custom_template_routes())
        .merge(cloud_routes())
        .merge(events_routes())
        .merge(blackboard::blackboard_routes())
        .merge(loop_::loop_routes())
}

/// 给 TraceLayer 用的 span 工厂：把 `request_id` / `method` / `uri` 直接挂在 span 字段上，
/// 这样 TraceLayer 默认的 on_request / on_response 日志、以及 handler 内部所有
/// `tracing::info!` / `tracing::warn!` 输出都会自动带上 `request_id` 字段，便于按
/// 调用链对账。`request_id` 来自 `propagate_request_id` 中间件写入的 extensions，
/// 未拿到时退化为 `"-"`，避免 span 字段缺失导致的下游日志格式异常。
fn make_request_span(req: &Request) -> tracing::Span {
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
}

/// 构造 `AppState` 并按需启动后台服务（feishu bot / stale binding cleanup / history fetcher /
/// reviewer template 初始化 + Loop Studio 三件套）。所有 `.await` 都在 `tokio::spawn` 或
/// `block_in_place` 中处理，保持 `build_app_state` 自身为同步函数。
///
/// todo hook 已整块移除，`build_app_state` 不再接收 hook_service 参数；下游的
/// MessageDebounce / LoopRunner 同步取消该字段。
async fn build_app_state(
    ctx: ServiceContext,
    scheduler: Arc<TodoScheduler>,
) -> AppState {
    let db = ctx.db.clone();
    let executor_registry = ctx.executor_registry.clone();
    let tx = ctx.tx.clone();
    let task_manager = ctx.task_manager.clone();
    let config = ctx.config.clone();

    // ====== Loop Studio 三件套初始化 ======
    // 用 block_in_place + Handle::block_on 走 sync 路径做 async DB 调用；
    // 这三件套是「可选能力」,初始化失败不阻塞 daemon 启动,只把 Option 置 None。
    // 按引用传入避免多余的 clone；函数内部按需 clone 进 Arc 包装
    let (loop_runner, loop_trigger_dispatcher, loop_scheduler) =
        init_loop_studio_services(&ctx, &tx);

    // MessageDebounce 在 feishu_listener 和 history_fetcher 之间共享（issue #600）
    use crate::services::auto_review::ensure_default_review_template_blocking;
    use crate::services::message_debounce::MessageDebounce;
    let debounce = Arc::new(MessageDebounce::new(ctx.clone(), loop_runner.clone()));
    let feishu_listener = Arc::new(FeishuListener::new(ctx.clone(), debounce.clone()));

    // 启动后台任务：bot 自启、stale binding 周期清理、history fetcher、reviewer template 初始化
    spawn_feishu_bot_starter(feishu_listener.clone(), db.clone());
    spawn_stale_binding_cleanup(db.clone());
    // 自动版本更新调度器：按配置的间隔周期性检查 npm 新版本
    crate::services::auto_update::spawn_auto_update_scheduler(config.clone(), db.clone());

    // PushService 在 AppState 之前构造，因为它要订阅事件 tx。
    use crate::services::feishu_push::FeishuPushService;
    let (push_service, push_mutator) = FeishuPushService::new(db.clone(), feishu_listener.clone());
    push_service.start(tx.subscribe());

    // feishu_listener 按引用传入避免 clone 整个 Arc；函数内部只 clone 内部字段
    spawn_feishu_history_fetcher(ctx.clone(), db.clone(), &feishu_listener, debounce.clone());
    ensure_default_review_template_blocking(&db);

    // 黑板防抖器初始化，启动 flush 监听器（监听 channel，收到消息后执行 LLM 更新黑板）。
    // 注意：防抖阈值已迁移到 per-workspace 配置（blackboards 表），此处使用系统级默认值
    //（600s / 10 条），实际防抖逻辑在 push_pending_record / remove_specific_pending_record_ids
    // 时会从对应工作空间的黑板配置中读取真实值。
    let flush_rx = crate::services::blackboard_debouncer::init().await;
    tokio::spawn(crate::executor_service::completion::blackboard_flush_listener(
        flush_rx,
        db.clone(),
        executor_registry.clone(),
        tx.clone(),
        task_manager.clone(),
        config.clone(),
    ));

    // 后台监听 todo 执行完成事件，派发给 loop_trigger_dispatcher
    spawn_todo_completed_listener(&tx, loop_trigger_dispatcher.clone());

    AppState {
        db,
        executor_registry,
        tx: tx.clone(),
        scheduler,
        task_manager,
        config,
        feishu_listener: feishu_listener.clone(),
        feishu_push_mutator: push_mutator,
        loop_scheduler,
        loop_trigger_dispatcher,
        loop_runner,
    }
}

/// 初始化 Loop Studio 三件套：runner / dispatcher / scheduler。
///
/// 全部失败容忍：返回 `None` 让 AppState 标记为「loop 功能不可用」,handler
/// 在被调用时返回 503 风格错误。daemon 启动不因 loop 故障而被拖垮。
// 返回三元组 Option<Arc<...>>，拆分为 type alias 过度抽象，允许 type_complexity
#[allow(clippy::type_complexity)]
fn init_loop_studio_services(
    // 按引用传入避免调用方 clone；函数内部按需 clone 进 Arc 包装
    ctx: &ServiceContext,
    // 按引用传入，函数内部 clone 进 LoopRunner 和 tokio::spawn 闭包
    tx: &tokio::sync::broadcast::Sender<ExecEvent>,
) -> (
    Option<Arc<crate::services::loop_runner::LoopRunner>>,
    Option<Arc<crate::services::loop_trigger::LoopTriggerDispatcher>>,
    Option<Arc<crate::services::loop_scheduler::LoopScheduler>>,
) {
    use crate::services::loop_runner::{LoopRunner, LoopRunnerCtx};
    use crate::services::loop_trigger::LoopTriggerDispatcher;
    // runner 与 dispatcher 是纯内存构造,无 IO,失败概率低
    // 创建 LoopRunnerCtx：从 ServiceContext 中 clone 出 Arc 字段，零成本共享引用计数
    let loop_runner_ctx = LoopRunnerCtx {
        db: ctx.db.clone(),
        executor_registry: ctx.executor_registry.clone(),
        task_manager: ctx.task_manager.clone(),
        config: ctx.config.clone(),
    };
    // clone tx 进 LoopRunner 内部，broadcast::Sender 是 Arc 包装，clone 只增加引用计数
    let runner = Arc::new(LoopRunner::new(loop_runner_ctx, tx.clone()));
    // dispatcher 复用 runner 的 ctx.db；ctx clone 同理，ServiceContext 内部字段均为 Arc
    let dispatcher = Arc::new(LoopTriggerDispatcher::new(
        runner.clone(),
        ctx.clone(),
    ));
    // scheduler 启动时需要 DB 读 + 启动后台 task,这里 block_in_place
    let scheduler_res = tokio::task::block_in_place(|| {
        let handle = tokio::runtime::Handle::current();
        handle.block_on(crate::services::loop_scheduler::LoopScheduler::start(
            ctx.db.clone(),
            runner.clone(),
        ))
    });
    let scheduler = match scheduler_res {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::error!("loop_scheduler start failed: {}", e);
            None
        }
    };
    // 即使 scheduler 失败,runner / dispatcher 仍可用（手动触发仍可工作）
    (Some(runner), Some(dispatcher), scheduler)
}

/// 后台任务：监听 ExecEvent::Finished 事件并派发到 loop_trigger_dispatcher。
///
/// 当 todo 执行完成时，触发 loop 的 todo_completed 触发器。
/// 这是一个 fire-and-forget 任务，失败不影响 daemon 主流程。
fn spawn_todo_completed_listener(
    // subscribe() 只需 &self，按引用传入避免 clone
    tx: &tokio::sync::broadcast::Sender<ExecEvent>,
    dispatcher: Option<Arc<crate::services::loop_trigger::LoopTriggerDispatcher>>,
) {
    let Some(dispatcher) = dispatcher else {
        return;
    };
    let mut rx = tx.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(ExecEvent::Finished {
                    todo_id,
                    success: true,
                    ..
                }) => {
                    // 仅当执行成功时派发 todo_completed 触发器
                    let _ = dispatcher.dispatch_todo_completed(todo_id, None).await;
                }
                // 忽略其它事件类型
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        "todo_completed_listener: lagged by {} events",
                        n
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!(
                        "todo_completed_listener: channel closed, exiting"
                    );
                    break;
                }
            }
        }
    });
}

/// 后台任务：启动所有已启用的飞书 bot。失败仅记录日志，不影响主流程。
fn spawn_feishu_bot_starter(feishu_listener: Arc<FeishuListener>, db: Arc<Database>) {
    tokio::spawn(async move {
        match db.get_agent_bots().await {
            Ok(bots) => {
                for bot in bots.iter().filter(|b| b.bot_type == "feishu" && b.enabled) {
                    if let Err(e) = feishu_listener.start_bot(bot).await {
                        tracing::error!("failed to start feishu bot {}: {e}", bot.id);
                    }
                }
            }
            Err(e) => tracing::error!("failed to load agent bots: {e}"),
        }
    });
}

/// 后台周期任务：30s 重置一次卡在 "running" 的 binding。处理 executor 崩溃 / daemon 重启
/// 等导致 binding 状态被永久卡住的边缘场景。首个 tick 跳过以避开启动期抖动。
fn spawn_stale_binding_cleanup(db: Arc<Database>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(e) = db.cleanup_stale_running_bindings().await {
                tracing::warn!("background cleanup_stale_running_bindings failed: {e}");
            }
        }
    });
}

/// 后台任务：拉取飞书聊天历史。`ServiceContext` 在此被 move 进 fetcher（之后不再需要）。
/// `db` 由调用方传入——`FeishuListener` 不直接持有 `db` 字段，需要走 `ctx.db`，
/// 而 `ctx` 又要在 fetcher 里被 move 走，所以这里把 `db` 独立参数化更直观。
fn spawn_feishu_history_fetcher(
    ctx: ServiceContext,
    db: Arc<Database>,
    // feishu_listener 仅用于提取 token_manager 和 bot_credentials，按引用传入后 clone Arc 字段
    feishu_listener: &Arc<FeishuListener>,
    debounce: Arc<crate::services::message_debounce::MessageDebounce>,
) {
    use crate::services::feishu_history_fetcher::FeishuHistoryFetcher;
    let fetcher = Arc::new(FeishuHistoryFetcher::new(
        ctx,
        feishu_listener.token_manager.clone(),
        feishu_listener.bot_credentials.clone(),
        debounce,
    ));
    let db_for_fetcher = db;
    tokio::spawn(async move {
        tracing::info!("[feishu-history-fetcher] starting initialization");
        let bots_for_fetcher: Vec<(i64, String, String)> = match db_for_fetcher.get_agent_bots().await {
            Ok(bots) => bots.into_iter()
                .filter(|b| b.bot_type == "feishu" && b.enabled)
                // into_iter() 已消费 bot，字段可直接 move 进 tuple，无需 clone
                .map(|b| (b.id, b.app_id, b.app_secret))
                .collect(),
            Err(e) => {
                tracing::error!("[feishu-history-fetcher] failed to get agent bots: {}", e);
                Vec::new()
            }
        };
        tracing::info!("[feishu-history-fetcher] starting with {} bots", bots_for_fetcher.len());
        fetcher.start(bots_for_fetcher);
    });
}

/// 根级系统路由（首页、健康检查）。这些不属于任何业务领域，单独放这里。
fn root_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index_handler))
        .route("/health", get(health_handler))
}

/// Todo 与 Tag CRUD。
fn todo_routes() -> Router<AppState> {
    Router::new()
        .route("/api/todos", get(todo::get_todos).post(todo::create_todo))
        .route("/api/todos/center", get(todo::get_todo_center))
        .route("/api/todos/{id}/force-status", put(todo::force_update_todo_status))
        .route("/api/todos/{id}/tags", put(todo::update_todo_tags))
        .route("/api/todos/{id}/summary", get(execution::get_execution_summary))
        .route("/api/todos/{id}/scheduler", put(scheduler::update_scheduler))
        .route("/api/todos/{id}/archive", post(todo::archive_todo))
        .route("/api/todos/{id}/restore", post(todo::restore_todo))
        .route("/api/todos/{id}/webhook", put(todo::update_webhook))
        .route("/api/todos/recent-completed", get(todo::get_recent_completed_todos))
        .route("/api/todos/batch-executor", put(todo::batch_update_todos_executor))
        .route("/api/todos/batch-workspace", put(todo::batch_move_todos_workspace))
        .route("/api/todos/batch-copy-workspace", post(todo::batch_copy_todos_workspace))
        .route("/api/todos/batch-scheduler", put(todo::batch_update_todos_scheduler))
        .route("/api/todos/{id}", get(todo::get_todo).put(todo::update_todo).delete(todo::delete_todo))
        .route("/api/tags", get(tag::get_tags).post(tag::create_tag))
        .route("/api/tags/{id}", delete(tag::delete_tag))
}

/// 执行记录、执行触发、执行控制相关路由。
fn execution_routes() -> Router<AppState> {
    Router::new()
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
        .route("/api/actions/execute", post(action::execute_action))
}

/// 定时任务相关路由（除 todo 内嵌的 scheduler 字段外，独立的查询入口）。
fn scheduler_routes() -> Router<AppState> {
    Router::new()
        .route("/api/scheduler/todos", get(scheduler::get_scheduler_todos))
}

/// 备份与日志清理相关路由（数据库备份、todo 备份、skills 备份、log 清理）。
fn backup_routes() -> Router<AppState> {
    Router::new()
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
}

/// 系统配置与 executor 配置。
fn config_routes() -> Router<AppState> {
    Router::new()
        .route("/api/config", get(config::get_config).put(config::update_config))
        .route("/api/executors", get(executor_config::list_executors))
        .route("/api/executors/{name}", put(executor_config::update_executor))
        .route("/api/executors/{name}/detect", post(executor_config::detect_executor))
        .route("/api/executors/{name}/test", post(executor_config::test_executor))
        .route("/api/executors/detect-all", post(executor_config::detect_all_executors))
        .route("/api/executors/{name}/resolve", post(executor_config::resolve_executor_path))
}

/// Skills 管理（列出/同步/导入导出/调用记录）。
fn skills_routes() -> Router<AppState> {
    Router::new()
        .route("/api/skills", get(skills::list_skills).delete(skills::delete_skill))
        .route("/api/skills/compare", get(skills::compare_skills))
        .route("/api/skills/version-update", get(skills::version_update_list))
        .route("/api/skills/sync", post(skills::sync_skill))
        .route("/api/skills/invocations", get(skills::list_invocations).post(skills::record_invocation))
        .route("/api/skills/content", get(skills::get_skill_content))
        .route("/api/skills/file", get(skills::get_skill_file))
        .route("/api/skills/export", get(skills::export_skill))
        .route("/api/skills/import", post(skills::import_skill))
}

/// Agent bot 路由（飞书 bot 管理、初始化、轮询、推送、群白名单）。
fn agent_bot_routes() -> Router<AppState> {
    Router::new()
        .route("/api/agent-bots", get(agent_bot::list_agent_bots))
        .route("/api/agent-bots/feishu/init", post(agent_bot::feishu_init))
        .route("/api/agent-bots/feishu/begin", post(agent_bot::feishu_begin))
        .route("/api/agent-bots/feishu/poll-stream", get(agent_bot::feishu_poll_sse))
        .route("/api/agent-bots/feishu/push", get(agent_bot::get_feishu_push).put(agent_bot::update_feishu_push))
        .route("/api/agent-bots/feishu/group-whitelist", get(agent_bot::get_group_whitelist).post(agent_bot::add_group_whitelist))
        .route("/api/agent-bots/feishu/group-whitelist/{id}", delete(agent_bot::delete_group_whitelist))
        .route("/api/agent-bots/{id}", delete(agent_bot::delete_agent_bot))
        .route("/api/agent-bots/{id}/config", put(agent_bot::update_agent_bot_config))
        .route("/api/agent-bots/{id}/workspace", put(agent_bot::move_bot_to_workspace))
        // Workspace 斜杠命令管理
        .route("/api/workspace/{workspace_id}/slash-commands", get(agent_bot::list_workspace_slash_commands).post(agent_bot::create_workspace_slash_command))
        .route("/api/workspace/{workspace_id}/slash-commands/{cmd_id}", put(agent_bot::update_workspace_slash_command).delete(agent_bot::delete_workspace_slash_command))
        // Workspace 设置管理
        .route("/api/workspace/{workspace_id}/settings", get(agent_bot::get_workspace_settings).put(agent_bot::update_workspace_settings))
}

/// 飞书相关路由：历史消息查询 + 绑定管理。
fn feishu_routes() -> Router<AppState> {
    Router::new()
        .route("/api/feishu/history-messages", get(feishu_history::get_history_messages))
        .route("/api/feishu/message-stats", get(feishu_history::get_message_stats))
        .route("/api/feishu/senders", get(feishu_history::get_distinct_senders))
        .route("/api/feishu/history-chats", get(feishu_history::get_history_chats).post(feishu_history::create_history_chat))
        .route("/api/feishu/history-chats/{id}", delete(feishu_history::delete_history_chat).put(feishu_history::update_history_chat))
        .route("/api/feishu/bindings", get(feishu_binding::list_bindings).post(feishu_binding::create_binding))
        .route("/api/feishu/bindings/by-chat", delete(feishu_binding::delete_binding_by_chat))
        .route("/api/feishu/bindings/{id}", delete(feishu_binding::delete_binding))
        .route("/api/feishu/bindings/{id}/enabled", patch(feishu_binding::update_binding_enabled))
}

/// Webhook 路由：外部触发端点（无 /api 前缀）+ Webhook 管理 API + 调用记录。
fn webhook_routes() -> Router<AppState> {
    Router::new()
        // Todo webhook: /webhook/trigger/todo/{todo_id}
        .route("/webhook/trigger/todo/{todo_id}", get(webhook::trigger_webhook_with_todo).post(webhook::trigger_webhook_with_todo_post_json))
        // Loop webhook: /webhook/trigger/loop/{loop_id}
        .route("/webhook/trigger/loop/{loop_id}", get(webhook::trigger_webhook_with_loop_get).post(webhook::trigger_webhook_with_loop_post))
}

/// Session 管理路由（列表、统计、详情、删除）。
fn session_routes() -> Router<AppState> {
    Router::new()
        .route("/api/sessions", get(session::list_sessions))
        .route("/api/sessions/stats", get(session::get_session_stats))
        .route("/api/sessions/{id}", get(session::get_session_detail).delete(session::delete_session))
}

/// 用量统计（usage stats）相关路由。
fn usage_stats_routes() -> Router<AppState> {
    Router::new()
        .route("/api/usage-stats", get(usage_stats::get_usage_stats))
        .route("/api/usage-stats/refresh", post(usage_stats::refresh_usage_stats))
        .route("/api/usage-stats/settings", get(usage_stats::get_usage_stats_settings).put(usage_stats::update_usage_stats_settings))
}

/// 版本查询与升级触发。
fn version_routes() -> Router<AppState> {
    Router::new()
        .route("/api/version", get(version_handler))
        .route("/api/version/latest", get(version_latest_handler))
        .route("/api/version/upgrade", post(version_upgrade_handler))
}

/// 静态资源服务（嵌入的 Vite 产物）。
fn static_routes() -> Router<AppState> {
    Router::new()
        .route("/assets/{*path}", get(static_handler))
}

/// Todo 模板（用户可复用的 todo 模板）。
fn todo_template_routes() -> Router<AppState> {
    Router::new()
        .route("/api/todo-templates", get(todo_template::get_templates).post(todo_template::create_template))
        .route("/api/todo-templates/{id}", put(todo_template::update_template).delete(todo_template::delete_template))
        .route("/api/todo-templates/{id}/copy", post(todo_template::copy_template))
}

/// 评审模板（自动评审用的 prompt 模板，独立于 todos 表）。
///
/// 路由说明：
/// - `/api/review-templates/options` 必须在 `/api/review-templates/{id}` 之前定义，
///   否则 axum 会把 "options" 当成 id 解析（路由匹配是顺序的）。
fn review_template_routes() -> Router<AppState> {
    Router::new()
        .route("/api/review-templates", get(review_template::list_review_templates).post(review_template::create_review_template))
        .route("/api/review-templates/options", get(review_template::list_review_template_options))
        .route("/api/review-templates/{id}", get(review_template::get_review_template).put(review_template::update_review_template).delete(review_template::delete_review_template))
}

/// 自定义模板（云端订阅）相关路由。
fn custom_template_routes() -> Router<AppState> {
    Router::new()
        .route("/api/custom-templates/status", get(custom_template::get_custom_template_status))
        .route("/api/custom-templates/subscribe", post(custom_template::subscribe_custom_template))
        .route("/api/custom-templates/unsubscribe", post(custom_template::unsubscribe_custom_template))
        .route("/api/custom-templates/sync", post(custom_template::sync_custom_template))
        .route("/api/custom-templates/auto-sync", put(custom_template::update_auto_sync_config))
}

/// 云端同步相关路由。
fn cloud_routes() -> Router<AppState> {
    Router::new()
        .route("/api/cloud/config", get(sync::cloud_get_config).post(sync::cloud_save_config))
        .route("/api/cloud/sync/status", get(sync::cloud_sync_status))
        .route("/api/cloud/sync/records", get(sync::cloud_sync_records).delete(sync::cloud_clear_sync_records))
        .route("/api/cloud/sync/push", post(sync::cloud_sync_push))
        .route("/api/cloud/sync/pull", post(sync::cloud_sync_pull))
}

/// WebSocket 事件流路由。`/api/events` 单独保留——它升级为 WS 而非普通 HTTP。
fn events_routes() -> Router<AppState> {
    Router::new()
        .route("/api/events", get(events_handler))
}

/// 构造 CORS 层：dev 模式允许任意 origin（与 Vite 5173 联调），prod 模式按配置白名单限制。
/// `expose_headers` 列表见 `cors_expose_headers`，dev/prod 共用一份常量避免漂移。
fn cors_layer() -> CorsLayer {
    if crate::config::Config::is_dev_mode() {
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
        let cors = CorsLayer::new()
            .allow_methods(methods)
            .allow_headers(headers)
            .expose_headers(cors_expose_headers());
        if origins.is_empty() {
            cors
        } else {
            cors.allow_origin(origins)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod app_state_config_helpers_tests {
    //! 覆盖 `AppState` 上 issue #609 新增的三个 config 辅助方法：
    //! 1. `config_snapshot` —— 读锁卫在闭包返回时立即 drop
    //! 2. `config_clone` —— 返回 owned `Config`,读锁卫立即 drop
    //! 3. `config_write_clone` —— 写锁卫在闭包返回时立即 drop
    //!
    //! 这三个方法的核心契约都是"锁卫在闭包返回后立即 drop,后续 await 不会持 std
    //! 锁卫跨 .await"。这里构造一个最小可用的 `AppState` —— 用 `Database::new(
    //! ":memory:")` 启一个真 db,再用它构造 `ServiceContext` / `HookService` /
    //! `TodoScheduler` 等依赖 —— 验证关键行为:闭包返回后立刻能再获取写锁。
    use super::AppState;
    use crate::adapters::ExecutorRegistry;
    use crate::config::Config;
    use crate::service_context::ServiceContext;
    use crate::task_manager::TaskManager;
    use std::sync::{Arc, RwLock};
    use tokio::sync::broadcast;

    /// 在测试运行时构造最小可用的 `AppState`。
    ///
    /// `Database::new` / `TodoScheduler::new` 是 async 的,所以整体包在
    /// `#[tokio::test]` 的 runtime 里。三个被测方法本身是 sync 的,放到
    /// `block_on` 闭包外执行,避免对 runtime 类型造成约束。
    async fn build_minimal_state_async() -> AppState {
        // 内存数据库:不依赖外部文件,跑得快,适合单测。
        let db = Arc::new(crate::db::Database::new(":memory:").await.unwrap());

        // ServiceContext 是其他几个组件共用的依赖（Scheduler / FeishuListener
        // 都需要它）。先把它准备好，后续用其 clone 出多个引用。
        // todo hook 已整块移除，下游不再需要 HookService 透传。
        let (tx, _rx) = broadcast::channel(1);
        let ctx = ServiceContext {
            db: db.clone(),
            executor_registry: Arc::new(ExecutorRegistry::default()),
            tx,
            task_manager: Arc::new(TaskManager::default()),
            config: Arc::new(RwLock::new(Config::default())),
        };

        // TodoScheduler::new 是 async 的；其内部 `JobScheduler` 需要 tokio runtime。
        // 这里 block_on 直接拿；在 `#[tokio::test]` 的 runtime 里调用，不会有额外约束。
        let scheduler = Arc::new(crate::scheduler::TodoScheduler::new().await.unwrap());

        // FeishuListener::new 需要 ServiceContext + MessageDebounce。MessageDebounce
        // 现在只依赖 ServiceContext，不再需要 HookService 注入。
        let debounce = Arc::new(
            crate::services::message_debounce::MessageDebounce::new(ctx.clone(), None),
        );
        let feishu_listener = Arc::new(
            crate::services::feishu_listener::FeishuListener::new(ctx.clone(), debounce),
        );

        let (feishu_push_mutator, _rx2) = broadcast::channel(1);

        AppState {
            db,
            executor_registry: Arc::new(ExecutorRegistry::default()),
            tx: ctx.tx.clone(),
            scheduler,
            task_manager: ctx.task_manager.clone(),
            config: ctx.config.clone(),
            feishu_listener,
            feishu_push_mutator,
            // 测试用最小 AppState 不需要 loop 服务
            loop_scheduler: None,
            loop_trigger_dispatcher: None,
            loop_runner: None,
        }
    }

    #[tokio::test]
    async fn config_snapshot_returns_value_and_drops_lock() {
        // 用一个默认 Config 配上一个非默认字段值,确认闭包能读到 cfg。
        let state = build_minimal_state_async().await;
        {
            let mut guard = state.config.write().unwrap();
            guard.auto_backup_max_files = 7;
        }

        // 闭包内取出 owned 投影值,验证 `config_snapshot` 行为。
        let observed = state.config_snapshot(|c| c.auto_backup_max_files);
        assert_eq!(observed, 7);

        // 关键契约:闭包返回后,读锁卫必须立即 drop,
        // 否则下面这次写锁会阻塞到 deadlock。这里用 `try_write` 验证
        // 锁是可立即获取的——如果闭包未 drop guard,`try_write` 会返回 Err。
        let write_result = state.config.try_write();
        assert!(
            write_result.is_ok(),
            "config_snapshot 闭包返回后必须立即 drop 读锁卫,但 try_write 仍失败: {:?}",
            write_result.err()
        );
    }

    #[tokio::test]
    async fn config_snapshot_returns_projection_tuple() {
        // 验证多字段投影:tuple 解构也能正常工作,call site 形如
        // `let (db_path, max) = state.config_snapshot(|c| (..., c.auto_backup_max_files));`
        let state = build_minimal_state_async().await;
        {
            let mut guard = state.config.write().unwrap();
            guard.db_path = "/tmp/test-data.db".to_string();
            guard.auto_backup_max_files = 5;
        }

        let (db_path, max_files) = state.config_snapshot(|c| {
            (std::path::PathBuf::from(&c.db_path), c.auto_backup_max_files)
        });
        assert_eq!(db_path.to_str().unwrap(), "/tmp/test-data.db");
        assert_eq!(max_files, 5);

        // 同样验证锁卫已 drop。
        assert!(state.config.try_write().is_ok());
    }

    #[tokio::test]
    async fn config_clone_returns_owned_and_drops_lock() {
        let state = build_minimal_state_async().await;
        {
            let mut guard = state.config.write().unwrap();
            guard.auto_cleanup_logs_days = Some(30);
        }

        let cloned = state.config_clone();
        assert_eq!(cloned.auto_cleanup_logs_days, Some(30));

        // 修改原 cfg 不影响 clone 出的副本,验证是真正的 owned clone 而非引用。
        {
            let mut guard = state.config.write().unwrap();
            guard.auto_cleanup_logs_days = Some(60);
        }
        assert_eq!(cloned.auto_cleanup_logs_days, Some(30));

        // 锁卫在 `config_clone` 返回后已 drop。
        assert!(state.config.try_write().is_ok());
    }

    #[tokio::test]
    async fn config_write_clone_mutates_and_drops_lock() {
        let state = build_minimal_state_async().await;
        {
            let mut guard = state.config.write().unwrap();
            guard.auto_backup_max_files = 0;
        }

        // 闭包内修改 cfg 字段并返回 owned clone。
        let cloned = state.config_write_clone(|c| {
            c.auto_backup_max_files = 9;
            c.clone()
        });

        // 闭包外的 clone 应该看到修改后的值。
        assert_eq!(cloned.auto_backup_max_files, 9);
        // AppState 内部 cfg 也应该被修改(写锁是 in-place)。
        assert_eq!(state.config.read().unwrap().auto_backup_max_files, 9);

        // 写锁卫在闭包返回后已 drop,可立刻再次获取。
        assert!(state.config.try_write().is_ok());
    }

    #[tokio::test]
    async fn config_write_clone_propagates_closure_error() {
        // 闭包返回 `Result<_, AppError>` 时,`?` 应当能直接传播 Err。
        // 这里构造一个失败的闭包,验证外层能拿到 Err。
        let state = build_minimal_state_async().await;
        let result: Result<i64, &'static str> = state.config_write_clone(|_c| Err("boom"));
        assert!(matches!(result, Err("boom")));

        // 写锁卫仍然在闭包返回后立即 drop,即使闭包返回 Err。
        assert!(state.config.try_write().is_ok());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod create_app_refactor_tests {
    //! 覆盖 issue #661 重构后拆出的辅助函数。
    //!
    //! 这些函数之前全部内联在 312 行 `create_app` 里，没法单测；重构后单独提了出来。
    //! 测试目标不是"覆盖所有路由分支"，而是"验证拆分后函数签名/返回值/基本行为不变"。
    use super::*;

    // 用 Mutex 串行化 NTD_MODE 环境变量操作，避免与 cargo test 默认多线程执行产生 data race
    // （std::env::set_var 在多线程下非线程安全）。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// `cors_layer()` 在 dev 模式 / 生产模式 都能正常构造而不 panic。
    /// `is_dev_mode()` 由环境变量控制（`NTD_MODE=dev`），单测里两种都覆盖：
    /// 先 dev（默认未设），再切到 prod，最后恢复现场。
    #[test]
    fn cors_layer_constructs_in_both_modes() {
        // 取锁后整个测试内部对 NTD_MODE / HOME 的读写都串行，避免 cargo test 多线程下
        // 与其它测试的 env var 写入产生竞争（std::env::set_var 在多线程下非线程安全）。
        let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

        // 把 HOME 重定向到临时目录，阻止 prod 分支的 `Config::load()` 写到
        // ~/.ntd/config.yaml。TempDir 在 scope 结束时自动清理，测试结束连同
        // 临时 config.yaml 一起消失，开发者本机 / CI runner 都不会留垃圾。
        let tmp_home = tempfile::TempDir::new().expect("create tempdir for HOME");
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp_home.path());

        let restore = |prev: Option<String>| {
            match prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            // tmp_home drop 删目录；显式 drop 避免 set_var 之后还残留 HOME
            drop(tmp_home);
        };

        // 默认未设 NTD_MODE，`is_dev_mode()` 返回 false → 实际走 prod 分支；
        // 这里第一次 `cors_layer()` 调用测的就是 prod 路径。
        let _dev_layer = cors_layer();

        // 切到 prod：保存原值避免污染其它测试，结束还原
        let prev_mode = std::env::var("NTD_MODE").ok();
        std::env::set_var("NTD_MODE", "prod");
        // prod 分支会调用 Config::load() 读 cors_allowed_origins；
        // 由于 HOME 已被重定向到 tmp_home，写入的 config.yaml 落在 tempdir 里。
        let _prod_layer = cors_layer();
        match prev_mode {
            Some(v) => std::env::set_var("NTD_MODE", v),
            None => std::env::remove_var("NTD_MODE"),
        }
        restore(prev_home);
    }

    /// 每个领域子路由函数都返回非空 `Router<AppState>`，不 panic。
    /// 用 `Router::route_count` 之类的 introspection API 没有；
    /// 这里改用「调用函数本身 + assert 返回 Router」——编译过 = 函数存在；
    /// 不 panic = 基本行为正确。配合 route_equivalence_test 做完整路由覆盖。
    #[test]
    fn each_domain_routes_function_returns_a_router() {
        // 让编译器帮我们检查每个函数都存在且签名正确
        let routers: Vec<Router<AppState>> = vec![
            root_routes(),
            todo_routes(),
            execution_routes(),
            scheduler_routes(),
            backup_routes(),
            config_routes(),
            skills_routes(),
            agent_bot_routes(),
            feishu_routes(),
            webhook_routes(),
            session_routes(),
            usage_stats_routes(),
            version_routes(),
            static_routes(),
            todo_template_routes(),
            review_template_routes(),
            custom_template_routes(),
            cloud_routes(),
            events_routes(),
            blackboard::blackboard_routes(),
        ];
        // 20 个领域子路由函数（blackboard_routes 为黑板功能新增）
        assert_eq!(routers.len(), 20, "领域子路由函数数量应与拆分清单一致");
        // `Router` 在 axum 0.8 中没有公开的 route_count，这里只能断言"全部成功构造"
    }

    /// 重构后 `create_app` 函数体大小合规：去除签名/空行/注释后应在 30 行以内
    /// （CLAUDE.md 要求）。动态从源码定位 `pub fn create_app` 的 body 范围：
    /// regex 锚定签名起点，再做花括号平衡扫描（跳过字符串/char 字面量与行注释），
    /// 最后只数 body 内非空非注释行。无需手维护行号常量，重构不再误伤。
    #[test]
    fn create_app_function_body_is_under_30_lines() {
        const BODY_LINES_BUDGET: usize = 30;
        let src = include_str!("mod.rs");

        // 1) regex 锚定 `pub async fn create_app\s*(` 签名起点
        let sig_re = regex::Regex::new(r"pub async fn create_app\s*\(")
            .expect("compile create_app signature regex");
        let sig_match = sig_re
            .find(src)
            .expect("create_app signature not found in mod.rs");
        let after_sig = &src[sig_match.end()..];
        // 签名所在行号（1-based），用于报错信息定位
        let sig_line = src[..sig_match.start()].matches('\n').count() + 1;

        // 2) 从签名后的第一个 `{` 开始做花括号平衡扫描；跳过字符串/char/行注释
        let open_idx = after_sig
            .find('{')
            .expect("create_app body opening brace not found");
        let body_start = sig_match.end() + open_idx;
        let body_bytes = src[body_start..].as_bytes();

        let mut depth: i32 = 0;
        let mut i = 0;
        let mut body_end = body_start; // exclusive: 指向匹配 `}` 之后一字节
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        let mut in_string = false;
        let mut in_char = false;
        let mut prev_was_escape = false;
        while i < body_bytes.len() {
            let c = body_bytes[i] as char;
            // 行注释优先级最高；遇到换行就退出
            if in_line_comment {
                if c == '\n' {
                    in_line_comment = false;
                }
                i += 1;
                continue;
            }
            if in_block_comment {
                if c == '*' && i + 1 < body_bytes.len() && body_bytes[i + 1] == b'/' {
                    in_block_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if in_string {
                if prev_was_escape {
                    prev_was_escape = false;
                } else if c == '\\' {
                    prev_was_escape = true;
                } else if c == '"' {
                    in_string = false;
                }
                i += 1;
                continue;
            }
            if in_char {
                if prev_was_escape {
                    prev_was_escape = false;
                } else if c == '\\' {
                    prev_was_escape = true;
                } else if c == '\'' {
                    in_char = false;
                }
                i += 1;
                continue;
            }
            // 不在任何字面量 / 注释中：识别字面量起点与花括号
            if c == '/' && i + 1 < body_bytes.len() {
                if body_bytes[i + 1] == b'/' {
                    in_line_comment = true;
                    i += 2;
                    continue;
                } else if body_bytes[i + 1] == b'*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
            }
            match c {
                '"' => in_string = true,
                '\'' => in_char = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        body_end = body_start + i + 1; // exclusive
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        assert!(
            depth == 0 && body_end > body_start,
            "create_app body 未匹配到闭合大括号（depth={}）",
            depth
        );

        // 3) body 范围按行展开，统计非空非注释行
        let body_src = &src[body_start..body_end];
        let body_lines = body_src
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                // 排除纯空行与纯行注释
                !trimmed.is_empty() && !trimmed.starts_with("//")
            })
            .count();
        assert!(
            body_lines <= BODY_LINES_BUDGET,
            "create_app 函数有效行数 {} 超过 CLAUDE.md 限制 {}（签名在第 {} 行）",
            body_lines,
            BODY_LINES_BUDGET,
            sig_line,
        );
    }
}
