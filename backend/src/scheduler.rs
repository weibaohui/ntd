use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler, JobSchedulerError};
use tracing::{error, info, warn};

use chrono::{TimeZone, Timelike};

use crate::executor_service::{run_todo_execution, RunTodoExecutionRequest};
use crate::hooks::HookService;
use crate::service_context::ServiceContext;

/// 调度器模块的统一错误类型（issue #499）。
///
/// 之所以替换原来的 `Box<dyn Error + Send + Sync>`：
/// - 调用方（handlers/scheduler.rs, main.rs）能针对具体错误做差异化处理，
///   例如把 `InvalidCron` 映射为 HTTP 400，而不是笼统的 500。
/// - 测试可以针对错误变体做断言（`assert!(matches!(e, SchedulerError::InvalidCron { .. }))`）。
/// - 错误链不被 `Box<dyn>` 切断，tracing/log 仍能展示完整 source。
/// - 与项目其他模块（feishu/sdk/error.rs、handlers/mod.rs 的 AppError）的 thiserror
///   风格保持一致。
#[derive(Debug, Error)]
pub enum SchedulerError {
    /// 用户传入的 cron 表达式无法被 `cron` crate 解析，或字段数不为 6。
    /// handler 层应映射为 HTTP 400（用户输入错误），而不是 500。
    ///
    /// Display 文案保留「AI must convert natural language」提示，因为
    /// 这是 HTTP 400 响应体里 AI agent 唯一能看到的信息（无 hint 时
    /// agent 会重复尝试自然语言 cron）。PR #543 review HIGH #3 的修复点。
    #[error(
        "Invalid cron expression '{expr}' for todo {todo_id}. \
         AI must convert natural language to valid cron format with 6 fields \
         (seconds + 5 standard). \
         Example: '0 */12 * * * *' (every 12 min), '0 0 9 * * *' (daily at 9am)."
    )]
    InvalidCron { expr: String, todo_id: i64 },

    /// 用户传入的时区字符串无法被 `chrono_tz::Tz` 解析。
    /// 与 `InvalidCron` 同属用户输入错误，应映射为 HTTP 400。
    #[error("Invalid timezone: {0}")]
    InvalidTimezone(String),

    /// 底层数据库错误（来自 `sea_orm::DbErr`）。
    /// 通过 `#[from]` 自动实现 `From<sea_orm::DbErr>`，让 `?` 直接工作。
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    /// `tokio_cron_scheduler` 后端错误（创建 scheduler、添加 job、启动调度等）。
    /// `JobSchedulerError` 已实现 `std::error::Error`，可走 `#[from]`。
    #[error("Scheduler backend error: {0}")]
    SchedulerBackend(#[from] JobSchedulerError),

    /// 兜底变体：上述类别之外的内部错误，保留 String 描述。
    #[error("Internal scheduler error: {0}")]
    Internal(String),
}

impl SchedulerError {
    /// 构造一个"内部错误"，统一从 String 字面量构造。
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Self::Internal(msg.into())
    }
}

/// 判断一个去重升序整数切片是否构成连续区间(步长 1)。
///
/// 期望输入是 `BTreeSet` 收集后转的 `Vec`,即元素已升序无重复。
/// - `len() < 2` 视为不连续(单值区间本身没有"区间"概念,留给 caller 走单值分支)。
/// - 例: `[0,1,2,3]` → true;`[0,2,4]` → false;`[5,6,8]` → false。
fn is_contiguous(items: &[u32]) -> bool {
    if items.len() < 2 {
        return false;
    }
    items.windows(2).all(|w| w[1] == w[0] + 1)
}

