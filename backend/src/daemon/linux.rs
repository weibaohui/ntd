//! Linux 平台的 daemon 实现：基于 systemd (system / user instance)。
//!
//! 设计要点：
//! - 支持两套 scope：
//!   - system (--system): /etc/systemd/system/ntd.service，需要 root
//!   - user   (默认):     ~/.config/systemd/user/ntd.service，普通用户即可
//! - unit 文件手工拼 PATH（避免依赖 systemd 的 Environment 继承），把
//!   ntd binary 目录、~/.local/bin、~/.cargo/bin 等用户级 bin 放前面，
//!   确保 service 起来后能找到刚 `make install` 的 ntd。
//! - `--run-as-user` 用于 system 模式下指定 service 以哪个非 root 用户运行，
//!   配合 sudo 装系统级 service 的场景。
//! - detached redeploy（升级流程用）单独拆到 `super::redeploy`，这里只做
//!   install/uninstall/start/stop/restart/status。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::common::{ntd_bin_dir, ntd_binary_path};
use super::DaemonAction;

#[allow(unused)]
pub(super) const SERVICE_NAME: &str = "ntd";
#[allow(unused)]
pub(super) const SERVICE_DESCRIPTION: &str = "ntd (Now Task, Done) - AI Task Engine Service";

pub(super) fn handle(action: &DaemonAction) {
    match action {
        DaemonAction::Install { force, system, run_as_user } => {
            install(*force, *system, run_as_user.as_deref())
        }
        DaemonAction::Uninstall { system } => uninstall(*system),
        DaemonAction::Start { system } => start(*system),
        DaemonAction::Stop { system } => stop(*system),
        DaemonAction::Restart { system } => restart(*system),
        DaemonAction::Status { system, verbose } => status(*system, *verbose),
    }
}

fn systemctl_cmd(system: bool) -> Vec<&'static str> {
    if system {
        // system 实例
        vec!["systemctl"]
    } else {
        // user 实例，普通用户就能管自己 ~/.config/systemd/user/
        vec!["systemctl", "--user"]
    }
}

fn unit_path(system: bool) -> PathBuf {
    let name = format!("{SERVICE_NAME}.service");
    if system {
        PathBuf::from("/etc/systemd/system").join(&name)
    } else {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        home.join(".config/systemd/user").join(&name)
    }
}

fn run_systemctl(system: bool, args: &[&str]) -> std::process::ExitStatus {
    let cmd = systemctl_cmd(system);
    let full_args: Vec<&str> = cmd.iter().copied().chain(args.iter().copied()).collect();

    Command::new(full_args[0])
        .args(&full_args[1..])
        .status()
        .expect("Failed to run systemctl. Is systemd installed?")
}

fn run_systemctl_output(system: bool, args: &[&str]) -> std::process::Output {
    let cmd = systemctl_cmd(system);
    let full_args: Vec<&str> = cmd.iter().copied().chain(args.iter().copied()).collect();

    Command::new(full_args[0])
        .args(&full_args[1..])
        .output()
        .expect("Failed to run systemctl")
}

/// 从 /etc/passwd 解析 username 对应的 home 目录。
///
/// systemd 安装时需要设 WorkingDirectory / HOME / USER 等环境变量，
/// 但 service 是以另一个用户身份跑的（system 模式），当前进程的 SUDO_USER
/// 反映不出"目标用户"的 home，必须查 /etc/passwd。
fn user_home_dir(username: &str) -> Option<PathBuf> {
    let content = fs::read_to_string("/etc/passwd").ok()?;
    for line in content.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() >= 6 && fields[0] == username {
            return Some(PathBuf::from(fields[5]));
        }
    }
    None
}

