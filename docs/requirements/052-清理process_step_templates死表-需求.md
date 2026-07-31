| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-31 | 初始版本（清理 process_step_templates 死表） |

# 需求：清理 process_step_templates 死表（052）

## 1. 背景与问题

`process_step_templates` 原本是「工艺环节原型表」，供工艺 YAML 用 `step_template: <name>` 引用。
执行链路早已改造：**安装工艺时不再查原型表**，执行配置（prompt / executor / expert / model /
acceptance_criteria）全部内联在工艺 YAML 的 link 里
（见 `backend/src/services/process/installer.rs::resolve_link_fields` 及其注释：
「step_template 已转为 spec 引用（不再查原型表），执行配置完全以内联字段为准」）。

核心运行链路（loops / todos / loop_steps）**完全不依赖**这张表。它当前只剩两类弱用途：

1. **启动同步写入（纯缓存）**：`backend/src/handlers/bundled.rs` 每次 bundled 同步时，
   先 `delete_all_system_process_step_templates()`，再扫描 `~/.ntd/bundled/processes/step-templates/*.yaml`
   「先删后插」回表。dev 库现有 8 行（write-prd / tech-design / plan-tasks / run-tests / tdd-cycle /
   commit-release / human-confirm / write-stories），全是 bundled yaml 的本地缓存。
2. **安装时 warn（查错表，不阻断）**：`installer.rs::check_skill_warnings` 按 name 查这张表，
   查不到只 `tracing::warn!`，不阻断安装。而且它查的是「环节原型表」而非「skills 市场」，
   与 skill 真实可用性无关——skill 是执行器级自由文本、运行时按名注入。

结论：这是一张可安全清理的死表。本次删表 + 摘掉所有读写点 + 清 ntd-resource 源头目录。

### 1.1 已确认的决策

- **`check_skill_warnings` → 直接删除**。理由：skill 是执行器级自由文本、运行时按名注入，
  warn 查的还是错表（原型表而非 skills 市场），价值很低；删掉还能省掉安装/升级时一次 DB 往返。
- **ntd-resource 源头由本仓库作者清理**：源头仓库本地路径 `/Users/weibh/projects/rust/ntd-resource`
  （remote `https://gitcode.com/weibaohui/ntd-resource`）。本地 `~/.ntd/bundled` 只是同步镜像
  （每次同步 `git reset --hard origin/main`），不能直接改镜像，必须改源头并 push。

## 2. 目标

1. 删除 `process_step_templates` 表（新增 v82 迁移 `DROP TABLE`）。
2. 摘掉后端所有读写点：`installer.rs`、`bundled.rs`、`db/process_template.rs`、实体、相关测试。
3. 清理 ntd-resource 源头的 `processes/step-templates/` 目录并推送。
4. 同步新增设计文档与实现总结文档（遵循项目「先文档后编码」）。

## 3. 边界与非目标（切勿误伤）

- **前端 `link.step_template` 不动**：它是 `StepTemplateRef` 的 `{name, path}` 数组
  （见 `frontend/src/components/process/propertyForms/LinkPropertyForm.tsx`、
  `frontend/src/components/process/nodes/LinkNode.tsx`），是「内联 spec 引用」机制，
  与本表无关。
- **工艺 YAML 里的 `step_template: []` 字段不动**：这是上述 spec 引用字段的 YAML 形态
  （如 `~/.ntd/bundled/processes/software/oral-requirement.yaml` 里的空列表）。
- **v71 / v72 迁移不可改**：已部署迁移视为不可变（`backend/src/db/migration/v71.rs` 建表、
  `v72.rs` 加 `category` 列）。v71 建表 → v82 删表，新旧库都能收敛；v71 测试断言表存在仍成立。
- **历史文档不改写**：`docs/{design,requirements}/025、026、029、036、040` 是时间点记录，
  仅新增 052 文档。
- **`process_templates` 的 guid reconcile 不触碰**：`delete_system_process_templates_not_in`
  及工艺模板同步逻辑保持原样。
