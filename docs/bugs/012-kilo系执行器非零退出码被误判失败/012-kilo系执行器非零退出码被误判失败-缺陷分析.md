# 012-kilo系执行器非零退出码被误判失败-缺陷分析

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |

## 1. 根因

项目存在新旧两套输出解析体系（093 扫描确认的架构问题）：

- **旧**：`CodeExecutor::parse_output_line` → `ParsedLogEntry`（各 adapter 实现，含**副作用**）
- **新**：`EventExtractor` → `EventPipeline` → `ExecutionEvent`（`execution_events/impls/`）

运行时路径（`executor_service/log_capture.rs:344-392`）：**pipeline 优先，命中即 `continue`，旧路径被跳过**。

问题出在 4 个执行器（kilo/mimo/opencode/zhanlu）的 `parse_output_line` 携带关键副作用：

```rust
// adapters/kilo.rs:95-101（mimo/opencode/zhanlu 同构）
fn handle_step_finish(...) {
    *self.has_successful_finish.lock() = true;   // ← 副作用：成功标记
    ...
}

fn check_success(&self, exit_code: i32) -> bool {
    if exit_code == 0 { return true; }
    *self.has_successful_finish.lock()           // ← 退出判定时读取
}
```

而这 4 个执行器的新 Extractor **完整处理 step_finish**（发出 StepFinish/Tokens/Cost 事件）→
pipeline 恒命中 → `parse_output_line` 永不被调用 → `has_successful_finish` 永远保持 `false`
→ 非零退出时 `check_success` 返回 `false` → **误判失败**。

## 2. 为什么单测没拦住

各 adapter 的既有单测直接调用 `parse_output_line`（旧路径），flag 正常置位；
没有任何测试走「pipeline 处理 step_finish → check_success」的真实运行时组合路径。

## 3. 证据链

1. `log_capture.rs:347` `parse_and_broadcast(...)` 返回非空即 `continue`（跳过旧路径）；
2. `execution_events/impls/kilo.rs:58` step_finish 分支产出 `StepFinish`+`Tokens`+`Cost` 事件（非空）；
3. `kilo.rs:202-207` `check_success` 非零退出码依赖 flag；
4. flag 全链路唯一写入点是 `parse_output_line` → `handle_step_finish`；
5. `spawn_lifecycle.rs:641` 完成判定时 `executor.check_success(exit_code)`。

## 4. 修复方案选型

| 方案 | 判断 |
|------|------|
| A. pipeline 命中后也回退调用 `parse_output_line` | ❌ 每行二次 JSON 解析，且会让日志条目重复（旧路径也产出 entry） |
| B. `check_success` 改为完成时扫 logs 找 step_finish | ❌ 改 4 个执行器的判定语义，且 logs snapshot 在 `resolve_exit_outcome` 之后才提取（`spawn_lifecycle.rs:615-624`），顺序要倒过来，影响面大 |
| C. **trait 加 `on_pipeline_event` 观察钩子**（采用） | ✅ pipeline 事件本来就已解析好，回授给执行器同步状态，零重复解析；默认空实现，其余 9 个执行器无感 |

### 采用方案 C 的改动点

1. `CodeExecutor` trait 新增默认空方法 `on_pipeline_event(&self, _event: &ExecutionEvent)`；
2. kilo/mimo/opencode/zhanlu 实现该钩子：`StepStart → flag=false`，`StepFinish → flag=true`（与旧 handler 语义逐字对齐）；
3. `log_capture::parse_and_broadcast` 与 `try_parse_stderr_with_pipeline` 增加 `executor` 参数，事件广播前回授；`message_debounce.rs:1038` 调用点同步适配。

## 5. 已知残留（不在本缺陷范围，记录在案）

pi 执行器的 `full_text`/`session_id` 缓存也是旧路径副作用，pipeline 命中后同样跳过——但
`get_final_result` 有 logs 兜底分支（`pi.rs:377-386` 注释明示），属「降级但正确」，不构成功能缺陷。
双解析体系的彻底收敛（删旧 trait 方法）是 093 专项后续多 PR 工程。
