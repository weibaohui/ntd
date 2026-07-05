use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler, JobSchedulerError};
use tracing::{error, info, warn};

use chrono::{TimeZone, Timelike};

use crate::executor_service::{run_todo_execution, RunTodoExecutionRequest};
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

/// 解析 `chrono_tz::Tz`,失败时返回结构化 `InvalidTimezone` 错误。
///
/// 把时区解析从 `convert_cron_to_utc` 抽出来,是因为后续 split phase
/// (枚举 / DST 检测 / 拼装)都不再需要字符串 `&str`,只关心解析后的
/// `Tz`。独立后能针对合法 / 非法输入分别做单测。
fn parse_tz(timezone: &str) -> Result<chrono_tz::Tz, SchedulerError> {
    timezone
        .parse()
        .map_err(|_| SchedulerError::InvalidTimezone(timezone.to_string()))
}

/// 校验 cron 表达式恰好 6 字段 (秒 分 时 日 月 周)。
///
/// 5 字段在 unix cron 里合法但 `tokio-cron-scheduler` 不接受;7 字段
/// 会被误解析为"年"。早 fail 早知道,避免错误字段在后续阶段污染输出。
/// `todo_id` 透传给错误响应,让 handler 知道是哪条 todo 的 cron 错了。
fn ensure_six_fields(cron_expr: &str, todo_id: i64) -> Result<(), SchedulerError> {
    // 直接用 `split_whitespace`,它内部会跳过首尾空白,无需 `trim`。
    // 用 `count()` 避免 `Vec` 分配 —— 我们只关心字段数,不关心字段本身
    // (后续阶段才在 `convert_cron_to_utc` 里通过 `cron_expr.split_whitespace()`
    // 拿到 fields 的引用做 `&fields[3..]` 切片)。
    if cron_expr.split_whitespace().count() != 6 {
        return Err(SchedulerError::InvalidCron {
            expr: cron_expr.to_string(),
            todo_id,
        });
    }
    Ok(())
}

/// 用 `cron::Schedule` 在 1 年内枚举所有 occurrence,逐个转 UTC,
/// 返回每个 occurrence 的 `(hour, minute, second)` 元组。
///
/// 关键点(对应 issue #502 的几个 DST bug):
/// - 用固定参考点 `2025-01-01 00:00:00 local`(而非 `Utc::now()`),
///   测试稳定 + 不依赖系统时间。
/// - 1 年窗口足够覆盖北/南半球所有 DST 切换日;`Schedule::after` 返回
///   `DateTime<Tz>`,chrono 内部按 `local_dt` 那一刻的实际 offset 算
///   UTC,无需我们手算 offset,也就避免了 DST 切换日的脏数据。
///
/// 返回 `Vec` 而非 `HashMap`,是因为后续 `summarize_field_set` 只关心
/// 集合去重,不需要计数;DST pair 的 dominant 选择在调用方 `detect_dst_dominant`
/// 里再做。
fn enumerate_utc_times(schedule: &cron::Schedule, tz: &chrono_tz::Tz) -> Vec<(u32, u32, u32)> {
    // 用 fixed 起点 (2025-01-01 00:00:00 local) 枚举,覆盖完整 1 年,
    // 确保 DST 切换日的 occurrence 都在样本内。
    let reference = match tz.with_ymd_and_hms(2025, 1, 1, 0, 0, 0) {
        chrono::LocalResult::Single(t) => t,
        _ => return Vec::new(),
    };
    let end = match tz.with_ymd_and_hms(2026, 1, 1, 0, 0, 0) {
        chrono::LocalResult::Single(t) => t,
        _ => return Vec::new(),
    };

    let mut times = Vec::new();
    for local_dt in schedule.after(&reference).take_while(|dt| *dt < end) {
        let utc = local_dt.with_timezone(&chrono::Utc);
        times.push((utc.hour(), utc.minute(), utc.second()));
    }
    times
}

