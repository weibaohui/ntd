# 卡片/列表视图切换

## 功能位置

事项列表页 → 顶部 header 的 `Segmented`（`AppstoreOutlined` 卡片 / `UnorderedListOutlined` 列表，`data-testid="todo-center-view-toggle"`）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[点击 Segmented 卡片/列表] --> onViewChange["handleViewChange (setViewMode + localStorage)"]
  onViewChange --> Local["localStorage.setItem('ntd_items_view', mode)"]
  onViewChange --> Render["TodoListPage 按 viewMode 条件渲染"]
  Render -->|viewMode=card| Card["TodoCenterCardView (内部自管加载, 搜索词透传)"]
  Render -->|viewMode=list| Effect["useTodoListData useEffect 触发 reload"]
  Effect --> FE["db.getTodoCenter(ws)"]
  FE -->|GET /api/v1/workspaces/:ws/todos/center| H["handlers/todo.rs::get_todo_center_v1"]
  H --> DAO["db/todo.rs::get_todo_center"]
  DAO --> T[(todos 表 — deleted_at IS NULL, workspace_id 过滤, id DESC)]
  DAO --> Agg["build_center_aggregates 批量补算聚合"]
  DAO --> ITEM["TodoCenterItem[] 返回"]
  ITEM --> UI["TodoListView (Ant Table)"]
```

## 调用关系链路图

```mermaid
flowchart TD
  Seg["Segmented onChange"] --> onViewChange["TodoListPage.handleViewChange"]
  onViewChange --> setViewMode["useState setViewMode"]
  onViewChange --> persist["localStorage.setItem(VIEW_STORAGE_KEY='ntd_items_view')"]
  setViewMode --> Render["主函数条件渲染"]
  Render -->|card| CardView["TodoCenterCardView"]
  Render -->|list| PageCard["PageCard + TodoListView"]
  TodoListPage -->|"useState readInitialView"| init["readInitialView localStorage.getItem"]
  init --> default["默认 'card'"]
```

## 数据结构图

```mermaid
classDiagram
class TodoListPage_state {
  +viewMode: 'card' | 'list'
  +searchKeyword: string
  +refreshKey: number
  +items: TodoCenterItem[]
  +loading: boolean
}
class TodoCenterItem {
  +computed_bucket: ComputedBucket
  +used_by_loop_step_count: number
  +last_execution_status: string
  +last_execution_at: string
  +referencing_loops: LoopRefSummary[]
  +consecutive_failure_count: number
}
class localStorage_ntd_items_view {
  +key: 'ntd_items_view'
  +value: 'card' | 'list'
}
TodoListPage_state --> localStorage_ntd_items_view : 持久化视图偏好
TodoCenterItem ..> todos_table : DAO 组装
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 卡片视图: readInitialView 默认 card
  卡片视图 --> 列表视图: Segmented onClick list + localStorage 持久化
  列表视图 --> 卡片视图: Segmented onClick card + localStorage 持久化
  列表视图 --> 列表视图: useEffect workspace/viewMode 变化触发 reload
  卡片视图 --> 卡片视图: TodoCenterCardView 内部自管刷新
```

## 开发指导

- **前端入口**：`frontend/src/components/todo-list/TodoListPage.tsx` 的 `TodoListPage`（`viewMode` state + `handleViewChange`）与 `frontend/src/components/todo-list/TodoListPageParts.tsx` 的 `TodoListHeader`（`Segmented` 渲染）。
- **后端入口**：`backend/src/handlers/todo.rs::get_todo_center_v1`（L676），DAO 在 `backend/src/db/todo.rs::get_todo_center`（L203）。
- **注意**：列表形态才会在 `useTodoListData` 的 useEffect 中拉数据；卡片形态不触发该 effect，由 `TodoCenterCardView` 内部自管加载与 `TODO_LIST_REFRESH_EVENT` 监听。视图偏好持久化在 `localStorage` 的 `ntd_items_view` 键，读时异常静默降级为默认 `card`。
- **扩展**：新增第三种视图（如「看板」）时，在 `viewMode` 联合类型加值 → `TodoListHeader` 的 `Segmented.options` 增项 → `TodoListPage` 主函数加条件渲染分支 → `readInitialView` 增值映射。
