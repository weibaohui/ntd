# 看板（Kanban）

> **位置**：Todo 列表 → 顶部「看板」切换
> **前端**：`frontend/src/components/KanbanBoard.tsx`

按状态分列的拖拽视图。看 Todo 全局状态最快的方式。

## 1. 列

| 列 | 显示的 Todo |
|----|-------------|
| Pending | 待跑 |
| Running | 跑中（含 spinner） |
| Completed | 成功 |
| Failed | 失败 |

列宽自适应，列内可滚动。

## 2. 拖拽改状态

- 把卡片从 A 列拖到 B 列 → Todo 状态直接改
- 后端 `PUT /api/todos/{id}/force-status`
- 「Running」状态特殊：从 Running 拖到别的列会触发「停止」

## 3. 时间筛选

顶部下拉：

- 今天
- 本周
- 本月
- 全部

筛选的是 Todo 的 `created_at`。

## 4. Chat 折叠

- 列右上「折叠 Chat」按钮 → 列宽变窄
- 适合大量 Todo 时聚焦状态
- 再次点击展开

## 5. 卡片内容

每张卡片显示：

- 标题
- 标签
- 执行器
- 最后运行时间（相对）
- Token（最近一次）

点卡片 → 跳到 Todo 详情。

## 6. 排序

- 列内按 `created_at DESC`（最新在最上）
- 不支持手动排序

## 7. 与 Todo 列表的关系

- 看板和列表是**同一份数据**的不同视图
- 在看板改了状态，列表同步刷新（WebSocket 推送）
- 在列表改了状态，看板同步刷新

## 8. 故障排查

### 8.1 拖拽无反应

- 看 Console，可能有 drag-end 报错
- 刷新页面重试

### 8.2 拖到 Running 列无反应

- 设计上不允许：Todo 只能从 Running「跑完」自动变 Running，不能直接设
- 从别的列拖到 Running 列会忽略

### 8.3 WebSocket 断连后看板不更新

- 看板不会自动重连（前端封装统一管理）
- 刷新页面恢复
