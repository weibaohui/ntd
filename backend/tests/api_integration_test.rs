use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::json;
use tokio::sync::broadcast;
use tower::ServiceExt;

use ntd::{
    adapters::{ExecutorRegistry, claude_code::ClaudeCodeExecutor},
    config::Config,
    db::Database,
    handlers::create_app,
    scheduler::TodoScheduler,
    task_manager::TaskManager,
};

async fn create_test_app() -> axum::Router {
    // 保留原签名返回 Router，供不需要直接访问 scheduler 的测试使用。
    create_test_app_with_scheduler().await.0
}

// 返回 (Router, Scheduler, workspace_id) 三元组。
// - Scheduler：让需要直接访问 scheduler 内存状态（验证 cron 是否同步）的测试可拿到句柄。
// - workspace_id：create_todo handler 现在要求 workspace_id 必填且必须存在，
//   这里预置一个工作空间供测试复用，避免每个测试重复创建。
async fn create_test_app_with_scheduler() -> (axum::Router, Arc<TodoScheduler>, i64) {
    let db = Arc::new(Database::new(":memory:").await.unwrap());

    let executor_registry = Arc::new(ExecutorRegistry::new());
    executor_registry.register(ClaudeCodeExecutor::new("claude".to_string())).await;

    let (tx, _rx) = broadcast::channel(100);
    let task_manager = Arc::new(TaskManager::new());

    let config = Arc::new(std::sync::RwLock::new(Config::default()));
    let scheduler = Arc::new(TodoScheduler::new().await.unwrap());
    // 预置工作空间：create_todo handler 强制校验 workspace_id 存在性，
    // 测试不预置会导致所有创建 todo 的请求 400。
    let dir = db
        .get_or_create_project_directory("/tmp/ntd-test-ws", Some("测试空间"))
        .await
        .unwrap();
    let ctx = ntd::service_context::ServiceContext {
        db: db.clone(),
        executor_registry: executor_registry.clone(),
        tx: tx.clone(),
        task_manager: task_manager.clone(),
        config: config.clone(),
    };
    scheduler
        .load_from_db(&ctx)
        .await
        .unwrap();
    scheduler.start().await.unwrap();

    let app = create_app(ctx, scheduler.clone()).await;
    (app, scheduler, dir.id)
}

