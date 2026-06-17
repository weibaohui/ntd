//! Git Worktree 服务（issue #643）
//!
//! 在项目目录级托管 git worktree 的完整生命周期。
//! 由 ntd（而不是 Claude Code 自身）负责：
//!   1. 执行前：若目录不是 git 仓库则 `git init`；然后 `git worktree add` 一个 worktree
//!   2. 执行中：把 worktree 路径回写到 execution_record（仅记录，不影响子进程）
//!   3. 执行后：若该目录启用了 `auto_cleanup`，调用 `git worktree remove --force` 清理
//!
//! 设计取舍：
//! - 所有 git 命令都用 `std::process::Command` 直接 spawn 同步执行（不用 git2 crate）：
//!   1. ntd 已经把 git 当作外部依赖（auto init / status 都靠 `Command::new("git")`），
//!      引入 git2 会多一层 Rust ABI 维护成本；
//!   2. git CLI 的错误信息更可读，调试更直观；
//!   3. 这部分逻辑只在前置/收尾阶段跑一次，不在 hot path，开销可以接受。
//! - worktree 目录名格式：`<todo_id>-<unix_secs>`。`unix_secs` 选秒而不是纳秒，
//!   避免出现同名 worktree 时仅相差几纳秒无法区分。
//! - `cleanup_worktree` 在目录已不存在或 `git worktree remove` 失败时**不报错**：
//!   用户手动删除或 git 元数据丢失时，让"清理"成为幂等 no-op 而非阻塞执行结果。

use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use tracing::{info, warn};

/// 在项目目录下创建 worktree 的相对目录名（issue #643 规范要求）。
///
/// 选 `.worktrees` 而不是 `worktrees` 是因为以 `.` 开头会被常见工具识别为"本地临时目录"，
/// 减少误提交风险；同时 ntd 启动时也会确保该目录在 `.gitignore` 里。
pub const WORKTREE_ROOT_DIR: &str = ".worktrees";

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("git is not installed or not in PATH: {0}")]
    GitUnavailable(String),
    #[error("project directory does not exist: {0}")]
    ProjectDirMissing(String),
    #[error("`git {cmd}` failed in {dir}: {stderr}")]
    GitCommandFailed {
        cmd: String,
        dir: String,
        stderr: String,
    },
}

/// 单实例无状态服务。
///
/// 这里用 unit struct 而不是 free function 集合，原因是 issue 描述里要求
/// "由 ntd 程序托管 worktree 生命周期" —— 用一个具名类型让调用方更明确
/// 表达"这是 worktree 相关操作"，未来加 metrics/tracing 接入也好挂。
pub struct WorktreeService;

impl WorktreeService {
    pub fn new() -> Self {
        Self
    }

    /// 确保 `project_path` 是一个 git 仓库，不是则自动 `git init`。
    ///
    /// 返回 `Ok(())` 表示目录已是 git 仓库（无论是本来就存在还是刚 init）。
    /// 返回 `Err` 时仅在三种场景：
    ///   1. `project_path` 不存在
    ///   2. `git` 命令无法 spawn（PATH 里找不到）
    ///   3. `git init` / `git rev-parse` 子命令退出码非 0
    pub fn ensure_git_repo(&self, project_path: &str) -> Result<(), WorktreeError> {
        let p = Path::new(project_path);
        if !p.exists() {
            return Err(WorktreeError::ProjectDirMissing(project_path.to_string()));
        }

        // 用 `git rev-parse --git-dir` 探测：返回成功即表示已是 git 仓库，
        // 比检查 `.git` 子目录更稳（worktree 自身的 `.git` 是文件不是目录）。
        let probe = Command::new("git")
            .arg("rev-parse")
            .arg("--git-dir")
            .current_dir(p)
            .output();
        match probe {
            Ok(out) if out.status.success() => return Ok(()),
            Ok(_) => {
                // 不是仓库，下一步执行 init
                info!(project = %project_path, "initializing empty git repository");
            }
            Err(e) => {
                return Err(WorktreeError::GitUnavailable(e.to_string()));
            }
        }

        let init_out = Command::new("git")
            .arg("init")
            .arg("-b")
            .arg("main")
            .current_dir(p)
            .output()
            .map_err(|e| WorktreeError::GitUnavailable(e.to_string()))?;
        if !init_out.status.success() {
            // 兜底：某些旧版 git 不支持 `-b main`，再用默认 init 重试
            let fallback = Command::new("git")
                .arg("init")
                .current_dir(p)
                .output()
                .map_err(|e| WorktreeError::GitUnavailable(e.to_string()))?;
            if !fallback.status.success() {
                let stderr = String::from_utf8_lossy(&fallback.stderr).into_owned();
                return Err(WorktreeError::GitCommandFailed {
                    cmd: "init".into(),
                    dir: project_path.to_string(),
                    stderr,
                });
            }
        }
        Ok(())
    }

