//! DirectExecutorSession：飞书默认响应（message_debounce）与 wiki chat（blackboard）
//! 两条直连执行通路共用的「spawn 子进程 + 逐行流读 + 超时 kill」骨架（096-W4-6）。
//!
//! 收口边界（详见 docs/design/101 W4-6 段）：
//! 本模块只管进程生命周期与 I/O：build 命令（command_args_with_session、piped stdio、
//! current_dir）→ spawn 普通 child → 关 stdin → select! 逐行读 stdout/stderr →
//! 超时 kill → wait → 返回原始文本与退出成功位。
//!
//! 逐行「语义」（A 的 EventPipeline 解析 + 私聊直推 vs B 的裸 parse_output_line +
//! WikiChatOutput）经 `on_line` 闭包交还调用方——session 不持有通路知识。
//!
//! 主执行链路（executor_service/spawn_lifecycle：进程组 + LogFlusher + cancel +
//! worktree）是架构独立的生命周期模块，非本骨架的平行副本，刻意不迁移（doc 101 逃生口）。
//!
//! timeout_secs == 0 表示不限时（与 A 的 direct_executor_timeout、C 的
//! configure_timeout_sleep 三方既有语义一致：0 = 禁用）。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

// 106：group_spawn/kill 整组杀进程的扩展 trait（与主链路 spawn_lifecycle 同源）。
use command_group::AsyncCommandGroup;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::adapters::CodeExecutor;

/// 直连执行 session 的 spawn 配置（A/B 调用方各自组装，通路差异不进本结构）。
pub(crate) struct SessionSpawnConfig {
    /// 目标执行器：提供 executable_path 与 command_args_with_session
    pub(crate) executor: Arc<dyn CodeExecutor>,
    /// 发给执行器的完整消息文本
    pub(crate) message: String,
    /// 子进程工作目录
    pub(crate) cwd: PathBuf,
    /// 续接会话 id（None = 新会话）；有无同时决定 is_resume 的推导
    pub(crate) session_id: Option<String>,
    /// 超时秒数；0 = 不限时
    pub(crate) timeout_secs: u64,
    /// 仅用于 tracing 关联的标识（各通路传自己的 task_id），不参与任何行为
    pub(crate) log_tag: String,
}

/// session 结束时交还调用方的原始产出；logs 由调用方在 on_line 闭包里自行累积。
/// Debug 仅供测试 unwrap 断言用。
#[derive(Debug)]
pub(crate) struct SessionOutcome {
    /// stdout 原文（逐行 join），供错误诊断与结果兜底
    pub(crate) raw_stdout: String,
    /// stderr 原文（逐行 join），供非零退出时拼进错误消息
    pub(crate) raw_stderr: String,
    /// 子进程是否成功退出（status.success()）
    pub(crate) success: bool,
    /// 退出码（被信号杀死时为 None）——A 通路非零退出的用户文案要报「退出码 N」
    pub(crate) exit_code: Option<i32>,
}

/// typed error：A/B 调用方各自映射回原有错误文案，不共用字符串（避免通路语义耦合）。
/// Debug 仅供测试断言用。
#[derive(Debug)]
pub(crate) enum SessionError {
    /// 子进程启动失败
    Spawn(std::io::Error),
    /// 超时（已 kill 回收）；携带 stderr 供 B 的超时文案拼接
    Timeout { secs: u64, stderr: String },
    /// 等待子进程退出失败
    WaitFailed(String),
    /// 读 stdout 出错；携带已收集的 stderr 供错误消息拼接
    StdoutReadFailed {
        source: std::io::Error,
        stderr: String,
    },
}

/// A/B 平行实现（~286 行）的统一骨架持有者。无字段：纯命名空间性质的入口。
pub(crate) struct DirectExecutorSession;

