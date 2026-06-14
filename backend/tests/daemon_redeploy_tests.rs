//! Tests for the detached redeploy command spec builder.
//!
//! `build_redeploy_spec` 是纯函数,只把 (mode, script) 翻译成一段
//! (program, args),不实际 fork 进程,所以可以放心断言参数顺序,
//! 防止以后重构时把 `--user` / `--scope` / `--collect` 顺序或位置
//! 写错 —— 这正是 PR #482 修的 cgroup-detach 修复的关键。

#[cfg(test)]
#[cfg(target_os = "linux")]
mod build_redeploy_spec_tests {
    use ntd::daemon::{build_redeploy_spec, DaemonInstallMode, RedeployCommandSpec};

    #[test]
    fn test_user_mode_adds_user_flag_first() {
        let spec = build_redeploy_spec(DaemonInstallMode::User, "echo hi");

        assert_eq!(spec.program, "systemd-run");
        // --user 必须出现在第一位,systemd-run 严格要求 flag 在 command 之前
        assert_eq!(spec.args[0], "--user");
        // scope 和 collect 紧跟其后
        assert_eq!(spec.args[1], "--scope");
        assert_eq!(spec.args[2], "--collect");
        // description property 必须带上,systemctl/journalctl 里排查靠它
        assert!(
            spec.args
                .iter()
                .any(|a| a.starts_with("--property=Description=")
                    && a.contains("ntd upgrade redeploy")),
            "missing Description property, args = {:?}",
            spec.args
        );
        // KillMode=process 是这次修复里多加的 belt-and-suspenders,
        // 即使 scope 被 kill 也只杀 systemd-run 自身,不杀 sh -c 链
        assert!(
            spec.args.iter().any(|a| a.contains("KillMode=process")),
            "missing KillMode=process, args = {:?}",
            spec.args
        );
        // 末尾必须以 sh -c <script> 收尾
        let n = spec.args.len();
        assert_eq!(spec.args[n - 3], "/bin/sh");
        assert_eq!(spec.args[n - 2], "-c");
        assert_eq!(spec.args[n - 1], "echo hi");
    }

    #[test]
    fn test_system_mode_omits_user_flag() {
        // system 模式不能加 --user,否则 systemd-run 会去连用户的
        // systemd 实例,跟 ntd.service (system) 不是一个 cgroup,
        // 修复就失效了
        let spec = build_redeploy_spec(DaemonInstallMode::System, "true");

        assert_eq!(spec.program, "systemd-run");
        assert_ne!(spec.args[0], "--user", "system mode must not pass --user");
        assert_eq!(spec.args[0], "--scope");
    }

    #[test]
    fn test_unknown_mode_falls_back_to_system_path() {
        // 探测失败时宁可走 system 路径(连不上也能产生可观察的错误),
        // 也不要错误地加 --user —— 那会让用户场景看上去"对"实际"错"
        let spec = build_redeploy_spec(DaemonInstallMode::Unknown, "true");

        assert_eq!(spec.program, "systemd-run");
        assert_ne!(spec.args[0], "--user");
        assert_eq!(spec.args[0], "--scope");
    }

    #[test]
    fn test_script_is_passed_verbatim() {
        // script 里有空格、&&、引号都不应该被 spec 改写;
        // 改写是 shell 的事,spec 阶段必须 byte-for-byte 透传
        let tricky = "ntd daemon stop && ntd daemon install --force && /usr/bin/echo 'ok done'";
        let spec = build_redeploy_spec(DaemonInstallMode::User, tricky);

        assert_eq!(spec.args.last().unwrap(), tricky);
    }

    #[test]
    fn test_spec_is_cloneable_for_reuse() {
        // 派生 spec 出来传给别的模块(比如 dry-run 展示)不消耗自身
        let spec = build_redeploy_spec(DaemonInstallMode::User, "echo a");
        let cloned: RedeployCommandSpec = spec.clone();
        assert_eq!(spec, cloned);
    }

    #[test]
    fn test_args_have_no_duplicate_flags() {
        // 防止以后重构时手抖加了两遍 --scope / --collect
        let spec = build_redeploy_spec(DaemonInstallMode::User, "true");
        for flag in ["--scope", "--collect"] {
            let count = spec.args.iter().filter(|a| a.as_str() == flag).count();
            assert_eq!(count, 1, "{flag} appeared {count} times in {:?}", spec.args);
        }
    }

