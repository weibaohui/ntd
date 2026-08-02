# 成本与模型 Tab

## 功能位置

仪表盘 → 「成本与模型」Tab（移动端短文案「成本」） → `CostTab` 组件（推理统计 / 会话统计 / Token 图表 / 模型分布 / 缓存命中 / 排行榜 / UsageStatsCard）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  D["Dashboard 顶层 props"] --> CT["CostTab stats/loading/usageSince/usageUntil"]
  CT --"InferenceStatsCard stats"--> IS["推理统计卡"]
  CT --"SessionsStatsCard"--> SS["会话统计卡"]
  CT --"TokenChartCard stats"--> T1["Token 分布图"]
  CT --"TokenTrendChartCard stats"--> T2["Token 趋势图"]
  CT --"ModelTaskChartCard stats"--> M1["模型任务分布图"]
  CT --"ModelTokenChartCard stats"--> M2["模型 Token 分布图"]
  CT --"ModelCacheCard stats"--> MC["模型缓存命中卡"]
  CT --"LeaderboardCard stats.leaderboard"--> LB["排行榜卡"]
  CT --"UsageStatsCard since/until"--> US["UsageStatsCard 自取"]
  US --"fetch /api/usage-stats"--> API2["/api/usage-stats?since=&until="]
  CT --> S1["stats.total_input_tokens / total_output_tokens / total_cost_usd"]
  CT --> S2["stats.model_distribution / model_cache_stats"]
  CT --> S3["stats.leaderboard"]
```

## 调用关系链路图

```mermaid
flowchart TD
  CT["CostTab panels: PanelItem[]"] --> TM["TabMasonry panels"]
  TM --> P1["inference-stats: InferenceStatsCard"]
  TM --> P2["sessions-stats: SessionsStatsCard"]
  TM --> P3["token-chart: TokenChartCard"]
  TM --> P4["token-trend-chart: TokenTrendChartCard"]
  TM --> P5["model-task-chart: ModelTaskChartCard"]
  TM --> P6["model-token-chart: ModelTokenChartCard"]
  TM --> P7["model-cache: ModelCacheCard"]
  TM --> P8["leaderboard: LeaderboardCard"]
  CT --> US["UsageStatsCard 单独置底 since/until"]
  US --> API["fetch /api/usage-stats 自取"]
```

## 数据结构图

```mermaid
classDiagram
  class CostTabProps {
    +stats: DashboardStats | null
    +loading: boolean
    +usageSince?: string
    +usageUntil?: string
  }
  class DashboardStats {
    +total_input_tokens: number
    +total_output_tokens: number
    +total_cache_read_tokens: number
    +total_cache_creation_tokens: number
    +total_cost_usd: number
    +model_distribution: ModelCount[]
    +model_cache_stats: ModelCacheStat[]
    +leaderboard: LeaderboardItem[]
  }
  class UsageStatsCard {
    +since: string
    +until: string
  }
  CostTabProps --> DashboardStats: 顶层下发
  CostTabProps --> UsageStatsCard: since/until ISO 派生
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 加载中
  加载中 --> 已渲染: stats 非空
  已渲染 --> 已渲染: stats 更新
  已渲染 --> UsageStatsCard自取中: since/until 变化
  UsageStatsCard自取中 --> 已渲染: /api/usage-stats 返回
  已渲染 --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/dashboard/tabs/CostTab.tsx` 的 `CostTab` 组件；`InferenceStatsCard` 在 `StatsGridCards.tsx`；`SessionsStatsCard` 在 `frontend/src/components/dashboard/cards/SessionsStatsCard.tsx`；`ModelTaskChartCard` / `ModelTokenChartCard` / `ModelCacheCard` 在 `DistributionCards.tsx`；`UsageStatsCard` 在 `frontend/src/components/dashboard/UsageStatsCard.tsx`
- **后端入口**：stats 字段来自 `get_dashboard_stats` handler 聚合；`UsageStatsCard` 自取 `/api/usage-stats`（ccusage 通道，与主 stats 解耦）
- **注意**：`UsageStatsCard` 数据较多（含 daily/weekly/monthly + 模型 breakdown），固定置底与瀑布流不拆散；`usageSince`/`usageUntil` 由顶层 `TimeRangeSelector` 派生 ISO 串，全局共享
- **扩展**：增成本维度卡时，在 `panels` 加项；新增 stats 字段时改后端 `DashboardStats` struct + `db.get_dashboard_stats` SQL 聚合
