//! 执行日志追加。
//!
//! log.md 记录每次摄入的时间、涉及页面、来源记录，便于追溯。
//! 采用追加模式，永不修改旧条目。

use chrono::Local;
use std::io;

use super::fs::append_log;

/// 追加一条摄入日志。
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

    append_log(workspace_id, &entry)?;

    Ok(())
}