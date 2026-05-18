//! Tests for scheduler module - cron expression validation and task management

#[cfg(test)]
mod scheduler_cron_validation_tests {
    use chrono::Timelike;
    use std::str::FromStr;

    #[test]
    fn test_valid_cron_expressions_for_scheduler() {
        // These are common scheduler cron expressions used in the application
        let expressions = vec![
            // Every 30 seconds (for testing)
            "*/30 * * * * *",
            // Every minute
            "0 * * * * *",
            // Every 5 minutes
            "0 */5 * * * *",
            // Every 15 minutes
            "0 */15 * * * *",
            // Every 30 minutes
            "0 */30 * * * *",
            // Every hour at minute 0
            "0 0 * * * *",
            // Every 2 hours
            "0 0 */2 * * *",
            // Every 6 hours
            "0 0 */6 * * *",
            // Every 12 hours
            "0 */12 * * * *",
            // Daily at 9am
            "0 0 9 * * *",
            // Daily at midnight
            "0 0 0 * * *",
            // Every Monday at 9am
            "0 0 9 * * 1",
            // Every weekday at 9am
            "0 0 9 * * 1-5",
            // Monthly on 1st at 9am
            "0 0 9 1 * *",
            // Twice daily (9am and 6pm)
            "0 0 9,18 * * *",
        ];

        for expr in expressions {
            let result = cron::Schedule::from_str(expr);
            assert!(result.is_ok(), "Cron '{}' should be valid but got: {:?}", expr, result.err());
        }
    }

    #[test]
    fn test_invalid_cron_expressions_rejected() {
        let invalid_expressions = vec![
            // Empty
            "",
            // Too few fields
            "* * * *",
            "* * *",
            // Too many fields
            "0 0 * * * * * *",
            // Invalid characters
            "abc def ghi jkl mno pqr",
            // Invalid second value (>59)
            "60 * * * * *",
            // Invalid minute value (>59)
            "* 60 * * * *",
            // Invalid hour value (>23)
            "* * 25 * * *",
            // Invalid day of month (>31)
            "* * * 32 * *",
            // Invalid month (>12)
            "* * * * 13 *",
            // Invalid day of week (>6) - note: some cron impls allow 7 for Sunday, but cron crate doesn't
            // Completely invalid
            "every day at 9am",
            "not a cron",
        ];

        for expr in invalid_expressions {
            let result = cron::Schedule::from_str(expr);
            assert!(result.is_err(), "Cron '{}' should be invalid", expr);
        }
    }

    #[test]
    fn test_cron_schedule_next_run_calculation() {
        let schedule = cron::Schedule::from_str("*/10 * * * * *").unwrap();
        let now = chrono::Utc::now();
        let next = schedule.upcoming(chrono::Utc).next();

        assert!(next.is_some(), "Should have a next scheduled time");
        let next_time = next.unwrap();

        // Next run should be in the future (within 10 seconds)
        // Allow 0 seconds for boundary conditions (when current time is exactly on a schedule boundary)
        let duration = next_time.signed_duration_since(now);
        assert!(duration.num_seconds() >= 0, "Next run should be in the future");
        assert!(duration.num_seconds() <= 10, "Next run should be within 10 seconds");
    }

    #[test]
    fn test_cron_schedule_multiple_upcoming() {
        let schedule = cron::Schedule::from_str("0 */5 * * * *").unwrap();
        let now = chrono::Utc::now();

        let upcoming: Vec<_> = schedule.upcoming(chrono::Utc).take(5).collect();

        assert_eq!(upcoming.len(), 5, "Should have 5 upcoming runs");

        // Each run should be 5 minutes apart
        for i in 1..upcoming.len() {
            let diff = upcoming[i].signed_duration_since(upcoming[i-1]);
            assert_eq!(diff.num_minutes(), 5, "Each run should be 5 minutes apart");
        }

        // All should be in the future
        for run in &upcoming {
            assert!((*run).signed_duration_since(now).num_seconds() > 0);
        }
    }

    #[test]
    fn test_cron_daily_schedule() {
        let schedule = cron::Schedule::from_str("0 0 9 * * *").unwrap();
        let now = chrono::Utc::now();
        let next = schedule.upcoming(chrono::Utc).next().unwrap();

        // Next run should be at 9am
        assert_eq!(next.hour(), 9);
        assert_eq!(next.minute(), 0);
        assert_eq!(next.second(), 0);

        // Should be in the future
        assert!(next.signed_duration_since(now).num_seconds() > 0);
    }

    #[test]
    fn test_cron_weekday_schedule() {
        // Every weekday at 9am
        let schedule = cron::Schedule::from_str("0 0 9 * * 1-5").unwrap();
        let next = schedule.upcoming(chrono::Utc).next().unwrap();

        // Should be Monday (1) through Friday (5)
        let weekday = next.format("%w").to_string();
        let weekday: u32 = weekday.parse().unwrap();
        assert!(weekday >= 1 && weekday <= 5, "Weekday should be 1-5 (Mon-Fri)");
    }
}

#[cfg(test)]
mod compute_next_run_tests {
    use std::str::FromStr;
    use chrono::Utc;

    fn compute_next_run(cron_expr: &str) -> Option<String> {
        cron::Schedule::from_str(cron_expr)
            .ok()
            .and_then(|schedule| {
                schedule
                    .upcoming(Utc)
                    .next()
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
            })
    }

    #[test]
    fn test_compute_next_run_valid_expression() {
        let result = compute_next_run("*/30 * * * * *");
        assert!(result.is_some());

        let ts = result.unwrap();
        assert!(ts.ends_with('Z'));
        assert!(ts.len() >= 20);
    }

    #[test]
    fn test_compute_next_run_invalid_expression() {
        let result = compute_next_run("invalid");
        assert!(result.is_none());
    }

    #[test]
    fn test_compute_next_run_every_minute() {
        let result = compute_next_run("0 * * * * *");
        assert!(result.is_some());

        let ts = result.unwrap();
        // Should be at second 0
        assert!(ts.contains(":00."), "Should be at second 00");
    }

    #[test]
    fn test_compute_next_run_format_is_rfc3339_like() {
        let result = compute_next_run("0 0 0 * * *");
        assert!(result.is_some());

        let ts = result.unwrap();
        // Format: 2026-05-13T00:00:00.000Z
        // Should match pattern YYYY-MM-DDTHH:MM:SS.000Z
        let parts: Vec<&str> = ts.split(['-', 'T', ':', '.', 'Z']).collect();
        assert!(parts.len() >= 6, "Should have at least 6 parts: {:?}", parts);
    }
}