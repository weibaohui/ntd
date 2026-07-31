# 048-废弃 review_type 统一 gate_config-需求

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode (GLM-5.2) | 2026-07-31 | 初始版本 |

> 关联：046 门禁与评审统一（建立 gate_config）/ 047 门禁评分等待与评审追溯。本需求是 046 的收尾——清除与 gate 语义重复的 `review_type` 旧字段。

## 背景

046 门禁统一后，门禁类型收敛为 `ai_criteria_review` + `human_approval`。但 `loop_steps.review_type` 字段（ai/human）仍保留，与 gate 语义**一一重复**：

- `review_type: ai` ≡ gate `ai_criteria_review`
- `review_type: human` ≡ gate `human_approval`

两套配置并存容易迷惑——用户改 review_type 不生效（已被 gate 覆盖），还可能建出「review_type=ai 但 gate=human_approval」自相矛盾的配置。gate_config 才是 046 后的权威。

## 现状

`review_type` 唯一运行时用途：`loop_runner.rs:846` 判定人工审批步骤（决定 `initial_status = pending_approval`）。
- AI 评审触发（auto_review）**不依赖** review_type（看 `auto_review_enabled`，v74 已把 `review_type='ai'` 回填）。
- 其余引用为 DB 存取 / 展示。
- 人工审批判定与 auto_review 启用都能从 `gate_config` 完整推导（含 human_approval / 含 ai_criteria_review）。

## 需求

1. **废弃 `review_type` 字段**，`gate_config` 成为评审/门禁唯一权威。
2. `loop_runner` 人工审批步骤判定改由「gate_config 含 human_approval」（新增 `has_human_approval_gate` helper）。
3. `installer` 的 `todo.auto_review_enabled` 从 gate_config 推导（含 ai_criteria_review → 启用）。
4. 删除 `review_type`：后端 entity/db/models/LinkDefinition + 前端 types/编辑表单 + 工艺 YAML。

## 边界

- **DB schema 变更**：drop `loop_steps.review_type` 列（迁移 v81，幂等）。
- 历史设计文档（025/029/033/034/037/044 等）记录当时设计，**不回溯修改**；本需求为后续变更。
- 不影响 gate_config 多门禁能力（complex-refactor 的 ai_criteria_review + human_approval 组合保留）。
- `review_type=human` 的历史步骤，gate_config 应已有 human_approval（046 后审批已依赖 gate）；无则数据本身已不一致。

## 验收标准

- [ ] `loop_runner` 用 `has_human_approval_gate` 判定人工审批步骤
- [ ] `installer` `auto_review_enabled` 从 gate_config 推导
- [ ] `review_type` 字段全删（entity/db/models/LinkDefinition/前端 types/编辑表单）
- [ ] DB 迁移 v81 drop `loop_steps.review_type` 列（幂等）
- [ ] 工艺 YAML 删 `review_type`（gate-test + 4p12s）
- [ ] `cargo clippy --all-targets -- -D warnings` 零告警
- [ ] `cargo test` 全通过
- [ ] `tsc --noEmit` 零错误
