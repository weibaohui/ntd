# 自动化 Tab

## 功能位置

仪表盘 → 「自动化」Tab → `AutomationTab` 组件（Loop 环路统计卡 / 飞书消息吞吐卡 / 飞书监听健康卡）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  D["Dashboard 顶层 props"] --> AT["AutomationTab msgStats/msgStatsError/processingRate/hours"]
  AT --"LoopStatsCard hours"--> LS["环路统计卡"]
  LS --"fetch loop_stats"--> API1["/api/v1/stats/loop?hours="]
  AT --"MessageStatsCard msgStats/processingRate"--> MS["消息吞吐卡"]
  AT --"FeishuMonitorCard"--> FM["监听健康卡"]
  FM --"fetch feishu config"--> API2["/api/v1/feishu/config"]
  MS --> S1["msgStats.total_messages / processed / unprocessed"]
  LS --> S2["hours 按窗口过滤 Loop 聚合"]
```

## 调用关系链路图

```mermaid
flowchart TD
  AT["AutomationTab panels: PanelItem[]"] --> TM["TabMasonry panels"]
  TM --> P1["loop-stats: LoopStatsCard hours"]
  TM --> P2["message-stats: MessageStatsCard msgStats/msgStatsError/processingRate"]
  TM --> P3["feishu-monitor: FeishuMonitorCard"]
  LS["LoopStatsCard"] --> API["db.getLoopStats(hours)"]
  MS["MessageStatsCard"] --> SRC["msgStats: FeishuMessageStats 顶层下发"]
  FM["FeishuMonitorCard"] --> API2["自取飞书监听状态"]
```

## 数据结构图

```mermaid
classDiagram
  class AutomationTabProps {
    +msgStats: FeishuMessageStats | null
    +msgStatsError: boolean
    +processingRate: number
    +hours?: number
  }
  class FeishuMessageStats {
    +total_messages: number
    +processed: number
    +unprocessed: number
  }
  class LoopStats {
    +active_loops: number
    +total_runs: number
    +ai_ratings: number
  }
  AutomationTabProps --> FeishuMessageStats: 顶层下发
  AutomationTabProps --> LoopStats: hours 过滤
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 加载中
  加载中 --> 已渲染: msgStats 非空
  已渲染 --> 已渲染: hours 变化 顶层重拉 Loop 聚合
  已渲染 --> 降级态: msgStatsError=true 非书未配置
  降级态 --> [*]
  已渲染 --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/dashboard/tabs/AutomationTab.tsx` 的 `AutomationTab` 组件；`MessageStatsCard` 在 `frontend/src/components/dashboard/StatsGridCards.tsx`；`LoopStatsCard` 在 `frontend/src/components/dashboard/cards/LoopStatsCard.tsx`；`FeishuMonitorCard` 在同 cards 子目录
- **后端入口**：飞书消息 stats 来自顶层 `db.getFeishuMessageStats`（handler `get_message_stats`，路由 `/api/v1/feishu/message-stats`）；Loop 聚合由 `LoopStatsCard` 自取 `/api/v1/stats/loop`（`hours` 过滤）
- **注意**：`msgStatsError=true` 时（飞书未配置）用布尔标记降级展示，不弹错误打扰用户；`hours` 在 custom 模式用区间跨度小时数，undefined=全时段；飞书消息吞吐与监听健康互补，同 Tab 展示
- **扩展**：增自动化维度卡时，在 `panels` 加项；新增 Loop 聚合字段时改后端 `get_loop_stats` handler + SQL
