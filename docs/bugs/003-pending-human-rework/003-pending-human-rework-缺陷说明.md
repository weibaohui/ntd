# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：BUG-003 缺陷说明 |

# 1. 缺陷标题

人工审批挂起环节被误记返工并写入「工艺终止」错误信息

# 2. 缺陷描述

`human_approval` 门禁挂起时，`phase_driver::execute_step` 把 `gates_passed` 设为 false，导致 `transition_resolver::resolve_next` 走 `on_rating_fail`（=上游跳转），`rework_tracker::evaluate_rework` 判定为返工，最终写入 `rework_count=1` 和假错误信息 `error_message='返工次数 1 已达到上限 1，工艺终止'`。虽然最终 `final_status` 会被覆写为 `pending_approval`，但返工计数与错误信息已落库。

# 3. 影响范围

含 human_approval 门禁 + `on_gate_fail` 指向上游的环节，挂起即被计返工。P2（数据错误，但不阻塞执行）。

# 4. 修复方案

`human_pending` 时短路流转解析与返工统计：返回 `(None, 原值, None)`，不推进、不计返工、无错误信息。
