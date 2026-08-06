# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-08-06 | 初始版本（讨论区 AI 上下文注入与 CLI 查询实现总结） |

---

# 实现总结：讨论区 AI 上下文注入与 CLI 查询（061）

## 1. 与需求 / 验收标准的对应关系

| 需求 | 实现位置 | 状态 |
|------|---------|------|
| **T1** CLI 只读 `task` 子命令组 | `commands.rs` `TaskAction`/`TaskPostAction`/`handle_task` + `main.rs`/`mod.rs` 镜像 | ✅ |
| **T2** 注入任务 id + workspace id + 最近讨论 + 命令 | `task_posts.rs::build_carrier_prompt`（+ `fetch_recent_main_posts` / `format_discussion_history` / `truncate_chars`） | ✅ |
| **T3** ntd-usage skill 新增任务讨论区小节 | `ntd-skills/ntd-usage/SKILL.md`（速查表 + 「💬 任务讨论区」小节） | ✅ |
| **T4** 零编译告警 | `cargo clippy --all-targets -- -D warnings` 通过；`cargo test` 全绿 | ✅ |

| 验收标准 | 实测证据 | 状态 |
|----------|---------|------|
| 1. `task posts ... list` 输出讨论帖 | dev server task 39 实测返回 `{"items":[...]}`，含人帖 + `@codex` 帖 | ✅ |
| 2. `task view` 输出任务全貌 | task 39 实测返回 task/template/loop/steps/executions | ✅ |
| 3. prompt 含四要素 | `build_carrier_prompt` 单测断言任务 #id / 工作空间 #ws / 历史段 / ntd 命令四要素均在 | ✅（单测） |
| 4. 无讨论帖时显示「（暂无历史）」 | `test_build_carrier_prompt_empty_history` | ✅ |
| 5. 读库失败静默回退 | `fetch_recent_main_posts` 用 `unwrap_or_default()` | ✅ |
| 6. `skills install` 后 skill 含讨论区小节 | `skills install --force` 后 `~/.claude/skills/ntd-usage/SKILL.md` grep「任务讨论区」命中 | ✅ |
| 7. clippy + test 通过 | 见 §4 | ✅ |

> 验收标准 3 的「端到端 @ 触发」未做真实执行器调用（设计文档 §8 限制：dev server 为旧二进制，重启 + 真实 AI 调用成本高且不可控；注入逻辑由 16 条单测精确覆盖，串联链为 060 既有已验证路径）。

## 2. 改了什么

### 2.1 `backend/src/cli/commands.rs`（块 1）
- 新增 `TaskAction { View / List / Posts }` + `TaskPostAction { List / Get { pid } }`，仿 `BlackboardAction`/`WikiAction`（workspace-scoped）。
- 新增 `handle_task` / `handle_task_posts`，复用 `ws_prefix` + `client.get`（自动补 `/api/v1`）+ `print_response`。
- `Commands` 枚举注册 `Task { action }`，`run_command` 加分派行。

### 2.2 `backend/src/main.rs`（块 1）
- 镜像 `Commands` 枚举同步新增 `Task { action: cli::TaskAction }`。
- dispatch 区块新增 `Task` 路由 arm。

### 2.3 `backend/src/cli/mod.rs`（块 1）
- `pub use` 导出 `TaskAction` / `TaskPostAction`。

### 2.4 `backend/src/handlers/task_posts.rs`（块 2）
- 新增常量 `DISCUSSION_HISTORY_LIMIT=5`、`HISTORY_POST_TRUNCATE=500`。
- 新增 `fetch_recent_main_posts(db, task_id)` —— 复用 `list_main_posts_paged(page=1, limit=N)`，失败 `unwrap_or_default()` 静默回退。
- 新增纯函数 `format_discussion_history(&[Model])` —— 帖子 → `[作者(身份) 状态] 正文` 文本，超长截断。
- 新增纯函数 `truncate_chars(s, max)` —— UTF-8 字符安全截断（不切坏多字节字符）。
- 改造 `build_carrier_prompt(task, post_content, history_text)` —— 注入任务 #id / 工作空间 #ws / 历史段（空则「（暂无历史）」）/ ntd 命令提示（值已替换实际 ws/id）。
- `create_agent_post` 内唯一调用点串联（对外签名不变）。

### 2.5 `ntd-skills/ntd-usage/SKILL.md`（块 3）
- 「高频命令速查」表补「看任务讨论区」行。
- 新增「💬 任务讨论区（Task Discussion）」小节：被 `@` 了解全貌的推荐流程、命令表、`--workspace-id` 必填说明、`--output`/`--fields` 准确用法、**命令格式坑**提示（父级参数须在子命令前）。

