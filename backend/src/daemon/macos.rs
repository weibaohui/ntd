//! macOS 平台的 daemon 实现：基于 launchd。
//!
//! 关键设计：
//! - `handle_launchd` 声明为 async 是为了让内部 Restart 分支可以 await
//!   `tokio::time::sleep`；其他分支（start/stop/install/uninstall/status）
//!   都是同步阻塞调用，但放在 async fn 里没问题 —— 它们仍按原样同步执行，
//!   只是函数签名统一了。
//! - PATH 在 plist 里手工拼一份"健全"的：用户级 bin 目录优先于系统级，
//!   这样 `make install` / `npm i -g` 装的 ntd 能被 service 找到。
//! - restart 改用 `tokio::time::sleep(500ms)` 让出 worker，不再阻塞 runtime。
//!   500ms 是经验值：launchd bootout 通常在数十 ms 内完成，慢盘/僵尸
//!   进程可能需要更久。这里不引入 polling（需要重新解析 launchctl list
//!   输出判断 PID），保持与原行为等价 —— 只是把阻塞 sleep 换成协作式 sleep。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::common::{ntd_binary_path, ntd_dir};
use super::DaemonAction;

#[allow(unused)]
const LAUNCHD_LABEL: &str = "com.nothing-todo.ntd";

pub(super) async fn handle(action: &DaemonAction) {
    match action {
        DaemonAction::Install { force, .. } => install(*force),
        DaemonAction::Uninstall { .. } => uninstall(),
        DaemonAction::Start { .. } => start(),
        DaemonAction::Stop { .. } => stop(),
        DaemonAction::Restart { .. } => restart().await,
        DaemonAction::Status { verbose, .. } => status(*verbose),
    }
}

fn plist_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("Library").join("LaunchAgents").join(format!("{LAUNCHD_LABEL}.plist"))
}

fn current_uid() -> u32 {
    // getuid 不会失败（无 errno），unsafe 只用于跨 FFI 边界
    unsafe { libc::getuid() }
}

fn launchd_domain() -> String {
    format!("gui/{}", current_uid())
}

fn generate_plist() -> String {
    let binary = ntd_binary_path();
    let ntd_dir = ntd_dir();
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

    // 把当前 PATH 中剩余的项补进来，但保持去重，避免 plist 体积膨胀
    if let Ok(current_path) = std::env::var("PATH") {
        for p in current_path.split(':') {
            if !path_entries.contains(&p.to_string()) {
                path_entries.push(p.to_string());
            }
        }
    }

    // 兜底：标准系统 bin 目录，确保即使 PATH 是空的也能跑基础命令
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

fn install(force: bool) {
    let plist_path = plist_path();
    let binary = ntd_binary_path();

    if !binary.exists() {
        eprintln!("ntd binary not found at {}. Run `make install` first.", binary.display());
        std::process::exit(1);
    }

    // --force 不传时遇到已存在 plist 直接提示退出，避免覆盖用户手改的配置
    if plist_path.exists() && !force {
        println!("Service already installed at: {}", plist_path.display());
        println!("Use --force to reinstall");
        return;
    }

    let ntd_dir = ntd_dir();
    fs::create_dir_all(&ntd_dir).ok();
    plist_path.parent().map(|p| fs::create_dir_all(p).ok());

    println!("Installing launchd service to: {}", plist_path.display());
    fs::write(&plist_path, generate_plist()).expect("Failed to write plist");

    let domain = launchd_domain();
    // bootstrap 会把 plist 加载进 launchd，已加载时返回非 0 + "already loaded"，视为幂等成功
    let output = Command::new("launchctl")
        .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
        .output()
        .expect("Failed to run launchctl");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        // code 5 (service already boot strapped) + already loaded 文案都按成功处理
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

fn uninstall() {
    let plist_path = plist_path();
    let domain = launchd_domain();
    let label = LAUNCHD_LABEL;

    // bootout 失败也无所谓 —— 没加载过 / 已经 unload 都是正常情况
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{label}")])
        .output();

    if plist_path.exists() {
        fs::remove_file(&plist_path).ok();
        println!("Removed {}", plist_path.display());
    }

    println!("Service uninstalled");
}

fn start() {
    let plist_path = plist_path();
    let domain = launchd_domain();
    let label = LAUNCHD_LABEL;

    // 兼容"plist 被删但 service 还在跑"的边角情况：自动重新生成 plist 并 bootstrap
    if !plist_path.exists() {
        println!("Service not installed. Regenerating...");
        plist_path.parent().map(|p| fs::create_dir_all(p).ok());
        fs::write(&plist_path, generate_plist()).expect("Failed to write plist");
        let _ = Command::new("launchctl")
            .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
            .output();
    }

    // kickstart 是 launchd 推荐的"原子启动"命令，比手动 bootstrap 更稳
    let output = Command::new("launchctl")
        .args(["kickstart", &format!("{domain}/{label}")])
        .output()
        .expect("Failed to run launchctl");

    if output.status.success() {
        println!("Service started");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        // 113 = service unavailable，常见于 service 处于半加载态，重 bootstrap 一次即可
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

fn stop() {
    let domain = launchd_domain();
    let label = LAUNCHD_LABEL;

    let output = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{label}")])
        .output();

    // 3/113 + "No such process" 都表示 service 本来就没跑，按"已停止"处理
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

// launchd_restart 是 CLI 子命令入口之一，但运行在 #[tokio::main] 上下文，
// 所以可以声明为 async 并 await tokio 的 sleep。
//
// 原实现用 std::thread::sleep(500ms)：在异步 runtime 上线程 sleep 会
// 阻塞当前 OS 线程，如果 runtime worker 池被填满，其它请求会被卡住。
// 改用 tokio::time::sleep().await 让出 worker，既不阻塞 runtime，
// 也保留了"等 stop 真正生效再 start"的语义。
//
// 500ms 是经验值：launchd bootout 通常在数十 ms 内完成，但慢盘/僵尸
// 进程可能需要更久。这里不引入 polling（需要重新解析 launchctl list
// 输出判断 PID），保持与原行为等价 —— 只是把阻塞 sleep 换成协作式 sleep。
async fn restart() {
    stop();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    start();
}

fn status(verbose: bool) {
    let plist_path = plist_path();
    let label = LAUNCHD_LABEL;

    if !plist_path.exists() {
        println!("Service is not installed");
        println!("  Run: ntd daemon install");
        return;
    }

    // launchctl list <label> 输出格式： "<pid> <last-exit-status> <label>"
    let output = Command::new("launchctl")
        .args(["list", label])
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if stdout.contains(label) {
                println!("Service is loaded");

                // 解析 PID 字段：>0 表示正在跑，-1 表示上次异常退出
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
            // launchctl 都不存在（极少见，理论上 macOS 必有），按"未加载"处理
            println!("Service is installed but not loaded");
            println!("  Run: ntd daemon start");
        }
    }

    // verbose 模式：把 plist 路径和最近 20 行日志打出来，方便排查
    if verbose {
        println!();
        println!("Plist: {}", plist_path.display());
        println!();

        let log_path = ntd_dir().join("run.log");
        if log_path.exists() {
            println!("Recent logs:");
            if let Ok(content) = fs::read_to_string(&log_path) {
                // 文件按追加顺序写入，最近的行在末尾，take(20) 取最后 20 行
                for line in content.lines().rev().take(20) {
                    println!("  {}", line);
                }
            }
        }
    }
}
