# 查看任务 DAG / 执行历史

## 功能位置

任务（详情） → `TaskDetailPanel` 内 `Tabs`「执行环路」Tab（DAG 流程图 + 验收标准）和「执行历史」Tab（分页执行列表）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U([用户切到执行环路 Tab]) -->|"Tab key: dag"| dag_tab["DAGTab<br/>(loopDetail, steps, onOpenTodo)"]
  dag_tab -->|"loopDetail.steps 存在"| flow["LoopStepsPanel<br/>→ LoopFlowGraph<br/>SVG DAG 流程图"]
  dag_tab -->|"steps.length > 0"| gates["验收标准列表<br/>按 order_index 排序<br/>展示技能/产物/门禁"]
  U2([用户切到执行历史 Tab]) -->|"Tab key: exec"| exec_tab["ExecHistoryTab<br/>(loopId, workspaceId, loopName)"]
  exec_tab -->|"loopId 存在"| exec_panel["LoopExecutionsPanel<br/>(loopId, workspaceId)"]
  exec_panel -->|"分页拉取执行列表"| exec_api["GET /api/v1/workspaces/{ws}/loops/{id}/executions"]
  exec_api --> exec_handler["handlers::loop_::list_executions"]
  exec_handler --> exec_db[(loop_executions 表)]
  exec_db --> exec_panel
  flow -.->|"数据来自 getLoop 拉取的<br/>LoopDetail.steps"| loop_api["dbLoops.getLoop<br/>GET /api/v1/workspaces/{ws}/loops/{id}"]
  gates -.->|"数据来自 getTaskDetail<br/>返回的 steps[]"| detail_api["bundledApi.getTaskDetail<br/>GET /api/v1/workspaces/{ws}/tasks/{id}"]
```

## 调用关系链路图

```mermaid
flowchart TD
  panel["TaskDetailPanel<br/>tabItems[3]"] -->|"key: overview"| overview["OverviewTab"]
  panel -->|"key: dag"| dag["DAGTab<br/>(loopDetail, steps, onOpenTodo)"]
  panel -->|"key: exec"| exec["ExecHistoryTab<br/>(loopId, workspaceId, loopName)"]
  dag --> guard{"loopDetail?.steps?.length > 0?"}
  guard -->|"否"| empty_dag["Empty '暂无执行环路'"]
  guard -->|"是"| steps_panel["LoopStepsPanel<br/>steps = loopDetail.steps<br/>onOpenTodo"]
  steps_panel --> flow_graph["LoopFlowGraph<br/>SVG DAG 渲染"]
  dag --> gates_list["steps.length > 0<br/>验收标准列表<br/>[...steps].sort(order_index)"]
  gates_list --> gate_item["每个 StepInfo:<br/>order_index + name<br/>skill_names: Tag purple<br/>expected_artifacts: Tag blue<br/>gate_config: gateLabel + gateDetailText"]
  exec --> guard2{"loopId > 0?"}
  guard2 -->|"否"| empty_exec["Empty '暂无关联环路'"]
  guard2 -->|"是"| exec_panel["LoopExecutionsPanel<br/>loopId, workspaceId, loopName"]
  exec_panel --> exec_list["分页执行列表<br/>DEFAULT_PAGE_LIMIT = 5"]
  exec_panel --> step_exec["StepExecList<br/>BlackboardDrawer<br/>TokenSummaryBar"]
```

## 数据结构图

```mermaid
classDiagram
  class LoopDetail {
    +id: number
    +steps: LoopStepDto[]
    +limits_config: string
    +abnormal_handler_prompt?: string
    +description?: string
  }
  class StepInfo {
    +id: number
    +name: string
    +order_index: number
    +skill_names: string[]
    +expected_artifacts: Array
    +gate_config: GateDefinition[]
  }
  class GateDefinition {
    +type: string
    +name: string
    +artifact?: string
    +script?: string
    +min_score?: number
    +timeout_secs?: number
  }
  class ExecInfo {
    +id: number
    +status: string
    +started_at?: string
    +finished_at?: string
    +total_steps: number
    +completed_steps: number
    +failed_steps: number
    +requirement?: string
    +pending_approval_count?: number
  }
  class LoopExecutionsPanel {
    +loopId: number
    +workspaceId: number|null
    +loopName: string
  }
  LoopDetail --> StepInfo : steps
  LoopDetail --> LoopExecutionsPanel : loopId
  StepInfo --> GateDefinition : gate_config
  LoopExecutionsPanel --> ExecInfo : 分页拉取
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 概览Tab: 默认选中 key=overview
  概览Tab --> 执行环路Tab: 用户切到 key=dag
  概览Tab --> 执行历史Tab: 用户切到 key=exec
  执行环路Tab --> 概览Tab: 用户切回
  执行环路Tab --> 执行历史Tab: 用户切到 key=exec
  执行历史Tab --> 执行环路Tab: 用户切回
  note right of 执行环路Tab: loopDetail 加载中 tabBarExtraContent 显示 Spin\nloopDetail.steps 为空 → Empty\n有 steps → LoopFlowGraph DAG + 验收标准列表
  note right of 执行历史Tab: loopId = 0 → Empty\nloopId > 0 → LoopExecutionsPanel 分页拉取\n每次执行含 StepExecList/TokenSummaryBar/BlackboardDrawer
```

## 开发指导

- **前端入口**：`frontend/src/components/tasks/TaskDetailPanel.tsx` 的 `tabItems` 数组（`DAGTab` 和 `ExecHistoryTab`）；Tab 子组件定义在 `frontend/src/components/tasks/TaskDetailTabs.tsx`，`DAGTab` 内用 `LoopStepsPanel`（`frontend/src/components/LoopStudioStepsPanel.tsx`）渲染 DAG，`ExecHistoryTab` 内用 `LoopExecutionsPanel`（`frontend/src/components/loop-studio/executions/index.tsx`）渲染历史
- **后端入口**：DAG 数据来自 `dbLoops.getLoop`（`GET /api/v1/workspaces/{ws}/loops/{id}`）返回的 `LoopDetail.steps`；验收标准 `steps[]` 来自 `bundledApi.getTaskDetail`（`GET /api/v1/workspaces/{ws}/tasks/{id}`）→ `handlers::tasks::get_task_detail` → `db.list_loop_steps_by_loop`；执行历史走 `LoopExecutionsPanel` 内的分页 API（`GET /api/v1/workspaces/{ws}/loops/{id}/executions`）
- **注意**：DAG Tab 数据有两个来源——流程图用 `loopDetail.steps`（`LoopStepDto[]`，来自 `dbLoops.getLoop`），验收标准用 `detail.steps`（`StepInfo[]`，来自 `getTaskDetail`），两者口径不同不可混用；`loopDetail` 加载中时 `Tabs` 的 `tabBarExtraContent` 显示小 `Spin`；`DAGTab` 在 `loopDetail` 为 null 或 `steps` 为空时显示 `Empty '暂无执行环路'`；`ExecHistoryTab` 在 `loopId` 为 0 时显示 `Empty '暂无关联环路'`；`getTaskDetail` 返回的 `executions` 列表（`ExecInfo[]`，`LIMIT 20`）只用于概览态，执行历史 Tab 走 `LoopExecutionsPanel` 分页拉取
- **扩展**：新增 Tab 时在 `TaskDetailPanel` 的 `tabItems` 数组加项，对应子组件仿照 `DAGTab`/`ExecHistoryTab` 模式拆分到 `TaskDetailTabs.tsx`；新增验收标准展示字段时在 `StepInfo` 接口加字段，`DAGTab` 的 `gate_item` 渲染分支加对应展示
