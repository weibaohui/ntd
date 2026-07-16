//! 启动检查：每次启动异步跑一次的「一次性」后台任务。
//!
//! 按配置 `bundled_source.sync_on_startup` 同步内置资源（专家 + 事项模板）。
//! 全程非阻塞：失败只记日志，绝不拖垮或中断启动。
//! 频繁重启时用 30 分钟冷却（复用 last_sync_at）避免反复打远端。
//!
//! 版本检查不在此处做——用户可在「设置 → 关于」页手动点「检查更新」
//! （实时调 /api/version/latest），或开启「自动更新」让定时调度器处理升级。

use std::time::Duration;

use crate::git_sync::SyncStrategy;
use crate::handlers::bundled::{run_bundled_sync, Subdir};
use crate::handlers::AppState;

/// 冷却窗口（秒）：距上次同步不足此时长则跳过。
/// 取 30 分钟——既能覆盖「每次启动」的常态诉求，又能挡住快速重启循环对远端的连打。
const COOLDOWN_SECS: i64 = 30 * 60;

/// 启动检查入口：在 `build_app_state` 末尾调用。
///
/// `tokio::spawn` 一个一次性任务（无 loop），延迟一小段避让启动 IO 后执行资源同步。
/// `state` 内部都是 Arc，clone 成本很低。
pub fn spawn_startup_check(state: AppState) {
    tokio::spawn(async move {
        // 与现有 scheduler 一致：启动后短暂延迟，避免与其他初始化任务竞争 IO。
        tokio::time::sleep(Duration::from_secs(10)).await;
        sync_resources(&state).await;
    });
}

/// 按配置同步内置资源（专家 + 事项模板）。未开启或冷却期内跳过；失败只记日志。
async fn sync_resources(state: &AppState) {
    let (enabled, last_sync) = state
        .config_snapshot(|c| (c.bundled_source.sync_on_startup, c.bundled_source.last_sync_at.clone()));
    if !enabled {
        tracing::debug!("[startup-check] sync_on_startup 关闭，跳过资源同步");
        return;
    }
    if !cooldown_elapsed(&last_sync) {
        tracing::debug!("[startup-check] 资源同步在冷却期内，跳过");
        return;
    }

    tracing::info!("[startup-check] 开始同步内置资源（专家 + 事项模板）");
    // Overwrite：bundled 是系统资源，远程为准；用户自定义在独立目录，不受影响。
    match run_bundled_sync(state, Subdir::All, SyncStrategy::Overwrite).await {
        Ok(r) => tracing::info!(
            "[startup-check] 资源同步完成：{}（更新 {} 个文件）",
            r.message,
            r.changed_files
        ),
        Err(e) => tracing::warn!("[startup-check] 资源同步失败：{}", e),
    }
}

/// 距上次记录时间是否已超过冷却窗口。
///
/// - `None`（从未记录）→ 视为需要执行；
/// - 解析失败（脏数据）→ 同样视为需要执行，宁可多同步一次也不卡死。
fn cooldown_elapsed(last_at: &Option<String>) -> bool {
    let Some(ts) = last_at else { return true };
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return true;
    };
    // last_sync_at 用 Utc::now() 写入，带时区是合法 RFC3339，转 UTC 后比较一致。
    let elapsed = chrono::Utc::now().signed_duration_since(parsed.with_timezone(&chrono::Utc));
    elapsed.num_seconds() >= COOLDOWN_SECS
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// 用 RFC3339 字符串构造「距今 N 秒前」的时间戳，方便断言冷却窗口。
    fn ago_rfc3339(secs_ago: i64) -> String {
        (chrono::Utc::now() - chrono::Duration::seconds(secs_ago)).to_rfc3339()
    }

    #[test]
    fn test_cooldown_elapsed_none_means_should_run() {
        // 从未记录过 → 必须执行，不能因为冷却把首次同步挡掉。
        assert!(cooldown_elapsed(&None));
    }

    #[test]
    fn test_cooldown_elapsed_unparseable_means_should_run() {
        // 脏数据解析失败 → 宁可多同步一次也不卡死，视为需要执行。
        assert!(cooldown_elapsed(&Some("not-a-date".to_string())));
    }

    #[test]
    fn test_cooldown_elapsed_within_window_skips() {
        // 1 分钟前刚同步过，远小于 30 分钟冷却 → 跳过。
        let recent = ago_rfc3339(60);
        assert!(!cooldown_elapsed(&Some(recent)));
    }

    #[test]
    fn test_cooldown_elapsed_beyond_window_runs() {
        // 60 分钟前同步过，超过 30 分钟冷却 → 执行。
        let stale = ago_rfc3339(60 * 60);
        assert!(cooldown_elapsed(&Some(stale)));
    }
}
