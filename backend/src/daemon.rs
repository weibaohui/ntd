use std::fs;
use std::path::PathBuf;
use std::process::Command;

use clap::Subcommand;
use thiserror::Error;

#[allow(unused)] const SERVICE_NAME: &str = "ntd";
#[allow(unused)] const SERVICE_DESCRIPTION: &str = "Nothing Todo (ntd) - AI Todo Service";
#[allow(unused)]
const LAUNCHD_LABEL: &str = "com.nothing-todo.ntd";
#[allow(unused)] const TASK_NAME: &str = "ntd";

/// 守护进程管理子命令的失败原因。
///
/// 把所有平台（launchd / systemd / Task Scheduler）的错误统一在一个
/// 枚举里，调用方（`main.rs` 的 CLI 入口）只需匹配一个错误类型并打印
/// 人类可读消息即可。这样就不需要在每个平台分支里散落 `eprintln!` +
/// `std::process::exit(1)`，也避免 `.expect()` 在生产路径上 panic 崩溃。
#[derive(Debug, Error)]
pub enum DaemonError {
    /// 需要 root 权限才能继续（systemd 的 system 模式）
    #[error("this operation requires root; re-run with sudo")]
    RequiresRoot,

    /// 拒绝以 root 身份安装 system 服务
    #[error("refusing to install system service as root; use --run-as-user to specify a user")]
    RefusingRootSystemInstall,

    /// ntd 二进制不存在，提示用户先 make install
    #[error("ntd binary not found at {0}; run `make install` first")]
    BinaryNotFound(PathBuf),

    /// 用户取消操作（例如已经装过了且未指定 --force）
    #[error("{0}")]
    AlreadyInstalled(String),

    /// 平台命令本身拉不起来（launchctl/systemd/schtasks 不可用）
    #[error("failed to run `{0}` ({1}); is the platform service manager installed?")]
    Spawn(String, std::io::Error),

    /// 平台命令退出码非 0
    #[error("`{command}` exited with code {code:?}: {stderr}")]
    NonZeroExit {
        command: String,
        code: Option<i32>,
        stderr: String,
    },

    /// 写文件失败（plist / unit / bat）
    #[error("failed to write {path}: {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// 读 /proc/self/exe 失败（拿不到当前二进制路径）
    #[error("failed to get current executable path: {0}")]
    CurrentExe(#[source] std::io::Error),
}

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
//
// 返回 Result 而不是内部 exit，让调用方决定如何呈现错误（打印 + 退出码），
// 也方便单测里断言失败路径（原先用 process::exit 会让测试进程直接挂掉）。
pub async fn handle_daemon_command(action: &DaemonAction) -> Result<(), DaemonError> {
    #[cfg(target_os = "macos")]
    { handle_launchd(action).await }
    #[cfg(target_os = "linux")]
    { handle_systemd(action) }
    #[cfg(target_os = "windows")]
    { handle_task_scheduler(action).await }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = action;
        Err(DaemonError::Spawn(
            "unsupported platform".to_string(),
            std::io::Error::new(std::io::ErrorKind::Unsupported, "daemon not supported on this platform"),
        ))
    }
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Get the path of the currently running ntd binary
/// Uses args()[0] to get the actual command path (handles sudo correctly)
/// Falls back to current_exe if args[0] is not an absolute path
///
/// 返回 Result 而不是 panic：
/// 在 sandbox / chroot / 权限被剥离等场景下 `current_exe()` 会失败，
/// 原来用 `.expect()` 会让 daemon 子命令直接 panic，给用户的信息
/// 只有一行 `Failed to get current executable path`。
/// 改用 `?` 让上层把错误包装成 `DaemonError::CurrentExe` 打印出来。
fn get_ntd_binary_path() -> Result<PathBuf, DaemonError> {
    if let Some(p) = std::env::args().next().map(PathBuf::from).filter(|p| p.is_absolute()) {
        return Ok(p);
    }
    std::env::current_exe().map_err(DaemonError::CurrentExe)
}

#[allow(unused)]
fn get_ntd_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".ntd")
}

/// Get the directory containing the ntd binary (for PATH in service definition)
///
/// 拿不到 binary path 时退到 `/usr/local/bin`（多数发行版 make install 的默认），
/// 这样 unit / plist 仍能生成——install 命令本身会在最后检查 binary 是否存在。
#[allow(unused)]
fn get_ntd_bin_dir() -> PathBuf {
    get_ntd_binary_path()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"))
}

// =============================================================================
// macOS: launchd
// =============================================================================

// handle_launchd 声明为 async 是为了内部 Restart 分支可以 .await
// launchd_restart();其他分支(start/stop/install/uninstall/status)都是
// 同步阻塞调用,但放在 async fn 里没问题——它们仍然按原样同步执行,
// 只是函数签名统一了。
#[cfg(target_os = "macos")]
async fn handle_launchd(action: &DaemonAction) -> Result<(), DaemonError> {
    match action {
        DaemonAction::Install { force, .. } => launchd_install(*force),
        DaemonAction::Uninstall { .. } => launchd_uninstall(),
        DaemonAction::Start { .. } => launchd_start(),
        DaemonAction::Stop { .. } => launchd_stop(),
        DaemonAction::Restart { .. } => launchd_restart().await,
        DaemonAction::Status { verbose, .. } => launchd_status(*verbose),
    }
}

#[cfg(target_os = "macos")]
fn get_launchd_plist_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("Library").join("LaunchAgents").join(format!("{LAUNCHD_LABEL}.plist"))
}

