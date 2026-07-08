# 事项中心改版设计方案

## 背景

当前事项列表把多种性质不同的对象混在同一个列表中展示，包括手动创建的一次性事项、闪念事项、黑板生成的问题处理事项、周期自动化事项、Webhook 触发事项、自动化评审事项、黑板分析事项，以及被 Loop 引用的事项。

这些对象虽然都可以落到 Todo 数据结构上，但用户管理它们时真正关心的是“这个事项由什么驱动”：

- 手动触发事项关注的是“是否需要我现在处理或手动执行”。
- 时间驱动事项关注的是“周期配置是否存在、是否暂停、下次何时运行”。
- 事件驱动事项关注的是“外部事件或 Webhook 是否会触发它”。
- Loop 驱动事项关注的是“它被哪个流程引用、在流程中扮演什么环节”。
- 已归档事项关注的是“日常隐藏，但历史记录和引用关系仍可追溯”。

因此，继续使用单一密集列表会带来两个问题：

- 信息密度过高，操作按钮挤在列表行里，浏览和决策成本高。
- 分类维度混杂，用户难以判断一个事项为什么出现在当前位置。

本方案目标是重新设计“事项”页面，将其升级为更适合管理自动化事项的“事项中心”。

## 设计目标

1. 用“驱动方式”替代“收件箱 / 琐碎 / 重复”这类不稳定概念。
2. 支持事项在手动触发、时间驱动、事件驱动、已归档之间低成本调整。
3. 明确限制：事项不能手动调整为 Loop 驱动，Loop 归属只能由 Loop 配置产生。
4. 用卡片式页面替代当前挤压的列表操作区。
5. 保留现有事项详情页和执行记录页，降低改造范围。
6. 归档不删除数据，不破坏执行记录、黑板结论、Loop 引用等历史关系。
7. 数据模型只新增 `archived_at`，其余分类由现有事实字段推导。

## 一级分类

事项中心采用五个一级分类：

```text
手动触发 | 时间驱动 | 事件驱动 | Loop 驱动 | 已归档
```

这五类是事项的主视图归属，不是新增数据库枚举字段。系统根据现有字段计算 `computed_bucket` 返回给前端。

## 手动触发

手动触发用于承载一次性、临时性、闪念式、问题处理类任务。

典型来源：

- 用户手动创建的普通事项。
- 闪念创建的事项。
- 黑板创建的问题处理事项。
- 一次性的黑板分析任务。
- 一次性的自动化评审结果处理事项。

判断规则：

```text
未归档
且未被 Loop 引用
且没有周期调度配置
且未启用 Webhook
=> 手动触发
```

用户关注信息：

- 标题。
- 当前状态。
- 来源提示，来自现有 `action_type/action_key` 或创建入口推导，不新增数据库字段。
- 最近执行结果。
- 最近更新时间。
- 所属工作空间。

可执行操作：

```text
执行一次
设为时间驱动
设为事件驱动
归档
复制
移动工作空间
```

斜杠命令绑定：

- 工作空间可配置斜杠命令（`workspace_slash_commands` 表，`command_type='todo'` 绑定 `todo_id`），用户在飞书发送 `/xxx` 即可触发该 todo 执行一次。
- 被命令绑定的 todo 仍属于手动触发：斜杠命令是执行入口（人主动发起的一次性动作），不是持续驱动。
- 卡片上标记绑定的命令名（如 `绑定命令: /todo`），手动触发 Tab 内提供“仅看可命令触发”筛选项。
- 数据上需聚合 `workspace_slash_commands.todo_id` 引用（与 `used_by_loop_step_count` 同构），第一版可先做单条查询，列表批量聚合延后。

边界说明：

- 自动派生的一次性事项（如评审实例 `todo_type=2`、黑板分析 todo）第一版也归入手动触发，后续若数量显著再细分。

## 时间驱动

时间驱动用于承载存在周期调度配置的事项。

关键规则：

```text
scheduler_config 非空 => 时间驱动
scheduler_config 为空 => 不是时间驱动
```

`scheduler_enabled` 不决定是否属于时间驱动，只表示这个时间驱动当前启用还是暂停。

判断规则：

```text
未归档
且未被 Loop 引用
且 scheduler_config 非空
=> 时间驱动
```

典型来源：

- 周期自动化评审。
- 周期黑板分析。
- 定时巡检。
- 定时生成报告。

