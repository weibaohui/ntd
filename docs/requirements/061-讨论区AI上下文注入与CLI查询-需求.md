# 061-讨论区 AI 上下文注入与 CLI 查询-需求

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude (AI) | 2026-08-06 | 初始版本 |

---

## 1. 背景（Why）

060 已实现任务讨论区（论坛跟帖 + `@专家`/`@执行器` 触发执行后回帖）。但 `@` 触发执行时，注入给执行器的提示词（`handlers/task_posts.rs::build_carrier_prompt`）只包含**任务标题 + 任务需求 + 当前这一条帖子正文**，存在三个缺口：

1. **没注入任务 ID / workspace ID**：被叫的 AI 不知道自己在哪个任务、哪个工作空间。
2. **没注入讨论历史**：AI 只看得到当前一条楼层，看不到之前的讨论上下文，难以了解全貌。
3. **没有「如何拉取」的途径**：ntd CLI 无 `task` 子命令，被叫的 AI 无法像 `gh pr view --comments` 那样主动拉取任务讨论历史与任务全貌；只能盲回复。

此外，ntd 官方 skill（`ntd-skills/ntd-usage/SKILL.md`，经 `ntd skills install` 装到各执行器）的命令速查表未覆盖任务讨论区命令，AI 无从知晓这些能力。

本需求让被 `@` 的 AI 能**了解全貌**：既在触发时内联最近讨论上下文，又能用新增的 ntd CLI 命令主动拉取完整历史与任务全貌，并在官方 skill 中描述这些命令。

---

## 2. 目标（What，必须可验证）

- [ ] **T1** ntd CLI 新增只读 `task` 子命令组：`ntd task view --workspace-id <N> --task <id>`（任务全貌）、`ntd task posts --workspace-id <N> --task <id> list`（讨论历史）。
- [ ] **T2** `@` 触发执行时，注入执行器的提示词包含：**任务 ID**、**workspace ID**、**最近 N 条讨论上下文**、**ntd 命令用法提示**。
- [ ] **T3** ntd 官方 skill `ntd-usage/SKILL.md` 新增「任务讨论区」命令说明，`ntd skills install` 后执行器可读到。
- [ ] **T4** 全流程零编译告警：后端 `cargo clippy --all-targets -- -D warnings`、`cargo test` 通过。

---

## 3. 非目标（Explicitly Out of Scope）

- 不让被叫的 AI 通过 CLI **发帖/回帖**（`ntd task post create`）——回帖仍走前端 `@` 机制；CLI 只读。
- 不新建独立的 bundled skill 体系——只**完善现有** `ntd-usage/SKILL.md`。
- 不改动 060 的论坛实体（`task_posts` 表）、`@` 解析、完成回写（`completion.rs` discussion 分支）、前端讨论 Tab。
- 不改动执行器适配器契约（`CodeExecutor` trait）与执行注入链的其他环节（workspace/expert/step/skills 注入保持不变）。
- 不把「任务全貌」做成新的聚合 HTTP 端点——复用现有 `GET /workspaces/{ws}/tasks/{id}`。

---

## 4. 使用场景 / 用户路径

**场景 A（被 @ 的 AI 主动了解全貌）**
用户在任务讨论区发 `@codex 帮我接着上一轮的思路继续分析` → codex 执行器收到的提示词里已含「最近 5 条讨论上下文 + 任务 id + workspace id」→ codex 还可自行运行 `ntd task posts --workspace-id 1 --task 42 list` 拉完整历史、`ntd task view ...` 看任务全貌 → 基于全貌给出结论并回帖。

**场景 B（用户在终端查讨论）**
用户/外部脚本在终端运行 `ntd task posts --workspace-id 1 --task 42 list` → 输出该任务的讨论帖（json/table/raw 可选）。

**场景 C（AI 通过 skill 知晓命令）**
`ntd skills install` 后，执行器（如 Claude Code）的 `~/.claude/skills/ntd-usage/SKILL.md` 含「任务讨论区」一节，AI 被叫时知道可以用 `ntd task ...` 查询。

