# 012-kilo系执行器非零退出码被误判失败-修复总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |

> 缺陷说明 / 缺陷分析见同目录前文。本缺陷在 093 优化扫描专项第 5 项（执行器双解析体系盘点）中发现。

## 1. 修复方案（对应分析文档方案 C）

新增 `CodeExecutor::on_pipeline_event` 观察钩子，把 pipeline 已解析的 `ExecutionEvent` 在广播前回授给执行器，恢复被跳过的流式副作用状态同步：

| 文件 | 改动 |
|------|------|
| `adapters/mod.rs` | trait 新增 `on_pipeline_event` 默认空实现（不依赖流式状态的 9 个执行器无感） |
| `adapters/{kilo,mimo,opencode,zhanlu}.rs` | 实现钩子：`StepStart → flag=false`、`StepFinish → flag=true`（与旧 handler 语义逐字对齐） |
| `executor_service/log_capture.rs` | `parse_and_broadcast` / `try_parse_stderr_with_pipeline` 增加 `executor` 参数，事件广播前回授；stdout/stderr 两个 reader 调用点同步 |
| `services/message_debounce.rs` | 标准广播路径调用点同步传 executor |

## 2. 为什么选这个方案

- **零重复解析**：事件已由 pipeline 解析，回授只是方法调用；备选方案 A（命中后再调旧 `parse_output_line`）每行二次 JSON 解析且会产生重复日志条目；
- **语义零漂移**：钩子实现与旧 handler 的 flag 操作逐字对齐，`check_success` 判定逻辑不变；
- **影响面可控**：默认空实现，其余 9 个执行器与全部调用方行为不变。

## 3. 验证结果

- **回归测试** `test_parse_and_broadcast_syncs_executor_success_flag`（log_capture 测试区）：
  真实组合 `KiloExtractor` pipeline 处理 step-finish → 断言 `check_success(144)` 由 false 变 true；
  step-start 重置路径同样覆盖。
- **有效性反证**：临时注释回授调用后该测试稳定失败（证明测试确实守着缺陷，非空断言），恢复后通过。
- `cargo clippy --all-targets -- -D warnings`：零告警 ✅
- `cargo test --no-fail-fast`：1708 通过；唯一失败 `git_sync::test_sync_repo_restores_deleted_file` 为预存量环境问题（本机 git 过老不支持 `git init -b`，main 同样失败）✅

## 4. 已知残留与后续

- **pi 执行器**的 `full_text`/`session_id` 缓存同属旧路径副作用，但其 `get_final_result` 有 logs 兜底（降级但正确），不构成功能缺陷，未在本 PR 处理。
- **双解析体系彻底收敛**（删除旧 `parse_output_line`/`parse_stderr_line` trait 方法）是 093 专项后续多 PR 工程，需逐执行器核对协议覆盖差异（盘点矩阵见缺陷分析文档）。
- `parse_for_direct_stream`（飞书私聊直连路径）未接回授：该路径无进程退出码判定场景，刻意不接，避免无谓耦合。

## 5. 安全反思

- 钩子只读事件不写外部状态（4 个实现仅操作内存 flag，parking_lot Mutex 无 await 持锁问题）；
- 无接口/权限/数据流变化；误判方向是把成功判失败，修复不产生反向误判（无 step_finish 的真实失败仍判失败，回归测试覆盖）。
