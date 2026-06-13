//! Linux 专属的 detached redeploy 实现（升级流程用）。
//!
//! 设计动机：`ntd.service` 的 `KillMode=mixed` 在 `daemon stop` 触发时，
//! 会按 cgroup 清理所有子进程。原实现 `sh -c "ntd daemon stop && ..."` 的
//! 子 shell 仍属于 ntd.service 的 cgroup，会被 SIGKILL 一起带走，导致
//! `uninstall / install --force / start` 三个步骤全部不执行。
//!
//! 根治方法：用 `systemd-run --scope` 把 redeploy 脚本放在独立的
//! transient scope（独立 cgroup）里跑，ntd.service 停止时不会牵连。
//! `--collect` 让 systemd 在 scope 退出后自动 GC，不留垃圾。
//! `--property=KillMode=process` 进一步收紧：即使 systemd 真的想杀 scope，
//! 也只杀 systemd-run 自身，不影响 sh -c 链上的子命令。
//!
//! 因为 `systemd-run` 必须连接到 *当前* 运行的 daemon 所在的 systemd 实例
//! （system 或 --user），所以需要先探测 install mode。
//!
//! 本模块还包含一些 platform-shared 的 helper（如 `check_linger`），
//! 因为它们和 Linux systemd 用户态行为强绑定，放在一起便于维护。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::linux::SERVICE_NAME;

/// ntd daemon 当前所在的 systemd 实例。
///
/// 探测顺序（从最可靠到兜底）：
/// 1. `systemctl show ntd.service` + `FragmentPath`，看是 /etc/systemd/system 还是 ~/.config/systemd/user
/// 2. `/etc/systemd/system/ntd.service` 和 `~/.config/systemd/user/ntd.service` 是否存在
///
/// **平台无关：** 这个枚举本身只是数据，不依赖 systemd，
/// 在 macOS/Windows 上也定义，以便 `build_redeploy_spec` 能在所有平台单测。
/// 实际探测和执行的函数（下面）仍然是 Linux-only。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonInstallMode {
    /// /etc/systemd/system/ntd.service，需要 `systemctl`（无 --user）
    System,
    /// ~/.config/systemd/user/ntd.service，需要 `systemctl --user`
    User,
    /// 探测失败（没有 unit 文件 / 没有 systemd）
    Unknown,
}

/// 探测 ntd 当前以哪种模式安装。
///
/// 实现说明：
/// - 优先 `systemctl show` 直接拿到 `FragmentPath`，从路径前缀判断
///   最准确：既不需要 root 也能识别 user 模式
/// - systemctl 不可用时，直接看磁盘上 unit 文件存在与否
/// - 都查不到返回 Unknown，调用方应决定是降级还是直接报错
pub fn detect_install_mode() -> DaemonInstallMode {
    // 先尝试通过 systemctl 探测 system 实例
    if let Ok(out) = Command::new("systemctl")
        .args(["show", SERVICE_NAME, "--property=FragmentPath", "--property=LoadState"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout.contains("LoadState=loaded") {
            // 路径以 /etc/systemd/system/ 开头即 system 模式
            if stdout.contains("FragmentPath=/etc/systemd/system/") {
                return DaemonInstallMode::System;
            }
            // 路径以 .config/systemd/user/ 开头即 user 模式
            if stdout.contains("FragmentPath=") && stdout.contains(".config/systemd/user/") {
                return DaemonInstallMode::User;
            }
        }
    }

    // systemctl 不可用或没拿到 FragmentPath，降级到读盘
    if PathBuf::from("/etc/systemd/system")
        .join(format!("{SERVICE_NAME}.service"))
        .exists()
    {
        return DaemonInstallMode::System;
    }
    if let Some(home) = dirs::home_dir() {
        if home
            .join(".config/systemd/user")
            .join(format!("{SERVICE_NAME}.service"))
            .exists()
        {
            return DaemonInstallMode::User;
        }
    }

    DaemonInstallMode::Unknown
}

/// 构造 detached redeploy 的进程描述。
///
/// 抽成纯函数是为了能在单测里断言参数顺序，不需要真的 fork systemd-run。
///
/// 平台无关：即便 macOS/Windows 不调用它，放在这里也方便跨平台单测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedeployCommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

pub fn build_redeploy_spec(mode: DaemonInstallMode, script: &str) -> RedeployCommandSpec {
    // User 模式必须加 --user 才能连到用户的 systemd 实例；
    // System 模式和 Unknown 模式都不加，这样即使探测失败回退也能跑
    // （Unknown 时连不上 ntd.service 的实例，但 redeploy 脚本里用的是
    //  ntd 自己的 stop/uninstall/install 逻辑，会重新匹配实际模式）。
    let mut args: Vec<String> = Vec::new();
    if mode == DaemonInstallMode::User {
        args.push("--user".to_string());
    }
    args.extend([
        "--scope".to_string(),
        "--collect".to_string(),
        "--property=Description=ntd upgrade redeploy".to_string(),
        // 即使 scope 内被 kill，也只杀 systemd-run 自身，不杀 sh 链
        "--property=KillMode=process".to_string(),
        "/bin/sh".to_string(),
        "-c".to_string(),
        script.to_string(),
    ]);
    RedeployCommandSpec {
        program: "systemd-run".to_string(),
        args,
    }
}

/// 默认的 redeploy 日志路径，失败时供用户排查。
pub fn redeploy_log_path() -> std::path::PathBuf {
    // 复用 ntd 的状态目录约定 `~/.ntd/`，跟 data.db / daemon.log 放一起
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".ntd")
        .join("upgrade-redeploy.log")
}