#[cfg(target_os = "macos")]
fn get_current_uid() -> u32 {
    unsafe { libc::getuid() }
}

#[cfg(target_os = "macos")]
fn get_launchd_domain() -> String {
    format!("gui/{}", get_current_uid())
}

#[cfg(target_os = "macos")]
fn generate_launchd_plist() -> String {
    // plist 是纯字符串生成，binary path 拿不到也不能 panic；
    // install 阶段在 fs::write 之前会再次校验 binary.exists()。
    let binary = get_ntd_binary_path()
        .unwrap_or_else(|_| PathBuf::from("/usr/local/bin/ntd"));
    let ntd_dir = get_ntd_dir();
    let log_path = ntd_dir.join("run.log");
    let err_log_path = ntd_dir.join("run.error.log");
    let label = LAUNCHD_LABEL;

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    // 用户级 PATH 优先于系统级，确保新安装的 ntd 优先被找到
    let mut path_entries = vec![
        format!("{}", home.join(".npm-global/bin").display()), // npm install -g --prefix 写入的可执行文件目录
        format!("{}", home.join(".local/bin").display()),      // make install 的默认安装目录
        format!("{}", home.join(".cargo/bin").display()),      // Rust 工具链（开发环境）
    ];

    if let Ok(current_path) = std::env::var("PATH") {
        for p in current_path.split(':') {
            if !path_entries.contains(&p.to_string()) {
                path_entries.push(p.to_string());
            }
        }
    }

    for entry in ["/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
        let s = entry.to_string();
        if !path_entries.contains(&s) {
            path_entries.push(s);
        }
    }

    let sane_path = path_entries.join(":");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>

    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>server</string>
        <string>start</string>
    </array>

    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>{sane_path}</string>
        <key>HOME</key>
        <string>{home}</string>
    </dict>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>

    <key>StandardOutPath</key>
    <string>{log_path}</string>

    <key>StandardErrorPath</key>
    <string>{err_log_path}</string>
</dict>
</plist>
"#,
        binary = binary.display(),
        log_path = log_path.display(),
        err_log_path = err_log_path.display(),
        home = home.display(),
    )
}

