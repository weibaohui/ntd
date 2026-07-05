//! 自动生成 index.md。
//!
//! index.md 是目录页，列出所有 topic 页面及其标题和摘要。
//! 由后端自动生成，不让 LLM 写，保证内容与实际文件 100% 一致。

use std::io;

use super::fs::{list_topics, read_topic, write_index};

/// 自动生成 index.md。
///
/// 遍历 topics 目录下所有文件，提取标题（第一行 # 开头），
/// 生成目录列表。
pub fn regenerate_index(workspace_id: i64) -> io::Result<()> {
    let topics = list_topics(workspace_id)?;

    let mut content = String::from("# 目录\n\n");

    if topics.is_empty() {
        content.push_str("_暂无主题页面，等待执行结论自动生成。_\n");
    } else {
        content.push_str("## 主题页面\n\n");

        for slug in &topics {
            // 读取文件，提取标题
            let topic_content = read_topic(workspace_id, slug)?;
            let title = extract_title(topic_content.as_deref(), slug);

            content.push_str(&format!("- **{}** — [{}](topics/{slug}.md)\n", title, slug));
        }

        content.push_str(&format!(
            "\n---\n*页面总数：{} | 最后更新：自动生成*\n",
            topics.len()
        ));
    }

    write_index(workspace_id, &content)?;

    Ok(())
}

/// 从 Markdown 内容提取标题。
///
/// 查找第一行 `# 标题`，如果不存在则返回 slug 作为标题。
fn extract_title(content: Option<&str>, slug: &str) -> String {
    if let Some(content) = content {
        for line in content.lines() {
            if line.starts_with("# ") {
                return line.trim_start_matches("# ").trim().to_string();
            }
        }
    }

    // 没找到标题，用 slug 作为显示名称
    slug.to_string()
}