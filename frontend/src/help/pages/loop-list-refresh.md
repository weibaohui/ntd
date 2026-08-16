# 刷新列表

## 功能位置

环路（列表） → 顶部 `PageCard` 的 `extra` 区 → `LoopListHeader` 「刷新」按钮（`Button` 带 `ReloadOutlined`，`aria-label="刷新"`，`loading` 绑定数据加载态）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击刷新按钮] --> RR["onReload reload"]
  RR --> LD["useLoopListData reload"]
  LD --> SL["setLoading(true)"]
  SL --> DBL["dbLoops.listLoops(workspaceId)"]
  DBL --> API["GET /api/v1/workspaces/{ws}/loops"]
  API --> H1["list_loops_v1 handler"]
  H1 --> DAO["db.list_loops_with_counts(Some ws_id)"]
  DAO --> DB[(loops 表 + 子查询聚合)]
  H1 --> RT[ApiResponse ok Vec LoopListItem]
  RT --> LD --> SI["setItems(data)"]
  SI --> LF["setLoading(false)"]
  LF --> LV[LoopListView 重渲染]
```

## 调用关系链路图

```mermaid
flowchart TD
  Header["LoopListHeader Button onClick"] -->|"onReload"| LLP["LoopListPage"]
  LLP -->|"reload 来自 useLoopListData"| LD["useLoopListData reload useCallback"]
  LD -->|"workspaceId != null"| DBL["dbLoops.listLoops(workspaceId)"]
  DBL -->|"unwrap"| API["api.get /api/v1/workspaces/{ws}/loops"]
  API -->|"HTTP"| H1["list_loops_v1"]
  H1 --> DAO["db.list_loops_with_counts"]
  H1 -->|"批量"| TPL["db.get_process_templates_by_ids"]
  LD -->|"effect 依赖 reload loopUpdateCount"| UE["useEffect reload"]
```

## 数据结构图

```mermaid
classDiagram
  class LoopListRow {
    +loop_: loops::Model
    +step_count: i32
    +last_execution_status: String
    +last_execution_at: Option String
    +pending_approval_count: i32
  }
  class LoopListItem {
    +id: number
    +name: string
    +status: string
    +step_count: number
    +pending_approval_count: number
    +last_execution_status: string
    +last_execution_at: string | null
    +updated_at: string | null
  }
  class LoopListData {
    +items: LoopListItem[]
    +loading: boolean
    +reload: () => void
  }
  LoopListRow --> LoopListItem : handler into Vec LoopListItem
  LoopListData --> LoopListItem : state
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 已加载: 挂载时 useEffect reload
  已加载 --> 加载中: 点击刷新触发 reload setLoading true
  加载中 --> 已加载: 请求成功 setItems setLoading false
  加载中 --> 错误态: catch message.error 加载环路列表失败
  错误态 --> 已加载: 用户再次点击刷新
  已加载 --> 加载中: workspaceId 变化或 loopUpdateCount 递增触发 reload
```

## 开发指导

- **前端入口**：`frontend/src/components/loop-list/LoopListPageParts.tsx` 的 `LoopListHeader`（刷新 `Button` `onClick={onReload}`），`reload` 由 `frontend/src/components/loop-list/index.tsx` 的 `useLoopListData` 提供（`useCallback` 包裹 `dbLoops.listLoops`），`loading` 绑定按钮 `loading` prop。
- **后端入口**：`backend/src/handlers/loop_.rs` 的 `list_loops_v1`（路由 `GET /api/v1/workspaces/{ws}/loops` 见 `v1_routes`），DAO `backend/src/db/loop_.rs` 的 `Database::list_loops_with_counts`。
- **注意**：`reload` 用 `useCallback` 保证引用稳定，避免 `useEffect([reload, loopUpdateCount])` 依赖循环；`workspaceId == null` 时直接 `setItems([])` 短路不发请求；后端原生 SQL 按 `id DESC` 排序（054 起与 task/todo 一致），并含 `step_count`/`last_execution_status`/`last_execution_at`/`pending_approval_count` 四个子查询聚合。
- **扩展**：要加服务端分页/排序，在 `listLoops` 增 `page`/`sort` 查询参数，`list_loops_v1` 解析后改 `list_loops_with_counts` 的 `ORDER BY` 与 `LIMIT/OFFSET`；当前前端用 `Table` 的 `pagination`（`pageSize:20`）做客户端分页。