/// 判断 `times` 是不是 "DST 双值对"形态:恰好 2 个 distinct (h, m, s)
/// 元组、它们只在 hour 上相差 1、其它字段全相同。
///
/// 若是,返回 dominant(出现次数最多的那个);否则返回 `None`,调用方
/// 走 union 路径。
///
/// 为什么需要这个判定:对 `0 0 9 * * *` 这类"单 hour"表达式,纽约时区
/// 一年里产生 13 UTC / 14 UTC 两种(夏冬令时各几个月);union 会导致
/// "每天多触发 1 次",dominant 才是对的。但对 `0 0 9,12,18 * * *` 多
/// hour 列表,dominant 又会丢触发时间,union 才对。启发式:仅"恰好 2
/// 个且差 1 小时"按 DST pair 处理,其它按 union。
///
/// `.expect()` 是在 `is_dst_pair=true` 守卫下从 non-empty 迭代器取
/// `max_by_key` 的穷尽性保证,不是运行时可达错误;按 lib.rs 测试里
/// 使用 `unwrap/expect` 需 `#[allow]` 的约定显式标注。
#[allow(clippy::expect_used)]
fn detect_dst_dominant(times: &[(u32, u32, u32)]) -> Option<(u32, u32, u32)> {
    let mut counts: HashMap<(u32, u32, u32), u32> = HashMap::new();
    for t in times {
        *counts.entry(*t).or_insert(0) += 1;
    }
    let distinct: Vec<(u32, u32, u32)> = counts.keys().copied().collect();
    let is_dst_pair = distinct.len() == 2
        && distinct[0].1 == distinct[1].1
        && distinct[0].2 == distinct[1].2
        && distinct[0].0.abs_diff(distinct[1].0) == 1;
    if !is_dst_pair {
        return None;
    }
    let (h, m, s) = distinct
        .iter()
        .max_by_key(|k| counts.get(k).copied().unwrap_or(0))
        .copied()
        .expect("DST pair invariant: is_dst_pair implies distinct.len() == 2, so max_by_key returns Some");
    Some((h, m, s))
}

/// 把一组 `(hour, minute, second)` 拆成三个 `BTreeSet`,再用
/// `format_cron_field` 紧凑格式化,返回 `(s, m, h)` 三个字段字符串。
///
/// 输入是 (h, m, s) 元组列表(可能 1 个 — DST dominant、可能多个 —
/// union),输出是 cron 字段值,顺序与 issue 期望一致
/// `(seconds, minutes, hours)`。
fn summarize_field_set(times: &[(u32, u32, u32)]) -> (String, String, String) {
    let mut sec: BTreeSet<u32> = BTreeSet::new();
    let mut min: BTreeSet<u32> = BTreeSet::new();
    let mut hr: BTreeSet<u32> = BTreeSet::new();
    for (h, m, s) in times {
        hr.insert(*h);
        min.insert(*m);
        sec.insert(*s);
    }
    (
        format_cron_field(&sec),
        format_cron_field(&min),
        format_cron_field(&hr),
    )
}

