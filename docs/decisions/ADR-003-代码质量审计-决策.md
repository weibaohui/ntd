# 代码质量审计报告（2026-07）

审计范围：后端 Rust（~73k 行）+ 前端 TS（~36k 行）。
方法：5 个维度并行子代理扫描（并发/async、错误处理/panic、DB/SQL、资源/进程、前端 React），高危项由人工读源码逐条复核确认。

> 状态标记：✅ 已人工读源码确认属实；🟡 子代理报告、可信但未独立复核。

严重度：HIGH（确定性 bug 或可触发的严重后果）/ MED（特定条件下触发）/ LOW（理论/边缘）。

> **修复进度**（每项一个 PR，复杂项先讨论）：
> - ✅ #7 已修 — PR #861（page=0 下溢 panic + 分页纯函数 + 单测）
> - ✅ #2 已修 — PR #862（**复核修正**：方法从未被调用，删死代码；原"竞态"不可触发）
> - ✅ #5 已修 — PR #863（BlackboardPage 工作区切换竞态，latest-wins 守卫）
> - ✅ #13 已修 — PR #864（前端 3 处 id 切换竞态，latest-wins 守卫）
> - ✅ #8 已修 — PR #865（unwrap_or_default 吞 DB 错误 → ? 传播，含 agent 漏报的第二处）
> - ✅ #1 已修 — PR #866（duplicate_loop goto 重映射到新 step id + 回归测试）
> - ✅ #4 已修 — PR #867（feishu-history-fetcher 复用 Client + 缓存 open_id）
> - ✅ #12 已修 — PR #868（config RwLock 中毒改 into_inner，避免级联 500）
> - ⏭️ #9 跳过（MED，#1 已修确定性 bug；加事务需重构 19 个 helper 调用点，性价比低）
> - ⏭️ meta 跳过（CI 加 clippy+test：当前 git 凭据缺 workflow scope 无法推送）
> - ⏭️ #6 跳过（MED，触发需「用户发消息的瞬间恰逢 60s history_fetcher tick 重放同一 chat」，窗口极窄）
> - ⏭️ #3 / #10 / #11 暂不做（进程生命周期一组：shutdown 孤儿 / timeout=0 永久孤儿 / panic 路径泄漏；涉及信号处理与 RAII 重构，留待后续排期）
>
> **本轮收尾**：8 项已修（#1/#2/#4/#5/#7/#8/#12/#13），4 项跳过/暂缓（#6/#9/meta/#3-10-11）。剩余进程生命周期一组为已知风险，建议后续单独排期。

---

## 🔴 HIGH

### 1. ✅ `duplicate_loop` 复制的 goto 引用指向源 loop 的步骤
**位置**：`backend/src/db/loop_.rs:441-442`

```rust
self.create_loop_step(new_loop.id, ..., &s.on_success,
    s.success_goto_step_id,   // ← 源 loop 的 step ID，未重映射
    &s.on_rating_fail,
    s.fail_goto_step_id, ...)  // ← 同上
```

新 step 拿到全新自增 ID，但 goto 字段原样写入源 loop 的旧 ID → 副本的 `goto` 分支指向不存在或错误的步骤。同文件 `batch_copy_loops_to_workspace`（:254-313）正确做了两遍 `old_to_new` 重映射，`duplicate_loop` 漏了这套修整。且整段无事务，中途失败留半份副本。

**触发**：UI「复制为新版本」一个带 goto 分支的 loop → 副本运行到 goto 时「step not found」或跳错分支。
**修法**：照搬 `batch_copy` 的两遍重映射；整段包进 `self.conn.begin()` 事务。
**复杂度**：中（有现成模式可抄，但需同时加事务）。

### 2. ✅ `take_pending_record_ids` 跳过队列锁（复核修正：死代码，已删除）
**位置**：`backend/src/db/blackboard.rs:239-269`（已删除）

初判：`take_pending_record_ids` 未取 `queue_lock`，与 `append_pending_record_id` 竞态会丢记录（HIGH）。