状态解释：

```text
scheduler_config 非空 且 scheduler_enabled = true
=> 时间驱动，已启用

scheduler_config 非空 且 scheduler_enabled = false
=> 时间驱动，已暂停

scheduler_config 为空
=> 不是时间驱动
```

用户关注信息：

- 周期调度是否启用。
- 调度表达式。
- 下次运行时间。
- 最近一次运行是否成功。
- 连续失败次数。
- 暂停/恢复入口。

可执行操作：

```text
暂停时间驱动
恢复时间驱动
取消时间驱动
执行一次
归档
复制
移动工作空间
```

行为规则：

- `暂停时间驱动`：设置 `scheduler_enabled=false`，保留 `scheduler_config`，仍属于时间驱动。
- `恢复时间驱动`：设置 `scheduler_enabled=true`，保留 `scheduler_config`，重新计算下次运行时间。
- `取消时间驱动`：设置 `scheduler_enabled=false` 且清空 `scheduler_config=null`。如果未被 Loop 引用、未启用 Webhook，则回到手动触发；如果启用 Webhook，则进入事件驱动；如果被 Loop 引用，则进入 Loop 驱动。

## 事件驱动

事件驱动用于承载由外部事件、Webhook、消息、集成等触发的事项。

第一版先以现有 `webhook_enabled` 表达事件驱动。后续如果有更多事件来源，可以继续复用“事件驱动”这个一级分类，不需要再拆出更多顶层入口。

判断规则：

```text
未归档
且未被 Loop 引用
且没有周期调度配置
且 webhook_enabled = true
=> 事件驱动
```

典型来源：

- Webhook 触发事项。
- 外部系统回调触发事项。
- 未来的集成事件触发事项。

用户关注信息：

- 事件入口是否启用。
- Webhook 或事件触发地址。
- 最近一次触发时间。
- 最近一次执行结果。
- 连续失败次数。

可执行操作：

```text
关闭事件驱动
执行一次
归档
复制
移动工作空间
```

行为规则：

- `关闭事件驱动`：设置 `webhook_enabled=false`。如果没有周期调度配置、未被 Loop 引用，则回到手动触发；如果存在周期调度配置，则进入时间驱动；如果被 Loop 引用，则进入 Loop 驱动。
- 如果一个事项同时存在 `scheduler_config` 和 `webhook_enabled=true`，默认归入时间驱动，并在卡片上标记“同时支持事件触发”。

## Loop 驱动

Loop 驱动用于展示被 Loop 配置引用的事项。它不是用户手动调整出来的分类，而是由系统根据 Loop 配置关系自动计算。

判断规则：

```text
未归档
且 used_by_loop_step_count > 0
=> Loop 驱动
```

关键限制：

```text
事项不能通过“调整分类”手动转为 Loop 驱动。
只有当用户在 Loop 配置中引用了该事项，该事项才自然进入 Loop 驱动分类。
当用户在 Loop 配置中移除引用，该事项才自然离开 Loop 驱动分类。
```

用户关注信息：

- 所属 Loop。
- 被引用次数。
- 在 Loop 中的环节位置。
- 最近一次关联 Loop 执行结果。
- 是否同时存在时间驱动或事件驱动。

可执行操作：

```text
查看所属 Loop
查看执行记录
执行一次
暂停/恢复时间驱动
关闭事件驱动
归档
```

不提供以下操作：

```text
转为 Loop 驱动
移出 Loop
```

原因：

- 是否属于 Loop 驱动由 Loop 配置决定。
- “移出 Loop”应该回到 Loop 编辑页完成，避免事项页绕过流程结构。
- 如果 Loop 驱动事项本身还有时间或事件触发，只允许调整这些触发配置，不允许在事项中心调整 Loop 归属。

### 与 `kind` 字段的关系

todos 表有一个 `kind` 列区分 `'item'`（一次性事项）与 `'step'`（可复用环节），它是“事项 vs 环节”的语义区分器，但目前不暴露给前端 API。

本方案以“是否被 Loop 引用”（`used_by_loop_step_count > 0`）而非 `kind='step'` 来定义 Loop 驱动，原因是现状代码里 `create_loop_step` 并不强制要求 `kind='step'`，任何未删除的 todo 都可能被 Loop 引用。因此“Loop 驱动”按引用事实计算更贴合真实状态。

