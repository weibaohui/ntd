# 纪念板 / 看板

> **位置**：Todo 列表 → 顶部「**看板**」按钮 → 默认进入「**结论视图**」
> **前端**：`frontend/src/components/MemorialBoard.tsx`

「纪念板」与「看板」实际是**合并页面**：通过页面顶部的 Segmented 在「结论视图（memorial）」与「看板视图（kanban）」之间切换。默认进入结论视图。

- **结论视图**：把最近完成的 Todo 卡片式陈列，适合做成就回顾、周报展示
- **看板视图**：嵌入 `KanbanBoard` 组件，按状态分列拖拽，详见 [kanban-board.md](kanban-board.md)

## 1. 展示规则

- 结论视图展示**最近 N 小时内有执行记录**（`success` / `failed`）的 Todo
- 数据来源：`db.getRecentCompletedTodos(hours)`（`MemorialBoard.tsx:55`）
- 软删除（`deleted_at != null`）的 Todo **不显示**
- 按完成时间倒序
- 每个 Todo 一张卡片，显示：标题、标签、最后运行时间、执行器、模型、token、运行历史切换

## 2. 视觉

- 卡片大小自适应内容
- 顶部彩色边：成功 `#22c55e`、失败 `#ef4444`
- 无动效（静态展示）
- 响应式列数：≥1600px 显示 4 列，≥1100px 3 列，≥769px 2 列，否则 1 列

## 3. 工具栏

页面顶部工具栏提供：

- **模式切换（Segmented）**：结论视图 ↔ 看板视图
- **搜索框**：匹配标题 / prompt
- **时间范围（Segmented）**：6h / 12h / 24h / 3d / 7d
- **项目过滤下拉**：按 `workspace` 路径过滤

> 时间窗口来自 `frontend/src/components/MemorialBoard.tsx:19-25`。

## 4. 统计

工具栏右侧汇总区会根据当前模式显示：

- 结论视图：总数 / 成功数 / 失败数
- 看板视图：总数 / 待办 / 进行中 / 已完成 / 失败

## 5. 运行历史切换

每张卡片支持切换最近 N 次运行：

- 0 号位默认使用最近一次执行记录
- 切换其他运行编号按需懒加载 `db.getExecutionRecords(todoId, page+1, 1)`
- 当前选中运行编号 / 总运行数 / 加载中态都通过 `runDataCache` / `loadingRunIndex` 局部维护

## 6. 与 Todo 列表的关系

- 纪念板的数据 = Todo 列表的子集 + execution_records 的轻量快照
- 删 Todo（软删）也会从纪念板消失
- 看板视图与普通 Todo 列表共享同一份 Todo 数据，状态修改实时同步

## 7. 故障排查

### 7.1 纪念板空

- 没有 `success` / `failed` 状态的执行记录
- 或所有 completed 都被软删了
- 或所选时间窗口（默认 24h）内无任何完成事件

### 7.2 项目过滤没生效

- `~/.ntd/data.db` 中 `project_directories` 表为空
- 先到「设置 → 项目目录」添加白名单
