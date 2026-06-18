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
//! - 所有同步 git 调用统一走 `run_git_with_timeout` 包装，避免在 lock / I/O hang 时
//!   阻塞调用方线程。超时后会主动 `kill` 子进程并返回 WorktreeError::GitTimeout。
//! - worktree 目录名格式：`<todo_id>-<yymmddHHMMss>-<rand8>`。用 `yymmddHHMMss`（可读时间）
//!   + 8 hex 字符（UUIDv4 高 32 bit）确保唯一性。
//! - `cleanup_worktree` 在目录已不存在或 `git worktree remove` 失败时**不报错**：
//!   用户手动删除或 git 元数据丢失时，让"清理"成为幂等 no-op 而非阻塞执行结果。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use thiserror::Error;
use tracing::{info, warn};
use uuid::Uuid;
use wait_timeout::ChildExt;

/// 单次 git 命令的硬超时。30 秒覆盖「首次 init + 空 commit」最坏路径，
/// 远高于 `git rev-parse` / `worktree add` 等轻量子命令的常态耗时。
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// git 子命令在 `GIT_COMMAND_TIMEOUT` 内未结束。kill 子进程后向上抛，
    /// 由调用方决定回退到原 workspace 还是直接报失败。
    #[error("`git {cmd}` in {dir} timed out after {timeout:?}")]
    GitTimeout {
        cmd: String,
        dir: String,
        timeout: Duration,
    },
}

/// 单实例无状态服务。
///
/// 这里用 unit struct 而不是 free function 集合，原因是 issue 描述里要求
/// "由 ntd 程序托管 worktree 生命周期" —— 用一个具名类型让调用方更明确
/// 表达"这是 worktree 相关操作"，未来加 metrics/tracing 接入也好挂。
pub struct WorktreeService;

