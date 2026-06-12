//! Tests for the detached redeploy command spec builder.
//!
//! `build_redeploy_spec` 是纯函数,只把 (mode, script) 翻译成一段
//! (program, args),不实际 fork 进程,所以可以放心断言参数顺序,
//! 防止以后重构时把 `--user` / `--scope` / `--collect` 顺序或位置
//! 写错 —— 这正是 PR #482 修的 cgroup-detach 修复的关键。

#[cfg(test)]
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
                .any(|a| a.starts_with("--property=Description=") && a.contains("ntd upgrade redeploy")),
            "missing Description property, args = {:?}",
            spec.args
        );
        // KillMode=process 是这次修复里多加的 belt-and-suspenders,
        // 即使 scope 被 kill 也只杀 systemd-run 自身,不杀 sh -c 链
        assert!(
            spec.args
                .iter()
                .any(|a| a.contains("KillMode=process")),
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
        assert_ne!(
            spec.args[0], "--user",
            "system mode must not pass --user"
        );
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
}
