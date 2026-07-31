| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-31 | 初始版本（需求 052 实现总结） |

# 实现总结：清理 process_step_templates 死表（052）

## 1. 背景

`process_step_templates` 原为「工艺环节原型表」，执行链路改造后已废弃：安装工艺时执行配置
全部内联在工艺 YAML 的 link 里，不再按 name 查原型表。清理前只剩两类弱用途——bundled 同步
「先删后插」的本地缓存、安装时 `check_skill_warnings` 查错表仅 warn。本次随表摘掉全部读写点。

## 2. 改动清单（对应需求文档 §5）

| 文件 | 改动 |
|------|------|
| `backend/src/db/migration/v82.rs` | **新增**：`DROP TABLE IF EXISTS process_step_templates`，含删表 + 幂等两个测试 |
| `backend/src/db/migration/mod.rs` | 注册 `mod v82` 与 `V82DropProcessStepTemplates` |
| `backend/src/db/entity/process_step_templates.rs` | **删除** |
| `backend/src/db/entity/mod.rs` | 删除 `pub mod process_step_templates;` 与 `ProcessStepTemplates` 重导出 |
| `backend/src/db/process_template.rs` | 删除 `get_process_step_template_by_name` / `upsert_system_process_step_template` / `delete_all_system_process_step_templates`，`use` 行去 `process_step_templates` |
| `backend/src/handlers/bundled.rs` | 删除同步写入分支（`delete_all_system_process_step_templates` 调用、环节原型解析/写入）、`BundledStepTemplateFile`/`BundledStepTemplateDefinition`/`parse_step_template_file`/`upsert_process_step_template`、相关测试；`scan_and_upsert_system_processes` 返回类型收敛为 `(usize, Vec<String>)`；注释与日志去 step_template 表述 |
| `backend/src/services/process/installer.rs` | 删除 `check_skill_warnings` 及两处调用（安装/升级）、测试辅助 `seed_step_template` 及两处调用 |
| `backend/src/db/migration/v72.rs` | 仅测试适配：补建 `process_step_templates` 前置表（v82 使 fresh 迁移链上该表不复存在） |
| `docs/design/052-清理process_step_templates死表-设计.md` | 设计文档 |
| `docs/requirements/052-清理process_step_templates死表-需求.md` | 需求文档 |
| `docs/requirements/052-清理process_step_templates死表-实现总结.md` | 本文档 |
| `ntd-resource`（外部仓库，分支 `chore/drop-step-templates`） | 删除 `processes/step-templates/` 8 个 yaml，commit `818b69fe`（push 待确认） |

**未动**：前端 `link.step_template`（spec 引用，另一套机制）、v71/v72 迁移本体、
`process_templates` 的 guid reconcile、历史文档。

## 3. 验证证据

### 3.1 编译与静态检查
```
cd backend && cargo clippy --all-targets -- -D warnings
→ Finished（零告警零错误）
```

### 3.2 单元/集成测试
```
cd backend && cargo test
→ 26 个测试二进制全部 ok，0 failed
  - 新增 v82 测试：v82_drops_process_step_templates / v82_is_idempotent 通过
  - v72 测试适配后通过；installer / bundled / process_template 改动后回归通过
```

### 3.3 运行时迁移（dev 库）
重启 dev 服务（`make dev`）后：
```
sqlite3 ~/.ntd/data.dev.db ".tables"
→ process_step_templates 不存在（v82 已 DROP）
→ process_templates / loop_phases / loop_steps 等正常保留
```

### 3.4 bundled 同步链路
dev 启动同步日志：
```
从 bundled/processes 导入了 4 个工艺模板        （不再出现「环节原型」计数）
```
残留的 8 个 step-template yaml 被 `parse_process_file` 解析失败而静默忽略，无报错。

## 4. 完成与遗留事项

- **ntd-resource push：已完成**。`chore/drop-step-templates` 推送并 fast-forward 合入
  `origin/main`（`fca4dbd7..818b69fe`，远端 hooks PASSED），本地分支已清理。
- **镜像已随同步清除**：触发 `/api/v1/bundled/sync?subdir=processes` 后，
  `~/.ntd/bundled/processes/step-templates/` 已不存在，`processes/` 仅剩 `software/`。
- 生产库（`~/.ntd/data.db`）会在下次生产启动时自动应用 v82，无需人工干预。

## 5. 影响与风险

- 运行中 loops / todos 零影响（执行配置已内联）。
- dev 库 8 行缓存数据已随 DROP 删除——预期行为。
