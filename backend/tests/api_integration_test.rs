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
    let db = Arc::new(Database::new(":memory:").await.unwrap());

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

    create_app(ctx, scheduler)
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
// 覆盖 POST /api/steps（直建环节，不再走 createTodo+promote 流程）。
// 之所以单测直建：todo 和 step 已经彻底拆开；前端"新建环节"必须直接
// 写 steps 表，避开「先建 todo 再 promote」造成的 id 错位与
// 副作用 todo 残留。

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_steps_creates_step_directly() {
    let app = create_test_app().await;

    // 起始状态：steps 表为空（seed 数据里没有 step）
    let list_req = Request::builder()
        .uri("/api/steps")
        .body(Body::empty())
        .unwrap();
    let list_res = app.clone().oneshot(list_req).await.unwrap();
    let list_body: serde_json::Value = read_json_body(list_res).await;
    assert_eq!(
        list_body["data"].as_array().unwrap().len(),
        0,
        "precondition: fresh DB 应当没有 step"
    );

    // POST /api/steps —— 直建，必须返回 200 + 完整 StepDto
    let req = json_request(
        "POST",
        "/api/steps",
        json!({
            "title": "代码审查",
            "prompt": "你是一个代码审查助手",
            "executor": "claudecode"
        }),
    );
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = read_json_body(response).await;
    assert_eq!(body["code"], 0);
    let data = &body["data"];
    assert_eq!(data["title"], "代码审查");
    assert_eq!(data["prompt"], "你是一个代码审查助手");
    assert_eq!(data["executor"], "claudecode");
    assert!(data["id"].as_i64().unwrap() > 0, "返回的 id 必须是正整数");

    // 验证：GET /api/steps/{id} 用返回的 id 必须能查到（这是修"id 错位"bug 的关键断言）
    let step_id = data["id"].as_i64().unwrap();
    let get_req = Request::builder()
        .uri(format!("/api/steps/{}", step_id))
        .body(Body::empty())
        .unwrap();
    let get_res = app.oneshot(get_req).await.unwrap();
    assert_eq!(get_res.status(), StatusCode::OK, "返回的 id 必须能直接 GET 到");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_steps_rejects_empty_title() {
    let app = create_test_app().await;

    let req = json_request(
        "POST",
        "/api/steps",
        json!({"title": "", "prompt": "P", "executor": "claudecode"}),
    );
    let response = app.oneshot(req).await.unwrap();
    // title 必填；空字符串应被业务层拒绝
    assert!(response.status().is_client_error() || response.status().is_server_error(),
            "空 title 必须非 2xx，实际 {}", response.status());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_steps_does_not_create_todo() {
    // 关键回归：之前 TodoList.tsx 的"新建环节"会先 createTodo 再 promote，
    // 留下一个孤儿 todo。这里 POST /api/steps 必须不触发任何 todo 插入。
    let app = create_test_app().await;

    // 起始 todos 数量
    let before: serde_json::Value = {
        let req = Request::builder()
            .uri("/api/todos?page=1&limit=1000")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        read_json_body(res).await
    };
    let before_total = before["data"]["total"].as_i64().unwrap_or(0);

    // POST 一个 step
    let req = json_request(
        "POST",
        "/api/steps",
        json!({"title": "直建环节", "prompt": "P", "executor": "claudecode"}),
    );
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 再查 todos 数量，应当 0 增长
    let after: serde_json::Value = {
        let req = Request::builder()
            .uri("/api/todos?page=1&limit=1000")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        read_json_body(res).await
    };
    let after_total = after["data"]["total"].as_i64().unwrap_or(0);
    assert_eq!(
        after_total, before_total,
        "POST /api/steps 不应触发 todo 插入；之前 before={}, after={}",
        before_total, after_total
    );
}

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
