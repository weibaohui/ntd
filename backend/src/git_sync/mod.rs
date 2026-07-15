//! Git 同步模块
//!
//! 提供从远程 Git 仓库同步内置资源（专家、模板、Skills）的能力。
//! 支持首次 clone 和后续 pull + merge 更新，冲突策略可选。

use std::path::{Path, PathBuf};
use thiserror::Error;

/// 同步策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStrategy {
    /// 保留本地修改（冲突时本地获胜）
    KeepLocal,
    /// 覆盖本地修改（冲突时远程获胜）
    Overwrite,
    /// 手动处理冲突（保留冲突状态）
    Manual,
}

impl From<&str> for SyncStrategy {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "overwrite" => SyncStrategy::Overwrite,
            "manual" => SyncStrategy::Manual,
            _ => SyncStrategy::KeepLocal,
        }
    }
}

/// 同步结果
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
    cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes -o StrictHostKeyChecking=no");

    let output = cmd.output().await.map_err(|e| {
        GitSyncError::CommandFailed(format!("执行命令失败: {}", e))
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(GitSyncError::from_output(output.status, &stderr));
    }

    Ok((stdout, stderr))
}

/// 获取本地仓库的当前分支
///
/// # 参数
/// - `repo_path`: 仓库路径
///
/// # 返回
/// 当前分支名称
pub async fn get_current_branch(repo_path: &Path) -> Result<String, GitSyncError> {
    let (stdout, _) = run_git_command(&["branch", "--show-current"], Some(repo_path)).await?;
    Ok(stdout.trim().to_string())
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

/// 检查是否存在未提交的本地修改
///
/// # 参数
/// - `repo_path`: 仓库路径
///
/// # 返回
/// 是否有未提交的修改
pub async fn has_local_changes(repo_path: &Path) -> Result<bool, GitSyncError> {
    let (stdout, _) = run_git_command(&["status", "--porcelain"], Some(repo_path)).await?;
    Ok(!stdout.is_empty())
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
    strategy: SyncStrategy,
) -> Result<SyncResult, GitSyncError> {
    if !repo_path.exists() {
        return Err(GitSyncError::DirectoryNotFound(format!(
            "仓库目录不存在: {}",
            repo_path.display()
        )));
    }

    let local_commit = get_current_commit(repo_path).await?;

    run_git_command(&["fetch", remote, branch], Some(repo_path)).await?;

    let remote_commit = get_remote_commit(repo_path, remote, branch).await?;

    if local_commit == remote_commit {
        return Ok(SyncResult {
            success: true,
            message: "已是最新版本".to_string(),
            is_first_clone: false,
            has_updates: false,
            changed_files: 0,
        });
    }

    match strategy {
        SyncStrategy::KeepLocal | SyncStrategy::Overwrite => {
            run_git_command(&["reset", "--hard", &format!("{}/{}", remote, branch)], Some(repo_path)).await?;
            Ok(SyncResult {
                success: true,
                message: "同步成功，远程覆盖本地".to_string(),
                is_first_clone: false,
                has_updates: true,
                changed_files: 0,
            })
        }
        SyncStrategy::Manual => {
            match run_git_command(&["merge", &format!("{}/{}", remote, branch)], Some(repo_path)).await {
                Ok((stdout, _)) => {
                    let changed_files = count_changed_files(&stdout);
                    Ok(SyncResult {
                        success: true,
                        message: format!("同步成功，更新了 {} 个文件", changed_files),
                        is_first_clone: false,
                        has_updates: true,
                        changed_files,
                    })
                }
                Err(e) => {
                    if let GitSyncError::CommandFailed(ref msg) = e {
                        if msg.contains("Automatic merge failed") {
                            return Ok(SyncResult {
                                success: true,
                                message: "存在冲突，请手动处理".to_string(),
                                is_first_clone: false,
                                has_updates: true,
                                changed_files: 0,
                            });
                        }
                    }
                    Err(e)
                }
            }
        }
    }
}

/// 从 merge 输出中统计变更文件数
fn count_changed_files(output: &str) -> usize {
    let has_update = output.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("Updating ") || trimmed.starts_with("Fast-forward")
    });
    let mode_lines = output
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("create mode") || trimmed.starts_with("delete mode")
        })
        .count();
    has_update as usize + mode_lines
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

/// 检查是否需要同步（首次或有更新）
///
/// # 参数
/// - `repo_path`: 仓库路径
/// - `remote`: 远程名称
/// - `branch`: 分支名称
///
/// # 返回
/// 是否需要同步
pub async fn needs_sync(repo_path: &Path, remote: &str, branch: &str) -> Result<bool, GitSyncError> {
    if !repo_path.exists() {
        return Ok(true);
    }

    let local_commit = get_current_commit(repo_path).await?;
    let remote_commit = get_remote_commit(repo_path, remote, branch).await?;

    Ok(local_commit != remote_commit)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_from_str() {
        assert_eq!(SyncStrategy::from("keep_local"), SyncStrategy::KeepLocal);
        assert_eq!(SyncStrategy::from("Keep_Local"), SyncStrategy::KeepLocal);
        assert_eq!(SyncStrategy::from("overwrite"), SyncStrategy::Overwrite);
        assert_eq!(SyncStrategy::from("manual"), SyncStrategy::Manual);
        assert_eq!(SyncStrategy::from("unknown"), SyncStrategy::KeepLocal);
    }

    #[test]
    fn test_bundled_dir() {
        if let Some(dir) = bundled_dir("bundled") {
            assert!(dir.to_string_lossy().contains(".ntd"));
            assert!(dir.to_string_lossy().contains("bundled"));
        }
    }

    #[test]
    fn test_count_changed_files() {
        let output = "Updating abc123..def456\nFast-forward\n create mode 100644 experts/test.md\n delete mode 100644 experts/old.md";
        assert_eq!(count_changed_files(output), 3);
    }
}
