# 切换历史运行记录

## 功能位置

运行中心页 → 结论视图卡片内 `TodoCard` 的运行记录下拉选择器（`onSelectRun`），仅 `boardMode=conclusion` 时可用

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户选择历史运行索引] --> SR["handleSelectRun(todoId, runIndex)"]
  SR --> CK1["selectedRunIndex[todoId] === runIndex?"]
  CK1 -->|是| RET[return 不重复加载]
  CK1 -->|否| SI["setSelectedRunIndex"]
  SR --> CK2["runDataCache[todoId][runIndex] 存在?"]
  CK2 -->|存在| USE[直接用缓存 不发请求]
  CK2 -->|不存在| CK3["runIndex === 0?"]
  CK3 -->|是| L0["从 items 构造 record 塞入 cache"]
  CK3 -->|否| API["db.getExecutionRecords(todoId, runIndex+1, 1, ...)"]
  API --> H["GET /api/v1/workspaces/{ws}/executions?todo_id=N&page=N&limit=1"]
  H --> DAO["db 查询 execution_records 表"]
  DAO --> REC[ExecutionRecord]
  REC --> CACHE["setRunDataCache 塞入缓存"]
```

## 调用关系链路图

```mermaid
flowchart TD
  TodoCard --> onSelectRun["onSelectRun(index)"]
  onSelectRun --> handleSelectRun["OpsCenter.handleSelectRun"]
  handleSelectRun --> setSelectedRunIndex["setSelectedRunIndex"]
  handleSelectRun --> cacheCheck["runDataCache[todoId]?.[runIndex]"]
  cacheCheck -->|有缓存| skip["return 用缓存"]
  cacheCheck -->|无缓存 runIndex=0| fromItem["从 items 构造 record"]
  cacheCheck -->|无缓存 runIndex>0| setLoadingRunIndex["setLoadingRunIndex"]
  setLoadingRunIndex --> db_get["db.getExecutionRecords(todoId, page, 1, ...)"]
  db_get --> api_get["api.get /api/v1/workspaces/{ws}/executions"]
  api_get --> setRunDataCache["setRunDataCache"]
  api_get --> setTotalRunsCache["setTotalRunsCache"]
```

## 数据结构图

```mermaid
classDiagram
  class ExecutionRecord {
    +id: number
    +todo_id: number
    +status: running_success_failed
    +result: string_null
    +usage: ExecutionUsage_null
    +executor: string_null
    +model: string_null
    +trigger_type: string
    +rating: number_null
  }
  class RunHistoryState {
    +selectedRunIndex: Record_number_number
    +totalRunsCache: Record_number_number
    +runDataCache: Record_number_ExecutionRecord_array
    +loadingRunIndex: Record_number_number_null
  }
  class RecentCompletedTodo {
    +todo_id: number
    +record_id: number
    +result: string_null
    +usage: ExecutionUsage_null
    +model: string_null
    +executor: string_null
    +trigger_type: string
    +execution_status: string
    +completed_at: string
    +rating: number_null
  }
  RunHistoryState --> ExecutionRecord
  RecentCompletedTodo --> ExecutionRecord: runIndex=0 时映射
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Run0: selectedRunIndex 默认 0（最近一次）
  Run0 --> Loading: 用户切到 runIndex > 0 且无缓存
  Loading --> Cached: getExecutionRecords 返回 → setRunDataCache
  Cached --> Displaying: 渲染选中 run 的 result/model/usage/rating
  Displaying --> Loading: 切到另一个无缓存 runIndex
  Displaying --> Run0: 切回 runIndex = 0
  Run0 --> Displaying: 从 item 直接取数据无需请求
```

## 开发指导

- **前端入口**：`frontend/src/components/OpsCenter.tsx` 的 `handleSelectRun` 函数和 `selectedRunIndex` / `runDataCache` / `totalRunsCache` / `loadingRunIndex` state
- **后端入口**：`backend/src/handlers/execution.rs` 的 `v1_get_execution_records` handler，查询 `execution_records` 表按 `todo_id` 过滤并分页
- **注意**：`runIndex = 0`（最近一次运行）直接从 `items` 中的 `RecentCompletedTodo` 字段构造 `ExecutionRecord` 塞入缓存，不发后端请求；`runIndex > 0` 时用 `page = runIndex + 1` + `limit = 1` 拉取指定页的单条记录；`totalRunsCache` 在首次加载时通过 `db.getExecutionRecords(todoId, 1, 1)` 获取 `page.total`，后续切 run 不重复拉取；缓存命中时直接用 `runDataCache[todoId][runIndex]` 不发请求
- **扩展**：若需展示运行记录的执行日志，在 `TodoCard` 中追加「查看日志」入口调用 `db.getExecutionLogs`；若需支持批量删除历史运行，在 `TodoCard` 下拉旁追加操作按钮调用后端批量删除接口
