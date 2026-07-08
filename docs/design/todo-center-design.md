# 事项中心改版设计方案

## 背景

当前事项列表把多种性质不同的对象混在同一个列表中展示，包括闪念创建的琐碎事项、黑板生成的问题处理事项、周期自动化事项、自动化评审事项、黑板分析事项，以及被 Loop 引用的事项。

这些对象虽然都可以落到 Todo 数据结构上，但用户管理它们时关注点不同：

- 琐碎事项关注的是“现在要不要处理、是否执行过”。
- 重复事项关注的是“是否还在持续运行、最近是否失败、下次何时运行”。
- Loop 事项关注的是“它被哪个流程引用、在流程中扮演什么环节”。
- 已归档事项关注的是“日常隐藏，但历史记录和引用关系仍可追溯”。

因此，继续使用单一密集列表会带来两个问题：

- 信息密度过高，操作按钮挤在列表行里，浏览和决策成本高。
- 分类维度混杂，用户难以判断一个事项为什么出现在当前位置。

本方案目标是重新设计“事项”页面，将其升级为更适合管理自动化事项的“事项中心”。

## 设计目标

1. 用更符合用户心智的分类替代“收件箱”概念。
2. 支持事项在合适分类之间低成本调整。
3. 明确限制：事项不能手动调整为 Loop 事项，Loop 归属只能由 Loop 配置产生。
4. 用卡片式页面替代当前挤压的列表操作区。
5. 保留现有事项详情页和执行记录页，降低改造范围。
6. 归档不删除数据，不破坏执行记录、黑板结论、Loop 引用等历史关系。

## 一级分类

事项中心采用四个一级分类：

```text
琐碎事项 | 重复事项 | Loop 事项 | 已归档
```

### 琐碎事项

琐碎事项用于承载一次性、临时性、闪念式、问题处理类任务。

典型来源：

- 用户手动创建的普通事项。
- 闪念创建的事项。
- 黑板创建的问题处理事项。
- 一次性的黑板分析任务。
- 一次性的自动化评审结果处理事项。

判断规则：

```text
未归档
且未启用周期调度 且 未启用 Webhook
且未被 Loop 引用
=> 琐碎事项
```

用户关注信息：

- 标题。
- 当前状态。
- 来源提示，来自现有 `action_type/action_key` 或创建入口推导，不新增数据库字段。
- 最近执行结果。
- 最近更新时间。
- 所属工作空间。

### 重复事项

重复事项用于承载周期调度、Webhook 触发等持续运行的自动化事项。

典型来源：

- 周期自动化评审。
- 周期黑板分析。
- 定时巡检。
- Webhook 触发的事项。

判断规则：

```text
未归档
且（启用周期调度 或 启用 Webhook）
且未被 Loop 引用
=> 重复事项
```

用户关注信息：

- 自动化是否启用。
- 最近一次运行是否成功。
- 连续失败次数。
- 下次运行时间。
- 触发方式。
- 暂停/恢复入口。

### Loop 事项

Loop 事项用于展示被 Loop 配置引用的事项。它不是用户手动调整出来的分类，而是由系统根据 Loop 配置关系自动计算。

判断规则：

```text
未归档
且 used_by_loop_step_count > 0
=> Loop 事项
```

关键限制：

```text
事项不能通过“调整分类”手动转为 Loop 事项。
只有当用户在 Loop 配置中引用了该事项，该事项才自然进入 Loop 事项分类。
当用户在 Loop 配置中移除引用，该事项才自然离开 Loop 事项分类。
```

### 与 `kind` 字段的关系

todos 表有一个 `kind` 列区分 `'item'`（一次性事项）与 `'step'`（可复用环节），它是“事项 vs 环节”的语义区分器，但目前不暴露给前端 API。

本方案以“是否被 Loop 引用”（`used_by_loop_step_count > 0`）而非 `kind='step'` 来定义 Loop 事项，原因是现状代码里 `create_loop_step` 并不强制要求 `kind='step'`，任何未删除的 todo 都可能被 Loop 引用。因此“Loop 事项”按引用事实计算更贴合真实状态。

