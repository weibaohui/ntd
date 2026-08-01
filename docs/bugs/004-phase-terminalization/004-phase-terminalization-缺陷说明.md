# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：BUG-004 缺陷说明 |

# 1. 缺陷标题

loop_phase_executions 永驻 running，phase 终态不落地

# 2. 缺陷描述

loop 成功结束后，其 `loop_phase_executions` 行 status 仍为 `running`、`finished_at` 为 NULL。人工挂起步骤（human_approval）执行时，`update_phase_execution` 因 `gates_passed=false` 把 phase 标为 "failed"，而非保持 pending。

# 3. 影响范围

含 human_approval 的环路，phase 终态永远不对；无 human_approval 的环路 phase 也不被终态化（BUG-008 级联删除前也表现为 running）。P2（审计链不一致）。

# 4. 修复方案

1. `update_phase_execution` 增加 `human_pending` 参数：挂起时不更新 phase 状态
2. 新增 `finalize_phase_executions` DB 方法：loop 终态时把剩余 running phase 标为 success/failed
3. `loop_runner.run_inner` 在 `finish_loop_execution` 后调用 `finalize_phase_executions`
