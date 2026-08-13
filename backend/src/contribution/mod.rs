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
