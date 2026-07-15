//! 内置资源自动同步调度器。
//!
//! 后台守护线程按 cron 表达式周期性同步远程 Git 仓库。

use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;
use crate::git_sync;

/// 启动内置资源自动同步调度器。
///
/// 调度逻辑：
/// 1. 读取 config 判断 auto_sync_enabled
/// 2. 解析 cron 表达式计算下一次同步时间
/// 3. sleep 到该时间
/// 4. 执行同步操作
/// 5. 记录结果日志
/// 6. 回到步骤 1
pub fn spawn_bundled_sync_scheduler(config: Arc<std::sync::RwLock<Config>>) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;

        loop {
            let (enabled, url, branch, local_path, cron) = {
                let cfg = match config.read() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let bundled = &cfg.bundled_source;
                (
                    bundled.auto_sync_enabled,
                    bundled.url.clone(),
                    bundled.branch.clone(),
                    bundled.local_path.clone(),
                    bundled.auto_sync_cron.clone(),
                )
            };

            if !enabled {
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }

            let next_sync = compute_next_cron_time(&cron);
            let now = chrono::Utc::now();
            let sleep_duration = (next_sync - now).to_std().unwrap_or(Duration::from_secs(60));
            tracing::info!(
                "[bundled-sync] next sync at {}, sleeping {:?}",
                next_sync.format("%Y-%m-%d %H:%M:%S"),
                sleep_duration,
            );
            tokio::time::sleep(sleep_duration).await;

            perform_sync(&url, &branch, &local_path).await;
        }
    });
}

/// 执行同步操作
async fn perform_sync(url: &str, branch: &str, local_path: &str) {
    let repo_path = match git_sync::bundled_dir(local_path) {
        Some(p) => p,
        None => {
            tracing::error!("[bundled-sync] 无法获取 home 目录");
            return;
        }
    };

    // 验证 repo_path 是否为合法的 Git 工作区（存在 .git 目录）。
    // 仅有空目录或损坏的 checkout 时执行强制克隆覆盖。
    let is_valid_repo = repo_path.join(".git").is_dir();

    let result = if !repo_path.exists() || !is_valid_repo {
        if !is_valid_repo && repo_path.exists() {
            tracing::warn!("[bundled-sync] 本地目录存在但非有效 Git 仓库，执行强制克隆覆盖");
        } else {
            tracing::info!("[bundled-sync] 本地目录不存在，执行首次克隆");
        }
        git_sync::clone_repo(url, &repo_path, branch).await
    } else {
        tracing::info!("[bundled-sync] 执行同步更新");
        git_sync::sync_repo(&repo_path, "origin", branch, git_sync::SyncStrategy::Overwrite).await
    };

    match result {
        Ok(r) => {
            if r.has_updates {
                tracing::info!(
                    "[bundled-sync] 同步成功: {}，更新了 {} 个文件",
                    r.message,
                    r.changed_files
                );
            } else {
                tracing::info!("[bundled-sync] {}", r.message);
            }
        }
        Err(e) => {
            tracing::error!("[bundled-sync] 同步失败: {}", e);
        }
    }
}

/// 计算下一次 cron 执行时间
fn compute_next_cron_time(cron: &str) -> chrono::DateTime<chrono::Utc> {
    let now = chrono::Utc::now();

    let schedule = if let Ok(s) = croner::Cron::new(cron).with_seconds_required().parse() {
        s
    } else if let Ok(s) = croner::Cron::new("0 0 4 * * *").with_seconds_required().parse() {
        s
    } else {
        tracing::warn!("无法解析 cron 表达式，使用当前时间 +24h");
        return now + chrono::Duration::hours(24);
    };

    schedule.find_next_occurrence(&now, false).unwrap_or_else(|_| {
        now + chrono::Duration::hours(24)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_next_cron_time() {
        let cron = "0 0 4 * * *";
        let next = compute_next_cron_time(cron);
        assert!(next > chrono::Utc::now());
    }
}
