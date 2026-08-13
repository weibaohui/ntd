//! PAT 持久化：把用户填写的 GitCode PAT 存到本地 `~/.ntd/`。
//!
//! 只存 PAT 本身；账号身份（login）不落盘，由 AI 执行时用 `/user` 接口实时获取，
//! 避免「文件里的 username 与 PAT 实际归属不一致」的隐患。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// GitCode PAT 凭据（持久化到 `~/.ntd/contribution_pat.json`）。
#[derive(Clone, Serialize, Deserialize)]
pub struct PatCredential {
    /// GitCode 个人访问令牌
    pub pat: String,
}

impl std::fmt::Debug for PatCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 手动实现 Debug：令牌字段脱敏，防止 {:?}/dbg! 把明文 PAT 打进日志。
        f.debug_struct("PatCredential")
            .field("pat", &"[redacted]")
            .finish()
    }
}

/// PAT 文件路径：`~/.ntd/contribution_pat.json`。
fn pat_file_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ntd").join("contribution_pat.json"))
}

/// 读取持久化的 PAT；不存在或解析失败返回 `None`。
pub fn load() -> Option<PatCredential> {
    let path = pat_file_path()?;
    load_from(&path)
}

/// 持久化 PAT 到 `~/.ntd/contribution_pat.json`，文件权限收紧为 0600。
pub fn save(cred: &PatCredential) -> Result<(), String> {
    let path = pat_file_path().ok_or_else(|| "无法获取 home 目录".to_string())?;
    save_to(&path, cred)
}

/// 删除持久化的 PAT（退出登录）。
pub fn clear() {
    if let Some(path) = pat_file_path() {
        // 删除失败仅影响下次登录（会重新填写 PAT），忽略即可。
        let _ = std::fs::remove_file(path);
    }
}

/// 从指定路径读取 PAT（抽出路径参数便于单测，避免依赖真实 home 目录）。
fn load_from(path: &Path) -> Option<PatCredential> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 持久化 PAT 到指定路径（抽出路径参数便于单测）。
fn save_to(path: &Path, cred: &PatCredential) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let json = serde_json::to_string(cred).map_err(|e| format!("序列化 PAT 失败: {e}"))?;
    write_owner_only(path, json.as_bytes()).map_err(|e| format!("写入 PAT 失败: {e}"))?;
    Ok(())
}

/// 以 0600 权限原子创建并写入文件，避免「先写 0644 再 chmod」的窗口期泄露令牌。
fn write_owner_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        // mode(0o600) 在 open 时即生效，文件创建即私有，无 TOCTOU 窗口。
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).write(true).truncate(true).mode(0o600);
        let mut file = opts.open(path)?;
        // open 的 mode 只对新建文件生效；若文件已存在且遗留宽松权限（如 0644），
        // 需在写入前先 chmod 收紧，避免「明文已写入、权限仍宽松」的窗口。
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
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

    fn sample_cred() -> PatCredential {
        PatCredential {
            pat: "test-pat-123456".to_string(),
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pat.json");
        let cred = sample_cred();
        save_to(&path, &cred).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.pat, cred.pat);
    }

    #[test]
    fn load_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_from(&dir.path().join("nope.json")).is_none());
    }

    #[test]
    fn debug_redacts_pat() {
        let cred = sample_cred();
        let debug_str = format!("{cred:?}");
        // 明文 PAT 不得出现在 Debug 输出里。
        assert!(!debug_str.contains(&cred.pat));
        assert!(debug_str.contains("[redacted]"));
    }

    #[cfg(unix)]
    #[test]
    fn write_owner_only_tightens_existing_loose_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pat.json");
        // 预置一个 0644 的宽松权限文件，模拟历史遗留。
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_owner_only(&path, b"new").unwrap();

        // 重写后权限应被收紧回 0600，而非保持原有的 0644。
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
