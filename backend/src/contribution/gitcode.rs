//! GitCode 平台集成：凭据常量、owner/repo 解析、OAuth 与 Issue 的 HTTP 调用。

use serde::Deserialize;

use super::{now_unix, GitCodeToken};

/// 编译期注入的 OAuth `client_id`：未设置环境变量 `NTD_GITCODE_CLIENT_ID` 时为 `None`。
///
/// 使用 `option_env!` 而非 `env!`：凭据缺失时编译通过、功能降级禁用，
/// 不会因为没配置凭据而编译失败。
pub const CLIENT_ID: Option<&str> = option_env!("NTD_GITCODE_CLIENT_ID");
/// 编译期注入的 OAuth `client_secret`：同上，未设置为 `None`。
pub const CLIENT_SECRET: Option<&str> = option_env!("NTD_GITCODE_CLIENT_SECRET");

/// GitCode OAuth 授权端点（平台固定地址，不属于用户配置）。
const AUTHORIZE_URL: &str = "https://gitcode.com/oauth/authorize";
/// GitCode OAuth token 端点。
const TOKEN_URL: &str = "https://gitcode.com/oauth/token";
/// GitCode OpenAPI 基础地址。
const API_BASE: &str = "https://api.gitcode.com";
/// 创建贡献 Issue 时打上的固定标签，便于官方仓库后台筛选。
const CONTRIBUTION_LABEL: &str = "expert-contribution";
/// 发起贡献 PR 所需的 OAuth 权限范围：用户信息 + 仓库读写（fork/写文件/建分支）+ PR。
/// 比纯 issue 更宽，因为 PR 需要 fork 官方仓库并向 fork 写入专家文件。
const PR_SCOPE: &str = "all_user all_projects all_pr all_repository";

/// 贡献功能是否启用：`client_id` 与 `client_secret` 均已注入且非空。
///
/// 判空原因：CI 在 pull_request 事件下 `secrets` 会展开为空字符串，
/// 此时 `option_env!` 读到 `Some("")` 而非 `None`；若不判空会误判 enabled=true。
pub fn contribution_enabled() -> bool {
    credential_present(CLIENT_ID) && credential_present(CLIENT_SECRET)
}

/// 判断单个凭据是否有效注入（`Some` 且非空）。
///
/// 抽成纯函数便于单测：`option_env!` 的结果在编译期固化，无法在测试里改变，
/// 但判空逻辑本身可独立验证。
fn credential_present(value: Option<&str>) -> bool {
    matches!(value, Some(v) if !v.is_empty())
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
        .append_pair("scope", PR_SCOPE)
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

/// 创建 Issue 的结果（返回给前端）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct IssueResult {
    /// Issue 编号（GitCode 沿用 Gitee 语义，编号是字符串，如 "I5YJX2"）
    pub issue_number: String,
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
    let response = reqwest::Client::new()
        .post(format!("{API_BASE}/api/v5/repos/{owner}/{repo}/issues"))
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "title": title,
            "body": body,
            "labels": CONTRIBUTION_LABEL,
        }))
        .send()
        .await
        .map_err(|_| "请求创建 Issue 失败".to_string())?;

    let status = response.status();
    // 先取原始文本：非 2xx 或非 JSON 时能带上 body 帮助定位（响应是用户自己的 issue，不含凭据）。
    let body_text = response
        .text()
        .await
        .map_err(|_| "读取 Issue 响应失败".to_string())?;

    if !status.is_success() {
        return Err(format!(
            "创建 Issue 失败（HTTP {status}）: {}",
            truncate_for_log(&body_text)
        ));
    }

    // 解析为通用 JSON 再宽松提取字段；GitCode 的 number 是字符串，不能用 i64 强反序列化。
    let resp: serde_json::Value = serde_json::from_str(&body_text).map_err(|_| {
        format!("解析 Issue 响应失败（非 JSON）: {}", truncate_for_log(&body_text))
    })?;

    let issue_number = extract_issue_number(&resp);
    let issue_url = extract_issue_url(&resp, owner, repo, &issue_number);
    let title = resp
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(title)
        .to_string();

    Ok(IssueResult {
        issue_number,
        issue_url,
        title,
    })
}

