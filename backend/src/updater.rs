//! 自升级模块：管理 ntd 的版本检测与升级流程。
//!
//! 本模块将升级来源配置化，支持 npm、manual、cargo 等多种安装方式，
//! 消除之前硬编码 `@weibaohui/nothing-todo` 包名和 npm 强耦合的问题。
//!
//! 升级来源在 `~/.ntd/config.yaml` 的 `update` 段配置，默认使用 npm。

use std::path::PathBuf;

use crate::config::Config;

/// 升级来源，封装安装方式和升级参数。
#[derive(Debug, Clone)]
pub struct UpdateSource {
    /// 安装方式
    pub method: InstallMethod,
    /// 包名（仅 npm/cargo 方式需要）
    pub package_name: String,
}

/// 支持的安装方式。
#[derive(Debug, Clone)]
pub enum InstallMethod {
    /// 通过 npm 全局安装
    Npm,
    /// 通过 apt 安装
    Apt,
    /// 手动下载二进制
    Manual,
    /// 通过 cargo install 安装
    Cargo,
}

impl UpdateSource {
    /// 从全局配置创建 `UpdateSource`。
    ///
    /// 读取 `~/.ntd/config.yaml` 中 `update` 段的配置。
    pub fn from_config() -> Self {
        let cfg = Config::load();
        Self::from_config_ref(&cfg)
    }

    /// 从已有 `Config` 引用创建 `UpdateSource`。
    pub fn from_config_ref(cfg: &Config) -> Self {
        let method = match cfg.update.source.as_str() {
            "npm" => InstallMethod::Npm,
            "manual" => InstallMethod::Manual,
            "cargo" => InstallMethod::Cargo,
            "apt" => InstallMethod::Apt,
            _ => InstallMethod::Npm, // 默认 npm
        };
        let package_name = cfg.update.npm_package.clone();
        Self { method, package_name }
    }

    /// 返回包名（用于 `npm install -g <pkg>@latest` 等场景）。
    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    /// 执行升级操作。
    ///
    /// 根据 `InstallMethod` 选择不同的升级路径：
    /// - `Npm`: 调用 `npm install -g <pkg>@latest`
    /// - `Manual`: 打印提示引导用户去 GitHub Releases
    /// - `Cargo`: 打印提示引导用户使用 `cargo install`
    /// - `Apt`: 打印提示引导用户使用 `apt upgrade`
    pub async fn upgrade(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.method {
            InstallMethod::Npm => {
                let prefix = get_npm_global_prefix();
                let status = std::process::Command::new("npm")
                    .args([
                        "install",
                        "-g",
                        &format!("--prefix={}", prefix),
                        &format!("{}@latest", self.package_name),
                    ])
                    .status()
                    .map_err(|e| format!("Failed to run npm: {}. Is npm installed?", e))?;

                if !status.success() {
                    return Err("npm upgrade failed".into());
                }
                Ok(())
            }
            InstallMethod::Manual => {
                eprintln!(
                    "请手动下载最新版本: https://github.com/weibaohui/nothing-todo/releases"
                );
                Err("manual upgrade requires user intervention".into())
            }
            InstallMethod::Cargo => {
                eprintln!("请运行 `cargo install --path .` 或从 GitHub Releases 下载二进制");
                Err("cargo upgrade requires user intervention".into())
            }
            InstallMethod::Apt => {
                eprintln!("请运行 `sudo apt update && sudo apt upgrade ntd`");
                Err("apt upgrade requires user intervention".into())
            }
        }
    }
}

/// 检查远程最新版本号。
///
/// 根据配置的 `source` 决定检查方式：
/// - `npm`: 调用 `npm view <pkg> version`
/// - 其他方式：返回 `None` 表示暂不支持自动检查
pub async fn check_latest_version() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let source = UpdateSource::from_config();
    match source.method {
        InstallMethod::Npm => {
            let output = std::process::Command::new("npm")
                .args(["view", &source.package_name, "version"])
                .output()
                .map_err(|e| format!("Failed to run npm view: {}", e))?;

            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if version.is_empty() {
                    Err("npm view returned empty version".into())
                } else {
                    Ok(version)
                }
            } else {
                let err_msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(format!("npm view failed: {}", err_msg).into())
            }
        }
        _ => Err("auto version check not supported for current update source".into()),
    }
}

