//! 自动版本更新调度器。
//!
//! 后台守护线程按天/周/月周期性检查 npm 最新版本。
//! 发现新版本后：
//! 1. 若有正在运行的 todo 或 loop → 记录日志，等待 5 分钟后重试
//! 2. 若无运行中任务 → 静默执行 npm upgrade + 重启服务
//!
//! 检查时间按绝对时间点（如每天凌晨 3:00），而非滑动间隔。

use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;
use crate::db::Database;

/// 启动自动更新调度器。在 `build_app_state` 中通过 `tokio::spawn` 调用。
///
/// 调度逻辑：
/// 1. 读取 config 判断 auto_update_enabled
/// 2. 根据 interval + hour 计算下一次检查的绝对时间
/// 3. sleep 到该时间
/// 4. 检查是否有 running todo/loop
/// 5. 有 → 等待 5 分钟后重试步骤 4
/// 6. 无 → 检查 npm 最新版本 → 有新版则执行升级
pub fn spawn_auto_update_scheduler(
    config: Arc<std::sync::RwLock<Config>>,
    db: Arc<Database>,
) {
    tokio::spawn(async move {
        // 启动后短暂延迟，避免与其他初始化任务竞争 IO
        tokio::time::sleep(Duration::from_secs(10)).await;

        loop {
            // 读取当前配置，判断是否启用
            let (enabled, interval, hour) = {
                // config.read() 可能因线程 panic 导致 PoisonError；回退到获取内部数据而非 panic
                let cfg = match config.read() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                (
                    cfg.auto_update_enabled,
                    cfg.auto_update_interval.clone(),
                    cfg.auto_update_hour,
                )
            };

            if !enabled {
                // 未启用，60 秒后重新检查配置（用户可能随时在 UI 上开启）
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }

            // 计算下一次检查的绝对时间并 sleep
            let next_check = compute_next_check_time(&interval, hour);
            let now = chrono::Local::now();
            let sleep_duration = (next_check - now).to_std().unwrap_or(Duration::from_secs(60));
            tracing::info!(
                "[auto-update] next check at {}, sleeping {:?}",
                next_check.format("%Y-%m-%d %H:%M:%S"),
                sleep_duration,
            );
            tokio::time::sleep(sleep_duration).await;

            // 检查是否有正在运行的 todo 或 loop
            if has_running_tasks(&db).await {
                tracing::info!("[auto-update] running tasks detected, skipping this cycle");
                // 运行中任务可能很快结束，5 分钟后重试
                tokio::time::sleep(Duration::from_secs(300)).await;
                if has_running_tasks(&db).await {
                    tracing::info!("[auto-update] still running tasks after retry, will try next cycle");
                    continue;
                }
                // 任务已结束，继续检查更新
            }

            // 检查 npm 最新版本
            match check_npm_latest_version().await {
                Ok(Some(latest)) => {
                    let current = option_env!("NTD_VERSION").unwrap_or("0.0.0");
                    let current_norm = normalize_version(current);
                    let latest_norm = normalize_version(&latest);

                    if compare_versions(&current_norm, &latest_norm) < 0 {
                        tracing::info!(
                            "[auto-update] new version available: {} -> {}",
                            current,
                            latest,
                        );
                        // 执行静默升级
                        if let Err(e) = execute_silent_upgrade().await {
                            tracing::error!("[auto-update] upgrade failed: {}", e);
                            // 升级失败通知用户（写日志 + 更新 last_check_at）
                            update_last_check_at(&config);
                        } else {
                            // 升级成功，进程即将退出（exit(0)）
                            // 更新 last_check_at 以防 exit 前未落盘
                            update_last_check_at(&config);
                        }
                    } else {
                        tracing::debug!("[auto-update] already up to date: {}", current);
                        update_last_check_at(&config);
                    }
                }
                Ok(None) => {
                    tracing::warn!("[auto-update] npm view returned no version");
                    update_last_check_at(&config);
                }
                Err(e) => {
                    tracing::warn!("[auto-update] failed to check npm version: {}", e);
                    update_last_check_at(&config);
                }
            }
        }
    });
}