    /// 基于 `<project>/.worktrees/<todo_id>-<unix_secs>/` 下创建 worktree。
    ///
    /// 如果当前分支还不存在（仓库刚 init），则先建一个空 commit 避免
    /// `git worktree add` 报 "fatal: invalid reference"。
    ///
    /// 返回值是 worktree 目录的**绝对路径**，可直接作为 `Command::current_dir` 使用。
    pub fn create_worktree(
        &self,
        project_path: &str,
        todo_id: i64,
    ) -> Result<String, WorktreeError> {
        // 入口先做 git 仓库检查（包含自动 init），保证下面的 worktree add 不会
        // 在非 git 目录上失败；这一步在并发首次执行时是幂等的。
        self.ensure_git_repo(project_path)?;

        // 探测当前分支是否存在提交。空仓库 init 后没有 HEAD，需要先做一次空 commit
        // 才能 `git worktree add`；否则 git 会报 "invalid reference"。
        // 这里用 HEAD 而不是硬编码 "main"——很多环境下默认分支是 master，
        // 硬编码 main 会导致老仓库 worktree 创建失败。
        if !self.has_any_commit(project_path)? {
            self.ensure_empty_commit(project_path)?;
        }

        let worktree_dir = self.worktree_path(project_path, todo_id);
        if worktree_dir.exists() {
            // 同名目录已存在（极小概率：todo_id 复用 + 同一秒）—— 不强制清理，
            // 让上层 executor 走原始 workspace 路径，把决策权留给调用方。
            warn!(
                worktree = %worktree_dir.display(),
                "worktree directory already exists, skipping creation"
            );
            return Ok(worktree_dir.to_string_lossy().into_owned());
        }

        // 创建 .worktrees 父目录（如果还不存在）。`git worktree add` 不会自动建父目录。
        if let Some(parent) = worktree_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|e| WorktreeError::GitCommandFailed {
                cmd: "create_dir_all".into(),
                dir: parent.to_string_lossy().into_owned(),
                stderr: e.to_string(),
            })?;
        }

        // 基于当前分支的 HEAD 创建 worktree，不再硬编码 "main"。
        // 当前分支名由 `current_branch` 探测得到，兼容 main/master/自定义分支。
        let base = self.current_branch(project_path)?;
        // 分支名只允许 [a-zA-Z0-9_-]，时间戳里如果带 `:` / `.` 会触发
        // "is not a valid branch name"，所以这里只取秒级 unix 时间。
        let now = crate::models::utc_timestamp();
        let sec_marker: i64 = now
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        let branch_name = format!("wt-{}-{}", todo_id, sec_marker);
        let out = Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg("-b")
            .arg(&branch_name)
            .arg(&worktree_dir)
            .arg(&base)
            .current_dir(project_path)
            .output()
            .map_err(|e| WorktreeError::GitUnavailable(e.to_string()))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            return Err(WorktreeError::GitCommandFailed {
                cmd: "worktree add".into(),
                dir: project_path.to_string(),
                stderr,
            });
        }
        info!(
            worktree = %worktree_dir.display(),
            base = %base,
            "created git worktree for todo execution"
        );
        Ok(worktree_dir.to_string_lossy().into_owned())
    }

    /// 清理 worktree。已不存在/已被手动删除/git 元数据丢失时一律记 warn 返回 Ok(())，
    /// 让 cleanup 步骤成为幂等 no-op。
    pub fn cleanup_worktree(&self, worktree_path: &str) -> Result<(), WorktreeError> {
        let path = Path::new(worktree_path);
        if !path.exists() {
            warn!(worktree = %worktree_path, "worktree path already gone, skip cleanup");
            return Ok(());
        }

        // 找 worktree 所属的主仓库（git 自身的 .git 文件记录了 gitdir 指向主仓的 worktrees/）
        let main_repo = match self.find_main_repo(worktree_path) {
            Ok(p) => p,
            Err(e) => {
                warn!(worktree = %worktree_path, error = %e, "cannot locate main repo, removing directory directly");
                let _ = std::fs::remove_dir_all(path);
                return Ok(());
            }
        };

        let out = Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg("--force")
            .arg(path)
            .current_dir(&main_repo)
            .output();
        match out {
            Ok(o) if o.status.success() => {
                info!(worktree = %worktree_path, "cleaned up git worktree");
                Ok(())
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                warn!(worktree = %worktree_path, stderr = %stderr, "git worktree remove failed, falling back to rm -rf");
                let _ = std::fs::remove_dir_all(path);
                Ok(())
            }
            Err(e) => {
                warn!(worktree = %worktree_path, error = %e, "failed to spawn git worktree remove, falling back to rm -rf");
                let _ = std::fs::remove_dir_all(path);
                Ok(())
            }
        }
    }

    /// worktree 目录的绝对路径（不含创建动作），便于单测与日志展示。
    pub fn worktree_path(&self, project_path: &str, todo_id: i64) -> PathBuf {
        let now = crate::models::utc_timestamp();
        PathBuf::from(project_path)
            .join(WORKTREE_ROOT_DIR)
            .join(format!("{}-{}", todo_id, now))
    }

    /// 探测仓库是否有任意 commit（HEAD 是否解析得到）。
    /// 取代硬编码 "main" 的存在性检查——空仓库 init 后任何分支都没有 commit。
    fn has_any_commit(&self, project_path: &str) -> Result<bool, WorktreeError> {
        let out = Command::new("git")
            .arg("rev-parse")
            .arg("--verify")
            .arg("HEAD")
            .current_dir(project_path)
            .output()
            .map_err(|e| WorktreeError::GitUnavailable(e.to_string()))?;
        Ok(out.status.success())
    }

    /// 获取当前分支名（空仓库 fallback 到默认 "main"）。
    /// 优先级：`rev-parse --abbrev-ref HEAD` → 当用户处于 detached HEAD 时退到 init.defaultBranch。
    fn current_branch(&self, project_path: &str) -> Result<String, WorktreeError> {
        let probe = Command::new("git")
            .arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .current_dir(project_path)
            .output()
            .map_err(|e| WorktreeError::GitUnavailable(e.to_string()))?;
        if probe.status.success() {
            let name = String::from_utf8_lossy(&probe.stdout).trim().to_string();
            // detached HEAD 时 git 会输出 "HEAD"，不是真正的分支名
            if !name.is_empty() && name != "HEAD" {
                return Ok(name);
            }
        }
        // 兜底：空仓库时没有 HEAD，但 `git init -b main` 仍会创建 main 分支引用。
        // 即便没有 commit，后续 `worktree add ... main` 在空仓库也会失败——
        // 这正是 `ensure_empty_commit` 介入的时机。这里只兜底分支名探测。
        Ok("main".to_string())
    }

    /// 在空仓库的 main 分支上建一个空 commit，让后续 `git worktree add main` 不报 invalid reference。
    fn ensure_empty_commit(&self, project_path: &str) -> Result<(), WorktreeError> {
        // 注意：必须用环境变量注入 author/committer 身份，而不是 `git config --local`。
        // 原因：某些精简 git 镜像（CI/容器）下 `safe.directory` 限制会让 `git config --local`
        // 静默失败，导致 commit 时 "unable to auto-detect email address" 报错。
        // 环境变量绕过配置层，是 git 官方推荐的"一次性提交"做法。
        let commit = Command::new("git")
            .args(["commit", "--allow-empty", "-m", "ntd: initial worktree base"])
            .current_dir(project_path)
            .env("GIT_AUTHOR_NAME", "ntd")
            .env("GIT_AUTHOR_EMAIL", "ntd@localhost")
            .env("GIT_COMMITTER_NAME", "ntd")
            .env("GIT_COMMITTER_EMAIL", "ntd@localhost")
            .output()
            .map_err(|e| WorktreeError::GitUnavailable(e.to_string()))?;
        if !commit.status.success() {
            let stderr = String::from_utf8_lossy(&commit.stderr).into_owned();
            return Err(WorktreeError::GitCommandFailed {
                cmd: "commit --allow-empty".into(),
                dir: project_path.to_string(),
                stderr,
            });
        }
        Ok(())
    }

    /// 从 worktree 内部读 `.git` 文件，找到主仓库目录。
    fn find_main_repo(&self, worktree_path: &str) -> Result<PathBuf, WorktreeError> {
        let dot_git = Path::new(worktree_path).join(".git");
        let content = std::fs::read_to_string(&dot_git).map_err(|e| {
            WorktreeError::GitCommandFailed {
                cmd: "read .git".into(),
                dir: dot_git.to_string_lossy().into_owned(),
                stderr: e.to_string(),
            }
        })?;
        // .git 文件内容形如 `gitdir: /path/to/main/.git/worktrees/<name>`
        let gitdir = content
            .trim_start_matches("gitdir:")
            .trim()
            .to_string();
        // 取出 `/path/to/main/.git/worktrees/<name>` 中的 `/path/to/main` 段。
        let p = PathBuf::from(&gitdir);
        let ancestors: Vec<_> = p.ancestors().collect();
        // ancestors 顺序: worktree_name -> worktrees -> .git -> main_repo
        if ancestors.len() >= 4 {
            Ok(ancestors[3].to_path_buf())
        } else {
            Err(WorktreeError::GitCommandFailed {
                cmd: "parse .git".into(),
                dir: worktree_path.to_string(),
                stderr: format!("unexpected gitdir format: {}", gitdir),
            })
        }
    }
}

