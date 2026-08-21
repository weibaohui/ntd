# NTD-019 zhanlu 执行器执行时未进入工作空间目录 — 缺陷分析

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-08-18 | 初始版本 |

---

## 1. 根因（Root Cause）

目录在 ntd 内部的传递链是「进程 cwd」单通道：

```
生效目录(effective_workspace_path / wiki 目录)
  └→ tokio::process::Command::current_dir()      ← 唯一传递方式
       └→ 子进程继承 cwd
            └→ 执行器 CLI 自行决定是否使用 cwd
```

- 事项执行通路：`spawn_lifecycle.rs` `spawn_executor_child` → `build_executor_command(workspace_path)` 设置 `current_dir`。
- 直连对话 / Wiki 对话通路：`executor_session.rs` `spawn_and_stream` 设置 `current_dir`。
- 全部 11 个适配器的 argv 拼装（`command_args` / `command_args_with_session`）都**不含目录参数**。

zhanlu（`zl`）的目录语义与这条链不兼容：`zl run` 以 `--dir <path>` 为准、不跟随进程 cwd。于是 cwd 设置了但 zl 不读，目录在「ntd → zl」边界断裂。

## 2. 为什么 argv 里没有目录

现有 `CodeExecutor` trait 的 argv 拼装方法（`command_args(&self, message)` / `command_args_with_session(&self, message, session_id, is_resume)`）**拿不到目录**：目录不是执行器的构造期属性，而是每次执行期的上下文。当初设计时所有目标 CLI 都跟随 cwd，argv 不需要目录；zhanlu 是第一个例外。

## 3. 方案选型

### 候选 A：给 trait 方法加 cwd 参数（改签名）

把 `command_args_with_session` 加一个 `dir: Option<&str>` 参数，由各适配器自行拼进 argv。

- 否决理由 ①：波及全部 11 个适配器与其测试，只有 1 家需要该参数，噪声极大。
- 否决理由 ②（决定性）：**事项执行通路的 argv 构造发生在 worktree 创建之前**（`pre_spawn.rs` `select_executor_and_build_command` 先拼 argv，`stages.rs` 之后才解析 `effective_workspace_path = worktree 路径 > todo.workspace_path`）。在 argv 构造点拿到的目录是 todo.workspace_path，worktree 启用时会传错目录；要传对必须把 argv 构造推迟到 spawn 期，重构面失控。

### 候选 B：`set_exec_cwd` 仿照 `set_exec_model` 的注入模式

在执行前把目录写进执行器实例内部状态，argv 拼装时读取。

- 否决理由：registry 的执行器实例是 per-type 单例（Arc 共享），`set_exec_model` 靠「注入与构建之间无 await」保原子（`pre_spawn.rs:282-284` 注释）；同样把注入挪到 spawn 期（worktree 之后）才能拿对目录，等于绕开既有原子性约定，引入并发覆盖风险。

### 候选 C（采用）：spawn 层注入目录参数

trait 加一个**默认方法** `dir_arg(&self) -> Option<&'static str>`（默认 `None`），只有 zhanlu 所在的 `StepProtocolExecutor`（Zhanlu flavor）返回 `Some("--dir")`；两个 spawn 点在拿到最终生效目录之后，把 `--dir <生效目录>` 插入 argv：

- 时机正确：注入发生在 `effective_workspace_path` 解析完成之后，worktree 场景天然传 worktree 路径。
- 波及面最小：不改任何既有方法签名，其余 10 家 `dir_arg()` 返回 `None`，argv 逐字节不变。
- 直连对话通路（`executor_session.rs`）与 Wiki 对话通路（同文件）注入点用 `SessionSpawnConfig.cwd`，语义与进程 cwd 完全同源。

### 插入位置：message 之前

所有适配器的 argv 都以 message 作为**最后一个位置参数**。clap 系 CLI 的 flag 必须出现在位置参数之前才稳妥（trailing flag 是否可解析取决于各家实现，不赌）。因此插入点取 `argv.len() - 1`（message 前一位），仅当 `dir_arg()` 为 `Some` 且生效目录存在时才动 argv。

## 4. 修复设计（候选 C 落地）

### 4.1 trait 默认方法（`adapters/mod.rs`）

```rust
fn dir_arg(&self) -> Option<&'static str> { None }
```

语义：返回 `Some(flag)` 表示该执行器不跟随进程 cwd，spawn 层把 `flag <生效目录>` 插到 argv 的 message 之前；`None` 表示只依赖进程 cwd。

### 4.2 flavor 差异点（`step_protocol.rs`）

`StepProtocolFlavor::dir_flag(self)`：仅 `Zhanlu` 返回 `Some("--dir")`；kilo/opencode/mimo 返回 `None`。`StepProtocolExecutor` 覆写 `dir_arg` 委托给 flavor。

### 4.3 注入函数（`adapters/mod.rs`）

`insert_dir_arg(args, dir_flag, dir) -> Vec<String>` 纯函数：flag 或 dir 缺一、argv 为空时原样返回；否则在 message 前插入 `flag`、`dir` 两元素。

### 4.4 两个注入点

| 通路 | 文件 | 目录来源 |
|------|------|---------|
| 事项执行 | `spawn_lifecycle.rs` `spawn_executor_child` | `runtime.effective_workspace_path` |
| 直连对话 / Wiki 对话 | `executor_session.rs` `spawn_and_stream` | `SessionSpawnConfig.cwd` |

进程 cwd 照旧设置（`current_dir`），`--dir` 与 cwd 双通道一致——即使 zl 未来改为跟随 cwd 也不会错。

## 5. 风险与边界

- **非 UTF-8 路径**：`PathBuf::to_str()` 失败时不注入 `--dir`（退化为现状的仅 cwd），不 panic。
- **无工作空间的执行**：`effective_workspace_path = None` 时不注入，argv 与现状一致。
- **zl 解析顺序**：采用「message 之前」的保守插入顺序；如 zl 实测要求其他位置（如 `run` 子命令前），只需调整 `insert_dir_arg` 一处。
- **并发**：注入读取的是 runtime/config 内的只读值，不触碰执行器共享状态，无 `set_exec_model` 类原子性约束。

## 6. 测试策略

- `adapters/mod.rs`：`insert_dir_arg` 单测——插入位置（message 前）、flag 为 None 原样、dir 为 None 原样、空 argv 原样。
- `step_protocol.rs`：`dir_arg` 单测——仅 Zhanlu 返回 `Some("--dir")`，kilo/opencode/mimo 为 `None`。
- 既有回归：`spawn_lifecycle.rs` 的 `effective_workspace_path` 系列、`executor_session.rs` 的 spawn 骨架测试保持通过，证明默认路径（其余执行器）零行为变化。
