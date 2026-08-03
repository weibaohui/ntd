# 跳转事项详情

## 功能位置

看板页 → 结论视图卡片内 `TodoCard` 的 `onSelectTodo`（点击事项标题）；环路视图流程图中点击事项标题（`onOpenTodo`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击事项标题] --> HT["handleSelectTodo / handleOpenTodoFromFlow"]
  HT --> DS["dispatch SELECT_TODO"]
  DS --> ST["App state selectedTodoId"]
  HT --> PU["pushUrl todos id=todoId"]
  PU --> VS["useViewState pushUrl"]
  VS --> NAV["URL 切到 /#/todos/{id}"]
  NAV --> APP["App 路由判定 渲染 TodoDetailPage"]
```

## 调用关系链路图

```mermaid
flowchart TD
  TodoCard --> onSelectTodo["onSelectTodo(e)"]
  onSelectTodo --> handleSelectTodo["MemorialBoard.handleSelectTodo"]
  handleSelectTodo --> stopPropagation["e.stopPropagation()"]
  handleSelectTodo --> dispatch["dispatch SELECT_TODO"]
  LoopKanban --> onOpenTodo["onOpenTodo prop"]
  onOpenTodo --> handleOpenTodoFromFlow["handleOpenTodoFromFlow"]
  handleOpenTodoFromFlow --> dispatch2["dispatch SELECT_TODO"]
  handleOpenTodoFromFlow --> pushUrl["pushUrl todos id"]
  dispatch --> selectedTodoId["state.selectedTodoId"]
  pushUrl --> useViewState["useViewState"]
  useViewState --> setActiveView["setActiveView todos"]
  useViewState --> setTodoDetailId["setTodoDetailId"]
```

## 数据结构图

```mermaid
classDiagram
  class Todo {
    +id: number
    +title: string
    +prompt: string_null
    +status: string
    +workspace_id: number_null
  }
  class AppAction {
    +type: SELECT_TODO
    +payload: number
  }
  class NavOpts {
    +id: number
  }
  Todo --> AppAction: todo.id
  handleSelectTodo --> AppAction
  handleOpenTodoFromFlow --> NavOpts
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> BoardView: 当前在看板页
  BoardView --> TodoSelected: 点击事项标题 → dispatch SELECT_TODO
  TodoSelected --> Navigating: pushUrl todos id=todoId
  Navigating --> TodoDetail: URL 切到 /#/todos/{id} → App 渲染详情页
  TodoDetail --> BoardView: history.back 回到看板
  note right of TodoDetail: 用 pushUrl 而非 replaceUrl，支持返回
end note
```

## 开发指导

- **前端入口**：`frontend/src/components/MemorialBoard.tsx` 的 `handleSelectTodo`（结论视图卡片点击）和 `handleOpenTodoFromFlow`（环路视图流程图点击）函数
- **后端入口**：无直接后端调用——跳转仅改 React state 和 URL hash，`TodoDetailPage` 挂载后自行拉取事项详情
- **注意**：`handleSelectTodo` 用 `e.stopPropagation()` 阻止冒泡，避免同时触发卡片展开 `toggleExpand`；`handleOpenTodoFromFlow` 用 `pushUrl`（而非 `replaceUrl`）让 `history.back` 能回到看板页；环路视图的 `onOpenTodo` prop 由 `MemorialBoard` 注入到 `LoopKanban`，再透传到 `LoopFlowGraph` 的步骤节点
- **扩展**：若需在跳转时携带额外上下文（如来源视图模式），在 `pushUrl` 的 `NavOpts` 中追加自定义 query 参数，在 `TodoDetailPage` 中解析并根据来源做差异化展示
