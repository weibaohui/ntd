//! GitCode 平台集成：凭据常量、owner/repo 解析、OAuth 与 Issue 的 HTTP 调用。

use serde::Deserialize;

use super::{now_unix, GitCodeToken};

/// 编译期注入的 OAuth `client_id`：未设置环境变量 `NTD_CONTRIB_CLIENT_ID` 时为 `None`。
///
/// 使用 `option_env!` 而非 `env!`：凭据缺失时编译通过、功能降级禁用，
/// 不会因为没配置凭据而编译失败。
pub const CLIENT_ID: Option<&str> = option_env!("NTD_CONTRIB_CLIENT_ID");
/// 编译期注入的 OAuth `client_secret`：同上，未设置为 `None`。
pub const CLIENT_SECRET: Option<&str> = option_env!("NTD_CONTRIB_CLIENT_SECRET");

/// GitCode OAuth 授权端点（平台固定地址，不属于用户配置）。
const AUTHORIZE_URL: &str = "https://gitcode.com/oauth/authorize";
/// GitCode OAuth token 端点。
const TOKEN_URL: &str = "https://gitcode.com/oauth/token";
/// GitCode OpenAPI 基础地址。
const API_BASE: &str = "https://api.gitcode.com";
/// 创建贡献 Issue 时打上的固定标签，便于官方仓库后台筛选。
const CONTRIBUTION_LABEL: &str = "expert-contribution";
/// 创建 Issue 时申请的 OAuth 权限范围：仅 issue，遵循最小权限。
const ISSUE_SCOPE: &str = "all_issue";

/// 贡献功能是否启用：`client_id` 与 `client_secret` 均已注入。
pub fn contribution_enabled() -> bool {
    CLIENT_ID.is_some() && CLIENT_SECRET.is_some()
}

/// 从 bundled 仓库 `url` 解析 `(owner, repo)`。
///
/// 支持 `https://gitcode.com/weibaohui/ntd-resource.git` 与不带 `.git` 的形式；
/// 解析失败（非 http(s)、段数不足、owner/repo 为空）返回 `None`。
pub fn parse_owner_repo(url: &str) -> Option<(String, String)> {
    // 仅处理 http(s)，剥离协议前缀。
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    // 按 '/' 切分，过滤空段；末尾两段分别为 owner 与 repo。
    let segments: Vec<&str> = without_scheme
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    // 至少需要 host + owner + repo 三段。
    if segments.len() < 3 {
        return None;
    }
    let owner = segments[segments.len() - 2].to_string();
    let repo = segments[segments.len() - 1].trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

/// 拼装 OAuth 授权跳转 URL（query 参数由 `reqwest::Url` 自动做 URL 编码）。
pub fn build_authorize_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    // AUTHORIZE_URL 是编译期常量，parse 失败只可能是代码错误；
    // 用 let-else 返回空串，避免生产代码使用 expect/panic。
    let Ok(mut url) = reqwest::Url::parse(AUTHORIZE_URL) else {
        return String::new();
    };
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", ISSUE_SCOPE)
        .append_pair("state", state);
    url.to_string()
}

/// GitCode token 接口响应体。
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    scope: Option<String>,
}

impl TokenResponse {
    /// 把 token 接口响应转换为带绝对过期时间的 `GitCodeToken`。
    fn into_token(self, now: i64) -> GitCodeToken {
        GitCodeToken {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            // 绝对过期时间 = 当前时间 + 相对有效期；用绝对值便于持久化后离线判断。
            expires_at: now + self.expires_in,
            scope: self.scope,
        }
    }
}

/// 用授权码换取访问令牌。
///
/// GitCode 约定：`grant_type`/`code`/`client_id` 走 query，`client_secret` 走 form-data。
pub async fn exchange_code_for_token(code: &str) -> Result<GitCodeToken, String> {
    let client_id = CLIENT_ID.ok_or_else(|| "未配置 client_id".to_string())?;
    let client_secret = CLIENT_SECRET.ok_or_else(|| "未配置 client_secret".to_string())?;
    let now = now_unix();
    let response = reqwest::Client::new()
        .post(TOKEN_URL)
        .query(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", client_id),
        ])
        .form(&[("client_secret", client_secret)])
        .send()
        .await
        .map_err(|_| "请求 token 端点失败".to_string())?;
    // 用 status 而非 error_for_status 的错误原文：后者 Display 含带 code 的完整 URL，
    // 若透传会把一次性授权码泄露到前端页面/日志。
    let status = response.status();
    if !status.is_success() {
        return Err(format!("换取 token 失败（HTTP {status}）"));
    }
    let resp: TokenResponse = response
        .json()
        .await
        .map_err(|_| "解析 token 响应失败".to_string())?;
    Ok(resp.into_token(now))
}

