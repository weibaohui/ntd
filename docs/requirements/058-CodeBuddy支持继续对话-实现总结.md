# 058-CodeBuddy 支持继续对话-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| pi (AI) | 2026-08-04 | 初始版本 |

## 1. 需求对应

| 需求条目 | 实现 | 状态 |
|---------|------|------|
| R1 后端适配器 resume 三要素 | `backend/src/adapters/codebuddy.rs` | ✅ |
| R2 RESUMABLE_EXECUTORS 登记 | `backend/src/adapters/mod.rs` | ✅ |
| R3 前端开放继续对话入口 | `frontend/src/utils/executors.tsx` | ✅ |

## 2. 关键实现点

1. **`CodebuddyExecutor` 新增 `session_id: Arc<Mutex<Option<String>>>`**：`handle_system` 解析 system 事件时同步缓存真实 session_id（与 `ClaudeCodeExecutor` 同款模式，克隆体共享 Arc 状态）。
2. **`supports_resume()` → true**：放行 `handlers/execution.rs` 的 resume 校验。
3. **`command_args_with_session()`**：`is_resume && sid.is_some()` 时在 `stream-json` 之后、`--verbose` 之前插 `--resume <sid>`（与 claude_code argv 布局一致）；首次执行不传 `--session-id`，由 CLI 自生成；resume 但无 sid 时静默降级（handler 层已先行拦截）。
4. **`extract_session_id()` / `get_session_id()`**：覆盖 EventPipeline 无事件产出时的回退回写路径。
5. **前端 codebuddy 条目加 `resumable: true`**：`RESUMABLE_EXECUTORS` Set 自动派生，`supportsResume(record)` 放行，帖子页渲染回复输入框。
6. **存量集成测试反转**：`adapter_extended_tests.rs::test_supports_resume` 原断言「不支持 resume」，同步反转为「支持」并补 argv 断言。

## 3. 测试与验证

### 单元/集成测试

- `cargo clippy --all-targets -- -D warnings`：零告警 ✅
- `cargo test`：1599 通过；新增 10 个用例（codebuddy.rs 8 个、mod.rs 2 个、adapter_extended_tests.rs 1 改 1 增）。
- 存量失败 `git_sync::tests::test_sync_repo_restores_deleted_file` 与本次无关（main 上同样失败，根因是系统 git 版本不支持 `git init -b`）。
- 前端 `npx tsc --noEmit`：零错误 ✅

### 功能验证（dev 环境，Playwright + 真实 API）

预置记录：todo 8 下 id=40（codebuddy + session）、id=41（atomcode + session，对照组）。

1. **前端入口**：`frontend/tests/check_codebuddy_resume.spec.ts` 通过——codebuddy 记录帖子页出现回复输入框，atomcode 记录不出现。
2. **resume API**：`POST /api/v1/workspaces/1/executions/40/resume` → 200，新记录 id=42。
3. **argv 正确性**：记录 42 实际命令为 `codebuddy -p --output-format stream-json --resume cb-sess-verify-058 --verbose --permission-mode bypassPermissions <message>`，`--resume` 位置符合设计。
4. **CLI 可达性**：CodeBuddy 返回 `{"type":"error","error":"No conversation found with session ID: cb-sess-verify-058"}`（假 sid 的预期错误），证明 flag 被 CLI 正确接收处理；新记录继承原 session_id 未被覆盖。
5. **设计期真机验证**：真实会话 `f3c866f3-...` resume 后事件流 session_id 一致，会话成功关联。

## 4. 已知限制

- 本机 codebuddy 未登录时 print 模式模型调用报 "Authentication required"，属环境问题，与 resume 机制无关。
- resume 一个不存在的 session_id 时 CLI 输出 `{"type":"error"}` 行，当前按 text 条目落库（与 claude_code 行为一致），未做特化展示。
