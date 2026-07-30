| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-30 | 初始版本 |

# 实现总结：工艺模板 GUID 身份（040）

## 1. 需求对应

对应 `docs/requirements/040-工艺模板GUID身份-需求.md` 与 `docs/design/040-工艺模板GUID身份-设计.md`。
经三轮推演（后缀名 / DB id 寻址 / GUID-in-YAML）后采用最彻底的 GUID 方案：身份随文件走，
与 038「磁盘唯一真源」自洽；顺带修复两个现存 bug。

## 2. 改动清单

### 远端仓库 ntd-resource（独立提交 `787e5d3d`，已推送）
- `processes/software/*.yaml`（4 个工艺模板，排除 step-templates）在 `process.name` 下插入 `guid: <uuid4>`。

### 后端

- **YAML schema**（`handlers/bundled.rs`）：`BundledProcessDefinition` 加 `#[serde(default)] guid`。
- **迁移 v79**（`db/migration/v79.rs`）：重建 `process_templates`——`guid TEXT NOT NULL` + 唯一索引、
  name 去掉 UNIQUE；按用户决策清空老行（`UPDATE loops SET process_template_id=NULL` + DROP/CREATE），
  恢复索引与时间戳触发器；幂等（已有 guid 列则跳过）。
- **实体**（`db/entity/process_templates.rs`）：加 `guid` 字段。
- **DB 层**（`db/process_template.rs`）：`upsert_system/user_process_template` upsert 键 name→guid
  （含 name 同步更新，修复改名残留）；新增 `get_process_template_by_guid`、
  `delete_system_process_templates_not_in`；`delete_process_template` 改按 guid；移除
  `delete_all_system_process_templates`。
- **同步 reconcile**（`handlers/bundled.rs`）：系统层从"先删后插"改为按 guid 对账（保留 id，
  loops 关联不断；只删远端下架的）；守卫 `process_count==0` 时不删除（避免仓库未更新时误清）。
- **用户层回写**（`services/process/user_dir.rs` + 新模块 `services/process/guid.rs`）：文件缺 guid
  时生成 UUID 行级回写（行级操作保格式），按 guid upsert；guid 冲突后者 warn 跳过。
- **路由 {name}→{guid}**（`handlers/process.rs`）：详情/安装/保存/删除/loops/升级/versions/diff/copy
  约 10 个端点；`ProcessTemplateListItem` 加 guid；recommend 返回 `template_guid`；
  `copy_process_to_user` 重写为纯文件复制 + 副本换新 guid（文件名冲突加 `-1`）。
- **环路透出**（`models/loop_.rs`）：`LoopDto` 加 `process_template_guid`，`with_process_template` 注入。

### 前端

- **API 层**（`api/bundled.ts`）：`ProcessTemplate` 加 guid；所有工艺 API 参数 name→guid；
  recommend 响应用 `template_guid`。
- **路由**（`hooks/useViewState.ts`、`App.tsx`）：URL 参数 `name`→`guid`、状态 `processName`→`processGuid`；
  环路「来源工艺」回跳按 guid（旧环路无 guid 时回退 name）。
- **工艺页**（`ProcessPage.tsx`）：寻址/高亮全 guid 化；模板视图卡片加「复制」按钮、详情弹窗警示条
  加「复制为我的工艺」按钮，成功后自动切「我的」视图。
- **编辑器**（`ProcessEditor.tsx`）：prop `processGuid`；系统工艺复制后直接跳副本编辑器。
- **新建工艺**（`buildEmptyProcessYaml.ts` + `CreateProcessMetaModal.tsx`）：`crypto.randomUUID()` 写入 YAML。
- **设置页**（`ProcessTemplatesTab.tsx`）、`types/loop.ts`：调用点与类型同步 guid 化。

## 3. 验证结果

| 项 | 结果 |
|---|---|
| ntd-resource 远端 | 4 个模板含 guid，已推送；同步后本地 bundled 带 guid |
| v79 迁移 | guid 列 + 唯一索引、name 无 UNIQUE；老行清空后重扫入库 |
| reconcile | 系统模板 id 跨同步稳定；loops 关联不再被 SET NULL |
| 复制端到端 | 副本新 guid、同名共存、原模板不消失（curl + Playwright 双验证） |
| `cargo test`（全量） | 1980 passed，0 failed |
| `cargo clippy --all-targets -- -D warnings` | 零告警 |
| `npx tsc --noEmit` | 零错误 |
| `npm run build` | 通过（chunk 体积告警为既有项，非本次引入） |
| Playwright `check_process_page.spec.ts` | 7/7 通过（含 040 复制用例：模板仍在 + 副本共存 + 清理） |

## 4. 关键设计取舍

- **guid 只能来自远端仓库**：系统层每次同步 `git reset --hard`，本地回写会被抹掉——故先落地
  ntd-resource；导入时无 guid 的系统文件 warn 跳过（不做 name 降级，用户已拍板删老数据）。
- **用户层回写安全**：用户层不受 git 管理，缺 guid 时生成回写是长期机制（含手写/M6 新建文件）。
- **loops 外键不变**：继续用本地 `process_template_id`；guid 仅作外部寻址，回跳时 id→guid 解析。
- **reconcile 守卫**：`process_count==0`（仓库未更新）时不删除，避免误清现有系统行。

## 5. 环境备注

- 开发实例由 launchd `ntd-dev` 看护；改代码后 `cargo build` + `launchctl kickstart -k gui/502/ntd-dev` 生效。
- v79 迁移清空表后需触发同步（`POST /api/v1/bundled/sync {"subdir":"all"}`）重新导入；
  注意同步接口在 **v1 路径**（`/api/v1/bundled/sync`）可用，非 v1 的 `/api/bundled/sync` 未注册。
