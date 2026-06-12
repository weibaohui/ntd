//! End-to-end test for the `track_response_done` middleware + `ResponseDone` extractor.
//!
//! 验证信号链路通畅:handler 同步返回响应后,后台线程上的
//! `blocking_recv()` 能立刻返回。如果链路有 bug (中间件没挂,
//! extractor 找不到 receiver,或者 drop(tx) 时机不对),这个测试会挂起
//! 或 panic。
//!
//! 同时验证:
//! - 中间件装上后,正常请求能正常返回 (不会因为 ResponseDone 不在
//!   handler 里就 panic)
//! - 中间件不会破坏 response body 内容

use std::time::{Duration, Instant};

use axum::{
    body::Body, http::Request, middleware::from_fn, response::IntoResponse, routing::get, Router,
};
use http_body_util::BodyExt;
use ntd::handlers::{track_response_done, ResponseDone};
use tower::ServiceExt;

const SLOW_RECV_TIMEOUT: Duration = Duration::from_secs(2);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_response_done_signal_fires_after_handler_returns() {
    // 这个 handler 模拟真实场景:同步返回响应,但起后台线程等信号
    // 再做"会杀当前进程"的事(这里用 sleep 代替)
    async fn handler(ResponseDone(rx): ResponseDone) -> impl IntoResponse {
        std::thread::spawn(move || {
            // 等信号到了再做"危险操作"。这是 version_upgrade_handler 的核心模式。
            let start = Instant::now();
            let _ = rx.blocking_recv();
            let elapsed = start.elapsed();
            // 信号应该 ~ms 级别到达(只等 axum 构造完 response 就触发),
            // 不可能 1 秒还不来。
            assert!(
                elapsed < SLOW_RECV_TIMEOUT,
                "signal took {elapsed:?} to arrive, expected < {SLOW_RECV_TIMEOUT:?}"
            );
        });
        axum::Json(serde_json::json!({"status": "ok"}))
    }

    let app = Router::new()
        .route("/test", get(handler))
        .layer(from_fn(track_response_done));

    let req = Request::builder()
        .uri("/test")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("request failed");

    assert_eq!(resp.status(), 200);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["status"], "ok");

    // 给后台线程一点时间 panic 出来(它已经 assert 了 elapsed)
    tokio::time::sleep(Duration::from_millis(200)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_middleware_does_not_break_handlers_without_response_done() {
    // 验证中间件对"不消费 ResponseDone"的 handler 也无害:
    // oneshot 没接收端 → tx drop 时 receiver 不存在 → 直接被回收
    // → 不会有 panic,不会有泄漏
    async fn plain_handler() -> impl IntoResponse {
        "hello"
    }

    let app = Router::new()
        .route("/plain", get(plain_handler))
        .layer(from_fn(track_response_done));

    let req = Request::builder()
        .uri("/plain")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("plain request failed");
    assert_eq!(resp.status(), 200);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body_bytes[..], b"hello");
}
