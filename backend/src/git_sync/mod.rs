//! Git 同步模块
//!
//! 提供从远程 Git 仓库同步内置资源（专家、模板、Skills）的能力。
//! 支持首次 clone 和后续 fetch + reset --hard 更新，固定以远程仓库为准（远程覆盖本地）。

use std::path::{Path, PathBuf};
use thiserror::Error;

/// 同步结果
///
/// 同步策略已固定为「以远程仓库为准」（远程覆盖本地）：`sync_repo` 始终
/// `git reset --hard` 到远程分支，保证工作区与远程完全一致，不再提供
/// keep_local / manual 等本地优先策略（keep_local 在本地 commit 已等于远程时会
/// 提前返回、跳过 reset --hard，导致被误删/被改的本地文件无法还原）。
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// 是否成功
    pub success: bool,
    /// 消息描述
    pub message: String,
    /// 是否是首次克隆
    pub is_first_clone: bool,
    /// 是否有更新
    pub has_updates: bool,
    /// 新增/更新的文件数
    pub changed_files: usize,
}

/// Git 操作错误
#[derive(Debug, Error)]
pub enum GitSyncError {
    #[error("git 命令未找到")]
    GitNotFound,
    #[error("git 命令执行失败: {0}")]
    CommandFailed(String),
    #[error("目录不存在: {0}")]
    DirectoryNotFound(String),
    #[error("无效的同步策略: {0}")]
    InvalidStrategy(String),
    #[error("网络错误: {0}")]
    NetworkError(String),
    #[error("认证失败: {0}")]
    AuthError(String),
    #[error("未知错误: {0}")]
    Unknown(String),
}

impl GitSyncError {
    /// 从命令输出解析错误类型
    fn from_output(status: std::process::ExitStatus, stderr: &str) -> Self {
        let stderr_lower = stderr.to_lowercase();
        if status.code() == Some(128) {
            if stderr_lower.contains("authentication") || stderr_lower.contains("permission") {
                GitSyncError::AuthError(stderr.to_string())
            } else if stderr_lower.contains("could not resolve") || stderr_lower.contains("network") {
                GitSyncError::NetworkError(stderr.to_string())
            } else {
                GitSyncError::CommandFailed(stderr.to_string())
            }
        } else {
            GitSyncError::CommandFailed(stderr.to_string())
        }
    }
}

/// 环境中是否可用 git。
///
/// 供启动检查等场景在同步前探测：未安装 git 时给出明确提示并跳过同步，
/// 而不是让 git_sync 抛错后被笼统记成「同步失败」、更不会 panic 主进程。
/// 与 `run_git_command` 用同一套 `which::which("git")` 探测，保持单一事实来源。
pub fn is_git_available() -> bool {
    which::which("git").is_ok()
}

