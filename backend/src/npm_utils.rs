//! npm 全局目录相关工具函数。
//!
//! 提供检测 npm 全局目录写权限、获取安全的安装 prefix 等能力，
//! 用于 `ntd upgrade` 和 Web API 一键更新场景。

use std::path::PathBuf;

/// 获取 npm 全局安装的 prefix。
///
/// 优先使用 `npm prefix -g` 返回的默认全局目录；若该目录不可写
/// （常见于 `/usr/local` 需要 root 权限的场景），则回退到 `~/.npm-global`，
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
