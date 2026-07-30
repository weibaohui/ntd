| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-30 | 初始版本 |

# 需求：工艺模板 GUID 身份（040）

## 1. 背景与问题

039 把工艺列表拆成「我的/模板」双视图后，暴露出一个断裂的交互：**模板视图的系统工艺无法
转化为"我的工艺"**——只有用户层 `~/.ntd/processes/` 里的工艺可编辑，但页面上没有转化入口
（详情弹窗 YAML 页警示条写着「请先复制到用户层后再编辑」，却没有按钮）。

更深的根因是**身份模型缺陷**：

1. 现有「复制到用户层」是同名复制：`name` 是 DB 唯一键，用户层 upsert 会覆盖系统行
   （`is_system` 翻转为 false），**原模板从「模板」视图消失**。用户明确要求模板不变、
   复制产物同名共存。
2. 现存 bug：bundled 同步对系统模板"先删后插"（`bundled.rs::import_process_templates_from_bundled`），
   DB 自增 id 每次同步全部重排，`loops.process_template_id`（`ON DELETE SET NULL`，v71）
   在每次同步后被清空——已安装环路丢失与来源工艺的关联。
3. 现存 bug：用户在 YAML 里改 `name` 后重新导入，按 name upsert 会残留旧行。

经三轮方案推演（后缀名 / DB id 寻址 / GUID-in-YAML），决定采用**最彻底的 GUID 方案**：
身份跟着文件走，与 038「磁盘是唯一真源，DB 只做索引」的架构方向自洽。

## 2. 目标

1. 每个工艺 YAML 在 `process.name` 下增加 `guid` 字段（UUID v4），作为工艺的稳定身份；
   DB `process_templates` 加 `guid` 列（唯一索引），`name` 放开唯一约束。
2. 远端仓库 `ntd-resource` 的所有工艺模板批量补上 guid（本地仓库直接改、提交、推送）。
3. 同步从"先删后插"改为按 guid **reconcile**：更新保留 DB id（loops 关联不断），
   只删除远端真正下架的模板——修复 loops 关联丢失 bug。
4. 所有按 name 寻址的工艺 API 路由改为按 guid 寻址（详情/安装/保存/删除/loops/升级/版本/复制等）。
5. 「模板」视图补「复制为我的工艺」入口：纯文件复制 + 副本生成新 guid，**原模板不消失**，
   复制成功自动切到「我的」视图。
6. 新建工艺（M6 流程）生成的 YAML 自动带 guid。
7. 用户层文件缺 guid 时导入自动生成并回写进文件（用户层不受 git 管理，回写安全）。

## 3. 边界与非目标

- **不做兼容降级**：迁移时删除老数据行（`process_templates` 清空、`loops.process_template_id`
  置 NULL），不保留旧数据、不做 name 键降级——用户已拍板。注意：只删相关数据行，不删整个库。
- `step-templates`（环节原型）不加 guid——它们按 name 被工艺 YAML 内引用，是另一套机制。
- `loops` 表结构不动，继续用本地 `process_template_id` 外键。
- 系统层文件禁止本地回写 guid（每次同步 `git reset --hard` 会抹掉），系统 guid 只能来自远端仓库。

## 4. 验收标准

1. ntd-resource 全部工艺 YAML 含 guid 并已推送；应用内同步后 `~/.ntd/bundled` 文件带 guid。
2. 迁移后 `process_templates` 有 guid 唯一索引、name 无 UNIQUE；老行已清空，重扫后按 guid 重新入库。
3. 连续两次同步，系统模板 DB id 不变；已安装环路的 `process_template_id` 不再被清空。
4. 模板视图点「复制」→ 自动切「我的」→ 同名副本可见且可编辑；切回「模板」原模板仍在。
5. 编辑器按 guid 打开/保存正常；新建工艺 YAML 含 guid。
6. `cargo clippy --all-targets -- -D warnings` 零告警、`cargo test` 全绿；
   `npx tsc --noEmit` 零错误；Playwright 用例通过。
