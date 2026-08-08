# 返回列表

## 功能位置

事项详情页 → `PageCard` 页头 extra 区最右端的「返回列表」按钮（`ArrowLeftOutlined`）。062 起统一走 `PageCard` 的 `onBack` prop（固定右上角锡点），各详情页不再手写 `titleSuffix` 按钮。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[点击返回列表按钮] --> onBack["TodoDetailPage onBack"]
  onBack --> backToList["App.tsx backToList (useViewState)"]
  backToList -->|"有浏览器历史"| history["history.back() 语义优先恢复列表态"]
  backToList -->|"无历史直达详情"| replace["replaceUrl('todos') → URL /#/todos"]
  replace --> app["useViewState 解析 activeView=todos todoDetailId=null"]
  app --> render["App.tsx 渲染 TodoListPage (列表态)"]
  history --> render
```

## 调用关系链路图

```mermaid
flowchart TD
  Btn["PageCard onBack（extra 最右端，062）"] --> App["App.tsx onBack={() => backToList()}"]
  App --> useViewState["useViewState.backToList"]
  useViewState -->|"todos + todoDetailId + postRecordId"| pushPost["pushUrl('todos', {id}) 返回父详情"]
  useViewState -->|"todos + todoDetailId"| replaceTodos["replaceUrl('todos') 返回列表"]
  replaceTodos --> setActiveView["useViewState activeView='todos' todoDetailId=null"]
  setActiveView --> AppRender["App.tsx 渲染 TodoListPage"]
  TodoDetailPage --> PageCard["PageCard onBack prop 统一渲染返回按钮（062）"]
```

## 数据结构图

```mermaid
classDiagram
class TodoDetailPage_props {
  +todoId: number
  +onBack: () => void
  +onOpenPost: (todoId, recordId) => void
}
class backToList {
  +activeView: View
  +todoDetailId: number
  +postRecordId: number
  +优先 history.back
  +fallback replaceUrl('todos')
}
class useViewState_url {
  +/#/todos/:id: 事项详情态
  +/#/todos/:id/posts/:rid: 帖子详情态
  +/#/todos: 事项列表态
}
TodoDetailPage_props --> backToList : onBack 绑定
backToList --> useViewState_url : 路由切换
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 事项详情态: URL /#/todos/:id todoDetailId 有值
  事项详情态 --> 事项列表态: 点击返回按钮 replaceUrl('todos')
  事项详情态 --> 帖子详情态: onOpenPost pushUrl({id, recordId})
  帖子详情态 --> 事项详情态: backToList pushUrl({id})
  事项列表态 --> 事项列表态: replaceUrl('todos') todoDetailId=null
```

## 开发指导

- **前端入口**：`frontend/src/components/TodoDetailPage.tsx` 的 `TodoDetailPage`（062 起将 `onBack` 传给 `PageCard` 的 `onBack` prop，按钮统一渲染在 extra 区最右端）；`onBack` 由 `frontend/src/App.tsx` 绑定为 `() => backToList()`；实现在 `frontend/src/hooks/useViewState.ts` 的 `backToList`。
- **后端入口**：无。返回列表纯前端路由切换，不调后端。
- **注意**：`backToList` 按 URL 层级 fallback：帖子页 → 父事项详情、事项详情 → 列表。062 起返回按钮与其他操作按钮同处 extra 区且固定最右端（统一样式与位置，不再紧贴标题）。返回后 `TodoListPage` 的 `viewMode` / `searchKeyword` 由各自组件 state 管理，浏览器 history后退可保留列表状态；直接 `replaceUrl` 进入则重置。
- **扩展**：若需返回时传参（如指定选中事项 id），改 `onBack` 为携带参数的回调，在 `backToList` 后 `pushUrl('todos', {selectId})` 并让 `TodoListPage` 读取 query 初始化选中。
