| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-30 | 初始版本 |

# 设计：工艺模板 GUID 身份（040）

## 1. 方案概述

每个工艺 YAML 在 `process.name` 下加 `guid`（UUID v4）作为稳定身份；DB 加 `guid` 唯一索引、
放开 `name` 唯一约束；同步从"先删后插"改为按 guid reconcile；API 路由从 `{name}` 改为
`{guid}` 寻址；「复制为我的工艺」= 纯文件复制 + 副本换新 guid，原模板不消失。

关键约束（决定落地顺序）：系统层每次同步 `git reset --hard`（`git_sync/mod.rs:267`），
本地写系统文件会被抹掉，所以 **guid 必须先落到远端仓库**，应用侧再消费。

用户拍板：不做兼容降级——迁移时清空 `process_templates` 老行、`loops.process_template_id`
置 NULL；用户层磁盘文件保留，重扫时自动生成 guid 回写后重新入库。

## 2. 第一部分：ntd-resource 仓库批量加 guid

本地仓库 `/Users/mac/projects/rust/ntd-resource`（gitcode，main）：

- python3 脚本扫 `processes/**/*.yaml`（排除 `step-templates/`——环节原型按 name 被
  YAML 内引用，不在范围），仅处理含顶层 `process:` 映射的文件
- 行级插入：在 `process:` 块的 `  name: xxx` 行后插入 `  guid: <uuid4>`，
  不动其他任何字节（保住注释与块标量格式）；已有 guid 的跳过（幂等可重跑）
- 抽查 `git diff` 后提交推送 main

## 3. 第二部分：应用仓库后端

### 3.1 YAML schema（`handlers/bundled.rs:702`）

```rust
pub struct BundledProcessDefinition {
    #[serde(default)]
    pub guid: String,   // 040：工艺稳定身份；系统层由远端仓库提供，用户层缺失时导入生成回写
    pub name: String,
    ...
}
```

`#[serde(default)]` 让无 guid 文件可解析，由导入逻辑决定生成还是拒绝。

### 3.2 迁移 v79（`db/migration/v79.rs`）

SQLite 无法删内联 UNIQUE，重建表：

1. `UPDATE loops SET process_template_id = NULL`（显式解关联，语义清晰不依赖 DROP 触发 FK 行为）
2. `DROP TABLE process_templates` + 按新 schema 重建：`guid TEXT`（唯一索引
   `uk_process_templates_guid`）、`name` 去掉 UNIQUE（保留普通索引）、其余列不变
   （含 v72 `previous_version_id`；不再有 v71 遗留的 `definition` 列——038 已删）
3. 老行不迁移（用户拍板删老数据）；重建为空表，由重扫填充

实体 `db/entity/process_templates.rs` 加 `guid: String`。

### 3.3 导入/同步 reconcile（替代"先删后插"）

**DB 层**（`db/process_template.rs`）：`upsert_system/user_process_template` 的查找键从
`name` 改为 `guid`；新增 `get_process_template_by_guid`、`delete_system_process_templates_not_in(guids)`
（reconcile 删除远端已下架的）。

**系统层**（`bundled.rs::import_process_templates_from_bundled`）：
- 废弃 `delete_all_system_process_templates` 调用
- 扫描仓库文件：无 guid → `warn` 跳过（提示仓库需更新，不做 name 降级）；有 guid → 按 guid upsert
  （已有行**保留 id** 更新元数据与 source_path，`loops.process_template_id` 关联不断）
- 扫描结束：`delete_system_process_templates_not_in(本次 guid 集)`——只删真正下架的，
  此时 loops SET NULL 是正确语义

**用户层**（`user_dir.rs`）：
- 文件无 guid → 生成 UUID 行级回写（`name:` 行后插入，与仓库脚本同一规则）再解析
- guid 冲突（两文件同 guid）→ 后者 warn 跳过
- 用户层 upsert 同样按 guid——顺带修复"YAML 里改名残留旧行"（name 改了 guid 不变，原地更新）

### 3.4 路由 {name} → {guid}（`handlers/process.rs`）

