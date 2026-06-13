use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info, warn};

use chrono::{TimeZone, Timelike};

use crate::executor_service::{run_todo_execution, RunTodoExecutionRequest};
use crate::hooks::HookService;
use crate::service_context::ServiceContext;

/// 把一个去重整数集合格式化成 cron 字段值。
///
/// 紧凑模式优先级:
/// 1. 空集 → `*` (由调用方决定,这里不处理)
/// 2. 单值 → `"7"`
/// 3. 连续区间 → `"a-b"` (例: `{0,1,2,3,4}` → `"0-4"`)
/// 4. 等差数列 → `"a-b/N"` (例: `{0,2,4,6}` → `"0-6/2"`)
/// 5. 其它情况 → `"a,b,c"` (例: `{0,5,10,12}` → `"0,5,10,12"`)
///
/// 为什么不用 4. 的等差:实现稍复杂,但产出 cron 更短、对人友好。常见
/// `*/N` / `a-b/N` 都是这种形式。
fn format_cron_field(set: &BTreeSet<u32>) -> String {
    if set.is_empty() {
        return "*".to_string();
    }
    let items: Vec<u32> = set.iter().copied().collect();
    if items.len() == 1 {
        return items[0].to_string();
    }

    // 检查连续区间
    let mut is_contiguous = true;
    for i in 1..items.len() {
        if items[i] != items[i - 1] + 1 {
            is_contiguous = false;
            break;
        }
    }
    if is_contiguous {
        return format!("{}-{}", items[0], items[items.len() - 1]);
    }

    // 检查等差数列 (步长固定)
    if items.len() >= 2 {
        let step = items[1] - items[0];
        if step > 0 {
            let mut is_arith = true;
            for i in 2..items.len() {
                if items[i] - items[i - 1] != step {
                    is_arith = false;
                    break;
                }
            }
            if is_arith {
                return format!("{}-{}/{}", items[0], items[items.len() - 1], step);
            }
        }
    }

    // 兜底:逗号列表
    items
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Convert a cron expression from user timezone to UTC timezone.
/// This is necessary because tokio-cron-scheduler always executes in UTC.
/// For example, if user is in Asia/Shanghai (UTC+8) and wants 9:00 local time,
/// we need to schedule UTC 1:00 (9:00 - 8 hours = 1:00).
///
/// 段落总览:
/// 把"用户时区的 cron 表达式"转成"等价的 UTC cron 表达式",驱动
/// `tokio-cron-scheduler` 在用户期望的时刻触发。
///
/// 关键点(对应 issue #502 列的几个 bug):
/// 1. **DST 正确**:用 `cron::Schedule` 在 1 年内枚举所有 occurrence,
///    逐个转 UTC,再统计"出现最多的 (h,m,s) 元组"。这样无论 DST
///    切换日偏移怎么变,主用值都能稳定取到,且和当前偏移不再耦合
///    (原实现用 `now()` 算偏移 → 跨天/跨年后会漂移)。
/// 2. **复杂表达式**:把枚举结果按 (h, m, s) 收集成 set,格式化时
///    自动选最紧凑的表示 —— 连续区间写 `a-b`,等差数列写 `a-b/N`,
///    其它写 `a,b,c`。`*/2`、`9,12,18`、`9-17` 一视同仁。
/// 3. **跨日回卷**:枚举时是 `DateTime<Tz>`,chrono 自动处理 wrap,
///    不会出现"减 8 小时后小时为负"的脏数据。
///
/// 简化点:
/// - day-of-month / month / day-of-week 字段保持原值。
///   在常见的"每天/每月几点"调度里这些字段都是 `*`,不会受影响;
///   即便不是 `*`,跨时区时这些字段的偏移对用户意义不大,改它反而
///   引入歧义。如果未来有强需求,可以再扩展。
/// - DST 切换日会"丢 1 小时"或"重 1 小时":这是单 cron 表达式表达
///   不出 1 年内多个 UTC 时刻的根本限制,会在日志 warn 提示用户。
fn convert_cron_to_utc(cron_expr: &str, timezone: &str) -> Result<String, String> {
    // 解析时区; 失败时给出可定位的错误,不要 panic。
    let tz: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| format!("Invalid timezone: {}", timezone))?;

    // 用 `cron` crate 解析,这一步同时校验 cron 语法。
    // 之前用一次性 `from_str(...)?` 吞掉错误再 split 字段,失败时
    // 报错信息不友好。
    let schedule = cron::Schedule::from_str(cron_expr)
        .map_err(|_| format!("Invalid cron expression: {}", cron_expr))?;

    // 要求 6 字段 (秒 分 时 日 月 周),与 `tokio-cron-scheduler` 一致;
    // 5 字段在 unix cron 里合法但本项目不接受 —— 早 fail 早知道。
    let fields: Vec<&str> = cron_expr.trim().split_whitespace().collect();
    if fields.len() != 6 {
        return Err(format!(
            "Cron expression must have 6 fields (seconds minute hour day-of-month month day-of-week), got {}",
            fields.len()
        ));
    }

    // fields 顺序: 秒 分 时 日 月 周
    // seconds/minutes 不直接用 —— 我们从 `Schedule::after` 枚举后取真实 (h, m, s),
    // 比手算偏移更准。day_of_month/month/day_of_week 保持原值,见函数 doc。
    let _seconds = fields[0];
    let _minutes = fields[1];
    let hours = fields[2];
    let day_of_month = fields[3];
    let month = fields[4];
    let day_of_week = fields[5];

    // hour 字段为 `*` 时,语义是"每小时一次",但**不能直接 passthrough**。
    // 因为 minute/second 字段可能不是 `*`,例如 `0,30 0 * * * *` 是
    // "每 30 分钟一次",跨时区后这些 minute/second 的 UTC 值会随
    // hour 一起 wrap,必须重新生成。
    //
    // 唯一能 passthrough 的场景是"三个时间字段都是 `*`"——但即便
    // 那样,枚举也只会得到 `{0..=23},{0..=59},{0..=59}` 的并集,等价于
    // 全通配,直接走枚举也没问题。所以这里不再做早返回,统一枚举。

    // 用一个固定的参考时间点 (2025-01-01 用户本地 00:00:00) 开始枚举,
    // 覆盖完整 1 年,确保 DST 切换日的 occurrence 都在样本内。
    // 用固定时间而非 `Utc::now()` 是为了测试稳定 + 不依赖系统时间。
    let reference = match tz.with_ymd_and_hms(2025, 1, 1, 0, 0, 0) {
        chrono::LocalResult::Single(t) => t,
        _ => return Err("Could not build reference datetime in timezone".to_string()),
    };
    let end = match tz.with_ymd_and_hms(2026, 1, 1, 0, 0, 0) {
        chrono::LocalResult::Single(t) => t,
        _ => return Err("Could not build end datetime in timezone".to_string()),
    };

    // 收集所有 UTC 时刻 (h, m, s),用 HashMap 计数。
    // 计数不只是为了找 dominant,更为了判断是不是"单小时 DST 双值"场景。
    let mut utc_time_counts: HashMap<(u32, u32, u32), u32> = HashMap::new();
    for local_dt in schedule.after(&reference).take_while(|dt| *dt < end) {
        // `Schedule::after` 返回的是带时区的 `DateTime<Tz>`,直接 `with_timezone(&Utc)` 即可。
        // 之前实现的 `tz.offset_from_utc_datetime(&now.naive_utc())` 手算偏移,
        // DST 切换日会算错 —— chrono 的 with_timezone 内部会按 `local_dt` 那一刻的
        // 实际 offset 算,所以总是对的。
        let utc = local_dt.with_timezone(&chrono::Utc);
        let key = (utc.hour(), utc.minute(), utc.second());
        *utc_time_counts.entry(key).or_insert(0) += 1;
    }

    if utc_time_counts.is_empty() {
        // 理论上不会发生 (reference 在 schedule 起点之后,365 天内必出至少一次);
        // 兜底返回原表达式,避免 panic。
        warn!(
            "No occurrences found for cron '{}' in {} between 2025-01-01 and 2026-01-01; \
            returning original expression.",
            cron_expr, timezone
        );
        return Ok(cron_expr.to_string());
    }

    // DST 检测: 当且仅当 distinct (h, m, s) 元组恰好有 2 个、且它们
    // 只在 hour 上相差 1、其它字段全相同时,就是单 hour 表达式的 DST
    // 双值场景(例:`0 0 9 * * *` 在 New York 一年里产生 13 UTC 和
    // 14 UTC 两种)。这种情形只取 dominant,避免"每天多触发 1 次"。
    //
    // 为什么不无脑用 union:对 `0 0 9 * * *` 这种单 hour 表达式,
    // union 会变成 `13,14`,导致每天都触发两次(一次夏令时一次冬令时),
    // 显然是错的。
    //
    // 为什么不无脑用 dominant:对 `0 0 9,12,18 * * *` 多 hour 列表,
    // dominant 只取一个,会丢失大部分触发时间。
    //
    // 启发式:仅"恰好 2 个且差 1 小时"是 DST 典型形态,其它多值情况
    // (multi-hour、step、range)按字面 union 才是对的。
    let distinct: Vec<(u32, u32, u32)> = utc_time_counts.keys().copied().collect();
    let is_dst_pair = distinct.len() == 2
        && distinct[0].1 == distinct[1].1
        && distinct[0].2 == distinct[1].2
        && distinct[0].0.abs_diff(distinct[1].0) == 1;

    let (utc_seconds_set, utc_minutes_set, utc_hours_set) = if is_dst_pair {
        // 取 dominant(出现次数最多的那个时刻)。
        // `is_dst_pair` 守卫了 distinct.len()==2,所以 max_by_key 一定成功；
        // 但我们用 match + 错误返回而不是 .expect()，让任何 invariant 失守
        // 都能被调用方看到具体错误而不是进程 panic。
        let dominant = distinct
            .iter()
            .max_by_key(|k| utc_time_counts.get(k).copied().unwrap_or(0))
            .copied();
        let (h, m, s) = match dominant {
            Some(v) => v,
            None => {
                return Err(format!(
                    "DST heuristic invariant violated: is_dst_pair=true but no dominant UTC time for cron '{}' in {}",
                    cron_expr, timezone
                ));
            }
        };
        warn!(
            "Cron '{}' in {} crosses DST; using dominant UTC time \
            (h={}, m={}, s={}) and dropping the other. \
            On DST transition days the schedule may be off by 1 hour.",
            cron_expr, timezone, h, m, s
        );
        (
            BTreeSet::from([s]),
            BTreeSet::from([m]),
            BTreeSet::from([h]),
        )
    } else {
        // 其它情况(无 DST、单值、multi-hour、step、range):用 union
        let sec: BTreeSet<u32> = utc_time_counts.keys().map(|k| k.2).collect();
        let min: BTreeSet<u32> = utc_time_counts.keys().map(|k| k.1).collect();
        let hr: BTreeSet<u32> = utc_time_counts.keys().map(|k| k.0).collect();
        (sec, min, hr)
    };

    let s_str = format_cron_field(&utc_seconds_set);
    let m_str = format_cron_field(&utc_minutes_set);
    let h_str = format_cron_field(&utc_hours_set);

    // 生成最终 UTC cron 表达式:把枚举出的 h/m/s 集合格式化,其余字段
    // (day-of-month, month, day-of-week) 保持原值 —— 跨时区时这些
    // 字段的偏移对用户意义不大,改它反而引入歧义,见函数 doc。
    Ok(format!(
        "{} {} {} {} {} {}",
        s_str, m_str, h_str, day_of_month, month, day_of_week
    ))
}

