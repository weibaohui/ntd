//! Loop Scheduler — 独立于 TodoScheduler, 专门处理 loop 的 cron 触发器。
//!
//! 设计取舍（issue / 决策点）：
//! - 用「轮询」而非 tokio-cron-scheduler：环路调度量级预期很小（数十个），
//!   30s 轮询足够覆盖；代码量比维护一套 JobScheduler 小一个量级。
//! - 时区处理：复用 `cron` crate + `chrono_tz` 在前端传入的时区上计算下次触发。
//! - 启动时全量加载 + 提供 `upsert_cron_trigger/remove_cron_trigger` 给 handler
//!   在增删改时调用,保持内存视图与 DB 同步。
//!
//! 用法：
//! - main.rs: `LoopScheduler::start(db, runner).await?`
//! - handlers/loop.rs: 增删改 cron trigger 时调 `upsert_cron_trigger/remove_cron_trigger`

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::services::loop_runner::LoopRunner;
use crate::services::loop_trigger::LoopTriggerDispatcher;

/// 内存中的 cron 调度项。
#[derive(Debug, Clone)]
struct CronEntry {
    _trigger_id: i64,
    loop_id: i64,
    schedule: cron::Schedule,
    /// 已经在内存中记下的「下次触发时间」,轮询时只 fire 那些已到期的。
    next_run_at: chrono::DateTime<chrono::Utc>,
}

pub struct LoopScheduler {
    db: Arc<Database>,
    runner: Arc<LoopRunner>,
    dispatcher: Arc<LoopTriggerDispatcher>,
    /// trigger_id → CronEntry
    entries: Arc<Mutex<HashMap<i64, CronEntry>>>,
    /// 关闭信号(暂留接口,目前进程退出靠 tokio runtime drop)
    _shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl LoopScheduler {
    pub async fn start(
        db: Arc<Database>,
        runner: Arc<LoopRunner>,
    ) -> Result<Arc<Self>, String> {
        // dispatcher 只需要 db（查 loop/trigger 元数据），实际执行交给 runner 自带的 ctx，
        // 因此只传 db，不再伪造含空 expert_manager 的 ServiceContext。
        let dispatcher = Arc::new(LoopTriggerDispatcher::new(
            runner.clone(),
            db.clone(),
        ));
        let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(false);

        let me = Arc::new(Self {
            db,
            runner,
            dispatcher,
            entries: Arc::new(Mutex::new(HashMap::new())),
            _shutdown_tx: shutdown_tx,
        });

        // 启动时全量加载
        me.reload_all().await?;

        // 后台轮询任务：每 30s 检查一次
        let me_bg = me.clone();
        tokio::spawn(async move {
            me_bg.run_loop().await;
        });

        Ok(me)
    }

    pub fn dispatcher(&self) -> Arc<LoopTriggerDispatcher> {
        self.dispatcher.clone()
    }

    /// 重新加载所有启用的 cron trigger（启动时/loop 状态切换时调用）。
    pub async fn reload_all(&self) -> Result<(), String> {
        let triggers = self
            .db
            .list_enabled_triggers_by_type("cron")
            .await
            .map_err(|e| e.to_string())?;
        let mut entries = self.entries.lock().await;
        entries.clear();
        for t in triggers {
            if let Err(e) = Self::add_entry(&mut entries, &t) {
                warn!(
                    "loop_scheduler: skip invalid cron trigger #{}: {}",
                    t.id, e
                );
            }
        }
        info!("loop_scheduler: loaded {} cron triggers", entries.len());
        Ok(())
    }

    /// 在内存中注册/更新单条 cron trigger。
    /// handler 在 create/update trigger 时调用,避免重启进程才生效。
    pub async fn upsert_cron_trigger(
        &self,
        trigger_id: i64,
    ) -> Result<(), String> {
        let t = self
            .db
            .get_trigger(trigger_id)
            .await
            .map_err(|e| e.to_string())?;
        let Some(t) = t else { return Ok(()); };
        if t.trigger_type != "cron" || t.enabled == 0 {
            // 不属于 cron 或被禁用,直接移除
            let mut entries = self.entries.lock().await;
            entries.remove(&trigger_id);
            return Ok(());
        }
        let mut entries = self.entries.lock().await;
        Self::add_entry(&mut entries, &t)
    }

    pub async fn remove_cron_trigger(&self, trigger_id: i64) {
        let mut entries = self.entries.lock().await;
        entries.remove(&trigger_id);
    }

    /// 把 trigger 解析成 CronEntry 放进 map。失败时 warn 并跳过(不抛错)。
    fn add_entry(
        entries: &mut HashMap<i64, CronEntry>,
        t: &crate::db::entity::loop_triggers::Model,
    ) -> Result<(), String> {
        let cfg: serde_json::Value = serde_json::from_str(&t.config)
            .map_err(|e| format!("config parse: {}", e))?;
        let cron_expr = cfg
            .get("cron")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing cron field".to_string())?;
        let tz_str = cfg.get("timezone").and_then(|v| v.as_str());
        let schedule = cron::Schedule::from_str(cron_expr)
            .map_err(|e| format!("invalid cron '{}': {}", cron_expr, e))?;
        // 计算下次触发时间
        let next_run_at = if let Some(tz_str) = tz_str {
            match tz_str.parse::<chrono_tz::Tz>() {
                Ok(tz) => schedule
                    .upcoming(tz)
                    .next()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .ok_or_else(|| "no upcoming occurrence".to_string())?,
                Err(_) => schedule
                    .upcoming(chrono::Utc)
                    .next()
                    .ok_or_else(|| "no upcoming occurrence".to_string())?,
            }
        } else {
            schedule
                .upcoming(chrono::Utc)
                .next()
                .ok_or_else(|| "no upcoming occurrence".to_string())?
        };
        entries.insert(
            t.id,
            CronEntry {
                _trigger_id: t.id,
                loop_id: t.loop_id,
                schedule,
                next_run_at,
            },
        );
        Ok(())
    }

    /// 后台轮询主循环。
    async fn run_loop(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(e) = self.tick_once().await {
                error!("loop_scheduler: tick error: {}", e);
            }
        }
    }

    /// 一次轮询：扫所有 entry,触发已到期的,然后把 next_run_at 推进到下一次。
    async fn tick_once(&self) -> Result<(), String> {
        let now = chrono::Utc::now();
        let due: Vec<(i64, i64)> = {
            let mut entries = self.entries.lock().await;
            let mut due = Vec::new();
            for (tid, e) in entries.iter_mut() {
                if e.next_run_at <= now {
                    due.push((*tid, e.loop_id));
                    // 推进到下一次;若 schedule 已无未来 occurrence,保持现状
                    if let Some(next) = e.schedule.upcoming(chrono::Utc).next() {
                        e.next_run_at = next.with_timezone(&chrono::Utc);
                    }
                }
            }
            due
        };
        for (trigger_id, loop_id) in due {
            debug!(
                "loop_scheduler: firing cron trigger #{} (loop #{})",
                trigger_id, loop_id
            );
            let meta = serde_json::json!({
                "trigger_id": trigger_id,
                "scheduled_at": now.to_rfc3339(),
            });
            let _ = self
                .runner
                .clone()
                .spawn_run(loop_id, Some(trigger_id), "cron", meta, None, None, None);
        }
        Ok(())
    }
}
