# 事项（列表）

> 页面级总览。本页各功能点的 4 图 + 开发指导在子文档中维护。

## 页面简介

事项列表页（URL `/#/todos`，组件 `TodoListPage`）是事项命名空间的列表态入口。它承担两类形态的切换与数据呈现：默认「卡片视图」渲染 `TodoCenterCardView`（按 `computed_bucket` 五类驱动分墙展示聚合卡片），用户切到「列表视图」后渲染 `PageCard + TodoListView`（Ant Design Table 单栏宽屏）。顶部 header 统一提供搜索框、刷新按钮、卡片/列表形态 `Segmented`、新建按钮。列表形态下数据由 `useTodoListData` hook 按 `selectedWorkspace` 拉取 `db.getTodoCenter`；卡片形态由 `TodoCenterCardView` 内部自管加载，两种形态共用同一搜索词。跨组件刷新通过 `TODO_LIST_REFRESH_EVENT` custom event 触发（TodoDrawer 保存、QuickCapture 创建后派发）。单行操作（删除/执行/带参执行）由 `useTodoRowActions` hook 提供，避免主函数膨胀。

## 页面级数据流总图

```mermaid
flowchart LR
  U[用户进入事项列表页] --> Page["TodoListPage (useTodoListData + viewMode)"]
  Page -->|viewMode=list| FE["db.getTodoCenter(ws)"]
  Page -->|viewMode=card| Card["TodoCenterCardView.reload"]
  FE -->|GET /api/v1/workspaces/:ws/todos/center| H1["handlers/todo.rs::get_todo_center_v1"]
  Card -->|GET /api/v1/workspaces/:ws/todos/center| H1
  H1 --> DAO["db/todo.rs::get_todo_center"]
  DAO --> T[(todos 表 — deleted_at IS NULL)]
  DAO --> Agg["build_center_aggregates: loop_count / referencing_loops / last_exec / consecutive_fail / last_webhook / slash_command"]
  DAO --> ITEM["TodoCenterItem[] 返回前端"]
  ITEM --> Filter["filterBySearchKeyword (前端二次过滤 title/prompt)"]
  Filter --> UI["TodoListView (Table) 或 TodoCenterCardView"]
```
