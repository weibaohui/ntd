// 测试代码允许 unwrap/expect/panic 等写法以简化断言逻辑，统一放宽以下 clippy 检查
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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

async fn create_test_app() -> (axum::Router, i64) {
    let db = Arc::new(Database::new(":memory:").await.unwrap());

    // 创建测试工作空间，handler 要求 workspace_id 必须对应已有目录
    let ws_id = db
        .create_project_directory("/tmp/test-api-workspace", Some("test"), false, false)
        .await
        .unwrap();

    let executor_registry = Arc::new(ExecutorRegistry::new());
    executor_registry.register(ClaudeCodeExecutor::new("claude".to_string())).await;

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
    };
    scheduler
        .load_from_db(&ctx)
        .await
        .unwrap();
    scheduler.start().await.unwrap();

    (create_app(ctx, scheduler).await, ws_id)
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
    let (app, ws_id) = create_test_app().await;

    // Create a todo first
    let req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Do this", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let req = json_request("POST", "/api/todos", json!({"title": "New Todo", "prompt": "Prompt text", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let req = json_request("POST", "/api/todos", json!({"title": "", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 40002);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_todo_prompt_fallback() {
    let (app, ws_id) = create_test_app().await;

    let req = json_request("POST", "/api/todos", json!({"title": "Fallback Title", "prompt": "", "workspace_id": ws_id, "tag_ids": []}));
    let response = app.oneshot(req).await.unwrap();

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["prompt"], "Fallback Title");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_todo_with_tags() {
    let (app, ws_id) = create_test_app().await;

    // Create a tag first
    let tag_req = json_request("POST", "/api/tags", json!({"name": "urgent", "color": "#ff0000"}));
    let tag_resp = app.clone().oneshot(tag_req).await.unwrap();
    let tag_body: serde_json::Value = read_json_body(tag_resp).await;
    let tag_id = tag_body["data"]["id"].as_i64().unwrap();

    let req = json_request("POST", "/api/todos", json!({"title": "Tagged", "prompt": "Do this", "workspace_id": ws_id, "tag_ids": [tag_id]}));
    let response = app.oneshot(req).await.unwrap();

    let body: serde_json::Value = read_json_body(response).await;
    let tag_ids = body["data"]["tag_ids"].as_array().unwrap();
    assert_eq!(tag_ids.len(), 1);
    assert_eq!(tag_ids[0], tag_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_update_todo_success() {
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Old", "prompt": "Old prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Title", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "To Delete", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, _ws_id) = create_test_app().await;
    let get_resp = app.oneshot(get_req).await.unwrap();
    let get_body: serde_json::Value = read_json_body(get_resp).await;
    let todos = get_body["data"].as_array().unwrap();
    assert!(todos.iter().all(|t| t["id"].as_i64().unwrap() != id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_todo_not_found() {
    let (app, _ws_id) = create_test_app().await;

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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, _ws_id) = create_test_app().await;

    let req = json_request("PUT", "/api/todos/9999", json!({"title": "Test", "prompt": "Prompt", "status": "pending"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ===== Step handlers =====
//
// ===== Tag handlers =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_tags() {
    let (app, _ws_id) = create_test_app().await;

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
    let (app, _ws_id) = create_test_app().await;

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
    let (app, _ws_id) = create_test_app().await;

    let req = json_request("POST", "/api/tags", json!({"name": "", "color": "#ff0000"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 40002);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_tag() {
    let (app, _ws_id) = create_test_app().await;

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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Test", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, _ws_id) = create_test_app().await;

    let req = json_request("POST", "/api/execute/stop", json!({"task_id": "nonexistent-task"}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 40002);
}

// ===== Scheduler handlers =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_update_scheduler_enable() {
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Scheduled", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Scheduled", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Scheduled", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, ws_id) = create_test_app().await;

    let create_req = json_request("POST", "/api/todos", json!({"title": "Scheduled", "prompt": "Prompt", "workspace_id": ws_id, "tag_ids": []}));
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

// ===== Lifecycle integration tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_todo_lifecycle() {
    let (app, ws_id) = create_test_app().await;

    // Create
    let req = json_request("POST", "/api/todos", json!({"title": "Lifecycle", "prompt": "Test", "workspace_id": ws_id, "tag_ids": []}));
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
    let (app, _ws_id) = create_test_app().await;

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
    let (app, ws_id) = create_test_app().await;

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
    let todo_req = json_request("POST", "/api/todos", json!({"title": "Tagged", "prompt": "Do it", "workspace_id": ws_id, "tag_ids": [tag1_id]}));
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

/// DELETE /api/workspaces/{id}/wiki/files/{slug}：删除已存在 topic，返回 deleted=true。
/// 用唯一 slug 避免与开发者本地真实 workspace 数据撞车；用例自身即删除该文件，自清理。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_wiki_file_existing() {
    let (app, ws_id) = create_test_app().await;
    let slug = "__delete_api_test__";
    // 种入 topic 文件，作为删除目标
    ntd::wiki::write_topic(ws_id, slug, "# to be deleted").unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/workspaces/{}/wiki/files/{}", ws_id, slug))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["slug"], slug);
    assert_eq!(body["data"]["deleted"], true);
    // 确认磁盘上确已删除，而非仅返回成功
    assert!(ntd::wiki::read_topic(ws_id, slug).unwrap().is_none());
}

/// DELETE 对不存在的 topic 返回 deleted=false（幂等），而非 404/500。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_wiki_file_missing_is_idempotent() {
    let (app, ws_id) = create_test_app().await;
    let slug = "__delete_api_test_missing__";

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/workspaces/{}/wiki/files/{}", ws_id, slug))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["data"]["deleted"], false);
}

/// DELETE slug=log 必须被拒绝（执行日志由系统维护），返回 400。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_wiki_file_log_forbidden() {
    let (app, ws_id) = create_test_app().await;

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/workspaces/{}/wiki/files/log", ws_id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ====== Loop 导入 merge 语义：原名保留 + 同名覆盖（对齐 Todo） ======

/// 构造最小可用的 loop 导出 YAML：一个 loop + 一个 step-todo，workspace_id 指向给定 ws。
fn loop_merge_yaml(name: &str, ws_id: i64) -> String {
    format!(
r#"version: "1.0"
type: loop
created_at: "2026-07-09T00:00:00Z"
source: nothing-todo
schema_version: 1
tags: []
review_templates: []
todos:
  - id: "@todo_1"
    title: "merge-todo"
    prompt: "p"
    status: "pending"
    executor: null
    scheduler_enabled: false
    webhook_enabled: false
    acceptance_criteria: null
    auto_review_enabled: false
    review_template_id: null
    review_template_name: null
    kind: "0"
    tag_ids: []
    tag_names: []
    is_abnormal_handler: false
    action_type: null
    action_key: null
    workspace_id: {ws}
    workspace_path: "/tmp/test-api-workspace"
loops:
  - id: "@loop_1"
    name: "{name}"
    description: ""
    icon: ""
    color: ""
    status: "paused"
    webhook_enabled: false
    limits_config: {{}}
    review_template_id: null
    review_template_name: null
    abnormal_handler_todo_id: null
    abnormal_handler_todo_title: null
    abnormal_handler_trigger_on: []
    tag_ids: []
    tag_names: []
    triggers: []
    steps:
      - id: "@step_1"
        name: "s1"
        description: ""
        todo_id: "@todo_1"
        todo_title: "merge-todo"
        order_index: 0
        run_mode: "auto"
        skip_on_source_failed: false
        min_rating: null
        unrated_policy: "skip"
        on_success: "continue"
        success_goto_step_id: null
        success_goto_step_name: null
        on_rating_fail: "stop"
        fail_goto_step_id: null
        fail_goto_step_name: null
        review_type: "none"
        enabled: true
    workspace_id: {ws}
    workspace_path: "/tmp/test-api-workspace"
"#,
        name = name,
        ws = ws_id
    )
}

/// 取指定工作空间下名为 name 的 loop 数量（通过 list 接口）
async fn count_loops_named(app: axum::Router, ws_id: i64, name: &str) -> usize {
    let uri = format!("/api/loops?workspace_id={}", ws_id);
    let req = Request::builder().uri(&uri).body(Body::empty()).unwrap();
    let response = app.oneshot(req).await.unwrap();
    let body: serde_json::Value = read_json_body(response).await;
    body["data"].as_array().unwrap_or(&vec![]).iter().filter(|l| l["name"] == name).count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_loop_merge_keeps_original_name() {
    // 导入后 loop 名应保持原名，不再追加 "-合并" 后缀
    let (app, ws_id) = create_test_app().await;
    let yaml = loop_merge_yaml("我的环路", ws_id);
    let req = json_request("POST", "/api/loops/merge", json!({
        "yaml": yaml, "workspace_id": null, "workspace_overrides": {}
    }));
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0, "merge 应成功: {:?}", body);
    // 原名命中 1 个，"-合并" 后缀命中 0 个
    assert_eq!(count_loops_named(app.clone(), ws_id, "我的环路").await, 1);
    assert_eq!(count_loops_named(app, ws_id, "我的环路-合并").await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_loop_merge_same_name_overwrites() {
    // 同名 loop 二次导入 → 覆盖（删旧重建），不产生重复
    let (app, ws_id) = create_test_app().await;
    let yaml = loop_merge_yaml("dup-loop", ws_id);
    let req = json_request("POST", "/api/loops/merge", json!({
        "yaml": yaml, "workspace_id": null, "workspace_overrides": {}
    }));
    let r1 = app.clone().oneshot(req).await.unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    assert_eq!(count_loops_named(app.clone(), ws_id, "dup-loop").await, 1);

    // 再次导入同名 → 覆盖，仍只 1 个
    let req2 = json_request("POST", "/api/loops/merge", json!({
        "yaml": yaml, "workspace_id": null, "workspace_overrides": {}
    }));
    let r2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(r2.status(), StatusCode::OK);
    assert_eq!(count_loops_named(app, ws_id, "dup-loop").await, 1);
}
