# 全局时间范围选择

## 功能位置

仪表盘 → 顶部 `TimeRangeSelector` 控件（Segmented 多档预设 + 自模式 DatePicker.RangePicker）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户选时间档] --> HTR["handleTimeRangeChange(value)"]
  HTR --"value === 'custom'"--> SKIP["不发请求 等用户选区间"]
  HTR --"value 是小时数"--> CLR["setCustomRange(null)"]
  CLR --> UUR["setUsageStatsRange since/until ISO"]
  HTR --> LS["loadStats(value)"]
  HTR --> LM["loadMsgStats(value)"]
  LS --"db.getDashboardStats(hours)"--> API1["/api/v1/stats/dashboard?hours="]
  LM --"db.getFeishuMessageStats(hours)"--> API2["/api/v1/feishu/message-stats?hours="]
  API1 --> H1["get_dashboard_stats handler"]
  H1 --> SVC["db.get_dashboard_stats(hours)"]
  SVC --> DB[(todos/loop_executions 聚合)]
  API2 --> H2["get_message_stats handler"]
  U --"自模式选区间"--> HCR["handleCustomRangeChange(dates)"]
  HCR --> UUR2["setUsageStatsRange since/until"]
  HCR --"hours = diff 区间跨度"--> LS2["loadStats(hours)"]
  HCR --> LM2["loadMsgStats(hours)"]
```

## 调用关系链路图

```mermaid
flowchart TD
  TRS["TimeRangeSelector Segmented onChange"] --> HTR["handleTimeRangeChange"]
  HTR --> ST["setTimeRange(value)"]
  HTR --> CLR["setCustomRange(null)"]
  HTR --> UUR["setUsageStatsRange since/until"]
  HTR --> LS["loadStats(value)"]
  HTR --> LM["loadMsgStats(value)"]
  LS --> API1["db.getDashboardStats(hours)"]
  LM --> API2["db.getFeishuMessageStats(hours)"]
  RP["RangePicker onChange"] --> HCR["handleCustomRangeChange"]
  HCR --> SCR["setCustomRange(dates)"]
  HCR --> UUR2["setUsageStatsRange"]
  HCR --> H["hours = diff hour true"]
  H --> LS2["loadStats(hours)"]
  H --> LM2["loadMsgStats(hours)"]
```

## 数据结构图

```mermaid
classDiagram
  class TimeRangeSelectorProps {
    +timeRange: number | 'custom'
    +customRange: [Dayjs, Dayjs] | null
    +onTimeRangeChange: (value) => void
    +onCustomRangeChange: (dates) => void
  }
  class DashboardStatsParams {
    +hours: Option~u32~
  }
  class MessageStatsParams {
    +hours: Option~u32~
    +workspace_id: Option~i64~
  }
  class DashboardStats {
    +total_executions: number
    +success_executions: number
    +daily_executions: DailyExecution[]
    +recent_executions: ExecutionRecord[]
  }
  TimeRangeSelectorProps --> DashboardStatsParams: hours 换算
  DashboardStatsParams --> DashboardStats: handler 返回
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 720h: 首次挂载默认 30 天
  720h --> 自模式: 用户选 custom
  自模式 --> 720h: 用户切回预设档
  自模式 --> 自模式: 选新区间
  预设档 --> 预设档: 切换档位
  预设档 --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/Dashboard.tsx` 的 `handleTimeRangeChange` / `handleCustomRangeChange` / `loadStats` / `loadMsgStats`；控件在 `frontend/src/components/dashboard/SpecialCards.tsx` 的 `TimeRangeSelector`；`db.getDashboardStats` / `db.getFeishuMessageStats` 在 `frontend/src/utils/database.ts`
- **后端入口**：`backend/src/handlers/execution.rs` 的 `get_dashboard_stats` handler（路由 `/api/v1/stats/dashboard`，`DashboardStatsParams.hours`）；飞书在 `backend/src/handlers/feishu_history.rs` 的 `get_message_stats` handler（路由 `/api/v1/feishu/message-stats`）
- **注意**：自模式不发中间态请求，等用户选完区间才触发；`currentHours` 在 custom 模式用区间跨度小时数，派生给自动化 Tab 的 Loop 聚合按窗口过滤；后端 `get_dashboard_stats` / `get_loop_stats` 目前只接受 hours（往前 N 小时），历史区间精确 since/until 需后端改造
- **扩展**：新增预设档时，改 `TimeRangeSelector` 的 `TIME_RANGE_OPTIONS` 常量；后端支持精确 since/until 时，`DashboardStatsParams` 增字段、`db.get_dashboard_stats` SQL WHERE 改区间
