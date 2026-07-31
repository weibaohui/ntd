# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-31 | 初始版本（环节期望产物 + spec 模板注入执行器） |
| Claude | 2026-07-31 | 审查后补充 7 条测试：installer 非空落库/升级重建、spec 内联三路径、按字段降级 E2E、raw SQL 映射 |

---

# 实现总结：环节「期望产物 + spec 模板」注入执行器提示词（054）

## 1. 实现了什么

执行器执行环节 todo 时，在第二层 prompt 注入链（`prepare_execution_state`，所有执行入口的统一收口）最内层前置 `# 环节交付要求` 段落，包含：

- **期望产物清单**：从 `loop_steps.expected_artifacts` 解析，逐条列出 name/type/path/locator。
- **参考 Spec 约定**：从新列 `loop_steps.step_template_refs` 解析 spec 引用，读取 `bundled://` markdown 正文内联（超 4KB 上限或读取失败时降级为路径引用）。

环节 spec 模板此前只活在磁盘 YAML、既未落库也未注入；本次补齐「落库 → 运行时注入」全链路。

## 2. 与需求对应

| 需求项 | 实现 |
|--------|------|
| G1 spec 模板落库 | 迁移 v83 新增 `loop_steps.step_template_refs` 列；`installer::create_loop_step_for_link` 序列化 `link.step_template` 写入 |
| G2 注入期望产物 | `inject_step_context` → `build_expected_artifacts_section` |
| G3 注入 spec 模板 | `inject_step_context` → `build_step_spec_section` → `format_single_spec`（内联/路径回退） |
| G4 全入口覆盖 | 在 `stages::prepare_execution_state` 接入（loop/手动/cron/飞书/hook 都收敛于此） |
| G5 未配置无副作用 | 产物+spec 全空 / 无 step / 解析失败 → 静默回退原 message |

## 3. 关键实现点

- **注入位置**（已确认决策）：第二层 `executor_service/stages.rs`，而非第一层 `loop_runner`。理由：覆盖所有执行入口；与 `inject_expert_context` 同模式；经 `find_loop_step_by_todo_id(todo_id)` 反查 step，无需改请求结构体。
- **spec 承载**（已确认决策）：给 `loop_steps` 加列 `step_template_refs`（与 `expected_artifacts` 完全同构），installer 持久化。
- **降级原则**：DB 反查失败 / JSON 解析失败 / spec 文件读取失败 均不阻断执行，warn 后回退；`todo.prompt` 只读，绝不写回 DB（沿用 `loop_runner.rs:806-808` 原则）。
- **spec 解析口径**：提取 `load_bundled_markdown` 到 `services/process/source.rs::read_bundled_markdown` 为 pub fn，安装期（`resolve_phase_spec_refs`）与运行时注入共用，保证「安装读到什么，运行时就注入什么」。
- **prompt 层级**：`# 运行背景 > 专家角色定义 > # 任务(工作空间共识 > # 环节交付要求 > 核心 prompt)`，交付要求紧贴核心任务。

## 4. 改动文件

| 文件 | 改动 |
|------|------|
| `backend/src/db/migration/v83.rs`（新） | 加 `step_template_refs TEXT NOT NULL DEFAULT '[]'` 列 + 幂等迁移 + 测试 |
| `backend/src/db/migration/mod.rs` | 注册 v83 |
| `backend/src/db/entity/loop_steps.rs` | entity 加 `step_template_refs` 字段 |
| `backend/src/db/loop_.rs` | raw SQL SELECT + Model 映射补 `step_template_refs` |
| `backend/src/services/process/installer.rs` | `create_loop_step_for_link` 写入新列；`load_bundled_markdown` 提取后改调 `source::read_bundled_markdown` |
| `backend/src/services/process/source.rs` | 新增 pub `read_bundled_markdown` |
| `backend/src/executor_service/pre_spawn.rs` | 新增 `inject_step_context` 及 6 个子函数 + 10 个单测 |
| `backend/src/executor_service/stages.rs` | `prepare_execution_state` 最内层接入 `inject_step_context` |
| `backend/src/services/loop_runner.rs`、`services/process/transition_resolver.rs` | 测试 fixture 同步新字段（10 处 Model 字面量） |

## 5. 测试验证

- `cargo clippy --all-targets -- -D warnings`：**零告警**（相对 main 零新增；本机 rust 1.95.0 下的 43 个 lint 为 main 存量工具链漂移，与本 PR 无关）。
- `cargo test --lib`：**1509 passed**（含 v83 迁移 3 条、`inject_step_context` 系列 14 条、installer 落库/升级 2 条、raw SQL 映射 1 条）。唯一失败 `git_sync::tests::test_sync_repo_restores_deleted_file` 为 main 存量环境依赖失败，与本需求无关。
- 集成测试 `loop_step_execution_status_tests` / `services_tests` / `db_feature_supplement_tests`：**68 passed, 0 failed**。
- 单测覆盖：产物渲染（全字段/缺省）、spec 读取失败回退路径、**spec 正文内联（≤4KB）/超限（>4KB）降级/恰好 4KB 边界**、配置全空返回 None、仅产物/仅 spec、无 step 回退、端到端注入（DB 写产物→段落前置→原 message 在尾）、**按字段降级（一字段坏 JSON 时另一字段仍注入，双向）**、**installer 非空 step_template 落库**、**升级工艺后新列同步重建**、**list_loop_steps_with_todo_meta raw SQL 新列映射（非标值逐字读回 + 存量默认 '[]'）**。
- 活体 E2E（人工审查验证）：PR 构建起服务后注册假 claude CLI 落盘 argv，HTTP 执行配置了产物+spec 的环节 todo，实际收到的 prompt 含完整 `# 环节交付要求`（产物清单 + spec 正文内联 + 分隔线 + 原 prompt 在尾）；未配置环节与独立 todo 的 prompt 与现状逐字一致；`todo.prompt` 未写回 DB。

## 6. 已知限制 / 后续

- 阶段级 `loop_phases.spec` 同样未进 prompt；如需生效，可在 `inject_step_context` 内按 `step.phase_id` 读取后并入「参考 Spec 约定」段落（设计文档 §10 已给出接入点）。
- 环节级 `skills` 仍只进评审门禁上下文，未进执行 prompt（本次刻意不动）。
- spec 正文内联上限固定 4KB（`SPEC_INLINE_LIMIT`），超限改路径引用；如需调整改该常量即可。
- 安全反思：spec 正文来自本地受信任目录 `~/.ntd/bundled/`，非外部输入；失败日志不含 prompt 正文。无越权/注入面。