**复核修正**：该方法**从未被调用**——实际 flush 路径（`executor_service/completion.rs:544`）用 `get_blackboard` 非破坏性读 + `remove_specific_pending_record_ids` 精准移除，注释显式写「不用 take_pending_record_ids」。故原竞态**无法触发**（初判为 false positive，代理未查调用方）。`pub` 方法在 lib crate 不被 `dead_code` lint 标记，静默残留。

**实际修法**（PR #862）：直接删除该死代码方法（其「读快照→写 `[]`」非原子，若被未来代码误用会丢记录，删除以消除隐患），并修正 `remove_specific_pending_record_ids` 与 `handlers/mod.rs` 中指向它的过时 rustdoc/注释引用。
**复杂度**：低（删除 + 引用清理）。

### 3. ✅ shutdown 时 executor 子进程变孤儿
**位置**：`backend/src/main.rs:658` + `backend/src/executor_service/spawn_lifecycle.rs:208-216`

```rust
axum::serve(listener, app).await           // 无 with_graceful_shutdown / ctrl_c
cmd.args(...).stdout(piped())...           // 无 .kill_on_drop(true)
```

daemon 收 SIGTERM/SIGINT 时父进程直接退出，executor 子进程（Claude Code/codex…）在自己的进程组里存活，依附 PID 1 继续跑——持续消耗 token、占着 worktree。`cleanup_orphan_execution_records`(:905) 只把 DB 行标 failed，不杀 PID。Linux systemd `KillMode=mixed` 能兜底，**macOS launchd 和 `make dev` 完全暴露**。

**修法**：`build_executor_command` 加 `.kill_on_drop(true)`；`axum::serve` 加 `.with_graceful_shutdown(...)` 配 ctrl_c，shutdown 时遍历 TaskManager 杀掉所有运行中的子进程组。
**复杂度**：高（涉及进程生命周期、信号、运行中任务清理，需设计讨论）。

### 4. ✅ `reqwest::Client::new()` 每请求新建 + bot open_id 每条消息重解析
**位置**：`backend/src/services/feishu_history_fetcher.rs:466,245`

```rust
let client = reqwest::Client::new();   // resolve_bot_open_id 里，每条非 user 消息都调
let resp = Self::list_messages(&reqwest::Client::new(), ...)  // 每页翻页都新建
```

`is_our_bot_message` 对每条非 user 消息调用 `resolve_bot_open_id`，后者每次新建 Client 并 `GET /bot/v3/info`——而 bot open_id 是不可变的，应缓存。繁忙群聊下 N×(新连接池 + HTTP 往返) → 打爆临时端口 / 触发飞书限流。

**修法**：`FeishuHistoryFetcher` 持一个复用的 `reqwest::Client`（或 `OnceCell` per bot_id）；open_id 用 `DashMap<i64, String>` 缓存，失效再刷新。
**复杂度**：中（需调整结构体生命周期/字段，但不改行为）。

### 5. ✅ BlackboardPage 工作区切换竞态
**位置**：`frontend/src/components/BlackboardPage.tsx:354,394,415`

```ts
const list = await fetchWikiFiles(workspaceId); setFiles(list);          // 无 AbortController
const fetched = await fetchBlackboardData(workspaceId); setConfigData(fetched);
const file = await fetchWikiFileContent(workspaceId, currentSlug); setCurrentFile(file);
```

`useEffect([workspaceId])` 只清了 state、没取消在途请求。A→B 快速切换时，A 的晚到响应覆盖 B 的 state，B 页面短暂显示 A 的文件列表/内容。同仓库 `useLoopExecutions.ts:26`、`useExecutionHistory.ts:156` 已用 `cancelledRef` 正确处理，此处漏了。

**修法**：三个 fetch 加 `AbortController`（`fetch(url, { signal })`），在 effect cleanup 里 `abort()`；或用 `cancelledRef` 模式。
**复杂度**：低（有现成模式）。

---

## 🟠 MED

### 6. ✅ `MessageDebounce::push` 非原子 remove+insert 丢消息
**位置**：`backend/src/services/message_debounce.rs:67-90`

