# 返回列表

## 功能位置

任务（详情） → `PageCard` 页头 extra 区最右端的「返回列表」按钮（`ArrowLeftOutlined`）。062 起返回按钮统一走 `PageCard` 的 `onBack` prop（固定右上角锡点，各详情页不再手写 `titleSuffix` 按钮）；`TasksPage` 内嵌态与独立路由 `TaskDetailPage` 均同。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U([用户点返回列表]) -->|"onClick"| button["返回按钮<br/>ArrowLeftOutlined"]
  button -->|"TasksPage 内嵌态"| handle_null["handleSelectTask(null)"]
  button -->|"TaskDetailPage 独立路由态"| on_back["onBack()"]
  handle_null --> replace_url["useViewState.replaceUrl('tasks', {})"]
  replace_url -->|"URL hash 清掉 id<br/>#/tasks（无 ?id=）"| url_state["URL hash 更新"]
  handle_null --> set_null["setSelectedTaskId(null)"]
  url_state --> popstate["popstate 监听同步"]
  set_null --> render["TasksPage rerender<br/>isDetail = false → 列表态渲染"]
  popstate --> render
  on_back -.->|"推荐 history.back()<br/>保留列表态筛选/滚动位置"| hist([浏览器历史栈])
```

## 调用关系链路图

```mermaid
flowchart TD
  is_detail["TasksPage<br/>isDetail = selectedTaskId != null"] -->|"true"| detail_suffix["detailTitleSuffix<br/>Button onClick: handleSelectTask(null)"]
  is_detail -->|"false（独立路由 TaskDetailPage）"| indep["TaskDetailPage<br/>PageCard titleSuffix Button<br/>onClick: onBack()"]
  detail_suffix --> handle["handleSelectTask(null)<br/>deps: pushUrl, replaceUrl"]
  handle -->|"taskId == null"| replace["replaceUrl('tasks', {})"]
  replace -->|"useViewState.replaceUrl<br/>清 URL hash id 参数"| url_clear["URL hash → #/tasks"]
  handle --> set_null["setSelectedTaskId(null)"]
  url_clear --> popstate_listen["popstate useEffect<br/>setSelectedTaskId(readSelectedTaskId())"]
  set_null --> rerender["React rerender<br/>isDetail = false"]
  popstate_listen --> rerender
  rerender --> list_view["列表态渲染<br/>TasksTableView/kanban/card"]
  indep --> on_back["onBack() 宿主注入<br/>推荐 history.back()"]
  on_back --> list_view2["宿主切回列表态"]
```

## 数据结构图

```mermaid
classDiagram
  class TasksPage_DetailState {
    +selectedTaskId: number|null
    +isDetail: boolean
  }
  class handleSelectTask {
    <<useCallback deps: pushUrl, replaceUrl>
    +taskId: number|null → void
  }
  class TaskDetailPageProps {
    +taskId: number
    +onBack: () => void
    +onSelectTodo?: function
    +onLoopChanged?: function
  }
  class useViewState {
    +pushUrl(view, opts)
    +replaceUrl(view, opts)
  }
  TasksPage_DetailState --> handleSelectTask : 返回调 null
  handleSelectTask --> useViewState : replaceUrl
  TaskDetailPageProps --> TasksPage_DetailState : onBack 切回列表态
  note for handleSelectTask "返回列表用 replaceUrl 避免详情页占历史栈\n点击任务用 pushUrl 进入详情"
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 列表态: selectedTaskId = null
  列表态 --> 详情态: 点击任务行 handleSelectTask(id)
  详情态 --> 列表态: 点返回按钮 handleSelectTask(null)
  note right of 详情态: URL hash = #/tasks?id=<id>\npushUrl 占历史栈
  note right of 列表态: URL hash = #/tasks（无 id）\nreplaceUrl 清 id 不占历史栈\nselectedTaskId = null → isDetail = false
  列表态 --> [*]: 三态视图渲染（viewMode 决定）
  详情态 --> 列表态: 工作空间切换 prevWsRef 变化<br/>自动 handleSelectTask(null)
```

## 开发指导

- **前端入口**：062 起返回按钮由 `frontend/src/components/common/PageCard.tsx` 统一渲染（传 `onBack` 即在 extra 区最右端出现，文案默认「返回列表」可用 `backLabel` 覆盖）。`TasksPage.tsx` 详情态传 `onBack={() => handleSelectTask(null)}`，`handleSelectTask` `useCallback`（deps `[pushUrl, replaceUrl]`）；独立路由态 `TaskDetailPage.tsx` 将宿主注入的 `onBack` 直接透传给 `PageCard`
- **后端入口**：无后端调用。返回列表是纯前端路由态切换，不触发任何 API
- **注意**：返回列表用 `replaceUrl('tasks', {})` 而非 `pushUrl`，清掉 URL hash id 参数且不占历史栈（点任务进详情才用 `pushUrl`）；`setSelectedTaskId(null)` 手动同步确保 SPA 内点击立即响应，`popstate` 监听是浏览器前进/后退时的兜底同步；工作空间切换时若处于详情态会自动 `handleSelectTask(null)`（`prevWsRef` 比较，详情 id 属于旧工作空间继续停留无意义）；独立路由态 `onBack` 推荐宿主注入 `history.back()`，保留列表态筛选/滚动位置
- **扩展**：若需返回时保留详情态的某个 scroll 位置，在 `handleSelectTask(null)` 前用 ref 记录并通过 `replaceUrl` 的 `opts` 透传；新增其他退出详情态的入口（如键盘 ESC）时复用 `handleSelectTask(null)` 硑
