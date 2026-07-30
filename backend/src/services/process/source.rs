//! 工艺定义源文件解析：把 `process_templates.source_path` 引用解析为磁盘文件并读取正文。
//!
//! 设计前提（需求 038）：DB 的 `process_templates` 只存路径引用，不存工艺 YAML 正文；
//! 真正的正文始终存在于磁盘（`~/.ntd/bundled/processes/` 系统层、`~/.ntd/processes/` 用户层）。
//! 因此所有需要工艺正文的地方，都通过本模块按 `source_path` 实时读文件，
//! 保证「磁盘是唯一真源」，DB 与文件永远单向一致（DB 镜像文件）。

use std::path::PathBuf;

use crate::git_sync;
use crate::handlers::errors::AppError;
use crate::services::process::user_dir;

/// 读取工艺定义正文时可能出现的错误。
///
/// 与 `AppError` / `InstallError` 之间实现了 `From`，调用方用 `?` 即可按自身错误类型传播，
/// 不必在每个调用点手写 match。
#[derive(Debug, thiserror::Error)]
pub enum ProcessSourceError {
    /// 路径无法解析，或解析后文件在磁盘上不存在（DB 行成了孤儿）。
    #[error("工艺源文件不存在: {0}")]
    NotFound(String),
    /// 文件读取失败（权限/编码等底层 IO 错误）。
    #[error("读取工艺源文件失败: {0}")]
    Read(#[from] std::io::Error),
}

/// 把逻辑 `source_path` 解析为磁盘绝对路径。
///
/// 两种前缀：
/// - `user://...`         → `~/.ntd/processes/...`（用户层，固定目录，与 git 同步无关）
/// - `bundled://...`      → `~/.ntd/{local_path}/...`（系统层，local_path 来自配置，默认 `bundled`）
///
/// 无法识别的前缀返回 `None`，交由调用方报 404。
pub fn resolve_source_path(source_path: &str, local_path: &str) -> Option<PathBuf> {
    // 用户层目录固定为 ~/.ntd/processes，不依赖 git 同步配置，因此忽略 local_path。
    if let Some(rel) = source_path.strip_prefix("user://") {
        return user_dir::user_processes_dir().map(|dir| dir.join(rel));
    }
    // 系统层目录来自 git 同步配置（默认 ~/.ntd/bundled），与 bundled_dir 同源，保证路径一致。
    if let Some(rel) = source_path.strip_prefix("bundled://") {
        return git_sync::bundled_dir(local_path).map(|dir| dir.join(rel));
    }
    None
}

/// 按 `source_path` 读取工艺定义正文（YAML 原文）。
///
/// 路径可解析但文件已不在磁盘时，返回 `NotFound`——这种情况说明磁盘已是真源、
/// DB 残留了孤儿行，应明确反馈 404 而非返回空内容。
pub fn read_definition(source_path: &str, local_path: &str) -> Result<String, ProcessSourceError> {
    // 先解析逻辑路径：无法识别前缀即视为资源不存在。
    let path = resolve_source_path(source_path, local_path)
        .ok_or_else(|| ProcessSourceError::NotFound(source_path.to_string()))?;
    // 再确认文件确实在磁盘上：磁盘是真源，DB 行可能指向已删除的文件。
    if !path.exists() {
        return Err(ProcessSourceError::NotFound(
            path.to_string_lossy().to_string(),
        ));
    }
    Ok(std::fs::read_to_string(&path)?)
}

/// `ProcessSourceError` → `AppError`：handler 层用 `?` 直接传播。
impl From<ProcessSourceError> for AppError {
    fn from(e: ProcessSourceError) -> Self {
        match e {
            // 找不到文件映射为 404，符合「资源缺失」语义。
            ProcessSourceError::NotFound(_) => AppError::NotFound,
            // 其它读取错误归为 500 内部错误。
            ProcessSourceError::Read(err) => {
                AppError::Internal(format!("读取工艺源文件失败: {}", err))
            }
        }
    }
}

/// `ProcessSourceError` → `InstallError`：安装链路用 `?` 直接传播（归为解析类错误）。
impl From<ProcessSourceError> for crate::services::process::InstallError {
    fn from(e: ProcessSourceError) -> Self {
        crate::services::process::InstallError::ParseError(format!("读取工艺源文件失败: {}", e))
    }
}

/// 源路径解析与读取的单元测试。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod source_tests {
    use super::*;

    #[test]
    fn resolve_source_path_user_prefix() {
        // user:// 应解析到用户层 ~/.ntd/processes 下的相对路径，与 git 同步配置无关。
        let p = resolve_source_path("user://software/x.yaml", "bundled")
            .expect("user:// 前缀应可解析");
        assert!(
            p.ends_with("processes/software/x.yaml"),
            "路径应拼到用户层目录下: {p:?}"
        );
    }

    #[test]
    fn resolve_source_path_bundled_prefix() {
        // bundled:// 应解析到 home/.ntd/<local_path>/...，跟随 git 同步配置（默认 bundled）。
        let p = resolve_source_path("bundled://processes/x.yaml", "bundled")
            .expect("bundled:// 前缀应可解析");
        assert!(
            p.ends_with("bundled/processes/x.yaml"),
            "路径应拼到系统层目录下: {p:?}"
        );
    }

    #[test]
    fn resolve_source_path_invalid_prefix_returns_none() {
        // 非法前缀无法识别，返回 None 交由调用方报 404，而不是误读成文件。
        assert!(resolve_source_path("http://nope", "bundled").is_none());
        assert!(resolve_source_path("plain-name.yaml", "bundled").is_none());
    }

    #[test]
    fn read_definition_invalid_prefix_is_not_found() {
        // 无法识别的前缀 → NotFound，明确反馈资源缺失，而非静默返回空内容。
        let err = read_definition("garbage://x.yaml", "bundled").unwrap_err();
        assert!(
            matches!(err, ProcessSourceError::NotFound(_)),
            "非法前缀必须映射为 NotFound"
        );
    }

    #[test]
    fn read_definition_missing_file_is_not_found() {
        // 路径前缀可解析，但磁盘上无对应文件 → NotFound，说明 DB 残留了孤儿行。
        // 用绝对路径作 local_path：bundled_dir 拼接绝对路径会直接采用该临时目录，非侵入。
        let tmp = std::env::temp_dir().join(format!("ntd_src_missing_{}", std::process::id()));
        let err = read_definition("bundled://missing/x.yaml", tmp.to_str().unwrap()).unwrap_err();
        assert!(
            matches!(err, ProcessSourceError::NotFound(_)),
            "缺失文件必须映射为 NotFound"
        );
    }

    #[test]
    fn read_definition_reads_existing_file() {
        // 成功路径：bundled:// 指向临时目录中的真实文件，应读回其正文。
        let tmp = std::env::temp_dir().join(format!("ntd_src_ok_{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("processes")).unwrap();
        let file = tmp.join("processes/ok.yaml");
        std::fs::write(&file, "process:\n  name: ok\n").unwrap();
        let content =
            read_definition("bundled://processes/ok.yaml", tmp.to_str().unwrap()).unwrap();
        assert!(content.contains("name: ok"), "应读回磁盘文件正文");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
