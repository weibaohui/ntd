//! Tests for scheduler logic - cron expression validation

// 测试代码允许 unwrap/expect/panic 等写法以简化断言逻辑，统一放宽以下 clippy 检查
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
#[cfg(test)]
mod cron_validation_tests {

    #[test]
    fn test_cron_seconds_field() {
        // 6-field cron: second minute hour day month weekday
        // Valid: every 30 seconds
        let result = croner::Cron::new("*/30 * * * * *").with_seconds_required().parse();
        assert!(result.is_ok());

        // Valid: every minute at second 0
        let result = croner::Cron::new("0 * * * * *").with_seconds_required().parse();
        assert!(result.is_ok());

        // Valid: every hour at minute 0
        let result = croner::Cron::new("0 0 * * * *").with_seconds_required().parse();
        assert!(result.is_ok());

        // Valid: daily at 9am
        let result = croner::Cron::new("0 0 9 * * *").with_seconds_required().parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_cron_common_patterns() {
        // Every 5 minutes
        assert!(croner::Cron::new("0 */5 * * * *").with_seconds_required().parse().is_ok());

        // Every 15 minutes
        assert!(croner::Cron::new("0 */15 * * * *").with_seconds_required().parse().is_ok());

        // Every hour
        assert!(croner::Cron::new("0 0 */1 * * *").with_seconds_required().parse().is_ok());

        // Every day at midnight
        assert!(croner::Cron::new("0 0 0 * * *").with_seconds_required().parse().is_ok());

        // Every Monday at 9am
        assert!(croner::Cron::new("0 0 9 * * 1").with_seconds_required().parse().is_ok());
    }

    #[test]
    fn test_cron_invalid_patterns() {
        // Empty
        assert!(croner::Cron::new("").with_seconds_required().parse().is_err());

        // Too few fields
        assert!(croner::Cron::new("* * * *").with_seconds_required().parse().is_err());

        // Invalid characters
        assert!(croner::Cron::new("abc def ghi jkl mno pqr").with_seconds_required().parse().is_err());

        // Out of range values
        assert!(croner::Cron::new("60 * * * * *").with_seconds_required().parse().is_err()); // second > 59
        assert!(croner::Cron::new("* 60 * * * *").with_seconds_required().parse().is_err()); // minute > 59
        assert!(croner::Cron::new("* * 25 * * *").with_seconds_required().parse().is_err()); // hour > 23
    }

    #[test]
    fn test_cron_schedule_next() {
        let cron = croner::Cron::new("*/10 * * * * *").with_seconds_required().parse().unwrap();
        let now = chrono::Utc::now();
        let next = cron.find_next_occurrence(&now, false);

        assert!(next.is_ok());
        let next_time = next.unwrap();
        // Next should be at most 10 seconds in the future
        // Allow 0 seconds for boundary conditions (when current time is exactly on a schedule boundary)
        let duration = next_time.signed_duration_since(now);
        assert!(duration.num_seconds() >= 0 && duration.num_seconds() < 10,
            "next run should be within 10 seconds, got {} seconds", duration.num_seconds());
    }
}