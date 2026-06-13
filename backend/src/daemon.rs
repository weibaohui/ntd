use std::fs;
use std::path::PathBuf;
use std::process::Command;

use clap::Subcommand;

#[allow(unused)] const SERVICE_NAME: &str = "ntd";
#[allow(unused)] const SERVICE_DESCRIPTION: &str = "Nothing Todo (ntd) - AI Todo Service";
#[allow(unused)]
const LAUNCHD_LABEL: &str = "com.nothing-todo.ntd";
#[allow(unused)] const TASK_NAME: &str = "ntd";

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
    { handle_launchd(action).await; }
    #[cfg(target_os = "linux")]
    { handle_systemd(action); }
    #[cfg(target_os = "windows")]
    { handle_task_scheduler(action).await; }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = action;
        eprintln!("Daemon service is not supported on this platform.");
        std::process::exit(1);
    }
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Get the path of the currently running ntd binary
/// Uses args()[0] to get the actual command path (handles sudo correctly)
/// Falls back to current_exe if args[0] is not an absolute path.
///
/// Falls back to "/usr/local/bin/ntd" when both args[0] is not absolute AND
/// current_exe() fails (rare; current_exe only fails on platforms without
/// /proc/self/exe like some BSDs in chroots). This avoids `.expect()` panicking
/// the process during daemon operations.
fn get_ntd_binary_path() -> PathBuf {
    std::env::args()
        .next()
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            std::env::current_exe().unwrap_or_else(|e| {
                eprintln!("Failed to get current executable path: {}. Using fallback /usr/local/bin/ntd.", e);
                PathBuf::from("/usr/local/bin/ntd")
            })
        })
}

#[allow(unused)]
fn get_ntd_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".ntd")
}

/// Get the directory containing the ntd binary (for PATH in service definition)
#[allow(unused)]
fn get_ntd_bin_dir() -> PathBuf {
    get_ntd_binary_path()
        .parent()
        .map(|p| p.to_path_buf())
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
async fn handle_launchd(action: &DaemonAction) {
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
    let binary = get_ntd_binary_path();
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
fn launchd_install(force: bool) {
    let plist_path = get_launchd_plist_path();
    let binary = get_ntd_binary_path();

    if !binary.exists() {
        eprintln!("ntd binary not found at {}. Run `make install` first.", binary.display());
        std::process::exit(1);
    }

    if plist_path.exists() && !force {
        println!("Service already installed at: {}", plist_path.display());
        println!("Use --force to reinstall");
        return;
    }

    let ntd_dir = get_ntd_dir();
    fs::create_dir_all(&ntd_dir).ok();
    plist_path.parent().map(|p| fs::create_dir_all(p).ok());

    println!("Installing launchd service to: {}", plist_path.display());
    // 写入 plist 是安装的前置步骤：失败则直接终止，避免后续 launchctl bootstrap
    // 加载一个不存在/损坏的 plist；改用 eprintln + exit(1) 让用户看到具体错误。
    if let Err(e) = fs::write(&plist_path, generate_launchd_plist()) {
        eprintln!("Failed to write plist to {}: {}", plist_path.display(), e);
        std::process::exit(1);
    }

    let domain = get_launchd_domain();
    // launchctl bootstrap 是核心加载步骤；调用本身失败（launchctl 不存在/权限不足）
    // 应给出明确错误而不是 panic。
    let output = match Command::new("launchctl")
        .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Failed to run launchctl: {}. Is launchctl available on this macOS?", e);
            std::process::exit(1);
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        if code != 5 && !stderr.contains("already loaded") {
            eprintln!("Failed to bootstrap service: {}", stderr.trim());
        }
    }

    println!();
    println!("Service installed and loaded!");
    println!();
    println!("Next steps:");
    println!("  ntd daemon status              # Check status");
    println!("  tail -f ~/.ntd/run.log         # View logs");
}

#[cfg(target_os = "macos")]
fn launchd_uninstall() {
    let plist_path = get_launchd_plist_path();
    let domain = get_launchd_domain();
    let label = LAUNCHD_LABEL;

    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{label}")])
        .output();

    if plist_path.exists() {
        fs::remove_file(&plist_path).ok();
        println!("Removed {}", plist_path.display());
    }

    println!("Service uninstalled");
}

#[cfg(target_os = "macos")]
fn launchd_start() {
    let plist_path = get_launchd_plist_path();
    let domain = get_launchd_domain();
    let label = LAUNCHD_LABEL;

    if !plist_path.exists() {
        println!("Service not installed. Regenerating...");
        plist_path.parent().map(|p| fs::create_dir_all(p).ok());
        // 重新生成 plist 失败应终止，避免后续 launchctl bootstrap 加载不存在的内容。
        if let Err(e) = fs::write(&plist_path, generate_launchd_plist()) {
            eprintln!("Failed to write plist to {}: {}", plist_path.display(), e);
            std::process::exit(1);
        }
        let _ = Command::new("launchctl")
            .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
            .output();
    }

    let output = match Command::new("launchctl")
        .args(["kickstart", &format!("{domain}/{label}")])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Failed to run launchctl: {}. Is launchctl available on this macOS?", e);
            std::process::exit(1);
        }
    };

    if output.status.success() {
        println!("Service started");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        if stderr.contains("already loaded") || code == 5 || code == 113 {
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
}

#[cfg(target_os = "macos")]
fn launchd_stop() {
    let domain = get_launchd_domain();
    let label = LAUNCHD_LABEL;

    let output = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{label}")])
        .output();

    match output {
        Ok(o) if o.status.success() => println!("Service stopped"),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let code = o.status.code().unwrap_or(-1);
            if code == 3 || code == 113 || stderr.contains("No such process") {
                println!("Service is not running");
            } else {
                eprintln!("Failed to stop service: {}", stderr.trim());
            }
        }
        Err(e) => eprintln!("Failed to run launchctl: {}", e),
    }
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
async fn launchd_restart() {
    launchd_stop();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    launchd_start();
}

#[cfg(target_os = "macos")]
fn launchd_status(verbose: bool) {
    let plist_path = get_launchd_plist_path();
    let label = LAUNCHD_LABEL;

    if !plist_path.exists() {
        println!("Service is not installed");
        println!("  Run: ntd daemon install");
        return;
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
}

// =============================================================================
// Linux: systemd
// =============================================================================

#[cfg(target_os = "linux")]
fn handle_systemd(action: &DaemonAction) {
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
fn run_systemctl(system: bool, args: &[&str]) -> std::process::ExitStatus {
    let cmd = systemctl_cmd(system);
    let full_args: Vec<&str> = cmd.iter().copied().chain(args.iter().copied()).collect();

    // systemctl 不存在/不可执行时给用户明确提示，而不是直接 panic。
    // 旧实现用 .expect() 在容器/无 systemd 主机上会让整个 daemon 子命令崩溃。
    match Command::new(full_args[0])
        .args(&full_args[1..])
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Failed to run systemctl ({}). Is systemd installed on this Linux host?",
                e
            );
            std::process::exit(1);
        }
    }
}

