//! 中间件模块：请求 ID 传播和 CORS 配置。

use axum::extract::Request;
use axum::http::{self, header, HeaderMap};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

/// Request ID 类型，存储在 request extensions 中。
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// 给每个 HTTP 请求注入 `X-Request-Id` 并回写到响应头。
pub async fn propagate_request_id(mut req: Request, next: Next) -> Response {
    let request_id = resolve_request_id(req.headers());
    req.extensions_mut().insert(RequestId(request_id.clone()));

    tracing::debug!(
        request_id = %request_id,
        method = %req.method(),
        uri = %req.uri(),
        "http request received"
    );

    let mut response = next.run(req).await;

    if let Ok(value) = header::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }

    response
}

/// 解析或生成 request_id：优先沿用上游传入值，否则生成 UUIDv4。
pub fn resolve_request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// CORS expose_headers 列表。
pub fn cors_expose_headers() -> [http::HeaderName; 1] {
    [http::HeaderName::from_static("x-request-id")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_request_id_when_header_missing() {
        let headers = HeaderMap::new();
        let id = resolve_request_id(&headers);
        assert_eq!(id.len(), 36, "expected UUID v4 length, got {id}");
    }

    #[test]
    fn preserves_inbound_request_id() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "trace-abc-123".parse().unwrap());
        let id = resolve_request_id(&headers);
        assert_eq!(id, "trace-abc-123");
    }

    #[test]
    fn ignores_empty_request_id() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "".parse().unwrap());
        let id = resolve_request_id(&headers);
        assert_eq!(id.len(), 36);
    }
}
