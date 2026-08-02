# 刷新列表

## 功能位置

事项列表页 → 顶部 header 的「刷新」`Button`（`ReloadOutlined`，`aria-label="刷新"`，`loading` 绑定数据加载态）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[点击刷新按钮] --> onReload["TodoListHeader onReload → handleReload"]
  onReload -->|"viewMode=list"| reload["useTodoListData.reload()"]
  onReload -->|"两形态共用"| refreshKey["setRefreshKey(k => k+1)"]
  reload --> FE["db.getTodoCenter(ws)"]
  FE -->|GET /api/v1/workspaces/:ws/todos/center| H["handlers/todo.rs::get_todo_center_v1"]
  H --> DAO["db/todo.rs::get_todo_center"]
  DAO --> T[(todos 表 — deleted_at IS NULL, workspace_id 过滤, id DESC)]
  DAO --> Agg["build_center_aggregates 批量补算聚合"]
  DAO --> ITEM["TodoCenterItem[] 返回"]
  ITEM --> setItems["setItems(data) setLoading(false)"]
  refreshKey -->|"viewMode=card"| Card["TodoCenterCardView useEffect[reload, refreshKey] 触发重拉"]
```

## 调用关系链路图

```mermaid
flowchart TD
  Btn["TodoListHeader 刷新 Button onClick"] --> handleReload["TodoListPage.handleReload"]
  handleReload -->|"list"| reloadFn["useTodoListData.reload (useCallback)"]
  reloadFn --> setLoading["setLoading(true)"]
  reloadFn --> dbCall["db.getTodoCenter(workspaceId)"]
  reloadFn --> setItems["setItems(data)"]
  reloadFn --> setLoadingFalse["setLoading(false)"]
  handleReload -->|"通用"| key["setRefreshKey(k => k+1)"]
  key --> cardEffect["TodoCenterCardView useEffect[reload, refreshKey]"]
```

## 数据结构图

```mermaid
classDiagram
class useTodoListData {
  +items: TodoCenterItem[]
  +loading: boolean
  +reload: () => Promise void
}
class refreshKey_state {
  +初始: 0
  +每次刷新: 自增
}
class TODO_LIST_REFRESH_EVENT {
  +type: 'ntd:todo-list-refresh'
  +触发方: TodoDrawer onSaved / QuickCapture
  +监听方: useTodoListData useEffect + TodoCenterCardView
}
useTodoListData --> refreshKey_state : 卡片形态由 CardView 自管
useTodoListData --> TODO_LIST_REFRESH_EVENT : 跨组件刷新监听
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 已加载: items 有数据
  已加载 --> 加载中: 点击刷新 / TODO_LIST_REFRESH_EVENT 触发 reload
  加载中 --> 已加载: db.getTodoCenter 返回 setItems
  加载中 --> 加载失败: catch e → message.error + setItems([])
  已加载 --> 已加载: 刷新按钮 loading 态绑定
```

## 开发指导

- **前端入口**：`frontend/src/components/todo-list/TodoListPage.tsx` 的 `handleReload`（L133-136）与 `useTodoListData`（L55-87，`reload` useCallback）；渲染在 `frontend/src/components/todo-list/TodoListPageParts.tsx` 的 `TodoListHeader`（刷新 Button，`loading` 绑定）。
- **后端入口**：`backend/src/handlers/todo.rs::get_todo_center_v1`（L676），DAO 在 `backend/src/db/todo.rs::get_todo_center`（L203）。
- **注意**：列表形态 `reload()` 调 `db.getTodoCenter`；卡片形态不走 `reload()`，靠 `refreshKey` 自增驱动 `TodoCenterCardView` 的 useEffect 重拉。`TODO_LIST_REFRESH_EVENT` custom event 是跨组件刷新通道（TodoDrawer 保存后 `window.dispatchEvent`），列表/卡片两种形态都监听该事件但仅在对应 `viewMode` 下触发，避免卡片形态误拉数据。
- **扩展**：新增「强制清缓存刷新」时，在 `reload` 前清空 `items` state 再拉取；若要给后端加时间戳参数（仅取增量），在 `db.getTodoCenter` 传 `since` 查询参数并让 `get_todo_center_v1` 支持。
