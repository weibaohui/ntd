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
    pub enabled: bool,
    pub default_todo_id: Option<i64>,
    /// 仅 webhook_type = "loop" 时使用
    pub loop_id: Option<i64>,
    /// "todo" | "loop"，默认为 "todo"
    #[serde(default = "default_webhook_type")]
    pub webhook_type: String,
}

fn default_webhook_type() -> String {
    "todo".to_string()
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateWebhookRequest {
    pub name: String,
    pub enabled: bool,
    pub default_todo_id: Option<i64>,
    /// 仅 webhook_type = "loop" 时使用
    pub loop_id: Option<i64>,
    /// "todo" | "loop"
    #[serde(default = "default_webhook_type")]
    pub webhook_type: String,
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

/// Parameters for triggering a webhook execution.
pub struct WebhookTriggerRequest {
    pub todo_id: i64,
    pub webhook_id: Option<i64>,
    pub method: String,
    pub path: String,
    pub query_params: HashMap<String, String>,
    pub content_type: Option<String>,
    pub body: Option<String>,
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
    let webhook = state.db.create_webhook(
        &req.name,
        req.enabled,
        req.default_todo_id,
        req.loop_id,
        &req.webhook_type,
    ).await?;
    Ok(ApiResponse::ok(webhook))
}

/// Update a webhook
pub async fn update_webhook(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.db.update_webhook(
        id,
        &req.name,
        req.enabled,
        req.default_todo_id,
        req.loop_id,
        &req.webhook_type,
    ).await?;
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
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let records = state.db.get_webhook_records(limit, offset).await?;
    let total = state.db.get_webhook_records_count().await?;

    // Collect unique webhook_ids and todo_ids
    let webhook_ids: Vec<i64> = records.iter().filter_map(|r| r.webhook_id).collect();
    let todo_ids: Vec<i64> = records.iter().filter_map(|r| r.triggered_todo_id).collect();

    // Batch fetch all webhooks and todos in a single query each
    let webhooks = state.db.get_webhooks_by_ids(&webhook_ids).await?;
    let todos = state.db.get_todos_by_ids(&todo_ids).await?;

    // Build lookup maps
    let webhook_map: HashMap<i64, String> = webhooks.into_iter().map(|w| (w.id, w.name)).collect();
    let todo_map: HashMap<i64, String> = todos.into_iter().map(|t| (t.id, t.title)).collect();

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

/// Trigger endpoint for webhook (with todo_id) - GET
pub async fn trigger_webhook_with_todo(
    State(state): State<AppState>,
    Path(todo_id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    // Find webhook that has this todo as default and is enabled
    let webhook = state.db.get_webhook_by_default_todo(todo_id).await?
        .ok_or_else(|| AppError::BadRequest("No enabled webhook configured for this todo".to_string()))?;

    trigger_webhook_internal(
        Arc::new(state),
        WebhookTriggerRequest {
            todo_id,
            webhook_id: Some(webhook.id),
            method: "GET".to_string(),
            path: format!("/webhook/trigger/todo/{}", todo_id),
            query_params: params,
            content_type: None,
            body: None,
        },
    ).await
}

/// Trigger endpoint for webhook (with todo_id) - POST with JSON body
pub async fn trigger_webhook_with_todo_post_json(
    State(state): State<AppState>,
    Path(todo_id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    // Find webhook that has this todo as default and is enabled
    let webhook = state.db.get_webhook_by_default_todo(todo_id).await?
        .ok_or_else(|| AppError::BadRequest("No enabled webhook configured for this todo".to_string()))?;

    let body_str = serde_json::to_string(&body)
        .map_err(|e| {
            tracing::warn!("Failed to serialize webhook body: {}", e);
            AppError::BadRequest(format!("Invalid body: {}", e))
        })?;
    trigger_webhook_internal(
        Arc::new(state),
        WebhookTriggerRequest {
            todo_id,
            webhook_id: Some(webhook.id),
            method: "POST".to_string(),
            path: format!("/webhook/trigger/todo/{}", todo_id),
            query_params: params,
            content_type: Some("application/json".to_string()),
            body: Some(body_str),
        },
    ).await
}

/// Trigger endpoint for loop webhook (with loop_id) - GET
pub async fn trigger_webhook_with_loop_get(
    State(state): State<AppState>,
    Path(loop_id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    trigger_webhook_loop_internal(Arc::new(state), loop_id, "GET", params, None, None).await
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
    trigger_webhook_loop_internal(Arc::new(state), loop_id, "POST", params, content_type, body_str).await
}

/// Internal loop webhook trigger implementation
async fn trigger_webhook_loop_internal(
    state: Arc<AppState>,
    loop_id: i64,
    method: &str,
    query_params: HashMap<String, String>,
    content_type: Option<String>,
    body: Option<String>,
) -> Result<impl IntoResponse, AppError> {
    // Find enabled loop webhook for this loop_id
    let webhook = state.db.get_webhook_by_loop(loop_id).await?
        .ok_or_else(|| AppError::NotFound)?;

    if !webhook.enabled {
        return Err(AppError::BadRequest("Webhook is disabled".to_string()));
    }

    // Record the webhook call
    let response_body = if let Some(dispatcher) = state.loop_trigger_dispatcher.as_ref() {
        // Dispatch to loop trigger - this triggers all loop triggers that reference this webhook
        let started = dispatcher.dispatch_webhook(webhook.id, body.as_deref()).await;
        serde_json::json!({ "success": true, "dispatched": true, "started_runs": started }).to_string()
    } else {
        serde_json::json!({ "success": false, "error": "Loop trigger dispatcher not available" }).to_string()
    };

    let query_params_json = if query_params.is_empty() {
        None
    } else {
        serde_json::to_string(&query_params).ok()
    };

    if let Err(e) = state.db.create_webhook_record(NewWebhookRecord {
        webhook_id: Some(webhook.id),
        method: method.to_string(),
        path: format!("/webhook/trigger/loop/{}", loop_id),
        query_params: query_params_json,
        body: body.clone(),
        content_type: content_type.clone(),
        triggered_todo_id: None, // loop webhook 不关联具体 todo
        status_code: Some(200),
        response_body: Some(response_body.clone()),
    }).await {
        tracing::warn!("Failed to create webhook record: {:?}", e);
    }

    Ok((StatusCode::OK, axum::Json(serde_json::json!({ "success": true }))).into_response())
}

/// Internal trigger implementation
async fn trigger_webhook_internal(
    state: Arc<AppState>,
    req: WebhookTriggerRequest,
) -> Result<impl IntoResponse, AppError> {
    // Get the todo
    let todo = state.db.get_todo(req.todo_id).await?
        .ok_or_else(|| AppError::NotFound)?;

    // Build message content
    let (message, raw_message) = build_message_content(&req.method, &req.query_params, &req.body, &req.content_type);

    // Execute the todo
    let exec_result = crate::handlers::execution::start_todo_execution(
        RunTodoExecutionRequest {
            db: state.db.clone(),
            executor_registry: state.executor_registry.clone(),
            tx: state.tx.clone(),
            task_manager: state.task_manager.clone(),
            config: state.config.clone(),
            todo_id: req.todo_id,
            message: todo.prompt.clone(),
            req_executor: todo.executor.clone(),
            trigger_type: "webhook".to_string(),
            params: Some({
                let mut p = req.query_params.clone();
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
            workspace: None,
        },
    ).await;

    let (status_code, response_json) = match exec_result {
        Ok(result) => (StatusCode::OK, serde_json::json!({ "success": true, "record_id": result.record_id })),
        Err(e) => {
            tracing::error!("webhook trigger execution failed: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "success": false, "error": "Internal server error" }))
        }
    };

    let response_body = response_json.to_string();

    // Record the webhook call
    let query_params_json = if req.query_params.is_empty() {
        None
    } else {
        match serde_json::to_string(&req.query_params) {
            Ok(json) => Some(json),
            Err(e) => {
                tracing::warn!("Failed to serialize query_params: {}", e);
                None
            }
        }
    };

    if let Err(e) = state.db.create_webhook_record(NewWebhookRecord {
        webhook_id: req.webhook_id,
        method: req.method,
        path: req.path,
        query_params: query_params_json,
        body: req.body.clone(),
        content_type: req.content_type,
        triggered_todo_id: Some(req.todo_id),
        status_code: Some(status_code.as_u16() as i32),
        response_body: Some(response_body.clone()),
    }).await {
        tracing::warn!("Failed to create webhook record: {:?}", e);
    }

    // Loop Studio: 把 webhook 事件转给 loop_trigger_dispatcher,匹配 webhook 触发器
    if let (Some(dispatcher), Some(webhook_id)) = (
        state.loop_trigger_dispatcher.as_ref(),
        req.webhook_id,
    ) {
        let _ = dispatcher.dispatch_webhook(webhook_id, req.body.as_deref()).await;
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
