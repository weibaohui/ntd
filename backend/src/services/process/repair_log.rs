//! `.ntd/repair-log.md` 维护。
//!
//! 每次返工、门禁失败、人工审批拒绝时追加条目。
//! 只追加不覆盖，保留完整修复历史。

use std::path::Path;

use tokio::io::AsyncWriteExt;

/// repair-log 写入错误。
#[derive(Debug, thiserror::Error)]
pub enum RepairLogError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

/// 追加一条 repair-log 条目。
///
/// # 参数
/// - `workspace_path`：工作空间根目录。
/// - `step_name`：发生回退/失败的环节名。
/// - `reason`：失败/回退原因。
/// - `rework_count`：当前的返工次数。
/// - `max_rework`：允许的最大返工次数。
/// - `target_step_name`：跳转到的目标环节名（返工回流目的地）。
pub async fn append_repair_entry(
    workspace_path: &str,
    step_name: &str,
    reason: &str,
    rework_count: i32,
    max_rework: i32,
    target_step_name: &str,
) -> Result<(), RepairLogError> {
    let dir = Path::new(workspace_path).join(".ntd");
    tokio::fs::create_dir_all(&dir).await?;

    let filepath = dir.join("repair-log.md");
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // 读取已有内容，计算条目序号。
    let existing = tokio::fs::read_to_string(&filepath).await.unwrap_or_default();
    let entry_count = existing.matches("## 条目").count() + 1;

    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&filepath)
        .await?;

    // 如果文件为空，先写标题头。
    if existing.is_empty() {
        file.write_all(b"# Repair Log\n\n").await?;
    }

    let entry = format!(
        "## 条目 {} ({})\n\n\
         - 环节：{}\n\
         - 原因：{}\n\
         - 返工次数：{}/{}\n\
         - 目标：{} → {}\n\n",
        entry_count, now, step_name, reason, rework_count, max_rework, step_name, target_step_name,
    );

    file.write_all(entry.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_append_repair_entry_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().to_str().unwrap();

        append_repair_entry(ws, "生成 PRD", "AI 评审未通过（评分 65，阈值 80）", 1, 3, "生成 PRD")
            .await
            .unwrap();

        let filepath = tmp.path().join(".ntd/repair-log.md");
        assert!(filepath.exists(), "repair-log.md should exist");

        let content = std::fs::read_to_string(&filepath).unwrap();
        assert!(content.contains("生成 PRD"));
        assert!(content.contains("1/3"));
        assert!(content.contains("AI 评审未通过"));
    }

    #[tokio::test]
    async fn test_append_repair_entry_appends_multiple() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().to_str().unwrap();

        append_repair_entry(ws, "step A", "第一次失败", 1, 3, "step A")
            .await
            .unwrap();
        append_repair_entry(ws, "step B", "第二次失败", 2, 3, "step A")
            .await
            .unwrap();

        let content = std::fs::read_to_string(tmp.path().join(".ntd/repair-log.md")).unwrap();
        assert_eq!(content.matches("## 条目").count(), 2);
        assert!(content.contains("第一次失败"));
        assert!(content.contains("第二次失败"));
        assert!(content.contains("step B → step A"));
    }
}
