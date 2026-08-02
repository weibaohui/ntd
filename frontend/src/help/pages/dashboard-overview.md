# 总览 Tab

## 功能位置

仪表盘 → 「总览」Tab → `OverviewTab` 组件（核心 KPI / 运行中任务 / 执行趋势 / 贡献热力图 / 最近执行记录表 / 分享卡）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  D["Dashboard 顶层 props"] --> OT["OverviewTab stats/loading/successRate/runningTasks/todos"]
  OT --"KeyMetricsCard stats"--> KM["核心 KPI 卡"]
  OT --"ActiveTasksCard runningTasks"--> AT["运行中任务卡"]
  OT --"TrendChartCard stats.daily_executions"--> TC["执行趋势图"]
  OT --"OverviewCard stats/successRate"--> OC["概览卡"]
  OT --"ContributionHeatmapCard stats"--> CH["贡献热力图 全宽"]
  OT --"RecentExecutionsTable stats.recent_executions"--> RE["最近执行记录表"]
  OT --"ShareCardPanel"--> SP["分享卡"]
  KM --> S1["stats.total_executions / success_executions / total_cost_usd"]
  TC --> S2["stats.daily_executions"]
  RE --> S3["stats.recent_executions"]
```

## 调用关系链路图

```mermaid
flowchart TD
  OT["OverviewTab panels: PanelItem[]"] --> TM["TabMasonry panels 渑布流"]
  TM --> P1["key-metrics: KeyMetricsCard"]
  TM --> P2["active-tasks: ActiveTasksCard"]
  TM --> P3["trend-chart: TrendChartCard"]
  TM --> P4["overview-card: OverviewCard"]
  OT --> CH["ContributionHeatmapCard 单独全宽"]
  OT --> RE["RecentExecutionsTable 单独成块"]
  OT --> SP["ShareCardPanel"]
  KM["KeyMetricsCard"] --> SRC["stats: DashboardStats"]
  AT["ActiveTasksCard"] --> SRC2["runningTasks: RunningTask[]"]
```

## 数据结构图

```mermaid
classDiagram
  class OverviewTabProps {
    +stats: DashboardStats | null
    +loading: boolean
    +successRate: number
    +runningTasks: RunningTask[]
    +todos: Todo[]
  }
  class DashboardStats {
    +total_executions: number
    +success_executions: number
    +total_cost_usd: number
    +daily_executions: DailyExecution[]
    +recent_executions: ExecutionRecord[]
  }
  class RunningTask {
    +todoId: number
    +status: string
  }
  class PanelItem {
    +key: string
    +render: () => JSX
  }
  OverviewTabProps --> DashboardStats: 顶层下发
  OverviewTabProps --> RunningTask: 运行中任务
  OverviewTabProps --> PanelItem: panels 数组
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 加载中
  加载中 --> 已渲染: stats 非空
  已渲染 --> 已渲染: stats 更新 顶层 useEffect 重拉
  已渲染 --> 空态: stats 为 null
  空态 --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/dashboard/tabs/OverviewTab.tsx` 的 `OverviewTab` 组件；`KeyMetricsCard` / `OverviewCard` 在 `frontend/src/components/dashboard/StatsGridCards.tsx`；`ActiveTasksCard` / `ShareCardPanel` 在 `frontend/src/components/dashboard/SpecialCards.tsx`；`TrendChartCard` / `ContributionHeatmapCard` 在 `frontend/src/components/dashboard/ChartCards.tsx`；`RecentExecutionsTable` 在同目录
- **后端入口**：数据来自顶层 `Dashboard` 拉的 `/api/v1/stats/dashboard`（`get_dashboard_stats` handler）；后端 `db.get_dashboard_stats` 聚合全库
- **注意**：贡献热力图因一年 53 周横向跨度大，从 Masonry 移出独占一行全宽渲染，格子随宽度放大可读性提升；最近执行记录表与瀑布流视觉节奏不同单独成块置于下方；分享卡置于总览底部作为推广位
- **扩展**：总览 Tab 增新卡时，在 `panels` 数组加 `{ key, render }` 项，或单独全宽渲染时直接追加在 `TabMasonry` 之后
