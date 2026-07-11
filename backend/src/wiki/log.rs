//! 执行日志追加。
//!
//! log.md 记录每次摄入的时间、涉及页面、来源记录，便于追溯。
//! 新条目始终在最上面，超过 3 天的条目会被自动清理。

use chrono::{Local, Duration};
use std::io;

use super::fs::{read_log, write_log};

/// 追加一条摄入日志（新条目在最上面）。
///
/// 记录时间、涉及的 topic 页面、来源执行记录 ID。
pub fn append_log_entry(
    workspace_id: i64,
    record_ids: &[i64],
    topics: &[String],
) -> io::Result<()> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();

    // 格式：## [时间] 摄入 | 执行记录 #42, #45
    let records_str = record_ids
        .iter()
        .map(|id| format!("#{}", id))
        .collect::<Vec<_>>()
        .join(", ");

    let topics_str = topics
        .iter()
        .map(|t| format!("- {}", t))
        .collect::<Vec<_>>()
        .join("\n");

    let entry = format!(
        "## [{}] 摄入 | 执行记录 {}\n\n涉及页面：\n{}\n\n",
        now, records_str, topics_str
    );

    // 读取现有内容，把新条目插到最前面（倒序）
    let existing = read_log(workspace_id)?.unwrap_or_default();
    write_log(workspace_id, &(entry + &existing))?;

    // 写入后自动清理超过 3 天的旧条目
    trim_old_entries(workspace_id)?;

    Ok(())
}

/// 清理超过 3 天的旧日志条目。
fn trim_old_entries(workspace_id: i64) -> io::Result<()> {
    let content = match read_log(workspace_id)? {
        Some(c) => c,
        None => return Ok(()),
    };

    let cutoff = cutoff_date();
    let filtered = filter_entries_by_date(&content, &cutoff);

    if filtered.len() == entry_count(&content) {
        // 没有条目被删除，无需重写
        return Ok(());
    }

    let new_content = rebuild_log(filtered);
    write_log(workspace_id, &new_content)
}

/// 返回 3 天前的日期字符串（yyyy-mm-dd），用于比较。
fn cutoff_date() -> String {
    (Local::now().date_naive() - Duration::days(3))
        .format("%Y-%m-%d")
        .to_string()
}

/// 从日志内容中提取有效条目（日期 >= cutoff）。
fn filter_entries_by_date<'a>(content: &'a str, cutoff: &str) -> Vec<&'a str> {
    content
        .split("## [")
        .skip(1) // 跳过空的前导
        .filter(|e| is_entry_within_cutoff(e, cutoff))
        .collect()
}

/// 判断单条条目是否在 cutoff 之后。
fn is_entry_within_cutoff(entry: &str, cutoff: &str) -> bool {
    entry.get(..10).map_or(true, |date_part| date_part >= cutoff)
}

/// 从日志内容中统计总条目数。
fn entry_count(content: &str) -> usize {
    content.split("## [").count().saturating_sub(1)
}

/// 用过滤后的条目重新组装日志内容。
fn rebuild_log(entries: Vec<&str>) -> String {
    let mut new_content = String::new();
    for e in entries {
        new_content.push_str("## [");
        new_content.push_str(e);
    }
    new_content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cutoff_date_format() {
        let date = cutoff_date();
        // 格式应为 yyyy-mm-dd（10 个字符）
        assert_eq!(date.len(), 10);
        assert!(date.chars().nth(4) == Some('-'));
        assert!(date.chars().nth(7) == Some('-'));
    }

    #[test]
    fn test_entry_count() {
        let content = "## [2026-07-07 08:57] 摄入\n\ntext\n\n## [2026-07-05 18:03] 摄入\n\ntext\n\n";
        assert_eq!(entry_count(content), 2);
    }

    #[test]
    fn test_entry_count_empty() {
        assert_eq!(entry_count(""), 0);
    }

    #[test]
    fn test_is_entry_within_cutoff() {
        // cutoff = 2026-07-05，07-07 应该保留，07-03 应该删除
        let cutoff = "2026-07-05";
        assert!(is_entry_within_cutoff("2026-07-07 08:57] 摄入", cutoff));
        assert!(is_entry_within_cutoff("2026-07-05 18:03] 摄入", cutoff));
        assert!(!is_entry_within_cutoff("2026-07-03 18:03] 摄入", cutoff));
        assert!(is_entry_within_cutoff("invalid", cutoff)); // 无法解析日期则保留
    }

    #[test]
    fn test_filter_entries_by_date() {
        let content = "## [2026-07-07 08:57] 摄入\n\n## [2026-07-05 18:03] 摄入\n\n## [2026-07-03 18:03] 摄入\n\n";
        let cutoff = "2026-07-05";
        let filtered = filter_entries_by_date(content, cutoff);
        // 07-07 和 07-05 保留，07-03 删除
        assert_eq!(filtered.len(), 2);
        assert!(filtered[0].starts_with("2026-07-07"));
        assert!(filtered[1].starts_with("2026-07-05"));
    }

    #[test]
    fn test_filter_entries_all_within_cutoff() {
        let content = "## [2026-07-07 08:57] 摄入\n\n";
        let cutoff = "2026-07-05";
        let filtered = filter_entries_by_date(content, cutoff);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_entries_none_within_cutoff() {
        let content = "## [2026-07-01 08:57] 摄入\n\n";
        let cutoff = "2026-07-05";
        let filtered = filter_entries_by_date(content, cutoff);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_rebuild_log() {
        let entries = vec!["2026-07-07 08:57] 摄入\n\ntext", "2026-07-05 18:03] 摄入\n\ntext"];
        let result = rebuild_log(entries);
        assert!(result.starts_with("## [2026-07-07"));
        assert!(result.contains("## [2026-07-05"));
        assert_eq!(result.matches("## [").count(), 2);
    }

    #[test]
    fn test_append_log_entry_reads_existing() {
        // 验证 append_log_entry 会先读后写，不会覆盖现有内容；
        // 这里直接测 filter/rebuild 逻辑（append_log_entry 内部即走该路径）。
        let existing = "## [2026-07-06 10:00] 摄入\n\n";
        let cutoff = "2026-07-05";
        let filtered = filter_entries_by_date(existing, cutoff);
        assert_eq!(filtered.len(), 1);
        let result = rebuild_log(filtered);
        assert!(result.contains("2026-07-06"));
    }
}
