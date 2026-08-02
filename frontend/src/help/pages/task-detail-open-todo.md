# DAG 节点跳转事项

## 功能位置

任务（详情） → 执行环路 Tab → `LoopStepsPanel` → `LoopFlowGraph` DAG 节点上的事项标题可点击 → `onOpenTodo(todoId)` 跳转 legacy todo 详情

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U([用户点击 DAG 节点事项标题]) -->|"onClick todoId"| flow["LoopFlowGraph<br/>节点事项标题"]
  flow -->|"onOpenTodo(todoId)"| steps_panel["LoopStepsPanel<br/>onOpenTodo prop"]
  steps_panel -->|"onOpenTodo prop 透传"| dag_tab["DAGTab<br/>onOpenTodo"]
  dag_tab -->|"onOpenTodo prop 透传"| panel["TaskDetailPanel<br/>onOpenTodo"]
  panel -->|"onOpenTodo prop 透传"| detail_page["TaskDetailPage<br/>onSelectTodo"]
  detail_page -->|"宿主注入跳转逻辑"| host([宿主 App<br/>切到 todo 详情态])
  host -.->|"legacy todo 系统<br/>可能调 GET /api/v1/todos/{id}"| todo_api["todo 详情 API"]
  todo_api -.-> todo_db[(todos 表)]
  steps_panel -.->|"onOpenTodo 未注入<br/>标题不可点击"| disabled([标题纯文本])
```

## 调用关系链路图

```mermaid
flowchart TD
  flow["LoopFlowGraph<br/>DAG 节点渲染<br/>事项标题 onClick"] -->|"todoId"| panel_prop1["LoopStepsPanel<br/>Props.onOpenTodo?: (todoId) => void"]
  panel_prop1 --> dag_prop["DAGTab<br/>Props.onOpenTodo?: (todoId) => void"]
  dag_prop --> detail_prop["TaskDetailPanel<br/>Props.onOpenTodo?: (todoId) => void"]
  detail_prop --> page_prop["TaskDetailPage<br/>Props.onSelectTodo?: (todoId) => void"]
  page_prop --> host["宿主 App<br/>onSelectTodo 注入<br/>切到 legacy todo 详情态"]
  host --> todo_detail["TodoDetailPage / todo 详情路由<br/>#/todos?id=<todoId>"]
  flow -->|"onOpenTodo === undefined"| no_click["事项标题纯文本<br/>不可点击"]
  detail_prop -->|"内嵌 TasksPage 态<br/>未传 onOpenTodo"| no_click2["事项标题纯文本<br/>不可点击"]
```

## 数据结构图

```mermaid
classDiagram
  class LoopStepsPanel_Props {
    +steps: LoopStepDto[]
    +onOpenTodo?: (todoId: number) => void
  }
  class DAGTab_Props {
    +loopDetail: LoopDetail|null
    +steps: StepInfo[]
    +onOpenTodo?: (todoId: number) => void
  }
  class TaskDetailPanel_Props {
    +taskId: number
    +workspaceId: number
    +onOpenTodo?: (todoId: number) => void
  }
  class TaskDetailPage_Props {
    +taskId: number
    +onBack: () => void
    +onSelectTodo?: (todoId: number) => void
  }
  class LoopStepDto {
    +id: number
    +todo_id?: number
    +name: string
    +order_index: number
  }
  TaskDetailPage_Props --> TaskDetailPanel_Props : onSelectTodo → onOpenTodo
  TaskDetailPanel_Props --> DAGTab_Props : onOpenTodo 透传
  DAGTab_Props --> LoopStepsPanel_Props : onOpenTodo 透传
  LoopStepsPanel_Props --> LoopStepDto : steps 渲染节点
  note for LoopStepsPanel_Props "onOpenTodo 未注入时<br/>节点事项标题不可点击（纯文本）"
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> DAG浏览态: 用户在执行环路 Tab
  DAG浏览态 --> 点击事项标题: onOpenTodo 已注入
  DAG浏览态 --> 无效果: onOpenTodo 未注入（标题纯文本）
  note right of 点击事项标题: todoId 经 LoopFlowGraph<br/>→ LoopStepsPanel → DAGTab<br/>→ TaskDetailPanel → TaskDetailPage<br/>→ 宿主 onSelectTodo
  点击事项标题 --> todo详情态: 宿主切到 legacy todo 详情
  todo详情态 --> DAG浏览态: 用户返回（onBack 链路）
  note right of todo详情态: legacy todo 系统<br/>TodoDetailPage / #/todos?id=<todoId>
```

## 开发指导

- **前端入口**：`frontend/src/components/LoopStudioStepsPanel.tsx` 的 `LoopStepsPanel`（`Props.onOpenTodo?: (todoId: number) => void`，透传给 `LoopFlowGraph`）；调用链透传：`TaskDetailPage.onSelectTodo` → `TaskDetailPanel.onOpenTodo` → `DAGTab.onOpenTodo` → `LoopStepsPanel.onOpenTodo` → `LoopFlowGraph` 节点事项标题 `onClick`
- **后端入口**：跳转事项本身无直接后端调用，由宿主 App 注入 `onSelectTodo` 后切到 legacy todo 详情态（`TodoDetailPage` 或 `#/todos?id=<todoId>`），todo 详情数据由 todo 系统 API 拉取
- **注意**：`onOpenTodo` 是可选 prop，未注入时 DAG 节点事项标题渲染为纯文本不可点击（`LoopFlowGraph` 内部判定）；`TasksPage` 内嵌详情态（`selectedTaskId != null`）渲染 `TaskDetailPanel` 时**未传** `onOpenTodo`，只有独立路由 `TaskDetailPage` 才透传 `onSelectTodo`；todoId 来自 `LoopStepDto.todo_id`，是 legacy todo 系统的事项 id
- **扩展**：若需在事项标题点击时弹出预览而非跳转，在 `LoopFlowGraph` 节点渲染处改为 `onOpenTodo` 回调内调 `Modal`/`Drawer` 而非宿主跳转；若需支持多事项跳转，`onOpenTodo` 签名改为 `(todoIds: number[]) => void`，全链路同步更新
