# 054 - 环节「期望产物 + spec 模板」注入执行器提示词

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-31 | 初始版本 |
| Claude | 2026-07-31 | §7.4 降级策略：JSON 解析失败改为「按字段降级」（与实现对齐） |

---

## 1. 背景与目标

### 1.1 要解决的问题

工艺环节（step）上可以配置两类约束：

1. **期望产物**（`expected_artifacts`）：本环节执行后必须产出的产物清单（文件/文本/URL/JSON 等），目前**只在执行完成后**被产物捕获与门禁消费，执行器跑的时候根本看不到。
2. **环节 spec 模板**（`step_template`）：一组指向外部 spec markdown 文件的引用（`{name, path}`），类型定义上的注释明确写着「执行时注入 AI 上下文供其重点阅读」，但**从未被实现**——既没落库，也没注入。

结果是：执行器拿到 prompt 时，对「这步该交付什么」「要遵循哪份 spec」一无所知，只能盲跑，事后再靠门禁判定是否产出。这导致：

- 执行器容易漏产物或偏离约定，事后门禁才报错，返工成本高；
- 配了 spec 模板却形同虚设，配置与执行脱节。

### 1.2 目标

执行器真正执行时，把环节的**期望产物**和**使用的环节 spec 模板**注入到执行器要执行的提示词中，让执行器在动手前就知道「要交付什么、遵循哪份约定」。

### 1.3 非目标（YAGNI，本次不做）

- 不注入环节级 `skills`（`skill_names` 现用于评审门禁上下文，执行 prompt 不动，避免扩大口径）。
- 不注入阶段级 `loop_phases.spec`（已落库但同样未进 prompt，留作后续可选扩展，文档末尾给出最小接入点）。
- 前端无改动：`step_template` 仍由工艺 YAML 编辑器维护，本次只打通后端「落库 → 运行时注入」链路。

---

## 2. 现状分析

### 2.1 执行器提示词是「两层串联」组装

| 层 | 位置 | 作用范围 | 做了什么 |
|----|------|---------|---------|
| 第一层 | `services/loop_runner.rs:821` → `build_enhanced_prompt_with_requirement`（定义 `:1669-1708`） | **仅工艺 loop 路径** | 占位符替换：`{{requirement}}/{{blackboard}}/{{last_output}}/{{last_conclusion}}/{{last_step_name}}/{{message}}/{{loop_execution_id}}/{{loop_name}}` |
| 第二层 | `executor_service/stages.rs:46-87` `prepare_execution_state` | **所有执行入口的统一收口**（loop / 手动单 todo / cron / 飞书 / hook） | 依次注入：`inject_workspace_prompt`（`pre_spawn.rs:550`）→ `inject_expert_context`（`:640`）→ `inject_workspace_background`（`:592`）→ `select_executor_and_build_command` |

第一层产出的 `enhanced_prompt` 作为 `message` 传入 `RunTodoExecutionRequest`，第二层在其上继续包装。最终 `message` 是被执行器 CLI 消费的位置参数（如 Claude Code 走 `claude --dangerously-skip-permissions ... -p <message>`）。

### 2.2 两项要注入的数据现状

| 项 | 落库 | 进 prompt? | 现状消费点 |
|----|------|-----------|-----------|
| 期望产物 `expected_artifacts` | ✅ `loop_steps.expected_artifacts`（JSON，`ExpectedArtifact{name,type,path,locator}`，`mod.rs:177-185`），installer 写入（`installer.rs:269-301`） | ❌ | **仅执行后**：`phase_driver.rs:76 parse_expected_artifacts` → `artifact_capture.rs:62 capture_step_artifacts` 捕获产物 / 门禁 |
| 环节 spec 模板 `step_template` | ❌ **完全未落库**（`StepTemplateRef{name,path}` 定义 `mod.rs:116-121`，`LinkDefinition.step_template` `:130`，注释「执行时注入 AI 上下文供其重点阅读」） | ❌ | 无（installer 的 `create_loop_step_for_link` 不写、无 DB 列；052 删了老 `process_step_templates` 表后此字段降级为内联 spec 引用，再无人实现） |

**核心判断**：当前进入执行器 prompt 的环节级字段**只有 `link.prompt` 一个**。`inject_expert_context` 注入的是「专家 agent MD + 专家技能」，不是环节字段。本次改造是在第二层注入链上**新建一条目前不存在的环节级注入路径**。

### 2.3 可复用的现成基础设施

- **按 todo_id 反查 step**：`db/loop_.rs:552 find_loop_step_by_todo_id(todo_id) -> Option<loop_steps::Model>`（已存在，返回完整 Model，含 `expected_artifacts` 与未来的 `step_template_refs`）。
- **`bundled://` spec 文件读取**：`installer.rs:549 load_bundled_markdown(uri)` 把 `bundled://processes/...` 解析为 `~/.ntd/bundled/...` 读正文（阶段级 `spec_ref` 在安装时就是这么读的，`installer.rs:529-544`）。
- **注入函数范式**：`inject_workspace_prompt` / `inject_workspace_background` / `inject_expert_context` 均为「读数据 → 拼段落前置 → 失败静默回退原 message」的纯增强函数，新注入照此模式。

