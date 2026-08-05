# 059-Codex 会话入库与继续对话-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| pi (AI) | 2026-08-04 | 初始版本 |

## 1. 总体思路

沿用 058（CodeBuddy）验证过的模式，但 Codex 多一步「修入库」：extractor 补 `thread.started`/`thread_id` 提取后，适配器按统一三要素（session_id 缓存 + `supports_resume` + `command_args_with_session`）对齐，最后前后端集合登记。

## 2. 现状与缺口

| 环节 | 现状 | 缺口 |
|------|------|------|
| extractor 入库 | 只认 `session_configured`/`task_started` 的 `session_id`；真实输出 `thread.started`/`thread_id` 落入 `_` → Info | **不入库** |
| `supports_resume()` | 未覆盖 → false，handler 400 | 需覆盖 |
| `command_args_with_session()` | 桩实现，忽略 session | 需拼 `exec resume` |
| `extract_session_id()`/`get_session_id()` | 未覆盖 | 回退路径回写需要 |
| 后端 `RESUMABLE_EXECUTORS` / 前端 `resumable` | 均无 codex | 需登记 |

真实输出证据（`docs/samples/codex/output.txt` 第 9 行）：
`{"type":"thread.started","thread_id":"019f13f6-4be4-74f1-8b77-74fe3878091c"}`

## 3. 详细改动

### 3.1 `execution_events/impls/codex.rs`（入库）

新增 match 分支（置于 `turn.started` 附近）：

```rust
"thread.started" => {
    // thread_id 即 codex exec resume 的会话凭据
    if let Some(tid) = json.get("thread_id").and_then(|v| v.as_str()) {
        if self.metadata.session_id.is_none() {
            self.metadata.session_id = Some(tid.to_string());
            events.push(ExecutionEvent::SessionStart { session_id: tid.to_string() });
        }
    }
}
```

旧 `session_configured`/`task_started` 路径保留：早期 codex 版本用该格式，两条路径都受 `session_id.is_none()` 判重保护，先到先赢。

### 3.2 `adapters/codex.rs`（可继续对话）

- 结构体加 `session_id: Arc<Mutex<Option<String>>>`（Arc 共享语义同 BaseExecutor，Clone 自动派生）。
- `parse_output_line` 的 `thread.started` 分支：除产出既有 "Codex thread started" 条目外，缓存 `thread_id`。
- `supports_resume()` → `true`。
- `command_args_with_session(message, session_id, is_resume)`：
  - `is_resume && Some(sid)`：`exec resume [-m <model>] --json --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check <sid> <message>`。
  - 其余情况：回退 `command_args(message)`（新会话，与现状一致）。
  - argv 顺序依据：`codex exec resume [OPTIONS] [SESSION_ID] [PROMPT]`，OPTIONS 与 exec 共享（clap 解析与位置无关），sid 在前、prompt 殿后。
- `extract_session_id(line)`：命中 `thread.started`.`thread_id` 或旧格式 `session_id` 则更新缓存并返回；否则返回缓存。
- `get_session_id()` → 缓存克隆。

### 3.3 登记

- `mod.rs` `RESUMABLE_EXECUTORS` 插 `"codex"`（紧跟 codebuddy 后，保持同类聚集）。
- `executors.tsx` codex 条目追加 `resumable: true`。

## 4. 测试设计

extractor 新增：

| 用例 | 断言 |
|------|------|
| `thread.started` 带 thread_id | 产出 SessionStart，metadata.session_id = tid |
| 重复 `thread.started` | 只产出一次 SessionStart（判重） |
| 旧格式 `session_configured` | 仍产出 SessionStart（回归保护） |

adapter 新增：

| 用例 | 断言 |
|------|------|
| resume + sid | argv = `exec resume ... <sid> <msg>`，含 `--json`/bypass/skip-git 与 `-m`（注入模型时） |
| 非 resume | argv 不含 `resume`，与 `command_args` 一致 |
| resume + None sid | 回退新会话 argv（防御） |
| `supports_resume` | true |
| `extract_session_id` thread.started / 旧格式 / 空行 | 返回并缓存 / 返回并缓存 / 回退缓存或 None |

mod.rs 新增：RESUMABLE_EXECUTORS 含 codex、`create_executor("codex")` 的 supports_resume 断言。

## 5. 验证限制说明

本机未安装 codex 二进制，CLI 级实测（真实 resume 会话关联）无法执行；`exec resume` 参数语义依据 Codex CLI 公开文档与 samples 格式确认。端到端验证以「resume API → 记录 argv → argv 符合 `exec resume [flags] <sid> <msg>`」为准，CLI 行为待有 codex 环境时复核。
