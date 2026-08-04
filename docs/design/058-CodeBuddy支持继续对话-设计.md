# 058-CodeBuddy 支持继续对话-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| pi (AI) | 2026-08-04 | 初始版本 |

## 1. 总体思路

完全复刻 `ClaudeCodeExecutor` 已验证的 resume 模式到 `CodebuddyExecutor`：两者同为 Claude Protocol（stream-json NDJSON），CLI 参数语义一致（`--resume <sessionId>`）。改动是「适配器状态 + 3 个 trait 方法覆盖 + 2 处集合登记」，无架构变更。

## 2. resume 链路现状（无需改动的部分）

```
前端继续对话按钮 → POST /execution-records/{id}/resume
  → handlers/execution.rs::resume_execution
      ├─ executor.supports_resume()          ← 缺口 1（默认 false，直接 400）
      ├─ resolve_resume_session_id(record)   ← 依赖 DB session_id，已由
      │     CodebuddyExtractor SessionStart 事件回写，无需改动 ✓
      └─ start_todo_execution(resume_session_id)
            → pre_spawn::build_executor_command_args
                  └─ executor.command_args_with_session(msg, Some(sid), true)  ← 缺口 2
```

执行期日志链路（`log_capture::spawn_stdout_reader`）：EventPipeline 优先解析，`pipeline.metadata().session_id` 首次出现即回写 DB；pipeline 无事件产出时回退 `executor.extract_session_id(line)` ← 缺口 3。

## 3. 详细改动

### 3.1 `backend/src/adapters/codebuddy.rs`

- 结构体新增 `session_id: Arc<Mutex<Option<String>>>`（`#[derive(Clone)]` 由 BaseExecutor 同款 Arc 共享语义覆盖）。
- `handle_system`：除缓存 model 外，同步缓存 `session_id`（副作用与 Claude Code 对齐；日志条目行为不变，见需求 3 边界）。
- `supports_resume()` → `true`。
- `command_args_with_session(message, session_id, is_resume)`：
  - argv 顺序与 claude_code 对齐：`-p --output-format stream-json [--resume <sid>] --verbose --permission-mode bypassPermissions <message>`。
  - 仅 `is_resume && session_id.is_some()` 时插 `--resume`；`is_resume=true` 但 sid 为 None 时静默降级为新会话（与 claude_code 行为一致——理论上 handler 层已拦截 None，这里只是防御）。
  - 非 resume 不传 `--session-id`：首次执行让 CLI 自生成，避免冲突。
- `extract_session_id(line)`：行内解析出 system.session_id 则更新缓存并返回；否则返回缓存值（handle_system 已写入）。
- `get_session_id()` → 缓存克隆。

### 3.2 `backend/src/adapters/mod.rs`

`RESUMABLE_EXECUTORS` 加入 `"codebuddy"`（该常量当前仅测试引用，作为前后端同步的权威声明）。

### 3.3 `frontend/src/utils/executors.tsx`

`EXECUTORS` codebuddy 条目追加 `resumable: true`；`RESUMABLE_EXECUTORS` Set 由 filter 自动派生，无需手改。

### 3.4 resume 后 session_id 不被覆盖

resume 场景 `initial_session_id = Some(sid)` → `session_id_updated` 初始即 true → 执行期跳过 DB 覆盖；且 codebuddy resume 后 system 事件携带的仍是同一 session_id，语义自洽。

## 4. 测试设计

新增单元测试（codebuddy.rs）：

| 用例 | 断言 |
|------|------|
| resume + sid | argv 含 `--resume <sid>`，位置在 stream-json 之后 |
| 新执行（is_resume=false，带 sid） | argv 不含 `--resume` |
| resume + None sid | argv 不含 `--resume`（防御降级） |
| `supports_resume` | true |
| `extract_session_id` 从 system 行 | 返回并缓存 sid |
| `extract_session_id` 空行/无 sid 行 | 回退缓存值或 None |
| `get_session_id` 初始 | None |

mod.rs 新增：`RESUMABLE_EXECUTORS` 含 codebuddy 的断言测试。

## 5. 验证记录（设计期实测）

- `codebuddy -p --output-format stream-json --resume 00000000-... "hi"` → `{"type":"error","error":"No conversation found with session ID: ..."}`：flag 被接受，错误可观测。
- 真实会话 `f3c866f3-...` resume → 后续事件 `session_id` 一致，会话成功关联（本机未登录导致模型调用失败，属环境问题，与 resume 机制无关）。
