# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-31 | 初始版本（环节期望产物 + spec 模板注入执行器提示词） |
| Claude | 2026-07-31 | 细化验收标准：JSON 解析出错改为「按字段降级」（与实现对齐） |

---

# 需求：环节「期望产物 + spec 模板」注入执行器提示词（054）

## 1. 背景（Why）

工艺环节（step）可配置两类约束，但执行器执行时都看不到：

- **期望产物**（`expected_artifacts`）：本环节必须产出的产物清单。当前**只在执行完成后**被产物捕获与门禁消费（`phase_driver` / `artifact_capture`），执行器跑的时候对「该交付什么」一无所知。
- **环节 spec 模板**（`step_template`）：一组指向外部 spec markdown 文件的引用。类型注释明确写着「执行时注入 AI 上下文供其重点阅读」，但**从未实现**——既未落库，也未注入。

后果：执行器盲跑，容易漏产物 / 偏离约定，事后门禁才报错，返工成本高；配了 spec 模板却形同虚设。

目标：执行器真正执行时，把环节的期望产物与 spec 模板注入其提示词，让执行器动手前就明确「交付什么、遵循哪份约定」。

## 2. 目标（What，可验证）

- [ ] G1 环节 spec 模板（`step_template`）落库到 `loop_steps`，安装工艺时随环节持久化。
- [ ] G2 执行器执行时，prompt 中包含该环节的「期望产物」清单。
- [ ] G3 执行器执行时，prompt 中包含该环节引用的 spec 模板内容（或路径）。
- [ ] G4 注入覆盖所有执行入口（工艺 loop / 手动单 todo / cron / 飞书 / hook），行为一致。
- [ ] G5 未配置期望产物与 spec 的环节，执行行为与现状完全一致（无副作用）。

## 3. 非目标（Out of Scope）

- 不注入环节级 `skills`（`skill_names` 现用于评审门禁上下文，执行 prompt 不动）。
- 不注入阶段级 `loop_phases.spec`（留作后续可选扩展，设计文档给出最小接入点）。
- 不改前端：`step_template` 仍由工艺 YAML 编辑器维护，本次只打通后端「落库 → 运行时注入」。
- 不改执行器进程调用方式（仍把最终 message 作为位置参数传给 CLI）。

## 4. 使用场景 / 用户路径

1. 用户在工艺 YAML 里为某环节配置 `expected_artifacts` 与 `step_template`（spec 文件引用）。
2. 安装工艺 → 环节落库到 `loop_steps`，含 `expected_artifacts` 与（新）`step_template_refs`。
3. 任务运行到该环节 → 执行器收到 prompt，开头包含 `# 环节交付要求`（期望产物清单 + 参考 spec 约定）。
4. 执行器按约束产出 → 执行后产物捕获 / 门禁按期望产物校验（既有逻辑不变）。

## 5. 功能需求清单（Checklist）

- [ ] F1 新增 DB 列 `loop_steps.step_template_refs`（`TEXT NOT NULL DEFAULT '[]'`），含幂等迁移。
- [ ] F2 安装工艺时把 `link.step_template` 序列化写入该列（与 `expected_artifacts` 同构）。
- [ ] F3 运行时按 `todo_id` 反查 `loop_steps`（复用 `find_loop_step_by_todo_id`），取期望产物与 spec 引用。
- [ ] F4 运行时按 spec 引用的 `path`（`bundled://...`）读取 spec markdown 正文。
- [ ] F5 新增环节级注入 `inject_step_context`，拼 `# 环节交付要求` 段落前置到 prompt 最内层。
- [ ] F6 在第二层 `prepare_execution_state` 注入链接入（所有执行入口收口处）。
- [ ] F7 任一步失败 / 无 step / 配置全空 → 静默回退原 prompt，不阻断执行、不写回 DB。

## 6. 约束条件

