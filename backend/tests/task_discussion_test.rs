// 任务讨论区（需求 060）集成测试：覆盖 #1 分页/树组装、#2 删帖路径的 API 端到端。
// 不依赖真实执行器：测试均发「不含 @ 的人帖」，避开 @ 触发执行的不确定路径；
// @消歧由 handlers::task_posts 的 classify_mention 单测覆盖，删 running 帖联动取消
// 复用 stop_execution 同源内核，逻辑等价，此处不强依赖真实执行起停。

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tower::ServiceExt;

use ntd::{
    adapters::{claude_code::ClaudeCodeExecutor, ExecutorRegistry},
    config::Config,
    db::Database,
    handlers::create_app,
    scheduler::TodoScheduler,
    task_manager::TaskManager,
};

/// 构造测试 app + workspace + db。db 一并返回，供测试直接建 task（避开 task 创建 API 细节）。
async fn create_discussion_app() -> (axum::Router, i64, Arc<Database>) {
    let db = Arc::new(Database::new(":memory:").await.unwrap());
    // handler 要求 workspace_id 对应已存在目录，先建一个测试目录。
    let ws_id = db
        .create_project_directory("/tmp/test-discussion-ws", Some("test"), false, false)
        .await
        .unwrap();
    let executor_registry = Arc::new(ExecutorRegistry::new());
    executor_registry
        .register(ClaudeCodeExecutor::new("claude".to_string()))
        .await;
    let (tx, _rx) = broadcast::channel(100);
    let task_manager = Arc::new(TaskManager::new());
    let config = Arc::new(std::sync::RwLock::new(Config::default()));
    let scheduler = Arc::new(TodoScheduler::new().await.unwrap());
    let ctx = ntd::service_context::ServiceContext {
        db: db.clone(),
        executor_registry: executor_registry.clone(),
        tx: tx.clone(),
        task_manager: task_manager.clone(),
        config: config.clone(),
        expert_manager: Arc::new(ntd::expert::ExpertIndexManager::new()),
    blackboard_debouncer: ntd::services::blackboard_debouncer::BlackboardDebouncer::new(),
    };
    scheduler.load_from_db(&ctx).await.unwrap();
    scheduler.start().await.unwrap();
    (create_app(ctx, scheduler).await, ws_id, db)
}

/// 读取响应体并反序列化（集成测试统一入口，失败直接 panic 暴露问题）。
async fn read_json_body<T: serde::de::DeserializeOwned>(response: axum::response::Response) -> T {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// 构造 JSON body 的 POST/PUT 请求。
fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// 讨论帖集合路由（列表/创建）。
fn posts_uri(ws_id: i64, task_id: i64) -> String {
    format!("/api/v1/workspaces/{}/tasks/{}/posts", ws_id, task_id)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap()
}

fn delete(uri: &str) -> Request<Body> {
    Request::builder().method("DELETE").uri(uri).body(Body::empty()).unwrap()
}

// 发一条纯人帖（无 @）→ 列表主楼层分页命中（#1 分页 + 树组装）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_human_post_and_paged_list() {
    let (app, ws_id, db) = create_discussion_app().await;
    let task_id = db.create_task("讨论任务", ws_id, 0, None).await.unwrap().id;

    let req = json_request("POST", &posts_uri(ws_id, task_id), json!({"content": "你好讨论"}));
    let body: Value = read_json_body(app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(body["code"], 0);
    // 纯人帖：无 @ 不触发执行，agent_post 为 null。
    assert_eq!(body["data"]["human_post"]["content"], "你好讨论");
    assert!(body["data"]["agent_post"].is_null());

    // 列表分页：total=1，items 含该主楼层（带空 replies 数组）。
    let body: Value = read_json_body(
        app.clone()
            .oneshot(get(&format!("{}?page=1&limit=20", posts_uri(ws_id, task_id))))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["content"], "你好讨论");
    assert!(body["data"]["items"][0]["replies"].is_array());
}

// 楼中楼回复挂在主楼层下（深度≤1）→ 列表 items[0].replies 命中（#1 树组装）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_reply_forms_nested_tree() {
    let (app, ws_id, db) = create_discussion_app().await;
    let task_id = db.create_task("T", ws_id, 0, None).await.unwrap().id;

    // 主楼层。
    let body: Value = read_json_body(
        app.clone()
            .oneshot(json_request("POST", &posts_uri(ws_id, task_id), json!({"content": "主帖"})))
            .await
            .unwrap(),
    )
    .await;
    let main_id = body["data"]["human_post"]["id"].as_i64().unwrap();
    // 楼中楼回复主楼层。
    let _body: Value = read_json_body(
        app.clone()
            .oneshot(json_request(
                "POST",
                &posts_uri(ws_id, task_id),
                json!({"content": "回复主帖", "parent_post_id": main_id}),
            ))
            .await
            .unwrap(),
    )
    .await;

    let list: Value = read_json_body(app.clone().oneshot(get(&posts_uri(ws_id, task_id))).await.unwrap()).await;
    let items = list["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "仅 1 个主楼层");
    let replies = items[0]["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 1, "主楼层挂 1 条楼中楼");
    assert_eq!(replies[0]["content"], "回复主帖");
}

// 删帖后列表不再含（#2 删帖路径；running 联动由同源取消内核保证，集成层不强依赖真实执行）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_post_removes_from_list() {
    let (app, ws_id, db) = create_discussion_app().await;
    let task_id = db.create_task("T2", ws_id, 0, None).await.unwrap().id;

    let body: Value = read_json_body(
        app.clone()
            .oneshot(json_request("POST", &posts_uri(ws_id, task_id), json!({"content": "待删"})))
            .await
            .unwrap(),
    )
    .await;
    let pid = body["data"]["human_post"]["id"].as_i64().unwrap();

    let resp = app
        .clone()
        .oneshot(delete(&format!("{}/{}", posts_uri(ws_id, task_id), pid)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let list: Value = read_json_body(app.clone().oneshot(get(&posts_uri(ws_id, task_id))).await.unwrap()).await;
    assert_eq!(list["data"]["total"], 0, "删后主楼层总数归零");
}

// 删主楼层 → 其楼中楼被 CASCADE 一并删除（get 单帖 404）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_main_cascades_replies() {
    let (app, ws_id, db) = create_discussion_app().await;
    let task_id = db.create_task("T3", ws_id, 0, None).await.unwrap().id;

    let body: Value = read_json_body(
        app.clone()
            .oneshot(json_request("POST", &posts_uri(ws_id, task_id), json!({"content": "主"})))
            .await
            .unwrap(),
    )
    .await;
    let main_id = body["data"]["human_post"]["id"].as_i64().unwrap();
    let reply_body: Value = read_json_body(
        app.clone()
            .oneshot(json_request(
                "POST",
                &posts_uri(ws_id, task_id),
                json!({"content": "楼中楼", "parent_post_id": main_id}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let reply_id = reply_body["data"]["human_post"]["id"].as_i64().unwrap();

    // 删主楼层（楼中楼由 v88 的 ON DELETE CASCADE 连带删除）。
    let _ = app
        .clone()
        .oneshot(delete(&format!("{}/{}", posts_uri(ws_id, task_id), main_id)))
        .await
        .unwrap();

    // 楼中楼随 CASCADE 消失：get 单帖返回 404。
    let resp = app
        .clone()
        .oneshot(get(&format!("{}/{}", posts_uri(ws_id, task_id), reply_id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