但这带来一个边界：一个 `kind='step'` 但尚未被任何 Loop 引用的环节，按本规则会落到琐碎事项。第一版可接受这种“按事实归类”，后续若希望“环节”语义独立可见，再考虑把 `kind` 暴露给前端并作为辅助筛选。

用户关注信息：

- 所属 Loop。
- 被引用次数。
- 在 Loop 中的环节位置。
- 最近一次关联 Loop 执行结果。
- 是否同时启用了周期调度。

### 已归档

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
- 可恢复到归档前分类。

## 分类优先级

同一个事项可能同时满足多个条件，例如既启用了周期调度，又被 Loop 引用。为了避免默认视图重复出现，主分类使用固定优先级。

```text
已归档 > Loop 事项 > 重复事项 > 琐碎事项
```

解释：

- 已归档优先级最高，因为归档代表用户明确希望日常隐藏。
- Loop 事项优先于重复事项，因为 Loop 引用代表它已成为流程的一部分。
- 重复事项优先于琐碎事项，因为自动化运行状态比普通处理状态更重要。
- 琐碎事项是默认兜底分类。

如果一个 Loop 事项同时启用了调度，卡片上应显示：

```text
Loop 引用 · 同时启用重复
```

重复事项页可以提供筛选项“包含 Loop 引用”，但默认不重复展示 Loop 事项。

## 分类调整规则

事项允许在“琐碎事项、重复事项、已归档”之间调整。

Loop 事项不允许通过事项页手动设置，只能通过 Loop 配置关系自动产生。

### 琐碎事项可执行操作

```text
设为重复事项
归档
执行
复制
移动工作空间
```

`设为重复事项` 行为：

- 打开轻量调度配置弹窗。
- 用户设置周期、时区、触发方式等。
- 保存后启用调度（设置 `scheduler_enabled=true`）。
- 事项自动进入重复事项分类。

注：启用 Webhook（`webhook_enabled=true`）同样会使事项进入重复事项分类，Webhook 的启停走其独立入口，不在此弹窗内。

`归档` 行为：

- 设置 `archived_at`。
- 记录归档前分类为 `trivial`。
- 从琐碎事项视图移入已归档。

### 重复事项可执行操作

```text
暂停重复
恢复重复
取消重复
归档
执行一次
复制
移动工作空间
```

`暂停重复` 行为：

- 将自动触发暂停。
- 保留调度配置。
- 卡片仍留在重复事项分类，状态显示为“已暂停”。

`恢复重复` 行为：

- 恢复自动触发。
- 重新计算下次运行时间。

`取消重复` 行为：

- 关闭周期调度（`scheduler_enabled=false`）。
- 保留历史调度配置以便未来恢复。
- 如果同时未启用 Webhook、且未被 Loop 引用，事项自动回到琐碎事项。
- 如果仍启用 Webhook，事项仍属于重复事项（Webhook 是独立的触发通路，需单独关闭才会离开）。
- 如果被 Loop 引用，事项自动进入 Loop 事项。

`归档` 行为：

- 设置 `archived_at`。
- 记录归档前分类为 `recurring`。
- 从重复事项视图移入已归档。

### Loop 事项可执行操作

```text
查看所属 Loop
查看执行记录
执行一次
暂停重复
恢复重复
归档
```

不提供以下操作：

```text
转为琐碎事项
转为重复事项
转为 Loop 事项
移出 Loop
```

原因：

- 是否属于 Loop 事项由 Loop 配置决定。
- “移出 Loop”应该回到 Loop 编辑页完成，避免事项页绕过流程结构。
- 如果 Loop 事项本身有调度，只允许调整调度状态，不允许调整 Loop 归属。

`归档` 行为需要特殊处理：

- 如果事项仍被 Loop 引用，归档只从普通事项中心日常视图隐藏。
- Loop 详情页仍应能看到该引用，且显示“已归档”标记。
- 执行 Loop 时是否允许执行已归档事项，需要在 Loop 层单独定义策略。

建议策略：

```text
默认允许继续执行已归档但仍被引用的 Loop 事项。
在 Loop 编辑页标记风险，并提供解除引用入口。
```

### 已归档事项可执行操作

```text
恢复
查看详情
查看执行记录
删除
```

`恢复` 行为：