- **不做数据迁移/降级**：表里 8 行系统缓存数据随 `DROP TABLE` 直接丢弃，是预期行为。

## 4. 现状清单（实现时的定位坐标，均已核实）

### 4.1 写入 / 维护点
| 位置 | 说明 |
|------|------|
| `backend/src/db/migration/v71.rs` | 建表（不改） |
| `backend/src/db/migration/v72.rs` | 加 `category` 列（不改） |
| `backend/src/db/entity/process_step_templates.rs` | Entity，整文件删除 |
| `backend/src/db/entity/mod.rs:23` | `pub mod process_step_templates;` 删除 |
| `backend/src/db/entity/mod.rs:62` | `pub use super::process_step_templates::Entity as ProcessStepTemplates;` 删除 |
| `backend/src/handlers/bundled.rs` 约 558 行 | `import_process_templates_from_bundled` 调 `delete_all_system_process_step_templates()` |
| `backend/src/handlers/bundled.rs` 约 658–667 行 | `scan_and_upsert_system_processes` 内环节原型解析/写入分支 |
| `backend/src/handlers/bundled.rs` 约 790–853 行 | `BundledStepTemplateFile` / `BundledStepTemplateDefinition` / `parse_step_template_file` / `upsert_process_step_template` |

### 4.2 唯一生产读取点
| 位置 | 说明 |
|------|------|
| `backend/src/services/process/installer.rs::check_skill_warnings` 约 688–706 行 | 调 `get_process_step_template_by_name`，只 warn |
| `installer.rs` 约 36 行 | `install_process_template` 内调用点 |
| `installer.rs` 约 589 行 | `upgrade_process_template_loop` 内调用点 |

### 4.3 DB 层（`backend/src/db/process_template.rs`）
| 函数 | 行 |
|------|----|
| `get_process_step_template_by_name` | 约 139–147 |
| `upsert_system_process_step_template` | 约 151–206 |
| `delete_all_system_process_step_templates` | 约 291–299 |
| `use crate::db::entity::{process_step_templates, process_templates};` | 第 6 行（去掉前者） |

### 4.4 测试
| 位置 | 说明 |
|------|------|
| `installer.rs::seed_step_template` 约 764 行 | 测试辅助，调 `upsert_system_process_step_template`，删除 |
| `installer.rs` 约 860、958 行 | `seed_step_template` 的两处调用，删除（这两个测试本身不断言原型表，去掉 seed 仍通过） |

### 4.5 ntd-resource 源头（`/Users/weibh/projects/rust/ntd-resource`）
- `processes/step-templates/` 下 8 个 yaml（commit-release / human-confirm / plan-tasks / run-tests /
  tdd-cycle / tech-design / write-prd / write-stories），顶层均为 `step_template:`。
- 已核实：`processes/software/*.yaml`（如 oral-requirement.yaml）只用 `step_template: []` 空列表，
  **无任何工艺按名引用原型**，删目录安全。

## 5. 任务分解

### 5.1 后端代码清理

**5.1a `backend/src/services/process/installer.rs`**
- 删除 `check_skill_warnings`（约 688–706）。
- 删两处调用（约 36、589）。
- 删测试辅助 `seed_step_template`（约 764）及两处调用（约 860、958）。
- 测试 YAML 里的 `step_template: []` 保留（属 YAML schema，范围外）。

**5.1b `backend/src/handlers/bundled.rs`**
- `import_process_templates_from_bundled`：删 `delete_all_system_process_step_templates()` 调用
  （约 558）及注释中环节原型「先删后插」描述。
- `scan_and_upsert_system_processes`：返回类型 `(usize, usize, Vec<String>)` → `(usize, Vec<String>)`
  （去掉 `step_template_count`）；删环节原型解析/写入分支（约 658–667）。
