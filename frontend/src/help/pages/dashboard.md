# 仪表盘

> 页面级总览。本页各功能点的 4 图 + 开发指导在子文档中维护。

## 页面简介

仪表盘是全局运营视图，不依赖当前选中的工作空间。它把此前塞进单个 Masonry 的 24 张卡片按语义拆成 7 个 Tab：总览、任务、执行、成本与模型、自动化、资源与运维、工艺。顶部全局时间范围（Segmented + 自定义 RangePicker）对所有 Tab 共享，切换 Tab 不丢失筛选上下文。

数据由 `/api/v1/stats/dashboard` 全库聚合返回 `DashboardStats`（含 todo 分布、执行统计、token/费用、模型分布、近期执行、排行榜等），并有 30 秒 TTL 缓存。飞书消息吞吐由 `/api/v1/feishu/message-stats` 单独拉取，失败时降级空态不弹错。工艺 Tab 由 `ProcessDashboard` 自取 `/api/v1/processes/stats`。

## 页面级数据流总图

```mermaid
flowchart LR
  U[用户进入仪表盘] --> D["Dashboard 组件"]
  D --"loadStats(hours) db.getDashboardStats"--> API1["/api/v1/stats/dashboard?hours="]
  D --"loadMsgStats(hours) db.getFeishuMessageStats"--> API2["/api/v1/feishu/message-stats?hours="]
  API1 --> H1["get_dashboard_stats handler"]
  H1 --> SVC1["db.get_dashboard_stats"]
  SVC1 --> DB1[(todos/loop_executions/executions 表 聚合)]
  H1 --> CACHE["DASHBOARD_CACHE 30s TTL"]
  API2 --> H2["get_message_stats handler"]
  H2 --> DB2[(feishu_history_messages 表)]
  D --"TimeRangeSelector 全局共享"--> TR["时间范围 Segmented + RangePicker"]
  D --"handleTabChange pushUrl"--> URL["URL hash tab 参数"]
  D --"resolvedTab 校验"--> TAB["Tabs 7 个 Tab"]
  TAB --> OT["OverviewTab stats/successRate/runningTasks"]
  TAB --> TT["TasksTab stats/totalTodos"]
  TAB --> ET["ExecutionsTab stats/tagsLength"]
  TAB --> CT["CostTab stats + UsageStatsCard 自取"]
  TAB --> AT["AutomationTab msgStats/hours"]
  TAB --> RT["ResourcesTab stats"]
  TAB --> PT["ProcessDashboard 自取 getProcessStats"]
```

## 功能点索引

- [Tab 切换（7 个语义域）](dashboard-tab-switch)
- [全局时间范围选择](dashboard-time-range)
- [总览 Tab](dashboard-overview)
- [成本与模型 Tab](dashboard-cost)
- [自动化 Tab](dashboard-automation)
- [工艺 Tab](dashboard-process)
