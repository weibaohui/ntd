# 061-讨论区 AI 上下文注入与 CLI 查询-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude (AI) | 2026-08-06 | 初始版本 |

> 对应需求：[`061-讨论区AI上下文注入与CLI查询-需求.md`](../requirements/061-讨论区AI上下文注入与CLI查询-需求.md)
> 关联：060-任务讨论区（本设计在其已落地基础上增强）

---

## 1. 总体思路

被 `@` 到任务讨论区的执行器，当前只能看到「标题 + 描述 + 当前一条帖子」，无法了解讨论全貌与任务全貌。本设计用「复用现有管线，不造新体系」的策略补三块：

1. **CLI 查询能力（只读）** → 新增 `ntd task` 子命令组，复用 060 已有的 task HTTP 路由 + 现有 CLI 模板（`BlackboardAction`/`WikiAction`）。像 `gh pr view --comments` 一样让 AI/用户能拉讨论与任务全貌。
2. **提示词上下文注入** → 改造唯一构造点 `build_carrier_prompt`，注入任务 id + workspace id + 最近讨论 + ntd 命令用法。拆为 3 个 ≤30 行小函数，降级策略与现有 `inject_*` 一致。
3. **ntd 官方 skill 完善** → 在 `ntd-skills/ntd-usage/SKILL.md` 补 task/讨论区命令说明。`rust-embed` 编译期嵌入，`ntd skills install` 后生效，无需改代码。

三块互不阻塞，可一起合入。

---

## 2. 已采纳决策（需求澄清结论）

| 决策点 | 采纳方案 | 落地影响 |
|--------|----------|----------|
| AI 了解全貌途径 | 内联最近 N=5 条讨论 + 注入 ntd 命令 | `build_carrier_prompt` 加历史参数 + 命令常量 |
| CLI 命令范围 | 只读（view / posts list / posts get / list） | 不新增 create，回帖仍走前端 `@` |
| ntd skill 载体 | 完善现有 `ntd-usage/SKILL.md` | 不新建 skill 体系 |

---

## 3. 现状与缺口

| 环节 | 现状 | 缺口 |
|------|------|------|
| carrier prompt 构造 | `handlers/task_posts.rs:173 build_carrier_prompt(task, post_content)`，唯一调用点 `:512`（在 `create_agent_post` 内，已有 `state: &AppState`） | 未注入任务 id / workspace id / 历史 / 命令 |
| 讨论历史 DAO | `db/task_post.rs:78 list_main_posts_paged(task_id, page, limit)`（注：需求 060 文档写的 `list_posts_by_task(task_id,parent_id,page,limit)` 不存在，真实方法无 parent_id） | 无「拉最近 N 条」便捷封装，但分页方法可直接用 page=1,limit=N |
| CLI task 命令 | 无（`cli/commands.rs:72-108` 只有 Todo/Loop/Tag/Stats/Blackboard/Workspace/Process） | 需新增 `Task` 子命令组 |
| task HTTP 路由 | 已就绪：`GET /workspaces/{ws}/tasks/{id}`（`handlers/tasks.rs:176 get_task_detail`，loop-based 全貌）、`GET /workspaces/{ws}/tasks/{id}/posts`（060） | 无需新增端点，CLI 直接对接 |
| ntd 官方 skill | `ntd-skills/ntd-usage/SKILL.md` 命令速查表（line 140-156）无 task/讨论区命令 | 需补小节 |

**关联关系事实**（影响「任务全貌」设计）：`tasks.id`(i64) 与 `todos`/`execution_records` 的 `task_id`(UUID 字符串) **无外键关联**；任务能关联到的执行只能经 `task_posts.source_execution_id/source_todo_id` 间接拿。因此「任务全貌」用现有 `get_task_detail`（task + template + loop + steps + loop_executions）即可，不强求关联 todo/execution_records。

---

## 4. 块 1：CLI 新增只读 `task` 子命令组

### 4.1 `cli/commands.rs`

仿 `BlackboardAction`（`:130`）/ `WikiAction`（`:150`）/ `ws_prefix`（`:145`）/ `handle_blackboard`（`:1017`）。

