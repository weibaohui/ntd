# Loop 执行引擎重构：基于边的有向图模型 + 结论黑板

> 日期：2026-06-20
> 分支：`feat/loop-flow-control`

---

## 1. 动机

当前 Loop 的执行是**线性顺序 + 中断式评分闸门**：

```
Step 1 ──执行──▶ 评分闸门 ──通过──▶ Step 2 ──执行──▶ ...
                         └──不通过──▶ break（整个 Loop 终止）
```

四个核心问题：

| 问题 | 表现 |
|---|---|
| ❌ 评分不通过直接终止 | 阈值不达标直接 `break`，没有分支选择 |
| ❌ 没有条件跳转 | 不能"Review 不通过 → 回去改代码" |
| ❌ 无结论传递 | 上一环节的输出无法传递给下一环节做上下文 |
| ❌ 无执行全景回溯 | 无法一目了然地看到一次 Loop 执行中每个环节每次执行的结论 |

---

## 2. 设计目标

| 需求 | 说明 |
|---|---|
| **条件分支** | 评分通过/不通过走不同路径（goto / skip / end / break） |
| **全局限制** | 防止无限循环：最大执行步数，预留 Token 计数器 |
| **结论黑板** | 一次 Loop 执行中，每个环节每次执行的结论按时间顺序记录 |
| **跨步传参** | 上一环节的结论可作为下一环节的 Prompt 模板变量 |
| **前端流程图** | 用有向图可视化替代线性卡片列表，支持执行轨迹回放 |

---

## 3. DB 改动

### 3.1 `loop_steps` 表 — 控制流字段

```rust
// ── 已有字段（保留）──
pub min_rating: Option<i32>,          // NULL = 不启用门禁
pub unrated_policy: String,           // 保留兼容，新逻辑不再读取

// ── 新增字段 ──
/// 成功时策略: "next" | "goto" | "end"
pub on_success: String,               // 默认 "next"
pub success_goto_step_id: Option<i64>,

/// 评分不通过时策略: "break" | "skip" | "goto" | "end"
pub on_rating_fail: String,           // 默认 "break"
pub fail_goto_step_id: Option<i64>,
```

**评分门禁行为**：

```
if min_rating == NULL → 不启用门禁，直接走 on_success
if 有评分 → rating >= min_rating 通过，< min_rating 不通过
if 无评分 → 视为 rating = 0，不通过（除非 min_rating ≤ 0）
```

**策略含义**：

| 策略 | 含义 |
|---|---|
| `next` | 按 `order_index` 顺序执行下一个环节 |
| `goto` | 跳到 `_goto_step_id` 指定的环节 |
| `end` | 终止 Loop |
| `break` | 终止 Loop（和 end 的区别：break 是因为失败） |
| `skip` | 跳到下一顺序环节（仅 `on_rating_fail` 可用） |

### 3.2 `loops` 表 — 全局限制

```rust
/// JSON: {"max_step_executions": 20, "max_total_tokens": null}
pub limits_config: String,            // 默认 "{}"
```

### 3.3 `loop_executions` 表 — 执行计数器

```rust
/// 累计执行过的 step 次数（含循环重走）
pub total_executed_steps: i32,       // 默认 0
```

引擎每次执行 step 前检查：`total_executed_steps ≥ limits.max_step_executions` → 终止 Loop，状态 `capped`。

### 3.4 `loop_step_executions` 表 — 结论黑板

`loop_step_executions` 本身就是结论黑板的载体，每行对应一次 step 拜访。

```rust
/// 本次 loop_execution 中的全局执行序号（1, 2, 3...）
pub sequence_index: i32,              // 默认 0

/// 本次步执行的核心结论摘要
pub conclusion: Option<String>,       // 从 execution_record.result 提取
```

**结论提取策略**：

```
1. 优先提取 output 中 ## 结论 或 Conclusion: 标记后的内容
2. 无标记时取 result 前 300 字符（截断加 ...）
3. result 为空时取 error_message 前 200 字符
```

### 3.5 黑板数据示例

```
Loop Execution #5
  limits_config = {"max_step_executions": 20}
  total_executed_steps = 5
  status = success

  loop_step_executions (按 sequence_index 排序):
  ┌────┬─────────┬──────┬──────────────────────────────────┐
  │ #  │ 环节     │ 评分 │ 结论                             │
  ├────┼─────────┼──────┼──────────────────────────────────┤
  │ 1  │ 编写代码 │  —   │ 完成了用户登录功能的 CRUD         │
  │ 2  │ Review  │  55  │ 代码缺少输入校验和错误处理         │
  │ 3  │ 编写代码 │  —   │ 已添加输入校验和 try-catch       │
  │ 4  │ Review  │  85  │ 代码质量符合要求                  │
  │ 5  │ 部署环境 │  —   │ 已部署到测试环境 v2.1.0          │
  └────┴─────────┴──────┴──────────────────────────────────┘
```

