//! 专家贡献 API 路由：OAuth 登录、预览、提交 Issue。

use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::contribution::{gitcode, issue, oauth, GitCodeToken};
use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;

/// 登录态查询响应。
#[derive(Debug, Serialize)]
pub struct AuthStatus {
    /// 贡献功能是否启用（凭据是否已注入）
    pub enabled: bool,
    /// 是否已登录（本地已存在 token）
    pub logged_in: bool,
}

/// OAuth 授权 URL 响应。
#[derive(Debug, Serialize)]
pub struct OAuthUrlResponse {
    pub url: String,
}

/// OAuth 回调 query 参数。
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackParams {
    /// 授权码（用户拒绝授权时缺失）
    pub code: Option<String>,
    /// 防 CSRF 的随机串
    #[serde(default)]
    pub state: String,
    /// 授权失败原因（GitCode 回传）
    #[serde(default)]
    pub error: Option<String>,
}

/// 提交 Issue 请求体。
#[derive(Debug, Deserialize)]
pub struct SubmitRequest {
    /// 覆盖默认标题（用户预览时可改）
    pub title: Option<String>,
    /// 覆盖默认正文（用户预览时可改）
    pub body: Option<String>,
}

/// `GET /api/v1/contribution/auth/status`：查询登录态与功能开关。
pub async fn auth_status() -> ApiResponse<AuthStatus> {
    let enabled = gitcode::contribution_enabled();
    // 仅当凭据已注入且本地存在 token 才视为已登录；
    // token 是否过期在 submit 时再校验/刷新，避免此处额外 IO。
    let logged_in = enabled && oauth::load_token().is_some();
    ApiResponse::ok(AuthStatus { enabled, logged_in })
}

/// `GET /api/v1/contribution/oauth/url`：生成 GitCode 授权跳转 URL。
pub async fn oauth_url(
    State(state): State<AppState>,
) -> Result<ApiResponse<OAuthUrlResponse>, AppError> {
    // 未注入凭据时直接拒绝，避免生成不可用的授权链接。
    if !gitcode::contribution_enabled() {
        return Err(AppError::BadRequest("贡献功能未配置".to_string()));
    }
    let client_id = gitcode::CLIENT_ID.ok_or(AppError::Internal("client_id 缺失".to_string()))?;
    let redirect_uri = build_redirect_uri(&state);
    let oauth_state = oauth::generate_state();
    let url = gitcode::build_authorize_url(client_id, &redirect_uri, &oauth_state);
    Ok(ApiResponse::ok(OAuthUrlResponse { url }))
}

/// `GET /api/v1/contribution/oauth/callback`：处理 GitCode 回跳。
///
/// 始终返回 HTML（成功跳转、失败提示），因为这是浏览器直接访问的端点。
pub async fn oauth_callback(Query(params): Query<OAuthCallbackParams>) -> Response {
    // 用户拒绝授权：GitCode 会回传 error，code 为空。
    if let Some(err) = &params.error {
        return oauth_error_page(&format!("授权失败：{err}"));
    }
    // 校验 state，一次性消费，防 CSRF。
    if !oauth::consume_state(&params.state) {
        return oauth_error_page("登录校验失败（state 不匹配或已过期），请重新发起");
    }
    match gitcode::exchange_code_for_token(params.code.as_deref().unwrap_or_default()).await {
        Ok(token) => match oauth::save_token(&token) {
            Ok(()) => Redirect::to("/#/experts").into_response(),
            Err(e) => oauth_error_page(&format!("保存登录态失败：{e}")),
        },
        Err(e) => {
            // e 已在 gitcode.rs 脱敏（仅含 HTTP 状态码），可安全记日志；页面用固定文案，
            // 不把底层错误原文（可能含授权码）回显给浏览器。
            tracing::warn!("OAuth 换取 token 失败: {e}");
            oauth_error_page("换取登录态失败，请重新发起登录")
        }
    }
}