---

## 3. 方案总览（已确认）

| 决策点 | 结论 | 理由 |
|--------|------|------|
| 注入位置 | **第二层 `stages.rs`**（全入口） | 覆盖 loop / 手动 / cron / 飞书 / hook 所有执行入口，行为一致；与已有的环节级注入 `inject_expert_context` 同位置同模式；step 经 `find_loop_step_by_todo_id` 反查，**无需给 `RunTodoExecutionRequest` 加字段** |
| spec 模板承载 | **给 `loop_steps` 加列落库** | 与 `expected_artifacts` 完全同构（同 JSON 数组、同 installer 写入路径），link 级细粒度，持久化、与磁盘 YAML 解耦 |
| 注入形式 | **无条件前置结构化段落**（最内层） | 仿现有 `inject_*`，开箱即用，不依赖 prompt 作者写占位符；放在原 message 最内层，紧贴核心任务 |
| spec 正文 | **内联 + 大小上限，超限回退为路径** | 保证 spec 进上下文，同时防爆 prompt |

---

## 4. 数据模型变更

### 4.1 新增列：`loop_steps.step_template_refs`

- 类型：`TEXT NOT NULL DEFAULT '[]'，存 `Vec<StepTemplateRef>` 的 JSON 串（`[{name,path}, ...]`）。
- 与 `expected_artifacts` / `skill_names` / `gate_config` 完全同构。

### 4.2 迁移 v83（新建 `db/migration/v83.rs`，仿 `v71.rs:250-256` 的 `add_column_if_missing`）

```text
ALTER TABLE loop_steps ADD COLUMN step_template_refs TEXT NOT NULL DEFAULT '[]'
```

- 在 `db/migration/mod.rs` 的 `all_migrations()` 列表末尾（V82 之后）注册 `Box::new(v83::V83AddLoopStepTemplateRefs)`。
- 幂等：`add_column_if_missing` 已封装「列存在则跳过」。
- 测试：仿 `v82.rs` 写「fresh 库有列」「列默认值为 `[]`」「重复应用幂等」三条。

### 4.3 entity（`db/entity/loop_steps.rs`）

在 `expected_artifacts` 旁新增字段：

```rust
/// 环节 spec 模板引用（JSON 数组，StepTemplateRef{name,path}），执行时注入 prompt 供重点阅读。
#[sea_orm(default_value = "[]")]
pub step_template_refs: String,
```

### 4.4 db 层必改点：raw SQL 手动映射（`db/loop_.rs:1249-1283`）

该处用原生 SQL `SELECT ... INTO loop_steps::Model` 手工构建 Model。给 entity 加字段后，此处**会编译中断**，必须同步：

- SELECT 列表（`:1249-1250`）追加 `s.step_template_refs`；
- 行映射（`:1280` 附近）追加 `step_template_refs: row.try_get_by::<String, _>("step_template_refs")?`。

### 4.5 其他 ActiveModel 写入点（4 处，`grep loop_steps::ActiveModel`）

- `installer.rs:274`（`create_loop_step_for_link`）：**必须显式写入** `step_template_refs`（见 §5）。
- `db/loop_.rs:405 / :750`（`c.into()` 转换）、`:724`：用 `..Default::default()` 或 `.into()`，未显式设值的列走 DB 默认 `[]`，无需改动（编译期由 `Default` 兜底，运行期由列默认值兜底）。实现时逐一确认。

---

## 5. 安装 / 升级写入（`installer.rs`）

`create_loop_step_for_link`（`:260-311`）在序列化 `expected_artifacts` / `gate_config` / `skill_names` 旁，新增一行写入：

```rust
// 环节 spec 模板引用：执行时注入 AI 上下文供重点阅读（需求 054）。
let step_template_refs = serde_json::to_string(&link.step_template.unwrap_or_default())
    .unwrap_or_else(|_| "[]".to_string());
```

并在 `ActiveModel { ... }` 中 `step_template_refs: ActiveValue::Set(step_template_refs)`。

`upgrade_process_template_loop`（`:574`）重建步骤时复用 `create_loop_step_for_link`，无需额外改动——新列自动随重装写入。

---

## 6. spec 文件运行时读取（提取到 `source.rs`）

把 `installer.rs:549 load_bundled_markdown` 提取到 `services/process/source.rs` 为 `pub fn`（命名如 `read_bundled_markdown`），installer 改为调用它，避免重复。`executor_service` 的 `inject_step_context` 通过该 pub 函数在运行时按 `StepTemplateRef.path` 读 spec 正文。

> 说明：保持与安装期 `resolve_phase_spec_refs` 完全相同的解析口径（`bundled://...` → `~/.ntd/bundled/...`），保证「安装时读到什么，运行时就注入什么」。

---

## 7. 注入实现

### 7.1 新函数 `inject_step_context`（`executor_service/pre_spawn.rs`）