- 清空 `archived_at`。
- 按当前真实关系重新计算分类。
- 如果仍被 Loop 引用，恢复到 Loop 事项。
- 如果仍启用调度，恢复到重复事项。
- 否则恢复到琐碎事项。

归档前分类只作为提示，不作为恢复时的强制分类。

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
- `automation_trigger` 暂不需要新增独立字段，“是否持续自动化”由现有 `scheduler_enabled` 与 `webhook_enabled` 两个字段共同表达（周期调度与 Webhook 是两条独立的触发通路，后者不走调度字段）。

### 复用现有字段

事项中心第一版尽量复用现有字段，通过规则推导分类。

可复用字段：

```text
scheduler_enabled
scheduler_config
scheduler_next_run_at
webhook_enabled
kind
todo_type
parent_todo_id
workspace_id
tag_ids
used_by_loop_step_count
action_type
action_key
```

字段用途：

- `scheduler_enabled` 与 `webhook_enabled` 共同判断事项是否属于重复事项：周期调度走 `scheduler_enabled`，Webhook 触发走独立的 `webhook_enabled`，二者只要其一启用即视为持续自动化。
- `scheduler_config`、`scheduler_next_run_at` 用于展示重复事项的调度信息。注意 `scheduler_next_run_at` 是运行时由 `compute_next_run(config, timezone)` 计算的字段，并非数据库列，只能用于展示，不能用于 SQL 层的分类过滤。
- `webhook_enabled` 判断事项是否启用了 Webhook 触发，是重复事项的第二条触发通路。
- `kind` 区分 `'item'`（一次性事项）与 `'step'`（可复用环节）。它是“事项 vs 环节”的语义区分器，但目前不暴露给前端 API。设计 Loop 事项分类时需明确按“当前被 Loop 引用”还是按“本质是环节（`kind='step'`）”定义，二者不完全等价（详见上文“与 kind 字段的关系”）。
- `used_by_loop_step_count` 用于判断事项是否属于 Loop 事项。注意：该字段**并非 Todo 的持久化列**，目前仅存在于前端 `StepSummary` 类型且为未实现的预留定义，需由 `SELECT COUNT(*) FROM loop_steps WHERE todo_id = ?` 聚合计算得到；列表场景必须用 `GROUP BY todo_id` 批量聚合以避免 N+1。
- `todo_type`、`parent_todo_id`、`review_template_id` 保留现有评审实例等历史语义，不作为本次新增分类字段。
- `workspace_id` 用于工作空间筛选。
- `tag_ids` 用于标签筛选。注意它不是 `todos` 列，而是由 `todo_tags` 关联表组装的虚拟字段。
- `action_type/action_key` 保留现有动作按钮和动作模板定位能力，可用于展示“黑板/标题优化/Prompt 优化”等来源提示，但不作为一级分类的权威字段。

### 分类推导

```text
if archived_at != null:
  archived
else if used_by_loop_step_count > 0:
  loop
else if scheduler_enabled == true OR webhook_enabled == true:
  recurring
else:
  trivial
```

说明：

- `recurring` 的判定必须同时覆盖周期调度（`scheduler_enabled`）与 Webhook（`webhook_enabled`）两条触发通路，否则 Webhook 触发的事项会被错误归入琐碎事项。
- `used_by_loop_step_count` 与 `scheduler_next_run_at` 均为运行时计算字段（前者需聚合 `loop_steps`，后者由调度配置推导），无法在单条 SQL 的 WHERE 中完成全部分类过滤。实践上先按归档状态、`scheduler_enabled`/`webhook_enabled` 在 SQL 层粗筛，再在应用层补算引用计数并最终分桶。

分类推导原则：

- 分类结果可以由 API 返回为 `computed_bucket`。
- `computed_bucket` 是返回值，不落库。
- 用户操作修改底层事实字段，系统重新计算分类。
- Loop 事项只由 Loop 引用关系产生，事项页不能手动设置。
- 重复事项只由调度状态产生，事项页通过启用/取消调度来改变。
- 已归档只由 `archived_at` 产生。

### 现有 action_type/action_key 的边界

`action_type/action_key` 已经存在，并且有唯一索引语义，用于按动作类型和动作键查找或创建对应 Todo。需要注意该唯一约束是**工作空间范围**的复合唯一索引 `(action_type, action_key, workspace_id)`（带 `action_type/action_key IS NOT NULL` 的部分条件），并非全局唯一，因此任何依赖它的查找/创建逻辑都必须带上 `workspace_id`。