/// 执行 git 命令
///
/// # 参数
/// - `args`: git 命令参数
/// - `cwd`: 工作目录（None 表示当前目录）
///
/// # 返回
/// 命令输出（stdout 和 stderr）
async fn run_git_command(args: &[&str], cwd: Option<&Path>) -> Result<(String, String), GitSyncError> {
    let git_path = match which::which("git") {
        Ok(p) => p,
        Err(_) => return Err(GitSyncError::GitNotFound),
    };

    let mut cmd = tokio::process::Command::new(git_path);
    cmd.args(args);
    if let Some(path) = cwd {
        cmd.current_dir(path);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    // 106：accept-new 替代 no——首次连接自动接受并记录主机密钥，之后连接密钥变化
    // 即拒绝（防 MITM 替换 bundled 工艺/专家资源内容）。no 完全放弃校验。
    cmd.env(
        "GIT_SSH_COMMAND",
        "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new",
    );

    // 106：网络半开时 output() 会永久悬挂，同步任务卡死。clone/fetch 走网络，
    // 给 120s。kill_on_drop(true)（106 评审修复）：tokio::process::Command 默认
    // false——超时分支 future 被 drop 时若不杀子进程，git 会变孤儿继续跑
    // （还持着 index.lock/fetch 锁）。
    cmd.kill_on_drop(true);
    const GIT_NET_TIMEOUT_SECS: u64 = 120;
    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(GIT_NET_TIMEOUT_SECS),
        cmd.output(),
    )
    .await
    {
        Ok(res) => res.map_err(|e| {
            GitSyncError::CommandFailed(format!("执行命令失败: {}", e))
        })?,
        Err(_) => {
            return Err(GitSyncError::CommandFailed(format!(
                "git 命令超时（{}s）: git {}",
                GIT_NET_TIMEOUT_SECS,
                args.join(" ")
            )));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(GitSyncError::from_output(output.status, &stderr));
    }

    Ok((stdout, stderr))
}

/// 获取本地仓库的当前提交哈希
///
/// # 参数
/// - `repo_path`: 仓库路径
///
/// # 返回
/// 当前 commit SHA
pub async fn get_current_commit(repo_path: &Path) -> Result<String, GitSyncError> {
    let (stdout, _) = run_git_command(&["rev-parse", "HEAD"], Some(repo_path)).await?;
    Ok(stdout.trim().to_string())
}

/// 获取远程仓库的最新提交哈希
///
/// # 参数
/// - `repo_path`: 仓库路径
/// - `remote`: 远程名称（默认 origin）
/// - `branch`: 分支名称
///
/// # 返回
/// 远程最新 commit SHA
pub async fn get_remote_commit(repo_path: &Path, remote: &str, branch: &str) -> Result<String, GitSyncError> {
    let (stdout, _) = run_git_command(
        &["ls-remote", "--heads", remote, branch],
        Some(repo_path),
    ).await?;
    let parts: Vec<&str> = stdout.split_whitespace().collect();
    if parts.is_empty() {
        return Err(GitSyncError::CommandFailed("无法获取远程提交".to_string()));
    }
    Ok(parts[0].to_string())
}

/// 克隆远程仓库
///
/// # 参数
/// - `url`: 远程仓库地址
/// - `target_path`: 目标路径
/// - `branch`: 分支名称
///
/// # 返回
/// 同步结果
pub async fn clone_repo(url: &str, target_path: &Path, branch: &str) -> Result<SyncResult, GitSyncError> {
    if target_path.exists() {
        return Err(GitSyncError::DirectoryNotFound(format!(
            "目标目录已存在: {}",
            target_path.display()
        )));
    }

    // 确保父目录存在
    let parent_dir = target_path.parent()
        .ok_or_else(|| GitSyncError::DirectoryNotFound("无效的目标路径".to_string()))?;
    if !parent_dir.exists() {
        std::fs::create_dir_all(parent_dir)
            .map_err(|e| GitSyncError::CommandFailed(format!("创建父目录失败: {}", e)))?;
    }

    // 使用目标目录名作为 clone 的最后一个参数
    // git clone -b branch --depth 1 url target_path
    let target_str = target_path.to_string_lossy();
    run_git_command(
        &["clone", "-b", branch, "--depth", "1", url, &target_str],
        None,  // 不指定 cwd，使用当前工作目录
    ).await?;

    Ok(SyncResult {
        success: true,
        message: "克隆成功".to_string(),
        is_first_clone: true,
        has_updates: true,
        changed_files: 0,
    })
}

/// 同步远程仓库（fetch + merge）
///
/// # 参数
/// - `repo_path`: 仓库路径
/// - `remote`: 远程名称
/// - `branch`: 分支名称
/// - `strategy`: 冲突处理策略
///
/// # 返回
/// 同步结果
pub async fn sync_repo(
    repo_path: &Path,
    remote: &str,
    branch: &str,
) -> Result<SyncResult, GitSyncError> {
    if !repo_path.exists() {
        return Err(GitSyncError::DirectoryNotFound(format!(
            "仓库目录不存在: {}",
            repo_path.display()
        )));
    }

    let local_commit = get_current_commit(repo_path).await?;

    // 先拉取远程最新提交，保证 origin/<branch> 指向远程最新
    run_git_command(&["fetch", remote, branch], Some(repo_path)).await?;

    let remote_commit = get_remote_commit(repo_path, remote, branch).await?;

    // 以远程为准：无论本地 commit 是否与远程相等，都把工作区重置到远程分支。
    // 这样即使本地文件被误删/被改（此时 local_commit == remote_commit 仍成立），
    // 也能通过 reset --hard 还原成远程版本，避免「已是最新版本」提前返回导致文件丢失。
    run_git_command(&["reset", "--hard", &format!("{}/{}", remote, branch)], Some(repo_path)).await?;

    // has_updates 仅反映 commit 是否前进；工作区被还原（如补回缺失文件）也算一次有效同步。
    let has_updates = local_commit != remote_commit;
    let message = if has_updates {
        "同步成功，远程覆盖本地".to_string()
    } else {
        "已是最新版本（如本地文件缺失已自动还原）".to_string()
    };
    Ok(SyncResult {
        success: true,
        message,
        is_first_clone: false,
        has_updates,
        changed_files: 0,
    })
}

/// 获取本地存储目录的绝对路径
///
/// # 参数
/// - `local_path`: 相对路径（相对于 ~/.ntd/）
///
/// # 返回
/// 绝对路径，如果无法获取 home 目录则返回 None
pub fn bundled_dir(local_path: &str) -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ntd").join(local_path))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_repo_restores_deleted_file() {
        // 回归测试：本地 commit 已等于远程、但工作区文件被删时，sync_repo 必须仍执行
        // reset --hard 把文件还原，不能提前返回「已是最新版本」导致文件丢失。
        // 这正是 keep_local 策略被移除前存在的同步缺陷。
        use std::process::Command;

        let remote = tempfile::tempdir().expect("创建临时远程仓库失败");
        let local = tempfile::tempdir().expect("创建临时本地仓库失败");

        // git 命令快捷执行器：失败即 panic，输出错误信息便于排查
        let git = |dir: &std::path::Path, args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .expect("执行 git 失败");
            assert!(
                out.status.success(),
                "git {:?} 失败: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        };

        // 1) 远程裸仓库（默认分支 main）
        git(remote.path(), &["init", "--bare", "-b", "main"]);

        // 2) 本地 clone 远程，落一个含 a.txt 的提交并推回远程
        git(local.path(), &["clone", remote.path().to_str().unwrap(), "."]);
        git(local.path(), &["config", "user.email", "t@t"]);
        git(local.path(), &["config", "user.name", "t"]);
        std::fs::write(local.path().join("a.txt"), "hello").expect("写 a.txt 失败");
        git(local.path(), &["add", "a.txt"]);
        git(local.path(), &["commit", "-m", "init"]);
        git(local.path(), &["push", "origin", "main"]);

        // 3) 模拟误删：删除工作区文件，但本地 commit 仍 == 远程（命中旧提前返回分支）
        std::fs::remove_file(local.path().join("a.txt")).expect("删除 a.txt 失败");
        assert!(!local.path().join("a.txt").exists(), "前置：a.txt 应已被删除");

        // 4) 执行同步，预期文件被 reset --hard 还原
        let rt = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        let result = rt.block_on(sync_repo(local.path(), "origin", "main"));
        assert!(result.is_ok(), "sync_repo 应成功: {:?}", result.err());
        assert!(
            local.path().join("a.txt").exists(),
            "被删文件应被 reset --hard 还原"
        );
    }

    #[test]
    fn test_bundled_dir() {
        if let Some(dir) = bundled_dir("bundled") {
            assert!(dir.to_string_lossy().contains(".ntd"));
            assert!(dir.to_string_lossy().contains("bundled"));
        }
    }

    #[test]
    fn test_is_git_available_when_git_present() {
        // 开发/CI 环境通常装了 git；此处验证探测函数在 git 存在时返回 true 且不 panic。
        // （git 缺失的分支无法在装了 git 的环境里测试，属可接受限制。）
        assert!(is_git_available());
    }
}
