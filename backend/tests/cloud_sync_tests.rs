// 回归测试：cloud 同步 handler 不能因为持读锁再申请写锁而自我死锁。
//
// 之前 cloud_sync_push / cloud_sync_pull 把 config 读锁一路持有到函数末尾，
// 随后在同一任务里调 `state.config.write().await`。tokio::sync::RwLock
// 不允许同一任务在持读锁时再等写锁 —— 整个 future 永远不被唤醒，
// 于是本端 HTTP 响应永远不返回（云端早处理完）。
//
// 修法：把所有要从 config 读的值一次性拷出来，立刻释放读锁，再去打云端。
// 这里用一个最朴素的 mock HTTP server（tokio TcpListener）验证：
// 1) push 在合理时间内返回，不会自我死锁；
// 2) 拿到的 last_sync_at 在成功 push 后被写入。
// 测试代码允许 unwrap/expect/panic 等写法以简化断言逻辑，统一放宽以下 clippy 检查
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use tokio::sync::broadcast;
use tower::ServiceExt;

use ntd::{
    adapters::{ExecutorRegistry, claude_code::ClaudeCodeExecutor},
    config::{CloudSyncConfig, Config},
    db::Database,
    handlers::create_app,
    scheduler::TodoScheduler,
    service_context::ServiceContext,
    task_manager::TaskManager,
};

/// 起一个最小 HTTP mock：任意 POST 都回 `success: true` 的 YAML。
/// 返回实际监听的 URL（`http://127.0.0.1:<port>`）。
async fn spawn_mock_cloud() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => break,
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                // 读完请求（不解析，直接吞掉）。
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let body = "success: true\nmerged_data: |\n  version: '1.0'\n  todos: []\n  tags: []\n  skills: []\n";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/yaml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    format!("http://{}", addr)
}

async fn build_test_app() -> (axum::Router, Arc<std::sync::RwLock<Config>>) {
    let db = Arc::new(Database::new(":memory:").await.unwrap());
    let executor_registry = Arc::new(ExecutorRegistry::new());
    executor_registry
        .register(ClaudeCodeExecutor::new("claude".to_string()))
        .await;
    let (tx, _rx) = broadcast::channel(100);
    let task_manager = Arc::new(TaskManager::new());

    let config = Arc::new(std::sync::RwLock::new(Config::default()));
    let scheduler = Arc::new(TodoScheduler::new().await.unwrap());
    let ctx = ServiceContext {
        db: db.clone(),
        executor_registry: executor_registry.clone(),
        tx: tx.clone(),
        task_manager: task_manager.clone(),
        config: config.clone(),
        expert_manager: Arc::new(ntd::expert::ExpertIndexManager::new()),
    };
    scheduler.load_from_db(&ctx).await.unwrap();
    scheduler.start().await.unwrap();
    (create_app(ctx, scheduler).await, config)
}

async fn read_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// 关键回归测试：把 mock 云端地址配进 config，调一次 push 必须在合理时间内返回。
/// 修改前：会无限期挂起（读锁 + 同任务写锁 = tokio RwLock 自我死锁）。
/// 修改后：正常返回，云端响应解析后 `success=true`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cloud_sync_push_does_not_deadlock_with_config_write() {
    let (app, config) = build_test_app().await;
    let cloud_url = spawn_mock_cloud().await;

    {
        let mut cfg = config.write().unwrap();
        cfg.cloud_sync = CloudSyncConfig {
            server_url: cloud_url,
            sync_token: Some("ntd_test_token".to_string()),
            last_sync_at: None,
            default_conflict_mode: "overwrite".to_string(),
        };
    }

    let req = Request::builder()
        .method("POST")
        // v1: cloud sync 也加 v1 前缀（路径与 handlers/action.rs::v1_routes 对齐）
        .uri("/api/v1/cloud/sync/push?conflict_mode=overwrite&dry_run=false")
        .body(Body::empty())
        .unwrap();

    // 关键：给个 10s 总超时。如果读锁 + 写锁自我死锁，这一行永远不返回。
    let response = tokio::time::timeout(Duration::from_secs(10), app.oneshot(req))
        .await
        .expect("cloud push 请求挂起 — 怀疑 config 读/写锁自我死锁回归")
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["code"], 0, "顶层 code 应为 0: {:?}", body);
    let data = &body["data"];
    assert_eq!(data["success"], true, "push 业务应成功: {:?}", data);
    assert_eq!(data["direction"], "push");

    // 成功 push 之后，last_sync_at 应被写回 config。
    let cfg = config.read().unwrap();
    assert!(
        cfg.cloud_sync.last_sync_at.is_some(),
        "成功 push 之后 last_sync_at 必须被更新"
    );
}

/// 同样覆盖 pull 路径，避免在 push 修好后 pull 又踩同一个坑。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cloud_sync_pull_does_not_deadlock_with_config_write() {
    let (app, config) = build_test_app().await;

    // pull 调的是 POST /api/v1/cloud/sync/pull，mock 对所有请求都回 success: true。
    let cloud_url = spawn_mock_cloud().await;
    {
        let mut cfg = config.write().unwrap();
        cfg.cloud_sync = CloudSyncConfig {
            server_url: cloud_url,
            sync_token: Some("ntd_test_token".to_string()),
            last_sync_at: None,
            default_conflict_mode: "overwrite".to_string(),
        };
    }

    let req = Request::builder()
        .method("POST") // 路由是 POST（见 handlers/action.rs::v1_routes）
        .uri("/api/v1/cloud/sync/pull?conflict_mode=overwrite&dry_run=false")
        .body(Body::empty())
        .unwrap();

    let response = tokio::time::timeout(Duration::from_secs(10), app.oneshot(req))
        .await
        .expect("cloud pull 请求挂起 — 怀疑 config 读/写锁自我死锁回归")
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["code"], 0, "顶层 code 应为 0: {:?}", body);
    let data = &body["data"];
    assert_eq!(data["direction"], "pull");
}

/// token 没配时，handler 应直接返回 400 BadRequest，绝不会去申请写锁、也不会
/// 卡在读锁里。顺便覆盖「缺 token」分支，避免有人误把读锁范围扩大。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cloud_sync_push_missing_token_returns_bad_request() {
    let (app, config) = build_test_app().await;
    {
        let mut cfg = config.write().unwrap();
        cfg.cloud_sync = CloudSyncConfig {
            server_url: "http://127.0.0.1:1".to_string(),
            sync_token: None, // 关键：没配 token
            last_sync_at: None,
            default_conflict_mode: "overwrite".to_string(),
        };
    }

    let req = Request::builder()
        .method("POST")
        // v1: 同上，cloud sync 加 v1 前缀
        .uri("/api/v1/cloud/sync/push")
        .body(Body::empty())
        .unwrap();

    let response = tokio::time::timeout(Duration::from_secs(5), app.oneshot(req))
        .await
        .expect("缺 token 的 push 也应快速返回")
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
