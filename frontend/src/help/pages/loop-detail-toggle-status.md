# 启停切换

## 功能位置

环路（详情） → `LoopDetailPanel` 基本信息 Section「启用状态」字段 → `Switch`（`checked` 绑定 `detail.status === 'enabled'`，`onChange` 触发切换）+ 状态文字（已启用/已暂停/草稿）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户拨动启用状态 Switch] --> OC["Switch onChange"]
  OC --> TS["useLoopDetailActions handleToggleStatus"]
  TS --> GT["dbLoops.getLoop workspaceId loopId"]
  GT --> API1["GET /api/v1/workspaces/{ws}/loops/{id}"]
  API1 --> H1["get_loop_v1 handler"]
  H1 --> DAO1["db.load_loop_full + tags + template"]
  DAO1 --> RT1[LoopDetail]
  RT1 --> TS --> NEXT["next = loop.status === enabled ? paused : enabled"]
  NEXT --> DBL["dbLoops.updateLoopStatus workspaceId id {status: next}"]
  DBL --> API2["PUT /api/v1/workspaces/{ws}/loops/{id}/status"]
  API2 --> H2["update_loop_status_v1 handler"]
  H2 --> DAO2["db.update_loop_status id status"]
  DAO2 --> DB[(loops 表 status/updated_at)]
  H2 --> OK["ApiResponse ok LoopDto"]
  OK --> TS --> MS["message.success 已启用/暂停"]
  MS --> RL["reload setTimeout 100ms 重拉详情"]
  RL --> OC2["onChanged 父组件递增 loopUpdateCount"]
```

## 调用关系链路图

```mermaid
flowchart TD
  SW["LoopStudioDetailPanel Switch onChange"] -->|"onToggleStatus"| PAN["LoopDetailPanel onToggleStatus"]
  PAN -->|"prop"| LDP["LoopDetailPage onToggleStatus"]
  LDP -->|"useLoopDetailActions"| TS["handleToggleStatus useCallback"]
  TS -->|"先取最新状态"| GT["dbLoops.getLoop workspaceId loopId"]
  TS -->|"next 计算"| DBL["dbLoops.updateLoopStatus workspaceId id {status}"]
  DBL -->|"api.put"| API["PUT /api/v1/workspaces/{ws}/loops/{id}/status"]
  API -->|"HTTP"| H2["update_loop_status_v1"]
  H2 --> DAO["db.update_loop_status id status"]
  PAN -->|"切换后"| RL["setTimeout reload 100ms"]
  PAN -->|"切换后"| OC["onChanged"]
```

## 数据结构图

```mermaid
classDiagram
  class LoopDetail {
    +id: number
    +status: string
  }
  class UseLoopDetailActionsArgs {
    +loopId: number
    +workspaceId: number | null
    +onLoopChanged(): void
  }
  class UpdateLoopStatusRequest {
    +status: LoopStatus | string
  }
  class LoopStatus {
    enabled: string
    paused: string
  }
  class loops::Model {
    +id: i64
    +status: String
    +updated_at: Option String
  }
  LoopDetail --> UpdateLoopStatusRequest : loop.status 决定 next
  UseLoopDetailActionsArgs --> UpdateLoopStatusRequest : 请求体
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> enabled: 已启用 Switch checked=true
  [*] --> paused: 已暂停 Switch checked=false
  enabled --> 加载中: 用户拨 Switch handleToggleStatus getLoop
  paused --> 加载中: 用户拨 Switch handleToggleStatus getLoop
  加载中 --> 切换中: next 计算 updateLoopStatus PUT 请求
  切换中 --> enabled: 原 paused 切到 enabled message.success 已启用
  切换中 --> paused: 原 enabled 切到 paused message.success 已暂停
  切换中 --> enabled: 校验失败 message.error 保持原态
  切换中 --> paused: 校验失败 message.error 保持原态
  enabled --> enabled: reload 100ms 后 Switch/文字同步刷新
  paused --> paused: reload 100ms 后 Switch/文字同步刷新
```

## 开发指导

- **前端入口**：`frontend/src/components/LoopStudioDetailPanel.tsx` 的 `LoopDetailPanel` 基本信息 Section「启用状态」`Switch`（`onChange` 内调 `onToggleStatus` + `setTimeout(reload, 100)` + `onChanged`），回调 `frontend/src/components/LoopDetailPage.tsx` 的 `useLoopDetailActions.handleToggleStatus`（在 `frontend/src/components/LoopDetailPageParts.tsx`）。
- **后端入口**：`backend/src/handlers/loop_.rs` 的 `update_loop_status_v1`（路由 `PUT /api/v1/workspaces/{ws}/loops/{id}/status`），先 `models::validate_loop_status` 校验枚举再 `workspace_guard::verify_loop_belongs_to_ws`，DAO `backend/src/db/loop_.rs` 的 `Database::update_loop_status`。
- **注意**：详情页切换前要先 `dbLoops.getLoop` 重取最新 `status` 再算 `next`（列表页是直接用行内 `loop.status`），避免详情页长时间停留后状态已过期导致翻转方向错；`setTimeout(reload, 100)` 让后端写入完成后再重拉详情，让 `Switch` 与状态文字同步刷新；`onChanged` 递增 `loopUpdateCount` 让列表页联动。
- **扩展**：要加新状态，需同步改 `backend/src/models/loop_.rs` 的 `validate_loop_status`、`frontend/src/types/loop.ts` 的 `LoopStatus`、`LoopStudioDetailPanel.tsx` 的状态文字判定（`enabled`/`paused`/草稿 三分支），以及 `LoopListViewParts.LOOP_STATUS_META` 的中文/颜色映射。