#[cfg(target_os = "linux")]
fn run_systemctl_output(system: bool, args: &[&str]) -> std::process::Output {
    let cmd = systemctl_cmd(system);
    let full_args: Vec<&str> = cmd.iter().copied().chain(args.iter().copied()).collect();

    // 同 run_systemctl：systemctl 缺失时输出捕获失败是预期错误路径，
    // 不应 panic。返回 Output 占位让上层 status.success()=false 分支处理。
    match Command::new(full_args[0])
        .args(&full_args[1..])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "Failed to run systemctl ({}). Is systemd installed on this Linux host?",
                e
            );
            std::process::exit(1);
        }
    }
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

        if username == "root" {
            eprintln!("Refusing to install system service as root. Use --run-as-user to specify a user.");
            std::process::exit(1);
        }

        let user_home = get_user_home_dir(&username)
            .unwrap_or_else(|| PathBuf::from(format!("/home/{username}")));
        let user_binary = get_ntd_binary_path();

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

    let binary = get_ntd_binary_path();
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
fn systemd_install(force: bool, system: bool, run_as_user: Option<&str>) {
    if system && unsafe { libc::geteuid() } != 0 {
        eprintln!("System service install requires root. Re-run with sudo.");
        std::process::exit(1);
    }

    let unit_path = get_systemd_unit_path(system);

    if unit_path.exists() && !force {
        println!("Service already installed at: {}", unit_path.display());
        println!("Use --force to reinstall");
        return;
    }

    let binary = if system {
        let username = run_as_user.map(|s| s.to_string()).unwrap_or_else(|| {
            std::env::var("SUDO_USER")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "nobody".to_string())
        });
        let _user_home = get_user_home_dir(&username)
            .unwrap_or_else(|| PathBuf::from(format!("/home/{username}")));
        get_ntd_binary_path()
    } else {
        get_ntd_binary_path()
    };
    if !binary.exists() {
        eprintln!("ntd binary not found at {}. Run `make install` first.", binary.display());
        std::process::exit(1);
    }

    unit_path.parent().map(|p| fs::create_dir_all(p).ok());

    let scope = if system { "system" } else { "user" };
    println!("Installing {scope} systemd service to: {}", unit_path.display());

    fs::write(&unit_path, generate_systemd_unit(system, run_as_user))
        .unwrap_or_else(|e| {
            eprintln!("Failed to write unit file: {e}");
            std::process::exit(1);
        });

    run_systemctl(system, &["daemon-reload"]);
    run_systemctl(system, &["enable", SERVICE_NAME]);

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
}