/// 获取 npm 全局安装的 prefix。
///
/// 优先使用 `npm prefix -g` 返回的默认全局目录；若该目录不可写
///（常见于 `/usr/local` 需要 root 权限的场景），则回退到 `~/.npm-global`，
/// 避免在无交互的 Web UI 中因 EACCES 而升级失败。
pub fn get_npm_global_prefix() -> String {
    // 通过 npm 子命令获取当前全局安装目录，而非硬编码路径
    let default_prefix = std::process::Command::new("npm")
        .args(["prefix", "-g"])
        .output();

    match default_prefix {
        Ok(out) if out.status.success() => {
            let prefix = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let path = std::path::Path::new(&prefix);
            // 默认目录存在且可写时直接使用，无需额外配置
            if path.exists() && is_writable_dir(path) {
                return prefix;
            }
        }
        // npm 不可用或执行失败时走 fallback
        _ => {}
    }

    // 默认全局目录不可写，回退到用户主目录下的 .npm-global，
    // 该目录普通用户有完整权限，不需要 sudo
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let user_prefix = home.join(".npm-global");
    // 确保目录存在，否则后续 npm install 会失败
    let _ = std::fs::create_dir_all(&user_prefix);
    tracing::info!(
        "npm global directory not writable, using {} as prefix",
        user_prefix.display()
    );
    user_prefix.to_string_lossy().to_string()
}

/// 检查目录是否可写。
///
/// 通过尝试创建临时文件来判断实际写权限，而非仅检查 POSIX 权限位。
/// 这样能正确处理 ACL、只读挂载等边界情况。
pub fn is_writable_dir(path: &std::path::Path) -> bool {
    if !path.exists() {
        return false;
    }
    // 使用进程 PID 作为文件名后缀，避免多进程并发时冲突
    let test_file = path.join(format!(".ntd_write_test_{}", std::process::id()));
    match std::fs::File::create(&test_file) {
        Ok(_) => {
            // 创建成功即说明有写权限，清理临时文件
            let _ = std::fs::remove_file(&test_file);
            true
        }
        // 无法创建文件 = 没有写权限
        Err(_) => false,
    }
}

/// 查找新安装的 ntd 可执行文件路径。
///
/// 按优先级依次尝试：
/// 1. `{prefix}/bin/ntd` — npm 全局安装后的标准位置
/// 2. 当前进程的可执行文件路径 — 从源码 make install 安装的场景
/// 3. `"ntd"` — 依赖 PATH 查找作为最终 fallback
pub fn find_ntd_binary(prefix: &str) -> String {
    // npm 全局安装的可执行文件链接到 {prefix}/bin/ 下
    let prefix_path = std::path::Path::new(prefix).join("bin").join("ntd");
    if prefix_path.exists() {
        tracing::info!("Found newly installed ntd at: {}", prefix_path.display());
        return prefix_path.to_string_lossy().to_string();
    }

    // 从源码 make install 的场景，ntd 可能在 ~/.local/bin 等其他位置，
    // 当前进程的可执行文件路径就是新版本的位置
    if let Ok(exe) = std::env::current_exe() {
        if exe.exists() {
            tracing::info!("Using current executable as ntd path: {}", exe.display());
            return exe.to_string_lossy().to_string();
        }
    }

    // 最终 fallback：依赖 PATH 环境变量查找
    tracing::warn!("Falling back to PATH lookup for ntd");
    "ntd".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_source_default_is_npm() {
        let source = UpdateSource::from_config();
        assert!(matches!(source.method, InstallMethod::Npm));
        assert_eq!(source.package_name, "@weibaohui/nothing-todo");
    }

    #[test]
    fn test_update_source_from_config_ref_default() {
        let cfg = Config::default();
        let source = UpdateSource::from_config_ref(&cfg);
        assert!(matches!(source.method, InstallMethod::Npm));
        assert_eq!(source.package_name, "@weibaohui/nothing-todo");
    }

    #[test]
    fn test_update_source_from_config_ref_manual() {
        let mut cfg = Config::default();
        cfg.update.source = "manual".to_string();
        let source = UpdateSource::from_config_ref(&cfg);
        assert!(matches!(source.method, InstallMethod::Manual));
    }

    #[test]
    fn test_update_source_from_config_ref_custom_package() {
        let mut cfg = Config::default();
        cfg.update.npm_package = "@myorg/my-ntd".to_string();
        let source = UpdateSource::from_config_ref(&cfg);
        assert_eq!(source.package_name(), "@myorg/my-ntd");
    }

    #[test]
    fn test_is_writable_dir_non_existent() {
        let tmp = std::path::Path::new("/nonexistent_path_12345");
        assert!(!is_writable_dir(tmp));
    }

    #[test]
    fn test_is_writable_dir_tmp() {
        let tmp = std::path::Path::new("/tmp");
        // /tmp 应该总是可写的（测试环境）
        assert!(is_writable_dir(tmp));
    }
}
