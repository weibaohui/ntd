# 正在运行 Tab 停停任务

## 功能位置
执行器页 →「正在运行」Tab →「批量停止 (N)」按钮 / 行内「停止」`Popconfirm`

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  RunningTab["正在运行 Tab"] -->|"useEffect 10s"| loadRunningRecords["loadRunningRecords()<br>db.getRunningExecutionRecords"]
  loadRunningRecords -->|"GET"| API1["GET /api/v1/workspaces/{ws}/executions/running"]
  User["选择记录"] --> selectedRecordIds["rowSelection"]
  User["点击批停"] --> handleBatchStop["handleBatchStop"]
  handleBatchStop -->|"Promise.allSettled<br>遍历 selectedRecordIds"| API2["POST /api/v1/workspaces/{ws}/executions/{id}/force-fail"]
  User["点击行停"] --> Popconfirm["Popconfirm 确认"]
  Popconfirm -->|"onConfirm"| API3["POST /api/v1/workspaces/{ws}/executions/{id}/force-fail"]
  API2 --> loadRunningRecords
  API3 --> loadRunningRecords
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExecutorsPanel["ExecutorsPanel.tsx<br>ExecutorsPanel()"] --> useEffectRunning["useEffect<br>runningTab=running 时 10s 定时"]
  useEffectRunning --> loadRunningRecords["loadRunningRecords()<br>db.getRunningExecutionRecords"]
  ExecutorsPanel --> handleBatchStop["handleBatchStop"]
  handleBatchStop --> setStoppingRecords["setStoppingRecords(true)"]
  handleBatchStop --> PromiseAllSettled["Promise.allSettled<br>遍历 selectedRecordIds"]
  PromiseAllSettled --> db1["db.forceFailExecution(ws, recordId)"]
  handleBatchStop --> messageReport["message.success/error<br>successCount/failCount"]
  handleBatchStop --> setSelectedRecordIds["setSelectedRecordIds([])"]
  handleBatchStop --> loadRunningRecords
  ExecutorsPanel --> Table["Table 行操作列"]
  Table --> Popconfirm["Popconfirm 确认停止"]
  Popconfirm --> db2["db.forceFailExecution(ws, record.id)"]
```

## 数据结构图

```mermaid
classDiagram
  class ExecutionRecord {
    id: number
    todo_id: number
    status: string
    executor: string|null
    trigger_type: string
    started_at: string
    finished_at: string|null
  }
  class StopResult {
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: runningTab != running
  Idle --> LoadingRecords: runningTab = running
  LoadingRecords --> Polling: 10s 定时启动
  Polling --> Polling: 定时刷新
  Polling --> Selected: rowSelection 勾选
  Selected --> Stopping: 批量停止
  Stopping --> Polling: 成功 loadRunningRecords
  Polling --> Stopping: 行 Popconfirm 确认
  Stopping --> Polling: 成功 loadRunningRecords
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExecutorsPanel.tsx` 的 `loadRunningRecords`、`handleBatchStop` 回调；行操作列的 `Popconfirm onConfirm`
- **后端入口**：`backend/src/handlers/execution.rs` 处理 `GET /api/v1/workspaces/{ws}/executions/running`、`POST /api/v1/workspaces/{ws}/executions/{id}/force-fail`
- **注意**：工作空间 ID 取 `state.selectedWorkspace ?? 0`；批停用 `Promise.allSettled` 统计成功/失败数，不因部分失败而中断
- **扩展**：如需停停时附带原因（如「用户手动停」），后端 force-fail handler 需接收 `reason` 字段并写入 execution record
