use axum::{
    extract::State,
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::executor_service::RunTodoExecutionRequest;
use crate::handlers::{AppError, AppState};

/// Trigger endpoint for webhook (with todo_id) - GET
pub async fn trigger_webhook_with_todo(
    State(state): State<AppState>,
    Path(todo_id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    trigger_todo_webhook_internal(Arc::new(state), todo_id, "GET", params, None, None).await
}

/// Trigger endpoint for webhook (with todo_id) - POST with JSON body
pub async fn trigger_webhook_with_todo_post_json(
    State(state): State<AppState>,
    Path(todo_id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let body_str = serde_json::to_string(&body).map_err(|e| {
        tracing::warn!("Failed to serialize webhook body: {}", e);
        AppError::BadRequest(format!("Invalid body: {}", e))
    })?;
    trigger_todo_webhook_internal(
        Arc::new(state),
        todo_id,
        "POST",
        params,
        Some("application/json".to_string()),
        Some(body_str),
    )
    .await
}

/// Trigger endpoint for loop webhook (with loop_id) - GET
pub async fn trigger_webhook_with_loop_get(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    trigger_loop_webhook_internal(Arc::new(state), loop_id, "GET", params, None, None).await
}

/// Trigger endpoint for loop webhook (with loop_id) - POST with JSON body
pub async fn trigger_webhook_with_loop_post(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let body_str = serde_json::to_string(&body).ok();
    let content_type = Some("application/json".to_string());
    trigger_loop_webhook_internal(Arc::new(state), loop_id, "POST", params, content_type, body_str).await
}

async fn trigger_loop_webhook_internal(
    state: Arc<AppState>,
    loop_id: i64,
    method: &str,
    query_params: HashMap<String, String>,
    content_type: Option<String>,
    body: Option<String>,
) -> Result<impl IntoResponse, AppError> {
    let Some(loop_) = state.db.get_loop(loop_id).await? else {
        return Err(AppError::NotFound);
    };
    if !loop_.webhook_enabled {
        return Err(AppError::BadRequest("Webhook 未启用".to_string()));
    }
    let dispatcher = state
        .loop_trigger_dispatcher
        .as_ref()
        .ok_or_else(|| AppError::Internal("loop dispatcher not ready".to_string()))?;
    let execution_id = dispatcher
        .dispatch_loop_webhook(loop_id, method, &query_params, body.as_deref(), content_type.as_deref())
        .await
        .ok_or_else(|| AppError::BadRequest("loop 不存在或未启用".to_string()))?;
    Ok((StatusCode::OK, axum::Json(serde_json::json!({ "success": true, "execution_id": execution_id }))).into_response())
}

async fn trigger_todo_webhook_internal(
    state: Arc<AppState>,
    todo_id: i64,
    method: &str,
    query_params: HashMap<String, String>,
    content_type: Option<String>,
    body: Option<String>,
) -> Result<impl IntoResponse, AppError> {
    let todo = state.db.get_todo(todo_id).await?.ok_or(AppError::NotFound)?;
    if !todo.webhook_enabled {
        return Err(AppError::BadRequest("Webhook 未启用".to_string()));
    }

    // Build message content
    let (message, raw_message) = build_message_content(method, &query_params, &body, &content_type);

    // Execute the todo
    let exec_result = crate::handlers::execution::start_todo_execution(
        RunTodoExecutionRequest {
            db: state.db.clone(),
            executor_registry: state.executor_registry.clone(),
            tx: state.tx.clone(),
            task_manager: state.task_manager.clone(),
            config: state.config.clone(),
            todo_id,
            message: todo.prompt.clone(),
            req_executor: todo.executor.clone(),
            trigger_type: "webhook".to_string(),
            params: Some({
                let mut p = query_params.clone();
                p.insert("message".to_string(), message.clone());
                p.insert("raw_message".to_string(), raw_message.clone());
                p
            }),
            resume_session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,
            step_id: None,
            feishu_bot_id: None,
            feishu_receive_id: None,
            workspace: todo.workspace.clone(),
        },
    ).await;

    let (status_code, response_json) = match exec_result {
        Ok(result) => (StatusCode::OK, serde_json::json!({ "success": true, "record_id": result.record_id })),
        Err(e) => {
            tracing::error!("webhook trigger execution failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "success": false, "error": "Internal server error" }))
        }
    };

    Ok((status_code, axum::Json(response_json)).into_response())
}

fn build_message_content(
    method: &str,
    query_params: &HashMap<String, String>,
    body: &Option<String>,
    content_type: &Option<String>,
) -> (String, String) {
    let raw = if let Some(ref b) = body {
        if b.is_empty() {
            String::new()
        } else {
            b.clone()
        }
    } else {
        String::new()
    };

    let processed = if let Some(ct) = content_type {
        if ct.contains("application/json") {
            // Try to parse as JSON and format
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                serde_json::to_string_pretty(&val).unwrap_or(raw.clone())
            } else {
                raw.clone()
            }
        } else if ct.contains("application/x-www-form-urlencoded") {
            // Form data - use as-is
            raw.clone()
        } else {
            raw.clone()
        }
    } else {
        raw.clone()
    };

    // Build message with method and query params
    let message = if !query_params.is_empty() {
        let params_str = query_params.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");
        format!("Method: {}\n{}\n\nQuery: {}", method, processed, params_str)
    } else {
        format!("Method: {}\n{}", method, processed)
    };

    (message, raw)
}