本方案对它们的处理是：

- 不删除。
- 不重命名。
- 不迁移成新的来源字段。
- 不把它们作为“琐碎事项 / 重复事项 / Loop 事项 / 已归档”的一级分类依据。
- 可在卡片上作为辅助信息展示，例如 `action_type=blackboard` 时显示“黑板”。

示例：

```text
action_type = blackboard, action_key = wiki-update
=> 卡片辅助显示：黑板
=> 一级分类仍按 archived_at / Loop 引用 / scheduler_enabled 推导
```

这样可以保留现有动作体系，同时避免“来源”和“分类”混在一起。

### 数据库迁移

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
GET /api/todos/center?bucket=trivial&workspace_id=1&search=xxx
```

返回：

```json
{
  "data": [
    {
      "id": 123,
      "title": "修复黑板同步失败",
      "status": "failed",
      "computed_bucket": "trivial",
      "action_type": "blackboard",
      "action_key": "wiki-update",
      "workspace_id": 1,
      "scheduler_enabled": false,
      "webhook_enabled": false,
      "scheduler_next_run_at": null,
      "used_by_loop_step_count": 0,
      "last_execution_status": "failed",
      "last_execution_at": "2026-07-08T10:00:00Z",
      "archived_at": null
    }
  ]
}
```

字段来源说明：

- `computed_bucket`、`used_by_loop_step_count` 为运行时计算字段，需在 handler 层补算（引用计数用 `GROUP BY todo_id` 批量聚合）。
- `last_execution_status`、`last_execution_at` 需 join `execution_records` 取最近一条；现有 `get_todos` 不返回任何执行记录字段，属于本次新增的聚合 DTO。
- `webhook_enabled` 已是持久化列，直接返回。

### 归档事项

```http
POST /api/todos/{id}/archive
```

请求体为空。

行为：

- 将 `archived_at` 设置为当前时间。
- 不修改 `deleted_at`。
- 不修改 `scheduler_enabled`。
- 不修改 Loop 引用关系。
- 返回重新计算后的 `computed_bucket=archived`。

### 恢复事项

```http
POST /api/todos/{id}/restore
```

行为：

- 将 `archived_at` 设置为 `null`。
- 不修改调度配置。
- 不修改 Loop 引用关系。
- 恢复后由系统重新计算 `computed_bucket`。

### 设置为重复事项（启用周期调度）

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

说明：现有 `PUT /api/todos/{id}/scheduler` 已支持调度配置，不必新增 `enable-schedule`。Webhook 触发的重复事项通过 `webhook_enabled` 字段管理（若现有无专门入口，可顺带补一个 `PUT /api/todos/{id}/webhook` 路由，保持扁平具名风格）。

### 取消重复（关闭周期调度）

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

- 关闭调度。
- 保留原调度配置。
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
琐碎事项
重复事项
Loop 事项
已归档
```

每个 Tab 显示数量：

```text
琐碎事项 12
重复事项 5
Loop 事项 8
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
调整分类相关操作
复制
移动工作空间
查看执行记录
归档/恢复
```

### 琐碎事项卡片

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
设为重复事项
归档
```

### 重复事项卡片

重点突出：

```text
自动化状态
触发方式
下次运行
最近运行结果
连续失败次数
```

操作：

```text
暂停重复
恢复重复
取消重复
执行一次
归档
```

### Loop 事项卡片

重点突出：

```text
所属 Loop
被引用次数
最近关联 Loop 执行状态
是否同时启用重复
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
归档前状态
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

### 调整分类

卡片右上角提供更多菜单。

菜单命名避免“转为 Loop”：

```text
设为重复事项
取消重复
暂停重复
恢复重复
归档
恢复
```

不出现：

```text
转为 Loop 事项
加入 Loop
移出 Loop
```

Loop 相关操作只提供导航：

```text
查看所属 Loop
```

如果用户需要调整 Loop 关系，应进入 Loop 编辑页完成。

### 空状态

琐碎事项为空：

```text
暂无琐碎事项
```

重复事项为空：

```text
暂无重复事项
```