#[cfg(target_os = "macos")]
fn launchd_install(force: bool) -> Result<(), DaemonError> {
    let plist_path = get_launchd_plist_path();
    let binary = get_ntd_binary_path()?;

    if !binary.exists() {
        return Err(DaemonError::BinaryNotFound(binary));
    }

    if plist_path.exists() && !force {
        println!("Service already installed at: {}", plist_path.display());
        println!("Use --force to reinstall");
        return Ok(());
    }

    let ntd_dir = get_ntd_dir();
    // 目录创建失败不致命——后面 fs::write 仍然会报具体错误，
    // 不需要这里用 expect() 提前 panic
    let _ = fs::create_dir_all(&ntd_dir);
    let _ = plist_path.parent().map(|p| fs::create_dir_all(p));

    println!("Installing launchd service to: {}", plist_path.display());
    fs::write(&plist_path, generate_launchd_plist()).map_err(|e| DaemonError::WriteFile {
        path: plist_path.clone(),
        source: e,
    })?;

    let domain = get_launchd_domain();
    let output = Command::new("launchctl")
        .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
        .output()
        .map_err(|e| DaemonError::Spawn("launchctl".to_string(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "already loaded" 表示已经载入,按成功对待
        if !stderr.contains("already loaded") {
            eprintln!("Failed to bootstrap service: {}", stderr.trim());
        }
    }

    println!();
    println!("Service installed and loaded!");
    println!();
    println!("Next steps:");
    println!("  ntd daemon status              # Check status");
    println!("  tail -f ~/.ntd/run.log         # View logs");
    Ok(())
}

#[cfg(target_os = "macos")]
fn launchd_uninstall() -> Result<(), DaemonError> {
    let plist_path = get_launchd_plist_path();
    let domain = get_launchd_domain();
    let label = LAUNCHD_LABEL;

    // bootout 失败不致命（service 可能本来就没跑），所以仍然吞掉
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{label}")])
        .output();

    if plist_path.exists() {
        // 文件删除失败也只 warn 一下：plist 可能已被用户手动清掉
        if let Err(e) = fs::remove_file(&plist_path) {
            eprintln!("Warning: failed to remove plist {}: {}", plist_path.display(), e);
        } else {
            println!("Removed {}", plist_path.display());
        }
    }

    println!("Service uninstalled");
    Ok(())
}

#[cfg(target_os = "macos")]
fn launchd_start() -> Result<(), DaemonError> {
    let plist_path = get_launchd_plist_path();
    let domain = get_launchd_domain();
    let label = LAUNCHD_LABEL;

    if !plist_path.exists() {
        println!("Service not installed. Regenerating...");
        let _ = plist_path.parent().map(|p| fs::create_dir_all(p));
        fs::write(&plist_path, generate_launchd_plist()).map_err(|e| DaemonError::WriteFile {
            path: plist_path.clone(),
            source: e,
        })?;
        let _ = Command::new("launchctl")
            .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
            .output();
    }

    let output = Command::new("launchctl")
        .args(["kickstart", &format!("{domain}/{label}")])
        .output()
        .map_err(|e| DaemonError::Spawn("launchctl".to_string(), e))?;

    if output.status.success() {
        println!("Service started");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // launchd 在某些版本里，kickstart 失败但 service 已 loaded
        // 也会返回非 0；这里 fallback 到 bootstrap + kickstart。
        if stderr.contains("already loaded") {
            let _ = Command::new("launchctl")
                .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
                .output();
            let _ = Command::new("launchctl")
                .args(["kickstart", &format!("{domain}/{label}")])
                .output();
            println!("Service started");
        } else {
            eprintln!("Failed to start service: {}", stderr.trim());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn launchd_stop() -> Result<(), DaemonError> {
    let domain = get_launchd_domain();
    let label = LAUNCHD_LABEL;

    // bootout 对未运行的 service 返回非 0，转换为可读消息而不是 panic
    let output = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{label}")])
        .output()
        .map_err(|e| DaemonError::Spawn("launchctl".to_string(), e))?;

    if output.status.success() {
        println!("Service stopped");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // 3 / 113 / "No such process" 都表示 service 本来就没在跑
        if stderr.contains("No such process") {
            println!("Service is not running");
        } else {
            eprintln!("Failed to stop service: {}", stderr.trim());
        }
    }
    Ok(())
}

// launchd_restart 是 CLI 子命令入口之一,但运行在 #[tokio::main] 上下文,
// 所以可以声明为 async 并 await tokio 的 sleep。
//
// 原实现用 std::thread::sleep(500ms):在异步 runtime 上线程 sleep 会
// 阻塞当前 OS 线程,如果 runtime worker 池被填满,其它请求会被卡住。
// 改用 tokio::time::sleep().await 让出 worker,既不阻塞 runtime,
// 也保留了"等 stop 真正生效再 start"的语义。
//
// 500ms 是经验值:launchd bootout 通常在数十 ms 内完成,但慢盘/僵尸
// 进程可能需要更久。这里不引入 polling(需要重新解析 launchctl list
// 输出判断 PID),保持与原行为等价——只是把阻塞 sleep 换成协作式 sleep。
#[cfg(target_os = "macos")]
async fn launchd_restart() -> Result<(), DaemonError> {
    launchd_stop()?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    launchd_start()
}

#[cfg(target_os = "macos")]
fn launchd_status(verbose: bool) -> Result<(), DaemonError> {
    let plist_path = get_launchd_plist_path();
    let label = LAUNCHD_LABEL;

    if !plist_path.exists() {
        println!("Service is not installed");
        println!("  Run: ntd daemon install");
        return Ok(());
    }

    let output = Command::new("launchctl")
        .args(["list", label])
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if stdout.contains(label) {
                println!("Service is loaded");

                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 && parts[2] == label {
                        if let Ok(pid) = parts[0].parse::<i32>() {
                            if pid > 0 {
                                println!("PID: {}", pid);
                                println!("Status: running");
                            } else {
                                let exit_code = parts[1];
                                println!("Status: stopped (exit code: {})", exit_code);
                            }
                        }
                        break;
                    }
                }
            } else {
                println!("Service is installed but not loaded");
                println!("  Run: ntd daemon start");
            }
        }
        Err(_) => {
            println!("Service is installed but not loaded");
            println!("  Run: ntd daemon start");
        }
    }

    if verbose {
        println!();
        println!("Plist: {}", plist_path.display());
        println!();

        let log_path = get_ntd_dir().join("run.log");
        if log_path.exists() {
            println!("Recent logs:");
            if let Ok(content) = fs::read_to_string(&log_path) {
                for line in content.lines().rev().take(20) {
                    println!("  {}", line);
                }
            }
        }
    }
    Ok(())
}

// =============================================================================
// Linux: systemd
// =============================================================================

#[cfg(target_os = "linux")]
fn handle_systemd(action: &DaemonAction) -> Result<(), DaemonError> {
    match action {
        DaemonAction::Install { force, system, run_as_user } => {
            systemd_install(*force, *system, run_as_user.as_deref())
        }
        DaemonAction::Uninstall { system } => systemd_uninstall(*system),
        DaemonAction::Start { system } => systemd_start(*system),
        DaemonAction::Stop { system } => systemd_stop(*system),
        DaemonAction::Restart { system } => systemd_restart(*system),
        DaemonAction::Status { system, verbose } => systemd_status(*system, *verbose),
    }
}

#[cfg(target_os = "linux")]
fn systemctl_cmd(system: bool) -> Vec<&'static str> {
    if system {
        vec!["systemctl"]
    } else {
        vec!["systemctl", "--user"]
    }
}

#[cfg(target_os = "linux")]
fn get_systemd_unit_path(system: bool) -> PathBuf {
    let name = format!("{SERVICE_NAME}.service");
    if system {
        PathBuf::from("/etc/systemd/system").join(&name)
    } else {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        home.join(".config/systemd/user").join(&name)
    }
}

#[cfg(target_os = "linux")]
fn run_systemctl(system: bool, args: &[&str]) -> Result<std::process::ExitStatus, DaemonError> {
    let cmd = systemctl_cmd(system);
    let full_args: Vec<&str> = cmd.iter().copied().chain(args.iter().copied()).collect();

    Command::new(full_args[0])
        .args(&full_args[1..])
        .status()
        .map_err(|e| DaemonError::Spawn("systemctl".to_string(), e))
}

#[cfg(target_os = "linux")]
fn run_systemctl_output(system: bool, args: &[&str]) -> Result<std::process::Output, DaemonError> {
    let cmd = systemctl_cmd(system);
    let full_args: Vec<&str> = cmd.iter().copied().chain(args.iter().copied()).collect();

    Command::new(full_args[0])
        .args(&full_args[1..])
        .output()
        .map_err(|e| DaemonError::Spawn("systemctl".to_string(), e))
}

#[cfg(target_os = "linux")]
fn get_user_home_dir(username: &str) -> Option<PathBuf> {
    let content = fs::read_to_string("/etc/passwd").ok()?;
    for line in content.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() >= 6 && fields[0] == username {
            return Some(PathBuf::from(fields[5]));
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn generate_systemd_unit(system: bool, run_as_user: Option<&str>) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));

    if system {
        let username = run_as_user.map(|s| s.to_string()).unwrap_or_else(|| {
            std::env::var("SUDO_USER")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "nobody".to_string())
        });

        let user_home = get_user_home_dir(&username)
            .unwrap_or_else(|| PathBuf::from(format!("/home/{username}")));
        // 拿不到 binary path 时退到 /usr/local/bin/ntd，避免 unit 文件
        // 出现空 ExecStart；install 阶段会在写入前再做 exists() 校验。
        let user_binary = get_ntd_binary_path()
            .unwrap_or_else(|_| PathBuf::from("/usr/local/bin/ntd"));

        let mut path_entries = vec![
            get_ntd_bin_dir().display().to_string(),
            format!("{}", user_home.join(".local/bin").display()),
            format!("{}", user_home.join(".npm-global/bin").display()),
            format!("{}", user_home.join(".cargo/bin").display()),
            "/usr/local/sbin".to_string(),
            "/usr/local/bin".to_string(),
            "/usr/sbin".to_string(),
            "/usr/bin".to_string(),
            "/sbin".to_string(),
            "/bin".to_string(),
        ];

        if let Ok(current_path) = std::env::var("PATH") {
            for p in current_path.split(':') {
                if !path_entries.contains(&p.to_string()) {
                    path_entries.push(p.to_string());
                }
            }
        }

        let sane_path = path_entries.join(":");
        let user_home_str = user_home.display();

        return format!(
            r#"[Unit]
Description={SERVICE_DESCRIPTION}
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=600
StartLimitBurst=5

[Service]
Type=simple
User={username}
ExecStart={binary} server start
WorkingDirectory={user_home}
Environment="HOME={user_home}"
Environment="USER={username}"
Environment="LOGNAME={username}"
Environment="PATH={sane_path}"
Restart=on-failure
RestartSec=10
KillMode=mixed
KillSignal=SIGTERM
TimeoutStopSec=60
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
"#,
            binary = user_binary.display(),
            user_home = user_home_str,
        );
    }

    let binary = get_ntd_binary_path()
        .unwrap_or_else(|_| PathBuf::from("/usr/local/bin/ntd"));
    // 当前 binary 目录优先，然后用户级 PATH，最后系统级 PATH
    let mut path_entries = vec![
        get_ntd_bin_dir().display().to_string(),              // 当前 ntd binary 所在目录
        format!("{}", home.join(".local/bin").display()),      // make install 的默认安装目录
        format!("{}", home.join(".npm-global/bin").display()), // npm install -g --prefix 写入的可执行文件目录
        format!("{}", home.join(".cargo/bin").display()),      // Rust 工具链（开发环境）
        "/usr/local/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/sbin".to_string(),
        "/usr/bin".to_string(),
        "/sbin".to_string(),
        "/bin".to_string(),
    ];

    if let Ok(current_path) = std::env::var("PATH") {
        for p in current_path.split(':') {
            if !path_entries.contains(&p.to_string()) {
                path_entries.push(p.to_string());
            }
        }
    }

    let sane_path = path_entries.join(":");

    format!(
        r#"[Unit]
Description={SERVICE_DESCRIPTION}
After=network.target
StartLimitIntervalSec=600
StartLimitBurst=5

[Service]
Type=simple
ExecStart={binary} server start
Environment="PATH={sane_path}"
Restart=on-failure
RestartSec=10
KillMode=mixed
KillSignal=SIGTERM
TimeoutStopSec=60
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
"#,
        binary = binary.display(),
    )
}