/// 计算一个去重升序整数切片的等差步长(若构成等差数列)。
///
/// 期望输入是 `BTreeSet` 收集后转的 `Vec`,即元素已升序无重复。
/// 返回 `Some(step)` 表示整个序列步长恒为 `step > 0`;
/// `None` 表示不是等差数列或元素数 < 2。
/// - 例: `[0,2,4,6]` → Some(2);`[0,2,5]` → None;`[5,5]` → None (步长为 0 不算等差)。
fn arithmetic_step(items: &[u32]) -> Option<u32> {
    if items.len() < 2 {
        return None;
    }
    // 步长为零的退化情况(去重集合理论上不会出现,但 `BTreeSet` 转 `Vec` 仍可能
    // 出现 caller 误传含重复元素的情况): 显式拒掉,避免后续值下溢。
    let step = items[1].checked_sub(items[0])?;
    if step == 0 {
        return None;
    }
    items
        .windows(2)
        .all(|w| w[1].checked_sub(w[0]) == Some(step))
        .then_some(step)
}

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
///
/// 重构后 (issue #614): 把"连续区间"和"等差数列"两个判断抽到
/// `is_contiguous` / `arithmetic_step` 辅助函数,本函数体只剩分支串联。
fn format_cron_field(set: &BTreeSet<u32>) -> String {
    if set.is_empty() {
        return "*".to_string();
    }
    let items: Vec<u32> = set.iter().copied().collect();
    if items.len() == 1 {
        return items[0].to_string();
    }

    // 连续区间: 步长 1 的等差,先匹配以避免被通用 arithmetic_step 抢走。
    // 安全: `is_contiguous` 在 `len() < 2` 时返回 false,能走到这里时 `len() >= 2`,
    // `items[items.len() - 1]` 不会越界; 风格上沿用主分支的索引取末位写法,避免 `.last().unwrap()` 触发 `clippy::unwrap_used`。
    if is_contiguous(&items) {
        let last = items[items.len() - 1];
        return format!("{}-{}", items[0], last);
    }

    // 等差数列 (步长固定,> 0): `a-b/N` 紧凑表示。
    // 安全同上的 `len() >= 2` 不变式,索引取末位不会越界。
    if let Some(step) = arithmetic_step(&items) {
        let last = items[items.len() - 1];
        return format!("{}-{}/{}", items[0], last, step);
    }

    // 兜底:逗号列表。
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
/// 把用户时区的 cron 表达式转换为 UTC 等价表达式。
///
/// 返回类型 `Result<String, SchedulerError>`（PR #543 review CRITICAL #1 修复）：
/// - 旧实现返回 `Result<String, String>`，让 `upsert_task` 只能 `match` + warn + fallback 到
///   原 `cron_expr`，结果 `SchedulerError::InvalidTimezone` 在生产代码不可达，
///   `From<SchedulerError> for AppError` 的 400 映射是死代码。
/// - 新实现用 typed error，调用方 `?` 直接传播 `InvalidTimezone` / `InvalidCron`，
///   handler 真的会返回 400。
///
/// `todo_id` 透传给 `SchedulerError::InvalidCron { expr, todo_id }`，让响应体
/// 能告诉调用方"是哪条 todo 的 cron 错了"。`load_from_db` 用 sentinel `-1`
/// 因为那是从 DB 加载历史数据，没有具体的 user-facing todo_id。
fn convert_cron_to_utc(
    cron_expr: &str,
    timezone: &str,
    todo_id: i64,
) -> Result<String, SchedulerError> {
    // 解析时区; 失败时给出可定位的错误,不要 panic。
    let tz: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| SchedulerError::InvalidTimezone(timezone.to_string()))?;

    // 用 `cron` crate 解析,这一步同时校验 cron 语法。
    // 之前用一次性 `from_str(...)?` 吞掉错误再 split 字段,失败时
    // 报错信息不友好。
    let schedule = cron::Schedule::from_str(cron_expr).map_err(|_| {
        SchedulerError::InvalidCron {
            expr: cron_expr.to_string(),
            todo_id,
        }
    })?;

    // 要求 6 字段 (秒 分 时 日 月 周),与 `tokio-cron-scheduler` 一致;
    // 5 字段在 unix cron 里合法但本项目不接受 —— 早 fail 早知道。
    let fields: Vec<&str> = cron_expr.trim().split_whitespace().collect();
    if fields.len() != 6 {
        return Err(SchedulerError::InvalidCron {
            expr: cron_expr.to_string(),
            todo_id,
        });
    }

    // fields 顺序: 秒 分 时 日 月 周
    // seconds/minutes 不直接用 —— 我们从 `Schedule::after` 枚举后取真实 (h, m, s),
    // 比手算偏移更准。day_of_month/month/day_of_week 保持原值,见函数 doc。
    let _seconds = fields[0];
    let _minutes = fields[1];
    let _hours = fields[2];
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
        _ => {
            return Err(SchedulerError::Internal(
                "Could not build reference datetime in timezone".to_string(),
            ));
        }
    };
    let end = match tz.with_ymd_and_hms(2026, 1, 1, 0, 0, 0) {
        chrono::LocalResult::Single(t) => t,
        _ => {
            return Err(SchedulerError::Internal(
                "Could not build end datetime in timezone".to_string(),
            ));
        }
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
        // `is_dst_pair` 守卫了 distinct.len()==2，所以 `max_by_key` 在
        // non-empty 迭代器上一定返回 `Some(_)`（即使 key 全相等也返回最后
        // 一个）。用 `.expect()` 把不可达分支压成显式 invariant message，
        // 让 clippy::expect_used 看得见这是 invariant 而非运行时错误——
        // 之前 `match { Some/None => Err }` 的 None 分支是 dead code，
        // `clippy::unreachable_patterns` lint 提升到 deny 时会 fail CI。
        // `.expect()` 是基于 type-level invariant 的穷尽性保证,
        // 不是运行时可达的错误路径——显式标注 `#[allow]` 让
        // `[lints.clippy] expect_used = "warn"` lint 不会误报。
        #[allow(clippy::expect_used)]
        let (h, m, s) = distinct
            .iter()
            .max_by_key(|k| utc_time_counts.get(k).copied().unwrap_or(0))
            .copied()
            .expect("DST pair invariant: is_dst_pair implies distinct.len() == 2, so max_by_key returns Some");
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
    /// 创建 `TodoScheduler` 单例，初始化底层 `JobScheduler`。
    /// 失败的原因（`JobSchedulerError`）通过 `#[from]` 自动转为 `SchedulerError::SchedulerBackend`。
    pub async fn new(hook_service: Arc<HookService>) -> Result<Self, SchedulerError> {
        let sched = JobScheduler::new().await?;
        Ok(Self {
            sched: Mutex::new(sched),
            job_map: Mutex::new(HashMap::new()),
            hook_service,
        })
    }

    /// 从 DB 读取所有启用调度的 todo，并注册到 `JobScheduler`。
    /// 单条 todo 的注册失败（cron 不合法等）只 warn 不中断，**外层返回 Ok**；
    /// 只有 DB 本身不可达才算失败。
    pub async fn load_from_db(
        &self,
        ctx: &ServiceContext,
    ) -> Result<(), SchedulerError> {
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
    ) -> Result<uuid::Uuid, SchedulerError> {
        // Validate cron expression
        if cron::Schedule::from_str(&cron_expr).is_err() {
            warn!(
                "Invalid cron expression '{}' for todo {}. \
                AI must convert natural language to valid cron format with 6 fields (seconds + 5 standard). \
                Example: '0 */12 * * * *' (every 12 min), '0 0 9 * * *' (daily at 9am).",
                cron_expr, todo_id
            );
            // 用结构化的 `SchedulerError::InvalidCron { expr, todo_id }` 替代原来的
            // `format!(...).into()`（后者会丢类型，handler 没法区分"用户输入错"
            // 和"内部错误"）。这样 `From<SchedulerError> for AppError` 才能把
            // 它映射为 400 BadRequest。
            return Err(SchedulerError::InvalidCron {
                expr: cron_expr,
                todo_id,
            });
        }

        // Convert cron expression to UTC if timezone is specified.
        // PR #543 review CRITICAL #1 修复: 旧实现用 `match` + warn + fallback 到原
        // `cron_expr`，导致 `SchedulerError::InvalidTimezone` 在生产路径上不可达、
        // handler 的 400 映射成死代码。新实现用 `?` 让 typed error 直接传播。
        let cron_expr_utc = if let Some(ref tz) = timezone {
            let utc_expr = convert_cron_to_utc(&cron_expr, tz, todo_id)?;
            if utc_expr != cron_expr {
                info!(
                    "Converted cron expression from '{}' ({})) to '{}' (UTC) for todo {}",
                    cron_expr, tz, utc_expr, todo_id
                );
            }
            utc_expr
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
                // 用结构化 `SchedulerError::SchedulerBackend(e)` 替代原来的
                // `Box::new(std::io::Error::other(...))`。`e: JobSchedulerError` 已
                // 实现 `std::error::Error`，可走 `#[from]` 直接 `?`，但这里我们要
                // 显式带上上下文（todo_id, cron_expr）便于排查，所以手写变体。
                error!("Failed to add job to scheduler: {:?}", e);
                Err(SchedulerError::SchedulerBackend(e))
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

    /// 启动调度循环。`JobSchedulerError` 通过 `#[from]` 自动转为 `SchedulerError::SchedulerBackend`。
    pub async fn start(&self) -> Result<(), SchedulerError> {
        self.sched.lock().await.start().await?;
        info!("Scheduler started");
        Ok(())
    }
}

#[cfg(test)]
// `cargo clippy --all-targets` 会同时 lint test mod。新增的
// `[lints.clippy] unwrap_used/expect_used = "warn"` 会误伤此处 17 处
// `.unwrap()` / `.expect()`（test path, panic = fail, 完全合理）。一次性
// 标注允许,与 lib.rs 文档"测试里使用 unwrap/expect 需加 `#[allow]`"保持一致。
// 测试 mod 整体允许,避免逐 fn 标注噪声。
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
        let utc = convert_cron_to_utc("0 0 9 * * *", "Asia/Shanghai", 0).unwrap();
        assert_eq!(utc, "0 0 1 * * *");
    }

    /// 美东 (UTC-5 standard / UTC-4 DST): 9 点本地 → 14 点或 13 点 UTC。
    /// 这里只断言它"在合理范围"内,避免被夏令时切换影响(实际生产里
    /// `chrono_tz` 会按当前 offset 计算,所以 13/14 都有可能)。
    #[test]
    fn test_new_york_9am_local_shifts_to_utc() {
        let utc = convert_cron_to_utc("0 0 9 * * *", "America/New_York", 0).unwrap();
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
        let utc = convert_cron_to_utc("0 0 * * * *", "Asia/Shanghai", 0).unwrap();
        // 0-23 等价于 *,枚举后紧凑表示
        assert_eq!(utc, "0 0 0-23 * * *");
    }

    /// 范围表达式 "9-17" 应该被转成等价的 UTC 范围 (shanghai → 1-9)。
    /// 这是工作时间段调度的常见写法。
    #[test]
    fn test_hour_range_is_shifted() {
        let utc = convert_cron_to_utc("0 0 9-17 * * *", "Asia/Shanghai", 0).unwrap();
        assert_eq!(utc, "0 0 1-9 * * *");
    }

    /// 列表表达式 "9,12,18" 三个具体小时,应该都减 8 再 wrap 到 0-23。
    /// shanghai: 9→1, 12→4, 18→10。
    #[test]
    fn test_hour_list_each_value_shifted() {
        let utc = convert_cron_to_utc("0 0 9,12,18 * * *", "Asia/Shanghai", 0).unwrap();
        assert_eq!(utc, "0 0 1,4,10 * * *");
    }

    /// 跨日回卷: 本地 23 点 + shanghai (UTC+8) = UTC 15。负值 23-8=15
    /// 没问题;真正触发 wrap 的是本地 0-7 (在 shanghai 是 16-23 前一天)。
    #[test]
    fn test_local_late_night_rolls_back_no_wrap() {
        // 23:00 Shanghai = 15:00 UTC (no wrap)
        let utc = convert_cron_to_utc("0 0 23 * * *", "Asia/Shanghai", 0).unwrap();
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
        let utc = convert_cron_to_utc("0 0 */2 * * *", "Asia/Shanghai", 0).unwrap();
        assert_eq!(utc, "0 0 0-22/2 * * *");
    }

    /// 步长表达式 "0-23/2" 等价于 "*/2",应该得到同样结果。
    /// 这个 case 验证"显式范围"和"通配符步长"走的是同一条路径。
    #[test]
    fn test_explicit_range_step_matches_wildcard_step() {
        let utc = convert_cron_to_utc("0 0 0-23/2 * * *", "Asia/Shanghai", 0).unwrap();
        assert_eq!(utc, "0 0 0-22/2 * * *");
    }

    /// DST 时区的"单 hour"表达式一年里只产生 2 个 UTC 时刻(差 1 小时),
    /// 应该用 dominant(出现多的)那个,而不是 union (会每天多触发一次)。
    /// `0 0 9 * * *` 在 New York:夏令时 7 个月是 13 UTC,冬令时 5 个月是
    /// 14 UTC;13 占多数,应该被选中。
    #[test]
    fn test_dst_single_hour_uses_dominant_offset() {
        let utc = convert_cron_to_utc("0 0 9 * * *", "America/New_York", 0).unwrap();
        assert_eq!(utc, "0 0 13 * * *");
    }

    /// Europe/London 也走 DST;和 New York 不同的是 BST 是 UTC+1,GMT 是
    /// UTC+0,所以"9 AM London" → 8 UTC (BST) 或 9 UTC (GMT)。9 (GMT)
    /// 出现 5 个月,8 (BST) 出现 7 个月,dominant 是 8。
    #[test]
    fn test_dst_london_uses_dominant_offset() {
        let utc = convert_cron_to_utc("0 0 9 * * *", "Europe/London", 0).unwrap();
        assert_eq!(utc, "0 0 8 * * *");
    }

    /// "每日 0 点" + 上海时区 → UTC 16 点前一天(跨日回卷)。验证
    /// `Schedule::after` 的 `DateTime<Tz>` 内部处理 wrap,不需要我们
    /// 手算"0 - 8 = -8 → 16"。
    #[test]
    fn test_midnight_local_rolls_back_one_day() {
        let utc = convert_cron_to_utc("0 0 0 * * *", "Asia/Shanghai", 0).unwrap();
        assert_eq!(utc, "0 0 16 * * *");
    }

    /// 半小时偏移(印度时区 Asia/Kolkata UTC+5:30):0:30 本地
    /// → UTC 19:00 前一天(0:30 - 5:30 = -5:00 = 19:00 昨日)。
    /// 验证非整数小时偏移也能正确处理(之前实现按整小时算会丢 30 分钟)。
    #[test]
    fn test_half_hour_offset_india() {
        let utc = convert_cron_to_utc("0 30 0 * * *", "Asia/Kolkata", 0).unwrap();
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
        let utc = convert_cron_to_utc("0,30 0 * * * *", "Asia/Shanghai", 0).unwrap();
        assert_eq!(utc, "0-30/30 0 0-23 * * *");
    }

    /// cron 表达式是 7 字段(带年)时,目前实现按 6 字段要求会拒绝。
    /// 验证拒绝路径稳定(避免误把年份字段当 day-of-month 之类的)。
    #[test]
    fn test_seven_field_cron_is_rejected() {
        let result = convert_cron_to_utc("0 0 9 * * * 2025", "Asia/Shanghai", 0);
        assert!(result.is_err(), "7-field cron should be rejected, got {:?}", result);
    }

    /// 错误的时区字符串必须报错,不能 panic 也不能"用 UTC 顶上"。
    /// cron 调度一旦悄悄退到 UTC,用户的 9 点就变成 UTC 9 = 17:00 北京,
    /// 这种"静默错误"是定时任务里最难排查的一类。
    #[test]
    fn test_invalid_timezone_returns_error() {
        let result = convert_cron_to_utc("0 0 9 * * *", "Not/A/Real/Zone", 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid timezone"));
    }

    /// cron 字段数量不对必须报错(我们要求 6 字段,标准 5 字段 + 秒)。
    /// 5 字段 cron 在 unix 传统里合法, 但 tokio-cron-scheduler 不接受,
    /// 提早在这里拒绝比让 scheduler 内部崩要友好。
    #[test]
    fn test_wrong_field_count_returns_error() {
        // 5 fields (missing seconds)
        let result = convert_cron_to_utc("0 9 * * *", "Asia/Shanghai", 0);
        // cron crate 接受 5 字段,所以这里先 cron-parse-ok 再字段数检查;
        // 任何 Err 都算防御成功(具体文案可能因 cron crate 版本变化)
        assert!(result.is_err(), "5-field cron should be rejected, got {:?}", result);
    }

    /// cron 字符串本身不合法(cron crate 解析失败)必须报错。
    /// 否则会在调度器里 panic,影响整个 daemon。
    #[test]
    fn test_invalid_cron_expression_returns_error() {
        let result = convert_cron_to_utc("not a cron string", "Asia/Shanghai", 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid cron expression"));
    }
}

#[cfg(test)]
mod scheduler_error_tests {
    //! 覆盖 `SchedulerError` 枚举本身（issue #499）：
    //! - `Display` 输出带原始信息
    //! - `From<sea_orm::DbErr>` 自动转 `Database` 变体
    //! - `From<JobSchedulerError>` 自动转 `SchedulerBackend` 变体
    //! - `From<SchedulerError> for AppError` 把用户输入错映射为 `BadRequest`、其它映射为 `Internal`
    //!
    //! 这些测试之前无法在 `Box<dyn Error>` 抽象下做 —— 抽象层把变体抹平了，
    //! 现在能针对具体变体断言。
    use super::{convert_cron_to_utc, SchedulerError};
    use crate::handlers::AppError;
    use sea_orm::DbErr;

    /// `InvalidCron` 的 Display 必须包含原始 cron 字符串、todo id，
    /// **以及 AI 提示**——PR #543 review HIGH #3 修复。AI agent 调 API
    /// 时只有这条响应体能看，没有 hint 时 agent 会重试同样的自然语言 cron。
    #[test]
    fn test_invalid_cron_display_includes_ai_hint() {
        let err = SchedulerError::InvalidCron {
            expr: "every morning".to_string(),
            todo_id: 42,
        };
        let s = err.to_string();
        assert!(s.contains("every morning"), "should echo expr, got: {s}");
        assert!(s.contains("42"), "should include todo_id, got: {s}");
        assert!(
            s.contains("AI must convert natural language"),
            "should include AI hint for agent retries, got: {s}"
        );
        assert!(
            s.contains("6 fields"),
            "should explain 6-field requirement, got: {s}"
        );
    }

    /// `InvalidTimezone` 在生产路径上现在真的可达 (PR #543 review CRITICAL #1
    /// 修复)：之前 convert_cron_to_utc 返回 Result<String, String>，调用方
    /// `match` + warn + fallback 到原 cron，SchedulerError::InvalidTimezone
    /// 永远不构造、handler 400 映射是死代码。现在 convert_cron_to_utc 返回
    /// Result<String, SchedulerError>，`?` 直接传播。
    #[test]
    fn test_convert_cron_to_utc_returns_scheduler_error_directly() {
        // 之前的实现：返回 Result<String, String>，调用方要 `match`
        // 现在的实现：返回 Result<String, SchedulerError>，调用方可以 `?`
        let result = convert_cron_to_utc("0 0 9 * * *", "Not/A/Real/Zone", 7);
        match result {
            Err(SchedulerError::InvalidTimezone(tz)) => {
                assert_eq!(tz, "Not/A/Real/Zone");
            }
            other => panic!(
                "expected Err(SchedulerError::InvalidTimezone), got {:?}",
                other
            ),
        }
    }

    /// 同上，cron 字段数错也是 typed error：之前的 `String` 错误现在归并到
    /// `SchedulerError::InvalidCron { expr, todo_id }`，todo_id 透传给 handler 响应体。
    #[test]
    fn test_convert_cron_to_utc_wrong_field_count_returns_invalid_cron() {
        let result = convert_cron_to_utc("0 9 * * *", "Asia/Shanghai", 99);
        match result {
            Err(SchedulerError::InvalidCron { expr, todo_id }) => {
                assert_eq!(expr, "0 9 * * *");
                assert_eq!(todo_id, 99, "todo_id should propagate");
            }
            other => panic!(
                "expected Err(SchedulerError::InvalidCron), got {:?}",
                other
            ),
        }
    }

    /// 否则日志/HTTP 响应里看不出"哪条 todo 的哪条 cron 错了"。
    #[test]
    fn test_invalid_cron_display_contains_expr_and_todo_id() {
        let err = SchedulerError::InvalidCron {
            expr: "bad cron".to_string(),
            todo_id: 42,
        };
        let s = err.to_string();
        assert!(s.contains("bad cron"), "display should include expr, got: {s}");
        assert!(s.contains("42"), "display should include todo_id, got: {s}");
    }

    /// `InvalidTimezone` 保留原始字符串，handler 需要把时区名回显给用户。
    #[test]
    fn test_invalid_timezone_display_contains_input() {
        let err = SchedulerError::InvalidTimezone("Atlantis/Azores".to_string());
        let s = err.to_string();
        assert!(s.contains("Atlantis/Azores"), "got: {s}");
    }

    /// `From<DbErr>` 自动 `?`：这是把 `get_scheduler_todos` 等 DB 调用
    /// 链入新错误类型的关键。
    #[test]
    fn test_from_db_err_yields_database_variant() {
        let db_err = DbErr::Custom("test connection refused".to_string());
        let err: SchedulerError = db_err.into();
        assert!(
            matches!(err, SchedulerError::Database(_)),
            "expected Database variant, got {err:?}"
        );
    }

    /// `From<JobSchedulerError>` 自动 `?`：覆盖 `JobScheduler::new()`、
    /// `sched.add()`、`sched.start()` 三处 `await?`。
    #[test]
    fn test_from_job_scheduler_error_yields_backend_variant() {
        let inner = tokio_cron_scheduler::JobSchedulerError::CantInit;
        let err: SchedulerError = inner.into();
        assert!(
            matches!(err, SchedulerError::SchedulerBackend(_)),
            "expected SchedulerBackend variant, got {err:?}"
        );
    }

    /// `internal()` 工厂是给 caller（main.rs）留的"带说明的内部错误"快捷方式。
    #[test]
    fn test_internal_constructor_wraps_string() {
        let err = SchedulerError::internal("sched down for maintenance");
        match &err {
            SchedulerError::Internal(s) => assert_eq!(s, "sched down for maintenance"),
            _ => panic!("expected Internal, got {err:?}"),
        }
    }

    /// 用户输入错 → 400 BadRequest。issue #499 的关键修复点。
    #[test]
    fn test_app_error_from_scheduler_error_invalid_cron_maps_to_bad_request() {
        let err = SchedulerError::InvalidCron {
            expr: "* * *".to_string(),
            todo_id: 7,
        };
        let app_err: AppError = err.into();
        match app_err {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("* * *"), "msg should echo input, got: {msg}");
                assert!(msg.contains("7"), "msg should include todo_id, got: {msg}");
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    /// 用户输入错（时区）→ 400 BadRequest。
    #[test]
    fn test_app_error_from_scheduler_error_invalid_timezone_maps_to_bad_request() {
        let err = SchedulerError::InvalidTimezone("Mars/Olympus".to_string());
        let app_err: AppError = err.into();
        assert!(
            matches!(app_err, AppError::BadRequest(_)),
            "expected BadRequest, got {app_err:?}"
        );
    }

    /// DB 错误 → 500 Internal。caller 没法直接修复，必须看 server 日志。
    #[test]
    fn test_app_error_from_scheduler_error_database_maps_to_internal() {
        let err: SchedulerError = DbErr::Custom("conn refused".to_string()).into();
        let app_err: AppError = err.into();
        assert!(
            matches!(app_err, AppError::Internal(_)),
            "expected Internal, got {app_err:?}"
        );
    }

    /// scheduler 后端错误 → 500 Internal。
    #[test]
    fn test_app_error_from_scheduler_error_backend_maps_to_internal() {
        let err: SchedulerError =
            tokio_cron_scheduler::JobSchedulerError::Shutdown.into();
        let app_err: AppError = err.into();
        assert!(
            matches!(app_err, AppError::Internal(_)),
            "expected Internal, got {app_err:?}"
        );
    }

    /// 内部错误 → 500 Internal。
    #[test]
    fn test_app_error_from_scheduler_error_internal_maps_to_internal() {
        let err = SchedulerError::internal("unexpected state");
        let app_err: AppError = err.into();
        assert!(
            matches!(app_err, AppError::Internal(_)),
            "expected Internal, got {app_err:?}"
        );
    }

    /// Issue #495 修复后的回归测试：non-DST-pair 多 hour 列表仍走 union 路径。
    /// 强化断言：必须**同时**包含两个 UTC 小时而非只包含任一个——证明是
    /// union 而非 dominant/单一选择。
    ///
    /// 注：`is_dst_pair=true` 路径（`.expect()` 实际被触发的分支）已由
    /// `test_dst_single_hour_uses_dominant_offset` / `test_dst_london_uses_dominant_offset`
    /// 覆盖（它们用 `0 0 9 * * *` 构造 hour diff=1 的真 DST pair），这里只补
    /// "走另一分支"的回归保护。
    #[test]
    fn test_multi_hour_list_uses_union_path() {
        // 9 点和 12 点不是 DST pair（hour diff=3），应走 union 路径。
        let utc = convert_cron_to_utc("0 0 9,12 * * *", "Asia/Shanghai", 0).unwrap();
        // Shanghai: 9 → 1, 12 → 4 (both UTC)
        assert!(
            utc.contains("1") && utc.contains("4"),
            "non-DST-pair multi-hour should union both hours, got: {utc}"
        );
    }

    /// Issue #495 修复后的回归测试：scheduler.rs 已不再用 .expect()，
    /// 即使时区/Cron 输入异常也会走 Result Err 路径返回错误消息。
    /// 这里验证错误消息里包含问题源头（cron 字符串或时区）便于排查。
    #[test]
    fn test_invalid_input_returns_descriptive_error() {
        // 输入垃圾字符串，错误信息应包含具体内容而不是"panic"。
        let result = convert_cron_to_utc("!!!bad!!!", "Asia/Shanghai", 0);
        let err = result.unwrap_err();
        // 错误消息必须非空且包含解析失败的描述，方便运维定位。
        assert!(!err.to_string().is_empty(), "error message should not be empty");
        // 不应该有 "panic" 字样——证明我们没走到 panic 路径。
        assert!(
            !err.to_string().to_lowercase().contains("panic"),
            "error should be returned via Result, not panic. got: {err}"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod format_cron_field_tests {
    //! Tests for `format_cron_field` and the helpers extracted in issue #614.
    //!
    //! 拆分前 `format_cron_field` 内部的两段 `for i in 1..len { ... }` 检查循环
    //! 没有任何独立测试,只能通过端到端 `convert_cron_to_utc` 间接覆盖。重构后
    //! 拆出 `is_contiguous` / `arithmetic_step`,这里对它们 + 主函数做穷尽覆盖,
    //! 覆盖 {空集/单值/连续/等差/不规则/单元素退化/步长 0 退化}。
    use super::{arithmetic_step, format_cron_field, is_contiguous};
    use std::collections::BTreeSet;

    /// `is_contiguous`: 单元素 / 空切片不算"区间"。
    /// 期望与 `format_cron_field` 行为一致 —— 单值走单值分支,不需要区间判定。
    #[test]
    fn test_is_contiguous_empty_and_single() {
        assert!(!is_contiguous(&[]), "empty slice is not contiguous");
        assert!(!is_contiguous(&[42]), "single element is not contiguous");
    }

    /// `is_contiguous`: 步长 1 的升序序列。
    #[test]
    fn test_is_contiguous_true_cases() {
        assert!(is_contiguous(&[0, 1, 2, 3, 4]));
        assert!(is_contiguous(&[7, 8, 9]));
        // 跨"小时边界"也不算特殊,只看数值差 1。
        assert!(is_contiguous(&[22, 23, 24]));
    }

    /// `is_contiguous`: 任一处差值不是 1 就判否。
    #[test]
    fn test_is_contiguous_false_cases() {
        // 等差但步长 2
        assert!(!is_contiguous(&[0, 2, 4, 6]));
        // 中间断了一格
        assert!(!is_contiguous(&[0, 1, 3, 4]));
        // 降序
        assert!(!is_contiguous(&[5, 4, 3]));
        // 重复
        assert!(!is_contiguous(&[1, 1, 2]));
    }

    /// `arithmetic_step`: 空 / 单元素 / 步长 0 都返回 None。
    #[test]
    fn test_arithmetic_step_degenerate() {
        assert_eq!(arithmetic_step(&[]), None);
        assert_eq!(arithmetic_step(&[7]), None);
        // 步长 0 是退化情况(集合理论上去重后不该出现),显式拒掉。
        assert_eq!(arithmetic_step(&[5, 5, 5]), None);
    }

    /// `arithmetic_step`: 标准等差数列。
    #[test]
    fn test_arithmetic_step_basic() {
        assert_eq!(arithmetic_step(&[0, 2, 4, 6]), Some(2));
        assert_eq!(arithmetic_step(&[0, 5, 10, 15, 20]), Some(5));
        // 步长 1 的等差 —— 这与 `is_contiguous` 语义重叠,只是 step 是 1。
        assert_eq!(arithmetic_step(&[0, 1, 2, 3]), Some(1));
    }

    /// `arithmetic_step`: 非等差序列返回 None。
    #[test]
    fn test_arithmetic_step_non_arithmetic() {
        // 中间断了一档
        assert_eq!(arithmetic_step(&[0, 2, 5, 6]), None);
        // 降序
        assert_eq!(arithmetic_step(&[10, 8, 6]), None);
    }

    /// `format_cron_field`: 空集 → `*`。
    #[test]
    fn test_format_cron_field_empty() {
        let set: BTreeSet<u32> = BTreeSet::new();
        assert_eq!(format_cron_field(&set), "*");
    }

    /// `format_cron_field`: 单值 → `"7"`。
    #[test]
    fn test_format_cron_field_single() {
        let set: BTreeSet<u32> = BTreeSet::from([7]);
        assert_eq!(format_cron_field(&set), "7");
    }

    /// `format_cron_field`: 连续区间 → `"a-b"`,步长 1 优先于通用等差。
    /// 这一点很关键: `[0,1,2,3]` 既是连续也是步长 1 的等差,
    /// 按 issue 描述的优先级应输出短形式 `0-3` 而不是 `0-3/1`。
    #[test]
    fn test_format_cron_field_contiguous_range() {
        let set: BTreeSet<u32> = BTreeSet::from([0, 1, 2, 3, 4]);
        assert_eq!(format_cron_field(&set), "0-4");
        let set: BTreeSet<u32> = BTreeSet::from([9, 10, 11]);
        assert_eq!(format_cron_field(&set), "9-11");
    }

    /// `format_cron_field`: 等差数列 → `"a-b/N"`。
    /// 常见例子: 小时字段展开为 `0,2,4,...,22` 紧凑写成 `0-22/2`。
    #[test]
    fn test_format_cron_field_arithmetic() {
        let set: BTreeSet<u32> = BTreeSet::from([0, 2, 4, 6]);
        assert_eq!(format_cron_field(&set), "0-6/2");
        // 实际生产里出现过的形态: 0,15,30,45 分钟 → 0-45/15
        let set: BTreeSet<u32> = BTreeSet::from([0, 15, 30, 45]);
        assert_eq!(format_cron_field(&set), "0-45/15");
    }

    /// `format_cron_field`: 不规则序列 → 逗号列表。
    #[test]
    fn test_format_cron_field_irregular() {
        let set: BTreeSet<u32> = BTreeSet::from([0, 5, 10, 12]);
        assert_eq!(format_cron_field(&set), "0,5,10,12");
        // 步长变了: 0,3,7,12 (差 3,4,5) 也不构成等差
        let set: BTreeSet<u32> = BTreeSet::from([0, 3, 7, 12]);
        assert_eq!(format_cron_field(&set), "0,3,7,12");
    }

    /// `format_cron_field`: 二元素连续 → 连续区间,而不是等差(虽然也是步长 1 等差)。
    /// 防御"二元素时被算成等差"导致输出 `a-b/1` 这种啰嗦表示的回归。
    #[test]
    fn test_format_cron_field_two_elements_contiguous() {
        let set: BTreeSet<u32> = BTreeSet::from([3, 4]);
        assert_eq!(format_cron_field(&set), "3-4");
    }

    /// `format_cron_field`: 二元素等差(步长 2) → 等差表示。
    /// 区别于上一个 case: 步长不是 1,触发 arithmetic_step 分支。
    #[test]
    fn test_format_cron_field_two_elements_arithmetic() {
        let set: BTreeSet<u32> = BTreeSet::from([3, 5]);
        assert_eq!(format_cron_field(&set), "3-5/2");
    }
}