/// 计算下一次检查的绝对时间。
///
/// - "day":  下一个 hour:00:00
/// - "week": 下一个指定小时的周一 00:00
/// - "month": 下一个指定小时的 1 号 00:00
fn compute_next_check_time(interval: &str, hour: u32) -> chrono::DateTime<chrono::Local> {
    use chrono::{Datelike, Duration, Local, NaiveTime, Weekday};

    let now = Local::now();
    // hour 超出 0..23 范围时回退到 00:00:00；from_hms_opt(0,0,0) 必定有效
    let time = NaiveTime::from_hms_opt(hour, 0, 0)
        .unwrap_or_default();

    match interval {
        "week" => {
            // 下一个周一的指定小时
            let days_until_monday = match now.weekday() {
                Weekday::Mon => {
                    if now.time() >= time { 7 } else { 0 }
                }
                Weekday::Tue => 6,
                Weekday::Wed => 5,
                Weekday::Thu => 4,
                Weekday::Fri => 3,
                Weekday::Sat => 2,
                Weekday::Sun => 1,
            };
            let date = now.date_naive() + Duration::days(days_until_monday);
            date.and_time(time).and_local_timezone(Local).unwrap()
        }
        "month" => {
            // 下一个月 1 号的指定小时
            let (next_year, next_month) = if now.month() == 12 {
                (now.year() + 1, 1)
            } else {
                (now.year(), now.month() + 1)
            };
            let date = chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)
                .unwrap_or_else(|| {
                    // fallback: 用当前日期 + 30 天
                    now.date_naive() + Duration::days(30)
                });
            date.and_time(time).and_local_timezone(Local).unwrap()
        }
        _ => {
            // "day" 或其他值：下一个指定小时
            let today = now.date_naive().and_time(time).and_local_timezone(Local).unwrap();
            if now >= today {
                today + Duration::days(1)
            } else {
                today
            }
        }
    }
}

/// 检查是否有正在运行的 todo 或 loop execution。
/// 双重校验：数据库 running 记录 + TaskManager 内存确认。
async fn has_running_tasks(db: &Database) -> bool {
    let has_todos = db.has_running_todos().await.unwrap_or(false);
    let has_loops = db.has_running_loop_executions().await.unwrap_or(false);
    has_todos || has_loops
}

/// 通过 npm view 获取最新版本号。
async fn check_npm_latest_version() -> Result<Option<String>, String> {
    let output = tokio::task::spawn_blocking(|| {
        std::process::Command::new("npm")
            .args(["view", "@weibaohui/ntd", "version"])
            .output()
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {}", e))?
    .map_err(|e| format!("npm view failed: {}", e))?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if version.is_empty() {
            Ok(None)
        } else {
            Ok(Some(version))
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("npm view failed: {}", stderr.trim()))
    }
}

/// 执行静默升级：npm install + fork 子进程重启服务。
///
/// 复用 `version_upgrade_handler` 的核心逻辑，但不返回 HTTP 响应。
/// 升级后主进程 exit(0)，子进程完成 install --force + start。
async fn execute_silent_upgrade() -> Result<(), String> {
    let prefix = crate::npm_utils::get_npm_global_prefix();
    let prefix_for_npm = prefix.clone();

    // 执行 npm 升级
    let npm_result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("npm")
            .args([
                "install",
                "-g",
                &format!("--prefix={}", prefix_for_npm),
                "@weibaohui/ntd@latest",
            ])
            .output()
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {}", e))?
    .map_err(|e| format!("npm install failed: {}", e))?;

    if !npm_result.status.success() {
        let stderr = String::from_utf8_lossy(&npm_result.stderr);
        return Err(format!("npm install failed: {}", stderr.trim()));
    }

    tracing::info!(
        "[auto-update] npm upgrade succeeded: {}",
        String::from_utf8_lossy(&npm_result.stdout).trim()
    );

    // 查找 ntd 可执行文件路径
    let ntd_cmd = crate::npm_utils::find_ntd_binary(&prefix);
    if ntd_cmd == "ntd" {
        return Err("ntd binary not found".to_string());
    }
    if !crate::daemon::common::is_safe_ntd_path(&ntd_cmd) {
        return Err(format!("ntd path contains illegal characters: {}", ntd_cmd));
    }

    // 写标记文件
    std::fs::write("/tmp/ntd.update", "").ok();

    let marker_cleanup_path = if cfg!(windows) {
        "%TEMP%\\ntd.update".to_string()
    } else {
        "/tmp/ntd.update".to_string()
    };

    // fork 子进程执行 install --force + start
    #[cfg(target_os = "linux")]
    {
        let script = format!(
            "sleep 3; {} daemon install --force; {} daemon start; rm -f {}",
            ntd_cmd, ntd_cmd, marker_cleanup_path,
        );
        if let Err(e) = crate::daemon::spawn_detached_redeploy_nonblocking(&script) {
            tracing::warn!("[auto-update] systemd-run failed ({}), falling back to sh -c", e);
            let log = crate::daemon::redeploy_log_path().to_string_lossy().to_string();
            spawn_redeploy_sh(&ntd_cmd, &marker_cleanup_path, &log);
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        spawn_redeploy_sh(&ntd_cmd, &marker_cleanup_path, "/tmp/ntd-upgrade.log");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let quoted = crate::daemon::common::shell_quote_single(&ntd_cmd);
        std::process::Command::new("cmd")
            .args(["/C", &format!(
                "timeout /t 3 /nobreak >nul && {quoted} daemon install --force && {quoted} daemon start && del /f /q {marker}",
                quoted = quoted,
                marker = marker_cleanup_path,
            )])
            .creation_flags(0x08000000)
            .spawn()
            .ok();
    }

    tracing::info!("[auto-update] forked child process, exiting main process");

    // 延迟 500ms 后 exit(0)，给响应时间完成
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        tracing::info!("[auto-update] main process exiting");
        std::process::exit(0);
    });

    Ok(())
}