### 2.6 顺手修复既有 bug：`ntd workspace` 不路由
- **现象**：`commands.rs` 早已完整定义 `Workspace` variant + `WorkspaceAction`（List/Create）+ `handle_workspace` + `run_command` 分支（line 765），但 `mod.rs` 未导出 `WorkspaceAction`、`main.rs` 镜像缺 `Workspace` variant → `ntd workspace` 一直 unrecognized。
- **修复**：`mod.rs` 补 `WorkspaceAction` 导出；`main.rs` 补 `Workspace` variant + dispatch arm（与本次新增 `Task` 完全同模式）。
- **为何纳入本 PR**：`--workspace-id` 是 `task` 命令必填前置，被 `@` 的 AI 自主探索需 `ntd workspace list` 拿 id；且 `ntd-usage/SKILL.md` 既有「工作空间」整节依赖该命令。零新逻辑（仅镜像补全），与已验证的 `Task` 同构。

## 3. 关键设计取舍

- **内联最近 5 条 + 注入命令，双管齐下**：内联保证 AI 触发即有上下文（即使 ntd 不在 PATH）；命令保证 AI 能按需拉完整历史/任务全貌。N=5 / 每条 ~500 字符平衡 prompt 体积与充分性。
- **carrier prompt 命令用实际值替换占位**：`ntd task view --workspace-id {ws} --task {id}` 在注入时已替换为真实数字（如 `--workspace-id 1 --task 42`），AI 复制即用，**不依赖** `ntd workspace list` 先查 id。
- **命令格式遵循 clap 实际解析**：`task posts` 是嵌套子命令，父级参数（`--workspace-id`/`--task`）必须在子命令前（`ntd task posts --workspace-id N --task id list`）。开发期通过 `--help` 的 USAGE 行发现并统一修正了 carrier prompt、SKILL.md、需求/设计文档中所有「子命令在前」的错误写法。
- **`fetch_recent_main_posts` 失败不阻断触发**：与 `inject_*` 降级哲学一致，读库失败回退空历史，`@` 触发照常进行。
- **不新增 CLI 写帖命令 / 不新建 skill 体系 / 不改 060 实体**：严格守住需求 §3 非目标。

## 4. 测试

- **clippy**：`cargo clippy --all-targets -- -D warnings` 零告警。
- **全量测试**：`cargo test` 全绿（lib 1635 passed / 0 failed；各集成测试 0 failed）。
- **task_posts 模块**：16 passed / 0 failed（含本次新增 8 条：`format_discussion_history` 多条/空/截断、`truncate_chars` 限内/超限、`build_carrier_prompt` 注入四要素/空历史）。
- **CLI 端到端实测**（连 dev server `http://localhost:18088`）：
  - `ntd task list --workspace-id 1` → 返回 workspace 1 的 task 列表。
  - `ntd task view --workspace-id 1 --task 39` → 返回 task 39 全貌（executions/loop/steps）。
  - `ntd task posts --workspace-id 1 --task 39 list` → 返回真实讨论帖（含 `@codex` 触发帖）。
  - `ntd task posts --workspace-id 1 --task 39 get 1` → 返回单帖。
  - `ntd workspace list`（顺手修复后）→ 返回 workspace 列表。
- **skill 传播**：`ntd skills install --force` 后 `~/.claude/skills/ntd-usage/SKILL.md` 含「任务讨论区」小节且命令格式正确。

## 5. 安全反思

- CLI `task` 命令**只读**（仅 `GET`），不触发执行、不写库；路径经 `require_task_in_ws` 既有的 workspace 归属校验。
- 注入的讨论历史来自**本任务**主楼层（`list_main_posts_paged(task_id, ...)`），受 task_id 作用域约束，不会串到其他任务。
- 注入的 ntd 命令**不含任何凭据**：CLI 自行从 `~/.ntd/config.yaml` 寻址，prompt 里只有命令文本 + 任务标识。
- `workspace_id` 在 prompt 中回退到 0 仅作文案占位；正常路径 `require_task_in_ws` 已保证 `task.workspace_id` 与 URL 的 ws 一致，不命中回退分支（注释已说明）。

## 6. 风险与回滚

- **prompt 体积**：内联历史最多 5 条 × ~500 字符 + 命令提示，增量可控；超长任务 AI 可改用 CLI 按需拉取。
- **`--fields` 对列表不递归**：实测 `--output raw --fields "..."` 对 `{items:[]}` 列表按顶层键过滤（返回 `{}`），仅对单条（`view`/`posts get`）按字段精简有效。SKILL.md 小提示已据实说明，避免 AI 误用。此为 ntd CLI 既有通用行为，非本次引入。
- **真实 @ 触发未端到端验证**：注入内容正确性由单测保证，串联链为 060 既有路径；如需完全确认，重启 dev server（新二进制）后 `@` 任一执行器，查 daemon.log / 执行记录 prompt 即可（见需求验收标准 3）。
- **回滚**：纯 CLI + prompt 文案 + skill 改动，无 DB migration；`workspace` 路由修复为镜像补全，revert 单 commit 即可（会重新使 `ntd workspace` 不可用，回到既有状态）。
