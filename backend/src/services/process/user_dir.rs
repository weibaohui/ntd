//! 用户工艺目录扫描与导入。
//!
//! 仿照专家模板的双层目录设计（`backend/src/expert/loader.rs`），
//! 为工艺模板引入用户层 `~/.ntd/processes/`，避免 bundled 同步覆盖用户自定义工艺。
//!
//! - 系统层：`~/.ntd/bundled/processes/`，从 `bundled-repo` 同步，会被 `git reset --hard` 覆盖
//! - 用户层：`~/.ntd/processes/`，用户自定义工艺，不会被同步覆盖
//!
//! 加载顺序：先扫描系统层（`is_system=true`），再扫描用户层（`is_system=false`）。
//! 同名工艺时用户层覆盖系统层（`name` 为唯一键，第二次 upsert 覆盖第一次）。

use std::path::PathBuf;

use crate::handlers::AppState;

/// 用户工艺根目录名称（与系统层 `bundled/processes/` 区分）。
const USER_PROCESSES_DIR_NAME: &str = "processes";

/// 用户工艺 `source_path` 前缀，与系统层 `bundled://` 区分。
pub const USER_SOURCE_PREFIX: &str = "user://";

/// 获取用户工艺根目录路径（`~/.ntd/processes/`）。
///
/// 与专家目录 `~/.ntd/experts/` 同构，便于用户记忆和管理。
/// 若 `dirs::home_dir` 无法获取 home 目录则返回 `None`。
pub fn user_processes_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ntd").join(USER_PROCESSES_DIR_NAME))
}

/// 扫描用户层工艺目录，把 `~/.ntd/processes/**/*.yaml` upsert 为 `is_system=false` 的工艺模板。
///
/// 目录不存在时返回 `Ok(())`，与系统层扫描逻辑保持一致（首次安装时用户层尚未创建）。
/// 单条文件解析失败只 warning，不阻断整体导入。
pub async fn import_user_process_templates(state: &AppState) -> Result<(), String> {
    // 用户层目录可能尚未创建（用户从未复制过系统工艺），此时跳过导入。
    let user_dir = user_processes_dir().ok_or_else(|| "无法获取 home 目录".to_string())?;
    if !user_dir.exists() {
        tracing::info!("用户工艺目录 {} 不存在，跳过导入", user_dir.display());
        return Ok(());
    }

    // 复用 `bundled.rs::collect_yaml_files` 的递归扫描逻辑，
    // 但本模块位于 `services/process`，为避免循环依赖，这里直接调用 std::fs。
    let yaml_files = collect_yaml_files(&user_dir)
        .map_err(|e| format!("读取用户工艺目录失败: {}", e))?;

    let mut imported_count = 0;
    for path in yaml_files {
        // 读取失败只 warning 跳过，不阻断整体导入，与系统层逻辑一致。
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("读取用户工艺文件失败 {}: {}", path.display(), e);
                continue;
            }
        };

        // source_path 用 `user://` 前缀，便于前端区分系统工艺与用户工艺。
        let rel_path = path
            .strip_prefix(&user_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let source_path = format!("{}{}", USER_SOURCE_PREFIX, rel_path);

        if let Err(e) = upsert_user_process_yaml(state, &content, &source_path).await {
            tracing::warn!("保存用户工艺模板 {} 失败: {}", source_path, e);
            continue;
        }
        imported_count += 1;
    }

    tracing::info!("从用户层目录导入了 {} 个工艺模板", imported_count);
    Ok(())
}