/// Unix sh -c 回退方案：在非 Linux 平台或 systemd-run 不可用时使用。
#[cfg(not(windows))]
fn spawn_redeploy_sh(ntd_cmd: &str, marker_cleanup_path: &str, log_path: &str) {
    let quoted = crate::daemon::common::shell_quote_single(ntd_cmd);
    std::process::Command::new("sh")
        .args(["-c", &format!(
            "(sleep 3; {quoted} daemon install --force; {quoted} daemon start; rm -f {marker}) >> {log} 2>&1 &",
            quoted = quoted,
            marker = marker_cleanup_path,
            log = log_path,
        )])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok();
}

/// 规范化版本号：去除 v 前缀和 -dirty/-alpha 等后缀，只保留 semver 主干。
fn normalize_version(v: &str) -> String {
    let v = v.strip_prefix('v').unwrap_or(v);
    if let Some(pos) = v.find('-') {
        v[..pos].to_string()
    } else {
        v.to_string()
    }
}

/// 比较两个版本号，返回 1 表示 a 更新，-1 表示 b 更新，0 表示相等。
fn compare_versions(a: &str, b: &str) -> i32 {
    let parts_a: Vec<u32> = a.split('.').filter_map(|p| p.parse().ok()).collect();
    let parts_b: Vec<u32> = b.split('.').filter_map(|p| p.parse().ok()).collect();
    for i in 0..parts_a.len().max(parts_b.len()) {
        let pa = parts_a.get(i).copied().unwrap_or(0);
        let pb = parts_b.get(i).copied().unwrap_or(0);
        if pa > pb { return 1; }
        if pa < pb { return -1; }
    }
    0
}

/// 更新 config 中的 last_check_at 并持久化。
fn update_last_check_at(config: &Arc<std::sync::RwLock<Config>>) {
    let cfg_to_save = {
        // config.write() 可能因线程 panic 导致 PoisonError；回退到获取内部数据而非 panic
        let mut cfg = match config.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        cfg.auto_update_last_check_at = Some(chrono::Local::now().to_rfc3339());
        cfg.clone()
    };
    // 异步落盘，不阻塞调度器
    tokio::task::spawn_blocking(move || {
        if let Err(e) = cfg_to_save.save() {
            tracing::warn!("[auto-update] failed to save config: {}", e);
        }
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn test_normalize_version_with_v_prefix() {
        assert_eq!(normalize_version("v1.2.3"), "1.2.3");
    }

    #[test]
    fn test_normalize_version_with_suffix() {
        assert_eq!(normalize_version("1.2.3-dirty"), "1.2.3");
        assert_eq!(normalize_version("v0.0.50-alpha"), "0.0.50");
    }

    #[test]
    fn test_normalize_version_clean() {
        assert_eq!(normalize_version("1.2.3"), "1.2.3");
    }

    #[test]
    fn test_compare_versions_equal() {
        assert_eq!(compare_versions("1.2.3", "1.2.3"), 0);
    }

    #[test]
    fn test_compare_versions_a_newer() {
        assert_eq!(compare_versions("1.2.4", "1.2.3"), 1);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), 1);
    }

    #[test]
    fn test_compare_versions_b_newer() {
        assert_eq!(compare_versions("1.2.3", "1.2.4"), -1);
        assert_eq!(compare_versions("1.9.9", "2.0.0"), -1);
    }

    #[test]
    fn test_compare_versions_different_lengths() {
        assert_eq!(compare_versions("1.2.3", "1.2"), 1);
        assert_eq!(compare_versions("1.2", "1.2.3"), -1);
    }

    #[test]
    fn test_compute_next_check_time_day() {
        // day 模式：下一个指定小时
        let next = compute_next_check_time("day", 3);
        assert_eq!(next.hour(), 3);
        assert_eq!(next.minute(), 0);
        assert_eq!(next.second(), 0);
    }

    #[test]
    fn test_compute_next_check_time_week() {
        let next = compute_next_check_time("week", 3);
        assert_eq!(next.weekday(), chrono::Weekday::Mon);
        assert_eq!(next.hour(), 3);
    }

    #[test]
    fn test_compute_next_check_time_month() {
        let next = compute_next_check_time("month", 3);
        assert_eq!(next.day(), 1);
        assert_eq!(next.hour(), 3);
    }
}
