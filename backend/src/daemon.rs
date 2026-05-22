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

pub fn handle_daemon_command(action: &DaemonAction) {
    #[cfg(target_os = "macos")]
    { handle_launchd(action); }
    #[cfg(target_os = "linux")]
    { handle_systemd(action); }
    #[cfg(target_os = "windows")]
    { handle_task_scheduler(action); }
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
/// Falls back to current_exe if args[0] is not an absolute path
fn get_ntd_binary_path() -> PathBuf {
    std::env::args()
        .next()
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| std::env::current_exe().expect("Failed to get current executable path"))
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

#[cfg(target_os = "macos")]
fn handle_launchd(action: &DaemonAction) {
    match action {
        DaemonAction::Install { force, .. } => launchd_install(*force),
        DaemonAction::Uninstall { .. } => launchd_uninstall(),
        DaemonAction::Start { .. } => launchd_start(),
        DaemonAction::Stop { .. } => launchd_stop(),
        DaemonAction::Restart { .. } => launchd_restart(),
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
    let mut path_entries = vec![
        format!("{}", home.join(".local/bin").display()),
        format!("{}", home.join(".cargo/bin").display()),
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
    fs::write(&plist_path, generate_launchd_plist()).expect("Failed to write plist");

    let domain = get_launchd_domain();
    let output = Command::new("launchctl")
        .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
        .output()
        .expect("Failed to run launchctl");

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
        fs::write(&plist_path, generate_launchd_plist()).expect("Failed to write plist");
        let _ = Command::new("launchctl")
            .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
            .output();
    }

    let output = Command::new("launchctl")
        .args(["kickstart", &format!("{domain}/{label}")])
        .output()
        .expect("Failed to run launchctl");

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

#[cfg(target_os = "macos")]
fn launchd_restart() {
    launchd_stop();
    std::thread::sleep(std::time::Duration::from_millis(500));
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

    Command::new(full_args[0])
        .args(&full_args[1..])
        .status()
        .expect("Failed to run systemctl. Is systemd installed?")
}

#[cfg(target_os = "linux")]
fn run_systemctl_output(system: bool, args: &[&str]) -> std::process::Output {
    let cmd = systemctl_cmd(system);
    let full_args: Vec<&str> = cmd.iter().copied().chain(args.iter().copied()).collect();

    Command::new(full_args[0])
        .args(&full_args[1..])
        .output()
        .expect("Failed to run systemctl")
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
    let mut path_entries = vec![
        get_ntd_bin_dir().display().to_string(),
        format!("{}", home.join(".local/bin").display()),
        format!("{}", home.join(".npm-global/bin").display()),
        format!("{}", home.join(".cargo/bin").display()),
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
// Windows: Task Scheduler
// =============================================================================

#[cfg(target_os = "windows")]
fn handle_task_scheduler(action: &DaemonAction) {
    match action {
        DaemonAction::Install { force, .. } => task_scheduler_install(*force),
        DaemonAction::Uninstall { .. } => task_scheduler_uninstall(),
        DaemonAction::Start { .. } => task_scheduler_start(),
        DaemonAction::Stop { .. } => task_scheduler_stop(),
        DaemonAction::Restart { .. } => task_scheduler_restart(),
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

    if query.is_ok() && query.unwrap().status.success() && !force {
        println!("Task already exists: {}", TASK_NAME);
        println!("Use --force to reinstall");
        return;
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
        .expect("Failed to run schtasks");

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

#[cfg(target_os = "windows")]
fn task_scheduler_restart() {
    task_scheduler_stop();
    std::thread::sleep(std::time::Duration::from_secs(2));
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

