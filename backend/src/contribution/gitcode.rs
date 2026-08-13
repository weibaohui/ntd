//! GitCode 平台集成：仅保留 PAT 验证所需的 HTTP 调用。
//!
//! 贡献提交改为「ActionButton + 提示词」驱动：由 AI 执行器读取本地 PAT、
//! 调用 GitCode API 完成 fork/建分支/写文件/建 PR，后端不再实现这些确定性调用。
//! 这里只保留 `verify_pat`（保存 PAT 时验证其有效性）。

use std::time::Duration;

/// GitCode OpenAPI 基础地址（平台固定地址，不属于用户配置）。
const API_BASE: &str = "https://api.gitcode.com";

/// 建连超时：GitCode 不可达时尽快失败，避免 TCP 层长等待。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 整体请求超时：覆盖建连 + 读响应，兜底任何慢响应，防止 handler 无限阻塞。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// PAT 校验失败的原因分类。
///
/// 区分两类是为了给前端正确的语义：
/// - [`VerifyError::Invalid`] 是用户输入问题（PAT 被服务端拒绝），映射 HTTP 400；
/// - [`VerifyError::Upstream`] 是网络/上游故障（连不上、超时、读响应失败），映射 HTTP 500，
///   避免把「GitCode 抽风」误报成「PAT 无效」诱导用户无谓轮换令牌。
pub enum VerifyError {
    /// PAT 被服务端拒绝（HTTP 非 2xx）—— 用户输入问题。
    Invalid(String),
    /// 网络/上游故障（连接失败、超时、响应读取失败）—— 非 PAT 本身问题。
    Upstream(String),
}

/// 验证 PAT 有效性：调用 `/user` 接口，返回 2xx 即视为有效。
///
/// 不解析 login（账号身份由 AI 执行时实时获取），这里只确认 PAT 能通过认证。
/// 带超时的客户端确保 GitCode 慢/挂时不会无限阻塞调用方。
pub async fn verify_pat(pat: &str) -> Result<(), VerifyError> {
    // 构造带超时的 Client；builder 失败极罕见（仅非法配置），归为上游故障。
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| VerifyError::Upstream(format!("构造 HTTP 客户端失败: {e}")))?;
    let response = client
        .get(format!("{API_BASE}/api/v5/user"))
        .bearer_auth(pat)
        .send()
        .await
        // 连接失败/超时/DNS 等都属上游可达性问题，与 PAT 本身是否有效无关。
        // reqwest 错误体只含 URL（PAT 在 Authorization 头，不会出现），可安全透传便于排障。
        .map_err(|e| VerifyError::Upstream(format!("请求用户信息失败: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        // 非 2xx → PAT 被拒绝；尝试读 body 丰富错误信息，读失败也不改「无效」结论。
        let detail = response
            .text()
            .await
            .map(|t| truncate_for_log(&t))
            .unwrap_or_else(|_| "(无法读取响应体)".to_string());
        return Err(VerifyError::Invalid(format!(
            "PAT 无效（HTTP {status}）: {detail}"
        )));
    }
    Ok(())
}

/// 截断日志/错误信息里的响应体，避免超长 body 刷屏。
fn truncate_for_log(s: &str) -> String {
    const MAX: usize = 200;
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let head: String = s.chars().take(MAX).collect();
    format!("{head}...（已截断，共 {} 字符）", s.chars().count())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn truncate_for_log_truncates_long_text() {
        let long = "x".repeat(300);
        let out = truncate_for_log(&long);
        assert!(out.contains("已截断"));
        assert!(out.chars().count() < 300);
    }
}