新增枚举：
```rust
/// Task CLI actions（workspace-scoped，仿 BlackboardAction）。
#[derive(Debug, Clone, Subcommand)]
pub enum TaskAction {
    /// 查看任务全貌（GET /workspaces/{ws}/tasks/{id}）
    View { #[arg(long="workspace-id")] workspace_id: i64, #[arg(long="task")] task: i64 },
    /// 列出任务（GET /workspaces/{ws}/tasks）
    List { #[arg(long="workspace-id")] workspace_id: i64, #[arg(long)] status: Option<String> },
    /// 讨论帖子命令组
    Posts { #[command(subcommand)] action: TaskPostAction,
            #[arg(long="workspace-id")] workspace_id: i64, #[arg(long="task")] task: i64 },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TaskPostAction {
    List,                              // GET /workspaces/{ws}/tasks/{id}/posts
    Get { pid: i64 },                  // GET /workspaces/{ws}/tasks/{id}/posts/{pid}
}
```

新增 handler（仿 `handle_blackboard`，每个分支 ≤ 数行）：
```rust
async fn handle_task(client: &ApiClient, action: &TaskAction, output: &OutputFormat, fields: &Option<String>) -> Result<()> {
    match action {
        TaskAction::View { workspace_id, task } => {
            let path = format!("{}/tasks/{}", ws_prefix(*workspace_id), task);
            let resp: ClientResponse<Value> = client.get(&path).await?;
            print_response(&resp, output, fields)?;
        }
        TaskAction::List { workspace_id, status } => { /* ?status= 拼 query */ }
        TaskAction::Posts { action, workspace_id, task } => match action {
            TaskPostAction::List => { /* format!("{}/tasks/{}/posts", ws_prefix(*ws), task) */ }
            TaskPostAction::Get { pid } => { /* .../posts/{pid} */ }
        },
    }
    Ok(())
}
```

注册点：
- `Commands` 枚举（`:72-108`）加 `Task { #[command(subcommand)] action: TaskAction }`。
- `run_command`（`:704-712`）match 加 `Commands::Task { action } => handle_task(&client, action, &cli.output, &cli.fields).await?,`。

### 4.2 `main.rs`

- 镜像 `Commands` 枚举（`:38-83`，Process 之后）加 `Task { #[command(subcommand)] action: cli::TaskAction }`。
- dispatch 区块（`:235-275`，参考 `:272 Process`）加：
```rust
Some(Commands::Task { action }) => {
    dispatch_subcommand(&cli, cli::Commands::Task { action: action.clone() }).await;
    return;
}
```

### 4.3 `cli/mod.rs`

`pub use commands::{ ... }`（`:5-9`）补 `TaskAction, TaskPostAction`。

### 4.4 坑

- 两处 `Commands` 必须同步，漏一处编译失败或命令不路由。
- 不重复写 `/api/v1`（`client.rs:22` 自动补）。
- `--workspace-id` 必填（i64）。

---

## 5. 块 2：改造 `build_carrier_prompt`

### 5.1 拆为 3 个小函数（`handlers/task_posts.rs`）

```rust
/// 内联到 carrier prompt 的最近主楼楼盘数；平衡上下文充分性与 prompt 体积。
const DISCUSSION_HISTORY_LIMIT: u64 = 5;
/// 单条历史正文的字符截断上限，防止长帖撑爆 prompt。
const HISTORY_POST_TRUNCATE: usize = 500;

/// 拉最近 N 条主楼层帖子，失败静默回退空 Vec（不阻断 @ 触发，同 inject_* 降级哲学）。
async fn fetch_recent_main_posts(db: &Database, task_id: i64) -> Vec<task_posts::Model> {
    db.list_main_posts_paged(task_id, 1, DISCUSSION_HISTORY_LIMIT).await.unwrap_or_default()
}

/// 把帖子列表格式化为「[作者(身份) 状态] 正文」逐条文本（纯函数，易测）。
fn format_discussion_history(posts: &[task_posts::Model]) -> String { /* 截断 + join */ }

/// carrier prompt 末尾的 ntd 命令提示（指向块 1 新命令）。
const NTD_CMD_HINTS: &str = "## 了解全貌的 ntd 命令（默认连本地 ntd，无需额外参数）\n\
- 任务全貌：ntd task view --workspace-id {ws} --task {id}\n\
- 完整讨论历史：ntd task posts --workspace-id {ws} --task {id} list";
```