---

## 5. 功能需求清单（Checklist）

- [ ] **F1** CLI：`commands.rs` 新增 `TaskAction`（`View` / `List` / `Posts`）+ `TaskPostAction`（`List` / `Get`）+ `handle_task`；`Commands` 枚举与 `run_command` 注册新增 `Task`。
- [ ] **F2** CLI：`main.rs` 镜像 `Commands` 枚举同步新增 `Task`，dispatch 区块新增路由 arm。
- [ ] **F3** CLI：`cli/mod.rs` 的 `pub use` 导出 `TaskAction` / `TaskPostAction`。
- [ ] **F4** 后端：`task_posts.rs` 新增 `fetch_recent_main_posts`（复用 `list_main_posts_paged`，失败静默回退空）。
- [ ] **F5** 后端：`task_posts.rs` 新增纯函数 `format_discussion_history`（帖子 → 文本，截断超长）。
- [ ] **F6** 后端：改造 `build_carrier_prompt`，注入任务 id + workspace id + 历史段落 + ntd 命令提示常量。
- [ ] **F7** skill：`ntd-skills/ntd-usage/SKILL.md` 命令速查表补 task 行 + 新增「任务讨论区」小节。
- [ ] **F8** 测试：`format_discussion_history` / `build_carrier_prompt` 单测；CLI `task` 命令集成/单测。

---

## 6. 约束条件

- **技术约束**：复用现有 HTTP API（`GET /workspaces/{ws}/tasks/{id}`、`GET /workspaces/{ws}/tasks/{id}/posts`）与 CLI 模板（`BlackboardAction`/`WikiAction`/`ws_prefix`/`print_response`）；生产代码禁止 `.unwrap()`/`.expect()`/`panic!`；单函数体 ≤50 行（超需豁免理由）；每个公开函数有单元测试。
- **架构约束**：后端分层 Entity → DAO → Handler；CLI 两处 `Commands` 枚举（`commands.rs` + `main.rs`）必须同步；CLI 路径不重复写 `/api/v1`（`client.rs` 自动补）。
- **安全约束**：CLI `task` 命令只读，不触发执行、不写库；carrier prompt 注入的讨论历史来自本任务帖子，受既有 workspace 隔离约束（`require_task_in_ws` 已校验归属）；注入的 ntd 命令不含任何凭据（CLI 自行从 `~/.ntd/config.yaml` 寻址）。
- **性能约束**：内联讨论历史最多 N=5 条主楼层、每条截断 ~500 字符，防止 prompt 膨胀；`fetch_recent_main_posts` 失败静默回退空，不阻断 `@` 触发。

---

## 7. 可修改 / 不可修改项

- ❌ **不可修改**：
  - 执行器适配器契约（`CodeExecutor` trait 及各 `adapters/*.rs`）。
  - 060 的 `task_posts` 表结构、`@` 解析（`resolve_mentions`）、完成回写（`completion.rs` discussion 分支）。
  - 执行注入链其他环节（`inject_workspace_prompt` / `inject_expert_context` / `inject_step_context` / `inject_todo_skills`）。
  - 前端讨论 Tab 组件。
- ✅ **可调整**：
  - carrier prompt 的具体文案、内联历史条数 N、截断长度。
  - CLI `task` 命令的输出字段默认值。
  - `ntd-usage/SKILL.md` 新增小节的措辞与排版。

---

## 8. 接口与数据约定

### 8.1 CLI 命令（新增，只读）