/// 把单个用户层 YAML 文件解析并 upsert 为 `is_system=false` 的工艺模板。
///
/// 解析逻辑复用 `handlers::bundled::parse_process_file` 的 YAML schema，
/// 确保用户层与系统层 YAML 格式完全一致。
async fn upsert_user_process_yaml(
    state: &AppState,
    content: &str,
    source_path: &str,
) -> Result<(), String> {
    // 复用 bundled handler 的解析函数，避免在本模块重复定义 YAML schema。
    let wrapper = crate::handlers::bundled::parse_process_file_for_user(content, source_path)
        .map_err(|e| format!("解析用户工艺 YAML 失败: {}", e))?;

    let display_name = if wrapper.process.display_name.is_empty() {
        &wrapper.process.name
    } else {
        &wrapper.process.display_name
    };

    state
        .db
        .upsert_user_process_template(
            &wrapper.process.name,
            display_name,
            &wrapper.process.description,
            &wrapper.process.category,
            &wrapper.process.complexity,
            &wrapper.process.version,
            content,
            source_path,
        )
        .await
        .map_err(|e| format!("数据库写入失败: {}", e))?;

    Ok(())
}

/// 递归收集目录下所有 `*.yaml` / `*.yml` 文件，按路径排序保证导入顺序稳定。
fn collect_yaml_files(dir: &std::path::Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    collect_yaml_files_recursive(dir, &mut files)?;
    // 按路径字符串排序，保证跨平台导入顺序稳定，与 `bundled.rs::collect_yaml_files` 一致。
    files.sort_by(|a, b| a.as_os_str().cmp(b.as_os_str()));
    Ok(files)
}

/// 递归收集 yaml 文件的内部实现。
fn collect_yaml_files_recursive(
    dir: &std::path::Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), std::io::Error> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_yaml_files_recursive(&path, out)?;
        } else if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "yaml" || ext == "yml" {
                out.push(path);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;

    /// `user_processes_dir` 应该指向 `~/.ntd/processes/`。
    #[test]
    fn test_user_processes_dir_path() {
        let dir = user_processes_dir();
        assert!(dir.is_some(), "home 目录应该可获取");
        let path = dir.unwrap();
        assert!(
            path.ends_with(".ntd/processes") || path.ends_with(".ntd\\processes"),
            "用户工艺目录路径应为 ~/.ntd/processes/, 实际: {}",
            path.display()
        );
    }

    /// `USER_SOURCE_PREFIX` 应为 `user://`，与系统层 `bundled://` 区分。
    #[test]
    fn test_user_source_prefix() {
        assert_eq!(USER_SOURCE_PREFIX, "user://");
    }

    /// 目录不存在时 `collect_yaml_files` 应返回空列表而非报错。
    #[test]
    fn test_collect_yaml_files_nonexistent_dir() {
        let nonexistent = std::path::Path::new("/tmp/ntd_test_nonexistent_dir_9999");
        let result = collect_yaml_files(nonexistent);
        assert!(result.is_ok(), "目录不存在时应返回 Ok");
        assert!(result.unwrap().is_empty(), "目录不存在时结果应为空");
    }

    /// 目录存在但为空时也应返回空列表。
    #[test]
    fn test_collect_yaml_files_empty_dir() {
        let temp = tempfile::tempdir().expect("创建 tempdir 失败");
        let result = collect_yaml_files(temp.path()).expect("空目录扫描应成功");
        assert!(result.is_empty(), "空目录应返回空文件列表");
    }

    /// 递归扫描应能找到子目录中的 yaml 文件。
    #[test]
    fn test_collect_yaml_files_recursive() {
        let temp = tempfile::tempdir().expect("创建 tempdir 失败");
        // 创建 software/ 子目录并放入一个 yaml 文件
        let sub_dir = temp.path().join("software");
        std::fs::create_dir_all(&sub_dir).expect("创建子目录失败");
        std::fs::write(sub_dir.join("test.yaml"), "process: {}")
            .expect("写入测试文件失败");
        // 也放一个非 yaml 文件，应被忽略
        std::fs::write(sub_dir.join("README.md"), "# readme")
            .expect("写入测试文件失败");

        let result = collect_yaml_files(temp.path()).expect("递归扫描应成功");
        assert_eq!(result.len(), 1, "应只找到 1 个 yaml 文件");
        assert!(result[0].ends_with("test.yaml"));
    }
}