async fn read_json_body<T: serde::de::DeserializeOwned>(
    response: axum::response::Response,
) -> T {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn json_request(method: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

// ===== Todo handlers =====

// All integration tests use multi_thread runtime with 2 workers to avoid panics
// and deadlocks. Why:
// - Single-thread runtime panics when code calls `block_in_place` (which SeaORM,
//   tower::ServiceExt::oneshot, and other async infrastructure may do internally).
// - Multi-thread runtime provides consistent task scheduling and reduces risk of
//   deadlock when tests spawn concurrent tasks or hold locks across await points.
// - 2 workers is sufficient for test workload and keeps resource usage low.
// Scope: all `#[tokio::test]` in this file use this configuration for consistency
// and to prevent intermittent test failures caused by runtime mismatches.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_todos() {
    let app = create_test_app().await;

    // Create a todo first
    let req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Do this", "tag_ids": []}));
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let req = Request::builder()
        .uri("/api/todos")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0);
    let todos = body["data"].as_array().unwrap();
    // Database::new 在 :memory: db 上会自动 seed 评审任务(todo_type=1)。
    // 这里只验证我们刚创建的 "Test" todo 出现在列表里,
    // 不去数总数(seed 数据是基础设施的一部分,不是用户数据)。
    let our_todo = todos
        .iter()
        .find(|t| t["title"] == "Test")
        .expect("newly created 'Test' todo should appear in GET /api/todos");
    assert_eq!(our_todo["prompt"], "Do this");
    assert_eq!(our_todo["status"], "pending");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_todo_success() {
    let app = create_test_app().await;

    let req = json_request("POST", "/api/todos", json!({"title": "New Todo", "prompt": "Prompt text", "tag_ids": []}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["title"], "New Todo");
    assert_eq!(body["data"]["prompt"], "Prompt text");
    assert_eq!(body["data"]["status"], "pending");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_todo_empty_title() {
    let app = create_test_app().await;

    let req = json_request("POST", "/api/todos", json!({"title": "", "prompt": "Prompt", "tag_ids": []}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 40002);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_todo_prompt_fallback() {
    let app = create_test_app().await;

    let req = json_request("POST", "/api/todos", json!({"title": "Fallback Title", "prompt": "", "tag_ids": []}));
    let response = app.oneshot(req).await.unwrap();

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["prompt"], "Fallback Title");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_todo_with_tags() {
    let app = create_test_app().await;

    // Create a tag first
    let tag_req = json_request("POST", "/api/tags", json!({"name": "urgent", "color": "#ff0000"}));
    let tag_resp = app.clone().oneshot(tag_req).await.unwrap();
    let tag_body: serde_json::Value = read_json_body(tag_resp).await;
    let tag_id = tag_body["data"]["id"].as_i64().unwrap();

    let req = json_request("POST", "/api/todos", json!({"title": "Tagged", "prompt": "Do this", "tag_ids": [tag_id]}));
    let response = app.oneshot(req).await.unwrap();

    let body: serde_json::Value = read_json_body(response).await;
    let tag_ids = body["data"]["tag_ids"].as_array().unwrap();
    assert_eq!(tag_ids.len(), 1);
    assert_eq!(tag_ids[0], tag_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_update_todo_success() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Old", "prompt": "Old prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let id = create_body["data"]["id"].as_i64().unwrap();

    let req = json_request("PUT", &format!("/api/todos/{}", id), json!({"title": "Updated", "prompt": "Updated prompt", "status": "in_progress"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["title"], "Updated");
    assert_eq!(body["data"]["status"], "in_progress");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_update_todo_prompt_fallback() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Title", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let id = create_body["data"]["id"].as_i64().unwrap();

    let req = json_request("PUT", &format!("/api/todos/{}", id), json!({"title": "New Title", "prompt": "", "status": "pending"}));
    let response = app.oneshot(req).await.unwrap();

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["prompt"], "New Title");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_update_todo_tags() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let todo_id = create_body["data"]["id"].as_i64().unwrap();

    let tag_req = json_request("POST", "/api/tags", json!({"name": "urgent", "color": "#ff0000"}));
    let tag_resp = app.clone().oneshot(tag_req).await.unwrap();
    let tag_body: serde_json::Value = read_json_body(tag_resp).await;
    let tag_id = tag_body["data"]["id"].as_i64().unwrap();

    let req = json_request("PUT", &format!("/api/todos/{}/tags", todo_id), json!({"tag_ids": [tag_id]}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_todo() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "To Delete", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let id = create_body["data"]["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/todos/{}", id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Verify it's gone
    let get_req = Request::builder()
        .uri("/api/todos")
        .body(Body::empty())
        .unwrap();
    let get_resp = create_test_app().await.oneshot(get_req).await.unwrap();
    let get_body: serde_json::Value = read_json_body(get_resp).await;
    let todos = get_body["data"].as_array().unwrap();
    assert!(todos.iter().all(|t| t["id"].as_i64().unwrap() != id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_todo_not_found() {
    let app = create_test_app().await;

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/todos/9999")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    // Attempting to delete a non-existent todo returns an error
    // because the database update affects 0 rows, which sea_orm may treat as an error
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_force_update_status() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let id = create_body["data"]["id"].as_i64().unwrap();

    let req = json_request("PUT", &format!("/api/todos/{}/force-status", id), json!({"title": "Test", "prompt": "Prompt", "status": "completed"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["status"], "completed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_todo_not_found() {
    let app = create_test_app().await;

    let req = json_request("PUT", "/api/todos/9999", json!({"title": "Test", "prompt": "Prompt", "status": "pending"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ===== Step handlers =====
//
// ===== Tag handlers =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_tags() {
    let app = create_test_app().await;

    let req = Request::builder()
        .uri("/api/tags")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0);
    assert!(body["data"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_tag_success() {
    let app = create_test_app().await;

    let req = json_request("POST", "/api/tags", json!({"name": "urgent", "color": "#ff0000"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["name"], "urgent");
    assert_eq!(body["data"]["color"], "#ff0000");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_tag_empty_name() {
    let app = create_test_app().await;

    let req = json_request("POST", "/api/tags", json!({"name": "", "color": "#ff0000"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 40002);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_tag() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/tags", json!({"name": "to-delete", "color": "#ff0000"}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let id = create_body["data"]["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/tags/{}", id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ===== Execution handlers =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_execution_records() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let todo_id = create_body["data"]["id"].as_i64().unwrap();

    let req = Request::builder()
        .uri(format!("/api/execution-records?todo_id={}", todo_id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["total"], 0);
    assert!(body["data"]["records"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_execution_records_pagination() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let todo_id = create_body["data"]["id"].as_i64().unwrap();

    let req = Request::builder()
        .uri(format!("/api/execution-records?todo_id={}&page=1&limit=5", todo_id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["page"], 1);
    assert_eq!(body["data"]["limit"], 5);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_execution_summary() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let todo_id = create_body["data"]["id"].as_i64().unwrap();

    let req = Request::builder()
        .uri(format!("/api/todos/{}/summary", todo_id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["total_executions"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_stop_execution_not_found() {
    let app = create_test_app().await;

    let req = json_request("POST", "/api/execute/stop", json!({"task_id": "nonexistent-task"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 40002);
}

// ===== Scheduler handlers =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_update_scheduler_enable() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Scheduled", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let id = create_body["data"]["id"].as_i64().unwrap();

    let req = json_request("PUT", &format!("/api/todos/{}/scheduler", id), json!({"scheduler_enabled": true, "scheduler_config": "0 0 0 * * *"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["scheduler_enabled"], true);
    assert_eq!(body["data"]["scheduler_config"], "0 0 0 * * *");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_update_scheduler_disable() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Scheduled", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let id = create_body["data"]["id"].as_i64().unwrap();

    // Enable first
    let enable_req = json_request("PUT", &format!("/api/todos/{}/scheduler", id), json!({"scheduler_enabled": true, "scheduler_config": "0 0 0 * * *"}));
    let _ = app.clone().oneshot(enable_req).await.unwrap();

    // Then disable
    let req = json_request("PUT", &format!("/api/todos/{}/scheduler", id), json!({"scheduler_enabled": false, "scheduler_config": null}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["scheduler_enabled"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_update_scheduler_missing_config() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Scheduled", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let id = create_body["data"]["id"].as_i64().unwrap();

    // Enable but without config -> should remove task
    let req = json_request("PUT", &format!("/api/todos/{}/scheduler", id), json!({"scheduler_enabled": true, "scheduler_config": null}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["scheduler_enabled"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_scheduler_todos() {
    let app = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Scheduled", "prompt": "Prompt", "tag_ids": []}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_body: serde_json::Value = read_json_body(create_resp).await;
    let id = create_body["data"]["id"].as_i64().unwrap();

    let enable_req = json_request("PUT", &format!("/api/todos/{}/scheduler", id), json!({"scheduler_enabled": true, "scheduler_config": "0 0 0 * * *"}));
    let _ = app.clone().oneshot(enable_req).await.unwrap();

    let req = Request::builder()
        .uri("/api/scheduler/todos")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0);
    let todos = body["data"].as_array().unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0]["id"], id);
}

// ===== Batch scheduler sync tests =====
// 这组测试覆盖 issue：批量暂停/恢复时 cron 任务未与 DB 同步。
// 通过 scheduler.has_task 直接验证内存 cron 状态，而非仅校验 HTTP 响应。

// 创建一个 todo 并通过单条接口启用调度，返回 todo id。
// 之所以走 HTTP 而非直接操作 DB，是为了与生产路径一致地注册 cron 任务。
// workspace_id 必填：create_todo handler 据此校验工作空间存在性。
async fn create_todo_with_scheduler(app: &axum::Router, workspace_id: i64, cron: &str) -> i64 {
    let create_req = json_request("POST", "/api/todos", json!({"title": "T", "prompt": "P", "tag_ids": [], "workspace_id": workspace_id}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let body: serde_json::Value = read_json_body(create_resp).await;
    let id = body["data"]["id"].as_i64().unwrap();
    let enable_req = json_request(
        "PUT",
        &format!("/api/todos/{}/scheduler", id),
        json!({"scheduler_enabled": true, "scheduler_config": cron}),
    );
    let _ = app.clone().oneshot(enable_req).await.unwrap();
    id
}

// 通过 GET /api/todos/{id} 读取单个 todo 的 scheduler_enabled 字段，
// 让测试能在不直接访问 DB 句柄的情况下验证持久化状态。
async fn get_todo_scheduler_enabled(app: &axum::Router, id: i64) -> bool {
    let req = Request::builder()
        .uri(format!("/api/todos/{}", id))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = read_json_body(resp).await;
    body["data"]["scheduler_enabled"].as_bool().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_batch_pause_removes_cron_tasks() {
    // 验证修复：批量暂停后，scheduler 内存中的 cron 任务应被同步移除，
    // 而不是仅把 DB 的 scheduler_enabled 置 false。
    let (app, scheduler, ws_id) = create_test_app_with_scheduler().await;
    let id1 = create_todo_with_scheduler(&app, ws_id, "0 */5 * * * *").await;
    let id2 = create_todo_with_scheduler(&app, ws_id, "0 */7 * * * *").await;
    assert!(scheduler.has_task(id1).await, "启用后 id1 应有 cron 任务");
    assert!(scheduler.has_task(id2).await, "启用后 id2 应有 cron 任务");

    let req = json_request("PUT", "/api/todos/batch-scheduler",
        json!({"ids": [id1, id2], "scheduler_enabled": false}));
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    assert!(!scheduler.has_task(id1).await, "批量暂停后 id1 的 cron 应被移除");
    assert!(!scheduler.has_task(id2).await, "批量暂停后 id2 的 cron 应被移除");
    assert_eq!(get_todo_scheduler_enabled(&app, id1).await, false, "DB 中 id1 应为暂停");
    assert_eq!(get_todo_scheduler_enabled(&app, id2).await, false, "DB 中 id2 应为暂停");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_batch_resume_restores_cron_tasks() {
    // 验证修复：批量恢复后，scheduler 内存中应重新注册 cron 任务。
    let (app, scheduler, ws_id) = create_test_app_with_scheduler().await;
    let id1 = create_todo_with_scheduler(&app, ws_id, "0 */5 * * * *").await;
    let id2 = create_todo_with_scheduler(&app, ws_id, "0 */7 * * * *").await;

    // 先批量暂停，再批量恢复，验证 cron 重新注册。
    let pause_req = json_request("PUT", "/api/todos/batch-scheduler",
        json!({"ids": [id1, id2], "scheduler_enabled": false}));
    let _ = app.clone().oneshot(pause_req).await.unwrap();
    assert!(!scheduler.has_task(id1).await, "暂停后 id1 不应有 cron");

    let resume_req = json_request("PUT", "/api/todos/batch-scheduler",
        json!({"ids": [id1, id2], "scheduler_enabled": true}));
    let resp = app.clone().oneshot(resume_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    assert!(scheduler.has_task(id1).await, "恢复后 id1 应重新注册 cron");
    assert!(scheduler.has_task(id2).await, "恢复后 id2 应重新注册 cron");
    assert_eq!(get_todo_scheduler_enabled(&app, id1).await, true, "DB 中 id1 应为启用");
    assert_eq!(get_todo_scheduler_enabled(&app, id2).await, true, "DB 中 id2 应为启用");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_batch_resume_skips_todo_without_config() {
    // 验证：恢复时遇到 scheduler_config 为空的 todo，不应注册 cron（无表达式可注册），
    // 也不会因为缺 config 而中断整批流程。
    let (app, scheduler, ws_id) = create_test_app_with_scheduler().await;
    // 创建一个 todo 但不启用调度（config 为空）
    let create_req = json_request("POST", "/api/todos", json!({"title": "NoConfig", "prompt": "P", "tag_ids": [], "workspace_id": ws_id}));
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let body: serde_json::Value = read_json_body(create_resp).await;
    let id_no_config = body["data"]["id"].as_i64().unwrap();
    // 另一个 todo 正常启用调度
    let id_with_config = create_todo_with_scheduler(&app, ws_id, "0 */5 * * * *").await;

    let resume_req = json_request("PUT", "/api/todos/batch-scheduler",
        json!({"ids": [id_no_config, id_with_config], "scheduler_enabled": true}));
    let resp = app.clone().oneshot(resume_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    assert!(!scheduler.has_task(id_no_config).await, "无 config 的 todo 不应注册 cron");
    assert!(scheduler.has_task(id_with_config).await, "有 config 的 todo 应注册 cron");
}

// ===== Lifecycle integration tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_todo_lifecycle() {
    let app = create_test_app().await;

    // Create
    let req = json_request("POST", "/api/todos", json!({"title": "Lifecycle", "prompt": "Test", "tag_ids": []}));
    let response = app.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = read_json_body(response).await;
    let id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["title"], "Lifecycle");

    // Update
    let req = json_request("PUT", &format!("/api/todos/{}", id), json!({"title": "Updated", "prompt": "Updated", "status": "in_progress"}));
    let response = app.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["title"], "Updated");

    // Delete
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/todos/{}", id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tag_lifecycle() {
    let app = create_test_app().await;

    // Create
    let req = json_request("POST", "/api/tags", json!({"name": "lifecycle", "color": "#00ff00"}));
    let response = app.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = read_json_body(response).await;
    let id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["name"], "lifecycle");

    // Get list
    let req = Request::builder()
        .uri("/api/tags")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // Delete
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/tags/{}", id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_todo_with_tags() {
    let app = create_test_app().await;

    // Create tags
    let tag1_req = json_request("POST", "/api/tags", json!({"name": "urgent", "color": "#ff0000"}));
    let tag1_resp = app.clone().oneshot(tag1_req).await.unwrap();
    let tag1_body: serde_json::Value = read_json_body(tag1_resp).await;
    let tag1_id = tag1_body["data"]["id"].as_i64().unwrap();

    let tag2_req = json_request("POST", "/api/tags", json!({"name": "later", "color": "#00ff00"}));
    let tag2_resp = app.clone().oneshot(tag2_req).await.unwrap();
    let tag2_body: serde_json::Value = read_json_body(tag2_resp).await;
    let tag2_id = tag2_body["data"]["id"].as_i64().unwrap();

    // Create todo with tags
    let todo_req = json_request("POST", "/api/todos", json!({"title": "Tagged", "prompt": "Do it", "tag_ids": [tag1_id]}));
    let todo_resp = app.clone().oneshot(todo_req).await.unwrap();
    let todo_body: serde_json::Value = read_json_body(todo_resp).await;
    let todo_id = todo_body["data"]["id"].as_i64().unwrap();
    assert_eq!(todo_body["data"]["tag_ids"], json!([tag1_id]));

    // Update tags
    let update_req = json_request("PUT", &format!("/api/todos/{}/tags", todo_id), json!({"tag_ids": [tag2_id]}));
    let _ = app.clone().oneshot(update_req).await.unwrap();

    // Verify
    let get_req = Request::builder()
        .uri("/api/todos")
        .body(Body::empty())
        .unwrap();
    let get_resp = app.oneshot(get_req).await.unwrap();
    let get_body: serde_json::Value = read_json_body(get_resp).await;
    let todos = get_body["data"].as_array().unwrap();
    let todo = todos.iter().find(|t| t["id"].as_i64().unwrap() == todo_id).unwrap();
    assert_eq!(todo["tag_ids"], json!([tag2_id]));
}
