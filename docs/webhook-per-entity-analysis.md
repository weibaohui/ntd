# Webhook 下沉到事项与 Loop 的改造分析

## 目标

将 `webhook` 从“设置中心里的独立资源”改为“事项 / Loop 自身的一个能力开关”。

新的用户心智模型：

- 每个事项都有固定的 webhook 地址：`/webhook/trigger/todo/{todo_id}`
- 每个 Loop 都有固定的 webhook 地址：`/webhook/trigger/loop/{loop_id}`
- 用户只需要决定“启用还是关闭”
- 启用后，在事项详情 / Loop 详情的基本信息区域展示 webhook 地址，并提供复制按钮
- 设置页不再承担 webhook 的增删改查职责

## 当前实现现状

### 前端

- 设置页通过 `SettingsPage.tsx` 挂载 `WebhooksPanel.tsx`
- `WebhooksPanel.tsx` 负责 webhook 的创建、编辑、删除、启停、URL 展示、调用记录展示
- Loop 侧的 `LoopStudioTriggersPanel.tsx` 仍然依赖“先创建一个独立 webhook，再在 trigger.config 里引用 webhook_id”
- 事项详情 `DetailHeader.tsx` 目前没有 webhook 展示区域
- Loop 详情 `LoopStudioDetailPanel.tsx` 的“基本信息”区域目前只展示启用状态、工作空间、待审批
- 关系图 `RelationMap.tsx` / `GraphBuilder.ts` 通过 `getWebhooks()` 拉取独立 webhook 节点

### 后端

- `webhooks` 表承载 webhook 的主数据：`name`、`enabled`、`default_todo_id`、`loop_id`、`webhook_type`
- `webhook_records` 表承载调用记录
- Todo webhook 触发时，后端先查 `get_webhook_by_default_todo(todo_id)`
- Loop webhook 触发时，后端先查 `get_webhook_by_loop(loop_id)`
- Loop 的 webhook 触发链路不是“loop 自身启用了 webhook”，而是“loop trigger 引用某条 webhook 记录”

这说明当前模型里有两层概念：

- 一层是“Webhook 资源”
- 一层是“Todo / Loop 是否可被 webhook 触发”

这两层在你的新需求里其实是同一个概念，因此可以收敛。

## 推荐方案

推荐采用“实体自带 webhook 开关”的方案，而不是继续保留 `webhooks` 表做管理中心兼容层。

### 新领域模型

- `todos` 新增 `webhook_enabled`
- `loops` 新增 `webhook_enabled`
- webhook URL 不再存库，按实体 id 直接推导
- `webhook_records` 继续保留，用于审计与排查

### 为什么推荐这条路径

- 与“每个事项 / Loop 都天然有一个地址，只是启用与否”的产品语义完全一致
- 前端交互最简单，不再需要“先创建 webhook，再绑定目标”
- 避免标题改名时还要同步 webhook 名称
- Loop 不再需要在 `trigger.config` 里保存 `webhook_id`
- 后端触发链路更直接，减少一次表查询与一次概念映射

## 不推荐但可选的兼容方案

保留 `webhooks` 表，只是把设置页隐藏，在事项 / Loop 编辑时自动创建或更新对应 webhook 记录。

这个方案的优点是改动较小，但缺点明显：

- 产品语义仍然是“实体背后偷偷挂了一条 webhook 资源”
- Todo / Loop 改名时需要同步 webhook.name
- Loop 仍然要维护 `trigger.config.webhook_id`
- 长期会留下隐性耦合，后面还得再清理一次

因此只适合作为极短期过渡，不适合作为最终形态。

## 数据层改造

### 表结构

建议新增迁移：

- `ALTER TABLE todos ADD COLUMN webhook_enabled INTEGER NOT NULL DEFAULT 0`
- `ALTER TABLE loops ADD COLUMN webhook_enabled INTEGER NOT NULL DEFAULT 0`

`webhooks` 表的处理建议分两阶段：

### 第一阶段

- 停止前端继续使用 `webhooks` 表
- 保留表和旧接口，避免一次性清理过大
- 所有新逻辑改为读 `todos.webhook_enabled` / `loops.webhook_enabled`