impl DirectExecutorSession {
    /// spawn + 逐行流读（on_line 回调交还调用方）+ 超时 kill + wait 的统一骨架。
    ///
    /// 成功路径：stdout EOF 后 wait 子进程，返回原始文本与退出位；
    /// 超时路径：kill 子进程后返回 `SessionError::Timeout`（避免孤儿/僵尸进程）。
    pub(crate) async fn spawn_and_stream<F>(
        config: SessionSpawnConfig,
        mut on_line: F,
    ) -> Result<SessionOutcome, SessionError>
    where
        F: FnMut(&str),
    {
        // ── 构建并启动子进程（收敛 A 的 spawn_executor_child 与 B 的内联段）──
        let SessionSpawnConfig {
            executor,
            message,
            cwd,
            session_id,
            timeout_secs,
            log_tag,
        } = config;
        // is_resume 由是否有 session_id 推导：有 session 则让执行器续上下文而非新开会话
        let is_resume = session_id.is_some();
        let command_args =
            executor.command_args_with_session(&message, session_id.as_deref(), is_resume);
        let program = executor.executable_path();
        tracing::info!(
            "[session] spawning {}: {:?} (cwd={:?}, tag={})",
            program,
            command_args,
            cwd,
            log_tag
        );

        // 三个 stdio 全 piped：stdout/stderr 由骨架读取，stdin 随即 take+drop 关闭
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(&command_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped())
            .current_dir(&cwd);
        // 106 体检：group_spawn 创建独立进程组，kill 时整组杀——此前裸 spawn 只
        // SIGKILL 直接子进程，执行器派生的孙进程（构建/测试子进程）成孤儿继续跑。
        // 与主链路 spawn_lifecycle 的 group_spawn 同策略。
        let mut child = cmd
            .group_spawn()
            .map_err(SessionError::Spawn)?;

        // take() 并 drop 关闭 stdin，避免 CLI 进程执行完后挂起等待 EOF。
        // 两条通路均非 worktree 场景，不预写 payload（pi 的 "y" 应答只在切
        // worktree 目录时才有意义，乱写会污染 stdin——A/B 原注释同源结论）。
        // group child 的 stdio 需经 inner() 访问（AsyncGroupChild 包了一层）。
        drop(child.inner().stdin.take());

        // piped 模式下 take 理论上必为 Some；防御性兜底为空流（立即 EOF → 走 wait 路径，
        // 返回空 raw 文本），与 A 原实现的「无 stdout → 空结果」降级语义一致。
        let stdout: Box<dyn tokio::io::AsyncRead + Unpin + Send> = match child.inner().stdout.take() {
            Some(s) => Box::new(s),
            None => Box::new(tokio::io::empty()),
        };
        let stderr: Box<dyn tokio::io::AsyncRead + Unpin + Send> = match child.inner().stderr.take() {
            Some(s) => Box::new(s),
            None => Box::new(tokio::io::empty()),
        };

        stream_lines_until_exit(child, stdout, stderr, timeout_secs, &log_tag, &mut on_line).await
    }
}