边界：

- 一个 `kind='step'` 但尚未被任何 Loop 引用的环节，第一版不会自动进入 Loop 驱动。
- 后续若希望“环节库”独立可见，应暴露 `kind` 并作为辅助筛选，而不是改变 Loop 驱动的定义。

## 已归档

已归档用于隐藏不再日常关注的事项，但不删除数据。

判断规则：

```text
archived_at 不为空
=> 已归档
```

归档语义：

- 从默认日常视图隐藏。
- 保留事项详情。
- 保留执行记录。
- 保留黑板结论。
- 保留 Loop 引用关系。
- 可恢复，恢复后按当前真实关系重新计算分类。

可执行操作：

```text
恢复
查看详情
查看执行记录
删除
```

恢复规则：

- 清空 `archived_at`。
- 按当前真实关系重新计算分类。
- 如果仍被 Loop 引用，恢复到 Loop 驱动。
- 如果存在周期调度配置，恢复到时间驱动。
- 如果启用 Webhook，恢复到事件驱动。
- 否则恢复到手动触发。

## 分类优先级

同一个事项可能同时满足多个条件，例如既有周期调度配置，又启用了 Webhook，还被 Loop 引用。为了避免默认视图重复出现，主分类使用固定优先级。

```text
已归档 > Loop 驱动 > 时间驱动 > 事件驱动 > 手动触发
```

解释：

- 已归档优先级最高，因为归档代表用户明确希望日常隐藏。
- Loop 驱动优先于时间驱动和事件驱动，因为 Loop 引用代表它已经成为流程结构的一部分。
- 时间驱动优先于事件驱动，因为时间驱动需要展示暂停/恢复、下次运行等更强的周期管理信息。
- 事件驱动优先于手动触发，因为事件入口会让事项被外部系统自动触发。
- 手动触发是默认兜底分类。

组合标记：

```text
Loop 驱动 · 同时支持时间驱动
Loop 驱动 · 同时支持事件驱动
时间驱动 · 同时支持事件触发
```

默认不让同一个事项重复出现在多个一级 Tab 中，但卡片必须展示它的其他驱动能力，避免信息丢失。

## 数据模型建议

### 确定新增字段

本方案确定只新增一个字段：

```text
todos.archived_at: datetime | null
```

字段语义：

- `archived_at = null` 表示事项未归档，继续参与事项中心的日常分类。
- `archived_at != null` 表示事项已归档，进入“已归档”分类。
- `archived_at` 只表达“用户希望从日常视图隐藏”的时间点，不表达删除、不表达停用、不表达解除 Loop 引用。

本方案不新增以下字段：

```text
item_bucket
item_source
archive_reason
automation_trigger
```

不新增这些字段的原因：

- `item_bucket` 容易和真实状态不一致，分类应由底层事实字段推导。
- `item_source` 暂不需要新增，现有 `action_type/action_key` 已能承载部分动作来源和动作模板语义。
- `archive_reason` 暂不需要，归档先保持为一个低成本隐藏动作。
- `automation_trigger` 暂不需要新增独立字段，驱动方式由 `scheduler_config`、`webhook_enabled`、Loop 引用关系共同推导。

### 复用现有字段

事项中心第一版尽量复用现有字段，通过规则推导分类。

分类推导使用的字段（`archived_at` 为本次新增，其余为现有字段）：

```text
archived_at
scheduler_enabled
scheduler_config
scheduler_next_run_at
webhook_enabled
kind
todo_type
parent_todo_id
review_template_id
workspace_id
tag_ids
used_by_loop_step_count
action_type
action_key
```

字段用途：

