# 刷新

## 功能位置

任务（列表） → 顶栏「刷新」按钮（`ReloadOutlined` + loading 态）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U([用户点刷新]) -->|"onClick"| page["TasksPage"]
  page -->|"setRefreshKey(k => k+1)"| key["refreshKey state 自增"]
  key -->|"useEffect deps: reload, refreshKey"| effect["useEffect 触发"]
  effect -->|"reload()"| reload_fn["reload useCallback<br/>deps: wsId"]
  reload_fn -->|"setLoading(true)"| loading["顶栏按钮 loading 态"]
  reload_fn -->|"bundledApi.listTasks(wsId)"| api1["GET /api/v1/workspaces/{ws}/tasks"]
  reload_fn -->|"listLoops(wsId)"| api2["GET /api/v1/workspaces/{ws}/loops"]
  api1 -->|"list_tasks handler"| list_tasks["handlers::tasks::list_tasks"]
  list_tasks -->|"db.list_tasks(ws, status?)<br/>fetch_latest_execution per task"| db1[(tasks/loop_executions 表)]
  api2 -->|"list_loops handler"| list_loops["handlers::loop_::list_loops"]
  list_loops --> db2[(loops 表)]
  reload_fn -->|"setTasks(data)"| state1["tasks state 更新"]
  reload_fn -->|"setLoops(filtered LoopLite[])"| state2["loops state 更新"]
  reload_fn -->|"setLoading(false)"| loading_clear([按钮 loading 解除])
  state1 -->|"timeFilteredTasks useMemo 重算"| views["三态视图 rerender"]
  state2 --> views
```

## 调用关系链路图

```mermaid
flowchart TD
  button["reloadButton<br/>Button size=small<br/>icon: ReloadOutlined<br/>onClick: setRefreshKey(k+1)<br/>loading: loading"] --> key["setRefreshKey(k => k+1)"]
  key --> effect["useEffect deps: [reload, refreshKey]"]
  effect --> reload["reload() useCallback deps: [wsId]"]
  reload --> set_loading["setLoading(true)"]
  reload --> api_tasks["bundledApi.listTasks(wsId)"]
  reload --> api_loops["listLoops(wsId)"]
  api_tasks --> route1["GET /api/v1/workspaces/{ws}/tasks"]
  api_loops --> route2["GET /api/v1/workspaces/{ws}/loops"]
  reload -->|"catch"| on_error["message.error('加载任务失败: ...')<br/>setTasks([])"]
  reload -->|"finally"| set_loading_false["setLoading(false)"]
  api_tasks -->|"then"| set_tasks["setTasks(data)"]
  api_loops -->|"then"| set_loops["setLoops(lpList<br/>.filter(process_template_id != null)<br/>.map(LoopLite))"]
  set_tasks --> memo["useMemo timeFilteredTasks<br/>deps: tasks, hours"]
  memo --> views["三态视图 rerender"]
  set_loops --> modal["CreateTaskModal loops prop 更新"]
```

## 数据结构图

```mermaid
classDiagram
  class TasksPage_RefreshState {
    +tasks: TaskItem[]
    +loading: boolean
    +refreshKey: number
    +loops: LoopLite[]
  }
  class reload {
    <<useCallback deps: wsId>>
    +setLoading(true)
    +bundledApi.listTasks(wsId) → setTasks
    +listLoops(wsId) → setLoops
    +setLoading(false)
  }
  class TaskItem {
    +id: number
    +title: string
    +status: string
    +latest_execution_status?: string
    +latest_execution_requirement?: string
    +created_at?: string
    +template_name?: string
    +template_version?: string
    +complexity?: string
  }
  class LoopLite {
    +id: number
    +name: string
    +process_template_id?: number|null
    +process_template_display_name?: string|null
    +process_template_name?: string|null
    +process_template_version?: string|null
  }
  TasksPage_RefreshState --> reload : refreshKey 触发
  reload --> TaskItem : setTasks
  reload --> LoopLite : setLoops
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 已加载: reload 首次执行（useEffect）
  已加载 --> 刷新中: 用户点刷新按钮
  刷新中 --> 已加载: API 成功 setTasks/setLoops
  刷新中 --> 已加载_空: API 失败 setTasks([])
  note right of 刷新中: refreshKey 自增触发 useEffect\nsetLoading(true) 按钮转 loading\nbundledApi.listTasks + listLoops 并发
  note right of 已加载: setLoading(false)\ntimeFilteredTasks useMemo 重算\n三态视图 rerender\nCreateTaskModal loops 更新
  已加载_空 --> 刷新中: 用户再次点刷新
  已加载_空 --> 已加载: 下次刷新成功
```

## 开发指导

- **前端入口**：`frontend/src/components/tasks/TasksPage.tsx` 的 `reloadButton` 常量（`Button` + `onClick` → `setRefreshKey(k => k+1)`），`reload` `useCallback`（deps `[wsId]`）内并发 `bundledApi.listTasks` 和 `listLoops`；`useEffect` deps `[reload, refreshKey]` 监听刷新
- **后端入口**：`backend/src/handlers/tasks.rs` 的 `list_tasks`（路由 `GET /api/v1/workspaces/{ws}/tasks`）；`backend/src/handlers/loop_.rs` 的 `list_loops`（路由 `GET /api/v1/workspaces/{ws}/loops`）；DAO 层 `backend/src/db/task.rs` 的 `list_tasks`（按 `workspace_id` 过滤 + 可选 `status`，`ORDER BY id DESC`）
- **注意**：`reload` 不依赖 `loading`/`tasks`，避免 `reload` 自身变化触发循环；`refreshKey` 自增是触发刷新的唯一机制（新建任务回调 `handleCreated`、批量删除回调 `onChanged`、再次执行回调 `onTriggered` 都走 `setRefreshKey`）；`reload` 内 `listLoops` 返回的 `LoopListItem` 被裁成 `LoopLite`（只保留 id/name/工艺来源字段），并过滤 `process_template_id` 非空（只有带工艺模板的环路才能创建任务）；API 失败时 `setTasks([])` 清空列表防止残留旧数据
- **扩展**：若需轮询自动刷新，新增 `useEffect` with `setInterval` 调 `setRefreshKey(k => k+1)`，卸载时 `clearInterval`；若需刷新时保留筛选状态，`searchKeyword`/`hours`/`viewMode` 不在 `reload` 内重置，刷新后筛选 useMemo 自动重算
