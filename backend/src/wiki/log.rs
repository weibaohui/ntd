//! 执行日志追加。
//!
//! log.md 记录每次摄入的时间、涉及页面、来源记录，便于追溯。
//! 采用追加模式，永不修改旧条目。超过 3 天的条目会被自动清理。

use chrono::{Local, NaiveDate, Duration};
use std::io;

use super::fs::{append_log, log_file, read_log, write_log};

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
    write_log(workspace_id, &(entry.clone() + &existing))?;

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

    let cutoff = (Local::now().date_naive() - Duration::days(3))
        .format("%Y-%m-%d")
        .to_string();

    // 按 ## [ 分割条目，第一段是空的前导内容
    let entries: Vec<&str> = content.split("## [").collect();
    let filtered: Vec<&str> = entries
        .iter()
        .skip(1) // 跳过空的前导
        .filter(|e| {
            // 每个条目格式：yyyy-mm-dd HH:MM] ...
            if let Some(date_part) = e.get(..10) {
                date_part >= cutoff.as_str()
            } else {
                true // 无法解析日期则保留
            }
        })
        .copied() // &&str -> &str
        .collect();

    if filtered.len() == entries.len() - 1 {
        // 没有条目被删除，无需重写
        return Ok(());
    }

    // 重新组装，保留标题行（第一段前导空串）
    let mut new_content = String::new();
    for e in filtered {
        new_content.push_str("## [");
        new_content.push_str(e);
    }

    write_log(workspace_id, &new_content)
}