/// 给同步 git 命令加超时边界。
///
/// 之所以自己包一层而不直接 `cmd.output()`：
///   - git 在持有锁、远端 I/O hang 时 `output()` 会无限阻塞，
///     把调用方所在 tokio worker 也拖死；
///   - 超时后必须主动 `kill` 子进程，否则即便我们返回 Err 也会留下孤儿 git。
///
/// 实现思路：在线程里跑 `cmd.output()`，把结果通过 channel 传出；主线程用
/// `recv_timeout` 等待。超时分支用 `Child::from_pid` 不行（我们没保留句柄），
/// 所以这里改为 `cmd.spawn() + wait_timeout` 直接同步等待，超时分支 `kill` 子进程。
///
/// 入参 `cmd_label` 用于在超时/失败时把「这条命令是啥」打到日志/错误信息里，
/// 方便排查。`cwd_display` 仅作错误日志用，current_dir 仍由调用方设置到 `cmd` 上。
fn run_git_with_timeout(
    mut cmd: Command,
    cmd_label: &str,
    cwd_display: &str,
) -> Result<std::process::Output, WorktreeError> {
    // 把输出重定向到管道，便于超时分支独立 kill 进程；不在超时分支读 stdout/stderr，
    // 减少 pipe 关闭的潜在阻塞。
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| WorktreeError::GitUnavailable(e.to_string()))?;
    // `wait_timeout` 是同步调用：返回 `Ok(Some(status))` 表示子进程已完成，
    // `Ok(None)` 表示还在跑（需要 kill），`Err` 通常意味着 wait 系统调用失败。
    match child
        .wait_timeout(GIT_COMMAND_TIMEOUT)
        .map_err(|e| WorktreeError::GitUnavailable(e.to_string()))?
    {
        Some(_) => {
            // 子进程已结束；用 `wait_with_output` 等价语义收集 stdout/stderr。
            // std 没有暴露「已经 wait 完但还要读 pipe」的 API，所以这里退化为
            // 重新调用 wait_with_output：对于正常结束的子进程，第二次 wait
            // 立刻返回已缓存的 ExitStatus，pipe 数据仍然可读。
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut s) = child.stdout.take() {
                use std::io::Read;
                let _ = s.read_to_end(&mut stdout);
            }
            if let Some(mut s) = child.stderr.take() {
                use std::io::Read;
                let _ = s.read_to_end(&mut stderr);
            }
            let status = child.wait().map_err(|e| {
                WorktreeError::GitUnavailable(e.to_string())
            })?;
            Ok(std::process::Output {
                status,
                stdout,
                stderr,
            })
        }
        None => {
            // 超时：先 kill 再 wait，避免僵尸进程
            warn!(
                cmd = cmd_label,
                dir = cwd_display,
                "git command exceeded timeout, killing child"
            );
            let _ = child.kill();
            let _ = child.wait();
            Err(WorktreeError::GitTimeout {
                cmd: cmd_label.to_string(),
                dir: cwd_display.to_string(),
                timeout: GIT_COMMAND_TIMEOUT,
            })
        }
    }
}

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
        // 探测本身走超时路径：失败=不是仓库，Ok 但退出非零=同义。
        let mut probe_cmd = Command::new("git");
        probe_cmd
            .arg("rev-parse")
            .arg("--git-dir")
            .current_dir(p);
        match run_git_with_timeout(probe_cmd, "rev-parse --git-dir", project_path) {
            Ok(out) if out.status.success() => return Ok(()),
            Ok(_) => {
                // 不是仓库，下一步执行 init
                info!(project = %project_path, "initializing empty git repository");
            }
            Err(e) => {
                // 超时或 spawn 失败都按「不可用」处理，让外层走 fallback
                return Err(e);
            }
        }

        // init 主路径走超时包装；fallback 同样。两条路径都失败时把第二次的错误向上抛。
        let mut init_cmd = Command::new("git");
        init_cmd.arg("init").arg("-b").arg("main").current_dir(p);
        let init_out = run_git_with_timeout(init_cmd, "init -b main", project_path)?;
        if init_out.status.success() {
            return Ok(());
        }

        // 兜底：某些旧版 git 不支持 `-b main`，再用默认 init 重试
        let mut fallback_cmd = Command::new("git");
        fallback_cmd.arg("init").current_dir(p);
        let fallback = run_git_with_timeout(fallback_cmd, "init", project_path)?;
        if !fallback.status.success() {
            // 兜底也失败，错误信息通常来自 stderr；这里只能粗略标记，由调用方日志定位
            return Err(WorktreeError::GitCommandFailed {
                cmd: "init".into(),
                dir: project_path.to_string(),
                stderr: "git init failed after fallback".into(),
            });
        }
        Ok(())
    }

    /// 基于 `<project>/.worktrees/<todo_id>-<yymmddHHMMss>-<rand8>/` 下创建 worktree。
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

        // 生成唯一标识：`yymmddHHMMss` 时间戳 + 8 hex 随机数（UUIDv4 高 32 bit）。
        // 目录名与分支名共享同一 identity 仅便于 `git worktree list` 人肉对账；
        // cleanup 通过 .git 文件里的 gitdir 指针定位主仓，不需要解析 identity。
        let identity = Self::mint_identity();
        // 分支名提前到这里构造：exists() 命中时 warn 需要把孤儿分支路径一起打出，
        // 便于操作者直接定位并手动清理（git worktree remove + git branch -D）。
        let branch_name = format!("wt-{}-{}", todo_id, &identity);
        let worktree_dir = PathBuf::from(project_path)
            .join(WORKTREE_ROOT_DIR)
            .join(format!("{}-{}", todo_id, &identity));
        if worktree_dir.exists() {
            // 同名目录已存在（典型场景：上一轮 cleanup 未跑 / 同 todo_id 跨进程并发创建竞争）——
            // 不再静默复用：复用「脏」目录会让新执行继承上一次留下的未追踪文件 / 残留分支，
            // 把问题推迟到执行末段更难排查。这里返回 Err 让上层 `resolve_worktree_context`
            // 走 `WorktreeContext::default()` 回退到原始 workspace。Err 路径至少把孤儿路径
            // 打到 warn，便于人工介入清理；不做主动 remove/branch -D 是因为同名 worktree
            // 可能属于另一条正在执行的 execution，主动回收风险大（见 review followup）。
            warn!(
                worktree = %worktree_dir.display(),
                orphan_branch = %branch_name,
                todo_id = todo_id,
                "worktree directory already exists, aborting to let caller fall back; \
                 manual cleanup if stale: git worktree remove --force <dir> && git branch -D <branch>"
            );
            return Err(WorktreeError::GitCommandFailed {
                cmd: "worktree add".into(),
                dir: project_path.to_string(),
                stderr: format!(
                    "worktree directory already exists: {}",
                    worktree_dir.display()
                ),
            });
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
        // 分支名在前面 exists() 检查之前已经构造好（用于 orphan warn），这里直接复用。
        // 分支名只允许 [a-zA-Z0-9_-]，yymmddHHMMss-随机数 格式完全符合规则。
        let mut add_cmd = Command::new("git");
        add_cmd
            .arg("worktree")
            .arg("add")
            .arg("-b")
            .arg(&branch_name)
            .arg(&worktree_dir)
            .arg(&base)
            .current_dir(project_path);
        let out = run_git_with_timeout(add_cmd, "worktree add", project_path)?;
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

        let mut rm_cmd = Command::new("git");
        rm_cmd
            .arg("worktree")
            .arg("remove")
            .arg("--force")
            .arg(path)
            .current_dir(&main_repo);
        let out = run_git_with_timeout(rm_cmd, "worktree remove --force", &main_repo.to_string_lossy());
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

    /// 生成 worktree 目录/分支名的唯一标识后缀。
    ///
    /// 格式：`<yymmddHHMMss>-<8 hex>`，例如 `260618043952-a3f12b4c`。
    /// - `yymmddHHMMss`：UTC 时间的紧凑可读形式，不包含 `-` `:` `.` 等非法分支名字符。
    /// - 8 hex 字符取 UUIDv4 高 32 bit：UUIDv4 共 122 bit 随机（去掉 version 4 bit + variant 2 bit），
    ///   `>> 96` 取出的 32 bit 中前 6 bit 是固定字段，实际熵 ≈ 26 bit（≈ 2^26 = 67M）。
    ///   与测试 doc 已坦诚的 partial entropy degradation 局限对齐——ntd 同 todo_id 在
    ///   同一秒并发远超 8K 量级才会撞 birthday boundary，PR 自述 YAGNI 不做。
    ///   直接用 `as_u128() >> 96` 抽位，不依赖 `simple()` 的字符串格式。
    /// 分支名 = `wt-{todo_id}-{identity}`，目录名 = `{todo_id}-{identity}`。
    fn mint_identity() -> String {
        let now = chrono::Utc::now();
        let ts = now.format("%y%m%d%H%M%S").to_string();
        let rand8 = format!("{:08x}", Uuid::new_v4().as_u128() >> 96);
        format!("{}-{}", ts, rand8)
    }

    /// 探测仓库是否有任意 commit（HEAD 是否解析得到）。
    /// 取代硬编码 "main" 的存在性检查——空仓库 init 后任何分支都没有 commit。
    fn has_any_commit(&self, project_path: &str) -> Result<bool, WorktreeError> {
        let mut cmd = Command::new("git");
        cmd.arg("rev-parse")
            .arg("--verify")
            .arg("HEAD")
            .current_dir(project_path);
        let out = run_git_with_timeout(cmd, "rev-parse --verify HEAD", project_path)?;
        Ok(out.status.success())
    }

    /// 获取当前分支名（空仓库 fallback 到默认 "main"）。
    /// 优先级：`rev-parse --abbrev-ref HEAD` → 当用户处于 detached HEAD 时退到 init.defaultBranch。
    fn current_branch(&self, project_path: &str) -> Result<String, WorktreeError> {
        let mut cmd = Command::new("git");
        cmd.arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .current_dir(project_path);
        let probe = run_git_with_timeout(cmd, "rev-parse --abbrev-ref HEAD", project_path)?;
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
        let mut cmd = Command::new("git");
        cmd.args(["commit", "--allow-empty", "-m", "ntd: initial worktree base"])
            .current_dir(project_path)
            .env("GIT_AUTHOR_NAME", "ntd")
            .env("GIT_AUTHOR_EMAIL", "ntd@localhost")
            .env("GIT_COMMITTER_NAME", "ntd")
            .env("GIT_COMMITTER_EMAIL", "ntd@localhost");
        let commit = run_git_with_timeout(cmd, "commit --allow-empty", project_path)?;
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

    /// 验证 mint_identity 输出格式稳定：12 位数字 + '-' + 8 位小写 hex。
    /// 不再硬编码字面量——直接调 mint_identity 并解析结果，格式漂移会立刻 fail。
    #[test]
    fn test_mint_identity_format() {
        let id = WorktreeService::mint_identity();
        assert_eq!(id.len(), 21, "identity 应该是 21 字符，实际: {}", id);
        assert_eq!(id.chars().nth(12), Some('-'), "分隔符应在第 12 位，实际: {}", id);
        let (ts, rand) = id.split_once('-').expect("包含 '-' 分隔符");
        assert_eq!(ts.len(), 12, "时间戳 12 位数字，实际: {}", ts);
        assert_eq!(rand.len(), 8, "随机段 8 位 hex，实际: {}", rand);
        assert!(ts.chars().all(|c| c.is_ascii_digit()), "时间戳全数字，实际: {}", ts);
        assert!(
            rand.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
            "随机段全小写 hex，实际: {}",
            rand
        );
    }

    /// 验证 mint_identity 在 1 秒内 1000 次调用无碰撞。
    /// 1000 次足以暴露 UUIDv4 退化（如 random 写死 0）；跨秒时 timestamp 也会
    /// 变化，所以「跨秒后靠 timestamp」不会让退化场景漏检——但理论上无法区分
    /// 「rand 退化被 timestamp 救」和「rand 真的够随机」。足够覆盖本 PR 的
    /// 「不撞」立论即可。
    /// 局限：N=1000 也无法检测 partial entropy degradation（如 UUIDv4 退到只剩 24 bit
    /// 熵，2^24 ≈ 16M 远超 1000，采样几乎不撞 → 假阳安全感）。要真测熵质量得 mock
    /// chrono::Utc::now() 拿掉 timestamp 救场再跑大 N —— YAGNI 不做。
    #[test]
    fn test_mint_identity_uniqueness_within_one_second() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = WorktreeService::mint_identity();
            assert!(seen.insert(id), "collision within 1000 calls");
        }
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
        // 把 todo_id 抽成局部变量，后续断言直接引用它，避免 hardcode "1" 假阳：
        // 将来如果有人把这里改成 2/3/...，旧的断言会立刻 fail 而不是默默通过。
        let todo_id: i64 = 1;
        let wt = svc
            .create_worktree(dir.path().to_str().unwrap(), todo_id)
            .expect("create worktree");
        let wt_path = PathBuf::from(&wt);
        assert!(wt_path.exists(), "worktree dir should exist after create");
        // 验证 create_worktree 产出的路径格式：<project>/.worktrees/<todo_id>-<12digits>-<8hex>
        // 走真实生产路径，覆盖 inline 在 create_worktree 里的 format 串；
        // 替代旧版 test_worktree_path_format（用字面量、未调 mint_identity 的假阳测试）。
        let wt_name = wt_path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("worktree dir name utf8");
        let parts: Vec<&str> = wt_name.split('-').collect();
        assert_eq!(parts.len(), 3, "expected 3 '-'-separated parts, got: {}", wt_name);
        assert_eq!(parts[0], todo_id.to_string(), "todo_id 前缀不匹配, got: {}", wt_name);
        assert_eq!(parts[1].len(), 12, "timestamp 应 12 位, got: {}", parts[1]);
        assert!(
            parts[1].chars().all(|c| c.is_ascii_digit()),
            "timestamp 全数字, got: {}",
            parts[1]
        );
        assert_eq!(parts[2].len(), 8, "random 应 8 位, got: {}", parts[2]);
        assert!(
            parts[2].chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
            "random 全小写 hex, got: {}",
            parts[2]
        );
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