/// 从响应里提取 Issue 编号：优先 `number`，其次 `id`；数字或字符串都兼容。
fn extract_issue_number(resp: &serde_json::Value) -> String {
    resp.get("number")
        .and_then(number_value_to_string)
        .or_else(|| resp.get("id").and_then(number_value_to_string))
        .unwrap_or_else(|| "?".to_string())
}

/// 把数字或字符串类型的 JSON 值统一转为字符串（GitCode 的 number 是字符串）。
fn number_value_to_string(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    v.as_i64().map(|n| n.to_string())
}

/// 提取 Issue 网页链接：优先官方返回的 html_url / url，缺失时用编号拼兜底。
fn extract_issue_url(resp: &serde_json::Value, owner: &str, repo: &str, number: &str) -> String {
    resp.get("html_url")
        .and_then(|v| v.as_str())
        .or_else(|| resp.get("url").and_then(|v| v.as_str()))
        .map(String::from)
        .unwrap_or_else(|| format!("https://gitcode.com/{owner}/{repo}/issues/{number}"))
}

/// 创建 PR 的结果（返回给前端）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PrResult {
    /// PR 编号
    pub pr_number: String,
    /// PR 网页链接
    pub pr_url: String,
    /// PR 标题
    pub title: String,
}

/// 获取当前授权用户的 username（作为 fork 后的 owner）。
pub async fn get_current_username(access_token: &str) -> Result<String, String> {
    let response = reqwest::Client::new()
        .get(format!("{API_BASE}/api/v5/user"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| "请求用户信息失败".to_string())?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|_| "读取用户响应失败".to_string())?;
    if !status.is_success() {
        return Err(format!(
            "获取用户信息失败（HTTP {status}）: {}",
            truncate_for_log(&body_text)
        ));
    }
    let resp: serde_json::Value =
        serde_json::from_str(&body_text).map_err(|_| "解析用户响应失败".to_string())?;
    resp.get("login")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "用户响应缺少 login 字段".to_string())
}

/// 确保官方仓库已 fork 到当前用户：fork 成功或「已 fork」都视为就绪。
pub async fn ensure_fork(access_token: &str, owner: &str, repo: &str) -> Result<(), String> {
    let response = reqwest::Client::new()
        .post(format!("{API_BASE}/api/v5/repos/{owner}/{repo}/forks"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| "请求 fork 仓库失败".to_string())?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|_| "读取 fork 响应失败".to_string())?;
    if status.is_success() {
        return Ok(());
    }
    // 已 fork 过（409/422）视为成功，复用现有 fork；其余错误上抛。
    if status == reqwest::StatusCode::CONFLICT || status == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
        return Ok(());
    }
    Err(format!(
        "fork 仓库失败（HTTP {status}）: {}",
        truncate_for_log(&body_text)
    ))
}

/// 在仓库上创建分支。
pub async fn create_branch(
    access_token: &str,
    owner: &str,
    repo: &str,
    branch_name: &str,
    refs: &str,
) -> Result<(), String> {
    let response = reqwest::Client::new()
        .post(format!("{API_BASE}/api/v5/repos/{owner}/{repo}/branches"))
        .bearer_auth(access_token)
        .json(&serde_json::json!({"branch_name": branch_name, "refs": refs}))
        .send()
        .await
        .map_err(|_| "请求创建分支失败".to_string())?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|_| "读取建分支响应失败".to_string())?;
    if !status.is_success() {
        return Err(format!(
            "创建分支 {branch_name} 失败（HTTP {status}）: {}",
            truncate_for_log(&body_text)
        ));
    }
    Ok(())
}

/// 向仓库分支写入一个文件（content 为 base64 编码的字节内容）。
pub async fn create_file(
    access_token: &str,
    owner: &str,
    repo: &str,
    branch: &str,
    path: &str,
    content_base64: &str,
    message: &str,
) -> Result<(), String> {
    let url = contents_url(owner, repo, path)?;
    let response = reqwest::Client::new()
        .post(&url)
        .bearer_auth(access_token)
        .form(&[
            ("content", content_base64),
            ("message", message),
            ("branch", branch),
        ])
        .send()
        .await
        .map_err(|_| "请求写入文件失败".to_string())?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|_| "读取写文件响应失败".to_string())?;
    if !status.is_success() {
        return Err(format!(
            "写入文件 {path} 失败（HTTP {status}）: {}",
            truncate_for_log(&body_text)
        ));
    }
    Ok(())
}