---

## 4. 环节间传参：模板变量

引擎启动每个 step 之前，将黑板内容以模板变量注入 Prompt。

| 模板变量 | 说明 | 示例值 |
|---|---|---|
| `{blackboard}` | 完整的黑板内容（所有历史结论） | 见下方 |
| `{last_conclusion}` | 上一环节的结论摘要 | "代码缺少输入校验" |
| `{last_step_name}` | 上一环节的名称 | "Review 代码" |
| `{last_output}` | 上一环节的完整输出（截断） | ... |
| `{loop_execution_id}` | 本次执行 ID | 5 |
| `{loop_name}` | Loop 名称 | "代码开发流程" |

**{blackboard} 渲染示例**：

```
--- 执行记录 #1: 编写代码 (成功) ---
结论: 完成了用户登录功能的 CRUD 代码

--- 执行记录 #2: Review 代码 (失败 · 评分 55) ---
结论: 代码缺少输入校验和错误处理

--- 执行记录 #3: 编写代码 (成功) ---
结论: 已添加输入校验和 try-catch 异常处理
```

---

## 5. 执行引擎流程

```rust
async fn run_inner(self: Arc<Self>, loop_id, loop_execution_id, trigger_type) {
    // 1. 加载配置
    steps = load_enabled_steps(loop_id)
    limits = parse_limits(loop.limits_config)

    current_step_idx = 0
    sequence_counter = 0          // 黑板全局序号
    attempt_counter = {}          // step_id → 第几次拜访
    total_executed_steps = 0
    last_blackboard_entry = None

    // 2. 主循环
    loop {
        step = resolve_by_index(steps, current_step_idx)
        if step is None → break

        // 2a. 全局限制检查
        if total_executed_steps >= limits.max_step_executions
            → end_loop("capped"); break

        // 2b. 构建 Prompt（注入黑板变量）
        prompt = step.prompt
            .replace("{blackboard}", build_blackboard_text())
            .replace("{last_conclusion}", ...)
            .replace("{last_step_name}", ...)
            .replace("{last_output}", ...)

        // 2c. 创建 step execution（含 sequence_index）
        sequence_counter += 1
        step_exec = db.create_loop_step_execution(
            loop_execution_id, step.id, sequence_counter, ...)

        // 2d. 执行并等待完成
        record_id = start_step_todo(prompt, step_exec.id)
        wait_for_finish(record_id)

        // 2e. 评分门禁
        passed = if gate_enabled { apply_rating_gate(...) } else { true }

        // 2f. 提取结论，写入黑板
        conclusion = extract_conclusion(record_id)
        db.update_step_execution_conclusion(step_exec.id, conclusion)

        // 2g. 更新计数器
        total_executed_steps += 1
        db.increment_counters(loop_execution_id, passed)

        // 2h. 确定下一步
        current_step_idx = resolve_next(
            step, if passed { on_success } else { on_rating_fail })
    }

    // 3. 结束 Loop
    db.finish_loop_execution(loop_execution_id, compute_status(...))
}

fn resolve_next(step, policy) -> Option<usize> {
    match policy {
        "next" => current_idx + 1,           // 顺序下一个
        "goto" => find_idx(step.goto_id),    // 目标 step
        "end"  | "break" => None,            // 终止
        "skip" => current_idx + 1,           // 跳到下一个
    }
}
```

---

## 6. 前端可视化

### 6.1 流程图编辑器（重构 `LoopStudioStepsPanel`）

当前：`[Step1] → [Step2] → [Step3]`

升级：dagre 自动布局 + SVG 绘制有向边

**边的视觉编码**：

| 边类型 | 颜色 | 线型 | 标注 |
|---|---|---|---|
| `on_success=next`（默认顺序） | 灰色 | 实线 → | 无标注 |
| `on_success=goto`（跨步跳） | 绿色 | 实线 → | "成功→Step N" |
| `on_rating_fail=skip` | 橙色 | 虚线 -→ | "失败→继续" |
| `on_rating_fail=goto`（回路） | 红色 | 实线 → | "失败→Step N" |
| `on_success=end` / `on_rating_fail=end` | 灰色 | 实线 → ⏹ | "结束" |

**迷你总览图**：右上角悬浮小画布，显示整个图的缩略视图及当前可视区域。

### 6.2 步骤编辑 Modal

新增「控制流配置」区块，替换旧 `unrated_policy`。

### 6.3 结论黑板展示（执行详情面板）

在 `LoopStudioExecutionsPanel` 的执行详情 Drawer 中，新增黑板时间线视图：