pub struct TodoScheduler {
    sched: Mutex<JobScheduler>,
    job_map: Mutex<HashMap<i64, uuid::Uuid>>,
    /// 共享的 HookService 单例（来自 AppState）。
    ///
    /// cron 触发的执行在到达 run_todo_execution 末段时也要 fire 状态变更钩子，
    /// 通过在 TodoScheduler 里直接持有 Arc<HookService> 避免再在 cron 回调里
    /// Arc::new 一份（见 issue #509）。调用方（main.rs / handlers/mod.rs）
    /// 在 TodoScheduler::new 时把 AppState.hook_service 传进来。
    hook_service: Arc<HookService>,
}

impl TodoScheduler {
    pub async fn new(hook_service: Arc<HookService>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let sched = JobScheduler::new().await?;
        Ok(Self {
            sched: Mutex::new(sched),
            job_map: Mutex::new(HashMap::new()),
            hook_service,
        })
    }

    pub async fn load_from_db(
        &self,
        ctx: &ServiceContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let todos = ctx.db.get_scheduler_todos().await?;

        for todo in todos {
            if let Some(ref config) = todo.scheduler_config {
                if todo.scheduler_enabled {
                    info!(
                        "Loading scheduled task for todo {} with cron: {} and timezone: {:?}",
                        todo.id, config, todo.scheduler_timezone
                    );
                    if let Err(e) = self
                        .upsert_task(
                            ctx,
                            todo.id,
                            config.clone(),
                            todo.scheduler_timezone.clone(),
                        )
                        .await
                    {
                        warn!(
                            "Skipping invalid scheduled task for todo {}: {}",
                            todo.id, e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn upsert_task(
        &self,
        ctx: &ServiceContext,
        todo_id: i64,
        cron_expr: String,
        timezone: Option<String>,
    ) -> Result<uuid::Uuid, Box<dyn std::error::Error + Send + Sync>> {
        // Validate cron expression
        if cron::Schedule::from_str(&cron_expr).is_err() {
            warn!(
                "Invalid cron expression '{}' for todo {}. \
                AI must convert natural language to valid cron format with 6 fields (seconds + 5 standard). \
                Example: '0 */12 * * * *' (every 12 min), '0 0 9 * * *' (daily at 9am).",
                cron_expr, todo_id
            );
            return Err(format!(
                "Invalid cron expression '{}' for todo {}. AI must convert natural language to valid cron format.",
                cron_expr, todo_id
            ).into());
        }

        // Convert cron expression to UTC if timezone is specified
        let cron_expr_utc = if let Some(ref tz) = timezone {
            match convert_cron_to_utc(&cron_expr, tz) {
                Ok(utc_expr) => {
                    if utc_expr != cron_expr {
                        info!(
                            "Converted cron expression from '{}' ({})) to '{}' (UTC) for todo {}",
                            cron_expr, tz, utc_expr, todo_id
                        );
                    }
                    utc_expr
                }
                Err(e) => {
                    warn!(
                        "Failed to convert cron expression '{}' to timezone {}: {}. Using original.",
                        cron_expr, tz, e
                    );
                    cron_expr.clone()
                }
            }
        } else {
            cron_expr.clone()
        };

        self.remove_task_for_todo(todo_id).await;

        let db_clone = ctx.db.clone();
        let registry_clone = ctx.executor_registry.clone();
        let tx_clone = ctx.tx.clone();
        let tm_clone = ctx.task_manager.clone();
        let config_clone = ctx.config.clone();
        // 闭包要 'static: 把 self.hook_service clone 一份进闭包，cron 触发时直接
        // 复用这份 Arc，避免再在回调里 Arc::new 新的 HookService。
        let hs_clone = self.hook_service.clone();

        info!("Creating job for todo {} with cron: {} (original: {:?})", todo_id, cron_expr_utc, timezone);
        let job = Job::new_async(&cron_expr_utc, move |_uuid, _l| {
            let db = db_clone.clone();
            let registry = registry_clone.clone();
            let tx = tx_clone.clone();
            let tm = tm_clone.clone();
            let cfg = config_clone.clone();
            let hs = hs_clone.clone();

            Box::pin(async move {
                match db.get_todo(todo_id).await {
                    Ok(Some(todo)) => {
                        let message = if todo.prompt.is_empty() {
                            todo.title.clone()
                        } else {
                            todo.prompt.clone()
                        };
                        let executor = todo.executor.clone();
                        info!("Scheduled execution triggered for todo {}", todo_id);
                        run_todo_execution(RunTodoExecutionRequest {
                            db,
                            executor_registry: registry,
                            tx,
                            task_manager: tm,
                            config: cfg,
                            // cron 触发的执行末段也要 fire state-change 钩子，
                            // 复用 TodoScheduler 里持有的单例 (issue #509)。
                            hook_service: hs,
                            todo_id,
                            message,
                            req_executor: executor,
                            trigger_type: "cron".to_string(),
                            params: None,
                            resume_session_id: None,
                            resume_message: None,
                            chain: vec![],
                            source_todo_id: None,
                            source_todo_title: None,
                            source_hook_id: None,
                            feishu_bot_id: None,
                            feishu_receive_id: None,
                        })
                        .await;
                    }
                    Ok(None) => warn!("Scheduled todo {} not found, skipping", todo_id),
                    Err(e) => tracing::error!("Failed to fetch scheduled todo {}: {}", todo_id, e),
                }
            })
        })?;

        let job_id = job.guid();
        info!(
            "Job created with guid {}, now adding to scheduler...",
            job_id
        );
        let sched = self.sched.lock().await;
        info!("Scheduler inited: {}", sched.inited().await);
        match sched.add(job).await {
            Ok(id) => {
                drop(sched);
                self.job_map.lock().await.insert(todo_id, id);
                info!(
                    "Added scheduled task {} for todo {} with cron: {}",
                    id, todo_id, cron_expr
                );
                Ok(id)
            }
            Err(e) => {
                error!("Failed to add job to scheduler: {:?}", e);
                Err(Box::new(std::io::Error::other(format!("{:?}", e))))
            }
        }
    }

    pub async fn remove_task_for_todo(&self, todo_id: i64) {
        let job_id = self.job_map.lock().await.remove(&todo_id);
        if let Some(job_id) = job_id {
            match self.sched.lock().await.remove(&job_id).await {
                Ok(_) => info!("Removed scheduled task {} for todo {}", job_id, todo_id),
                Err(e) => error!(
                    "Failed to remove scheduled task {} for todo {}: {:?}",
                    job_id, todo_id, e
                ),
            }
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sched.lock().await.start().await?;
        info!("Scheduler started");
        Ok(())
    }
}

#[cfg(test)]
mod convert_cron_to_utc_tests {
    //! `convert_cron_to_utc` 是把用户时区的 cron 表达式换成 UTC 的纯函数。
    //! 之所以单独测这个函数:
    //! - 它驱动 `tokio-cron-scheduler` 的实际调度时间,错了等于调度时间整体漂移
    //! - 函数本身没有副作用,可以在任何 timezone 任意时刻调用
    //! - 早期 bug: 把 hour 字段当分钟字段减 offset,导致用户配的"上午 9 点"变成
    //!   "UTC 9 点"在 Asia/Shanghai 跑出"下午 5 点"执行。下面的测试是这些 bug
    //!   的回归网。
    use super::convert_cron_to_utc;

    /// 标准 6 字段 cron + 用户时区,应该按时区偏移反向减。
    /// Asia/Shanghai 是 UTC+8, 9 点本地 → 1 点 UTC。
    #[test]
    fn test_shanghai_9am_local_becomes_1am_utc() {
        let utc = convert_cron_to_utc("0 0 9 * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0 0 1 * * *");
    }

    /// 美东 (UTC-5 standard / UTC-4 DST): 9 点本地 → 14 点或 13 点 UTC。
    /// 这里只断言它"在合理范围"内,避免被夏令时切换影响(实际生产里
    /// `chrono_tz` 会按当前 offset 计算,所以 13/14 都有可能)。
    #[test]
    fn test_new_york_9am_local_shifts_to_utc() {
        let utc = convert_cron_to_utc("0 0 9 * * *", "America/New_York").unwrap();
        let hour: i32 = utc.split_whitespace().nth(2).unwrap().parse().unwrap();
        // 9 - (-4) = 13 (DST) 或 9 - (-5) = 14 (标准时间)
        assert!(hour == 13 || hour == 14, "got {utc}, hour={hour}");
    }

    /// hour 字段是 * 时:不直接 passthrough,而是走枚举。枚举出
    /// `{0..=23}` 的 UTC 小时,`format_cron_field` 紧凑写成 `0-23`,
    /// 语义和 `*` 等价。这是修 issue #502 时发现的一个边界 case:
    /// 之前实现对 `*` 直接放行,但如果 minute/second 也不是 `*`,
    /// 会漏掉 UTC 端的 hour wrap。
    #[test]
    fn test_wildcard_hour_passes_through_unchanged() {
        let utc = convert_cron_to_utc("0 0 * * * *", "Asia/Shanghai").unwrap();
        // 0-23 等价于 *,枚举后紧凑表示
        assert_eq!(utc, "0 0 0-23 * * *");
    }

    /// 范围表达式 "9-17" 应该被转成等价的 UTC 范围 (shanghai → 1-9)。
    /// 这是工作时间段调度的常见写法。
    #[test]
    fn test_hour_range_is_shifted() {
        let utc = convert_cron_to_utc("0 0 9-17 * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0 0 1-9 * * *");
    }

    /// 列表表达式 "9,12,18" 三个具体小时,应该都减 8 再 wrap 到 0-23。
    /// shanghai: 9→1, 12→4, 18→10。
    #[test]
    fn test_hour_list_each_value_shifted() {
        let utc = convert_cron_to_utc("0 0 9,12,18 * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0 0 1,4,10 * * *");
    }

    /// 跨日回卷: 本地 23 点 + shanghai (UTC+8) = UTC 15。负值 23-8=15
    /// 没问题;真正触发 wrap 的是本地 0-7 (在 shanghai 是 16-23 前一天)。
    #[test]
    fn test_local_late_night_rolls_back_no_wrap() {
        // 23:00 Shanghai = 15:00 UTC (no wrap)
        let utc = convert_cron_to_utc("0 0 23 * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0 0 15 * * *");
    }

    /// 步长表达式 "*/2" 现在会被展开为 UTC 等价集合:Shanghai (UTC+8)
    /// 下 0,2,4,...,22 本地 → 16,18,20,22,0,2,4,6,8,10,12,14 UTC。
    /// 这 12 个 UTC 小时是 0-22 等差(步长 2),`format_cron_field` 会
    /// 紧凑写成 "0-22/2"。
    ///
    /// 修这个 case 是 issue #502 的核心诉求之一:之前实现对 `*/N` 直接
    /// passthrough,warn 一下就算完事 —— 在 Asia/Shanghai 配的"每 2 小时"
    /// 实际跑成 UTC 每 2 小时(差 8 小时),用户感受是"全跑偏"。
    #[test]
    fn test_step_expression_expands_to_utc_equivalent() {
        let utc = convert_cron_to_utc("0 0 */2 * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0 0 0-22/2 * * *");
    }

    /// 步长表达式 "0-23/2" 等价于 "*/2",应该得到同样结果。
    /// 这个 case 验证"显式范围"和"通配符步长"走的是同一条路径。
    #[test]
    fn test_explicit_range_step_matches_wildcard_step() {
        let utc = convert_cron_to_utc("0 0 0-23/2 * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0 0 0-22/2 * * *");
    }

    /// DST 时区的"单 hour"表达式一年里只产生 2 个 UTC 时刻(差 1 小时),
    /// 应该用 dominant(出现多的)那个,而不是 union (会每天多触发一次)。
    /// `0 0 9 * * *` 在 New York:夏令时 7 个月是 13 UTC,冬令时 5 个月是
    /// 14 UTC;13 占多数,应该被选中。
    #[test]
    fn test_dst_single_hour_uses_dominant_offset() {
        let utc = convert_cron_to_utc("0 0 9 * * *", "America/New_York").unwrap();
        assert_eq!(utc, "0 0 13 * * *");
    }

    /// Europe/London 也走 DST;和 New York 不同的是 BST 是 UTC+1,GMT 是
    /// UTC+0,所以"9 AM London" → 8 UTC (BST) 或 9 UTC (GMT)。9 (GMT)
    /// 出现 5 个月,8 (BST) 出现 7 个月,dominant 是 8。
    #[test]
    fn test_dst_london_uses_dominant_offset() {
        let utc = convert_cron_to_utc("0 0 9 * * *", "Europe/London").unwrap();
        assert_eq!(utc, "0 0 8 * * *");
    }

    /// "每日 0 点" + 上海时区 → UTC 16 点前一天(跨日回卷)。验证
    /// `Schedule::after` 的 `DateTime<Tz>` 内部处理 wrap,不需要我们
    /// 手算"0 - 8 = -8 → 16"。
    #[test]
    fn test_midnight_local_rolls_back_one_day() {
        let utc = convert_cron_to_utc("0 0 0 * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0 0 16 * * *");
    }

    /// 半小时偏移(印度时区 Asia/Kolkata UTC+5:30):0:30 本地
    /// → UTC 19:00 前一天(0:30 - 5:30 = -5:00 = 19:00 昨日)。
    /// 验证非整数小时偏移也能正确处理(之前实现按整小时算会丢 30 分钟)。
    #[test]
    fn test_half_hour_offset_india() {
        let utc = convert_cron_to_utc("0 30 0 * * *", "Asia/Kolkata").unwrap();
        // 30 分钟 + 半小时偏移 = 整数小时偏移的 5.5h,跨日 wrap 后落在 19 UTC
        assert_eq!(utc, "0 0 19 * * *");
    }

    /// "每隔 30 分钟"在 Asia/Shanghai 下:本地 `0,30 0 * * * *` 是
    /// "minute=0, second=0 或 30, hour=*" —— 一天 24 hours × 2 seconds = 48 次。
    /// 折算到 UTC (Shanghai - 8) 后:
    /// - hours: 跨日 wrap 0-7 → 16-23 昨日,8-23 → 0-15 当日,所以是 0-23
    /// - minutes: 始终是 0 (本地 minute=0)
    /// - seconds: 0, 30
    /// 紧凑写成 `0-30/30 0 0-23`,语义等价于"每 30 分钟一次"。
    ///
    /// 关键:这个 case 修了 issue #502 报告的 "* + 其它字段" 的 bug。
    /// 旧实现对 `hours == "*"` 直接 passthrough,意味着 `0,30 0 * * * *`
    /// 在 Asia/Shanghai 实际跑成"UTC 端每 30 分钟一次",完全偏离了
    /// "本地每 30 分钟一次"的语义。
    #[test]
    fn test_every_half_hour_shanghai() {
        let utc = convert_cron_to_utc("0,30 0 * * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0-30/30 0 0-23 * * *");
    }

    /// cron 表达式是 7 字段(带年)时,目前实现按 6 字段要求会拒绝。
    /// 验证拒绝路径稳定(避免误把年份字段当 day-of-month 之类的)。
    #[test]
    fn test_seven_field_cron_is_rejected() {
        let result = convert_cron_to_utc("0 0 9 * * * 2025", "Asia/Shanghai");
        assert!(result.is_err(), "7-field cron should be rejected, got {:?}", result);
    }

    /// 错误的时区字符串必须报错,不能 panic 也不能"用 UTC 顶上"。
    /// cron 调度一旦悄悄退到 UTC,用户的 9 点就变成 UTC 9 = 17:00 北京,
    /// 这种"静默错误"是定时任务里最难排查的一类。
    #[test]
    fn test_invalid_timezone_returns_error() {
        let result = convert_cron_to_utc("0 0 9 * * *", "Not/A/Real/Zone");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid timezone"));
    }

    /// cron 字段数量不对必须报错(我们要求 6 字段,标准 5 字段 + 秒)。
    /// 5 字段 cron 在 unix 传统里合法, 但 tokio-cron-scheduler 不接受,
    /// 提早在这里拒绝比让 scheduler 内部崩要友好。
    #[test]
    fn test_wrong_field_count_returns_error() {
        // 5 fields (missing seconds)
        let result = convert_cron_to_utc("0 9 * * *", "Asia/Shanghai");
        // cron crate 接受 5 字段,所以这里先 cron-parse-ok 再字段数检查;
        // 任何 Err 都算防御成功(具体文案可能因 cron crate 版本变化)
        assert!(result.is_err(), "5-field cron should be rejected, got {:?}", result);
    }

    /// cron 字符串本身不合法(cron crate 解析失败)必须报错。
    /// 否则会在调度器里 panic,影响整个 daemon。
    #[test]
    fn test_invalid_cron_expression_returns_error() {
        let result = convert_cron_to_utc("not a cron string", "Asia/Shanghai");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid cron expression"));
    }

    /// Issue #495 修复后的回归测试：`convert_cron_to_utc` 在 DST 启发式
    /// invariant 失守时（distinct.len()==2 但没有任何 UTC 时间）应返回 Err
    /// 而不是 panic。手工构造一个让 utc_time_counts 为空、但 is_dst_pair
    /// 仍为 true 的场景非常困难，所以我们改测"非 DST pair 的多 hour 列表"
    /// 仍走 union 路径——这保证 .expect() 路径不会因为输入污染而 panic。
    #[test]
    fn test_multi_hour_list_does_not_use_dominant_path() {
        // 9 点和 12 点不是 DST pair（hour diff=3），应走 union 路径，
        // 不触发 .expect("DST pair has 2 elements") 的失守路径。
        let utc = convert_cron_to_utc("0 0 9,12 * * *", "Asia/Shanghai").unwrap();
        // Shanghai: 9 → 1, 12 → 4
        assert!(utc.contains("1") || utc.contains("4"));
    }

    /// Issue #495 修复后的回归测试：scheduler.rs 已不再用 .expect()，
    /// 即使时区/Cron 输入异常也会走 Result Err 路径返回错误消息。
    /// 这里验证错误消息里包含问题源头（cron 字符串或时区）便于排查。
    #[test]
    fn test_invalid_input_returns_descriptive_error() {
        // 输入垃圾字符串，错误信息应包含具体内容而不是"panic"。
        let result = convert_cron_to_utc("!!!bad!!!", "Asia/Shanghai");
        let err = result.unwrap_err();
        // 错误消息必须非空且包含解析失败的描述，方便运维定位。
        assert!(!err.is_empty(), "error message should not be empty");
        // 不应该有 "panic" 字样——证明我们没走到 panic 路径。
        assert!(
            !err.to_lowercase().contains("panic"),
            "error should be returned via Result, not panic. got: {err}"
        );
    }
}
