//! 用户工艺目录扫描与导入。
//!
//! 仿照专家模板的双层目录设计（`backend/src/expert/loader.rs`），
//! 为工艺模板引入用户层 `~/.ntd/processes/`，避免 bundled 同步覆盖用户自定义工艺。
//!
//! - 系统层：`~/.ntd/bundled/processes/`，从 `bundled-repo` 同步，会被 `git reset --hard` 覆盖
//! - 用户层：`~/.ntd/processes/`，用户自定义工艺，不会被同步覆盖
//!
//! 加载顺序：先扫描系统层（`is_system=true`），再扫描用户层（`is_system=false`）。
//! 040 起按 `guid` upsert：用户副本与系统模板 guid 不同，同名共存、不再互相覆盖。
//! 用户层文件缺 guid 时生成 UUID 并回写进文件（用户层不受 git 管理，回写安全）。

use std::path::PathBuf;

use crate::handlers::AppState;

/// 用户工艺根目录名称（与系统层 `bundled/processes/` 区分）。
const USER_PROCESSES_DIR_NAME: &str = "processes";

/// 056：导入失败 warn 的进程内去重表（source_path → 已告警的错误首行）。
/// 导入链路在每次工艺变更后全量重扫，不可修复的文件会每次触发同一条 warn；
/// 记录「文件+错误」组合，只在该组合首次出现时告警。
static IMPORT_WARNED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<(String, String)>>> =
    std::sync::OnceLock::new();

/// 同一 (source_path, error) 组合进程内只 warn 一次（056 日志刷屏治理）。
fn warn_once_per_file(source_path: &str, err: &str) {
    let warned = IMPORT_WARNED.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    // Mutex 中毒时取内部数据继续：告警去重不是关键路径，不值得 panic
    let mut set = warned.lock().unwrap_or_else(|e| e.into_inner());
    // 错误消息可能含动态细节，取首行做键，避免时间戳类噪声导致去重失效
    let key = (source_path.to_string(), err.lines().next().unwrap_or("").to_string());
    if set.insert(key) {
        tracing::warn!("保存用户工艺模板 {} 失败: {}", source_path, err);
    }
}

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
    // 本次扫描已见的 guid：两文件撞 guid（用户手动 cp 未改）时后者跳过，
    // 避免后扫描的文件顶掉先扫描的（upsert 静默覆盖难以排查）。
    let mut seen_guids: std::collections::HashSet<String> = std::collections::HashSet::new();
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

        match upsert_user_process_yaml(state, &path, &content, &source_path, &mut seen_guids).await {
            Ok(true) => imported_count += 1,
            // Ok(false)：guid 冲突跳过，已在内部 warn。
            Ok(false) => {}
            Err(e) => {
                // 056：同一文件的同一错误进程内只 warn 一次——
                // 该导入在每次 PUT /api/v1/processes 后都会全量重扫，
                // 不可修复的文件（如 flow 风格 YAML）会产生日志刷屏。
                warn_once_per_file(&source_path, &e);
            }
        }
    }

    tracing::info!("从用户层目录导入了 {} 个工艺模板", imported_count);
    Ok(())
}

/// 把单个用户层 YAML 文件解析并 upsert 为 `is_system=false` 的工艺模板。
///
/// 返回值：`Ok(true)` 入库成功；`Ok(false)` guid 冲突跳过；`Err` 解析/写库失败。
/// 文件缺 guid 时生成 UUID 回写进文件（行级插入，保住原格式）再继续——
/// 回写只发生在用户层，系统层文件不受此逻辑影响。
async fn upsert_user_process_yaml(
    state: &AppState,
    path: &std::path::Path,
    content: &str,
    source_path: &str,
    seen_guids: &mut std::collections::HashSet<String>,
) -> Result<bool, String> {
    // 复用 bundled handler 的解析函数，避免在本模块重复定义 YAML schema。
    let mut wrapper = crate::handlers::bundled::parse_process_file_for_user(content, source_path)
        .map_err(|e| format!("解析用户工艺 YAML 失败: {}", e))?;

    // 040：缺 guid 的用户层文件生成并回写，之后按 guid 作为身份。
    if wrapper.process.guid.is_empty() {
        let guid = super::guid::new_guid();
        // 056：行级插入失败时退化 serde 往返（flow 风格/非常规缩进文件）
        let updated = super::guid::insert_guid_with_serde_fallback(content, &guid)
            .ok_or_else(|| format!("无法在 {source_path} 的 process 块内插入 guid 行"))?;
        // 幂等情况下内容未变（文件已有 guid 但 serde 模型读为空），跳过写盘
        if updated != content {
            std::fs::write(path, &updated)
                .map_err(|e| format!("回写 guid 到 {} 失败: {}", path.display(), e))?;
            tracing::info!("为用户工艺 {} 生成并回写 guid: {}", source_path, guid);
        }
        wrapper.process.guid = guid;
    }

    if !seen_guids.insert(wrapper.process.guid.clone()) {
        tracing::warn!(
            "用户工艺 guid 冲突（另一文件已使用 {}），跳过 {}",
            wrapper.process.guid,
            source_path
        );
        return Ok(false);
    }

    let display_name = if wrapper.process.display_name.is_empty() {
        &wrapper.process.name
    } else {
        &wrapper.process.display_name
    };

    // 工艺正文只存于磁盘（~/.ntd/processes/），DB 仅保存 source_path 引用，不再落库 definition。
    state
        .db
        .upsert_user_process_template(
            &wrapper.process.guid,
            &wrapper.process.name,
            display_name,
            &wrapper.process.description,
            &wrapper.process.category,
            &wrapper.process.complexity,
            &wrapper.process.version,
            source_path,
        )
        .await
        .map_err(|e| format!("数据库写入失败: {}", e))?;

    Ok(true)
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
