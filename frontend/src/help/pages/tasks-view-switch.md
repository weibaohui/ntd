# 视图切换（列表/看板/卡片）

## 功能位置

任务（列表） → 顶栏 `Segmented` 视图切换控件（列表 `UnorderedListOutlined` / 看板 `AppstoreOutlined` / 卡片 `LayoutOutlined`）

> 063 起三态视图的待审批透出差异：**看板**为 5 列泳道（首列「待审批」虚拟泳道，`pending_approval_count > 0` 的任务只进该列）；**列表**为独立「待审批」可排序列；**卡片**为头部红色「N 待审批」标记。列表/卡片的状态筛选下拉均含「待审批」虚拟项（筛选项与过滤谓词收口在 `constants.tsx` 的 `TASK_STATUS_FILTER_OPTIONS` + `matchesTaskStatusFilter`，两视图共享唯一事实源）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U([用户点 Segmented 选项]) -->|"onChange v as TasksViewMode"| page["TasksPage"]
  page -->|"handleViewChange(mode)"| set_state["setViewMode(mode)"]
  set_state -->|"persistView(mode)"| ls([localStorage<br/>键: ntd_tasks_view])
  page -->|"viewMode === 'list'"| render_table["渲染 TasksTableView<br/>传 timeFilteredTasks + searchKeyword"]
  page -->|"viewMode === 'kanban'"| render_kanban["渲染 TasksKanbanView<br/>传 timeFilteredTasks"]
  page -->|"viewMode === 'card'"| render_card["渲染 TasksCardView<br/>传 timeFilteredTasks + searchKeyword"]
  render_table -.->|"数据已由 reload 全量拉取<br/>切换不触发后端请求"| api["bundledApi.listTasks<br/>仅在 reload 时调用"]
  render_kanban -.-> api
  render_card -.-> api
```

## 调用关系链路图

```mermaid
flowchart TD
  init["TasksPage useState<br/>viewMode = readInitialView()"] -->|"首次 render"| ls_read["readInitialView()<br/>localStorage.getItem(ntd_tasks_view)"]
  ls_read -->|"合法值 list/kanban/card"| default["使用持久化值"]
  ls_read -->|"无值/非法"| fallback["回退 'list'"]
  segmented["顶栏 Segmented<br/>options: list/kanban/card<br/>value: viewMode"] -->|"onChange"| handle["handleViewChange(mode)"]
  handle --> set_view["setViewMode(mode)"]
  handle --> persist["persistView(mode)<br/>localStorage.setItem"]
  set_view -->|"React rerender"| dispatch{"viewMode?"}
  dispatch -->|"list"| view_table["TasksTableView<br/>(tasks, loading, searchKeyword,<br/>workspaceId, selectedTaskId,<br/>onSelectTask, onChanged)"]
  dispatch -->|"kanban"| view_kanban["TasksKanbanView<br/>(tasks, loading,<br/>workspaceId, onSelectTask)"]
  dispatch -->|"card"| view_card["TasksCardView<br/>(tasks, loading, searchKeyword,<br/>workspaceId, onSelectTask)"]
```

## 数据结构图

```mermaid
classDiagram
  class TasksViewMode {
    <<enumeration>>
    list
    kanban
    card
  }
  class TASKS_VIEW_STORAGE_KEY {
    = 'ntd_tasks_view'
  }
  class TasksPage_State {
    +viewMode: TasksViewMode
    +tasks: TaskItem[]
    +searchKeyword: string
    +hours: number|null
    +selectedTaskId: number|null
  }
  class TasksTableView {
    +tasks: TaskItem[]
    +searchKeyword: string
    +selectedTaskId: number|null
    +onSelectTask: function
    +onChanged: function
  }
  class TasksKanbanView {
    +tasks: TaskItem[]
    +loading: boolean
    +onSelectTask: function
  }
  class TaskItem_063 {
    +pending_approval_count?: number
  }
  TasksKanbanView --> TaskItem_063 : laneOfTask 待审批优先入首列
  TasksCardView --> TaskItem_063 : 头部红色标记
  TasksTableView --> TaskItem_063 : 独立待审批列
  class TasksCardView {
    +tasks: TaskItem[]
    +searchKeyword: string
    +onSelectTask: function
  }
  TasksPage_State --> TasksViewMode : viewMode 字段
  TasksViewMode --> TASKS_VIEW_STORAGE_KEY : 持久化键
  TasksPage_State --> TasksTableView : list 态
  TasksPage_State --> TasksKanbanView : kanban 态
  TasksPage_State --> TasksCardView : card 态
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> list: 首次进入（无持久化值默认 list）
  list --> kanban: 点看板 Segmented
  list --> card: 点卡片 Segmented
  kanban --> list: 点列表 Segmented
  kanban --> card: 点卡片 Segmented
  card --> list: 点列表 Segmented
  card --> kanban: 点看板 Segmented
  note right of list: localStorage 同步更新\n为 ntd_tasks_view
  note right of kanban: 看板不做 keyword filter\n不传 searchKeyword\n063：5 列泳道，首列待审批
  note right of card: 三态共享同一份 tasks\n切换不触发后端请求
```

## 开发指导

- **前端入口**：`frontend/src/components/tasks/TasksPage.tsx` 的 `TasksPage` 组件；视图切换在 `handleViewChange`（`setViewMode` + `persistView`），`Segmented` 控件定义在 `viewSwitch` 常量；持久化键 `TASKS_VIEW_STORAGE_KEY`（值 `ntd_tasks_view`）定义在 `frontend/src/components/tasks/constants.tsx`
- **后端入口**：无后端调用。视图切换是纯前端态，数据由 `reload`（`bundledApi.listTasks`）在页级全量拉取，三态视图共享同一份 `timeFilteredTasks`
- **注意**：`readInitialView` 只接受 `list`/`kanban`/`card` 三个字面量，`localStorage` 不可用时静默降级回 `'list'`；看板态（`TasksKanbanView`）不接收 `searchKeyword`，不做关键词过滤（设计如此，与需求 031 结论一致）；时间过滤和搜索过滤在页级 `useMemo` 完成，三态视图 props 零改动即可复用过滤结果
- **扩展**：新增第四种视图模式时在 `TasksViewMode` 联合类型加值，`Segmented` 的 `options` 加对应项，`TasksPage` 渲染分发 `if (viewMode === 'xxx')` 分支加新视图组件；`readInitialView` 的合法值白名单同步加新值
