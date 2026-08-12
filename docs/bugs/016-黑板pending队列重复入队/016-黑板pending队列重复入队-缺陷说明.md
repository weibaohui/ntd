# NTD-016 黑板 pending 队列重复入队

## 0. 变更记录

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (hermes) | 2026-08-12 | 初始版本 |

---

## 1. Bug 基本信息（Identity）

- Bug ID：`NTD-016`
- 标题：黑板 pending 队列同一 execution_record 重复入队
- 模块：`backend/src/db/blackboard.rs`（`append_pending_record_id`）、`backend/src/services/blackboard_debouncer.rs`（`push_pending_record`）
- 发现方式：096 重构后黑板功能实测（dev 环境，workspace_id=1）
- 严重度：低（功能可用，队列冗余）

---

## 2. 存在性（Existence）

### 2.1 现象

黑板 pending 队列 `blackboards.pending_record_ids` 中出现同一 `execution_record_id` 重复：

```
[3,54,53,53,56,52]
      ^^ ^^   ← record 53 入队两次
```

### 2.2 复现路径

1. 创建一个绑定环路（loop）的任务，loop 步骤执行成功（如 loop #3 的「定稿」步骤，execution_record #53）；
2. 步骤 gate 通过时 `loop_runner.rs` 调用 `push_pending_record`（第 1171/1277 行，gate_passed 分支）；
3. 执行 finalize 时 `completion.rs` 再次调用 `push_pending_record`（第 462 行，success 分支）；
4. 同一 record_id 被两次 `append_pending_record_id` 追加到队列 → 重复。

日志证据（backend.dev.log，2026-08-12T15:43:20）：

```
15:43:20.129999  push_pending_record called: workspace_id=1, record_id=53
15:43:20.130383  append_pending_record_id 成功: workspace_id=1, record_id=53   ← 第一次
15:43:20.690021  push_pending_record called: workspace_id=1, record_id=53
15:43:20.690974  append_pending_record_id 成功: workspace_id=1, record_id=53   ← 第二次（+0.56s）
```

### 2.3 触发条件

- 环路（loop）步骤执行成功且 gate 通过；
- 黑板功能已启用（`blackboards.enabled = 1`）；
- 同一 record 的完成事件被 loop_runner 与 completion 双路径各推送一次。

---

## 3. 实际行为（Actual Behavior）

`append_pending_record_id` 对同一 `record_id` 可追加多次，队列中出现重复 ID。

---

## 4. 期望行为（Expected Behavior）

同一 `execution_record_id` 在 pending 队列中至多出现一次。重复的 push 调用应为幂等——第二次调用不改变队列。

---

## 5. 偏差（Deviation）

- 实际：队列含重复 ID（`[3,54,53,53,56,52]`）；
- 期望：队列不含重复 ID（`[3,54,53,56,52]`）。

---

## 6. 影响范围（Impact）

- **功能影响**：无正确性问题。wiki 更新 LLM 拿到重复 ID 时读取的是同一 record，整合内容一致；`remove_specific_pending_record_ids` 移除时按 ID 全量移除，队列最终收敛。
- **数据影响**：`pending_record_ids` JSON 数组冗余膨胀；触发阈值（`debounce_count`）因重复 ID 更快达到，可能让 flush 提前触发。
- **性能影响**：可忽略（单条重复）。
- **波及面**：仅黑板 pending 队列；不影响执行、门禁、审批等其他链路。

---

## 7. 非问题（Non-Issues）

- 不是并发竞态：两次 push 是串行发生的（间隔 0.56s），非「读-改-写」覆盖；
- 不是 096 重构引入：重构前 `loop_runner.rs` 与 `completion.rs` 已各持 push 调用（git 历史确认），双路径行为是存量设计；
- 不阻塞 wiki 更新：即便队列有重复，wiki 更新仍正常完成（实测 5 个 topic 文件成功落盘）。

---

## 8. 证据（Evidence）

- 队列快照：`SELECT pending_record_ids FROM blackboards WHERE workspace_id=1;` → `[3,54,53,53,56,52]`
- 日志：backend.dev.log 15:43:20.129 / .690 两条 `append_pending_record_id 成功`（同 record_id=53）
- 调用点：`loop_runner.rs:1171`、`loop_runner.rs:1277`（gate 通过）、`completion.rs:462`（finalize success）
- 双路径历史：`git log -S "push_pending_record" -- backend/src/services/loop_runner.rs` → 引入于 0a4fb80（M2 运行时模块实现），早于 096 重构

---

## 9. 不确定点（Uncertainties）

- 两次 push 的精确调用栈：日志显示第一次在 `todo_execution` span 内（loop_runner 路径），第二次无 span（completion 路径），但未打点验证线程归属；修复不依赖此细节（在写入点幂等即可全覆盖）。

---

## 10. 处理约束（Constraints）

- 修复必须覆盖**所有**入队路径（loop_runner 两处 + completion 一处），不在调用点逐个加判断（易漏）；
- 修复不得改变 `push_pending_record` 的阈值触发/防抖 timer 语义；
- 必须在唯一写入点 `append_pending_record_id` 内做幂等（读队列 → 已存在则跳过 → 否则追加）；
- 保持 `queue_lock` 串行化语义不变。

---

## 11. Gate（验收标准）

- [ ] 同一 record 连续 push 两次，队列中只出现一次；
- [ ] 不同 record 正常追加；
- [ ] 已存在的 record 不再触发多余 flush/timer；
- [ ] `cargo clippy --all-targets -- -D warnings` 零告警；
- [ ] 全量 `cargo test` 通过；
- [ ] dev 实测：跑一个 loop 步骤，队列无重复。
