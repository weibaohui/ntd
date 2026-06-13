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
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl InstallMethod {
    /// 序列化为配置里使用的 snake-case 字符串（与 YAML 配置一致）。
    ///
    /// 用于 API 响应、日志、配置文件回写等场景，
    /// 避免 `{:?}` Debug 格式产生 "Npm" / "Manual" 这种 PascalCase。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Apt => "apt",
            Self::Manual => "manual",
            Self::Cargo => "cargo",
        }
    }

    /// 返回给用户的升级指引文案。
    ///
    /// 适用于 manual/cargo/apt 这类无法由 daemon 端自动执行的安装方式，
    /// 通过 HTTP API / CLI 返回给前端或用户终端显示。
    pub fn guidance(&self) -> &'static str {
        match self {
            Self::Npm => "将通过 npm 全局安装最新版本",
            Self::Apt => {
                // ntd 当前未发布到 apt 仓库，引导用户去 GitHub Releases 自行下载。
                "ntd 未发布到 apt 仓库，请前往 https://github.com/weibaohui/nothing-todo/releases 下载最新二进制"
            }
            Self::Manual => "请前往 https://github.com/weibaohui/nothing-todo/releases 下载最新二进制并替换当前版本",
            Self::Cargo => "请运行 `cargo install --path .` 或从 GitHub Releases 下载预编译二进制",
        }
    }
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
            unknown => {
                // 未知的 source 值静默回退为 npm 会掩盖配置错误（例如把 "npm" 写成 "npmm"），
                // 输出 warn 日志让用户能在日志中发现拼写错误。
                tracing::warn!(
                    "未知的 update.source 值 '{}'，回退为默认值 npm。可选值: npm/manual/cargo/apt",
                    unknown
                );
                InstallMethod::Npm
            }
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
                eprintln!("{}", self.method.guidance());
                Err("manual upgrade requires user intervention".into())
            }
            InstallMethod::Cargo => {
                eprintln!("{}", self.method.guidance());
                Err("cargo upgrade requires user intervention".into())
            }
            InstallMethod::Apt => {
                eprintln!("{}", self.method.guidance());
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
    fn test_update_source_unknown_falls_back_to_npm_without_panic() {
        // 用户配错 source 值（拼写错误等），应回退到 npm 而不 panic。
        // 这里不直接断言 warn 日志（tracing subscriber 未初始化），
        // 只验证不崩溃 + method = Npm。
        let mut cfg = Config::default();
        cfg.update.source = "npmm".to_string(); // 故意拼错
        let source = UpdateSource::from_config_ref(&cfg);
        assert!(matches!(source.method, InstallMethod::Npm));
    }

    #[test]
    fn test_install_method_as_str_is_snake_case() {
        // 防止有人改回 PascalCase，API 响应 / YAML 配置都用 snake-case。
        assert_eq!(InstallMethod::Npm.as_str(), "npm");
        assert_eq!(InstallMethod::Apt.as_str(), "apt");
        assert_eq!(InstallMethod::Manual.as_str(), "manual");
        assert_eq!(InstallMethod::Cargo.as_str(), "cargo");
    }

    #[test]
    fn test_install_method_guidance_nonempty_for_all_variants() {
        // 所有变体都需要给用户一个明确的指引，避免 guidance() 返回空串。
        for method in [
            InstallMethod::Npm,
            InstallMethod::Apt,
            InstallMethod::Manual,
            InstallMethod::Cargo,
        ] {
            assert!(
                !method.guidance().is_empty(),
                "{:?} guidance must be non-empty",
                method
            );
        }
    }

    #[test]
    fn test_install_method_apt_guidance_mentions_releases_not_apt_repo() {
        // 防回归：ntd 当前未发布到 apt 仓库，不能给用户错误的 `apt upgrade ntd` 提示。
        let g = InstallMethod::Apt.guidance();
        assert!(
            !g.contains("sudo apt upgrade ntd"),
            "apt guidance must not suggest apt upgrade ntd (ntd is not in apt repos)"
        );
        assert!(
            g.contains("github.com") || g.contains("releases"),
            "apt guidance should point users to GitHub Releases"
        );
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
