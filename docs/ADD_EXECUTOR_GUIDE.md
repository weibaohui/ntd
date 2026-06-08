# 新增执行器集成指南

本文说明在 Nothing Todo 中新增一个 AI 执行器需要完成的工作。执行器的核心边界在后端 `CodeExecutor` trait，前端只负责展示和选择执行器值。

## 1. 确认执行器 CLI 能力

新增前先确认目标 CLI 支持非交互模式，并明确这些信息：

- 可执行文件名和配置路径字段，例如 `codex`。
- 非交互执行参数，例如 `exec --json <prompt>`。
- stdout/stderr 输出格式，优先使用 JSONL 或结构化格式。
- 是否需要跳过权限确认、是否支持 session id、是否会在成功时返回非 0 退出码。
- usage、model、工具调用、最终结果分别从哪个事件读取。

如果执行器只能输出普通文本，也可以接入，但至少要能从输出中稳定提取最终结果。

## 2. 新增后端 adapter

在 `backend/src/adapters/` 下新增文件，例如 `my_executor.rs`，实现：

- `CodeExecutor::executor_type`
- `CodeExecutor::executable_path`
- `CodeExecutor::command_args`
- `CodeExecutor::parse_output_line`
- `CodeExecutor::parse_stderr_line`，如果 stderr 里也有结构化事件
- `CodeExecutor::get_final_result`
- `CodeExecutor::get_usage`
- `CodeExecutor::get_model`
- 必要时覆盖 `check_success`

建议优先参考这些现有实现：

- `claude_code.rs`：Claude 风格 stream-json。
- `opencode.rs` / `joinai.rs`：事件流中包含 step、tool、text、tokens。
- `kimi.rs`：OpenAI chat message 风格 JSONL。
- `codex.rs`：宽松解析不同 JSONL 事件字段。

输出日志应映射到 `ParsedLogEntry` 的常用类型：

- `text`：模型输出文本，也是多数执行器的最终结果来源。
- `thinking`：推理过程。
- `tool_call` / `tool_use` / `tool`：工具调用。
- `tool_result`：工具结果。
- `result`：执行器最终结果事件。
- `tokens`：token 使用统计。
- `error` / `stderr`：错误信息。

## 3. 注册执行器类型

需要修改这些后端入口：

1. `backend/src/models/mod.rs`
   - 给 `ExecutorType` 增加新 variant。
   - 在 `ExecutorType::as_str` 中返回持久化字符串。

2. `backend/src/adapters/mod.rs`
   - 增加 `pub mod my_executor;`。
   - 在 `parse_executor_type` 中加入主名称和别名。
   - 在 `EXECUTORS` 数组中加一条 `ExecutorDef`（name / executor_type / binary_name / display_name / default_path / session_dir / aliases）。

3. `backend/src/config.rs`
   - `ExecutorPaths` 是 `HashMap<String, String>` 结构，在 `Default` 中不需要为新执行器加字段；
     在 `Config.executors.paths` 写一个键值对即可，例如 `codex -> "codex"`（运行时由 `db.sync_new_executors` 自动 seed）。

4. `backend/src/main.rs`
   - **新执行器会被 DB 自动注册**：`main.rs` 启动流程中调用 `db.seed_default_executors()` 与 `db.sync_new_executors()`，把 `EXECUTORS` 数组里新增的执行器同步到 `executors` 表，再通过 `executor_registry.register_by_name(...)` 自动注册。
   - 若执行器在注册表中找不到，会打 `warn!` 日志跳过。

5. `backend/src/cli/commands.rs`
   - 更新 `executor` 参数 help 文本中的执行器列表。

## 4. 更新前端选择项

修改 `frontend/src/types/execution.tsx:121` 的 `EXECUTORS` 数组：

- `value` 必须等于 `ExecutorType::as_str()` 返回值。
- `label` 是 UI 展示名称。
- `color` 和 `icon` 用于列表 badge 和选择器。

如果新执行器引入新的日志类型，还要同步更新 `LogEntry['type']` union，并检查 `ExecutionPanel` 是否需要特殊渲染。

## 5. 增加测试

至少补这些测试：

- adapter 单测：命令参数、执行器类型、关键 JSONL/文本解析、usage/model 提取。
- `backend/tests/business_logic_tests.rs`：`ExecutorType::to_string` 和 `parse_executor_type`。
- `backend/tests/cli_command_tests.rs`：`ExecutorPaths::default` 和配置结构构造。
- 如执行器有特殊成功判定，补 `check_success` 测试。

建议执行：

```bash
cargo test --manifest-path backend/Cargo.toml
```

如果改了前端类型或展示，再执行：

```bash
cd frontend
pnpm test
```

## 6. 本次 Codex 集成示例

Codex 集成改动可以作为模板参考：

- adapter：`backend/src/adapters/codex.rs`
- 类型解析：`ExecutorType::Codex` 和 `parse_executor_type("codex")`
- 默认路径：`Config.executors.codex = "codex"`
- 注册：`CodexExecutor::new(cfg.executors.codex.clone())`
- 前端选项：`{ value: "codex", label: "Codex", ... }`

Codex 使用 `codex exec --json --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check <prompt>` 接入现有执行管线。它的 JSONL 事件由 adapter 宽松解析，以便兼容不同版本 CLI 的字段差异。
