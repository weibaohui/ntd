//! 专家贡献 API 路由：PAT 配置（保存/查询/清除）。
//!
//! 提交动作由前端「ActionButton + 提示词」驱动，不经过这里；
//! 这里只负责 PAT 的验证与持久化。

use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::contribution::{gitcode, pat};
use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;

/// 配置态查询响应。
#[derive(Debug, Serialize)]
pub struct AuthStatus {
    /// 是否已配置 PAT
    pub configured: bool,
}

/// PAT 验证响应：PAT 所属账号的用户名（证明令牌当前可用）。
#[derive(Debug, Serialize)]
pub struct VerifyResult {
    /// 账号登录名（GitCode /user 的 login 字段，分享 prompt 的 {owner} 口径）。
    pub username: String,
    /// 显示名；上游缺失时等于 username（build_git_user 兜底，前端无需再判空）。
    pub name: String,
}

/// 保存 PAT 请求体。
#[derive(Deserialize)]
pub struct PatRequest {
    /// GitCode 个人访问令牌
    pub pat: String,
}

// 手动实现 Debug：PAT 脱敏，防止误用 {:?} 把明文 PAT 打进日志（与 PatCredential 一致）。
impl std::fmt::Debug for PatRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatRequest")
            .field("pat", &"[redacted]")
            .finish()
    }
}

/// `GET /api/v1/contribution/auth/status`：查询 PAT 配置态。
pub async fn auth_status() -> ApiResponse<AuthStatus> {
    // 本地存在 PAT 即视为已配置；不存 username，账号身份由 AI 执行时实时获取。
    ApiResponse::ok(AuthStatus {
        configured: pat::load().is_some(),
    })
}

/// `GET /api/v1/contribution/verify`：验证已保存 PAT 并返回所属账号用户名。
///
/// 与保存时验证的区别：保存验证只确认「能过认证」，这里把 `/user` 响应的
/// login 解析出来返回给前端展示，让用户肉眼确认 PAT 归属（验证入口在设置页）。
pub async fn verify() -> Result<ApiResponse<VerifyResult>, AppError> {
    // 未配置 PAT 时没有可验证对象，直接 400 提示先保存，避免对空凭据发起远程调用。
    let cred = pat::load().ok_or_else(|| AppError::BadRequest("尚未配置 PAT".to_string()))?;
    // 校验错误按原因分类映射：PAT 无效→400，网络/上游故障→500（避免误报诱导轮换令牌）。
    let user = gitcode::fetch_user(&cred.pat)
        .await
        .map_err(map_verify_error)?;
    Ok(ApiResponse::ok(VerifyResult {
        username: user.username,
        name: user.name,
    }))
}

/// `POST /api/v1/contribution/pat`：验证并保存 PAT。
pub async fn save_pat(Json(req): Json<PatRequest>) -> Result<ApiResponse<String>, AppError> {
    // 先校验 PAT 非空，避免对空串发起无意义的远程调用。
    let pat_trimmed = req.pat.trim();
    if pat_trimmed.is_empty() {
        return Err(AppError::BadRequest("PAT 不能为空".to_string()));
    }
    // 长度与字符集校验：reqwest 的 HeaderValue 不接受控制字符/超长值，提前拦截防 panic。
    validate_pat(pat_trimmed)?;
    // 用 PAT 调 /user 验证有效性（不落盘 username）；失败不落盘。
    // 校验错误按原因分类映射：PAT 无效→400，网络/上游故障→500（避免误报诱导轮换令牌）。
    gitcode::verify_pat(pat_trimmed)
        .await
        .map_err(map_verify_error)?;
    // 验证通过才持久化，避免写入无效 PAT。
    pat::save(&pat::PatCredential {
        pat: pat_trimmed.to_string(),
    })
    .map_err(AppError::Internal)?;
    Ok(ApiResponse::ok("已保存 PAT".to_string()))
}

/// `POST /api/v1/contribution/logout`：清除本地 PAT。
pub async fn logout() -> ApiResponse<String> {
    pat::clear();
    ApiResponse::ok("已清除 PAT".to_string())
}

/// PAT 长度上限：GitCode PAT 通常几十字符，放宽到 256 足够，防超长输入。
const PAT_MAX_LEN: usize = 256;

/// 校验 PAT 长度与字符集，防止恶意输入触发 reqwest HeaderValue panic 或请求失败。
fn validate_pat(pat: &str) -> Result<(), AppError> {
    if pat.chars().count() > PAT_MAX_LEN {
        return Err(AppError::BadRequest("PAT 过长".to_string()));
    }
    // 拒绝 ASCII 控制字符（如 \r\n）：HeaderValue 不接受，会 panic/报错。
    if pat.chars().any(|c| c.is_control()) {
        return Err(AppError::BadRequest("PAT 含非法字符".to_string()));
    }
    Ok(())
}

/// 把 PAT 校验错误映射为合适的 HTTP 状态码：
/// `Invalid`（PAT 被服务端拒绝）→ 400；`Upstream`（网络/上游故障）→ 500。
/// 抽成独立函数便于单测，并让 save_pat 保持简短。
fn map_verify_error(err: gitcode::VerifyError) -> AppError {
    match err {
        gitcode::VerifyError::Invalid(msg) => AppError::BadRequest(msg),
        gitcode::VerifyError::Upstream(msg) => AppError::Internal(msg),
    }
}

/// 专家贡献路由。
pub fn contribution_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/contribution/auth/status", get(auth_status))
        .route("/api/v1/contribution/verify", get(verify))
        .route("/api/v1/contribution/pat", post(save_pat))
        .route("/api/v1/contribution/logout", post(logout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_pat_accepts_normal_pat() {
        assert!(validate_pat("test-pat-123456").is_ok());
    }

    #[test]
    fn validate_pat_rejects_overlong() {
        let long = "a".repeat(PAT_MAX_LEN + 1);
        assert!(validate_pat(&long).is_err());
    }

    #[test]
    fn validate_pat_rejects_control_chars() {
        assert!(validate_pat("abc\r\n").is_err());
    }

    #[test]
    fn map_verify_error_invalid_to_bad_request() {
        // PAT 被服务端拒绝属用户输入问题，映射 400。
        let app = map_verify_error(gitcode::VerifyError::Invalid("bad pat".to_string()));
        assert!(matches!(app, AppError::BadRequest(msg) if msg == "bad pat"));
    }

    #[test]
    fn map_verify_error_upstream_to_internal() {
        // 网络/上游故障不是 PAT 本身问题，映射 500，避免误报诱导用户轮换令牌。
        let app = map_verify_error(gitcode::VerifyError::Upstream("timeout".to_string()));
        assert!(matches!(app, AppError::Internal(msg) if msg == "timeout"));
    }
}