- `archived_at` 判断事项是否已归档。
- `scheduler_config` 判断事项是否具备时间驱动能力。
- `scheduler_enabled` 判断时间驱动当前启用还是暂停。
- `scheduler_next_run_at` 用于展示下次运行时间。注意它是运行时由 `compute_next_run(config, timezone)` 计算的字段，并非数据库列，只能用于展示，不能用于 SQL 层的分类过滤。
- `webhook_enabled` 判断事项是否具备事件驱动能力。
- `used_by_loop_step_count` 用于判断事项是否属于 Loop 驱动。注意：该字段并非 Todo 的持久化列，需由 `SELECT COUNT(*) FROM loop_steps WHERE todo_id = ? AND enabled = 1` 聚合计算得到（只统计启用中的步骤，禁用步骤不参与 loop 执行，不计入 Loop 驱动）；列表场景必须用 `GROUP BY todo_id` 批量聚合以避免 N+1。
- `kind` 区分 `'item'` 与 `'step'`，第一版只作为辅助语义，不作为 Loop 驱动的主判断条件。
- `todo_type`、`parent_todo_id`、`review_template_id` 保留现有评审实例等历史语义，不作为本次新增分类字段。
- `workspace_id` 用于工作空间筛选。
- `tag_ids` 用于标签筛选。注意它不是 `todos` 列，而是由 `todo_tags` 关联表组装的虚拟字段。
- `action_type/action_key` 保留现有动作按钮和动作模板定位能力，可用于展示“黑板/标题优化/Prompt 优化”等来源提示，但不作为一级分类的权威字段。

### 分类推导

```text
if archived_at != null:
  archived
else if used_by_loop_step_count > 0:
  loop_driven
else if scheduler_config IS NOT NULL:
  time_driven
else if webhook_enabled == true:
  event_driven
else:
  manual
```

说明：

- `scheduler_config` 非空就是时间驱动，`scheduler_config` 为空就不是时间驱动。
- `scheduler_enabled=false` 不代表取消时间驱动；当 `scheduler_config` 非空时，它表示时间驱动暂停。
- `webhook_enabled=true` 表示事件驱动，但如果同一事项同时有 `scheduler_config`，默认主分类为时间驱动，并在卡片上标记“同时支持事件触发”。
- `used_by_loop_step_count` 与 `scheduler_next_run_at` 均为运行时计算字段。实践上必须在分页前完成最终分桶：可以用 `LEFT JOIN/GROUP BY` 或子查询先算 Loop 引用计数，再按 `computed_bucket` 过滤和分页，避免先分页后分桶导致数量和分页错误。

分类推导原则：

- 分类结果可以由 API 返回为 `computed_bucket`。
- `computed_bucket` 是返回值，不落库。
- 用户操作修改底层事实字段，系统重新计算分类。
- Loop 驱动只由 Loop 引用关系产生，事项页不能手动设置。
- 时间驱动由 `scheduler_config` 产生，事项页通过写入/清空 `scheduler_config` 调整。
- 事件驱动由 `webhook_enabled` 产生，事项页通过开启/关闭 Webhook 调整。
- 已归档只由 `archived_at` 产生。

### 现有 action_type/action_key 的边界

`action_type/action_key` 已经存在，并且有唯一索引语义，用于按动作类型和动作键查找或创建对应 Todo。需要注意该唯一约束是工作空间范围的复合唯一索引 `(action_type, action_key, workspace_id)`，并非全局唯一，因此任何依赖它的查找/创建逻辑都必须带上 `workspace_id`。

本方案对它们的处理是：

- 不删除。
- 不重命名。
- 不迁移成新的来源字段。
- 不把它们作为“手动触发 / 时间驱动 / 事件驱动 / Loop 驱动 / 已归档”的一级分类依据。
- 可在卡片上作为辅助信息展示，例如 `action_type=blackboard` 时显示“黑板”。

## 数据库迁移

```text
ALTER TABLE todos ADD COLUMN archived_at TEXT;
```

建议同时增加索引：

```text
CREATE INDEX IF NOT EXISTS idx_todos_archived_at ON todos(archived_at);
```

如果 SQLite 查询经常按工作空间和归档状态过滤，可以后续再评估复合索引：

```text
CREATE INDEX IF NOT EXISTS idx_todos_workspace_archived ON todos(workspace_id, archived_at);
```

复合索引不作为第一版强制要求，先以实际查询性能决定。

## API 建议

### 查询事项中心

```http
GET /api/todos/center?bucket=manual&workspace_id=1&search=xxx
GET /api/todos/center?bucket=time_driven&workspace_id=1
GET /api/todos/center?bucket=event_driven&workspace_id=1
GET /api/todos/center?bucket=loop_driven&workspace_id=1
GET /api/todos/center?bucket=archived&workspace_id=1
```

返回：

