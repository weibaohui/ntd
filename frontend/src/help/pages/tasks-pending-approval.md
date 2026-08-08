# 待审批透出与直达审批

## 功能位置

任务（列表） → 三态视图各有入口：
- **列表态**：独立「待审批」列（可排序），有待审批的任务显示红色「⚠ N 待审批」标记
- **看板态**：首列「待审批」泳道（橙色列头），`pending_approval_count > 0` 的任务卡片只进该列
- **卡片态**：卡片头部状态 Tag 旁的红色「N 待审批」标记

点击任一标记 → 跳转 `#/tasks/<id>?tab=exec` → 详情「执行历史」Tab 自动展开首条待审批执行 → 环节卡片内「通过 / 拒绝」按钮直接可见。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U([用户打开任务页]) --> Page["TasksPage reload()"]
  Page -->|"GET /api/v1/workspaces/{ws}/tasks"| list_tasks["handlers::tasks::list_tasks"]
  list_tasks -->|"db.list_tasks(ws)"| tasks_tbl[(tasks 表)]
  list_tasks -->|"count_pending_approvals_by_task_ids<br/>单条 SQL 按 task 分组统计"| lse[(loop_step_executions 表)]
  lse -->|"JOIN loop_executions ON task_id<br/>approval_status='pending'<br/>OR status='pending_approval'"| list_tasks
  list_tasks -->|"TaskItem[].pending_approval_count<br/>无记录=0"| Page
  Page -->|"三态分发"| views["Table / Kanban / Card"]
  views -->|"isPendingApproval(task)<br/>count>0"| tag["PendingApprovalTag<br/>红色 N 待审批"]
  views -->|"laneOfTask(task)<br/>待审批优先"| lane["看板首列「待审批」泳道"]
  U2([用户点击待审批标记]) -->|"stopPropagation 后<br/>onSelectTask(id, 'exec')"| nav["pushUrl: #/tasks/id?tab=exec"]
  nav --> panel["TaskDetailPanel<br/>Tabs activeKey=exec"]
  panel --> exec_tab["ExecHistoryTab<br/>LoopExecutionsPanel autoExpandFirstPending"]
  exec_tab -->|"items 首条 pending_approval_count>0<br/>autoExpandedRef 守卫仅一次"| expand["handleExpand 自动展开"]
  expand --> approve["StepExecList 审批区<br/>通过 / 拒绝按钮"]
```

## 调用关系链路图

```mermaid
flowchart TD
  subgraph 后端
    lt["list_tasks handler"] --> gp["get_latest_execution_by_task_ids<br/>(既有批量)"]
    lt --> cp["count_pending_approvals_by_task_ids (063 新增)<br/>WHERE task_id IN (...) GROUP BY task_id"]
    cp --> item["build_task_item 注入<br/>pending_approval_count: i32"]
  end
  subgraph 前端共享层
    c1["constants.tsx<br/>PENDING_APPROVAL_LANE = 'pending_approval'<br/>isPendingApproval(task) → boolean<br/>laneOfTask(task) → 泳道 key<br/>PendingApprovalTag 组件<br/>TASK_STATUS_FILTER_OPTIONS<br/>matchesTaskStatusFilter(task, filter)"]
  end
  subgraph 三态视图
    tv["TasksTableView<br/>待审批列 + 筛选"] --> c1
    kv["TasksKanbanView<br/>groupByLane 用 laneOfTask"] --> c1
    cv["TasksCardView<br/>头部标记 + 筛选"] --> c1
  end
  c1 -->|"onApprove 点击"| sel["TasksPage.handleSelectTask(id, 'exec')"]
  sel --> pd["TaskDetailPanel (?tab=exec)"]
  pd --> eht["ExecHistoryTab<br/>autoExpandFirstPending"]
  eht --> lep["LoopExecutionsPanel<br/>autoExpandedRef 守卫"]
  lep -->|"items 到达且含待审批"| he["handleExpand(firstPending.id)"]
```

## 数据结构图

```mermaid
classDiagram
  class TaskItem {
    +id: number
    +status: string
    +pending_approval_count?: number  «063 新增，后端恒 ≥0»
    +latest_execution_status?: string
  }
  class TaskLane {
    +status: string
    +label: string
    +color: string
  }
  class TASK_LANES_063 {
    pending_approval 待审批 #fa8c16 «首列，虚拟泳道»
    pending / running / success / failed
  }
  class LoopExecutionsPanel_Props {
    +autoExpandFirstPending?: boolean  «063 新增，缺省 false»
  }
  TaskItem --> TASK_LANES_063 : laneOfTask 分组依据
  LoopExecutionsPanel_Props --> TaskItem : 同源 pending_approval_count
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 无标记: 任务无待审批环节（count=0）
  [*] --> 待审批: 环节执行暂停在 human_approval 门禁（count>0）
  待审批 --> 泳道迁移: 看板中卡片只进「待审批」列，不占真实状态列
  待审批 --> 直达审批: 点标记 → ?tab=exec → 自动展开 → 通过/拒绝
  直达审批 --> 无标记: 审批落库后 WebSocket 事件刷新列表，count 归零
  note right of 待审批: 统计口径 NTD-004\napproval_status='pending'\nOR status='pending_approval'\n按任务所有执行累加
  note right of 无标记: 列表 Tag 消失\n看板卡片回到真实状态泳道
```

## 开发指导

- **前端入口**：共享层集中在 `frontend/src/components/tasks/constants.tsx`——`PENDING_APPROVAL_LANE`（虚拟泳道/筛选项键）、`isPendingApproval`、`laneOfTask`（待审批优先于真实 status）、`PendingApprovalTag`（红色标记，可点击）、`TASK_STATUS_FILTER_OPTIONS` + `matchesTaskStatusFilter`（列表/卡片筛选唯一事实源）。三态视图只消费不各自实现。
- **后端入口**：`backend/src/handlers/tasks.rs::list_tasks` 在既有批量查询后追加 `db.count_pending_approvals_by_task_ids(&task_ids)`（`backend/src/db/loop_.rs`），单条 `GROUP BY task_id` SQL 零 N+1；`build_task_item` 注入字段。
- **直达链路**：`PendingApprovalTag.onApprove` → `TasksPage.handleSelectTask(id, 'exec')` → URL `?tab=exec`（`TaskDetailPanel` 既有 tab 白名单机制）→ `ExecHistoryTab` 给 `LoopExecutionsPanel` 传 `autoExpandFirstPending` + `key={loopId}`（切任务强制 remount，守卫 ref 不残留）→ 面板内 `autoExpandedRef` 保证自动展开仅首屏触发一次。
- **注意**：统计口径与 NTD-004 一致（`approval_status='pending' OR status='pending_approval'`，两条暂停路径互斥不重复计数）；计数范围是该任务**所有执行**而非最近一次——旧执行滞留的审批不被新执行掩盖；待审批任务在看板只进一个泳道，各列计数之和恒等于任务总数；自动展开只在执行列表第一页内查找（分页 5 条），不在首页时行内红色标记仍可手动展开。
- **扩展**：新增视图形态时消费共享层即可获得待审批能力；若未来任务状态落库（真实 `pending_review` 枚举），只需改后端统计与 `laneOfTask`/`isPendingApproval` 两处判定，视图层零改动。
