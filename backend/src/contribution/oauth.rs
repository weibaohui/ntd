//! OAuth 辅助：state 管理（防 CSRF）与 token 本地持久化。

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use dashmap::DashMap;

use super::{now_unix, GitCodeToken};

/// OAuth state 存储：`state -> 过期时间戳`（Unix 秒）。
///
/// 用全局 `DashMap` 而非 `AppState` 字段：单进程 daemon 足够，
/// 且避免为每个请求 clone 大型 AppState。state 一次性使用，回调时移除。
static OAUTH_STATES: LazyLock<DashMap<String, i64>> = LazyLock::new(DashMap::new);

/// state 有效期（秒）：过期后即使匹配也拒绝，降低 CSRF 窗口。
const STATE_TTL_SECS: i64 = 600;

/// 生成随机 state 并记录过期时间，返回给前端拼进授权 URL。
pub fn generate_state() -> String {
    let state = uuid::Uuid::new_v4().to_string();
    let expires = now_unix() + STATE_TTL_SECS;
    OAUTH_STATES.insert(state.clone(), expires);
    // 顺手清理已过期条目，防止长期运行内存缓慢增长。
    let now = now_unix();
    OAUTH_STATES.retain(|_, exp| *exp > now);
    state
}

/// 校验并一次性消费 state：存在且未过期返回 `true`。
pub fn consume_state(state: &str) -> bool {
    // DashMap::remove 返回 (key, value) 元组，这里只关心过期时间。
    match OAUTH_STATES.remove(state) {
        Some((_, expires)) => expires > now_unix(),
        None => false,
    }
}

/// token 文件路径：`~/.ntd/contribution_token.json`。
fn token_file_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ntd").join("contribution_token.json"))
}

/// 读取持久化的 token；不存在或解析失败返回 `None`。
pub fn load_token() -> Option<GitCodeToken> {
    let path = token_file_path()?;
    load_token_from(&path)
}

/// 持久化 token 到 `~/.ntd/contribution_token.json`，文件权限收紧为 0600。
pub fn save_token(token: &GitCodeToken) -> Result<(), String> {
    let path = token_file_path().ok_or_else(|| "无法获取 home 目录".to_string())?;
    save_token_to(&path, token)
}

/// 删除持久化的 token（退出登录）。
pub fn clear_token() {
    if let Some(path) = token_file_path() {
        // 删除失败仅影响下次登录（会重新走授权），忽略即可。
        let _ = std::fs::remove_file(path);
    }
}

/// token 是否仍有效（未过期）。
pub fn token_is_valid(token: &GitCodeToken) -> bool {
    token.expires_at > now_unix()
}

/// 从指定路径读取 token（抽出路径参数便于单测，避免依赖真实 home 目录）。
fn load_token_from(path: &Path) -> Option<GitCodeToken> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 持久化 token 到指定路径（抽出路径参数便于单测）。
fn save_token_to(path: &Path, token: &GitCodeToken) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let json = serde_json::to_string(token).map_err(|e| format!("序列化 token 失败: {e}"))?;
    write_owner_only(path, json.as_bytes()).map_err(|e| format!("写入 token 失败: {e}"))?;
    Ok(())
}

/// 以 0600 权限原子创建并写入文件，避免「先写 0644 再 chmod」的窗口期泄露令牌。
fn write_owner_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        // mode(0o600) 在 open 时即生效，文件创建即私有，无 TOCTOU 窗口。
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).write(true).truncate(true).mode(0o600);
        let mut file = opts.open(path)?;
        file.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_token() -> GitCodeToken {
        GitCodeToken {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: now_unix() + 3600,
            scope: Some("all_issue".to_string()),
        }
    }

    #[test]
    fn save_and_load_token_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token.json");
        let token = sample_token();
        save_token_to(&path, &token).unwrap();
        let loaded = load_token_from(&path).unwrap();
        assert_eq!(loaded.access_token, "at");
        assert_eq!(loaded.refresh_token, "rt");
        assert_eq!(loaded.expires_at, token.expires_at);
    }

    #[test]
    fn load_token_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_token_from(&dir.path().join("nope.json")).is_none());
    }

    #[test]
    fn token_is_valid_respects_expiry() {
        let mut token = sample_token();
        assert!(token_is_valid(&token));
        token.expires_at = now_unix() - 1;
        assert!(!token_is_valid(&token));
    }

    #[test]
    fn generate_and_consume_state_succeeds_once() {
        let state = generate_state();
        assert!(consume_state(&state));
        // state 一次性使用：第二次消费必须失败。
        assert!(!consume_state(&state));
    }

    #[test]
    fn consume_unknown_state_fails() {
        assert!(!consume_state("does-not-exist"));
    }
}
