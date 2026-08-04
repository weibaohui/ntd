# 058-CodeBuddy 支持继续对话-需求

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| pi (AI) | 2026-08-04 | 初始版本 |

## 1. 背景与问题

ntd 的执行记录支持「继续对话」（resume）：基于某条历史执行记录的 `session_id`，让执行器恢复原会话上下文继续追问。目前 Claude Code、Kimi、Opencode 等 10 个执行器已支持，但 **CodeBuddy 不在可恢复执行器集合中**，其执行记录上无法发起继续对话。

实测确认 CodeBuddy 执行的 stdout 第一行就是 system init 事件，携带真实 `session_id`（如 `37814b2c-c93e-44ca-8462-bd7fc8d8105c`），且该 session_id 已经由 `CodebuddyExtractor`（EventPipeline）提取并回写到 `execution_records.session_id`——即**数据链路已通，缺的只是「把 session_id 用回 CLI」的适配层实现**。

已验证 CodeBuddy CLI 原生支持恢复会话：

- `-r, --resume [sessionId]`：恢复指定 session 的对话（print 模式 `-p` + `stream-json` 下可用）。
- 传入不存在的 session id 时输出 `{"type":"error","error":"No conversation found with session ID: ..."}`，错误可观测。
- 传入真实 session id 时，后续事件携带同一 `session_id`，会话成功恢复。

## 2. 需求条目

### R1 后端适配器实现 resume 三要素

`backend/src/adapters/codebuddy.rs` 的 `CodebuddyExecutor`：

- 新增 `session_id: Arc<Mutex<Option<String>>>` 状态字段，在解析 system 事件时缓存真实 session_id（与 `ClaudeCodeExecutor` 同款模式）。
- 覆盖 `supports_resume()` 返回 `true`。
- 覆盖 `command_args_with_session()`：`is_resume=true` 且有 session_id 时，在 argv 中插入 `--resume <session_id>`；首次执行（非 resume）不传 `--session-id`，由 CLI 自行生成。
- 覆盖 `extract_session_id()` / `get_session_id()`，保证 EventPipeline 回退路径也能回写 session_id。

### R2 后端可恢复执行器集合登记

`backend/src/adapters/mod.rs` 的 `RESUMABLE_EXECUTORS` 常量加入 `"codebuddy"`，与前端保持同步。

### R3 前端开放「继续对话」入口

`frontend/src/utils/executors.tsx` 的 `EXECUTORS` 中 codebuddy 条目增加 `resumable: true`，使 `supportsResume(record)` 对 codebuddy 执行记录返回 true，前端展示继续对话按钮。

## 3. 边界与非目标

- 不做 CodeBuddy session 目录扫描器（`handlers/session.rs` 的 session 浏览功能），那是独立特性。
- 不为 codebuddy 增加 `--model` 注入（`set_exec_model`），属另一需求（执行器指定模型）。
- 不改动 codebuddy system 事件的日志展示行为（`Session init` 条目维持现状）。

## 4. 验收标准

1. codebuddy 执行记录（非 running、DB 有 session_id）上，前端出现「继续对话」入口并可发起。
2. 发起继续对话后，后端实际执行的 argv 含 `--resume <session_id>`。
3. `cargo clippy --all-targets -- -D warnings` 零告警、`cargo test` 全通过、`npx tsc --noEmit` 零错误。
