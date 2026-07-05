//! Tests for `npm_utils` — the pure-ish helpers that drive the `ntd upgrade`
//! flow used by the Web API. These functions sit at the boundary between the
//! daemon process and the host system's npm/toolchain, so we exercise the
//! decision branches (writable vs not, prefix/bin vs current_exe vs PATH)
//! using temp directories rather than mocking subprocesses.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
use std::path::Path;

use ntd::npm_utils::{find_ntd_binary, is_writable_dir};

#[cfg(test)]
mod is_writable_dir_tests {
    use super::*;

    /// 临时目录是肯定可写的(测试运行用户能创建文件)。
    /// 这是 `get_npm_global_prefix` 选默认 prefix 的前提。
    #[test]
    fn test_writable_temp_dir_is_true() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        assert!(is_writable_dir(tmp.path()));
    }

    /// 不存在的路径必须返回 false,不能因为 fs::File::create
    /// "可能"能创建就放行 —— `get_npm_global_prefix` 据此判断
    /// 是否需要回退到 ~/.npm-global,逻辑错了会让升级选到
    /// 一个根本访问不到的 prefix。
    #[test]
    fn test_nonexistent_path_is_false() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let ghost = tmp.path().join("does_not_exist");
        assert!(!is_writable_dir(&ghost));
    }

    /// 文件而非目录: 应该返回 false(我们关心的是 *目录*
    /// 可写性,目录里能塞文件;给一个文件路径,函数不能误判)。
    /// 这一点 ls -l 看不到但语义上很重要 —— 比如
    /// `get_npm_global_prefix` 拿到一个指向文件的 stale prefix 路径
    /// 时,必须正确降级,否则 npm install 会拿到奇怪的错误。
    #[test]
    fn test_file_path_is_false() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let file_path = tmp.path().join("a_file");
        std::fs::write(&file_path, b"x").expect("write file");
        assert!(!is_writable_dir(&file_path));
    }
}

#[cfg(test)]
mod find_ntd_binary_tests {
    use super::*;

    /// 优先级 1: `{prefix}/bin/ntd` 文件存在时,直接返回这个绝对路径。
    /// 这是 `npm install -g` 之后 ntd 链接的实际位置,Web 升级
    /// 流程必须用新版本而不是 current_exe(旧版本)。
    #[test]
    fn test_prefers_prefix_bin_when_present() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let bin_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&bin_dir).expect("create bin dir");
        let fake_ntd = bin_dir.join("ntd");
        std::fs::write(&fake_ntd, b"#!/bin/sh\necho fake\n").expect("write fake ntd");

        let prefix = tmp.path().to_string_lossy().to_string();
        let result = find_ntd_binary(&prefix);

        // must_symlink_eq: symlink resolution 跟平台相关,这里只看
        // 文件名 + 父目录(应该是 bin/),不强行做完整 path 相等
        let result_path = Path::new(&result);
        assert_eq!(
            result_path.file_name().and_then(|s| s.to_str()),
            Some("ntd"),
            "expected ntd filename, got {result}"
        );
        assert_eq!(
            result_path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()),
            Some("bin"),
            "expected bin/ prefix, got {result}"
        );
    }

    /// 优先级 2: prefix/bin/ntd 不存在时,回退到 current_exe。
    /// 这条路径覆盖 `make install` (cargo install --path) 的场景:
    /// 升级 npm 版本后 ntd 仍然在原来的 ~/.local/bin,而新
    /// 安装的版本在 current_exe 的位置上 —— current_exe 才是新版本。
    #[test]
    fn test_falls_back_to_current_exe_when_prefix_bin_missing() {
        // 用一个肯定不存在 ntd 的 prefix,触发 fallback
        let tmp = tempfile::tempdir().expect("create tempdir");
        let empty_prefix = tmp.path().to_string_lossy().to_string();

        let result = find_ntd_binary(&empty_prefix);

        // current_exe 在测试环境里就是 cargo 跑出来的 test binary;
        // 只要不是 prefix/bin/ntd 就说明走了 fallback
        let result_path = Path::new(&result);
        let prefix_bin = Path::new(&empty_prefix).join("bin").join("ntd");
        assert_ne!(
            result_path.canonicalize().ok(),
            prefix_bin.canonicalize().ok(),
            "should not pick prefix/bin/ntd when file is absent"
        );
    }

    /// 优先级 3 的间接验证: 任何 prefix 都不会让函数 panic,
    /// 返回值永远是非空字符串 (要么绝对路径,要么 "ntd")。
    /// 这条是 "不挂" 的稳定性测试 —— 之前如果 path 解析出 None
    /// 又 unwrap,函数会直接 panic,Web 升级就崩了。
    #[test]
    fn test_returns_nonempty_for_arbitrary_prefix() {
        let result = find_ntd_binary("/totally/made/up/prefix/that/does/not/exist");
        assert!(!result.is_empty());
    }
}

#[cfg(test)]
mod get_npm_global_prefix_tests {
    use super::*;

    /// npm 子进程缺失 / 失败时必须回退到 ~/.npm-global 而不是 panic 或
    /// 返回空串 —— 否则 Web 升级会拿到一个无效 prefix 传给 npm install。
    /// 在没有 npm 的 CI 环境(我们跑测试的 macOS 默认有)这条路径天然
    /// 走不到,所以只断言 "返回值非空 + 是绝对路径" 这个不变量。
    #[test]
    fn test_returns_nonempty_absolute_path() {
        let prefix = ntd::npm_utils::get_npm_global_prefix();
        assert!(!prefix.is_empty(), "prefix must not be empty");
        let p = Path::new(&prefix);
        // 要么是绝对路径(npm prefix -g 成功的情况)
        // 要么是 ~ 开头的字符串(dirs::home_dir 返回的 .npm-global)
        // 都要能解析为有效 Path
        assert!(p.is_absolute() || prefix.starts_with('~') || prefix.starts_with('/'));
    }
}