```json
{
  "data": [
    {
      "id": 123,
      "title": "修复黑板同步失败",
      "status": "failed",
      "computed_bucket": "manual",
      "action_type": "blackboard",
      "action_key": "wiki-update",
      "workspace_id": 1,
      "scheduler_enabled": false,
      "scheduler_config": null,
      "scheduler_next_run_at": null,
      "webhook_enabled": false,
      "used_by_loop_step_count": 0,
      "last_execution_status": "failed",
      "last_execution_at": "2026-07-08T10:00:00Z",
      "archived_at": null
    }
  ]
}
```

字段来源说明：

- `computed_bucket`、`used_by_loop_step_count` 为运行时计算字段，需在 handler 层补算。
- `last_execution_status`、`last_execution_at` 需 join `execution_records` 取最近一条；现有 `get_todos` 不返回任何执行记录字段，属于本次新增的聚合 DTO。
- `scheduler_config` 需要返回给前端，用于区分“时间驱动已暂停”和“不是时间驱动”。
- `webhook_enabled` 需要返回给前端，用于判断是否具备事件驱动能力。

### 归档事项

```http
POST /api/todos/{id}/archive
```

请求体为空。

行为：

- 将 `archived_at` 设置为当前时间。
- 不修改 `deleted_at`。
- 不修改 `scheduler_enabled`。
- 不修改 `scheduler_config`。
- 不修改 `webhook_enabled`。
- 不修改 Loop 引用关系。
- 返回重新计算后的 `computed_bucket=archived`。

### 恢复事项

```http
POST /api/todos/{id}/restore
```

行为：

- 将 `archived_at` 设置为 `null`。
- 不修改调度配置。
- 不修改 Webhook 配置。
- 不修改 Loop 引用关系。
- 恢复后由系统重新计算 `computed_bucket`。

### 设为时间驱动

复用现有路由：

```http
PUT /api/todos/{id}/scheduler
```

请求：

```json
{
  "scheduler_enabled": true,
  "scheduler_config": "0 9 * * *",
  "scheduler_timezone": "Asia/Shanghai"
}
```

说明：现有 `PUT /api/todos/{id}/scheduler` 已支持调度配置，不必新增 `enable-schedule`。

### 取消时间驱动

复用现有路由：

```http
PUT /api/todos/{id}/scheduler
```

请求：

```json
{
  "scheduler_enabled": false,
  "scheduler_config": null
}
```

行为：

- 关闭调度。
- 清空原调度配置。
- 返回重新计算后的 `computed_bucket`。

实现要求：

- `scheduler_config=null` 必须被解释为显式清空配置。
- 如果当前接口实现把 `null` 和“字段未传”都当作不更新，需要调整请求 DTO 或新增明确的清空逻辑。

### 暂停时间驱动

复用现有路由：

```http
PUT /api/todos/{id}/scheduler
```

请求：

```json
{
  "scheduler_enabled": false
}
```

行为：

- 关闭调度执行。
- 保留原调度配置。
- 因为 `scheduler_config` 仍存在，事项仍属于时间驱动。
- 卡片状态显示为“已暂停”。

### 设为事件驱动

如果现有无专门入口，建议新增扁平具名路由：

```http
PUT /api/todos/{id}/webhook
```

请求：

```json
{
  "webhook_enabled": true
}
```

行为：

- 开启 Webhook 触发。
- 返回重新计算后的 `computed_bucket`。

### 关闭事件驱动

```http
PUT /api/todos/{id}/webhook
```

请求：

```json
{
  "webhook_enabled": false
}
```

行为：

- 关闭 Webhook 触发。
- 返回重新计算后的 `computed_bucket`。

## 页面结构

事项中心页面替代现有密集列表主体，但保留现有详情页和执行记录页。

```text
左侧导航：事项
  ↓
事项中心页面
  顶部工具栏
  分类 Tab
  卡片网格
  右侧/详情页：现有 TodoDetail
```

### 顶部工具栏

包含：

```text
工作空间选择
搜索框
状态筛选
动作类型筛选
新建事项
视图切换
```

默认视图：

```text
卡片视图
```

可选保留：

```text
紧凑列表视图
```

保留紧凑列表的原因：

- 批量操作仍然需要高密度模式。
- 老用户可能习惯快速扫描列表。
- 渐进迁移风险更低。

### 分类 Tab

```text
手动触发
时间驱动
事件驱动
Loop 驱动
已归档
```

每个 Tab 显示数量：

```text
手动触发 12
时间驱动 5
事件驱动 3
Loop 驱动 8
已归档 31
```