/// 用 refresh_token 刷新访问令牌。
pub async fn refresh_access_token(refresh_token: &str) -> Result<GitCodeToken, String> {
    let now = now_unix();
    let response = reqwest::Client::new()
        .post(TOKEN_URL)
        .query(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await
        .map_err(|_| "请求刷新 token 失败".to_string())?;
    // 同上：refresh_token 也在 query 里，错误原文不得透传。
    let status = response.status();
    if !status.is_success() {
        return Err(format!("刷新 token 失败（HTTP {status}）"));
    }
    let resp: TokenResponse = response
        .json()
        .await
        .map_err(|_| "解析刷新响应失败".to_string())?;
    Ok(resp.into_token(now))
}

/// GitCode 创建 Issue 响应体（字段名以真实 API 为准，用 Option 容错）。
#[derive(Debug, Deserialize)]
struct CreateIssueResponse {
    number: Option<i64>,
    html_url: Option<String>,
    url: Option<String>,
    title: Option<String>,
}

/// 创建 Issue 的结果（返回给前端）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct IssueResult {
    /// Issue 编号
    pub issue_number: i64,
    /// Issue 网页链接
    pub issue_url: String,
    /// Issue 标题
    pub title: String,
}

/// 调用 GitCode API 创建 Issue。
///
/// 端点歧义说明：官方文档「创建 Issue」标题写作 `/repos/:owner/issues`，
/// 其余 Issue 接口均为 `/repos/:owner/:repo/issues`；此处采用与其余接口一致的
/// `/repos/{owner}/{repo}/issues`，若真实请求 404 再据验证结果调整。
/// `labels` 沿用 Gitee `/api/v5` 语义（逗号分隔字符串），真实验证时确认。
pub async fn create_issue(
    owner: &str,
    repo: &str,
    access_token: &str,
    title: &str,
    body: &str,
) -> Result<IssueResult, String> {
    let resp: CreateIssueResponse = reqwest::Client::new()
        .post(format!("{API_BASE}/api/v5/repos/{owner}/{repo}/issues"))
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "title": title,
            "body": body,
            "labels": CONTRIBUTION_LABEL,
        }))
        .send()
        .await
        .map_err(|e| format!("请求创建 Issue 失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("创建 Issue 失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析 Issue 响应失败: {e}"))?;

    // 优先用官方返回的链接，缺失时用编号拼网页链接兜底。
    let number = resp.number.unwrap_or(0);
    let issue_url = resp
        .html_url
        .or(resp.url)
        .unwrap_or_else(|| format!("https://gitcode.com/{owner}/{repo}/issues/{number}"));
    let title = resp.title.unwrap_or_else(|| title.to_string());
    Ok(IssueResult {
        issue_number: number,
        issue_url,
        title,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_owner_repo_https_with_git() {
        let (owner, repo) =
            parse_owner_repo("https://gitcode.com/weibaohui/ntd-resource.git").unwrap();
        assert_eq!(owner, "weibaohui");
        assert_eq!(repo, "ntd-resource");
    }

    #[test]
    fn parse_owner_repo_https_without_git() {
        let (owner, repo) = parse_owner_repo("https://gitcode.com/weibaohui/ntd-resource").unwrap();
        assert_eq!(owner, "weibaohui");
        assert_eq!(repo, "ntd-resource");
    }

    #[test]
    fn parse_owner_repo_http() {
        let (owner, repo) = parse_owner_repo("http://gitcode.com/a/b").unwrap();
        assert_eq!(owner, "a");
        assert_eq!(repo, "b");
    }

    #[test]
    fn parse_owner_repo_nested_path_uses_last_two() {
        let (owner, repo) = parse_owner_repo("https://host/org/group/repo.git").unwrap();
        assert_eq!(owner, "group");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_owner_repo_rejects_ssh_and_invalid() {
        assert!(parse_owner_repo("git@gitcode.com:weibaohui/ntd-resource.git").is_none());
        assert!(parse_owner_repo("").is_none());
        assert!(parse_owner_repo("https://host/onlyowner").is_none());
        assert!(parse_owner_repo("https://host/").is_none());
    }

    #[test]
    fn build_authorize_url_contains_expected_params() {
        let url = build_authorize_url("cid", "http://localhost:18088/cb", "s123");
        assert!(url.starts_with("https://gitcode.com/oauth/authorize?"));
        assert!(url.contains("client_id=cid"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope=all_issue"));
        assert!(url.contains("state=s123"));
    }
}