```rust
/// 注入环节级「期望产物 + spec 模板」到 message 最内层（需求 054）。
///
/// 按 todo_id 反查 loop_step；解析 expected_artifacts 与 step_template_refs；
/// 拼成 `# 环节交付要求` 段落前置到 message。任一步失败/无 step/配置全空 → 静默回退原 message。
pub(crate) async fn inject_step_context(db: &Database, todo_id: i64, message: &str) -> String
```

子函数（每个 ≤30 行，均有单测）：

- `build_step_context_section(step) -> Option<String>`：统筹，产物/spec 都空返回 None。
- `build_expected_artifacts_section(&[ExpectedArtifact]) -> String`：渲染产物清单。
- `build_step_spec_section(&[StepTemplateRef]) -> String`：渲染 spec 列表，逐条读正文。
- `format_single_spec(ref) -> String`：单条 spec——正文 ≤ 上限内联，否则只给 `路径: <path>`。
- 顶层组装：`format!("# 环节交付要求\n...\n---\n{}", message)`。

### 7.2 接入注入链（`stages.rs:60-68`）

在 `substitute_message_placeholders` 之后、`inject_workspace_prompt` **之前**接入（= 最内层，紧贴核心任务）：

```rust
// 4.4) 注入环节级「期望产物 + spec 模板」（需求 054）。
//      放在所有注入最内层：原任务在最尾，交付要求紧贴其上。
//      无 step（独立 todo）/ 配置全空 / 读取失败时静默回退，不阻断执行。
let step_message = inject_step_context(&request.db, request.todo_id, &substituted.message).await;
let workspace_message = inject_workspace_prompt(&request.db, request.workspace_id, &step_message).await;
```

最终 prompt 层级（外→内）：

```
# 运行背景（工作空间）
# 专家角色定义 / 可用技能
# 任务
  工作空间共识 system_prompt
  # 环节交付要求          ← 本次新增
    ## 期望产物
    ## 参考 Spec 约定
  <核心 prompt>          ← 最内层
```

### 7.3 注入文本格式样例

```text
# 环节交付要求

## 期望产物
必须产出下列产物，缺一项视为未完成：
- PRD 文档（类型: file）路径: docs/design/xxx.md
- 接口契约（类型: json）定位: $.paths

## 参考 Spec 约定
请重点遵循以下 spec：
### 需求环节规范
<spec 正文；超上限则改为：详见 bundled://processes/conventions/requirement-phase-spec.md>

---
<原 message>
```

### 7.4 降级策略（与现有 `inject_*` 一致）

- 反查不到 step（独立 todo）→ 原样返回。
- 某字段 JSON 解析失败 → 该字段按空处理（跳过），另一字段仍注入；warn，不阻断执行（按字段降级，单个脏字段不拖垮整段注入）。
- spec 文件读取失败 → 该条降级为「路径引用」，不影响其它条目与产物段落。
- 产物与 spec 全空 → 原样返回（不产生空标题）。
- **绝不阻断执行**，绝不写回 DB（`todo.prompt` 只读原则，`loop_runner.rs:806-808`）。

---

## 8. 测试方案

- **迁移 v83**：fresh 库列存在 / 默认值 `[]` / 幂等。
- **`build_expected_artifacts_section`**：多产物渲染、各 type、`path`/`locator` 缺省。
- **`build_step_spec_section` / `format_single_spec`**：正文 ≤ 上限内联、超上限回退路径、读取失败回退路径、多条拼接。
- **`build_step_context_section`**：产物+spec 全空返回 None；仅产物 / 仅 spec / 两者都有。
- **`inject_step_context`**：无 step 回退、解析失败回退、正常注入段落顺序与分隔线正确。
- **回归**：`cd backend && cargo test`；`cargo clippy --all-targets -- -D warnings` 零告警。

---

## 9. 影响范围与兼容性

- **数据库**：仅加列（`DEFAULT '[]'`），存量行自动得到空数组，向后兼容。
- **后端**：`db/migration`、`db/entity/loop_steps.rs`、`db/loop_.rs`（迁移+entity+映射）、`services/process/installer.rs`、`services/process/source.rs`、`executor_service/pre_spawn.rs`、`executor_service/stages.rs`。
- **前端**：无改动。
- **执行行为**：对配了期望产物/spec 的环节，执行器 prompt 多一段前置约束；未配的环节行为完全不变（静默回退）。
- **安全反思**：spec 正文来自本地 `~/.ntd/bundled/` 受信任文件，非外部输入，注入到本地执行器 prompt 无越权/注入面；产物清单来自库内配置。无敏感信息打印（与 `inject_workspace_prompt` 一致，失败仅 warn 不带正文）。

---

## 10. 后续可选扩展（非本次范围）

- 阶段级 `loop_phases.spec` 同样可在 `inject_step_context` 内按 `step.phase_id` 读取后并入「参考 Spec 约定」段落，一处接入即可让阶段 spec 也生效。
- 若需环节级 `skills` 进执行 prompt，可同模式扩展（当前刻意不动）。
