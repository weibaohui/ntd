//! GitCode 平台集成：仅保留 PAT 验证所需的 HTTP 调用。
//!
//! 贡献提交改为「ActionButton + 提示词」驱动：由 AI 执行器读取本地 PAT、
//! 调用 GitCode API 完成 fork/建分支/写文件/建 PR，后端不再实现这些确定性调用。
//! 这里只保留 `verify_pat`（保存 PAT 时验证其有效性）。

/// GitCode OpenAPI 基础地址（平台固定地址，不属于用户配置）。
const API_BASE: &str = "https://api.gitcode.com";

/// 验证 PAT 有效性：调用 `/user` 接口，返回 2xx 即视为有效。
///
/// 不解析 login（账号身份由 AI 执行时实时获取），这里只确认 PAT 能通过认证。
pub async fn verify_pat(pat: &str) -> Result<(), String> {
    let response = reqwest::Client::new()
        .get(format!("{API_BASE}/api/v5/user"))
        .bearer_auth(pat)
        .send()
        .await
        .map_err(|_| "请求用户信息失败".to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body_text = response
            .text()
            .await
            .map_err(|_| "读取用户响应失败".to_string())?;
        return Err(format!(
            "PAT 无效（HTTP {status}）: {}",
            truncate_for_log(&body_text)
        ));
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
