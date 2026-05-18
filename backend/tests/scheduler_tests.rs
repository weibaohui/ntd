//! Tests for scheduler logic - cron expression validation

#[cfg(test)]
mod cron_validation_tests {
    use std::str::FromStr;

    #[test]
    fn test_cron_seconds_field() {
        // 6-field cron: second minute hour day month weekday
        // Valid: every 30 seconds
        let result = cron::Schedule::from_str("*/30 * * * * *");
        assert!(result.is_ok());

        // Valid: every minute at second 0
        let result = cron::Schedule::from_str("0 * * * * *");
        assert!(result.is_ok());

        // Valid: every hour at minute 0
        let result = cron::Schedule::from_str("0 0 * * * *");
        assert!(result.is_ok());

        // Valid: daily at 9am
        let result = cron::Schedule::from_str("0 0 9 * * *");
        assert!(result.is_ok());
    }

    #[test]
    fn test_cron_common_patterns() {
        // Every 5 minutes
        assert!(cron::Schedule::from_str("0 */5 * * * *").is_ok());

        // Every 15 minutes
        assert!(cron::Schedule::from_str("0 */15 * * * *").is_ok());

        // Every hour
        assert!(cron::Schedule::from_str("0 0 */1 * * *").is_ok());

        // Every day at midnight
        assert!(cron::Schedule::from_str("0 0 0 * * *").is_ok());

        // Every Monday at 9am
        assert!(cron::Schedule::from_str("0 0 9 * * 1").is_ok());
    }

    #[test]
    fn test_cron_invalid_patterns() {
        // Empty
        assert!(cron::Schedule::from_str("").is_err());

        // Too few fields
        assert!(cron::Schedule::from_str("* * * *").is_err());

        // Invalid characters
        assert!(cron::Schedule::from_str("abc def ghi jkl mno pqr").is_err());

        // Out of range values
        assert!(cron::Schedule::from_str("60 * * * * *").is_err()); // second > 59
        assert!(cron::Schedule::from_str("* 60 * * * *").is_err()); // minute > 59
        assert!(cron::Schedule::from_str("* * 25 * * *").is_err()); // hour > 23
    }

    #[test]
    fn test_cron_schedule_next() {
        use std::str::FromStr;

        let schedule = cron::Schedule::from_str("*/10 * * * * *").unwrap();
        let now = chrono::Utc::now();
        let next = schedule.upcoming(chrono::Utc).next();

        assert!(next.is_some());
        let next_time = next.unwrap();
        // Next should be at most 10 seconds in the future
        // Allow 0 seconds for boundary conditions (when current time is exactly on a schedule boundary)
        let duration = next_time.signed_duration_since(now);
        assert!(duration.num_seconds() >= 0 && duration.num_seconds() < 10,
            "next run should be within 10 seconds, got {} seconds", duration.num_seconds());
    }
}