impl Default for WorktreeService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    /// 创建一个用 git init 过的临时目录并返回路径。
    /// 部分测试用例的"前置 init"需要在用例里显式调用 WorktreeService::ensure_git_repo，
    /// 这里只给一个直接走 CLI 的小 helper，避免用例代码被 git 命令细节淹没。
    fn init_temp_repo() -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = StdCommand::new("git")
            .arg("init")
            .current_dir(dir.path())
            .status()
            .expect("git init");
        assert!(status.success(), "git init should succeed");
        dir
    }

    /// 已存在仓库时 ensure_git_repo 是 no-op。
    #[test]
    fn test_ensure_git_repo_existing_repo_is_noop() {
        let dir = init_temp_repo();
        let svc = WorktreeService::new();
        svc.ensure_git_repo(dir.path().to_str().unwrap())
            .expect("existing repo should not error");
    }

    /// 目录不存在时返回 ProjectDirMissing。
    #[test]
    fn test_ensure_git_repo_missing_dir_errors() {
        let svc = WorktreeService::new();
        let res = svc.ensure_git_repo("/this/path/should/not/exist/ntd-test-643");
        assert!(matches!(res, Err(WorktreeError::ProjectDirMissing(_))));
    }

    /// 非 git 目录会自动 init。
    #[test]
    fn test_ensure_git_repo_non_existing_repo_initializes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = WorktreeService::new();
        svc.ensure_git_repo(dir.path().to_str().unwrap())
            .expect("auto init should succeed");
        // init 之后再调一次应该依然 ok（命中"已是仓库"分支）
        svc.ensure_git_repo(dir.path().to_str().unwrap())
            .expect("re-call should be noop");
    }

    /// worktree_path 不依赖文件系统状态，只把 todo_id 拼进路径里。
    #[test]
    fn test_worktree_path_format() {
        let svc = WorktreeService::new();
        let p = svc.worktree_path("/tmp/proj", 42);
        let s = p.to_string_lossy();
        assert!(s.contains("/tmp/proj/.worktrees/42-"), "got: {}", s);
    }

    /// 完整 create + cleanup 流程，验证 worktree 真的被 git 管起来。
    /// 跳过的条件：本机没装 git。CI 上没有 git 也能编过（test 不依赖 git 可用性）。
    #[test]
    fn test_create_and_cleanup_worktree_full_cycle() {
        if StdCommand::new("git").arg("--version").output().is_err() {
            // 没装 git 就跳过，避免在精简镜像里挂掉
            return;
        }
        let dir = init_temp_repo();
        // 给 main 一个空 commit，否则 worktree add 报 invalid reference
        // 必须设 author env，否则容器/CI 上 git 报 "unable to auto-detect email"。
        let status = StdCommand::new("git")
            .args(["commit", "--allow-empty", "-m", "init"])
            .env("GIT_AUTHOR_NAME", "ntd")
            .env("GIT_AUTHOR_EMAIL", "ntd@localhost")
            .env("GIT_COMMITTER_NAME", "ntd")
            .env("GIT_COMMITTER_EMAIL", "ntd@localhost")
            .current_dir(dir.path())
            .status()
            .expect("git commit");
        assert!(status.success(), "initial empty commit should succeed");

        let svc = WorktreeService::new();
        let wt = svc
            .create_worktree(dir.path().to_str().unwrap(), 1)
            .expect("create worktree");
        let wt_path = PathBuf::from(&wt);
        assert!(wt_path.exists(), "worktree dir should exist after create");
        // .worktrees 子目录在主仓下应当存在
        let wt_root = dir.path().join(WORKTREE_ROOT_DIR);
        assert!(wt_root.exists(), ".worktrees root should be created");

        svc.cleanup_worktree(&wt).expect("cleanup should be ok");
        // cleanup 后 worktree 目录应该消失（git worktree remove 会删目录）
        assert!(!wt_path.exists(), "worktree dir should be removed after cleanup");
    }

    /// cleanup 在目录已经不存在时返回 Ok(()), 验证幂等性。
    #[test]
    fn test_cleanup_worktree_missing_path_is_idempotent() {
        let svc = WorktreeService::new();
        svc.cleanup_worktree("/tmp/ntd-643-nonexistent-path")
            .expect("missing path cleanup should not error");
    }

    /// 仓库里没有 main 分支时，create_worktree 会自动建一个空 commit 让 main 可用。
    /// 验证空仓库 + worktree 也能工作（这是首次启用 worktree 的真实场景）。
    #[test]
    fn test_create_worktree_on_fresh_empty_repo() {
        if StdCommand::new("git").arg("--version").output().is_err() {
            return;
        }
        let dir = init_temp_repo();
        let svc = WorktreeService::new();
        let wt = svc
            .create_worktree(dir.path().to_str().unwrap(), 7)
            .expect("create worktree on fresh repo should succeed");
        assert!(PathBuf::from(&wt).exists());
        // 清理避免污染 /tmp（tempdir drop 会兜底删主目录，但 worktree 在子目录）
        let _ = fs::remove_dir_all(dir.path().join(WORKTREE_ROOT_DIR));
    }
}