fn generate_unit(system: bool, run_as_user: Option<&str>) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));

    if system {
        // system 模式必须显式指定运行用户，否则 service 以 root 跑会引入提权风险
        let username = run_as_user.map(|s| s.to_string()).unwrap_or_else(|| {
            std::env::var("SUDO_USER")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "nobody".to_string())
        });

        // 拒绝 root 运行：systemd 不允许 Type=simple + User=root 的组合，
        // 而且让 ntd 以 root 长跑也违反最小权限原则
        if username == "root" {
            eprintln!("Refusing to install system service as root. Use --run-as-user to specify a user.");
            std::process::exit(1);
        }

        let user_home = user_home_dir(&username)
            .unwrap_or_else(|| PathBuf::from(format!("/home/{username}")));
        let user_binary = ntd_binary_path();

        let mut path_entries = vec![
            ntd_bin_dir().display().to_string(),
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

    // user 模式：直接以当前用户身份跑，不需要 WorkingDirectory / User 字段
    let binary = ntd_binary_path();
    // 当前 binary 目录优先，然后用户级 PATH，最后系统级 PATH
    let mut path_entries = vec![
        ntd_bin_dir().display().to_string(),              // 当前 ntd binary 所在目录
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

fn install(force: bool, system: bool, run_as_user: Option<&str>) {
    if system {
        crate::sys::require_root_or_exit("install");
    }

    let unit_path = unit_path(system);

    if unit_path.exists() && !force {
        println!("Service already installed at: {}", unit_path.display());
        println!("Use --force to reinstall");
        return;
    }

    // system 模式下 binary 始终是当前 sudo 上下文看到的路径；
    // user 模式同理。两者都指向 ntd_binary_path()，保留这块逻辑
    // 是为了以后 system 模式支持"为指定用户预先解析 binary 位置"留接口。
    let binary = if system {
        let username = run_as_user.map(|s| s.to_string()).unwrap_or_else(|| {
            std::env::var("SUDO_USER")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "nobody".to_string())
        });
        let _user_home = user_home_dir(&username)
            .unwrap_or_else(|| PathBuf::from(format!("/home/{username}")));
        ntd_binary_path()
    } else {
        ntd_binary_path()
    };
    if !binary.exists() {
        eprintln!("ntd binary not found at {}. Run `make install` first.", binary.display());
        std::process::exit(1);
    }

    unit_path.parent().map(|p| fs::create_dir_all(p).ok());

    let scope = if system { "system" } else { "user" };
    println!("Installing {scope} systemd service to: {}", unit_path.display());

    fs::write(&unit_path, generate_unit(system, run_as_user))
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

    // user 模式单独提示 linger 状态：没开 linger 的话，logout 后 user
    // manager 会退出，service 跟着停。这是 systemd 设计，不是 ntd bug，
    // 但首次安装时提示一次能省掉"为什么我退出 SSH service 就没了"的工单。
    if !system {
        super::redeploy::check_linger();
    }
}

fn uninstall(system: bool) {
    if system {
        crate::sys::require_root_or_exit("uninstall");
    }

    // stop / disable 都允许失败：service 没起来或没 enable 是正常状态
    let _ = run_systemctl(system, &["stop", SERVICE_NAME]);
    let _ = run_systemctl(system, &["disable", SERVICE_NAME]);

    let unit_path = unit_path(system);
    if unit_path.exists() {
        fs::remove_file(&unit_path).ok();
        println!("Removed {}", unit_path.display());
    }

    run_systemctl(system, &["daemon-reload"]);
    println!("Service uninstalled");
}

fn start(system: bool) {
    if system {
        crate::sys::require_root_or_exit("start");
    }

    let status = run_systemctl(system, &["start", SERVICE_NAME]);
    if status.success() {
        println!("Service started");
    } else {
        eprintln!("Failed to start service");
        std::process::exit(1);
    }
}

fn stop(system: bool) {
    if system {
        crate::sys::require_root_or_exit("stop");
    }

    let status = run_systemctl(system, &["stop", SERVICE_NAME]);
    if status.success() {
        println!("Service stopped");
    } else {
        eprintln!("Failed to stop service");
        std::process::exit(1);
    }
}

fn restart(system: bool) {
    if system {
        crate::sys::require_root_or_exit("restart");
    }

    let status = run_systemctl(system, &["restart", SERVICE_NAME]);
    if status.success() {
        println!("Service restarted");
    } else {
        eprintln!("Failed to restart service");
        std::process::exit(1);
    }
}

fn status(system: bool, verbose: bool) {
    let unit_path = unit_path(system);

    if !unit_path.exists() {
        println!("Service is not installed");
        let sudo = if system { "sudo " } else { "" };
        println!("  Run: {sudo}ntd daemon install{}", if system { " --system" } else { "" });
        return;
    }

    // status 命令用户大概率想看 raw 输出，所以 stdout/stderr 都直接转发，
    // 不做美化，让 systemctl 自带的彩色 / 排版完整呈现
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
        super::redeploy::check_linger();
    }
}