/// `POST /api/v1/contribution/experts/{name}/preview`：组装 Issue 草稿（不提交）。
pub async fn preview(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<issue::IssueDraft>, AppError> {
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;
    let draft = issue::build_issue_draft(&expert).map_err(AppError::Internal)?;
    Ok(ApiResponse::ok(draft))
}

/// `POST /api/v1/contribution/experts/{name}/submit`：提交 Issue。
pub async fn submit(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<SubmitRequest>,
) -> Result<ApiResponse<gitcode::IssueResult>, AppError> {
    // 1. 组装默认草稿；用户预览时可能覆盖标题/正文。
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;
    let draft = issue::build_issue_draft(&expert).map_err(AppError::Internal)?;
    let title = req.title.unwrap_or(draft.title);
    let body = req.body.unwrap_or(draft.body);
    // 校验覆盖字段长度：防止绕过「打包本地文件」向官方仓库提交任意超长内容。
    validate_submit_payload(&title, &body)?;

    // 2. 从 bundled url 解析贡献目标仓库。
    let (owner, repo) = resolve_target(&state)?;

    // 3. 取有效 token（过期自动刷新）。
    let token = get_valid_token().await?;

    // 4. 调用 GitCode 创建 Issue。
    let result = gitcode::create_issue(&owner, &repo, &token.access_token, &title, &body)
        .await
        .map_err(AppError::Internal)?;

    Ok(ApiResponse::ok(result))
}

/// `POST /api/v1/contribution/logout`：清除本地登录态。
pub async fn logout() -> ApiResponse<String> {
    oauth::clear_token();
    ApiResponse::ok("已退出登录".to_string())
}

/// Issue 标题长度上限（在 GitCode 标题约束内）。
const TITLE_MAX_CHARS: usize = 255;
/// Issue 正文长度上限：足以容纳完整专家文件，同时防滥用超长提交。
const BODY_MAX_CHARS: usize = 100_000;

/// 校验提交字段长度，防止绕过「打包本地文件」提交任意内容。
fn validate_submit_payload(title: &str, body: &str) -> Result<(), AppError> {
    if title.chars().count() > TITLE_MAX_CHARS {
        return Err(AppError::BadRequest("标题过长（最多 255 字）".to_string()));
    }
    if body.chars().count() > BODY_MAX_CHARS {
        return Err(AppError::BadRequest("正文过长".to_string()));
    }
    Ok(())
}

/// 从 config 的 bundled url 解析贡献目标 owner/repo。
fn resolve_target(state: &AppState) -> Result<(String, String), AppError> {
    let url = state.config_snapshot(|c| c.bundled_source.url.clone());
    gitcode::parse_owner_repo(&url)
        .ok_or_else(|| AppError::Internal(format!("无法从 bundled url 解析 owner/repo: {url}")))
}

/// 取有效 token：有效直接返回，过期则用 refresh_token 刷新并持久化。
async fn get_valid_token() -> Result<GitCodeToken, AppError> {
    let token = oauth::load_token()
        .ok_or_else(|| AppError::Forbidden("未登录，请先登录 GitCode".to_string()))?;
    if oauth::token_is_valid(&token) {
        return Ok(token);
    }
    let refreshed = gitcode::refresh_access_token(&token.refresh_token)
        .await
        .map_err(|_| AppError::Forbidden("登录态已过期，请重新登录".to_string()))?;
    oauth::save_token(&refreshed).map_err(AppError::Internal)?;
    Ok(refreshed)
}

/// 拼 OAuth 回调地址：本机 + 当前端口。
///
/// 用 `127.0.0.1` 而非 `localhost`：与 GitCode OAuth 应用注册的 redirect_uri
/// 保持一致（GitCode 对 localhost 形式校验不通过）；路径 `/api/v1/gitcode/oauth/callback`
/// 也与注册值逐字符对齐，否则 authorize 阶段会报「回调地址错误」。
fn build_redirect_uri(state: &AppState) -> String {
    let port = state.config_snapshot(|c| c.port);
    format!("http://127.0.0.1:{port}/api/v1/gitcode/oauth/callback")
}

/// 构造 OAuth 失败提示页（HTML）。
fn oauth_error_page(msg: &str) -> Response {
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>登录失败</title></head>\
         <body style=\"font-family:sans-serif;padding:40px\">\
         <h2>GitCode 登录失败</h2><p>{}</p>\
         <p><a href=\"/#/experts\">返回专家页</a></p></body></html>",
        html_escape(msg)
    );
    Html(html).into_response()
}

/// 简单 HTML 转义，防止错误信息注入标签。
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 专家贡献路由。
pub fn contribution_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/contribution/auth/status", get(auth_status))
        .route("/api/v1/contribution/oauth/url", get(oauth_url))
        .route("/api/v1/gitcode/oauth/callback", get(oauth_callback))
        .route(
            "/api/v1/contribution/experts/{name}/preview",
            post(preview),
        )
        .route(
            "/api/v1/contribution/experts/{name}/submit",
            post(submit),
        )
        .route("/api/v1/contribution/logout", post(logout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escape_escapes_special_chars() {
        assert_eq!(html_escape("<b>&</b>"), "&lt;b&gt;&amp;&lt;/b&gt;");
    }

    #[test]
    fn html_escape_passthrough_plain_text() {
        assert_eq!(html_escape("hello world"), "hello world");
    }

    #[test]
    fn validate_submit_payload_accepts_within_limits() {
        assert!(validate_submit_payload("ok", "body").is_ok());
    }

    #[test]
    fn validate_submit_payload_rejects_overlong_title_and_body() {
        let long_title = "x".repeat(TITLE_MAX_CHARS + 1);
        assert!(validate_submit_payload(&long_title, "body").is_err());

        let long_body = "y".repeat(BODY_MAX_CHARS + 1);
        assert!(validate_submit_payload("title", &long_body).is_err());
    }
}
