# 059-Codex 会话入库与继续对话-需求

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| pi (AI) | 2026-08-04 | 初始版本 |

## 1. 背景与问题

继 058（CodeBuddy 继续对话）后排查全部 13 个执行器的 resume 链路，发现 **Codex 存在「双重缺失」**：

1. **session_id 实际不入库**：`CodexExtractor` 只从 `session_configured` / `task_started` 事件的 `session_id` 字段提取会话 ID，但当前版本 Codex CLI 的真实输出（见 `docs/samples/codex/output.txt`）是 `{"type":"thread.started","thread_id":"019f13f6-..."}` —— extractor 没有 `thread.started` 分支，该事件落入 `_` 兜底变成 Info 日志，`session_id` 永远提取不到，`execution_records.session_id` 恒为 NULL。
2. **不能继续对话**：`CodexExecutor` 未覆盖 `supports_resume()`（默认 false），`command_args_with_session()` 是忽略 session 的桩实现；前端 codex 条目也无 `resumable` 标志。

Codex CLI 原生支持恢复会话：`codex exec resume [SESSION_ID] [PROMPT]`（`--json` 等 exec 选项同样适用），`thread_id` 即 resume 凭据。

**同时确认 AtomCode 不在本次范围**：atomcode 4.25.7 headless 模式仅支持 `-c/--continue`（继续最近一次会话）与 TUI 内 `/resume`，无「按 session id 恢复」的 CLI 能力，ntd 侧无法正确实现 per-record resume，维持不可恢复。

## 2. 需求条目

### R1 extractor 补齐 thread_id 提取（入库）

`backend/src/execution_events/impls/codex.rs`：

- 新增 `thread.started` 事件分支：提取 `thread_id` 写入 `metadata.session_id` 并产出 `SessionStart` 事件（仅首次），使 `execution_records.session_id` 能回写真实会话 ID。
- 保留既有 `session_configured` / `task_started` + `session_id` 旧格式路径，向后兼容。

### R2 适配器实现 resume 三要素

`backend/src/adapters/codex.rs` 的 `CodexExecutor`：

- 新增 `session_id: Arc<Mutex<Option<String>>>` 状态字段；解析 `thread.started`（新格式）与 `session_configured`（旧格式）时缓存会话 ID。
- 覆盖 `supports_resume()` 返回 `true`。
- 覆盖 `command_args_with_session()`：resume 时构造 `codex exec resume [flags] <session_id> <message>`（保留 `-m` 模型注入、`--json`、`--dangerously-bypass-approvals-and-sandbox`、`--skip-git-repo-check`）；非 resume 走既有 `command_args`。
- 覆盖 `extract_session_id()` / `get_session_id()`，覆盖 EventPipeline 回退路径的 DB 回写。

### R3 前后端集合登记

- `backend/src/adapters/mod.rs` `RESUMABLE_EXECUTORS` 加入 `"codex"`。
- `frontend/src/utils/executors.tsx` codex 条目加 `resumable: true`。

## 3. 边界与非目标

- AtomCode 不做（CLI 无 resume-by-id 能力，见背景）。
- 不改动 codex 其余事件解析逻辑与多 agent（collab_tool_call）提取。

## 4. 验收标准

1. codex 执行记录的 `session_id` 列为真实 thread_id（入库）。
2. 非 running 且有 session_id 的 codex 记录前端出现「继续对话」回复框。
3. resume 请求产生的新记录 argv 为 `codex exec resume ... <session_id> <message>`。
4. `cargo clippy --all-targets -- -D warnings` 零告警、`cargo test` 通过、`npx tsc --noEmit` 零错误。