### 5.2 改造 `build_carrier_prompt`

签名 `build_carrier_prompt(task, post_content)` → `build_carrier_prompt(task, post_content, history_text)`（保持纯函数，易测）。注入 `task.id` + `task.workspace_id`（回退默认值，避免 Option 格式化麻烦）+ 历史段落 + 命令（`{ws}`/`{id}` 用任务实际值替换）。

### 5.3 串联点（`create_agent_post`，对外签名不变）

在 `:512` `build_carrier_prompt` 调用前，先 `let history = fetch_recent_main_posts(&state.db, task.id).await;` + `let history_text = format_discussion_history(&history);`，再传入。

### 5.4 注入链落点

`build_carrier_prompt` 产出 → `trigger_discussion_execution` → `RunTodoExecutionRequest.message`（`executor_service/mod.rs:56`）→ `inject_expert_context`（`pre_spawn.rs:642`）将其作为「# 任务」段包在专家人设之后。**仅 trigger_type=discussion 路径**生效，不影响普通 todo。

---

## 6. 块 3：完善 `ntd-usage/SKILL.md`

- 「高频命令速查」表（line 144-156）补一行：`查看任务讨论 | ntd task posts --workspace-id <N> --task <id> list`。
- 新增「## 💬 任务讨论区（Task Discussion）」小节（仿「工艺怎么用」line 182-219 风格）：说明 `ntd task view` / `ntd task posts list` / `ntd task posts get` 用途、`--workspace-id` 必填、`--output raw --fields` 精简技巧、被 `@` 的 AI 可用这些命令了解全貌。
- 改完无需改代码：`lib.rs:54 #[folder="../ntd-skills/"]` rust-embed 下次编译自动重嵌入；用户 `ntd skills install`（`main.rs:355 handle_skill_install`）后写入 `~/.claude/skills/ntd-usage/`。

---

## 7. 测试设计

### 7.1 后端单测（`handlers/task_posts.rs` 内 `#[cfg(test)]`，仿 `pre_spawn.rs` 测试风格）

| 用例 | 断言 |
|------|------|
| `format_discussion_history` 多条 | 逐条含作者/身份/状态/正文 |
| `format_discussion_history` 单条超长 | 截断到 `HISTORY_POST_TRUNCATE` |
| `format_discussion_history` 空列表 | 返回空串 |
| `build_carrier_prompt` 含 id/ws | 文本含「任务 #<id>」「工作空间 #<ws>」 |
| `build_carrier_prompt` 含命令 | 文本含 `ntd task view` / `ntd task posts list` 且 ws/id 已替换 |
| `build_carrier_prompt` 空历史 | 历史段为「（暂无历史）」 |

### 7.2 CLI 测试

- 仿现有 CLI 测试模式，单测 `handle_task` 各分支的 path 拼接（`ws_prefix` + `/tasks/...`），或在 `backend/tests/` 加针对 `ntd task posts list` 的集成测试（需 dev server）。

### 7.3 端到端

1. `make dev` → 任务讨论区 `@<默认执行器>` 发帖。
2. `ntd task posts --workspace-id <N> --task <id> list` 拉到讨论。
3. 查 daemon.log / 执行记录，确认执行器 prompt 含任务 id + 历史 + ntd 命令。
4. `ntd skills install` 后检查 `~/.claude/skills/ntd-usage/SKILL.md` 含讨论区小节。

---

## 8. 验证限制说明

- 本机以默认执行器（Claude Code）为主验证 `@` 触发 + prompt 注入；其余执行器只验证 CLI 命令路径。
- CLI 集成测试若依赖 dev server，按现有 `backend/tests/` 模式编写；纯 path 拼接用单测覆盖即可。
- `ntd task view` 复用的 `get_task_detail` 不含 description，已在需求 §10 记为已知点，不阻塞。