/// 真正启动 detached redeploy。
///
/// - `script`：stop && uninstall && install --force && start 的 shell 片段
/// - 返回：Ok(()) 表示 systemd-run 至少拉起了 sh（脚本本身的成败要看日志）
/// - Err：探测/启动/IO 失败，带具体原因
///
/// **stdio 处理**：
/// - stdin 重定向到 /dev/null：防止 sh 等 tty 输入
/// - stdout/stderr 追加写到日志文件：失败时用户能直接 `cat` 排查
pub fn spawn_detached_redeploy(script: &str) -> Result<(), RedeployError> {
    let mode = detect_install_mode();
    let log_path = redeploy_log_path();
    if let Some(parent) = log_path.parent() {
        // 日志目录创建失败也不致命，后面 OpenOptions 会带具体错误
        let _ = fs::create_dir_all(parent);
    }

    let spec = build_redeploy_spec(mode, script);

    // 用 OpenOptions::append 而不是 create，这样多次升级日志会累积，
    // 排查"上次升级到这次升级之间出了什么事"更方便
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| RedeployError::LogOpen {
            path: log_path.clone(),
            source: e,
        })?;

    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args);
    cmd.stdin(std::process::Stdio::null());
    // stdout 和 stderr 都指向同一个文件句柄（用 try_clone 拿到两个独立 fd，
    // 这样两个流是独立打开的，避免共享 buffer 互相阻塞）
    if let Ok(f) = log_file.try_clone() {
        cmd.stdout(f);
    }
    cmd.stderr(log_file);

    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            let msg = format!(
                "redeploy script exited with code {:?}; log: {}",
                s.code(),
                log_path.display()
            );
            tracing::error!("{msg}");
            Err(RedeployError::NonZeroExit { code: s.code(), log: log_path })
        }
        Err(e) => {
            let msg = format!(
                "failed to spawn systemd-run: {e}; log: {}",
                log_path.display()
            );
            tracing::error!("{msg}");
            Err(RedeployError::Spawn { source: e, log: log_path })
        }
    }
}

#[derive(Debug)]
pub enum RedeployError {
    /// 日志文件打不开（权限/磁盘满）
    LogOpen {
        path: PathBuf,
        source: std::io::Error,
    },
    /// systemd-run 启动了但脚本退出码非 0
    NonZeroExit {
        code: Option<i32>,
        log: PathBuf,
    },
    /// 连 systemd-run 都拉不起来（没装 systemd / PATH 里找不到）
    Spawn {
        source: std::io::Error,
        log: PathBuf,
    },
}

impl std::fmt::Display for RedeployError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RedeployError::LogOpen { path, source } => {
                write!(f, "failed to open redeploy log {}: {}", path.display(), source)
            }
            RedeployError::NonZeroExit { code, log } => {
                write!(
                    f,
                    "redeploy script exited with code {:?}; see log: {}",
                    code,
                    log.display()
                )
            }
            RedeployError::Spawn { source, log } => {
                write!(
                    f,
                    "failed to spawn systemd-run ({}); log: {}",
                    source,
                    log.display()
                )
            }
        }
    }
}

impl std::error::Error for RedeployError {}

/// 检查 systemd user instance 的 linger 状态。
///
/// 没开 linger 时，logout 后 user manager 会退出，service 跟着停。
/// 这是 systemd 设计，不是 ntd bug，但首次安装时提示一次能省掉
/// "为什么我退出 SSH service 就没了"的工单。
pub(super) fn check_linger() {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();

    if username.is_empty() {
        return;
    }

    let linger_file = PathBuf::from(format!("/var/lib/systemd/linger/{username}"));
    if linger_file.exists() {
        println!("Linger is enabled (service survives logout)");
        return;
    }

    let output = Command::new("loginctl")
        .args(["show-user", &username, "--property=Linger", "--value"])
        .output();

    match output {
        Ok(o) => {
            let val = String::from_utf8_lossy(&o.stdout).trim().to_lowercase();
            if val == "yes" || val == "true" || val == "1" {
                println!("Linger is enabled (service survives logout)");
            } else {
                println!("Linger is disabled (service may stop when you log out)");
                println!("  Run: sudo loginctl enable-linger {username}");
            }
        }
        Err(_) => {
            println!("Could not check linger status");
            println!("  To enable: sudo loginctl enable-linger {username}");
        }
    }
}