| 命令 | 对接 HTTP | 说明 |
|------|-----------|------|
| `ntd task view --workspace-id <N> --task <id>` | `GET /api/v1/workspaces/{ws}/tasks/{id}` | 任务全貌（task + template + loop + steps + executions） |
| `ntd task list --workspace-id <N> [--status <s>]` | `GET /api/v1/workspaces/{ws}/tasks` | 列出任务 |
| `ntd task posts --workspace-id <N> --task <id> list` | `GET /api/v1/workspaces/{ws}/tasks/{id}/posts` | 讨论历史（主楼层 + 楼中楼） |
| `ntd task posts get --workspace-id <N> --task <id> <pid>` | `GET /api/v1/workspaces/{ws}/tasks/{id}/posts/{pid}` | 单帖 |

- `--workspace-id` 必填（i64）；支持全局 `--output json|raw` 与 `--fields`。
- 复用 `print_response`，`code != 0` 抛错。

### 8.2 注入提示词结构（`build_carrier_prompt` 产出）

落点 = 执行器 prompt 的「# 任务」段（由 `inject_expert_context` 包在专家人设之后）。结构：

```
你被 @ 到「任务 #{id}（工作空间 #{ws}）」的讨论区。
任务标题：{title}
任务需求：{desc}
## 既有讨论上下文（最近 N 条，按时间正序）
{history 或「（暂无历史）」}
## 本次讨论诉求
{post_content}
## 了解全貌的 ntd 命令（默认连本地 ntd，无需额外参数）
- 任务全貌：ntd task view --workspace-id {ws} --task {id}
- 完整讨论历史：ntd task posts --workspace-id {ws} --task {id} list
请基于以上上下文给出可直接回复的结论（Markdown）。
```

---

## 9. 验收标准（Acceptance Criteria）

1. **若**运行 `ntd task posts --workspace-id <N> --task <id> list`，**则**输出该任务的讨论帖（默认 json，支持 `--output raw --fields`）。
2. **若**运行 `ntd task view --workspace-id <N> --task <id>`，**则**输出任务全貌（task/template/loop/steps/executions）。
3. **若**在任务讨论区 `@<执行器>` 发帖触发执行，**则**执行器收到的 prompt 含「任务 #<id>」「工作空间 #<ws>」「既有讨论上下文」「ntd 命令」四要素（查 daemon.log 或执行记录可验）。
4. **若**该任务此前无任何讨论帖，**则**prompt 的历史段显示「（暂无历史）」，且仍含任务 id 与命令提示。
5. **若**`fetch_recent_main_posts` 读库失败，**则**静默回退为「（暂无历史）」，**不**阻断 `@` 触发执行。
6. **若**运行 `ntd skills install` 后查看 `~/.claude/skills/ntd-usage/SKILL.md`，**则**含「任务讨论区」命令说明。
7. **若**运行 `cargo clippy --all-targets -- -D warnings` 与 `cargo test`，**则**全部通过。

---

## 10. 风险与已知不确定点

- **prompt 膨胀**：内联历史 + 命令提示会增加 prompt 长度。缓解：N=5、每条截断 ~500 字符；超长任务 AI 可改用 CLI 按需拉取，不全部内联。
- **CLI 寻址**：执行器在 worktree 内运行 `ntd`，依赖 `ntd` 在 PATH 且 `~/.ntd/config.yaml` 指向正确实例。若环境缺失，命令失败但不影响已内联的上下文（AI 仍有即时上下文可依赖）。
- **任务全貌不含 description**：现有 `GET /tasks/{id}` 返回的 task 对象省略 description（loop-based 全貌）。`ntd task view` 直接复用该端点；description 已在 carrier prompt 中提供，不构成阻塞。如需 view 也带 description，另立小需求增强该端点。
- **两处 Commands 枚举不同步**：`commands.rs` 与 `main.rs` 的 `Commands` 必须同时加 `Task` variant，漏一处会导致编译失败或命令不路由——实现时同步修改并编译验证。

---

## 11. 非目标（再次明确）

- 本需求**不**实现：AI 通过 CLI 发帖/回帖、新建 skill 体系、新聚合 HTTP 端点、改动 060 论坛实体或注入链其他环节、前端改动。
- 上述能力若后续需要，另立需求，不在 061 范围内。
