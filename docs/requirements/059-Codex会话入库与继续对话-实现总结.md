# 059-Codex 会话入库与继续对话-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| pi (AI) | 2026-08-05 | 初始版本 |

## 1. 需求对应

| 需求条目 | 实现 | 状态 |
|---------|------|------|
| R1 extractor 补齐 thread_id 提取 | `backend/src/execution_events/impls/codex.rs` 新增 `thread.started` 分支 | ✅ |
| R2 适配器 resume 三要素 | `backend/src/adapters/codex.rs` session_id 缓存 + `supports_resume` + `command_args_with_session` + `extract_session_id`/`get_session_id` | ✅ |
| R3 前后端集合登记 | `backend/src/adapters/mod.rs` + `frontend/src/utils/executors.tsx` | ✅ |

## 2. 关键实现点

1. **修入库**：extractor 新增 `thread.started` 分支提取 `thread_id` → `metadata.session_id` + `SessionStart` 事件（`is_none()` 判重只记首次）。真实输出证据 `docs/samples/codex/output.txt` 第 9 行。旧格式 `session_configured`+`session_id` 路径保留，两条路径先到先赢。
2. **resume argv**：`codex exec resume [-m <model>] --json --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check <session_id> <message>`——`exec resume` 独立子命令，flags 与 exec 共享，sid 在前 prompt 殿后；`-m` 与 `command_args` 同源（`set_exec_model` 注入）。非 resume 或 sid 为 None 回退既有 `command_args`。
3. **session_id 缓存**：适配器 `parse_output_line` 的 `thread.started` 分支同步缓存 thread_id（展示条目行为不变）；`extract_session_id` 复用自由函数 `extract_sid_from_line` 兼容新旧两种格式，未命中回退缓存。
4. **登记**：后端 `RESUMABLE_EXECUTORS` 插 `"codex"`；前端 codex 条目 `resumable: true`（Set 自动派生）。

## 3. 测试与验证

### 单元/集成测试

- `cargo clippy --all-targets -- -D warnings`：零告警 ✅
- `cargo test`：1612 通过（较 main +16：extractor 4、adapter 10、mod.rs 2）；唯一失败 `git_sync::tests::test_sync_repo_restores_deleted_file` 为存量环境问题（系统 git 不支持 `git init -b`，main 上同样失败）✅
- `npx tsc --noEmit`：零错误 ✅

### 功能验证（dev 环境）

1. **前端入口**：`frontend/tests/check_codex_resume.spec.ts` 通过——预置 codex 记录（success + thread_id 形式 session_id）帖子页出现回复输入框。
2. **resume API**：`POST /api/v1/workspaces/1/executions/44/resume` → 200（改动前必 400），新记录生成。
3. **argv 正确性**：新记录 command 实测为 `codex exec resume --json --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check 019f13f6-4be4-74f1-8b77-74fe3878091c <message>`，与设计完全一致；session_id 正确继承未被覆盖。

## 4. 已知限制与说明

- **本机无 codex 二进制**，CLI 级真实会话恢复未能实测；`exec resume` 参数语义依据 Codex CLI 公开文档与仓库 samples 确认，待有 codex 环境时复核（设计文档 §5 已记录）。
- **预存现象（非本次引入）**：执行器二进制缺失时执行记录停留在 running（无 pid、无日志）。用 kilo（已支持 resume、本机同样缺二进制）对照复现，行为完全一致，属 spawn 失败处理的 executor 无关存量行为，建议另行立项排查。
- **AtomCode 维持不可恢复**：atomcode 4.25.7 headless 仅支持 `-c/--continue`（继续最近一次会话）与 TUI 内 `/resume`，无 resume-by-id 能力，ntd 侧无法正确实现 per-record 继续对话。
