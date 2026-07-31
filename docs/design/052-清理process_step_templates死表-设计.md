| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-31 | 初始版本 |

# 设计：清理 process_step_templates 死表（052）

## 1. 背景

`process_step_templates` 原设计为「工艺环节原型表」，供工艺 YAML 用 `step_template: <name>`
引用原型、实例化环节。但执行链路早已改造：`installer.rs::resolve_link_fields` 起，安装工艺时
执行配置（prompt / executor / expert / model / acceptance_criteria）**全部内联在工艺 YAML 的
link 里**，不再按 name 查原型表。`step_template` 字段也转型为「内联 spec 引用」
（`StepTemplateRef` 的 `{name, path}` 数组），与本表彻底无关。

截至清理前，该表只剩两类弱用途：

| 用途 | 位置 | 行为 |
|------|------|------|
| bundled 同步缓存 | `bundled.rs::import_process_templates_from_bundled` | 每次同步先 `delete_all_system_process_step_templates()`，再把 `step-templates/*.yaml` 删后插回（8 行系统数据） |
| 安装时 warn | `installer.rs::check_skill_warnings` | 按 name 查表，查不到只 `tracing::warn!`、不阻断；且查的是错表（原型表而非 skills 市场） |

核心运行链路（loops / todos / loop_steps）不依赖它。故清理是安全且收敛的做法。

## 2. 设计决策

### 2.1 `check_skill_warnings` 直接删除
skill 是执行器级自由文本、运行时按名注入，原型表查不到不代表 skill 缺失；warn 查错表、价值低。
删除还省掉安装/升级各一次 DB 往返。

### 2.2 用 v82 迁移 DROP TABLE
- v71 建表、v72 加 `category` 列：**已部署迁移不可变**，保持原样。
- 新增 v82 `DROP TABLE IF EXISTS process_step_templates`：SQLite 原生幂等，且会一并删除
  `idx_process_step_templates_*` 索引与 `set_process_step_templates_*` 触发器。
- 全新库走「v71 建表 → v82 删表」，旧库升级到 v82 直接删表，两条路径收敛到同一终态。
- 表内 8 行系统缓存数据随表删除——预期行为，不做数据迁移。

### 2.3 bundled 同步不再处理 step_template
- 删除 `delete_all_system_process_step_templates()` 调用与环节原型「先删后插」分支。
- `scan_and_upsert_system_processes` 返回类型收敛为 `(process_count, seen_guids)`。
- 仓库残留的 `step_template:` yaml 会被 `parse_process_file` 解析失败而静默跳过（有既有测试覆盖）。

### 2.4 ntd-resource 源头同步清理
- 本地 `~/.ntd/bundled` 是同步镜像（`git reset --hard origin/main`），不能直接改镜像。
- 源头仓库 `/Users/weibh/projects/rust/ntd-resource` 删除 `processes/step-templates/` 并 push，
  下次同步镜像自动消失。
- 已核实 `processes/software/*.yaml` 只用 `step_template: []` 空列表，无任何工艺按名引用原型。

## 3. 范围边界

**不动：**
- 前端 `link.step_template`（`StepTemplateRef`）与工艺 YAML 的 `step_template: []` 字段——内联 spec 引用，另一套机制。
- v71 / v72 迁移本体。
- `process_templates` 的 guid reconcile（`delete_system_process_templates_not_in`）。
- 历史文档（025 / 026 / 029 / 036 / 040）——时间点记录，不改写。

**删：**
- 表 + Entity + DB 层三函数 + installer 的 warn + bundled 导入分支 + 相关测试。
- ntd-resource 的 `processes/step-templates/` 目录。

## 4. 改动清单

| 文件 | 改动 |
|------|------|
| `backend/src/db/migration/v82.rs` | 新建：`DROP TABLE IF EXISTS` + 幂等/删表测试 |
| `backend/src/db/migration/mod.rs` | 注册 `mod v82` 与迁移实例 |
| `backend/src/db/entity/process_step_templates.rs` | 删除 |
| `backend/src/db/entity/mod.rs` | 删 `pub mod` 与 `pub use ... as ProcessStepTemplates` |
| `backend/src/db/process_template.rs` | 删三个函数 + `use` 行 |
| `backend/src/handlers/bundled.rs` | 删同步写入分支、环节原型结构体/解析/写入函数、相关测试、更新注释与日志 |
| `backend/src/services/process/installer.rs` | 删 `check_skill_warnings`、两处调用、测试 `seed_step_template` |
| `backend/src/db/migration/v72.rs` | 仅测试：补建前置表（v82 使 fresh 链上该表不复存在） |
| `docs/design/052-...设计.md` | 本文档 |
| `docs/requirements/052-...需求.md` | 需求文档 |
| `docs/requirements/052-...实现总结.md` | 实现总结 |
| `ntd-resource`（外部仓库） | 删 `processes/step-templates/`，分支提交并 push |

## 5. 验证策略

1. `cargo clippy --all-targets -- -D warnings` 零告警。
2. `cargo test` 全绿（v82 新测试；v72 测试适配 fresh 链；installer/bundled/process_template 改动后回归）。
3. `make dev` 启动后 `sqlite3 ~/.ntd/data.dev.db ".tables"` 无 `process_step_templates`。
4. 触发一次 bundled 同步：工艺模板导入正常，残留 step-template yaml 静默忽略。

## 6. 风险

- 运行中 loops / todos 零影响：安装时执行配置已内联，不依赖本表。
- 8 行系统缓存数据被 DROP——预期。
- `process_templates` 同步/reconcile 未触碰，无连带。