```
  ┌────┬─────────┬──────┬──────────────────────────────────┐
  │ #  │ 环节     │ 评分 │ 结论                             │
  ├────┼─────────┼──────┼──────────────────────────────────┤
  │ 1  │ 编写代码 │  —   │ 完成了用户登录功能的 CRUD         │
  │    │ ✅ 成功  │      │                                  │
  ├────┼─────────┼──────┼──────────────────────────────────┤
  │ 2  │ Review  │  55  │ 代码缺少输入校验和错误处理         │
  │    │ ❌ 失败  │      │                                  │
  │    │ ── goto → Step 1 (编写代码) ──                    │
  ├────┼─────────┼──────┼──────────────────────────────────┤
  │ 3  │ 编写代码 │  —   │ 已添加输入校验和 try-catch       │
  │    │ ✅ 成功  │      │                                  │
  └────┴─────────┴──────┴──────────────────────────────────┘
```

---

## 7. API 改动

| 接口 | 改动 |
|---|---|
| `GET /v2/loops/:id` | `steps[]` 中新增 `on_success`, `success_goto_step_id`, `on_rating_fail`, `fail_goto_step_id`；新增 `limits_config` |
| `POST /v2/loops/:id/steps` | 请求体新增上述控制流字段 |
| `PUT /v2/loops/:id/steps/:step_id` | 同上 |
| `GET /v2/loop-executions/:id` | `loop_step_executions` 按 `sequence_index` 排序返回 |
| `POST /v2/loops/:id` (update) | 支持更新 `limits_config` |
| `LoopStepExecutionDto` | 新增 `sequence_index`, `conclusion`, `attempt_number` |

---

## 8. 迁移策略

1. 新建 migration，所有新字段设默认值：
   - `on_success = "next"`, `on_rating_fail = "break"`
   - `limits_config = "{}"`
   - `total_executed_steps = 0`, `sequence_index = 0`
   - `conclusion = NULL`
2. 旧数据升级后行为完全不变（`on_success=next`, `on_rating_fail=break` 即原线性顺序 + 失败终止）
3. `unrated_policy` 保留不动，新代码不再读取

---

## 9. 改动范围总表

| 层 | 文件 / 模块 | 改动 |
|---|---|---|
| **DB schema** | `loop_steps` | +4 列 |
| | `loops` | +1 列 (`limits_config`) |
| | `loop_executions` | +1 列 (`total_executed_steps`) |
| | `loop_step_executions` | +2 列 (`sequence_index`, `conclusion`) |
| **后端 Entity** | 对应 4 个实体文件 | 加字段定义 |
| **后端 DTO** | `models/loop_.rs` | `LoopStepDto`, CRUD Request 加字段 |
| **后端 DB** | `db/loop_.rs`, `db/loop_step_*.rs` | 新增 create/update/query 方法 |
| **后端 Service** | `services/loop_runner.rs` | 重写为 DAG 引擎（resolve_next、黑板、传参、限制） |
| **后端 Handler** | `handlers/loop_.rs` | CRUD 支持新字段 |
| **前端类型** | `types/loop.ts` | 类型扩展 |
| **前端组件** | `LoopStudioStepsPanel.tsx` | 重构为 dagre + SVG 流程图 |
| | `LoopStudioDetailPanel.tsx` | 新增全局限制配置 |
| | `LoopStudioExecutionsPanel.tsx` | 新增黑板时间线 + 轨迹回放 |
| | 新建 `loop-flow/` 组件目录 | 节点、边、迷你总览图 |
| **npm** | 新增 `dagre` | DAG 自动布局引擎 |

---

## 10. 场景映射

### 场景一：笑话 + 评分门禁（低分不中断）

```
Step 1 [讲个笑话]
  min_rating = 60
  on_success  = next       ← 够好笑 → 下一个
  on_rating_fail = skip    ← 不好笑 → 也继续下一个
Step 2 [下一个环节]
```

### 场景二：Code → Review → 循环直到通过 → Deploy

```
Step 1 [写代码]
  min_rating = NULL
  on_success  = next       ← 写完后去 Review

Step 2 [Review 代码]
  min_rating = 80          ← 验收标准
  on_success  = goto       ← 通过 → 去 Deploy
  success_goto_step_id = 3
  on_rating_fail = goto    ← 不通过 → 回去改
  fail_goto_step_id = 1

Step 3 [部署 / 合并 PR]
  on_success = end         ← 完成后结束 Loop
```

```
Step1 → Step2 ──不通过──┐
          │             │
         通过           │
          ▼             │
        Step3          Step1 → Step2 ...
```

全局限制 `max_step_executions = 20` 确保不会无限循环。
