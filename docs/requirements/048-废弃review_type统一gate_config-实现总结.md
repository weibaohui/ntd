# 048-废弃 review_type 统一 gate_config-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode (GLM-5.2) | 2026-07-31 | 初始版本 |

> 关联：[需求](./048-废弃review_type统一gate_config-需求.md) / [设计](../design/048-废弃review_type统一gate_config-设计.md)

## 实现内容

### A. 后端逻辑（gate_config 为权威）
- **`gate_evaluator.rs`** 新增 `has_human_approval_gate(gate_config_json)`。
- **`loop_runner.rs:846`**：人工审批步骤改由 `gate_evaluator::has_human_approval_gate(&step.gate_config)` 判定，替代 `step.review_type == "human"`。
- **`installer.rs`**：`auto_review_enabled` 改从 `link.gates.iter().any(|g| g.gate_type == "ai_criteria_review")` 推导，替代 `link.review_type == "ai"`。

### B. 后端字段删除
`entity/loop_steps.rs`、`db/loop_.rs`（create_loop_step 参数/ActiveValue/clone/SQL SELECT/Model 构造 + test）、`models/loop_.rs`（LoopStepRawDto + From）、`models/mod.rs`（导入预览 DTO）、`process/mod.rs`（LinkDefinition.review_type + default_review_type）、`transition_resolver.rs` + `loop_runner.rs` tests 构造点。

### C. DB 迁移 v81
`db/migration/v81.rs`：`drop_column_if_exists("loop_steps", "review_type")`，注册到 `all_migrations`。

### D. 前端
`types/process.ts`、`types/loop.ts` 删 review_type；`LinkPropertyForm.tsx` 删「审核类型」下拉；`concepts.tsx` GATE_TYPES 删废弃门禁类型（046 编辑器清理同源）。

### E. 工艺 YAML
gate-test ×4 + 4p12s（user/bundled/ntd-resource 源）删 review_type。ntd-resource 已 push（`fca4dbd7`）。

## 验证

- `cargo clippy --all-targets -- -D warnings`：**零告警**。
- `cargo test`：**1486+ 全通过**（含 v81 fresh-db 迁移、installer auto_review 改 gates 推导、gate_evaluator has_human_approval_gate）。
- `npx tsc --noEmit`：**零错误**。

## 文件清单
- 后端：`gate_evaluator.rs`、`loop_runner.rs`、`installer.rs`、`process/mod.rs`、`transition_resolver.rs`、`db/loop_.rs`、`db/entity/loop_steps.rs`、`db/migration/v81.rs`(新)+`mod.rs`、`models/loop_.rs`、`models/mod.rs`、`tests/loop_step_execution_status_tests.rs`
- 前端：`types/loop.ts`、`types/process.ts`、`propertyForms/LinkPropertyForm.tsx`、`onboarding/concepts.tsx`

## 遗留
- v81 迁移在 dev/prod 库下次启动时自动 drop review_type 列（fresh-db 测试已覆盖迁移逻辑）。
- 历史设计文档（025/029/033/034/037/044 等）含 review_type，按「记录当时设计」惯例不回溯修改。
