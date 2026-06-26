use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::handlers::{AppError, AppState};
use crate::models::{ApiResponse, FeishuMessageStats};

#[derive(Debug, Deserialize)]
pub struct HistoryMessagesQuery {
    pub chat_id: Option<String>,
    pub sender_open_id: Option<String>,
    pub is_history: Option<bool>,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct HistoryMessagesResponse {
    pub messages: Vec<HistoryMessageItem>,
    pub total: i64,
    pub page: u64,
    pub page_size: u64,
}

#[derive(Debug, Serialize)]
pub struct HistoryMessageItem {
    pub id: i64,
    pub message_id: String,
    pub chat_id: String,
    pub chat_type: String,
    pub sender_open_id: String,
    pub sender_nickname: Option<String>,
    pub sender_type: Option<String>,
    pub content: Option<String>,
    pub msg_type: String,
    pub is_history: bool,
    pub processed: bool,
    pub processed_todo_id: Option<i64>,
    pub execution_record_id: Option<i64>,
    pub created_at: Option<String>,
    /// 消息接收时，智能体所属的工作空间 ID
    pub workspace_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateHistoryChatRequest {
    pub bot_id: i64,
    pub chat_id: String,
    pub chat_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HistoryChatItem {
    pub id: i64,
    pub bot_id: i64,
    pub chat_id: String,
    pub chat_name: Option<String>,
    pub enabled: bool,
    pub last_fetch_time: Option<String>,
    pub polling_interval_secs: i32,
    pub created_at: Option<String>,
}

pub async fn get_history_messages(
    State(state): State<AppState>,
    Query(query): Query<HistoryMessagesQuery>,
) -> Result<Json<ApiResponse<HistoryMessagesResponse>>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).min(50);

    let (messages, total) = state.db.get_feishu_history_messages(
        query.chat_id.as_deref(),
        query.sender_open_id.as_deref(),
        query.is_history,
        page,
        page_size,
    ).await?;

    let items = messages
        .into_iter()
        .map(|m| HistoryMessageItem {
            id: m.id,
            message_id: m.message_id,
            chat_id: m.chat_id,
            chat_type: m.chat_type,
            sender_open_id: m.sender_open_id,
            sender_nickname: m.sender_nickname,
            sender_type: m.sender_type,
            content: m.content,
            msg_type: m.msg_type,
            is_history: m.is_history,
            processed: m.processed,
            processed_todo_id: m.processed_todo_id,
            execution_record_id: m.execution_record_id,
            created_at: m.created_at,
            workspace_id: m.workspace_id,
        })
        .collect();

    Ok(Json(ApiResponse::ok(HistoryMessagesResponse {
        messages: items,
        total,
        page,
        page_size,
    })))
}

#[derive(Debug, Deserialize)]
pub struct HistoryChatsQuery {
    pub bot_id: Option<i64>,
}

pub async fn get_history_chats(
    State(state): State<AppState>,
    Query(query): Query<HistoryChatsQuery>,
) -> Result<Json<ApiResponse<Vec<HistoryChatItem>>>, AppError> {
    let bot_id = query.bot_id.unwrap_or(1i64);

    let chats = state.db.get_feishu_history_chats(bot_id).await?;

    let items = chats
        .into_iter()
        .map(|c| HistoryChatItem {
            id: c.id,
            bot_id: c.bot_id,
            chat_id: c.chat_id,
            chat_name: c.chat_name,
            enabled: c.enabled,
            last_fetch_time: c.last_fetch_time,
            polling_interval_secs: c.polling_interval_secs,
            created_at: c.created_at,
        })
        .collect();

    Ok(Json(ApiResponse::ok(items)))
}

pub async fn create_history_chat(
    State(state): State<AppState>,
    Json(req): Json<CreateHistoryChatRequest>,
) -> Result<Json<ApiResponse<HistoryChatItem>>, AppError> {
    let id = state.db.create_feishu_history_chat(
        req.bot_id,
        &req.chat_id,
        req.chat_name.as_deref(),
    ).await?;

    let chats = state.db.get_feishu_history_chats(req.bot_id).await?;
    let chat = chats
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| AppError::Internal("failed to get created chat".to_string()))?;

    Ok(Json(ApiResponse::ok(HistoryChatItem {
        id: chat.id,
        bot_id: chat.bot_id,
        chat_id: chat.chat_id,
        chat_name: chat.chat_name,
        enabled: chat.enabled,
        last_fetch_time: chat.last_fetch_time,
        polling_interval_secs: chat.polling_interval_secs,
        created_at: chat.created_at,
    })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateHistoryChatRequest {
    pub chat_name: Option<String>,
    pub enabled: Option<bool>,
    pub polling_interval_secs: Option<i32>,
}

pub async fn update_history_chat(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateHistoryChatRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    state.db.update_feishu_history_chat(
        id,
        req.chat_name.as_deref(),
        req.enabled,
        req.polling_interval_secs,
    ).await?;

    Ok(Json(ApiResponse::ok(())))
}

#[derive(Debug, Serialize)]
pub struct SenderItem {
    pub sender_open_id: String,
    pub sender_type: Option<String>,
    pub sender_nickname: Option<String>,
    pub count: i64,
}

pub async fn get_distinct_senders(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<SenderItem>>>, AppError> {
    let senders = state.db.get_distinct_senders().await?;
    let items = senders.into_iter().map(|(sender_open_id, sender_type, sender_nickname, count)| SenderItem {
        sender_open_id,
        sender_type,
        sender_nickname,
        count,
    }).collect();
    Ok(Json(ApiResponse::ok(items)))
}

pub async fn delete_history_chat(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    state.db.delete_feishu_history_chat(id).await?;
    Ok(Json(ApiResponse::ok(())))
}

pub async fn get_message_stats(
    State(state): State<AppState>,
    Query(params): Query<MessageStatsParams>,
) -> Result<Json<ApiResponse<FeishuMessageStats>>, AppError> {
    let stats = state.db.get_feishu_message_stats(params.hours).await?;
    Ok(Json(ApiResponse::ok(stats)))
}

#[derive(Debug, Deserialize)]
pub struct MessageStatsParams {
    #[serde(default)]
    pub hours: Option<u32>,
}
