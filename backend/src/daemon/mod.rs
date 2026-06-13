//! Daemon service management for ntd.
//!
//! 模块拆分说明（issue #505 重构）：
//! - 入口和 clap 子命令枚举在本文件（`DaemonAction`）。
//! - 跨平台共享的路径定位工具在 [`common`]。
//! - 每个 OS 各占一个子模块：`macos` (launchd) / `linux` (systemd) / `windows` (Task Scheduler)。
//! - Linux 专属的 detached redeploy（升级流程用）单独拆到 [`redeploy`]，
//!   这样 macOS/Windows 编译时不会带进 systemd-run 相关代码。
//!
//! Public 表面（与拆分前保持完全一致，调用方不需要改动）：
//! - [`DaemonAction`] —— clap derive 的子命令枚举
//! - [`handle_daemon_command`] —— 根据 `target_os` 分发到对应平台模块
//! - [`spawn_detached_redeploy`] —— Linux 升级时独立 cgroup 拉起 stop→install→start 脚本

use clap::Subcommand;

// 平台无关子模块：路径定位等 helper
mod common;

// 三个平台各自的 service 后端实现
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

// Linux 专属的 detached redeploy，独立成模块以便在所有平台单测 `build_redeploy_spec`
#[cfg(target_os = "linux")]
mod redeploy;

// 把 Linux-only 的 detached redeploy 在 daemon 模块根上 re-export，
// 保留 `ntd::daemon::spawn_detached_redeploy` / `build_redeploy_spec` 等
// 旧路径，避免外部调用方和单测改动。
#[cfg(target_os = "linux")]
pub use redeploy::{
    build_redeploy_spec, spawn_detached_redeploy, DaemonInstallMode, RedeployCommandSpec,
    RedeployError,
};

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Install ntd as a system daemon (launchd/systemd/Task Scheduler)
    Install {
        /// Force reinstall even if already installed
        #[arg(short, long)]
        force: bool,
        /// Install as system-level service (requires sudo on Linux)
        #[arg(long)]
        system: bool,
        /// User to run the service as (system service only, Linux)
        #[arg(long)]
        run_as_user: Option<String>,
    },
    /// Uninstall the ntd daemon service
    Uninstall {
        /// Uninstall system-level service (requires sudo on Linux)
        #[arg(long)]
        system: bool,
    },
    /// Start the ntd daemon service
    Start {
        /// Start system-level service (requires sudo on Linux)
        #[arg(long)]
        system: bool,
    },
    /// Stop the ntd daemon service
    Stop {
        /// Stop system-level service (requires sudo on Linux)
        #[arg(long)]
        system: bool,
    },
    /// Restart the ntd daemon service
    Restart {
        /// Restart system-level service (requires sudo on Linux)
        #[arg(long)]
        system: bool,
    },
    /// Show daemon service status
    Status {
        /// Show system-level service status (requires sudo on Linux)
        #[arg(long)]
        system: bool,
        /// Show detailed status with recent logs
        #[arg(short, long)]
        verbose: bool,
    },
}

// 调度入口声明为 async：Windows / macOS 的 restart 路径需要 await
// tokio::time::sleep(...).await,而不是阻塞线程的 std::thread::sleep。
// 在 #[tokio::main] 里调用方自然处于 async 上下文,改成 async 没额外成本。
pub async fn handle_daemon_command(action: &DaemonAction) {
    #[cfg(target_os = "macos")]
    { macos::handle(action).await; }
    #[cfg(target_os = "linux")]
    { linux::handle(action); }
    #[cfg(target_os = "windows")]
    { windows::handle(action).await; }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = action;
        eprintln!("Daemon service is not supported on this platform.");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    //! 模块拆分后的回归保护：clap 子命令枚举对外是稳定 ABI（main.rs 直接依赖），
    //! 任何重构都不能误删变体或改字段名，否则下游 CLI 解析就崩了。
    //! 这里用 `match` 把所有变体匹配一遍 —— 如果新增变体而没在这里加 case，
    //! 或者删除某个变体，编译器会立刻报 non-exhaustive match 错误。
    //!
    //! `use super::*;` 显式导入父模块的所有可见 item，避免嵌套模块里
    //! 出现"DaemonAction 找不到"的迷惑错误（嵌套 mod 的可见性规则）。
    use super::*;

    #[test]
    fn test_daemon_action_variants_are_exhaustive() {
        // 通过把每个变体构造出来 + match 全覆盖，强制未来重构保留所有分支
        let actions = vec![
            DaemonAction::Install { force: false, system: false, run_as_user: None },
            DaemonAction::Install { force: true, system: true, run_as_user: Some("ntd".into()) },
            DaemonAction::Uninstall { system: false },
            DaemonAction::Uninstall { system: true },
            DaemonAction::Start { system: false },
            DaemonAction::Start { system: true },
            DaemonAction::Stop { system: false },
            DaemonAction::Stop { system: true },
            DaemonAction::Restart { system: false },
            DaemonAction::Restart { system: true },
            DaemonAction::Status { system: false, verbose: false },
            DaemonAction::Status { system: true, verbose: true },
        ];
        for action in &actions {
            let label = match action {
                DaemonAction::Install { .. } => "install",
                DaemonAction::Uninstall { .. } => "uninstall",
                DaemonAction::Start { .. } => "start",
                DaemonAction::Stop { .. } => "stop",
                DaemonAction::Restart { .. } => "restart",
                DaemonAction::Status { .. } => "status",
            };
            assert!(!label.is_empty());
        }
        assert_eq!(actions.len(), 12);
    }
}
