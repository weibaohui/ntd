//! 专家贡献模块：把本地专家以 Issue 形式提交回官方 GitCode 仓库。
//!
//! 职责划分：
//! - `gitcode.rs`：GitCode 平台常量、owner/repo 解析、OAuth 授权 URL 拼装、
//!   token 交换/刷新、创建 Issue 的 HTTP 调用。
//! - `oauth.rs`：OAuth state 管理（防 CSRF）与 token 本地持久化。
//! - `issue.rs`：读取本地专家目录，组装贡献 Issue 的标题与 Markdown body。
//! Handler 层在 `handlers/contribution.rs`。

pub mod gitcode;
pub mod issue;
pub mod oauth;

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// GitCode OAuth 访问令牌（持久化到 `~/.ntd/contribution_token.json`）。
#[derive(Clone, Serialize, Deserialize)]
pub struct GitCodeToken {
    /// 访问令牌，用于调用 GitCode API
    pub access_token: String,
    /// 刷新令牌，access_token 过期后用其换取新令牌
    pub refresh_token: String,
    /// 过期时间（Unix 秒），由 token 接口的 expires_in + 当前时间计算
    pub expires_at: i64,
    /// 授权范围（如 all_issue）
    pub scope: Option<String>,
}

impl std::fmt::Debug for GitCodeToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 手动实现 Debug：令牌字段一律脱敏，防止误用 {:?}/dbg! 把明文 token 打进日志。
        f.debug_struct("GitCodeToken")
            .field("access_token", &"[redacted]")
            .field("refresh_token", &"[redacted]")
            .field("expires_at", &self.expires_at)
            .field("scope", &self.scope)
            .finish()
    }
}

/// 当前 Unix 秒。
///
/// 用 `unwrap_or(0)` 而非 `unwrap`：系统时钟异常时回退到 epoch，
/// 仅影响 token 过期判断（会误判为已过期并触发刷新），不会 panic。
pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 编排 PR 提交流程：fork → 建分支 → 写文件 → 建 PR。
///
/// 返回创建好的 PR 结果；任一步失败则上抛错误（此时 fork/分支可能已创建，
/// 属无害残留，用户下次重试会复用 fork、以新时间戳分支重新提交）。
pub async fn submit_pr(
    token: &GitCodeToken,
    owner: &str,
    repo: &str,
    expert: &crate::expert::ExpertMetadata,
    title: &str,
    body: &str,
) -> Result<gitcode::PrResult, String> {
    let access_token = &token.access_token;

    // 1. 取当前用户 username，作为 fork 后的 owner。
    let username = gitcode::get_current_username(access_token).await?;
    // 2. fork 官方仓库到用户账号（幂等：已 fork 则复用）。
    gitcode::ensure_fork(access_token, owner, repo).await?;
    // 3. 收集专家目录全部文件（含二进制头像）。
    let files = issue::collect_expert_files(expert)?;

    // 4. 用时间戳保证分支名唯一，避免与历史贡献分支冲突。
    let branch = format!("contrib/{}-{}", expert.name, now_unix());
    gitcode::create_branch(access_token, &username, repo, &branch, "main").await?;

    // 5. 逐个把文件写入 fork 分支。
    let message = format!("贡献专家 {} v{}", expert.name, expert.version);
    for f in &files {
        let content_b64 = encode_base64(&f.content);
        gitcode::create_file(access_token, &username, repo, &branch, &f.path, &content_b64, &message)
            .await?;
    }

    // 6. 创建 PR：head = "{username}:{branch}"，base 固定 main。
    let head = format!("{username}:{branch}");
    gitcode::create_pr(access_token, owner, repo, title, body, &head, "main").await
}

/// base64 编码字节内容（GitCode contents API 要求 content 为 base64）。
fn encode_base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_base64_encodes_standard() {
        // "abc" 的标准 base64 为 "YWJj"。
        assert_eq!(encode_base64(b"abc"), "YWJj");
    }
}