/// 构造 contents API 完整 URL，path 各段做 URL 编码（防空格/中文等破坏 URL）。
fn contents_url(owner: &str, repo: &str, path: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse(&format!("{API_BASE}/api/v5/repos/{owner}/{repo}/contents"))
        .map_err(|_| "构造 contents URL 失败".to_string())?;
    {
        let mut segs = url
            .path_segments_mut()
            .map_err(|_| "无法修改 URL 路径".to_string())?;
        for seg in path.split('/') {
            segs.push(seg);
        }
    }
    Ok(url.to_string())
}

/// 创建 Pull Request：head 为源（fork 的 `owner:branch`），base 为目标分支。
pub async fn create_pr(
    access_token: &str,
    owner: &str,
    repo: &str,
    title: &str,
    body: &str,
    head: &str,
    base: &str,
) -> Result<PrResult, String> {
    let response = reqwest::Client::new()
        .post(format!("{API_BASE}/api/v5/repos/{owner}/{repo}/pulls"))
        .bearer_auth(access_token)
        .json(&serde_json::json!({"title": title, "body": body, "head": head, "base": base}))
        .send()
        .await
        .map_err(|_| "请求创建 PR 失败".to_string())?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|_| "读取 PR 响应失败".to_string())?;
    if !status.is_success() {
        return Err(format!(
            "创建 PR 失败（HTTP {status}）: {}",
            truncate_for_log(&body_text)
        ));
    }
    let resp: serde_json::Value =
        serde_json::from_str(&body_text).map_err(|_| "解析 PR 响应失败".to_string())?;
    // number 提取逻辑与 issue 通用（兼容数字/字符串），复用同一函数。
    let pr_number = extract_issue_number(&resp);
    let pr_url = resp
        .get("html_url")
        .and_then(|v| v.as_str())
        .or_else(|| resp.get("url").and_then(|v| v.as_str()))
        .map(String::from)
        .unwrap_or_else(|| format!("https://gitcode.com/{owner}/{repo}/pulls/{pr_number}"));
    let title = resp
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(title)
        .to_string();
    Ok(PrResult {
        pr_number,
        pr_url,
        title,
    })
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
        // scope 已改为 PR 所需权限组合，值经 URL 编码（空格变 +），只断言前缀。
        assert!(url.contains("scope=all_user"));
        assert!(url.contains("state=s123"));
    }

    #[test]
    fn credential_present_requires_some_and_non_empty() {
        assert!(credential_present(Some("x")));
        assert!(!credential_present(Some("")));
        assert!(!credential_present(None));
    }

    #[test]
    fn extract_issue_number_prefers_string_number() {
        let v = serde_json::json!({"number": "I5YJX2"});
        assert_eq!(extract_issue_number(&v), "I5YJX2");
    }

    #[test]
    fn extract_issue_number_accepts_numeric_number() {
        let v = serde_json::json!({"number": 123});
        assert_eq!(extract_issue_number(&v), "123");
    }

    #[test]
    fn extract_issue_number_falls_back_to_id() {
        let v = serde_json::json!({"id": 456});
        assert_eq!(extract_issue_number(&v), "456");
    }

    #[test]
    fn extract_issue_number_missing_returns_placeholder() {
        let v = serde_json::json!({});
        assert_eq!(extract_issue_number(&v), "?");
    }

    #[test]
    fn extract_issue_url_prefers_html_url() {
        let v = serde_json::json!({"html_url": "https://gitcode.com/o/r/issues/I1"});
        assert_eq!(
            extract_issue_url(&v, "o", "r", "I1"),
            "https://gitcode.com/o/r/issues/I1"
        );
    }

    #[test]
    fn extract_issue_url_falls_back_to_constructed() {
        let v = serde_json::json!({});
        assert_eq!(
            extract_issue_url(&v, "o", "r", "I1"),
            "https://gitcode.com/o/r/issues/I1"
        );
    }

    #[test]
    fn truncate_for_log_truncates_long_text() {
        let long = "x".repeat(300);
        let out = truncate_for_log(&long);
        assert!(out.contains("已截断"));
        assert!(out.chars().count() < 300);
    }

    #[test]
    fn contents_url_encodes_path_segments() {
        let url = contents_url("o", "r", "a b/c.md").unwrap();
        // 空格被编码为 %20，`/` 作为路径分隔保留。
        assert!(url.ends_with("/contents/a%20b/c.md"));
    }
}
