# 101 — 直连执行 session 收敛（096-W4-6）实现总结

> 对应设计：`docs/design/101-096-W4剩余项实施设计.md` W4-6 段。
> 分支：`refactor/096-w4-6-direct-executor-session`。

## 做了什么

三处「spawn 子进程 + 流式读 stdout/stderr + 超时控制」实现中，A（飞书默认响应）与
B（wiki chat）两处真平行（~286 行同构骨架）收敛进新建的
`backend/src/services/executor_session.rs::DirectExecutorSession`：

| 通 路 | 迁移前 | 迁移后 |
|---|---|---|
| B `blackboard::spawn_executor_for_chat_streaming` | 单函数 154 行内联 select! | 组装 config + on_line 闭包 + 错误映射（~70 行） |
| A `message_debounce::handle_default_response_executor` | spawn/await/collect/stream 等 6 个本地 helper + StdoutStreamResult/ExecutorOutput 中间结构 | 一次 `spawn_and_stream` 调用；on_line 闭包承接 EventPipeline 解析 + 私聊直推 |
| C `executor_service/spawn_lifecycle` | — | **不迁移**（见下） |

净变化：B 迁移 commit +447/−172；A 迁移 commit +147/−329。

## 关键设计

- **session 只管进程生命周期与 I/O**：build 命令（`command_args_with_session` + piped
  stdio + current_dir）→ spawn → 关 stdin → select! 逐行读 → 超时 kill → wait。
  逐行「语义」（A 的 EventPipeline + 直推 vs B 的裸 parse_output_line + WikiChatOutput）
  经 `on_line: FnMut(&str)` 闭包交还调用方——session 不持有通路知识（SRP）。
- **typed error**（`SessionError`）：A/B 各自映射回原有错误文案，不共用字符串。
- **`timeout_secs == 0` = 不限时**：session 内用极大 duration 表达，与 A/C 既有语义一致。
- **C 保留（doc 101 逃生口）**：spawn_lifecycle 是进程组 + LogFlusher + cancel +
  worktree 的独立生命周期模块（#660/093-B3 重构产物），非 A/B 的平行副本；强行收敛
  要么阉割 C 语义要么把 session 撑成 god 对象。PR 如实声明。

## 行为微变（三处，均在设计文档声明）

1. A 的 stderr 从「退出后 read_to_end」改为「select! 内联读」——顺带消除 stderr
   管道写满 64KB 时子进程阻塞的潜在死锁。
2. A 的 stdout 读错误从「静默结束循环」改为上抛 `SessionError::StdoutReadFailed`
   （用户可见的「读取进程输出失败」提示）。
3. A 的「开始处理」回执从 spawn 成功后提前到 spawn 前——仅失败场景多收一条开始
   提示（紧随错误提示，不产生误导）。

## 验证

- `cargo clippy --all-targets -- -D warnings` 零告警；`cargo test` 全量 28 套件
  通过（含 executor_session 新增 7 个单测：成功/启动失败/超时 kill 携带 stderr/
  stderr 不串流/不限时/非零退出/exit_code 透传）。
- wiki chat 真实通路冒烟（dev 实例 ws1，claude 执行器）：
  `POST /api/v1/workspaces/1/wiki/chat` → `{"content":"收到","success":true,"duration_secs":6}`；
  `backend.dev.log` 确认 `[session] spawning` / `[session] finished: exit_code=Some(0)`
  两条骨架 tracing 落印。
- 飞书通路无凭证环境，靠既有单测（executor_feedback_tests 等）+ 错误文案逐字保留护航。
