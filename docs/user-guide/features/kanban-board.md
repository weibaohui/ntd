# 看板（Kanban）

> **位置**：Todo 列表 → 顶部「**看板**」按钮 → 切换到「看板视图」标签
> **前端**：`frontend/src/components/KanbanBoard.tsx`
> **合并入口**：同一页面默认显示「结论视图」，点击 Segmented 切到「看板视图」（见 [memorial-board.md](memorial-board.md)）

按状态分列的拖拽视图。看 Todo 全局状态最快的方式。

## 1. 列

| 列 | 显示的 Todo |
|----|-------------|
| Pending（待办） | 待跑 |
| Running（进行中） | 跑中（含 spinner） |
| Completed（已完成） | 成功 |
| Failed（失败） | 失败 |

> 颜色与中文标签来自 `frontend/src/components/kanban/constants.ts:17-22`（待办 `#3b82f6`、进行中 `#f59e0b`、已完成 `#22c55e`、失败 `#ef4444`）。

列宽自适应，列内可滚动。

## 2. 拖拽改状态

- 把卡片从 A 列拖到 B 列 → Todo 状态直接改
- 前端调用 `db.updateTodo(id, title, prompt, targetStatus, executor)` → `PUT /api/todos/{id}`（`KanbanBoard.tsx:142-148`）
- 拖拽不带任何状态校验，Kanban 直接修改状态，不做特殊处理

## 3. 时间筛选

顶部 Segmented：

- 6h
- 12h
- 24h
- 3d
- 7d

> 时间窗口来源 `frontend/src/components/kanban/constants.ts:3-9`。筛选只对 `completed` / `failed` 状态的 Todo 按 `updated_at` 生效；`pending` / `running` 不受时间过滤影响。

## 4. 项目过滤 & 搜索

顶部工具栏额外提供：

- **项目维度下拉**：按 `workspace` 路径过滤（数据来自 `project_directories` 表）
- **搜索框**：匹配 `title` 或 `prompt`（不区分大小写）

## 5. 移动端 Tab 滑动

- 列数在 `xs:1 / sm:1 / md:2 / lg:2 / xl:3` 之间随屏幕宽度变化
- 列内可横向滚动，避免在窄屏挤压

## 6. 卡片内容

每张卡片显示：

- 标题
- 标签
- 执行器
- 最后运行时间（相对）
- Token（最近一次）
- 可展开 prompt / result；多运行历史可切换

点卡片 → 跳到 Todo 详情。

## 7. 排序

- 列内按 `created_at DESC`（最新在最上）
- 不支持手动排序

## 8. 与 Todo 列表的关系

- 看板和列表是**同一份数据**的不同视图
- 在看板改了状态，列表同步刷新（WebSocket 推送）
- 在列表改了状态，看板同步刷新

## 9. 故障排查

### 9.1 拖拽无反应

- 看 Console，可能有 drag-end 报错
- 刷新页面重试

### 9.2 拖拽不会拒绝

- Kanban 拖拽不做任何特殊状态校验，会直接修改状态
- 想避免误操作可在 TodoDrawer 内手动改

### 9.3 WebSocket 断连后看板不更新

- 看板不会自动重连（前端封装统一管理）
- 刷新页面恢复