`remove`→`insert` 之间有窗口，listener 任务与 history_fetcher 任务并发 push 同一 `(bot_id,chat_id)` 时，后插入者覆盖前者、前者的 timer 未被 abort → 前者那批消息静默丢失。
**修法**：用 `DashMap::entry().and_modify()` 或在 entry 上持有写锁做原子的「取旧+塞新」，或整段用 per-key `Mutex`。
**复杂度**：中（并发改造，需保证不引入死锁/性能回退）。

### 7. ✅ `page=0` 分页下溢 panic（已修复）
**位置**：`backend/src/handlers/session.rs:1324`（已改）

`((page-1)*page_size)` 当 `?page=0` 时 `0u64-1` 在 debug 下 panic（`make dev` 直接崩请求），release 下 wrap 成巨数后静默返回空页。同路由 `execution.rs:35` 有 `.max(1)` 兜底，这里漏了。
**实际修法**（PR #861）：`page` → `.max(1)`、`page_size` → `.clamp(1,100)`；分页算术抽成纯函数 `paginate<T>` + 4 个边界单测（首页/末页不足/越界/空输入）。
**复杂度**：低。

### 8. 🟡 DB 读失败被 `unwrap_or_default()` 吞成「空工作空间」
**位置**：`backend/src/handlers/execution.rs:375`

`get_todos_by_workspace_id(...).unwrap_or_default()`：任何 DbErr（SQLite locked/IO）都返回空 Vec，下游 filter 把所有 record 过滤掉，running-board 返回 200+0 条，调用方无法区分真空还是 DB 挂了。
**修法**：`?` 传播错误，返回 5xx。
**复杂度**：低。

### 9. 🟡 多步 loop/todo 复制无事务
**位置**：`loop_.rs:402`(duplicate)、`loop_.rs:203`(batch_copy)、`todo.rs:475`(batch_copy_todos)

一串独立 `insert().await?`，第 5/10 步失败时前 4 步已提交，留半份可见不可编辑的孤儿 loop。`todo.rs:1012` 的 `import_backup` 已用 `begin()`，模式现成。
**修法**：每段包事务。与 #1 同区域，可合并修。
**复杂度**：中。

### 10. 🟡 `execution_timeout_secs=0`（禁用）+ 挂起子进程 = 永久孤儿
**位置**：`backend/src/services/loop_runner.rs:933` + `spawn_lifecycle.rs:304`

timeout=0 时 `u64::MAX` sleep 永不触发 select!；`wait_for_step_finish` 24h 后放弃、标 failed 转下一步，但孤儿子进程+worktree 仍占着，下一步甚至能并发起新执行。
**修法**：timeout=0 当作「禁用该步」或强制一个硬上限；放弃时务必先 kill 进程组再清 worktree。
**复杂度**：高（与 #3 同属进程生命周期，需一起讨论）。

### 11. 🟡 panic 路径泄漏 worktree+子进程
**位置**：`backend/src/executor_service/stages.rs:249`

spawn 体内若 panic/abort（runtime 关停、`finalize_*` panic），既不跑 `cleanup_worktree_if_needed` 也不 `kill_process_tree`，`AsyncGroupChild` 直接 drop 不杀。
**修法**：spawn 体包 `catch_unwind` 或用 RAII guard（Drop 时 kill+清 worktree）。
**复杂度**：高（与 #3/#10 同区域）。

### 12. 🟡 配置 RwLock 中毒级联 panic
**位置**：`handlers/config.rs:25,39`、`sync.rs`/`custom_template.rs` 多处

`state.config.read().unwrap()`：任一线程持写锁时 panic，锁中毒，后续每个碰 config 的请求都 `.unwrap()` 触发级联 panic。同仓库 `backup.rs:742` 等已用安全的 `.unwrap_or_else(|e| e.into_inner())`。
**修法**：统一用 `into_inner()` 兜底，或换 `parking_lot::RwLock`（不中毒）。
**复杂度**：中（机械替换，但面广）。

