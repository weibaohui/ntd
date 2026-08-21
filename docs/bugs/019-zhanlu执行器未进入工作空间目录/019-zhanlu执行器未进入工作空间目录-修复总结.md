# NTD-019 zhanlu 执行器执行时未进入工作空间目录 — 修复总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-08-18 | 初始版本 |

---

## 1. 修复方案（一句话）

在两个 spawn 点把生效目录以 `--dir <path>` 注入 zhanlu 的 argv（插在 message 位置参数之前），进程 cwd 照旧设置；其余 10 家执行器 argv 逐字节不变。方案选型与根因见同目录《缺陷分析》。

## 2. 改动清单

| 文件 | 改动 | 说明 |
|------|------|------|
| `backend/src/adapters/mod.rs` | trait 新增默认方法 `dir_arg()` + 纯函数 `insert_dir_arg()` | 默认 `None` 让其余执行器零改动；注入位置取 message 前 |
| `backend/src/adapters/step_protocol.rs` | `StepProtocolFlavor::dir_flag()` + `StepProtocolExecutor::dir_arg()` 覆写 | 仅 Zhanlu 返回 `Some("--dir")`，kilo/opencode/mimo 为 `None` |
| `backend/src/executor_service/spawn_lifecycle.rs` | `spawn_executor_child` 注入 | 目录用 `runtime.effective_workspace_path`（worktree 启用时为 worktree 路径） |
| `backend/src/services/executor_session.rs` | `spawn_and_stream` 注入 | 目录用 `SessionSpawnConfig.cwd`（飞书直连对话 / Wiki 对话通路） |

覆盖的触发通路：事项执行、飞书单聊/群聊直连对话（dm_chat / butler_chat）、黑板 Wiki 对话、环路步骤（经事项执行主链路）。

## 3. 行为对照

| 场景 | 修复前 | 修复后 |
|------|--------|--------|
| zhanlu + 工作空间 | argv 无目录，zl 不跟随 cwd，落错目录 | `zl run --format json --dir <workspace> ... "<message>"` |
| zhanlu + worktree 启用 | 同上 | `--dir <worktree 路径>`（与 cwd 同源） |
| zhanlu + Wiki 对话 | argv 无目录 | `--dir <wiki 目录>` |
| zhanlu + 未配置工作空间 | argv 无目录，仅 cwd | 不注入（无目录可传），与现状一致 |
| 其余 10 家执行器 | 仅 cwd | 仅 cwd，argv 逐字节不变（单测锁定） |
| 非 UTF-8 路径 | 仅 cwd | `to_str()` 失败不注入，退化为现状，不 panic |

## 4. 测试

新增 5 个单测（全部通过）：

- `adapters::tests::test_insert_dir_arg_目录参数插在message之前` — 锁定插入位置与最终 argv 形态
- `adapters::tests::test_insert_dir_arg_执行器无目录flag时argv原样` — 其余执行器零变化
- `adapters::tests::test_insert_dir_arg_无生效目录时argv原样` — 无目录不注入
- `adapters::tests::test_insert_dir_arg_空argv不注入` — 防御分支
- `step_protocol::tests::test_dir_arg_仅zhanlu返回flag其余flavor为none` — flavor 差异

回归：既有 `effective_workspace_path` 系列、`executor_session` spawn 骨架测试全部保持通过，证明默认路径零行为变化。

## 5. 验证记录

- `cd backend && cargo clippy --all-targets -- -D warnings` → 零告警零错误
- `cd backend && cargo test` → 1791 passed / 0 failed / 2 ignored
- 本机未安装 `zl` 二进制，端到端（飞书发消息 → zhanlu 汇报 pwd）待用户在装有 zl 的环境实测；argv 形态已由单测锁定。

## 6. 环境备注（与本缺陷无关但影响验证）

- `frontend/dist` 与 `backend/target` 曾被清理，导致 `make dev` 构建失败（rust_embed 找不到 dist 目录 → `Assets::get` 缺失 → 4 个编译错误）。本次已通过 `cd frontend && npm run build` 重建 dist 后全部编译通过。