| 端点 | 改动 |
|---|---|
| `GET /api/**/bundled/processes/{name}` 详情 | `{guid}`，按 guid 查 |
| `POST .../processes/{name}/install` 安装 | `{guid}` |
| `PUT /api/v1/processes/{name}` 保存 | `{guid}` |
| `DELETE /api/v1/processes/{name}` 删除 | `{guid}` |
| `GET .../{name}/loops`、`POST .../{name}/loops/{id}/upgrade` | `{guid}` |
| `GET .../{name}/versions`、`.../versions/{v}/diff` | `{guid}` |
| `POST .../{name}/copy-to-user` | `{guid}`，逻辑简化见 3.5 |
| `ProcessTemplateListItem` DTO | 加 `guid` 字段 |
| recommend 响应 | 返回 `template_guid`（前端高亮按 guid，同名不歧义） |

`get_process_template_by_name` 保留（仅内部 reconcile/测试用），路由层全部走 guid。

### 3.5 复制端点简化（本需求初心）

`copy_process_to_user` 重写为：

1. 按 guid 查模板（系统/用户都可复制——用户工艺也能再复制一份）
2. **纯文件复制**到用户层同相对路径；目标文件名冲突时加 `-1`/`-2` 后缀
3. 副本里 `guid:` 行替换为新 UUID（单行精确替换，行级操作同仓库脚本规则）
4. `import_user_process_templates` 重扫入库
5. 返回 `{ user_source_path, guid, name }`

原 409 同名检查删除——同名现在是合法场景。

## 4. 第三部分：前端

### 4.1 API 层（`api/bundled.ts`）

`ProcessTemplate` 加 `guid: string`；`getProcess/installProcess/putProcess/deleteProcess/
listProcessLoops/upgradeProcessLoop/copyProcessToUser` 参数从 name 改 guid；
recommend 响应用 `template_guid`。

### 4.2 路由与回跳

- `useViewState`：`processName` 状态改 `processGuid`（URL 参数 `?guid=xxx`）
- `App.tsx:201` 环路详情「来源工艺」回跳：loops API 响应加 `process_template_guid`
  （后端 join process_templates 取），回跳带 guid
- `ProcessPage`：`processName` prop → `processGuid`；编辑器跳转 `pushUrl('processes', { processMode: 'edit', guid })`
- `ProcessEditor`：`processName` prop → `processGuid`，加载/保存/删除/复制全部按 guid

### 4.3 复制交互（`ProcessPage`）

- 模板视图卡片 actions 加「复制」按钮（`renderProcessCard` 内 `p.is_system` 分支）；
  详情弹窗 YAML 警示条旁加「复制为我的工艺」按钮
- 点击 → `copyProcessToUser(guid)` → 成功提示「已复制为我的工艺：xxx」→
  自动切「我的」视图（复用 039 `handleScopeChange('mine')`）
- `ProcessTemplatesTab`（设置页）调用点同步改 guid

### 4.4 新建工艺生成 guid

`buildEmptyProcessYaml`（`components/process/`）生成的 YAML 加 `guid: crypto.randomUUID()`；
`CreateProcessMetaModal` 的 name 唯一性预检保留（UX 层防重名，DB 层面允许同名）。

## 5. 测试计划

1. **后端单测**：
   - reconcile：新增/更新保留 id/删除下架三分支
   - 用户层：无 guid 回写后入库、guid 冲突跳过
   - upsert by guid：同 guid 改名原地更新（验证残留旧行修复）
   - 复制：副本 guid 不同、name 相同、原文件不动
   - v79 迁移：重建后 schema 正确、老行清空、loops 关联置 NULL
2. `cargo test` + `cargo clippy --all-targets -- -D warnings` 零告警
3. `npx tsc --noEmit` 零错误
4. Playwright `check_process_page.spec.ts`：复制→自动切「我的」→副本可编辑；
   切回「模板」原模板仍在；用 DELETE 清理副本
5. 端到端：应用内同步（拉带 guid 的仓库）→ 连续两次同步系统模板 id 不变 → 复制链路

## 6. 风险与回滚

- **风险**：迁移清空 `process_templates` 老行，若用户层文件解析失败会丢模板——缓解：
  用户层文件保留在磁盘，重扫失败有 warn 日志；系统层由远端仓库保证。
- **回滚**：应用侧 revert commit；ntd-resource 的 guid 字段对旧版应用无害
  （serde 忽略未知字段），不需要回滚仓库。
