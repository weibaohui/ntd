//! GitCode 平台集成：PAT 验证与账号信息获取所需的 HTTP 调用。
//!
//! 贡献提交改为「ActionButton + 提示词」驱动：由 AI 执行器读取本地 PAT、
//! 调用 GitCode API 完成 fork/建分支/写文件/建 PR，后端不再实现这些确定性调用。
//! 这里只保留 `verify_pat`（保存 PAT 时验证其有效性）与
//! `fetch_user`（设置页「验证」按钮获取 PAT 所属账号用户名）。

use std::time::Duration;

use serde::{Deserialize, Serialize};

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
#[derive(Debug)]
pub enum VerifyError {
    /// PAT 被服务端拒绝（HTTP 非 2xx）—— 用户输入问题。
    Invalid(String),
    /// 网络/上游故障（连接失败、超时、响应读取失败）—— 非 PAT 本身问题。
    Upstream(String),
}

/// GitCode `/user` 接口返回的账号信息（设置页「验证」按钮的展示数据）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitUser {
    /// 账号登录名（username），PAT 归属的直接证据。
    pub username: String,
    /// 显示名；上游缺失时与 username 相同，保证前端字段恒有值。
    pub name: String,
}

/// `/user` 响应里真正用到的字段（serde 忽略其余未知字段，上游加字段不破坏解析）。
#[derive(Debug, Deserialize)]
struct UserResponse {
    /// 账号登录名（GitCode 文档确认字段，与分享 prompt 里 {owner} 的口径一致）。
    login: Option<String>,
    /// 显示名；login 缺失时的回退项。
    name: Option<String>,
    /// 账号 id；实测为十六进制字符串（如 "68ce..."），故按 String 解析；
    /// login/name 都缺失时的最终回退项（此时展示 #id 至少可辨识归属）。
    id: Option<String>,
}

/// 验证 PAT 有效性：调用 `/user` 接口，返回 2xx 即视为有效。
///
/// 不解析 login（账号身份由 AI 执行时实时获取），这里只确认 PAT 能通过认证。
/// 带超时的客户端确保 GitCode 慢/挂时不会无限阻塞调用方。
pub async fn verify_pat(pat: &str) -> Result<(), VerifyError> {
    let client = build_client()?;
    let response = client
        .get(format!("{API_BASE}/api/v5/user"))
        .bearer_auth(pat)
        .send()
        .await
        .map_err(|e| VerifyError::Upstream(format!("请求用户信息失败: {e}")))?;
    ensure_success(response).await?;
    Ok(())
}

/// 获取 PAT 所属账号的用户名：调用 `/user` 并解析 `login`（回退 name / #id）。
///
/// 与 `verify_pat` 的区别：这里把响应体解析成账号信息返回，供设置页展示
/// 「验证通过：@xxx」，让用户肉眼确认 PAT 归属，而不是只得到一个布尔结果。
pub async fn fetch_user(pat: &str) -> Result<GitUser, VerifyError> {
    let client = build_client()?;
    let response = client
        .get(format!("{API_BASE}/api/v5/user"))
        .bearer_auth(pat)
        .send()
        .await
        .map_err(|e| VerifyError::Upstream(format!("请求用户信息失败: {e}")))?;
    let body = ensure_success(response).await?;
    // 解析失败（上游结构变化）属上游故障而非 PAT 无效：不诱导用户轮换令牌。
    let user: UserResponse = serde_json::from_str(&body)
        .map_err(|e| VerifyError::Upstream(format!("解析用户信息失败: {e}")))?;
    Ok(build_git_user(user))
}

/// 构造带超时的 HTTP 客户端（verify_pat 与 fetch_user 共用同一套超时参数）。
fn build_client() -> Result<reqwest::Client, VerifyError> {
    // builder 失败极罕见（仅非法配置），归为上游故障。
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| VerifyError::Upstream(format!("构造 HTTP 客户端失败: {e}")))
}

/// 非 2xx 响应统一转 `VerifyError::Invalid`；2xx 返回响应体文本（供调用方解析）。
///
/// 非 2xx 尝试读 body 丰富错误信息，读失败也不改「无效」结论。
async fn ensure_success(response: reqwest::Response) -> Result<String, VerifyError> {
    let status = response.status();
    if !status.is_success() {
        let detail = response
            .text()
            .await
            .map(|t| truncate_for_log(&t))
            .unwrap_or_else(|_| "(无法读取响应体)".to_string());
        return Err(VerifyError::Invalid(format!(
            "PAT 无效（HTTP {status}）: {detail}"
        )));
    }
    response
        .text()
        .await
        .map_err(|e| VerifyError::Upstream(format!("读取响应体失败: {e}")))
}

/// 把 `/user` 响应字段收敛为展示用 `GitUser`：
/// login 优先（分享 prompt 的 {owner} 口径），name 次之，最后 #id（至少可辨识归属）。
fn build_git_user(user: UserResponse) -> GitUser {
    // name 先取出（Option），避免后续 or(user.name) 把字段 move 走导致无法再用。
    let name = user.name;
    let username = user
        .login
        .or(name.clone())
        .or_else(|| user.id.map(|id| format!("#{id}")))
        .unwrap_or_else(|| "unknown".to_string());
    GitUser {
        name: name.unwrap_or_else(|| username.clone()),
        username,
    }
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

    #[test]
    fn build_git_user_prefers_login() {
        // login 存在时优先用 login（分享 prompt 的 {owner} 口径），name 独立保留。
        let user = build_git_user(UserResponse {
            login: Some("alice".to_string()),
            name: Some("Alice".to_string()),
            id: Some("68ce".to_string()),
        });
        assert_eq!(user.username, "alice");
        assert_eq!(user.name, "Alice");
    }

    #[test]
    fn build_git_user_falls_back_to_name() {
        // 上游未返回 login 时回退 name，保证 username 恒有值。
        let user = build_git_user(UserResponse {
            login: None,
            name: Some("Bob".to_string()),
            id: Some("abc123".to_string()),
        });
        assert_eq!(user.username, "Bob");
    }

    #[test]
    fn build_git_user_falls_back_to_id() {
        // login/name 都缺失时回退 #id，至少可辨识 PAT 归属账号。
        let user = build_git_user(UserResponse {
            login: None,
            name: None,
            id: Some("abc123".to_string()),
        });
        assert_eq!(user.username, "#abc123");
    }

    #[test]
    fn build_git_user_all_missing_falls_back_unknown() {
        // 极端情况（字段全缺）也不 panic，用 unknown 占位。
        let user = build_git_user(UserResponse {
            login: None,
            name: None,
            id: None,
        });
        assert_eq!(user.username, "unknown");
    }
}