### 卡片网格

桌面端：

```text
每行 2 到 3 张卡片
卡片最小宽度 320px
卡片高度保持接近，避免瀑布流造成扫描困难
```

移动端：

```text
单列卡片
主要操作收进更多菜单
点击卡片进入详情
```

## 卡片设计

### 通用卡片信息

每张卡片展示：

```text
标题
状态
驱动类型
来源提示
最近更新时间
最近执行状态
工作空间
标签
主操作按钮
更多菜单
```

主操作按钮：

```text
运行中 => 查看运行
失败 => 查看失败
待执行/已完成 => 执行一次
已归档 => 恢复
```

更多菜单：

```text
调整驱动方式相关操作
复制
移动工作空间
查看执行记录
归档/恢复
```

### 手动触发卡片

重点突出：

```text
标题
来源提示
当前状态
最近执行结果
```

操作：

```text
执行一次
设为时间驱动
设为事件驱动
归档
```

### 时间驱动卡片

重点突出：

```text
时间驱动状态
调度表达式
下次运行
最近运行结果
连续失败次数
是否同时支持事件触发
```

操作：

```text
暂停时间驱动
恢复时间驱动
取消时间驱动
执行一次
归档
```

### 事件驱动卡片

重点突出：

```text
事件入口状态
最近触发时间
最近执行结果
连续失败次数
```

操作：

```text
关闭事件驱动
执行一次
归档
```

### Loop 驱动卡片

重点突出：

```text
所属 Loop
被引用次数
最近关联 Loop 执行状态
是否同时支持时间驱动
是否同时支持事件驱动
```

操作：

```text
查看所属 Loop
查看执行记录
执行一次
归档
```

### 已归档卡片

重点突出：

```text
标题
归档时间
当前推导分类
最近执行记录
是否仍被 Loop 引用
```

操作：

```text
恢复
查看执行记录
删除
```

如果仍被 Loop 引用，应显示醒目标记：

```text
仍被 Loop 引用
```

## 交互细节

### 点击卡片

点击卡片主体进入现有事项详情页。

```text
/#/items?id=123
```

详情页继续展示：

- Prompt。
- 执行记录。
- 执行输出。
- 总结。
- 后置 Todo 进度。

### 调整驱动方式

卡片右上角提供更多菜单。

菜单命名：

```text
设为时间驱动
取消时间驱动
暂停时间驱动
恢复时间驱动
设为事件驱动
关闭事件驱动
归档
恢复
```

不出现：

```text
转为 Loop 驱动
加入 Loop
移出 Loop
```

Loop 相关操作只提供导航：

```text
查看所属 Loop
```

如果用户需要调整 Loop 关系，应进入 Loop 编辑页完成。

### 空状态

```text
暂无手动触发事项
暂无时间驱动事项
暂无事件驱动事项
暂无被 Loop 引用的事项
暂无已归档事项
```

空状态不需要长篇说明，避免页面像帮助文档。

## 与现有页面关系

### 当前 TodoList

当前 `TodoList` 同时承载事项和环路列表，适合作为旧版紧凑视图保留。

改造建议：

1. 新增 `TodoCenterPage` 或 `TodoCardBoard`。
2. 默认在“事项”入口展示卡片式事项中心。
3. 保留旧 `TodoList` 作为“紧凑列表”视图。
4. `Loop` 入口继续使用现有 Loop 页面和 Loop Studio。

### TodoDetail

保持现有详情页，不在第一阶段重写。

原因：

- 用户已经依赖现有执行记录入口。
- 卡片页主要解决浏览和分类问题。
- 详情页重写会显著扩大改造风险。

### Loop 配置

Loop 驱动归属只由 Loop 配置改变。

需要在 Loop 编辑页提供：

```text
添加事项为环节
移除事项引用
查看事项详情
```

事项中心只展示结果，不编辑 Loop 结构。

## 分阶段实施计划

### 阶段一：卡片中心与驱动分桶

目标：

- 新增事项中心卡片页。
- 用现有数据推导五类分类。
- 卡片点击进入现有详情。

范围：

