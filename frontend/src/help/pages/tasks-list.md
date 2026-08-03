# 任务（列表）

> 页面级总览。本页各功能点的 4 图 + 开发指导在子文档中维护。

## 页面简介

任务列表页（`TasksPage`）是「任务」命名空间的列表态入口，由顶栏 `PageCard` 全屏挂载。
顶栏从左到右摆放：搜索框（`searchKeyword`，三态共享）→ 时间窗分段（`hours`，`showAll` 形态，`null` = 不过滤）→ 刷新按钮 → 视图切换 `Segmented`（列表/看板/卡片）→ 新建按钮。
三态视图共享同一份任务数据（`tasks`，由 `reload` 拉取全量），时间过滤与搜索过滤在页级 `useMemo` 统一完成，三态视图 props 零改动即可复用。
视图模式持久化到 `localStorage`（键 `ntd_tasks_view`），时间窗不持久化（管理视角默认收窄会让老任务消失）。
点击任务行通过 `useViewState.pushUrl('tasks', { id })` 写入 URL hash `#/tasks?id=<id>`，SPA 内同步 `setSelectedTaskId` 立即进入详情态全屏。
浏览器前进/后退监听 `popstate`，从 URL hash 同步 `selectedTaskId`，保证路由与组件态一致。
工作空间切换时若处于详情态会自动退出回到列表态（详情 id 属于旧工作空间，继续停留无意义）。

## 页面级数据流总图

```mermaid
flowchart LR
  U([用户进入任务页]) --> Page["TasksPage<br/>(workspaceId)"]
  Page -->|"reload() 初次/手动刷新"| listTasks["bundledApi.listTasks(wsId)"]
  Page -->|"顺便拉环路下拉"| listLoops["listLoops(wsId)"]
  listTasks -->|"GET /api/v1/workspaces/{ws}/tasks"| list_tasks["handlers::tasks::list_tasks"]
  list_tasks -->|"按 workspace_id 过滤<br/>可选 status 过滤"| db_list["db::task::list_tasks"]
  db_list -->|"SELECT * FROM tasks<br/>WHERE workspace_id=?<br/>ORDER BY id DESC"| tasks_tbl[(tasks 表)]
  list_tasks -->|"批量取模板/环路<br/>fetch_latest_execution"| loop_exec_tbl[(loop_executions 表)]
  loop_exec_tbl -->|"trigger_meta.requirement<br/>started_at 倒序取首行"| list_tasks
  list_tasks -->|"组装 TaskItem[]<br/>含 template_name/version/complexity<br/>latest_execution_status/requirement"| Page
  listLoops -->|"GET /api/v1/workspaces/{ws}/loops"| list_loops["handlers::loop_::list_loops"]
  list_loops --> loops_tbl[(loops 表)]
  list_loops -->|"过滤 process_template_id 非空<br/>映射 LoopLite[]"| Page
  Page -->|"页级 useMemo timeFilteredTasks<br/>按 created_at 过滤 hours"| filtered["timeFilteredTasks"]
  Page -->|"searchKeyword 三态共享<br/>Table/卡片在前端 filter<br/>看板不做 keyword filter"| filtered
  filtered -->|"viewMode=list"| view_table["TasksTableView"]
  filtered -->|"viewMode=kanban"| view_kanban["TasksKanbanView"]
  filtered -->|"viewMode=card"| view_card["TasksCardView"]
  Page -->|"viewMode 持久化"| ls([localStorage<br/>ntd_tasks_view])
  Page -->|"handleSelectTask(id)"| hash["useViewState.pushUrl<br/>/#/tasks?id=<id>"]
  hash -->|"popstate 监听<br/>setSelectedTaskId"| Page
```
