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

/// 覆盖写入 log.md。
pub fn write_log(workspace_id: i64, content: &str) -> io::Result<()> {
    init_wiki_dir(workspace_id)?;
    let path = log_file(workspace_id)?;
    fs::write(path, content)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// wiki fs 直接落在 `~/.ntd/workspace/<id>/` 下，与真实工作空间共享 home 目录。
    /// 取一个绝不会与真实工作空间撞车的超大 id 做隔离，并在用例结束时整体清掉该 id 的目录，
    /// 保证测试不污染开发者本地数据；即便中途 panic 残留，该 id 也不会被真实数据命中。
    const TEST_WORKSPACE_ID: i64 = 999_999_899;

    /// 多个用例共用同一个 TEST_WORKSPACE_ID 且都做「先清后写」的磁盘操作，
    /// cargo 默认并行跑用例会互相覆盖（A 刚写入就被 B 的 cleanup 删掉）。
    /// 用一把全局 Mutex 把本模块的用例串行化，避免磁盘竞态导致随机失败。
    fn workspace_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// 用例收尾：删除测试专属工作空间目录，幂等（不存在则跳过）。
    fn cleanup_test_workspace() {
        let _ = fs::remove_dir_all(
            ntd_home()
                .expect("测试需可获取 home 目录")
                .join("workspace")
                .join(TEST_WORKSPACE_ID.to_string()),
        );
    }

    /// delete_topic 的契约：存在则删并返回 true；不存在返回 false（幂等）；
    /// 删除后 read_topic 必须读到 None，确认文件确实被移除而非残留空壳。
    #[test]
    fn test_delete_topic_removes_existing_file() {
        let _guard = workspace_lock().lock().unwrap();
        cleanup_test_workspace();
        // 种入一个 topic，作为删除目标
        write_topic(TEST_WORKSPACE_ID, "to-be-deleted", "# hello")
            .expect("写入 topic 失败");
        // 存在时删除返回 true
        let removed = delete_topic(TEST_WORKSPACE_ID, "to-be-deleted")
            .expect("删除已存在 topic 失败");
        assert!(removed, "文件存在时 delete_topic 应返回 true");
        // 文件确实没了：read_topic 返回 None
        let after = read_topic(TEST_WORKSPACE_ID, "to-be-deleted")
            .expect("删除后 read_topic 不应报错");
        assert!(after.is_none(), "删除后文件不应残留");
        cleanup_test_workspace();
    }

    /// delete_topic 对不存在的 slug 必须幂等返回 false，而不是报错——
    /// 这支撑了 HTTP 层「文件本就不存在也算成功」的语义。
    #[test]
    fn test_delete_topic_missing_returns_false() {
        let _guard = workspace_lock().lock().unwrap();
        cleanup_test_workspace();
        let removed = delete_topic(TEST_WORKSPACE_ID, "never-exists")
            .expect("删除不存在的 topic 不应报错");
        assert!(!removed, "文件不存在时 delete_topic 应返回 false");
        cleanup_test_workspace();
    }

    /// validate_slug 必须挡住路径遍历与非法字符，避免 delete_topic 被诱导删除 topics/ 之外的文件。
    #[test]
    fn test_delete_topic_rejects_path_traversal() {
        let _guard = workspace_lock().lock().unwrap();
        cleanup_test_workspace();
        let err = delete_topic(TEST_WORKSPACE_ID, "../secrets").err();
        assert!(err.is_some(), "含 .. 的 slug 应被 validate_slug 拒绝");
        cleanup_test_workspace();
    }
}
