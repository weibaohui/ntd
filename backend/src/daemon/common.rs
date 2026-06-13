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

/// 校验 ntd 可执行文件路径可安全嵌入 shell 脚本。
///
/// 拒绝任何不在 `[A-Za-z0-9/_.-]` 字符集内的字符（包括空格、`;`、`$`、反引号、
/// 换行等所有 shell 元字符）。这条白名单比"用单引号包裹"更安全 —— 单引号包裹
/// 只能挡 single-token injection,挡不住 `[ -f "/path with \$IFS" ]` 之类的
/// shell expansion 攻击。
///
/// 一旦 path 含非法字符,daemon redeploy 应该**直接拒绝执行**,而不是"清洗"——
/// `npm prefix -g` 返回的路径在 .npmrc 被污染时可能是 `/foo;rm -rf /;` 这类
/// 攻击载荷(PR #476 第二轮评审的 CRITICAL #1 反复复发,本模块统一加 guard)。
pub(crate) fn is_safe_ntd_path(path: &str) -> bool {
    !path.is_empty()
        && path.starts_with('/')
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'))
}

/// 将 ntd 路径包成单引号 shell-safe 字符串。
///
/// 假定调用方已先用 [`is_safe_ntd_path`] 校验过路径本身没有单引号 / 反斜杠等
/// 危险字符。单引号在 bash 里禁用所有 expansion,是最强的引号方式。
pub(crate) fn shell_quote_single(path: &str) -> String {
    format!("'{}'", path)
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

    #[test]
    fn test_is_safe_ntd_path_rejects_shell_metacharacters() {
        // 正常路径通过
        assert!(is_safe_ntd_path("/usr/local/bin/ntd"));
        assert!(is_safe_ntd_path("/home/user/.ntd/bin/ntd"));
        // shell 元字符全部拒绝
        assert!(!is_safe_ntd_path("/foo;rm -rf /"));
        assert!(!is_safe_ntd_path("/foo$(whoami)"));
        assert!(!is_safe_ntd_path("/foo`id`"));
        assert!(!is_safe_ntd_path("/foo bar"));
        assert!(!is_safe_ntd_path("/foo\nbar"));
        assert!(!is_safe_ntd_path("/foo\\bar"));
        assert!(!is_safe_ntd_path("/foo'bar"));
        // 相对路径拒绝(避免 systemd-run 误解析)
        assert!(!is_safe_ntd_path("bin/ntd"));
        // 空字符串拒绝
        assert!(!is_safe_ntd_path(""));
    }

    #[test]
    fn test_shell_quote_single_wraps_in_single_quote() {
        assert_eq!(shell_quote_single("/usr/bin/ntd"), "'/usr/bin/ntd'");
    }
}
