//! HTTP 错误处理模块：定义统一的 handler 层错误类型和 JSON 提取器。
//!
//! `AppError` 是所有 handler 的统一错误类型，包含四个语义层级：
//! - `NotFound`：资源缺失（404）
//! - `BadRequest`：用户输入错误（400，caller 可修复）
//! - `Forbidden`：权限不足（403，跨 workspace 访问等）
//! - `Internal`：服务器侧故障（500）

use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// HTTP handler 统一错误类型。
///
/// 公开枚举保持稳定（issue #613 要求：仅重构实现方式，不改公开 API）。
/// 4 个变体对应 4 个语义层级。
#[derive(Debug)]
pub enum AppError {
    NotFound,
    BadRequest(String),
    Forbidden(String),
    Internal(String),
}

impl AppError {
    /// 把错误拆成 HTTP 响应三件套：(status, code, message)。
    pub(crate) fn error_response_parts(&self) -> (StatusCode, i32, String) {
        match self {
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                crate::models::codes::NOT_FOUND,
                "Not found".to_string(),
            ),
            Self::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                crate::models::codes::BAD_REQUEST,
                msg.clone(),
            ),
            Self::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
                crate::models::codes::FORBIDDEN,
                msg.clone(),
            ),
            Self::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::models::codes::INTERNAL,
                msg.clone(),
            ),
        }
    }

    /// 从 `sea_orm::DbErr` 构造：`RecordNotFound` 视为 404，其余归 500。
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn from_db_err(err: sea_orm::DbErr) -> Self {
        match &err {
            sea_orm::DbErr::RecordNotFound(_) => Self::NotFound,
            _ => Self::Internal(err.to_string()),
        }
    }

    /// 从 `std::io::Error` 构造：统一归 500。
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn from_io_err(err: std::io::Error) -> Self {
        Self::Internal(err.to_string())
    }

    /// 从 `SchedulerError` 构造：用户输入错误→400，其它→500。
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn from_scheduler_error(err: crate::scheduler::SchedulerError) -> Self {
        match &err {
            crate::scheduler::SchedulerError::InvalidCron { .. }
            | crate::scheduler::SchedulerError::InvalidTimezone(_) => {
                Self::BadRequest(err.to_string())
            }
            crate::scheduler::SchedulerError::Database(_)
            | crate::scheduler::SchedulerError::SchedulerBackend(_)
            | crate::scheduler::SchedulerError::Internal(_) => {
                Self::Internal(err.to_string())
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = self.error_response_parts();
        let body = axum::Json(crate::models::ApiResponse::<()>::err(code, &message));
        (status, body).into_response()
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        Self::from_db_err(err)
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        Self::BadRequest(s)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::from_io_err(err)
    }
}

impl From<crate::scheduler::SchedulerError> for AppError {
    fn from(err: crate::scheduler::SchedulerError) -> Self {
        Self::from_scheduler_error(err)
    }
}

/// `ApiResponse` 的 `IntoResponse` 实现，支持泛型序列化。
impl<T: Serialize> IntoResponse for crate::models::ApiResponse<T> {
    fn into_response(self) -> Response {
        axum::Json(self).into_response()
    }
}

/// 自定义 JSON 提取器，将解析错误转换为统一的 ApiResponse 错误格式。
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Json::<T>::from_request(req, state).await {
            Ok(axum::extract::Json(value)) => Ok(ApiJson(value)),
            Err(rejection) => Err(AppError::BadRequest(rejection.to_string())),
        }
    }
}