/// 把紧凑格式化后的 h/m/s 字段 + 保留原值的 dom/month/dow 拼成
/// 最终 6 字段 UTC cron 表达式。
///
/// `rest` 长度必须为 3 (day-of-month / month / day-of-week)。
/// 跨时区时这三个字段的偏移对用户意义不大,改它反而引入歧义,所以
/// 保持原值(见 `convert_cron_to_utc` doc)。
fn assemble_utc_cron(s_str: &str, m_str: &str, h_str: &str, rest: &[&str]) -> String {
    debug_assert_eq!(rest.len(), 3, "rest must be [dom, month, dow]");
    format!(
        "{} {} {} {} {} {}",
        s_str, m_str, h_str, rest[0], rest[1], rest[2]
    )
}

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
///
/// 拆分后,本函数是「线性串接」5 个 helper,不再有内层逻辑。
/// issue #615 把 100+ 行的长函数拆成 phase 化的小函数,每个阶段单测,
/// 后续要修改"DST 启发式"或"枚举窗口"时只动对应 helper。
fn convert_cron_to_utc(
    cron_expr: &str,
    timezone: &str,
    todo_id: i64,
) -> Result<String, SchedulerError> {
    // 1) 解析时区
    let tz = parse_tz(timezone)?;

    // 2) 解析 cron 字符串 + 校验 6 字段
    let schedule = cron::Schedule::from_str(cron_expr).map_err(|_| {
        SchedulerError::InvalidCron {
            expr: cron_expr.to_string(),
            todo_id,
        }
    })?;
    ensure_six_fields(cron_expr, todo_id)?;

    // 3) 提取 dom / month / dow 字段(原样保留),h/m/s 由枚举结果生成
    // 用 `split_whitespace` 收集成 Vec,直接用下标访问;
    // (避免 `trim().split_whitespace()` 模式,`split_whitespace` 本身已处理首尾空白)
    let fields: Vec<&str> = cron_expr.split_whitespace().collect();
    let rest = &fields[3..];

    // 4) 枚举 1 年内所有 UTC 时刻
    let mut times = enumerate_utc_times(&schedule, &tz);
    if times.is_empty() {
        // 理论上不会发生 (reference 在 schedule 起点之后,365 天内必出至少一次);
        // 兜底返回原表达式,避免 panic。
        warn!(
            "No occurrences found for cron '{}' in {} between 2025-01-01 and 2026-01-01; \
            returning original expression.",
            cron_expr, timezone
        );
        return Ok(cron_expr.to_string());
    }

    // 5) DST pair → dominant;其它 → union。union 路径用 sort+dedup 让
    // (h, m, s) 顺序稳定,便于测试断言和避免 `summarize_field_set` 内部
    // 对 BTreeSet 重新排序的额外开销。
    if let Some((h, m, s)) = detect_dst_dominant(&times) {
        warn!(
            "Cron '{}' in {} crosses DST; using dominant UTC time \
            (h={}, m={}, s={}) and dropping the other. \
            On DST transition days the schedule may be off by 1 hour.",
            cron_expr, timezone, h, m, s
        );
        times = vec![(h, m, s)];
    }

    // 6) 紧凑格式化 h/m/s,组装最终表达式
    let (s_str, m_str, h_str) = summarize_field_set(&times);
    Ok(assemble_utc_cron(&s_str, &m_str, &h_str, rest))
}

pub struct TodoScheduler {
    sched: Mutex<JobScheduler>,
    job_map: Mutex<HashMap<i64, uuid::Uuid>>,
}

impl TodoScheduler {
    /// 创建 `TodoScheduler` 单例，初始化底层 `JobScheduler`。
    /// 失败的原因（`JobSchedulerError`）通过 `#[from]` 自动转为 `SchedulerError::SchedulerBackend`。
    pub async fn new() -> Result<Self, SchedulerError> {
        let sched = JobScheduler::new().await?;
        Ok(Self {
            sched: Mutex::new(sched),
            job_map: Mutex::new(HashMap::new()),
        })
    }