    /// 验证 build_redeploy_spec 的 args 布局稳定，以便
    /// spawn_detached_redeploy_nonblocking 在 --collect 后插入 --no-block。
    /// 如果布局（如 --collect 位置）变了，nonblocking 的插入逻辑会失效。
    #[test]
    fn test_redeploy_spec_layout_collect_position() {
        // 确认 --collect 在 args 中的位置是稳定的（第三个位置，0-indexed）。
        // This is important for spawn_detached_redeploy_nonblocking which inserts
        // --no-block right after --collect.
        let spec = build_redeploy_spec(DaemonInstallMode::User, "true");
        // args 布局: [--user, --scope, --collect, --property=..., ...]
        assert_eq!(
            spec.args[2], "--collect",
            "Expected --collect at index 2, got {:?}",
            spec.args
        );
    }

    #[test]
    fn test_redeploy_spec_system_layout_collect_position() {
        // System 模式下 args 布局: [--scope, --collect, --property=..., ...]
        let spec = build_redeploy_spec(DaemonInstallMode::System, "true");
        assert_eq!(
            spec.args[1], "--collect",
            "Expected --collect at index 1 for System mode, got {:?}",
            spec.args
        );
    }
}

/// 验证 build_redeploy_spec_nonblocking 能正确插入 --no-block。
///
/// build_redeploy_spec_nonblocking 现在是 redeploy.rs 的公有函数，
/// 与 spawn_detached_redeploy_nonblocking 使用同一份实现，
/// 维护者无需担心测试与实际行为不一致。
#[cfg(test)]
#[cfg(target_os = "linux")]
mod nonblocking_spec_tests {
    use ntd::daemon::build_redeploy_spec_nonblocking;
    use ntd::daemon::DaemonInstallMode;

    /// 通过公有函数 build_redeploy_spec_nonblocking 获取 args，
    /// 验证 --no-block 被插入到正确位置。
    fn build_nonblocking_spec(ntd_cmd: &str) -> Vec<String> {
        let script = format!(
            "sleep 3; {} daemon install --force; {} daemon start; rm -f /tmp/ntd.update",
            ntd_cmd, ntd_cmd
        );
        build_redeploy_spec_nonblocking(DaemonInstallMode::User, &script).args
    }

    #[test]
    fn test_nonblocking_inserts_no_block_after_collect() {
        let args = build_nonblocking_spec("/usr/bin/ntd");
        // args 布局: [--user, --scope, --collect, --no-block, --property=..., /bin/sh, -c, <script>]
        let collect_idx = args.iter().position(|a| a == "--collect").unwrap();
        assert_eq!(
            args[collect_idx + 1],
            "--no-block",
            "--no-block must be inserted immediately after --collect, got {:?}",
            args
        );

        // Verify --no-block appears exactly once
        let count = args.iter().filter(|a| a.as_str() == "--no-block").count();
        assert_eq!(
            count, 1,
            "--no-block must appear exactly once, args: {:?}",
            args
        );
    }

    #[test]
    fn test_nonblocking_preserves_all_other_args() {
        let args = build_nonblocking_spec("/usr/local/bin/ntd");
        // Must contain all the standard flags
        assert!(args.contains(&"--scope".to_string()), "Missing --scope");
        assert!(args.contains(&"--collect".to_string()), "Missing --collect");
        assert!(
            args.contains(&"--no-block".to_string()),
            "Missing --no-block"
        );
        assert!(args.contains(&"/bin/sh".to_string()), "Missing /bin/sh");
        assert!(args.contains(&"-c".to_string()), "Missing -c");

        // Verify the script is the last arg
        let expected_script = "sleep 3; /usr/local/bin/ntd daemon install --force; /usr/local/bin/ntd daemon start; rm -f /tmp/ntd.update";
        assert_eq!(args.last().unwrap(), expected_script);
    }

    #[test]
    fn test_nonblocking_no_duplicate_flags() {
        let args = build_nonblocking_spec("/usr/bin/ntd");
        for flag in ["--scope", "--collect", "--no-block"] {
            let count = args.iter().filter(|a| a.as_str() == flag).count();
            assert_eq!(count, 1, "{flag} appeared {count} times in {:?}", args);
        }
    }
}
