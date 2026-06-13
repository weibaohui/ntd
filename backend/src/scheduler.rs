use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info, warn};

use chrono::TimeZone;
use chrono::Offset;

use crate::executor_service::{run_todo_execution, RunTodoExecutionRequest};
use crate::service_context::ServiceContext;

/// Convert a cron expression from user timezone to UTC timezone.
/// This is necessary because tokio-cron-scheduler always executes in UTC.
/// For example, if user is in Asia/Shanghai (UTC+8) and wants 9:00 local time,
/// we need to schedule UTC 1:00 (9:00 - 8 hours = 1:00).
///
/// Returns (utc_cron_expr, original_timezone) on success.
fn convert_cron_to_utc(cron_expr: &str, timezone: &str) -> Result<String, String> {
    // Parse timezone
    let tz: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| format!("Invalid timezone: {}", timezone))?;

    // Validate cron expression format
    let _ = cron::Schedule::from_str(cron_expr)
        .map_err(|_| format!("Invalid cron expression: {}", cron_expr))?;

    // Get the cron fields (seconds, minute, hour, day, month, weekday)
    // cron crate uses: seconds minute hour day-of-month month day-of-week
    let fields = cron_expr.trim().split_whitespace().collect::<Vec<_>>();
    if fields.len() != 6 {
        return Err(format!(
            "Cron expression must have 6 fields, got {}",
            fields.len()
        ));
    }

    let seconds = fields[0];
    let minutes = fields[1];
    let hours = fields[2];
    let day_of_month = fields[3];
    let month = fields[4];
    let day_of_week = fields[5];

    // Check if hours field contains a wildcard or specific values
    // If hours is a wildcard (*), we don't need to convert
    if hours == "*" {
        return Ok(cron_expr.to_string());
    }

    // For specific hour values, we need to convert to UTC
    // Parse the hour value(s) and calculate UTC offset
    let now = chrono::Utc::now();
    let offset_secs = tz.offset_from_utc_datetime(&now.naive_utc()).fix().local_minus_utc();
    let offset_hours = offset_secs / 3600;

    // Convert hour values from user timezone to UTC
    let convert_hour = |h: i32| -> i32 {
        let mut utc_hour = h - offset_hours;
        if utc_hour < 0 {
            utc_hour += 24;
        } else if utc_hour >= 24 {
            utc_hour -= 24;
        }
        utc_hour
    };

    // Handle specific hour values
    if let Ok(hour_val) = hours.parse::<i32>() {
        let utc_hour = convert_hour(hour_val);
        return Ok(format!(
            "{} {} {} {} {} {}",
            seconds, minutes, utc_hour, day_of_month, month, day_of_week
        ));
    }

    // Handle ranges like "9-17"
    if hours.contains('-') {
        let parts: Vec<&str> = hours.split('-').collect();
        if parts.len() == 2 {
            if let (Ok(start), Ok(end)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                let utc_start = convert_hour(start);
                let utc_end = convert_hour(end);
                // Hour field can contain range like "9-17", so pass as single string "{}"
                return Ok(format!(
                    "{} {} {}-{} {} {} {}",
                    seconds, minutes, utc_start, utc_end, day_of_month, month, day_of_week
                ));
            }
        }
    }

    // Handle step values like "*/2" or "0-23/2"
    if hours.contains('/') {
        // For step values, we can't easily convert, so just return as-is with a warning
        warn!(
            "Hour step expression '{}' may not correctly account for timezone. Consider using specific hours.",
            hours
        );
        return Ok(cron_expr.to_string());
    }

    // Handle lists like "9,12,18"
    if hours.contains(',') {
        let hour_list: Result<Vec<i32>, _> = hours
            .split(',')
            .map(|h| h.parse::<i32>())
            .collect();
        if let Ok(list) = hour_list {
            let utc_list: Vec<i32> = list.iter().map(|&h| convert_hour(h)).collect();
            return Ok(format!(
                "{} {} {} {} {} {}",
                seconds,
                minutes,
                utc_list.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(","),
                day_of_month,
                month,
                day_of_week
            ));
        }
    }

    Ok(cron_expr.to_string())
}

pub struct TodoScheduler {
    sched: Mutex<JobScheduler>,
    job_map: Mutex<HashMap<i64, uuid::Uuid>>,
}

impl TodoScheduler {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let sched = JobScheduler::new().await?;
        Ok(Self {
            sched: Mutex::new(sched),
            job_map: Mutex::new(HashMap::new()),
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

    /// hour 字段是 * 时不应该做时区转换 —— 通配符代表"每小时",不论
    /// 在哪个时区,UTC 的"每小时"都还是每小时。这是冷门但重要的 case,
    /// 之前实现里对 * 也走 convert_hour 路径会得到 8-32 这种废值。
    #[test]
    fn test_wildcard_hour_passes_through_unchanged() {
        let utc = convert_cron_to_utc("0 0 * * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0 0 * * * *");
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

    /// 步长表达式 "*/2" 当前实现是直接 passthrough(配 warn log),
    /// 因为步长跨时区没有简单等价。测试"它不会 panic"是稳定性的最低保障。
    #[test]
    fn test_step_expression_passes_through() {
        let utc = convert_cron_to_utc("0 0 */2 * * *", "Asia/Shanghai").unwrap();
        assert_eq!(utc, "0 0 */2 * * *");
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
}