- 删 `BundledStepTemplateFile` / `BundledStepTemplateDefinition` / `parse_step_template_file` /
  `upsert_process_step_template`（约 790–853）。
- 同步更新 `import_process_templates_from_bundled` doc 注释与日志（去掉 step_template 表述）。

**5.1c `backend/src/db/process_template.rs`**
- 删三个函数（4.3 表）。
- `use` 行去掉 `process_step_templates`。

**5.1d 实体**
- 删文件 `backend/src/db/entity/process_step_templates.rs`。
- `entity/mod.rs` 删第 23、62 行。

### 5.2 迁移 v82（DROP TABLE）

新建 `backend/src/db/migration/v82.rs`，仿 `v81.rs` 结构：
```rust
//! V82 迁移：删除废弃的 process_step_templates 表（需求 052）。
//! step_template 原型机制已废弃：安装工艺时执行配置全部内联在 YAML link，
//! 不再查原型表。表仅剩 bundled 同步缓存与一处查错表的 warn，清理之。
//! 幂等：DROP TABLE IF EXISTS 原生幂等；SQLite 会一并删除其索引与触发器。
use super::Migration;
use crate::db::Database;

pub(super) struct V82DropProcessStepTemplates;

#[async_trait::async_trait]
impl Migration for V82DropProcessStepTemplates {
    fn version(&self) -> i64 { 82 }
    fn name(&self) -> &'static str { "V82DropProcessStepTemplates" }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        db.exec("DROP TABLE IF EXISTS process_step_templates").await
    }
}
```
- 补 `#[cfg(test)]`：应用后 `table_exists("process_step_templates") == false`，且二次应用幂等
  （用 `super::super::table_exists`，与 v71 测试同源；`db.exec` 用法见 v71）。
- 注册 `backend/src/db/migration/mod.rs`：加 `mod v82;` 与 `all_migrations()` 末尾
  `Box::new(v82::V82DropProcessStepTemplates)`。
- 索引（`idx_process_step_templates_*`）与触发器（`set_process_step_templates_*`）
  随表一并删除，无需单独清理。

### 5.3 ntd-resource 源头（`/Users/weibh/projects/rust/ntd-resource`）
- 新建分支（如 `chore/drop-step-templates`），`git rm -r processes/step-templates/`，commit。
- **push 前再次向用户确认**（面向 gitcode 外部仓库、不可逆）。
- push 后，本地 `~/.ntd/bundled` 在下次 `make dev` 同步时（`git reset --hard origin/main`）
  自动清掉 step-templates 目录，无需手动动镜像。

### 5.4 文档
- 设计文档：`docs/design/052-清理process_step_templates死表-设计.md`。
- 实现总结：`docs/requirements/052-清理process_step_templates死表-实现总结.md`。

## 6. 验收标准

1. `process_step_templates` 表及其 Entity、DB 三函数、bundled 导入分支、`check_skill_warnings` 全部移除；
   `cargo check` / `cargo clippy --all-targets -- -D warnings` 零告警零错误。
2. v82 迁移应用后表不存在；`cargo test` 全绿（含 v82 新测试、installer/bundled/process_template
   改动后测试）。
3. `make dev` 启动正常，`sqlite3 ~/.ntd/data.dev.db ".tables" | grep process_step_templates` 无结果；
   日志无 step_template 相关报错。
4. 触发一次 bundled 同步，工艺模板导入正常，残留的 step-templates yaml 被静默忽略。
5. ntd-resource 已删除 `processes/step-templates/` 并 push；重新同步后镜像目录消失。
6. 运行中的 loops / todos / loop_steps 不受影响（安装时执行配置已内联）。

## 7. 风险

- 运行中实例零影响：安装时执行配置已内联到 loop_steps / todos。
- 8 行系统缓存数据被 DROP——预期（仅为 bundled yaml 的本地缓存）。
- `process_templates` 的 guid reconcile 不受影响，未触碰。
- 全新库走 v71 建表 → v82 删表，与旧库升级到 v82 结果一致。