### 第二阶段

- 删除 `/api/webhooks` 相关接口
- 删除 `webhooks` 表与相关 ORM / db 层代码
- 关系图与文档全部切换到实体内建 webhook 模型

## 后端改造点

### Todo 链路

需要改造：

- `backend/src/db/entity/todos.rs`
- `backend/src/db/todo.rs`
- `backend/src/models/mod.rs`
- `backend/src/handlers/todo.rs`

主要变化：

- `Todo` DTO 增加 `webhook_enabled`
- `CreateTodoRequest` / `UpdateTodoRequest` 增加 `webhook_enabled`
- `TodoUpdate` 增加 `webhook_enabled`
- `model_to_todo()` 返回该字段
- `update_todo_full()` 支持更新该字段

### Loop 链路

需要改造：

- `backend/src/db/entity/loops.rs`
- `backend/src/db/loop_.rs`
- `backend/src/models/loop_.rs`
- `backend/src/handlers/loop_.rs`

主要变化：

- `LoopDto` / `LoopDetail` / `LoopListItem` 增加 `webhook_enabled`
- `CreateLoopRequest` / `UpdateLoopRequest` 增加 `webhook_enabled`
- `create_loop()` / `update_loop()` 支持保存该字段

### Webhook 触发 handler

需要改造：

- `backend/src/handlers/webhook.rs`

建议改法：

- Todo 触发时，不再查询 `webhooks` 表，而是直接查询 todo
- 当 `todo.webhook_enabled == false` 时返回 400
- Loop 触发时，不再查询 `webhooks` 表，而是直接查询 loop
- 当 `loop.webhook_enabled == false` 时返回 400 或 404，建议统一为 400，语义更清晰

### Loop webhook 调度

这里是本次最关键的结构点。

当前实现中：

- `/webhook/trigger/loop/{loop_id}` 先查到独立 webhook
- 再通过 `dispatcher.dispatch_webhook(webhook.id, body)` 去匹配 `loop_triggers.config.webhook_id`

如果改成“loop 自带 webhook”，建议同步把 Loop 的 webhook 触发从 `loop_triggers` 体系里抽出来。

推荐做法：

- 当 `loop.webhook_enabled == true` 时，`/webhook/trigger/loop/{loop_id}` 直接触发该 Loop
- 新增一个专用方法，例如 `dispatch_loop_webhook(loop_id, body)`
- `loop_execution.trigger_type` 仍记为 `webhook`
- `trigger_id` 可以为 `None`

这样做的结果是：

- Loop 的 webhook 不再属于“可配置触发器之一”
- 它变成 Loop 的基础能力，和“启用状态”“工作空间”同级
- 这和你的需求最一致

## 前端改造点

### 设置页

需要改造：

- `frontend/src/components/SettingsPage.tsx`
- `frontend/src/components/WebhooksPanel.tsx`

建议：

- 从 `SettingsPage.tsx` 移除 `Webhook` Tab
- `WebhooksPanel.tsx` 暂时保留文件，作为过渡期调用记录页或待删除文件

### Todo 编辑与展示

需要改造：

- `frontend/src/types/todo.ts`
- `frontend/src/utils/database/todos.ts`
- `frontend/src/components/todo-drawer/reducer.ts`
- `frontend/src/components/TodoDrawer.tsx`
- `frontend/src/components/todo-detail/DetailHeader.tsx`

建议交互：

- `TodoDrawer.tsx` 增加“Webhook 启用”开关
- `DetailHeader.tsx` 在基本信息区展示：
  - webhook 状态标签
  - webhook URL
  - 复制按钮

URL 生成规则：

- `const url = \`${window.location.origin}/webhook/trigger/todo/${todo.id}\``

显示条件：

- `selectedTodo.webhook_enabled === true`

### Loop 编辑与展示

需要改造：

- `frontend/src/types/loop.ts`
- `frontend/src/utils/database/loops.ts`
- `frontend/src/components/LoopFormModal.tsx`
- `frontend/src/components/LoopStudioDetailPanel.tsx`
- `frontend/src/components/LoopStudioTriggersPanel.tsx`