- **架构约束**：注入只在第二层 `executor_service` 内存中进行；`todo.prompt` 只读，绝不写回 DB（沿用 `loop_runner.rs:806-808` 原则）。
- **一致性约束**：spec 文件运行时解析口径必须与安装期 `resolve_phase_spec_refs` 完全一致（`bundled://...` → `~/.ntd/bundled/...`）。
- **降级约束**：DB 查询失败 / JSON 解析失败 / 文件读取失败 均不得让 todo 执行失败（与现有 `inject_workspace_prompt` 等一致）。
- **质量约束**：单函数 ≤30 行（纯数据构建/线性管道除外）；`cargo clippy --all-targets -- -D warnings` 零告警；`cargo test` 通过；生产代码禁 `unwrap/expect/panic`。
- **安全约束**：spec 正文来自本地受信任目录（`~/.ntd/bundled/`），非外部输入；失败日志不得打印 prompt 正文。

## 7. 可修改 / 不可修改项

- ❌ 不可修改：执行器 CLI 调用方式、第一层 `loop_runner` 的占位符替换语义、产物捕获与门禁既有逻辑、前端。
- ✅ 可调整：注入段落的标题与排版文案、spec 正文内联的大小上限阈值、子函数拆分方式。

## 8. 接口与数据约定

- **`loop_steps.step_template_refs`**：JSON 数组串，元素 `{ "name": string, "path": string }`（`StepTemplateRef`），默认 `"[]"`。
- **`ExpectedArtifact`**：`{ name, type∈{file|text|url|json|delivery-state|repair-log}, path?, locator? }`（已存在，不动）。
- **注入段落**：标题 `# 环节交付要求`，含 `## 期望产物` 与 `## 参考 Spec 约定` 两节，用 `\n---\n` 与原 message 分隔，置于 prompt 最内层（核心任务之上）。
- **spec 正文策略**：正文 ≤ 上限内联；超上限或读取失败 → 回退为「详见 <path>」。
- 无新增 HTTP API，无新增前端字段。

## 9. 验收标准（Acceptance Criteria）

- 如果环节配了期望产物，则执行器收到的 prompt 含「## 期望产物」清单，逐条列出 name/type/path/locator。
- 如果环节配了 spec 模板且文件可读，则 prompt 含「## 参考 Spec 约定」并内联 spec 正文（超限则为路径）。
- 如果环节未配期望产物与 spec，则 prompt 与现状逐字一致（不出现空标题）。
- 如果该 todo 不属于任何 loop_step（独立 todo），则 prompt 与现状一致。
- 如果 spec 文件缺失/不可读，则该条降级为路径引用，其它条目与产物段落正常注入，执行不中断。
- 如果 DB 反查出错（查询失败或该 todo 无 step），则原样返回原 prompt，执行不中断，仅 warn 日志。
- 如果某个字段（期望产物 / spec 引用）JSON 解析出错，则**仅该字段按空处理（跳过）**，另一字段仍正常注入；执行不中断，仅 warn 日志（按字段降级，单个脏字段不拖垮整段注入）。
- 安装工艺后，`loop_steps.step_template_refs` 与 YAML 中 `step_template` 一致；升级工艺（`upgrade_process_template_loop`）后新列同步重建。

## 10. 风险与已知不确定点

- **raw SQL 手动映射**：`db/loop_.rs` 存在对 `loop_steps::Model` 的原生 SQL 手工构建，加字段会编译中断——必须同步 SELECT 列与行映射（设计文档 §4.4 已定位）。
- **spec 文件大小**：过大的 spec 全文内联会膨胀 prompt。处置：设内联上限，超限回退路径（设计文档 §7）。
- **不确定性处理**：遇到规范未覆盖的边界，优先静默回退（不阻断执行）并 warn，不自行扩大注入口径；必要时停下来请求确认。

## 11. 非目标

（与第 3 节一致，此处从略：不注入 skills / 阶段 spec；不改前端与执行器调用方式。）
