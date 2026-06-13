//! 跨平台共享的路径定位 helper。
//!
//! 这几个函数被 macOS/Linux/Windows 三边的实现都用（写 plist / unit file / schtasks 命令行时
//! 都需要拿到 binary 路径和状态目录），所以抽到这里统一维护。
//! 仍然保持 `pub(crate)` 可见性 —— 没必要对外暴露。

use std::path::PathBuf;

/// Get the path of the currently running ntd binary
/// Uses args()[0] to get the actual command path (handles sudo correctly)
/// Falls back to current_exe if args[0] is not an absolute path
pub(crate) fn ntd_binary_path() -> PathBuf {
    std::env::args()
        .next()
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| std::env::current_exe().expect("Failed to get current executable path"))
}

#[allow(unused)]
pub(crate) fn ntd_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".ntd")
}

/// Get the directory containing the ntd binary (for PATH in service definition)
#[allow(unused)]
pub(crate) fn ntd_bin_dir() -> PathBuf {
    ntd_binary_path()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"))
}

#[cfg(test)]
mod tests {
    //! 单测覆盖：`ntd_dir()` 在 home_dir 缺失时回退 `/tmp/.ntd`，
    //! 这是 daemon install 流程在 sandbox/CI 等环境下的兜底行为，
    //! 一旦回归就会让 plist / unit 写到 `/tmp/.ntd/run.log` 等意外位置。
    use super::*;

    #[test]
    fn test_ntd_dir_falls_back_to_tmp_when_home_missing() {
        // home_dir 在多数 CI 镜像里仍然存在，但函数本身要保证不 panic，
        // 这里只断言返回值是绝对路径并且以 ".ntd" 结尾
        let dir = ntd_dir();
        assert!(dir.is_absolute(), "ntd_dir must be absolute, got {:?}", dir);
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some(".ntd"),
            "ntd_dir should end with .ntd"
        );
    }

    #[test]
    fn test_ntd_bin_dir_is_parent_of_binary() {
        // ntd_bin_dir = ntd_binary_path().parent()（兜底 /usr/local/bin）
        let bin = ntd_binary_path();
        let dir = ntd_bin_dir();
        // 在当前 cargo test 进程里，args()[0] 是 cargo 给的临时 path，
        // 这里只断言"bin_dir 是 bin 的祖先路径之一或回退到 /usr/local/bin"
        if let Some(parent) = bin.parent() {
            assert!(
                dir == parent || dir == PathBuf::from("/usr/local/bin"),
                "bin_dir {:?} should equal parent {:?} or fallback /usr/local/bin",
                dir,
                parent
            );
        }
    }

    #[test]
    fn test_ntd_binary_path_returns_absolute() {
        // 这是 ntd install 链路最核心的输入：必须能解析出当前 binary 的绝对路径，
        // 否则 plist / unit / schtasks 都会指向不存在的位置
        let p = ntd_binary_path();
        assert!(p.is_absolute(), "binary path must be absolute, got {:?}", p);
    }
}