/// select! 主循环：逐行读 stdout（回调调用方）/ stderr（收集）/ 超时（kill），
/// 直到 stdout EOF 后 wait 子进程返回。骨架逐字源自 B（blackboard）——最自洽的一份。
///
/// 函数体超 50 行豁免（CLAUDE.md「强行拆分将导致数据碎片化」）：tokio::select! 的
/// 三个分支是宏内联状态机，共享 raw_stdout_lines / raw_stderr_lines / child 的可变
/// 借用，把分支抽成独立函数需把这套状态在每次轮转间穿线传递。
async fn stream_lines_until_exit<F>(
    mut child: command_group::AsyncGroupChild,
    stdout: Box<dyn tokio::io::AsyncRead + Unpin + Send>,
    stderr: Box<dyn tokio::io::AsyncRead + Unpin + Send>,
    timeout_secs: u64,
    log_tag: &str,
    on_line: &mut F,
) -> Result<SessionOutcome, SessionError>
where
    F: FnMut(&str),
{
    // BufReader 逐行读：executor 输出天然按行组织（JSONL / 人类可读日志都是）
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    // 收集容器：raw 文本按行 push、结束时 join，避免反复 String concat
    let mut raw_stdout_lines: Vec<String> = Vec::new();
    let mut raw_stderr_lines: Vec<String> = Vec::new();
    // 106 体检：行数上限——执行器失控刷输出（死循环打印）时无上限缓冲会让内存随
    // 输出线性增长，timeout_secs=0（不限时）时无任何兜底。超限丢弃后续行并记 warn，
    // stdout 回调同样停发（结果已无意义），主循环继续消费管道防写端阻塞。
    const MAX_LINES_PER_STREAM: usize = 10_000;
    let (mut stdout_dropped, mut stderr_dropped) = (false, false);

    // 0 = 不限时：用极大 duration（u64::MAX 秒 ≈ 5.8 亿年）模拟「永不超时」，
    // select! 该分支永不命中（与 C 的 configure_timeout_sleep 同一手法）
    let effective_secs = if timeout_secs == 0 { u64::MAX } else { timeout_secs };
    let timeout_fut = tokio::time::sleep(Duration::from_secs(effective_secs));
    tokio::pin!(timeout_fut);

    // on_line panic 隔离（评审修复）：A 原实现把逐行解析放在独立 tokio::spawn 任务里，
    // panic 被 JoinHandle 降级为空结果，不会击穿 debounce 任务；收敛后闭包内联进本循环，
    // 用 catch_unwind 恢复同等隔离——且比原来更强：panic 前已解析的 logs 由调用方保留，
    // raw 收集不受影响。panic 后置 poisoned，后续行只收集不再回调（原「任务死一次即止」语义）。
    let mut on_line_poisoned = false;
    // 排空模式（评审修复）：计时器到点时若进程其实已退出、只是管道还在排空，
    // 不误判超时（A 原实现的超时只包 child.wait()，不存在排空误判）。
    let mut drain_mode = false;
    // stderr EOF/读失败后禁用该分支（评审修复）：next_line() 在 EOF 后持续立即
    // 返回 Ok(None)，不禁用会让 select! 在 stdout 未结束时空转占满 CPU
    let mut stderr_done = false;

    loop {
        tokio::select! {
            // 读取下一行 stdout：这是主数据流，EOF 即子进程产出完毕
            line_result = stdout_reader.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        if !stdout_dropped {
                            if raw_stdout_lines.len() >= MAX_LINES_PER_STREAM {
                                stdout_dropped = true;
                                tracing::error!(
                                    "[session] stdout exceeded {} lines, dropping remaining output (tag={})",
                                    MAX_LINES_PER_STREAM, log_tag
                                );
                            } else {
                                raw_stdout_lines.push(line.clone());
                            }
                        }
                        // 逐行语义（解析 + 事件推送）交还调用方闭包；
                        // panic 只废掉回调本身，骨架继续读流（见 on_line_poisoned 注释）
                        if !on_line_poisoned && !stdout_dropped {
                            let callback = std::panic::AssertUnwindSafe(|| on_line(&line));
                            if let Err(payload) = std::panic::catch_unwind(callback) {
                                on_line_poisoned = true;
                                let reason = payload
                                    .downcast_ref::<&str>()
                                    .map(|s| (*s).to_string())
                                    .or_else(|| payload.downcast_ref::<String>().cloned())
                                    .unwrap_or_else(|| "unknown".to_string());
                                tracing::error!(
                                    "[session] on_line callback panicked, disabling callback (tag={}, reason={})",
                                    log_tag, reason
                                );
                            }
                        }
                    }
                    // stdout 读完了：等子进程退出并打包返回。
                    Ok(None) => {
                        // 先排空 stderr 再等退出。
                        // 106：select! 轮转顺序不保证 stderr 先于 stdout EOF 被消费——
                        // 进程退出时两个管道同时关闭，缓冲里的 stderr 行可能还没读到，
                        // 直接返回会丢诊断信息（全量测试并行时该窗口被放大，偶发断言失败）。
                        // 5s 兜底：不为凑齐 stderr 无限阻塞（与「拿到多少算多少」语义兼容）。
                        // while let 形态：Ok(Some(line)) 持续排空，其余（EOF/读失败/超时）终止。
                        if !stderr_done {
                            while let Ok(Ok(Some(line))) = tokio::time::timeout(
                                Duration::from_secs(5),
                                stderr_reader.next_line(),
                            )
                            .await
                            {
                                if !stderr_dropped {
                                    if raw_stderr_lines.len() >= MAX_LINES_PER_STREAM {
                                        stderr_dropped = true;
                                    } else {
                                        raw_stderr_lines.push(line);
                                    }
                                }
                            }
                        }
                        // 106 体检：wait 包 5 分钟超时——select! 的计时器对本分支不再
                        // 生效，子进程关了 stdout 但自身不退出（或 exec 后挂起）时
                        // wait 会永久阻塞。超时后按整组 kill 回收再上抛，不留孤儿。
                        let wait_result =
                            tokio::time::timeout(Duration::from_secs(300), child.wait()).await;
                        let status = match wait_result {
                            Ok(Ok(s)) => s,
                            Ok(Err(e)) => {
                                let _ = child.kill().await;
                                return Err(SessionError::WaitFailed(e.to_string()));
                            }
                            Err(_) => {
                                tracing::error!(
                                    "[session] child did not exit after stdout EOF within 300s, killing group: tag={}",
                                    log_tag
                                );
                                let _ = child.kill().await;
                                let stderr = raw_stderr_lines.join("\n");
                                return Err(SessionError::Timeout { secs: 300, stderr });
                            }
                        };
                        let raw_stderr = raw_stderr_lines.join("\n");
                        let raw_stdout = raw_stdout_lines.join("\n");
                        tracing::info!(
                            "[session] finished: tag={}, exit_code={:?}, stdout_len={}, stderr_len={}",
                            log_tag, status.code(), raw_stdout.len(), raw_stderr.len()
                        );
                        if !raw_stderr.is_empty() {
                            tracing::warn!("[session] stderr: tag={}, stderr={}", log_tag, raw_stderr);
                        }
                        // 退出位与退出码一次性取出后再构造返回值：status 后续不再使用
                        let success = status.success();
                        let exit_code = status.code();
                        return Ok(SessionOutcome { raw_stdout, raw_stderr, success, exit_code });
                    }
                    Err(e) => {
                        // stdout 读失败即放弃本次执行：先 kill 回收，再上抛（评审修复，
                        // 与超时分支同姿势——不留孤儿进程）
                        let _ = child.kill().await;
                        let stderr = raw_stderr_lines.join("\n");
                        return Err(SessionError::StdoutReadFailed { source: e, stderr });
                    }
                }
            }
            // 读取下一行 stderr：只收集不回调（stderr 是诊断通道，无通路语义）；
            // EOF/读失败后经 stderr_done 禁用本分支，防忙循环
            line_result = stderr_reader.next_line(), if !stderr_done => {
                match line_result {
                    Ok(Some(line)) => {
                        if !stderr_dropped {
                            if raw_stderr_lines.len() >= MAX_LINES_PER_STREAM {
                                stderr_dropped = true;
                                tracing::error!(
                                    "[session] stderr exceeded {} lines, dropping remaining output (tag={})",
                                    MAX_LINES_PER_STREAM, log_tag
                                );
                            } else {
                                raw_stderr_lines.push(line);
                            }
                        }
                    }
                    // stderr EOF：关闭本分支，继续等 stdout 主流
                    Ok(None) => {
                        stderr_done = true;
                    }
                    // stderr 读失败不致命：关闭本分支，丢掉诊断信息，主流继续（B 原语义）
                    Err(e) => {
                        tracing::warn!("[session] stderr read error: tag={}, error={}", log_tag, e);
                        stderr_done = true;
                    }
                }
            }
            // 超时：以「进程是否退出」为准（评审修复）。进程已退出只是管道还在排空时
            // 切入排空模式（禁用计时分支），读完 stdout EOF 正常返回，不误判超时；
            // 真未退出才 kill 回收，错误携带已收集的 stderr 供调用方拼文案。
            _ = &mut timeout_fut, if !drain_mode => {
                if let Ok(Some(_status)) = child.try_wait() {
                    tracing::info!(
                        "[session] timer fired but child already exited, draining remaining output: tag={}",
                        log_tag
                    );
                    drain_mode = true;
                    continue;
                }
                tracing::error!(
                    "[session] timed out after {}s, killing child: tag={}",
                    timeout_secs, log_tag
                );
                let _ = child.kill().await;
                let stderr = raw_stderr_lines.join("\n");
                return Err(SessionError::Timeout { secs: timeout_secs, stderr });
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::models::{ExecutorType, ParsedLogEntry};

    /// 最小 CodeExecutor 桩：program/args 固定，仅实现无默认值的 trait 方法。
    /// command_args_with_session 走默认实现（委托 command_args），让 session 的
    /// session_id/is_resume 分支不被桩的额外行为干扰。
    struct StubExecutor {
        program: String,
        args: Vec<String>,
    }

    impl CodeExecutor for StubExecutor {
        fn executor_type(&self) -> ExecutorType {
            // 仅作 registry 键与日志标识，session 骨架不依赖具体变体
            ExecutorType::default()
        }
        fn executable_path(&self) -> &str {
            &self.program
        }
        fn command_args(&self, _message: &str) -> Vec<String> {
            self.args.clone()
        }
        fn parse_output_line(&self, _line: &str) -> Option<ParsedLogEntry> {
            None
        }
        fn get_model(&self) -> Option<String> {
            None
        }
    }

    /// 组装最小 config：cwd 用临时目录（sh/echo 对目录无要求），message 空串
    /// （桩的 command_args 忽略 message，参数由 args 显式控制断言面）。
    fn make_config(program: &str, args: &[&str], timeout_secs: u64) -> SessionSpawnConfig {
        SessionSpawnConfig {
            executor: Arc::new(StubExecutor {
                program: program.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
            }),
            message: String::new(),
            cwd: std::env::temp_dir(),
            session_id: None,
            timeout_secs,
            log_tag: "test".to_string(),
        }
    }

    /// 成功路径：stdout 逐行进 on_line 回调、raw_stdout 为行 join、success=true。
    #[tokio::test]
    async fn test_spawn_and_stream_成功路径逐行回调并返回success() {
        let mut lines_seen: Vec<String> = Vec::new();
        let outcome = DirectExecutorSession::spawn_and_stream(
            make_config("/bin/sh", &["-c", "echo line1; echo line2"], 10),
            |line| lines_seen.push(line.to_string()),
        )
        .await
        .unwrap();

        assert_eq!(lines_seen, vec!["line1".to_string(), "line2".to_string()]);
        assert_eq!(outcome.raw_stdout, "line1\nline2");
        assert_eq!(outcome.raw_stderr, "");
        assert!(outcome.success);
        assert_eq!(outcome.exit_code, Some(0), "正常退出应为退出码 0");
    }

    /// 启动失败：程序不存在 → SessionError::Spawn（调用方据此映射各自文案）。
    #[tokio::test]
    async fn test_spawn_and_stream_程序不存在返回spawn失败() {
        let result =
            DirectExecutorSession::spawn_and_stream(make_config("/nonexistent/definitely-not-here", &[], 10), |_| {})
                .await;
        assert!(matches!(result, Err(SessionError::Spawn(_))));
    }

    /// 超时路径：kill 子进程并返回 Timeout{secs}，已读到的 stderr 随错误带出
    /// （B 通路超时文案需要拼 stderr）。
    #[tokio::test]
    async fn test_spawn_and_stream_超时kill返回timeout携带stderr() {
        let result = DirectExecutorSession::spawn_and_stream(
            make_config("/bin/sh", &["-c", "echo boom >&2; sleep 30"], 1),
            |_| {},
        )
        .await;
        match result {
            Err(SessionError::Timeout { secs, stderr }) => {
                assert_eq!(secs, 1);
                assert!(stderr.contains("boom"), "超时错误应携带已收集的 stderr");
            }
            other => panic!("应为 Timeout，实际为其他结果（ok={}）", other.is_ok()),
        }
    }

    /// stderr 捕获：stdout/stderr 同时产出时两路各自收集不串流。
    #[tokio::test]
    async fn test_spawn_and_stream_双流分流捕获不串流() {
        let outcome = DirectExecutorSession::spawn_and_stream(
            make_config("/bin/sh", &["-c", "echo out; echo err 1>&2"], 10),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(outcome.raw_stdout, "out");
        assert_eq!(outcome.raw_stderr, "err");
        assert!(outcome.success);
    }

    /// timeout_secs=0 表示不限时：正常完成的进程不受「立即超时」误伤
    /// （u64::MAX sleep 永不命中的语义回归锚点）。
    #[tokio::test]
    async fn test_spawn_and_stream_超时0不限时正常完成() {
        let outcome = DirectExecutorSession::spawn_and_stream(
            make_config("/bin/sh", &["-c", "sleep 1; echo done"], 0),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(outcome.raw_stdout, "done");
        assert!(outcome.success);
    }

    /// on_line panic 隔离（评审修复的回归锚点）：回调 panic 后骨架不击穿，
    /// 正常返回完整 raw 文本，回调仅触发到 panic 行为止。
    #[tokio::test]
    async fn test_spawn_and_stream_回调panic不击穿骨架() {
        let mut lines_seen: Vec<String> = Vec::new();
        let outcome = DirectExecutorSession::spawn_and_stream(
            make_config("/bin/sh", &["-c", "echo l1; echo l2; echo l3"], 10),
            |line| {
                // 第二行模拟解析器 panic：骨架应隔离它并继续收集
                if line == "l2" {
                    panic!("模拟解析器 panic");
                }
                lines_seen.push(line.to_string());
            },
        )
        .await
        .unwrap();
        assert_eq!(lines_seen, vec!["l1".to_string()], "panic 行之后的回调应被禁用");
        assert_eq!(outcome.raw_stdout, "l1\nl2\nl3", "raw 收集不受回调 panic 影响");
        assert!(outcome.success);
    }

    /// 非零退出码：success=false（调用方按各自语义决定是否视为失败）。
    #[tokio::test]
    async fn test_spawn_and_stream_非零退出success为false() {
        let outcome = DirectExecutorSession::spawn_and_stream(
            make_config("/bin/sh", &["-c", "exit 3"], 10),
            |_| {},
        )
        .await
        .unwrap();
        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, Some(3), "exit 3 的退出码应透传给调用方");
    }
}