Loop 事项为空：

```text
暂无被 Loop 引用的事项
```

已归档为空：

```text
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

Loop 事项归属只由 Loop 配置改变。

需要在 Loop 编辑页提供：

```text
添加事项为环节
移除事项引用
查看事项详情
```

事项中心只展示结果，不编辑 Loop 结构。

## 分阶段实施计划

### 阶段一：设计与静态结构

目标：

- 新增事项中心卡片页。
- 用现有数据推导四类分类。
- 卡片点击进入现有详情。

范围：

- 前端新增卡片组件。
- 前端新增分类 Tab。
- 前端复用 `scheduler_enabled` 与 `webhook_enabled`。
- 后端**新建** Loop 引用计数聚合（`SELECT COUNT(*) FROM loop_steps GROUP BY todo_id` 批量返回），因为该字段目前完全不存在，需从零实现。
- 后端列表 DTO 扩展 `webhook_enabled`、`computed_bucket`，并按需聚合最近一次执行记录（`last_execution_status`/`last_execution_at`，需 join `execution_records`）。

不做：

- 不重写详情页。
- 不改变 Loop 编辑流程。
- 不新增复杂批量操作。

### 阶段二：归档能力

目标：

- 支持归档和恢复。
- 已归档进入独立 Tab。

范围：

- 后端新增 `archived_at`。
- API 支持归档/恢复。
- 前端卡片菜单支持归档/恢复。

注意：

- 归档不删除执行记录。
- 归档不解除 Loop 引用。
- 已归档但仍被 Loop 引用的事项应有明确标记。

### 阶段三：重复事项操作

目标：

- 支持从琐碎事项设置为重复事项。
- 支持暂停、恢复、取消重复。

范围：

- 调度配置弹窗。
- 重复事项健康信息展示。
- 下次运行时间展示。
- 最近失败信息展示。

### 阶段四：Loop 事项增强

目标：

- Loop 事项卡片展示所属 Loop 和引用信息。
- 从卡片跳转到所属 Loop。

范围：

- 后端返回引用 Loop 摘要。
- 前端展示所属 Loop。
- 支持跳转 Loop 详情。

不做：

- 不在事项中心编辑 Loop 引用。
- 不提供“转为 Loop 事项”。

## 风险与边界

### 风险一：分类字段与真实状态不一致

如果直接落库 `item_bucket`，可能出现事项已经关闭调度但仍显示为重复事项的问题。

建议：

```text
分类由底层事实字段推导。
API 返回 computed_bucket。
用户操作修改事实字段。
```

### 风险二：Loop 事项归档语义不清

用户可能以为归档会让 Loop 不再使用该事项。

建议：

- 归档弹窗提示“归档不会解除 Loop 引用”。
- Loop 详情中标记已归档引用。
- 解除引用必须到 Loop 编辑页执行。

需要一并修复的现状缺陷：当前删除 todo（`delete_todo` 为软删除）时，handler 只清理 scheduler 和在跑 task，**完全不校验也不清理 `loop_steps.todo_id` 引用**，被 Loop 引用的 todo 可被静默软删，之后 Loop 仍能被触发并按 id 取到 stale 数据。归档/删除链路应统一补一道 `loop_steps` 引用校验（拒绝删除被引用的 todo，或软删时给出明确警告）。

### 风险三：卡片页信息过多

卡片如果承载所有操作，会变成另一个拥挤列表。

建议：

- 卡片只放一个主操作。
- 次要操作进入更多菜单。
- 执行记录和复杂配置放到详情页或弹窗。

## 推荐结论

事项中心应以四类视图组织：

```text
琐碎事项 | 重复事项 | Loop 事项 | 已归档
```

其中：

- 琐碎事项和重复事项可以通过调度状态（或 Webhook 开关）互转。
- 已归档可以从任何分类进入，也可以恢复。
- Loop 事项不能通过事项页手动转化，只能由 Loop 配置关系自然产生或消失。
- 默认页面采用卡片式大页面，点击卡片进入现有执行记录详情。
- 旧列表作为紧凑视图保留，降低迁移风险。

这套方案既能解决当前列表拥挤的问题，也能避免把 Loop 编排关系误做成普通事项分类，从而保持产品模型清晰。
