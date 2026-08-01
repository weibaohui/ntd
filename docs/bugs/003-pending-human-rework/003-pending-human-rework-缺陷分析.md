# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：根因分析 |

# 1. 根因分析

## 代码链路

```rust
let gates_passed = all_passed && !has_pending_human; // 挂起时 = false
let next_idx = resolve_next(step, gates_passed, ...); // false → on_rating_fail → 上游
let rework = evaluate_rework(..., current_idx, next_idx); // upstream ≤ current → rework
```

## 设计问题

`has_pending_human` 被当作门禁失败处理，但 pending 不是失败——是等待。流转应在审批后由 `resume_loop_execution` 决定，而非在挂起时预解析。

## 修复策略

把流转/返工决策提取为纯函数 `decide_transition_and_rework`，human_pending 时直接返回 `(None, 原值, None)`。