建议交互：

- `LoopFormModal.tsx` 增加“Webhook 启用”开关
- `LoopStudioDetailPanel.tsx` 在“基本信息”区域新增：
  - webhook 状态
  - webhook URL
  - 复制按钮
- `LoopStudioTriggersPanel.tsx` 移除 `webhook` 这一类触发器

这是因为新模型里，Loop webhook 已经不是“触发器配置项”，而是 Loop 自身的基础入口。

## 关系图与可视化

需要改造：

- `frontend/src/components/relation-map/RelationMap.tsx`
- `frontend/src/components/relation-map/GraphBuilder.ts`

当前关系图通过 `getWebhooks()` 拉独立节点。

新模型下建议改为：

- 对 `todo.webhook_enabled === true` 的事项生成 webhook source 节点
- 对 `loop.webhook_enabled === true` 的 Loop 生成 webhook source 节点
- 节点 id 可以改为：
  - `todo-webhook-{todo_id}`
  - `loop-webhook-{loop_id}`

这样可以继续保留“外部 webhook 来源”这一视觉表达，而不依赖独立 `webhooks` 表。

## 调用记录

`webhook_records` 建议保留，不要删。

但记录结构建议逐步从“引用 webhook_id”过渡到“引用实体”：

可选增强字段：

- `triggered_loop_id`
- `source_type`，值为 `todo | loop`
- `source_id`

如果希望先最小化改动，也可以先保留现有结构，只是允许 `webhook_id = null`。

## 分阶段实施建议

### 第 1 步

- 新增 `todos.webhook_enabled`
- 新增 `loops.webhook_enabled`
- 完成 Todo / Loop DTO 与 CRUD 打通

### 第 2 步

- 改造 `TodoDrawer` / `LoopFormModal`
- 改造 `DetailHeader` / `LoopStudioDetailPanel`
- 从设置页移除 `Webhook` Tab

### 第 3 步

- 改造 `handlers/webhook.rs`
- Todo 直接按 `todo.webhook_enabled` 判定
- Loop 直接按 `loop.webhook_enabled` 判定并触发

### 第 4 步

- 从 `LoopStudioTriggersPanel.tsx` 移除 webhook 触发器
- 让 Loop webhook 从“触发器”升级为“基础能力”

### 第 5 步

- 改造关系图
- 清理 `/api/webhooks` 前后端代码
- 更新文档与测试

## 风险点

### Loop webhook 与 trigger 体系脱钩

这是本次最大的行为变化。

需要确认的产品决策：

- Loop 的 webhook 是否还需要出现在“触发条件统计”里
- 如果不再出现在 trigger 列表，用户是否接受它只在“基本信息”中管理

从你的描述看，答案应当是“接受”，因为你希望它和事项一样，只是一个启用开关。

### 旧数据兼容

如果线上已有 `webhooks` 表数据：

- Todo：可以按 `default_todo_id` 回填 `todos.webhook_enabled = 1`
- Loop：可以按 `loop_id` 回填 `loops.webhook_enabled = 1`

是否需要保留 webhook 的 `enabled` 状态：

- 若存在同一 todo/loop 多条 webhook，建议采用“只要存在 enabled=true 的关联记录，就回填为启用”

### 文档与接口漂移

当前文档大量写的是“设置页管理 webhook”与“`/api/webhooks` 管理中心”。

这次改造后需要同步更新：

- `docs/user-guide/settings/webhooks.md`
- `docs/user-guide/features/webhooks-and-automations.md`
- `docs/ntd-api.md`
- `docs/FEATURES.md`

## 结论

这次改造最合适的落点不是“把设置页入口挪走”，而是把 webhook 的领域模型从“独立资源”改成“Todo / Loop 的内建能力”。

一句话总结：

- Todo：新增 `webhook_enabled`，详情页显示 URL 和复制按钮
- Loop：新增 `webhook_enabled`，详情页显示 URL 和复制按钮
- 设置页移除 webhook 管理
- Loop 侧删除 `webhook trigger` 配置项
- `webhook_records` 保留，`webhooks` 表进入兼容清理期
