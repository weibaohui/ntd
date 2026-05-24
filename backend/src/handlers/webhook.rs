use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::db::webhook::NewWebhookRecord;
use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;
use crate::executor_service::RunTodoExecutionRequest;

#[derive(Debug, serde::Deserialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub default_todo_id: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateWebhookRequest {
    pub name: String,
    pub enabled: bool,
    pub default_todo_id: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WebhookRecordQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct WebhookRecordResponse {
    pub id: i64,
    pub webhook_id: Option<i64>,
    pub webhook_name: Option<String>,
    pub method: String,
    pub path: String,
    pub query_params: Option<String>,
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub triggered_todo_id: Option<i64>,
    pub triggered_todo_title: Option<String>,
    pub status_code: Option<i32>,
    pub response_body: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct WebhookRecordsPage {
    pub records: Vec<WebhookRecordResponse>,
    pub total: i64,
    pub limit: usize,
    pub offset: usize,
}

/// List all webhooks
pub async fn list_webhooks(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let webhooks = state.db.get_webhooks().await?;
    Ok(ApiResponse::ok(webhooks))
}

/// Get a single webhook
pub async fn get_webhook(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let webhook = state.db.get_webhook(id).await?;
    match webhook {
        Some(w) => Ok(ApiResponse::ok(w)),
        None => Err(AppError::NotFound),
    }
}

/// Create a new webhook
pub async fn create_webhook(
    State(state): State<AppState>,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<impl IntoResponse, AppError> {
    let webhook = state.db.create_webhook(&req.name, req.default_todo_id).await?;
    Ok(ApiResponse::ok(webhook))
}

/// Update a webhook
pub async fn update_webhook(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.db.update_webhook(id, &req.name, req.enabled, req.default_todo_id).await?;
    let webhook = state.db.get_webhook(id).await?;
    Ok(ApiResponse::ok(webhook))
}

/// Delete a webhook
pub async fn delete_webhook(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.db.delete_webhook(id).await?;
    Ok(ApiResponse::ok(()))
}

/// Get webhook records with pagination
pub async fn get_webhook_records(
    State(state): State<AppState>,
    Query(query): Query<WebhookRecordQuery>,
) -> Result<impl IntoResponse, AppError> {
    use std::collections::HashMap;

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let records = state.db.get_webhook_records(limit, offset).await?;
    let total = state.db.get_webhook_records_count().await?;

    // Collect unique webhook_ids and todo_ids
    let webhook_ids: Vec<i64> = records.iter().filter_map(|r| r.webhook_id).collect();
    let todo_ids: Vec<i64> = records.iter().filter_map(|r| r.triggered_todo_id).collect();

    // Fetch all webhooks and todos in parallel using tokio::task::spawn
    let state_db = state.db.clone();
    let webhook_ids_clone = webhook_ids.clone();
    let webhook_handle = tokio::spawn(async move {
        let mut map = HashMap::new();
        for id in webhook_ids_clone {
            if let Ok(Some(w)) = state_db.get_webhook(id).await {
                map.insert(id, w.name);
            }
        }
        map
    });

    let state_db = state.db.clone();
    let todo_ids_clone = todo_ids.clone();
    let todo_handle = tokio::spawn(async move {
        let mut map = HashMap::new();
        for id in todo_ids_clone {
            if let Ok(Some(t)) = state_db.get_todo(id).await {
                map.insert(id, t.title);
            }
        }
        map
    });

    let webhook_map = webhook_handle.await.unwrap_or_default();
    let todo_map = todo_handle.await.unwrap_or_default();

    // Enrich records with webhook name and todo title
    let response_records: Vec<WebhookRecordResponse> = records
        .into_iter()
        .map(|record| WebhookRecordResponse {
            id: record.id,
            webhook_id: record.webhook_id,
            webhook_name: record.webhook_id.and_then(|id| webhook_map.get(&id).cloned()),
            method: record.method,
            path: record.path,
            query_params: record.query_params,
            body: record.body,
            content_type: record.content_type,
            triggered_todo_id: record.triggered_todo_id,
            triggered_todo_title: record.triggered_todo_id.and_then(|id| todo_map.get(&id).cloned()),
            status_code: record.status_code,
            response_body: record.response_body,
            created_at: record.created_at,
        })
        .collect();

    Ok(ApiResponse::ok(WebhookRecordsPage {
        records: response_records,
        total,
        limit,
        offset,
    }))
}

/// Get a single webhook record
pub async fn get_webhook_record(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let record = state.db.get_webhook_record(id).await?;
    match record {
        Some(record) => {
            let webhook_id = record.webhook_id;
            let triggered_todo_id = record.triggered_todo_id;

            // Fetch webhook name and todo title in parallel
            let webhook_handle = {
                let db = state.db.clone();
                tokio::spawn(async move {
                    if let Some(wid) = webhook_id {
                        db.get_webhook(wid).await.ok().flatten().map(|w| w.name)
                    } else {
                        None
                    }
                })
            };
            let todo_handle = {
                let db = state.db.clone();
                tokio::spawn(async move {
                    if let Some(tid) = triggered_todo_id {
                        db.get_todo(tid).await.ok().flatten().map(|t| t.title)
                    } else {
                        None
                    }
                })
            };

            let webhook_name = webhook_handle.await.ok().flatten();
            let triggered_todo_title = todo_handle.await.ok().flatten();

            let response = WebhookRecordResponse {
                id: record.id,
                webhook_id: record.webhook_id,
                webhook_name,
                method: record.method,
                path: record.path,
                query_params: record.query_params,
                body: record.body,
                content_type: record.content_type,
                triggered_todo_id: record.triggered_todo_id,
                triggered_todo_title,
                status_code: record.status_code,
                response_body: record.response_body,
                created_at: record.created_at,
            };
            Ok(ApiResponse::ok(response))
        }
        None => Err(AppError::NotFound),
    }
}

/// Trigger endpoint for webhook (without todo_id - uses default webhook todo)
pub async fn trigger_webhook_default(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    // Get the default webhook
    let webhook = state.db.get_default_webhook().await?;
    let webhook = webhook.ok_or_else(|| AppError::BadRequest("No enabled webhook configured".to_string()))?;

    let default_todo_id = webhook.default_todo_id
        .ok_or_else(|| AppError::BadRequest("Webhook has no default todo configured".to_string()))?;

    trigger_webhook_internal(
        Arc::new(state),
        default_todo_id,
        Some(webhook.id),
        "GET".to_string(),
        "/webhook/trigger".to_string(),
        params,
        None,
        None,
    ).await
}

/// Trigger endpoint for webhook (without todo_id - uses default webhook todo) - POST with JSON body
pub async fn trigger_webhook_default_post_json(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    // Get the default webhook
    let webhook = state.db.get_default_webhook().await?;
    let webhook = webhook.ok_or_else(|| AppError::BadRequest("No enabled webhook configured".to_string()))?;

    let default_todo_id = webhook.default_todo_id
        .ok_or_else(|| AppError::BadRequest("Webhook has no default todo configured".to_string()))?;

    let body_str = serde_json::to_string(&body).unwrap_or_default();
    trigger_webhook_internal(
        Arc::new(state),
        default_todo_id,
        Some(webhook.id),
        "POST".to_string(),
        "/webhook/trigger".to_string(),
        params,
        Some("application/json".to_string()),
        Some(body_str),
    ).await
}

/// Trigger endpoint for webhook (with todo_id) - GET
pub async fn trigger_webhook_with_todo(
    State(state): State<AppState>,
    Path(todo_id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    trigger_webhook_internal(
        Arc::new(state),
        todo_id,
        None,
        "GET".to_string(),
        format!("/webhook/trigger/{}", todo_id),
        params,
        None,
        None,
    ).await
}

/// Trigger endpoint for webhook (with todo_id) - POST with JSON body
pub async fn trigger_webhook_with_todo_post_json(
    State(state): State<AppState>,
    Path(todo_id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let body_str = serde_json::to_string(&body).unwrap_or_default();
    trigger_webhook_internal(
        Arc::new(state),
        todo_id,
        None,
        "POST".to_string(),
        format!("/webhook/trigger/{}", todo_id),
        params,
        Some("application/json".to_string()),
        Some(body_str),
    ).await
}

/// Internal trigger implementation
async fn trigger_webhook_internal(
    state: Arc<AppState>,
    todo_id: i64,
    webhook_id: Option<i64>,
    method: String,
    path: String,
    query_params: HashMap<String, String>,
    content_type: Option<String>,
    body: Option<String>,
) -> Result<impl IntoResponse, AppError> {
    // Get the todo
    let todo = state.db.get_todo(todo_id).await?
        .ok_or_else(|| AppError::NotFound)?;

    // Build message content
    let (message, raw_message) = build_message_content(&method, &query_params, &body, &content_type);

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
        },
    ).await;

    let (status_code, response_json) = match exec_result {
        Ok(result) => (StatusCode::OK, serde_json::json!({ "success": true, "record_id": result.record_id })),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "success": false, "error": format!("{:?}", e) })),
    };

    let response_body = response_json.to_string();

    // Record the webhook call
    if let Err(e) = state.db.create_webhook_record(NewWebhookRecord {
        webhook_id,
        method,
        path,
        query_params: if query_params.is_empty() { None } else { Some(serde_json::to_string(&query_params).unwrap_or_default()) },
        body,
        content_type,
        triggered_todo_id: Some(todo_id),
        status_code: Some(status_code.as_u16() as i32),
        response_body: Some(response_body.clone()),
    }).await {
        tracing::warn!("Failed to create webhook record: {:?}", e);
    }

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