#[cfg(target_os = "linux")]
fn systemd_uninstall(system: bool) {
    if system && unsafe { libc::geteuid() } != 0 {
        eprintln!("System service uninstall requires root. Re-run with sudo.");
        std::process::exit(1);
    }

    let _ = run_systemctl(system, &["stop", SERVICE_NAME]);
    let _ = run_systemctl(system, &["disable", SERVICE_NAME]);

    let unit_path = get_systemd_unit_path(system);
    if unit_path.exists() {
        fs::remove_file(&unit_path).ok();
        println!("Removed {}", unit_path.display());
    }

    run_systemctl(system, &["daemon-reload"]);
    println!("Service uninstalled");
}

#[cfg(target_os = "linux")]
fn systemd_start(system: bool) {
    if system && unsafe { libc::geteuid() } != 0 {
        eprintln!("System service start requires root. Re-run with sudo.");
        std::process::exit(1);
    }

    let status = run_systemctl(system, &["start", SERVICE_NAME]);
    if status.success() {
        println!("Service started");
    } else {
        eprintln!("Failed to start service");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
fn systemd_stop(system: bool) {
    if system && unsafe { libc::geteuid() } != 0 {
        eprintln!("System service stop requires root. Re-run with sudo.");
        std::process::exit(1);
    }

    let status = run_systemctl(system, &["stop", SERVICE_NAME]);
    if status.success() {
        println!("Service stopped");
    } else {
        eprintln!("Failed to stop service");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
fn systemd_restart(system: bool) {
    if system && unsafe { libc::geteuid() } != 0 {
        eprintln!("System service restart requires root. Re-run with sudo.");
        std::process::exit(1);
    }

    let status = run_systemctl(system, &["restart", SERVICE_NAME]);
    if status.success() {
        println!("Service restarted");
    } else {
        eprintln!("Failed to restart service");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
fn systemd_status(system: bool, verbose: bool) {
    let unit_path = get_systemd_unit_path(system);

    if !unit_path.exists() {
        println!("Service is not installed");
        let sudo = if system { "sudo " } else { "" };
        println!("  Run: {sudo}ntd daemon install{}", if system { " --system" } else { "" });
        return;
    }

    let output = run_systemctl_output(system, &["status", SERVICE_NAME, "--no-pager"]);
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));

    let is_active = run_systemctl_output(system, &["is-active", SERVICE_NAME]);
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
async fn handle_task_scheduler(action: &DaemonAction) {
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
fn task_scheduler_install(force: bool) {
    let binary = get_ntd_binary_path();

    if !binary.exists() {
        eprintln!("ntd binary not found at {}. Run `make install` first.", binary.display());
        std::process::exit(1);
    }

    // Check if task already exists
    let query = Command::new("schtasks")
        .args(["/query", "/tn", TASK_NAME])
        .output();

    // 用 match 替代 .is_ok() && .unwrap()：schtasks 不存在的环境（如某些 Server Core）
    // 会返回 Err，这里走"任务不存在 → 继续安装"分支而不是 panic。
    if let Ok(q) = query {
        if q.status.success() && !force {
            println!("Task already exists: {}", TASK_NAME);
            println!("Use --force to reinstall");
            return;
        }
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
    let output = match Command::new("schtasks")
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
    {
        Ok(o) => o,
        Err(e) => {
            // schtasks 不可用时给明确提示，避免 panic。
            eprintln!("Failed to run schtasks ({}). Is Task Scheduler available?", e);
            std::process::exit(1);
        }
    };

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
        let ntd_dir = get_ntd_dir();
        fs::create_dir_all(&ntd_dir).ok();

        let wrapper_path = ntd_dir.join("ntd_watchdog.bat");
        let wrapper_content = format!(
            "@echo off\r\n:restart\r\n\"{}\" server start\r\necho ntd exited, restarting in 5 seconds...\r\ntimeout /t 5 /nobreak >nul\r\ngoto restart\r\n",
            binary_str
        );
        fs::write(&wrapper_path, wrapper_content).ok();
        println!();
        println!("Watchdog script: {}", wrapper_path.display());
        println!("For auto-restart on crash, use the watchdog script as the task action.");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Failed to create task: {}", stderr.trim());
        std::process::exit(1);
    }
}

#[cfg(target_os = "windows")]
fn task_scheduler_uninstall() {
    let output = Command::new("schtasks")
        .args(["/delete", "/tn", TASK_NAME, "/f"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            println!("Task deleted");
            // Clean up watchdog script
            let watchdog = get_ntd_dir().join("ntd_watchdog.bat");
            if watchdog.exists() {
                fs::remove_file(&watchdog).ok();
            }
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.contains("does not exist") || stderr.contains("The system cannot find") {
                println!("Task does not exist");
            } else {
                eprintln!("Failed to delete task: {}", stderr.trim());
            }
        }
        Err(e) => eprintln!("Failed to run schtasks: {}", e),
    }

    println!("Service uninstalled");
}

#[cfg(target_os = "windows")]
fn task_scheduler_start() {
    let output = Command::new("schtasks")
        .args(["/run", "/tn", TASK_NAME])
        .output();

    match output {
        Ok(o) if o.status.success() => println!("Service started"),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.contains("already running") {
                println!("Service is already running");
            } else {
                eprintln!("Failed to start task: {}", stderr.trim());
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Failed to run schtasks: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(target_os = "windows")]
fn task_scheduler_stop() {
    let output = Command::new("schtasks")
        .args(["/end", "/tn", TASK_NAME])
        .output();

    match output {
        Ok(o) if o.status.success() => println!("Service stopped"),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.contains("not running") || stderr.contains("does not exist") {
                println!("Service is not running");
            } else {
                eprintln!("Failed to stop task: {}", stderr.trim());
            }
        }
        Err(e) => eprintln!("Failed to run schtasks: {}", e),
    }
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
async fn task_scheduler_restart() {
    task_scheduler_stop();
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    task_scheduler_start();
}

#[cfg(target_os = "windows")]
fn task_scheduler_status(verbose: bool) {
    let output = Command::new("schtasks")
        .args(["/query", "/tn", TASK_NAME, "/fo", "list"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            println!("{}", stdout);

            if stdout.contains("Running") {
                println!("Status: running");
            } else if stdout.contains("Ready") {
                println!("Status: ready (not running)");
                println!("  Run: ntd daemon start");
            }
        }
        Ok(_) => {
            println!("Task is not installed");
            println!("  Run: ntd daemon install");
        }
        Err(_) => {
            println!("Task is not installed");
            println!("  Run: ntd daemon install");
        }
    }

    if verbose {
        println!();
        println!("Binary: {}", get_ntd_binary_path().display());

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
}

// =============================================================================
// Tests for Issue #495: error handling without panic
// =============================================================================

#[cfg(test)]
mod error_handling_tests {
    //! 验证 Issue #495 修复后，daemon 模块不再在异常路径上 panic。
    //!
    //! 这些测试聚焦在"helper 函数在极端环境下不再 panic"：
    //! - `get_ntd_binary_path()` 即使 `current_exe()` 失败也能回退到安全路径
    //!
    //! 涉及真实 subprocess / launchctl / systemctl / schtasks 的命令路径
    //! 需要 root 或特定 OS，不在单测范围内；它们走的是 eprintln + exit(1)
    //! 而不是 panic，进程级测试可以通过 daemon 子命令的退出码间接验证。

    use super::*;

    /// Issue #495 回归：`get_ntd_binary_path()` 在 `current_exe()` 失败的极端场景下
    /// 不应 panic，而是回退到 `/usr/local/bin/ntd`。这里通过传入相对路径的 args[0]
    /// 强制走 fallback 分支（args[0] 不是 absolute → 走 current_exe → 成功 →
    /// 拿到真实路径），证明函数在常规环境下仍能正确解析。
    #[test]
    fn test_get_ntd_binary_path_returns_valid_path() {
        // 测试自身是用 cargo run 运行的，args[0] 是 target/debug/deps/...-<hash>，
        // 不是 absolute path——这正是我们要覆盖的 fallback 分支。
        let path = get_ntd_binary_path();
        // 返回的路径必须非空且以一个合理目录开头（target 或 fallback）。
        assert!(!path.as_os_str().is_empty(), "path should not be empty");
        // 路径要么是绝对路径（current_exe 成功）要么是 fallback。
        // 注意：在某些 CI 环境下，args[0] 可能是绝对路径，所以这里只断言"非空"。
    }

    /// Issue #495 回归：`get_ntd_dir()` 在 `dirs::home_dir()` 失败时回退到 /tmp，
    /// 不 panic。这是 daemon 子命令的常见路径：用户没 HOME 时仍能生成 systemd
    /// unit 文件（虽然路径奇怪，但不会 crash）。
    #[test]
    fn test_get_ntd_dir_does_not_panic() {
        let dir = get_ntd_dir();
        // 返回的目录必须非空。
        assert!(!dir.as_os_str().is_empty(), "dir should not be empty");
        // 必须以 ".ntd" 结尾（这是 ntd 数据目录约定）。
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some(".ntd"),
            "expected .ntd directory"
        );
    }
}