- 前端新增卡片组件。
- 前端新增分类 Tab。
- 前端复用 `scheduler_config`、`scheduler_enabled` 与 `webhook_enabled`。
- 后端新增 `archived_at` 并在列表 DTO 返回。
- 后端新建 Loop 引用计数聚合，列表场景用 `GROUP BY todo_id` 批量返回。
- 后端列表 DTO 扩展 `computed_bucket`，并按需聚合最近一次执行记录（`last_execution_status`/`last_execution_at`）。
- 连续失败次数是更重的聚合（需按时间倒序扫执行记录计连续失败），第一版不在阶段一范围，延后到阶段三/四随时间驱动、事件驱动卡片一起实现。

不做：

- 不重写详情页。
- 不改变 Loop 编辑流程。
- 不新增复杂批量操作。

### 阶段二：归档交互完善

目标：

- 完善归档和恢复交互。
- 补齐归档确认、归档标记、被 Loop 引用时的提示。

范围：

- 前端卡片菜单支持归档/恢复。
- 归档确认弹窗提示 Loop 引用不受影响。
- Loop 详情页标记已归档引用。

注意：

- 归档不删除执行记录。
- 归档不解除 Loop 引用。
- 已归档但仍被 Loop 引用的事项应有明确标记。

### 阶段三：时间驱动操作

目标：

- 支持从手动触发设置为时间驱动。
- 支持暂停、恢复、取消时间驱动。

范围：

- 调度配置弹窗。
- 时间驱动健康信息展示。
- 下次运行时间展示。
- 最近失败信息展示。

### 阶段四：事件驱动操作

目标：

- 支持从手动触发设置为事件驱动。
- 支持关闭事件驱动。
- 事件驱动卡片展示最近触发和执行结果。

范围：

- Webhook 启停入口。
- 事件驱动卡片。
- 最近触发信息展示。

### 阶段五：Loop 驱动增强

目标：

- Loop 驱动卡片展示所属 Loop 和引用信息。
- 从卡片跳转到所属 Loop。

范围：

- 后端返回引用 Loop 摘要。
- 前端展示所属 Loop。
- 支持跳转 Loop 详情。

不做：

- 不在事项中心编辑 Loop 引用。
- 不提供“转为 Loop 驱动”。

## 风险与边界

### 风险一：分类字段与真实状态不一致

如果直接落库 `item_bucket`，可能出现配置已经变化但分类仍显示旧值的问题。

建议：

```text
分类由底层事实字段推导。
API 返回 computed_bucket。
用户操作修改事实字段。
```

### 风险二：分页与分桶顺序错误

Loop 引用计数是运行时聚合字段。如果先对 todos 分页，再补算引用计数和分桶，会导致各 Tab 数量和分页错误。

建议：

- 在分页前完成 Loop 引用计数聚合。
- 在分页前完成 `computed_bucket` 过滤。
- 使用 `LEFT JOIN/GROUP BY` 或子查询避免 N+1。

### 风险三：Loop 驱动归档语义不清

用户可能以为归档会让 Loop 不再使用该事项。

建议：

- 归档弹窗提示“归档不会解除 Loop 引用”。
- Loop 详情中标记已归档引用。
- 解除引用必须到 Loop 编辑页执行。

需要一并修复的现状缺陷：当前删除 todo（`delete_todo` 为软删除）时，handler 只清理 scheduler 和在跑 task，不校验也不清理 `loop_steps.todo_id` 引用。归档/删除链路应统一补一道 `loop_steps` 引用校验，拒绝删除被引用的 todo，或软删时给出明确警告。

### 风险四：卡片页信息过多

卡片如果承载所有操作，会变成另一个拥挤列表。

建议：

- 卡片只放一个主操作。
- 次要操作进入更多菜单。
- 执行记录和复杂配置放到详情页或弹窗。

## 推荐结论

事项中心应以五类驱动视图组织：

```text
手动触发 | 时间驱动 | 事件驱动 | Loop 驱动 | 已归档
```

其中：

- 手动触发是默认兜底分类。
- 时间驱动由 `scheduler_config` 是否为空决定。
- 事件驱动由 `webhook_enabled` 决定。
- Loop 驱动不能通过事项页手动转化，只能由 Loop 配置关系自然产生或消失。
- 已归档可以从任何分类进入，也可以恢复。
- 默认页面采用卡片式大页面，点击卡片进入现有执行记录详情。
- 旧列表作为紧凑视图保留，降低迁移风险。

这套方案既能解决当前列表拥挤的问题，也能把“事项为什么会被执行”表达清楚，从而更贴近自动化事项管理的核心模型。
