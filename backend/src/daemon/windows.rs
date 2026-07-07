//! Windows 平台的 daemon 实现：基于 Task Scheduler。
//!
//! 设计要点：
//! - 用 schtasks /create onlogon + /it 让 task 仅在用户登录态跑，
//!   避免 service 在锁屏 / 注销状态下还要拉起的麻烦。
//! - `daemon restart` 改用 `tokio::time::sleep(1s)`，比原 `std::thread::sleep(2s)`
//!   缩短一半的同时让出 runtime worker；1s 是 schtasks /end 长尾
//!   （慢盘 / AV 扫描）下的保守阈值，比 launchd 的 500ms 更紧一档。
//! - 额外提供一个 `ntd_watchdog.bat` 包装脚本，让用户按需配置成
//!   "crash 后 5 秒重启" 的 task action —— schtasks 原生不支持
//!   restart-on-failure，只能靠包装脚本兜底。

use std::fs;
use std::process::Command;

use super::common::{ntd_binary_path, ntd_dir};
use super::DaemonAction;

const TASK_NAME: &str = "ntd";

// handle_task_scheduler 声明为 async 是为了内部 Restart 分支可以 .await
// task_scheduler_restart()；其他分支保持同步，函数签名统一而已。
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

fn install(force: bool) {
    let binary = ntd_binary_path();

    if !binary.exists() {
        eprintln!("ntd binary not found at {}. Run `make install` first.", binary.display());
        std::process::exit(1);
    }

    // Check if task already exists
    let query = Command::new("schtasks")
        .args(["/query", "/tn", TASK_NAME])
        .output();

    // query 成功 + 非 force 时按"已存在"退出，避免覆盖用户的自定义配置
    if let Ok(result) = query {
        if result.status.success() && !force {
            println!("Task already exists: {}", TASK_NAME);
            println!("Use --force to reinstall");
            return;
        }
    }

    // force 模式：先删除旧 task 再重建，确保新 unit 生效
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

        // 创建一个 watchdog 包装脚本，让用户可以按需配置成"crash 后自动重启"的 task action
        let ntd_dir = ntd_dir();
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

fn uninstall() {
    let output = Command::new("schtasks")
        .args(["/delete", "/tn", TASK_NAME, "/f"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            println!("Task deleted");
            // 顺手清理 watchdog 脚本，避免留下孤儿 .bat 文件
            let watchdog = ntd_dir().join("ntd_watchdog.bat");
            if watchdog.exists() {
                fs::remove_file(&watchdog).ok();
            }
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // 任务本身就不存在时按"已卸载"处理，不要报错
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

fn start() {
    let output = Command::new("schtasks")
        .args(["/run", "/tn", TASK_NAME])
        .output();

    match output {
        Ok(o) if o.status.success() => println!("Service started"),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // schtasks 对"已在跑"会返回带 "already running" 的非 0，按用户语义视为成功
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

fn stop() {
    let output = Command::new("schtasks")
        .args(["/end", "/tn", TASK_NAME])
        .output();

    match output {
        Ok(o) if o.status.success() => println!("Service stopped"),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // "not running" / "does not exist" 都是正常状态，按"未运行"提示
            if stderr.contains("not running") || stderr.contains("does not exist") {
                println!("Service is not running");
            } else {
                eprintln!("Failed to stop task: {}", stderr.trim());
            }
        }
        Err(e) => eprintln!("Failed to run schtasks: {}", e),
    }
}

// Windows Task Scheduler restart：stop → 等 → start。
//
// 原实现 std::thread::sleep(2s) 在 tokio runtime 上同样会阻塞当前
// OS 线程；改用 tokio::time::sleep().await 让出 worker。
//
// 时长：第一次提交缩到 500ms 对齐 launchd 路径，但收到 review 反馈
// 在慢盘/慢 VM/AV 扫描下可能 race（schtasks /end 长尾可达数百 ms，
// start 早于 stop 完全生效会撞上 "Service is already running" 分支）。
// 这里改用 1s 作为折中：既比原 2s 显著缩短（用户感受的 CLI 等待），
// 又保留足够冗余覆盖慢主机的长尾场景。比 launchd 的 500ms 更保守，
// 因为 schtasks /end → start 的串行依赖比 launchd bootout 更紧。
async fn restart() {
    stop();
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    start();
}

fn status(verbose: bool) {
    let output = Command::new("schtasks")
        .args(["/query", "/tn", TASK_NAME, "/fo", "list"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            // 直接转发 schtasks 的 list 输出，让用户看到 Next Run Time /
            // Last Run Time / Status 等完整字段
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
        println!("Binary: {}", ntd_binary_path().display());

        let log_path = ntd_dir().join("run.log");
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
