# 任务（详情）

> 页面级总览。本页各功能点的 4 图 + 开发指导在子文档中维护。

## 页面简介

任务详情态由 URL hash `#/tasks?id=<id>` 驱动：`TasksPage` 读到非 null 的 `selectedTaskId` 即进入详情全屏态，
渲染 `PageCard`（标题动态更新为「任务 #\<id\>: \<title\>」+ `onBack` 返回按钮，062 起返回按钮统一在页头 extra 区最右端）内嵌 `TaskDetailPanel`。
`TaskDetailPanel` 合并了环路详情全部内容（任务与环路是 1:1 关系，`task.loop_id → loop.id`），通过四个 Tab 呈现：
- Tab 1 概览：任务描述 + 环路基本信息（工作空间/待审批）+ 全局限制 + 最新执行进度
- Tab 2 执行环路：来源工艺面包屑 + SVG DAG 流程图（复用 `LoopStepsPanel` → `LoopFlowGraph`）+ 步骤验收标准列表
- Tab 3 执行历史：复用 `LoopExecutionsPanel` 分页执行列表 + `TokenSummaryBar` + `StepExecList` + `BlackboardDrawer`；063 起传 `autoExpandFirstPending`——从任务列表点「待审批」标记进入（URL 带 `?tab=exec`）时，首屏自动展开首条待审批执行，审批按钮一步可见
- Tab 4 讨论（060）：论坛式跟帖，支持 `@专家` / `@执行器` 触发执行并将结论自动回帖；帖子页可从执行明细跳转，返回时经 `?tab=discussion` 恢复选中态

数据获取分两步：先 `bundledApi.getTaskDetail(wsId, taskId)` 拉任务详情（含基本 loop 信息、steps、executions 列表），
拿到 `task.loop_id` 后并行 `dbLoops.getLoop(wsId, loopId)` 拉完整 `LoopDetail`（含 steps、limits_config 等）。
顶部 `DetailHeader` 提供「再次执行」（弹 Modal 输入新需求 → `createTaskExecution`）和「删除环路」（`Popconfirm` → `dbLoops.deleteLoop`）两个操作。
任务标题加载完成后通过 `onTitleReady` 回调让外层 `PageCard` 动态更新标题为「任务 #<id>: <title>」。
DAG 流程图节点上的事项标题可点击，通过 `onOpenTodo(todoId)` 跳转 legacy todo 详情。

## 页面级数据流总图

```mermaid
flowchart LR
  U([用户点击任务行]) -->|"URL hash #/tasks?id=<id>"| page["TasksPage<br/>selectedTaskId != null"]
  page -->|"全屏 PageCard"| panel["TaskDetailPanel<br/>(taskId, workspaceId)"]
  panel -->|"useEffect 初次加载"| get_detail["bundledApi.getTaskDetail(wsId, taskId)"]
  get_detail -->|"GET /api/v1/workspaces/{ws}/tasks/{id}"| get_task_detail["handlers::tasks::get_task_detail"]
  get_task_detail -->|"get_task / get_loop<br/>list_loop_steps_by_loop"| db[(tasks/loops/loop_steps 表)]
  get_task_detail -->|"SELECT loop_executions<br/>WHERE task_id=? LIMIT 20"| loop_exec[(loop_executions 表)]
  get_task_detail -->|"count_pending_approvals_by_execution_ids"| approvals[(loop_step_approval 表)]
  get_task_detail -->|"组装 {task, template, loop, steps, executions}"| panel
  panel -->|"detail.task.loop_id 存在<br/>useEffect 并行拉取"| get_loop["dbLoops.getLoop(wsId, loopId)"]
  get_loop -->|"GET /api/v1/workspaces/{ws}/loops/{id}"| loop_detail["handlers::loop_::get_loop"]
  loop_detail -->|"含 steps/limits_config<br/>abnormal_handler_*"| panel
  panel -->|"onTitleReady(task.title)"| page
  panel -->|"Tab 2 DAG"| dag_tab["DAGTab<br/>LoopStepsPanel → LoopFlowGraph"]
  panel -->|"Tab 3 历史"| exec_tab["ExecHistoryTab<br/>LoopExecutionsPanel autoExpandFirstPending（063）"]
  panel -->|"Tab 4 讨论（060）"| disc_tab["DiscussionTab<br/>task_posts 表<br/>@专家/@执行器触发回帖"]
  panel -->|"再次执行 Modal"| create_exec["bundledApi.createTaskExecution<br/>→ POST /tasks/{id}/executions"]
  panel -->|"删除环路 Popconfirm"| del_loop["dbLoops.deleteLoop<br/>→ DELETE /loops/{id}"]
```
