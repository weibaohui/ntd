//! Wiki 文件系统操作。
//!
//! 提供 wiki 目录的创建、读取、写入、删除操作。

use std::fs;
use std::io;
use std::path::PathBuf;

/// 获取 ntd home 目录（~/.ntd）。
///
/// 与系统其他模块保持一致，使用 dirs::home_dir()。
/// 如果 home_dir 不可用则报错，防止 wiki 数据意外写入 /tmp 导致丢失。
fn ntd_home() -> io::Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法获取用户 home 目录，请检查 $HOME 环境变量"))?;
    Ok(home.join(".ntd"))
}

/// 获取指定工作空间的 wiki 目录路径。
///
/// 路径格式：~/.ntd/workspace/<workspace_id>/wiki/
pub fn wiki_dir(workspace_id: i64) -> io::Result<PathBuf> {
    Ok(ntd_home()?
        .join("workspace")
        .join(workspace_id.to_string())
        .join("wiki"))
}

/// 获取 topics 子目录路径。
///
/// 路径格式：~/.ntd/workspace/<workspace_id>/wiki/topics/
pub fn topics_dir(workspace_id: i64) -> io::Result<PathBuf> {
    Ok(wiki_dir(workspace_id)?.join("topics"))
}

/// 获取 index.md 文件路径。
pub fn index_file(workspace_id: i64) -> io::Result<PathBuf> {
    Ok(wiki_dir(workspace_id)?.join("index.md"))
}

/// 获取 log.md 文件路径。
pub fn log_file(workspace_id: i64) -> io::Result<PathBuf> {
    Ok(wiki_dir(workspace_id)?.join("log.md"))
}

/// 验证 topic slug 防止路径遍历攻击。
///
/// 只允许字母、数字、连字符（-）、下划线（_），不允许路径分隔符和 ..。
fn validate_slug(slug: &str) -> io::Result<()> {
    if slug.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "slug 不能为空"));
    }
    if slug.contains('/') || slug.contains('\\') || slug.contains("..") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("slug 包含非法字符: {}", slug),
        ));
    }
    for c in slug.chars() {
        if c == '.' || c == '<' || c == '>' || c == ':' || c == '"' || c == '|' || c == '?' || c == '*' {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("slug 包含非法字符 '{}': {}", c, slug),
            ));
        }
    }
    Ok(())
}

/// 获取指定 topic 文件路径。
///
/// slug 示例：auth-module → topics/auth-module.md
pub fn topic_file(workspace_id: i64, slug: &str) -> io::Result<PathBuf> {
    validate_slug(slug)?;
    Ok(topics_dir(workspace_id)?.join(format!("{}.md", slug)))
}

/// 初始化 wiki 目录结构。
///
/// 创建 wiki/ 和 topics/ 目录（如果不存在）。
pub fn init_wiki_dir(workspace_id: i64) -> io::Result<()> {
    let wiki = wiki_dir(workspace_id)?;
    let topics = topics_dir(workspace_id)?;

    if !wiki.exists() {
        fs::create_dir_all(&wiki)?;
    }
    if !topics.exists() {
        fs::create_dir_all(&topics)?;
    }

    Ok(())
}

/// 列出所有 topic 文件。
///
/// 返回 topics 目录下所有 .md 文件的 slug 列表（不含 .md 后缀）。
pub fn list_topics(workspace_id: i64) -> io::Result<Vec<String>> {
    let topics = topics_dir(workspace_id)?;

    if !topics.exists() {
        return Ok(Vec::new());
    }

    let mut slugs = Vec::new();
    for entry in fs::read_dir(topics)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "md").unwrap_or(false) {
            let slug = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            slugs.push(slug);
        }
    }

    slugs.sort();
    Ok(slugs)
}

/// 读取 topic 文件内容。
///
/// 返回 None 表示文件不存在。
pub fn read_topic(workspace_id: i64, slug: &str) -> io::Result<Option<String>> {
    let path = topic_file(workspace_id, slug)?;

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    Ok(Some(content))
}

/// 写入 topic 文件（创建或覆盖）。
pub fn write_topic(workspace_id: i64, slug: &str, content: &str) -> io::Result<()> {
    init_wiki_dir(workspace_id)?;
    let path = topic_file(workspace_id, slug)?;
    fs::write(path, content)?;
    Ok(())
}

/// 删除 topic 文件。
///
/// 返回 false 表示文件不存在。
pub fn delete_topic(workspace_id: i64, slug: &str) -> io::Result<bool> {
    let path = topic_file(workspace_id, slug)?;

    if !path.exists() {
        return Ok(false);
    }

    fs::remove_file(path)?;
    Ok(true)
}

/// 读取 index.md 内容。
pub fn read_index(workspace_id: i64) -> io::Result<Option<String>> {
    let path = index_file(workspace_id)?;

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    Ok(Some(content))
}

/// 写入 index.md（覆盖）。
pub fn write_index(workspace_id: i64, content: &str) -> io::Result<()> {
    init_wiki_dir(workspace_id)?;
    let path = index_file(workspace_id)?;
    fs::write(path, content)?;
    Ok(())
}

/// 读取 log.md 内容。
pub fn read_log(workspace_id: i64) -> io::Result<Option<String>> {
    let path = log_file(workspace_id)?;

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    Ok(Some(content))
}

/// 追加内容到 log.md。
///
/// 使用 `.append(true).create(true)` 原子性追加，避免 TOCTOU 竞争。
pub fn append_log(workspace_id: i64, entry: &str) -> io::Result<()> {
    init_wiki_dir(workspace_id)?;
    let path = log_file(workspace_id)?;

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;

    use std::io::Write;
    file.write_all(entry.as_bytes())?;
    Ok(())
}