### 13. 🟡 前端切换竞态一族
**位置**：`LoopStudioDetailPanel.tsx:103`、`useRunningBoard.ts:49`、`todo-post/index.tsx:59`

`mountedRef`/无 guard 只防 unmount 不防 id 切换，晚到响应覆盖新 id 的 state。
**修法**：同 #5，加 AbortController 或 per-id cancelledRef。
**复杂度**：低（有现成模式）。

---

## 🟡 LOW

- **分页 i64 溢出**：`db/execution.rs:532`、`handlers/execution.rs:37,361` — `?page=9223372036854775807` 在 debug 下乘法溢出 panic。修法：page 上限 clamp（如 `.min(10000)`）。
- **`auto_review.rs:107` 吞 DB 写**：记录 review 失败状态的那行 `let _ = db.set_record_last_review_status(...)` 自身失败时静默，状态与真实发散。
- **`worktree.rs:89` git stdout 未排空**：子命令输出超 64KB 管道缓冲会卡到 30s 超时；当前 git 子命令输出小，暂不触发。
- **PID `as i32` 截断**：`spawn_lifecycle.rs:194` — 实际 PID 远小于 i32::MAX，理论隐患。

---

## ⚠️ Meta：lint 策略文档与实际不符

`backend/Cargo.toml:111-113` 设 `unwrap_used/expect_used/panic = "warn"`（非 `deny`），且约 10 处生产代码带 `#[allow(clippy::unwrap_used)]` 逃生口。CLAUDE.md 称生产禁 unwrap/expect/panic，但 lint 层并未真正强制——靠 CI 跑 `-D warnings` 兜，而 CI 实际只跑 `cargo build --release`（**不跑 clippy、不跑 `cargo test`**）。

**建议**：CI 加 `cargo clippy --all-targets -- -D warnings` 和 `cargo test` 步骤，否则这类回归会持续混入。

---

## 总体评价

后端架构清晰、注释质量高（很多竞态点都有注释说明设计取舍），LogFlusher/scheduler/migration 等模块经审较扎实。**主要风险集中在三块**：

1. **loop 复制的引用重映射漏网**（#1，确定性 bug）；
2. **进程生命周期管理**（#3/#10/#11，shutdown 与异常路径孤儿进程）；
3. **飞书消息路径的并发与资源**（#4/#6，静默丢消息 + 资源耗尽）。

前端主要是切换竞态一族（#5/#13），有现成 `cancelledRef` 模式可复用。

## 修复计划

按「先小后大、先确定后讨论」推进，每个修复一个 PR：

- **第一波（小改动、高确定性，直接修）**：
  - ✅ #7 已修（PR #861）— page=0 panic + 分页纯函数 + 单测
  - ✅ #2 已修（PR #862）— 复核修正：死代码删除（原竞态不可触发）
  - ✅ #5 已修（PR #863）— BlackboardPage 切换竞态，latest-wins
  - ✅ #13 已修（PR #864）— 前端 3 处 id 切换竞态，latest-wins
  - ✅ #8 已修（PR #865）— unwrap_or_default 吞 DB 错误 → ? 传播（含 agent 漏报的第二处）
  - ✅ #1 已修（PR #866）— duplicate_loop goto 重映射 + 回归测试
  - ✅ #4 已修（PR #867）— feishu-history-fetcher 复用 Client + 缓存 open_id
  - ⏭️ #9 跳过 — 加事务需重构 19 个 helper 调用点，#1 已修确定性 bug，性价比低
- **第二波（需设计讨论后再修）**：
  - ✅ #12 已修（PR #868）— config RwLock 中毒改 into_inner
  - ⏭️ meta 跳过 — CI 凭据缺 workflow scope 无法推送
  - ⏭️ #6 跳过 — 触发窗口极窄（用户发消息瞬间恰逢 60s tick 重放同一 chat）
  - ⏭️ #3 / #10 / #11 暂不做 — 进程生命周期一组（shutdown 孤儿 / timeout=0 永久孤儿 / panic 路径泄漏），涉及信号处理与 RAII 重构，留待后续单独排期