#[cfg(target_os = "linux")]
fn systemd_install(force: bool, system: bool, run_as_user: Option<&str>) -> Result<(), DaemonError> {
    if system && unsafe { libc::geteuid() } != 0 {
        return Err(DaemonError::RequiresRoot);
    }

    let unit_path = get_systemd_unit_path(system);

    if unit_path.exists() && !force {
        println!("Service already installed at: {}", unit_path.display());
        println!("Use --force to reinstall");
        return Ok(());
    }

    let binary = if system {
        // 在 system 模式下明确拒绝 root：systemd 不允许 User=root 跑 Type=simple
        // 服务。原先用 eprintln!+exit 这里改成 Result，错误由 main.rs 统一处理。
        let username = run_as_user.map(|s| s.to_string()).unwrap_or_else(|| {
            std::env::var("SUDO_USER")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "nobody".to_string())
        });
        if username == "root" {
            return Err(DaemonError::RefusingRootSystemInstall);
        }
        let _user_home = get_user_home_dir(&username)
            .unwrap_or_else(|| PathBuf::from(format!("/home/{username}")));
        get_ntd_binary_path()?
    } else {
        get_ntd_binary_path()?
    };
    if !binary.exists() {
        return Err(DaemonError::BinaryNotFound(binary));
    }

    // 目录创建失败不致命——fs::write 仍会报具体原因
    let _ = unit_path.parent().map(|p| fs::create_dir_all(p));

    let scope = if system { "system" } else { "user" };
    println!("Installing {scope} systemd service to: {}", unit_path.display());

    fs::write(&unit_path, generate_systemd_unit(system, run_as_user)).map_err(|e| {
        DaemonError::WriteFile {
            path: unit_path.clone(),
            source: e,
        }
    })?;

    run_systemctl(system, &["daemon-reload"])?;
    run_systemctl(system, &["enable", SERVICE_NAME])?;

    println!();
    println!("{scope} service installed and enabled!");
    println!();
    let sudo = if system { "sudo " } else { "" };
    println!("Next steps:");
    println!("  {sudo}ntd daemon start{}", if system { " --system" } else { "" });
    println!("  {sudo}ntd daemon status{}", if system { " --system" } else { "" });
    let journal = if system { "journalctl" } else { "journalctl --user" };
    println!("  {journal} -u {SERVICE_NAME} -f  # View logs");

    if !system {
        check_linger();
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn systemd_uninstall(system: bool) -> Result<(), DaemonError> {
    if system && unsafe { libc::geteuid() } != 0 {
        return Err(DaemonError::RequiresRoot);
    }

    // stop / disable 失败容忍（service 可能本来就没在跑 / 没 enable）
    let _ = run_systemctl(system, &["stop", SERVICE_NAME]);
    let _ = run_systemctl(system, &["disable", SERVICE_NAME]);

    let unit_path = get_systemd_unit_path(system);
    if unit_path.exists() {
        if let Err(e) = fs::remove_file(&unit_path) {
            eprintln!("Warning: failed to remove unit file {}: {}", unit_path.display(), e);
        } else {
            println!("Removed {}", unit_path.display());
        }
    }

    run_systemctl(system, &["daemon-reload"])?;
    println!("Service uninstalled");
    Ok(())
}

#[cfg(target_os = "linux")]
fn systemd_start(system: bool) -> Result<(), DaemonError> {
    if system && unsafe { libc::geteuid() } != 0 {
        return Err(DaemonError::RequiresRoot);
    }

    let status = run_systemctl(system, &["start", SERVICE_NAME])?;
    if status.success() {
        println!("Service started");
    } else {
        return Err(DaemonError::NonZeroExit {
            command: "systemctl start".to_string(),
            code: status.code(),
            stderr: "see journalctl for details".to_string(),
        });
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn systemd_stop(system: bool) -> Result<(), DaemonError> {
    if system && unsafe { libc::geteuid() } != 0 {
        return Err(DaemonError::RequiresRoot);
    }

    let status = run_systemctl(system, &["stop", SERVICE_NAME])?;
    if status.success() {
        println!("Service stopped");
    } else {
        return Err(DaemonError::NonZeroExit {
            command: "systemctl stop".to_string(),
            code: status.code(),
            stderr: "see journalctl for details".to_string(),
        });
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn systemd_restart(system: bool) -> Result<(), DaemonError> {
    if system && unsafe { libc::geteuid() } != 0 {
        return Err(DaemonError::RequiresRoot);
    }

    let status = run_systemctl(system, &["restart", SERVICE_NAME])?;
    if status.success() {
        println!("Service restarted");
    } else {
        return Err(DaemonError::NonZeroExit {
            command: "systemctl restart".to_string(),
            code: status.code(),
            stderr: "see journalctl for details".to_string(),
        });
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn systemd_status(system: bool, verbose: bool) -> Result<(), DaemonError> {
    let unit_path = get_systemd_unit_path(system);

    if !unit_path.exists() {
        println!("Service is not installed");
        let sudo = if system { "sudo " } else { "" };
        println!("  Run: {sudo}ntd daemon install{}", if system { " --system" } else { "" });
        return Ok(());
    }

    let output = run_systemctl_output(system, &["status", SERVICE_NAME, "--no-pager"])?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));

    let is_active = run_systemctl_output(system, &["is-active", SERVICE_NAME])?;
    let active = String::from_utf8_lossy(&is_active.stdout).trim().to_string();

    if active == "active" {
        println!("\nService is running");
    } else {
        println!("\nService is stopped");
        let sudo = if system { "sudo " } else { "" };
        println!("  Run: {sudo}ntd daemon start{}", if system { " --system" } else { "" });
    }

    if verbose {
        println!();
        let log_output = Command::new("journalctl")
            .args(if system {
                vec!["-u", SERVICE_NAME, "-n", "20", "--no-pager"]
            } else {
                vec!["--user", "-u", SERVICE_NAME, "-n", "20", "--no-pager"]
            })
            .output();
        if let Ok(o) = log_output {
            println!("Recent logs:");
            print!("{}", String::from_utf8_lossy(&o.stdout));
        }
    }

    if !system {
        check_linger();
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn check_linger() {
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

// =============================================================================
// Linux: detached redeploy (used by Web API upgrade flow)
//
// 设计动机: `ntd.service` 的 `KillMode=mixed` 在 `daemon stop` 触发时,
// 会按 cgroup 清理所有子进程。原实现 `sh -c "ntd daemon stop && ..."` 的
// 子 shell 仍属于 ntd.service 的 cgroup,会被 SIGKILL 一起带走,导致
// `uninstall / install --force / start` 三个步骤全部不执行。
//
// 根治方法: 用 `systemd-run --scope` 把 redeploy 脚本放在独立的
// transient scope(独立 cgroup)里跑,ntd.service 停止时不会牵连。
// `--collect` 让 systemd 在 scope 退出后自动 GC,不留垃圾。
// `--property=KillMode=process` 进一步收紧: 即使 systemd 真的想杀 scope,
// 也只杀 systemd-run 自身,不影响 sh -c 链上的子命令。
//
// 因为 `systemd-run` 必须连接到 *当前* 运行的 daemon 所在的 systemd 实例
// (system 或 --user),所以需要先探测 install mode。
// =============================================================================

/// ntd daemon 当前所在的 systemd 实例。
///
/// 探测顺序(从最可靠到兜底):
/// 1. `systemctl show ntd.service` + `FragmentPath`,看是 /etc/systemd/system 还是 ~/.config/systemd/user
/// 2. `/etc/systemd/system/ntd.service` 和 `~/.config/systemd/user/ntd.service` 是否存在
///
/// **平台无关:** 这个枚举本身只是数据,不依赖 systemd,
/// 在 macOS/Windows 上也定义,以便 `build_redeploy_spec` 能在所有平台单测。
/// 实际探测和执行的函数(下面)仍然是 Linux-only。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonInstallMode {
    /// /etc/systemd/system/ntd.service,需要 `systemctl`(无 --user)
    System,
    /// ~/.config/systemd/user/ntd.service,需要 `systemctl --user`
    User,
    /// 探测失败(没有 unit 文件 / 没有 systemd)
    Unknown,
}

/// 探测 ntd 当前以哪种模式安装。
///
/// 实现说明:
/// - 优先 `systemctl show` 直接拿到 `FragmentPath`,从路径前缀判断
///   最准确:既不需要 root 也能识别 user 模式
/// - systemctl 不可用时,直接看磁盘上 unit 文件存在与否
/// - 都查不到返回 Unknown,调用方应决定是降级还是直接报错
#[cfg(target_os = "linux")]
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

    // systemctl 不可用或没拿到 FragmentPath,降级到读盘
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
/// 抽成纯函数是为了能在单测里断言参数顺序,不需要真的 fork systemd-run。
///
/// 平台无关: 即便 macOS/Windows 不调用它,放在这里也方便跨平台单测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedeployCommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

pub fn build_redeploy_spec(mode: DaemonInstallMode, script: &str) -> RedeployCommandSpec {
    // User 模式必须加 --user 才能连到用户的 systemd 实例;
    // System 模式和 Unknown 模式都不加,这样即使探测失败回退也能跑
    // (Unknown 时连不上 ntd.service 的实例,但 redeploy 脚本里用的是
    //  ntd 自己的 stop/uninstall/install 逻辑,会重新匹配实际模式)。
    let mut args: Vec<String> = Vec::new();
    if mode == DaemonInstallMode::User {
        args.push("--user".to_string());
    }
    args.extend([
        "--scope".to_string(),
        "--collect".to_string(),
        "--property=Description=ntd upgrade redeploy".to_string(),
        // 即使 scope 内被 kill,也只杀 systemd-run 自身,不杀 sh 链
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

/// 默认的 redeploy 日志路径,失败时供用户排查。
#[cfg(target_os = "linux")]
pub fn redeploy_log_path() -> std::path::PathBuf {
    // 复用 ntd 的状态目录约定 `~/.ntd/`,跟 data.db / daemon.log 放一起
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".ntd")
        .join("upgrade-redeploy.log")
}

/// 真正启动 detached redeploy。
///
/// - `script`: stop && uninstall && install --force && start 的 shell 片段
/// - 返回: Ok(()) 表示 systemd-run 至少拉起了 sh(脚本本身的成败要看日志)
/// - Err: 探测/启动/IO 失败,带具体原因
///
/// **stdio 处理**:
/// - stdin 重定向到 /dev/null: 防止 sh 等 tty 输入
/// - stdout/stderr 追加写到日志文件: 失败时用户能直接 `cat` 排查
#[cfg(target_os = "linux")]
pub fn spawn_detached_redeploy(script: &str) -> Result<(), RedeployError> {
    let mode = detect_install_mode();
    let log_path = redeploy_log_path();
    if let Some(parent) = log_path.parent() {
        // 日志目录创建失败也不致命,后面 OpenOptions 会带具体错误
        let _ = fs::create_dir_all(parent);
    }

    let spec = build_redeploy_spec(mode, script);

    // 用 OpenOptions::append 而不是 create,这样多次升级日志会累积,
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
    // stdout 和 stderr 都指向同一个文件句柄(用 try_clone 拿到两个独立 fd,
    // 这样两个流是独立打开的,避免共享 buffer 互相阻塞)
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

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub enum RedeployError {
    /// 日志文件打不开(权限/磁盘满)
    LogOpen {
        path: PathBuf,
        source: std::io::Error,
    },
    /// systemd-run 启动了但脚本退出码非 0
    NonZeroExit {
        code: Option<i32>,
        log: PathBuf,
    },
    /// 连 systemd-run 都拉不起来(没装 systemd / PATH 里找不到)
    Spawn {
        source: std::io::Error,
        log: PathBuf,
    },
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
impl std::error::Error for RedeployError {}

// =============================================================================
// Windows: Task Scheduler
// =============================================================================

// handle_task_scheduler 声明为 async 是为了内部 Restart 分支可以 .await
// task_scheduler_restart();其他分支保持同步,函数签名统一而已。
#[cfg(target_os = "windows")]
async fn handle_task_scheduler(action: &DaemonAction) -> Result<(), DaemonError> {
    match action {
        DaemonAction::Install { force, .. } => task_scheduler_install(*force),
        DaemonAction::Uninstall { .. } => task_scheduler_uninstall(),
        DaemonAction::Start { .. } => task_scheduler_start(),
        DaemonAction::Stop { .. } => task_scheduler_stop(),
        DaemonAction::Restart { .. } => task_scheduler_restart().await,
        DaemonAction::Status { verbose, .. } => task_scheduler_status(*verbose),
    }
}

#[cfg(target_os = "windows")]
fn task_scheduler_install(force: bool) -> Result<(), DaemonError> {
    let binary = get_ntd_binary_path()?;

    if !binary.exists() {
        return Err(DaemonError::BinaryNotFound(binary));
    }

    // Check if task already exists
    // query 失败（schtasks 不在 PATH 上、权限不足等）只 warn，
    // 不阻断重装流程——用户明确 --force 时本就是想覆盖。
    let query = Command::new("schtasks")
        .args(["/query", "/tn", TASK_NAME])
        .output()
        .map_err(|e| DaemonError::Spawn("schtasks".to_string(), e))?;

    if query.status.success() && !force {
        println!("Task already exists: {}", TASK_NAME);
        println!("Use --force to reinstall");
        return Ok(());
    }

    // Delete existing task if force
    if force {
        let _ = Command::new("schtasks")
            .args(["/delete", "/tn", TASK_NAME, "/f"])
            .output();
    }

    let binary_str = binary.to_string_lossy();

    // Create a task that runs at logon, repeats every 1 minute for 1 day (auto-restart),
    // and restarts on failure
    let output = Command::new("schtasks")
        .args([
            "/create",
            "/tn", TASK_NAME,
            "/tr", &format!("\"{}\" server start", binary_str),
            "/sc", "onlogon",
            "/rl", "limited",
            "/f",
            "/it",  // Run only when user is logged on (interactive)
        ])
        .output()
        .map_err(|e| DaemonError::Spawn("schtasks".to_string(), e))?;

    if output.status.success() {
        println!();
        println!("Task Scheduler task created!");
        println!();
        println!("The service will start automatically at logon.");
        println!();
        println!("Next steps:");
        println!("  ntd daemon start                # Start now");
        println!("  ntd daemon status               # Check status");
        println!("  ntd daemon stop                 # Stop");

        // Create a wrapper script for restart-on-failure behavior
        // 目录创建失败不致命——watchdog 是可选优化,失败 warn 一下即可
        let ntd_dir = get_ntd_dir();
        let _ = fs::create_dir_all(&ntd_dir);

        let wrapper_path = ntd_dir.join("ntd_watchdog.bat");
        let wrapper_content = format!(
            "@echo off\r\n:restart\r\n\"{}\" server start\r\necho ntd exited, restarting in 5 seconds...\r\ntimeout /t 5 /nobreak >nul\r\ngoto restart\r\n",
            binary_str
        );
        if let Err(e) = fs::write(&wrapper_path, wrapper_content) {
            eprintln!("Warning: failed to write watchdog script: {}", e);
        } else {
            println!();
            println!("Watchdog script: {}", wrapper_path.display());
            println!("For auto-restart on crash, use the watchdog script as the task action.");
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DaemonError::NonZeroExit {
            command: "schtasks /create".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        });
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn task_scheduler_uninstall() -> Result<(), DaemonError> {
    let output = Command::new("schtasks")
        .args(["/delete", "/tn", TASK_NAME, "/f"])
        .output()
        .map_err(|e| DaemonError::Spawn("schtasks".to_string(), e))?;

    if output.status.success() {
        println!("Task deleted");
        // Clean up watchdog script
        let watchdog = get_ntd_dir().join("ntd_watchdog.bat");
        if watchdog.exists() {
            if let Err(e) = fs::remove_file(&watchdog) {
                eprintln!("Warning: failed to remove watchdog: {}", e);
            }
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("does not exist") || stderr.contains("The system cannot find") {
            println!("Task does not exist");
        } else {
            eprintln!("Failed to delete task: {}", stderr.trim());
        }
    }

    println!("Service uninstalled");
    Ok(())
}

#[cfg(target_os = "windows")]
fn task_scheduler_start() -> Result<(), DaemonError> {
    let output = Command::new("schtasks")
        .args(["/run", "/tn", TASK_NAME])
        .output()
        .map_err(|e| DaemonError::Spawn("schtasks".to_string(), e))?;

    if output.status.success() {
        println!("Service started");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already running") {
            println!("Service is already running");
        } else {
            return Err(DaemonError::NonZeroExit {
                command: "schtasks /run".to_string(),
                code: output.status.code(),
                stderr: stderr.trim().to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn task_scheduler_stop() -> Result<(), DaemonError> {
    let output = Command::new("schtasks")
        .args(["/end", "/tn", TASK_NAME])
        .output()
        .map_err(|e| DaemonError::Spawn("schtasks".to_string(), e))?;

    if output.status.success() {
        println!("Service stopped");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not running") || stderr.contains("does not exist") {
            println!("Service is not running");
        } else {
            eprintln!("Failed to stop task: {}", stderr.trim());
        }
    }
    Ok(())
}

// Windows Task Scheduler restart:stop → 等 → start。
//
// 原实现 std::thread::sleep(2s) 在 tokio runtime 上同样会阻塞当前
// OS 线程;改用 tokio::time::sleep().await 让出 worker。
//
// 时长:第一次提交缩到 500ms 对齐 launchd 路径,但收到 review 反馈
// 在慢盘/慢 VM/AV 扫描下可能 race(schtasks /end 长尾可达数百 ms,
// start 早于 stop 完全生效会撞上 "Service is already running" 分支)。
// 这里改用 1s 作为折中:既比原 2s 显著缩短(用户感受的 CLI 等待),
// 又保留足够冗余覆盖慢主机的长尾场景。比 launchd 的 500ms 更保守,
// 因为 schtasks /end → start 的串行依赖比 launchd bootout 更紧。
#[cfg(target_os = "windows")]
async fn task_scheduler_restart() -> Result<(), DaemonError> {
    task_scheduler_stop()?;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    task_scheduler_start()
}

#[cfg(target_os = "windows")]
fn task_scheduler_status(verbose: bool) -> Result<(), DaemonError> {
    let output = Command::new("schtasks")
        .args(["/query", "/tn", TASK_NAME, "/fo", "list"])
        .output()
        .map_err(|e| DaemonError::Spawn("schtasks".to_string(), e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("{}", stdout);

        if stdout.contains("Running") {
            println!("Status: running");
        } else if stdout.contains("Ready") {
            println!("Status: ready (not running)");
            println!("  Run: ntd daemon start");
        }
    } else {
        println!("Task is not installed");
        println!("  Run: ntd daemon install");
    }

    if verbose {
        println!();
        // 拿不到 binary path 也只 warn,不能因为 verbose 输出就 fail
        if let Ok(binary) = get_ntd_binary_path() {
            println!("Binary: {}", binary.display());
        }

        let log_path = get_ntd_dir().join("run.log");
        if log_path.exists() {
            println!();
            println!("Recent logs ({}):", log_path.display());
            if let Ok(content) = fs::read_to_string(&log_path) {
                for line in content.lines().rev().take(20) {
                    println!("  {}", line);
                }
            }
        }
    }
    Ok(())
}

// =============================================================================
// Tests
// =============================================================================
//
// 这些测试覆盖 #495 issue 的核心契约：
// 1) DaemonError 的 Display 输出人类可读
// 2) 错误原因链（thiserror #[source]）能透传到底层 std::io::Error
// 3) 平台无关的纯函数（build_redeploy_spec、DaemonInstallMode）跨平台断言
//
// 涉及 platform-cfg 的函数（launchd_*、systemd_*、task_scheduler_*）不在这里
// 测,因为 CI 主要跑 Linux；那些函数的"返回 Result"编译期就保证了不 panic。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_error_requires_root_is_user_facing() {
        // sudo 提示是给运维人员看的，应该是英文短句而不是技术错误码
        let err = DaemonError::RequiresRoot;
        assert_eq!(err.to_string(), "this operation requires root; re-run with sudo");
    }

    #[test]
    fn daemon_error_binary_not_found_includes_path() {
        // 路径要带在错误里，方便用户交叉检查 PATH/安装位置
        let path = PathBuf::from("/opt/nonexistent/ntd");
        let err = DaemonError::BinaryNotFound(path.clone());
        let msg = err.to_string();
        assert!(msg.contains("not found"), "msg should say 'not found': {msg}");
        assert!(msg.contains("/opt/nonexistent/ntd"), "msg should include path: {msg}");
    }

    #[test]
    fn daemon_error_non_zero_exit_includes_stderr() {
        // 错误信息要能让用户看到 schtasks/systemctl 报的具体原因
        let err = DaemonError::NonZeroExit {
            command: "systemctl start".to_string(),
            code: Some(1),
            stderr: "Unit not found".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("systemctl start"));
        assert!(msg.contains("Unit not found"));
        assert!(msg.contains("1"));
    }

    #[test]
    fn daemon_error_write_file_preserves_source() {
        // #[source] 让 anyhow 在打印时把底层 io::Error 也带出来
        use std::error::Error as _;
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = DaemonError::WriteFile {
            path: PathBuf::from("/etc/systemd/system/ntd.service"),
            source: io_err,
        };
        let msg = err.to_string();
        assert!(msg.contains("ntd.service"));
        // source 链可以 walk
        let source = err.source().expect("WriteFile should expose source");
        assert!(source.to_string().contains("denied"));
    }

    #[test]
    fn daemon_error_refuse_root_system_install_is_descriptive() {
        let err = DaemonError::RefusingRootSystemInstall;
        let msg = err.to_string();
        assert!(msg.contains("root"));
        assert!(msg.contains("--run-as-user"));
    }

    // 平台无关的 redeploy spec 在所有平台都能测，用来钉死参数顺序
    #[test]
    fn build_redeploy_spec_user_mode_adds_user_flag() {
        let spec = build_redeploy_spec(DaemonInstallMode::User, "echo hi");
        assert_eq!(spec.program, "systemd-run");
        // 第一参数必须是 --user（User 模式）
        assert_eq!(spec.args.first().map(String::as_str), Some("--user"));
        // 末尾是 /bin/sh -c <script>
        assert!(spec.args.contains(&"/bin/sh".to_string()));
        assert!(spec.args.contains(&"-c".to_string()));
        assert!(spec.args.contains(&"echo hi".to_string()));
    }

    #[test]
    fn build_redeploy_spec_system_mode_omits_user_flag() {
        let spec = build_redeploy_spec(DaemonInstallMode::System, "echo hi");
        // System 模式不加 --user，连得上 system 实例
        assert_ne!(spec.args.first().map(String::as_str), Some("--user"));
    }

    #[test]
    fn build_redeploy_spec_unknown_mode_omits_user_flag() {
        // Unknown 时脚本里会自己重新探测模式，不在这里加 --user
        let spec = build_redeploy_spec(DaemonInstallMode::Unknown, "echo hi");
        assert_ne!(spec.args.first().map(String::as_str), Some("--user"));
    }
}