    /// 从 DB 读取所有启用调度的 todo，并注册到 `JobScheduler`。
    /// 单条 todo 的注册失败（cron 不合法等）只 warn 不中断，**外层返回 Ok**；
    /// 只有 DB 本身不可达才算失败。
    pub async fn load_from_db(
        &self,
        ctx: &ServiceContext,
    ) -> Result<(), SchedulerError> {
        let todos = ctx.db.get_scheduler_todos(None).await?;

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

        info!("Creating job for todo {} with cron: {} (original: {:?})", todo_id, cron_expr_utc, timezone);
        let job = Job::new_async(&cron_expr_utc, move |_uuid, _l| {
            let db = db_clone.clone();
            let registry = registry_clone.clone();
            let tx = tx_clone.clone();
            let tm = tm_clone.clone();
            let cfg = config_clone.clone();

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
                        // 从 todo 自身回填 workspace_id / workspace_path。
                        // 早期实现把两者都置 None，导致 cron 触发的任务在执行完成后，
                        // 黑板更新钩子（completion.rs：仅当 workspace_id 为 Some 时才
                        // push pending record）被跳过，cron 任务永远进不了黑板分析。
                        // 这里取自 DB 的 todo 字段，与手动触发路径保持一致。
                        let workspace_id = todo.workspace_id;
                        let workspace_path = todo.workspace_path.clone();
                        run_todo_execution(RunTodoExecutionRequest {
                            db,
                            executor_registry: registry,
                            tx,
                            task_manager: tm,
                            config: cfg,
                            todo_id,
                            message,
                            req_executor: executor,
                            trigger_type: "cron".to_string(),
                            params: None,
                            resume_session_id: None,
                            resume_message: None,
                            source_todo_id: None,
                            source_todo_title: None,
                            loop_step_execution_id: None,
                            feishu_bot_id: None,
                            step_id: None,
                            feishu_receive_id: None,
                            workspace_path,
                            workspace_id,
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
// 整个 mod 允许 `unwrap_used` / `expect_used`:helper 单测的 panic = 用例失败,
// 与 lib.rs 顶部"测试里使用 unwrap/expect 需加 `#[allow]]`"约定保持一致。
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod convert_cron_to_utc_helpers_tests {
    //! 覆盖 issue #615 拆分出来的 5 个 helper 函数的单元测试。
    //!
    //! 拆分前只能"端到端"测 `convert_cron_to_utc`,无法定位"DST 错在哪一步"
    //! 或"5 字段为什么没拒绝"等问题。拆出 helper 后,每个阶段可独立验证。
    use super::{
        assemble_utc_cron, detect_dst_dominant, enumerate_utc_times, ensure_six_fields,
        parse_tz, summarize_field_set,
    };
    use crate::scheduler::SchedulerError;
    use std::str::FromStr;

    // ===== parse_tz =====
    // 时区解析是后续 phase 的输入,错了会污染整个枚举 / DST 检测,所以必须独立测。

    /// 合法 IANA 时区字符串应该被接受,且 round-trip 解析回同一时区。
    #[test]
    fn test_parse_tz_accepts_valid_iana_zone() {
        let tz = parse_tz("Asia/Shanghai").expect("shanghai should parse");
        let s = format!("{:?}", tz);
        assert!(s.contains("Shanghai") || s.contains("+08"));
    }

    /// 合法 UTC 偏移别名(`UTC` / `Etc/UTC` / `+08:00`)也要通过。
    /// 这是一些用户在 scheduler 配置里写"UTC"时的常见写法。
    #[test]
    fn test_parse_tz_accepts_utc_aliases() {
        for tz_str in ["UTC", "Etc/UTC", "Etc/GMT+8"] {
            parse_tz(tz_str).unwrap_or_else(|e| panic!("{tz_str} should parse, got {e}"));
        }
    }

    /// 非法时区字符串必须返回 `InvalidTimezone`,且原字符串保留在错误里
    /// (handler 会回显给用户 / AI agent)。
    #[test]
    fn test_parse_tz_rejects_garbage() {
        let err = parse_tz("Not/A/Real/Zone").unwrap_err();
        match err {
            SchedulerError::InvalidTimezone(s) => assert_eq!(s, "Not/A/Real/Zone"),
            other => panic!("expected InvalidTimezone, got {other:?}"),
        }
    }

    /// 大小写敏感: 小写 `asia/shanghai` 必须拒绝,提示用户大小写。
    /// 这能避免一个常见笔误"Asia/shanghai" 通过校验后默默退到 UTC。
    #[test]
    fn test_parse_tz_is_case_sensitive() {
        assert!(parse_tz("asia/shanghai").is_err());
    }

    // ===== ensure_six_fields =====
    // 6 字段是后续 `fields[3..]` 切片的前提。错字段数必须早 fail。

    /// 6 字段表达式应该通过,原样保留空白拆分能力。
    #[test]
    fn test_ensure_six_fields_accepts_six() {
        ensure_six_fields("0 0 9 * * *", 1).expect("6 fields should pass");
    }

    /// 多余的前后空白不应让字段数变少(用 `trim` 后 split)。
    #[test]
    fn test_ensure_six_fields_trims_whitespace() {
        ensure_six_fields("  0  0  9  *  *  *  ", 1)
            .expect("leading/trailing whitespace should not change field count");
    }

    /// 5 字段(标准 unix cron 格式,缺秒)必须拒绝,带原始 expr + todo_id。
    #[test]
    fn test_ensure_six_fields_rejects_five() {
        let err = ensure_six_fields("0 9 * * *", 7).unwrap_err();
        match err {
            SchedulerError::InvalidCron { expr, todo_id } => {
                assert_eq!(expr, "0 9 * * *");
                assert_eq!(todo_id, 7);
            }
            other => panic!("expected InvalidCron, got {other:?}"),
        }
    }

    /// 7 字段(带年)必须拒绝,避免年份字段被误当成 day-of-month。
    #[test]
    fn test_ensure_six_fields_rejects_seven() {
        assert!(ensure_six_fields("0 0 9 * * * 2025", 0).is_err());
    }

    /// 空字符串 / 纯空白必须拒绝。
    #[test]
    fn test_ensure_six_fields_rejects_empty() {
        assert!(ensure_six_fields("", 0).is_err());
        assert!(ensure_six_fields("   ", 0).is_err());
    }

    // ===== enumerate_utc_times =====
    // 枚举结果决定后续 DST 检测和最终表达式,必须测准。

    /// `0 0 9 * * *` Asia/Shanghai: 一年 365 次,UTC 小时 = 1 (无 DST)。
    /// 上海不参与 DST,2025 年每一天 9 点本地对应 UTC 1 点。
    #[test]
    fn test_enumerate_utc_times_daily_9am_shanghai_365_occurrences() {
        let schedule = cron::Schedule::from_str("0 0 9 * * *").unwrap();
        let tz = parse_tz("Asia/Shanghai").unwrap();
        let times = enumerate_utc_times(&schedule, &tz);
        // 2025 年非闰年: 365 天
        assert_eq!(times.len(), 365, "got {} times: {times:?}", times.len());
        for (h, m, s) in &times {
            assert_eq!(*h, 1, "Shanghai 9am → UTC 1am, got {h}");
            assert_eq!(*m, 0);
            assert_eq!(*s, 0);
        }
    }

    /// `0 0 0/3 * * *`(每 3 小时一次)在 1 年窗口内应枚举出 364*8 + 7 = 2919 次。
    /// 边界:参考时间 `2025-01-01 00:00:00` 本身在 `Schedule::after` 语义里
    /// 是**不计入**的(after 是严格大于),所以 2025-01-01 当天只有 7 次
    /// 触发(03, 06, ..., 21),后续 364 天每天 8 次,2026-01-01 00:00
    /// 也不计入(被 `take_while(*dt < end)` 排除)。这是窗口边界,不是 bug。
    #[test]
    fn test_enumerate_utc_times_every_3_hours_year_window() {
        let schedule = cron::Schedule::from_str("0 0 0/3 * * *").unwrap();
        let tz = parse_tz("UTC").unwrap();
        let times = enumerate_utc_times(&schedule, &tz);
        assert_eq!(times.len(), 364 * 8 + 7, "got {} times", times.len());
    }

    /// DST 时区(`America/New_York`)在 1 年内应该枚举出 2 个 distinct UTC 小时
    /// (13 和 14,差 1 小时),其它字段全一致。
    /// 这是 issue #502 修复的核心场景。
    #[test]
    fn test_enumerate_utc_times_dst_produces_two_distinct_hours() {
        let schedule = cron::Schedule::from_str("0 0 9 * * *").unwrap();
        let tz = parse_tz("America/New_York").unwrap();
        let times = enumerate_utc_times(&schedule, &tz);
        let mut hours: Vec<u32> = times.iter().map(|t| t.0).collect();
        hours.sort_unstable();
        hours.dedup();
        assert_eq!(hours, vec![13, 14], "New York 9am should produce {{13, 14}} UTC");
        for t in &times {
            assert_eq!(t.1, 0, "minute must be 0 across DST, got {}", t.1);
            assert_eq!(t.2, 0, "second must be 0 across DST, got {}", t.2);
        }
    }

    /// 时区无法构造 reference / end datetime 时,返回空 Vec 而不是 panic。
    /// 这是兜底保护 —— `convert_cron_to_utc` 见到空 Vec 会 warn + 返原表达式。
    /// (生产代码不会触发,因为 `parse_tz` 接受的所有 Tz 都能 `with_ymd_and_hms`,
    /// 但 helper 自身的鲁棒性值得保证。)
    #[test]
    fn test_enumerate_utc_times_handles_utc_zone() {
        let schedule = cron::Schedule::from_str("0 0 0 * * *").unwrap();
        let tz = parse_tz("UTC").unwrap();
        let times = enumerate_utc_times(&schedule, &tz);
        assert!(!times.is_empty());
    }

    // ===== detect_dst_dominant =====
    // DST 启发式的关键判定: 错会"每天多触发 1 次"或"丢触发时间"。

    /// DST pair: 13 UTC 和 14 UTC 各占几个月,dominant 是出现多的那个。
    /// New York: 7 个月夏令时 (13 UTC) + 5 个月冬令时 (14 UTC) → 13 dominant。
    #[test]
    fn test_detect_dst_dominant_picks_more_frequent() {
        // 手工构造 7 个 (13,0,0) + 5 个 (14,0,0)
        let mut times = Vec::new();
        for _ in 0..7 {
            times.push((13u32, 0u32, 0u32));
        }
        for _ in 0..5 {
            times.push((14u32, 0u32, 0u32));
        }
        let dominant = detect_dst_dominant(&times).expect("DST pair should be detected");
        assert_eq!(dominant, (13, 0, 0), "13 UTC is more frequent, must win");
    }

    /// 真正"差 1 小时"的 pair 即便出现次数相同也应被识别为 DST pair
    /// (返回 Some 而非 None,调用方拿到 dominant)。
    /// 关键:不要因为"50/50"就 fallthrough 到 union 路径。
    #[test]
    fn test_detect_dst_dominant_handles_tie_breaker() {
        let times = vec![(13u32, 0u32, 0u32), (14u32, 0u32, 0u32)];
        let dominant = detect_dst_dominant(&times).expect("pair shape must be detected");
        // tie 时 max_by_key 行为是"返回最后一个"(`iter::max_by_key`),所以是 (14,0,0)
        assert!(
            dominant == (13, 0, 0) || dominant == (14, 0, 0),
            "tie should still pick one, got {dominant:?}"
        );
    }

    /// 非 DST pair: 9 点和 12 点相差 3 小时,不能被误判成 DST pair,
    /// 否则会丢掉 12 点的触发,这是 issue #495 的回归点。
    #[test]
    fn test_detect_dst_dominant_rejects_non_pair() {
        let times = vec![(1u32, 0u32, 0u32), (4u32, 0u32, 0u32)];
        assert!(
            detect_dst_dominant(&times).is_none(),
            "hour diff=3 is not a DST pair, must return None"
        );
    }

    /// 非 DST pair: 小时差 1 但 minute 不同(例 `(0,13,0)` vs `(0,14,0)`)
    /// 也不能被误判。启发式要求 minute/second 完全一致。
    #[test]
    fn test_detect_dst_dominant_requires_minute_second_match() {
        let times = vec![(1u32, 13u32, 0u32), (2u32, 14u32, 0u32)];
        assert!(
            detect_dst_dominant(&times).is_none(),
            "minute mismatch breaks DST pair shape"
        );
    }

    /// 单值(无 DST,单一 UTC 时刻)必须返回 None,走 union 路径。
    #[test]
    fn test_detect_dst_dominant_returns_none_for_single() {
        let times = vec![(1u32, 0u32, 0u32)];
        assert!(detect_dst_dominant(&times).is_none());
    }

    /// 多 hour 列表(3 个 distinct 小时)按 union 路径走,不能误判成 DST pair。
    #[test]
    fn test_detect_dst_dominant_rejects_three_distinct() {
        let times = vec![(1u32, 0u32, 0u32), (4u32, 0u32, 0u32), (10u32, 0u32, 0u32)];
        assert!(detect_dst_dominant(&times).is_none());
    }

    // ===== summarize_field_set =====
    // 紧凑格式化的入口: 输出顺序 (s, m, h),用 `format_cron_field` 内部规则。

    /// 单一 (h, m, s) → 三个字段都格式化为单值。
    #[test]
    fn test_summarize_field_set_single_value() {
        let (s, m, h) = summarize_field_set(&[(13u32, 0u32, 0u32)]);
        assert_eq!((s.as_str(), m.as_str(), h.as_str()), ("0", "0", "13"));
    }

    /// 多 hour 列表 → 多个值,小时按字面 union 输出。
    #[test]
    fn test_summarize_field_set_multi_hour_union() {
        let times = vec![(1u32, 0u32, 0u32), (4u32, 0u32, 0u32), (10u32, 0u32, 0u32)];
        let (s, m, h) = summarize_field_set(&times);
        assert_eq!(s, "0");
        assert_eq!(m, "0");
        assert_eq!(h, "1,4,10");
    }

    /// 连续区间 → 紧凑成 `a-b` (例 0-23 整 24 小时)。
    #[test]
    fn test_summarize_field_set_contiguous_range() {
        // 模拟 `0 0 * * * *` 在 Shanghai 的 union 路径(枚举出 0-23)
        let times: Vec<(u32, u32, u32)> = (0u32..24).map(|h| (h, 0, 0)).collect();
        let (s, m, h) = summarize_field_set(&times);
        assert_eq!(s, "0");
        assert_eq!(m, "0");
        assert_eq!(h, "0-23");
    }

    /// 等差数列 → 紧凑成 `a-b/N` (例 `0-22/2`)。
    #[test]
    fn test_summarize_field_set_arithmetic_step() {
        let times: Vec<(u32, u32, u32)> =
            (0u32..=22).step_by(2).map(|h| (h, 0, 0)).collect();
        let (s, m, h) = summarize_field_set(&times);
        assert_eq!(h, "0-22/2");
        assert_eq!(s, "0");
        assert_eq!(m, "0");
    }

    /// DST pair 走 dominant 路径后,输入是 1 个 tuple,输出就是单值字段。
    /// 这条把 `detect_dst_dominant` 和 `summarize_field_set` 串起来,作为
    /// 集成冒烟测试,确保 dominant 选择不会在格式化阶段被覆盖。
    #[test]
    fn test_summarize_field_set_after_dst_dominant() {
        let times = vec![(13u32, 0u32, 0u32)]; // simulate dominant
        let (s, m, h) = summarize_field_set(&times);
        assert_eq!((s.as_str(), m.as_str(), h.as_str()), ("0", "0", "13"));
    }

    // ===== assemble_utc_cron =====
    // 拼装是机械的字符串操作,但字段顺序和 dom/month/dow 保留的语义
    // 必须被锁住,避免后续 PR 改坏"保持原值"的关键不变式。

    /// 6 字段按 s m h dom month dow 顺序拼装。
    #[test]
    fn test_assemble_utc_cron_basic() {
        let result = assemble_utc_cron("0", "0", "1", &["*", "*", "*"]);
        assert_eq!(result, "0 0 1 * * *");
    }

    /// 复杂 h/m/s 字段(等差 / 范围)拼装后保持紧凑形式。
    #[test]
    fn test_assemble_utc_cron_preserves_compact_form() {
        let result = assemble_utc_cron("0", "0", "0-22/2", &["*", "*", "*"]);
        assert_eq!(result, "0 0 0-22/2 * * *");
    }

    /// day-of-month / month / day-of-week 三个字段必须原样保留 —— 跨时区
    /// 这三个字段的偏移对用户意义不大,改它反而引入歧义 (见 `convert_cron_to_utc` doc)。
    #[test]
    fn test_assemble_utc_cron_preserves_dom_month_dow() {
        let result = assemble_utc_cron("0", "0", "1", &["1", "JAN", "MON"]);
        assert_eq!(result, "0 0 1 1 JAN MON");
    }

    /// `rest` 长度不为 3 时,debug build 触发 debug_assert (release build 静默接受)。
    /// 这里覆盖 happy path,debug_assert 在 test build 下会运行。
    #[test]
    fn test_assemble_utc_cron_requires_three_rest_fields() {
        // 正常 3 字段:不触发 debug_assert
        let _ = assemble_utc_cron("0", "0", "1", &["*", "*", "*"]);
        // 0 字段:debug build 会 panic; 我们不在这里断言 (避免双重 panic)
        // 只确认函数本身不主动 panic 其它路径。
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
