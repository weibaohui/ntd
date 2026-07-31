# 048-废弃 review_type 统一 gate_config-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode (GLM-5.2) | 2026-07-31 | 初始版本 |

> 关联需求：`docs/requirements/048-废弃review_type统一gate_config-需求.md`

## 1. 背景

046 建立了 gate_config（门禁统一），但 review_type 旧字段保留，与 gate 语义重复。详见需求文档。

## 2. 决策

**gate_config 作为唯一权威，废弃 review_type**（不保留兼容列）。

| 判定 | 现状（review_type） | 048（gate_config） |
|---|---|---|
| 人工审批步骤（loop_runner initial_status） | `review_type == "human"` | gate_config 含 `human_approval` |
| AI 评审启用（installer auto_review_enabled） | `link.review_type == "ai"` | gate_config 含 `ai_criteria_review` |

保留 review_type 会持续产出「两套配置打架」的 bug（用户改 review_type 不生效），故直接删除而非降级为兼容字段。

## 3. 详细设计

### 3.1 后端逻辑
- **`gate_evaluator.rs`** 新增 `pub fn has_human_approval_gate(gate_config_json) -> bool`（与 `rating_wait::has_ai_criteria_review_gate` 同模式，serde 解析后判 gate_type）。
- **`loop_runner.rs:846`**：`is_human_approval_step = gate_evaluator::has_human_approval_gate(&step.gate_config)`，替代 `step.review_type == "human"`。
- **`installer.rs`**：`auto_review_enabled = link.gates.iter().any(|g| g.gate_type == "ai_criteria_review")`，替代 `link.review_type == "ai"`。

### 3.2 后端字段删除
`db/entity/loop_steps.rs`、`db/loop_.rs`（create_loop_step 参数 + ActiveValue + clone + SQL SELECT + Model 构造）、`models/loop_.rs`（LoopStepRawDto + From）、`models/mod.rs`（导入预览 DTO）、`services/process/mod.rs`（`LinkDefinition.review_type` + `default_review_type`）。

### 3.3 DB 迁移 v81
`db/migration/v81.rs`：`drop_column_if_exists(db, "loop_steps", "review_type")`（幂等，与 v77 同模式）。注册到 `all_migrations`。

### 3.4 前端
`types/process.ts`（LinkDefinition）、`types/loop.ts`（LoopStepDto）删字段；`LinkPropertyForm.tsx` 删「审核类型」下拉；`concepts.tsx` onboarding `GATE_TYPES` 删废弃门禁类型（046 编辑器清理同源）。

### 3.5 工艺 YAML
删 `review_type` 行（gate-test ×4 + 4p12s）。ntd-resource 源仓库同步（commit `fca4dbd7`）。

## 4. 不做的

- **不重排 persist/finalize 顺序**（047 已评估，破坏 executor 终态信号）。
- **不回溯改历史文档**（025/029/033/034/037/044 记录当时设计）。
- **不删 gate_config 多门禁能力**（complex-refactor 的 ai+human 保留）。

## 5. 测试

- `gate_evaluator::has_human_approval_gate`（含/空/非法 JSON）。
- `installer::test_install_link_review_type_enables_auto_review` 改验证：write-prd（ai_criteria_review）→ auto_review=true；confirm-prd（human_approval）→ auto_review=false。
- `db::migration` fresh-db 全迁移（含 v81）注册测试。

## 6. 风险

- **DB drop 列**：v81 幂等（drop_column_if_exists），旧库无列跳过。部署时启动一次即迁移。
- **历史 review_type=human 无 human_approval gate**：数据本身已不一致（046 后审批已依赖 gate），属于历史脏数据，不在本需求兜底。
