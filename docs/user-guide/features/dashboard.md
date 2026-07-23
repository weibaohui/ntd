# 仪表盘

> **位置**：主界面左侧栏 → 「全局视图」→「仪表盘」
> **前端**：`frontend/src/components/Dashboard.tsx` + `frontend/src/components/dashboard/*`
> **后端**：`backend/src/handlers/execution.rs` + `backend/src/db/dashboard.rs`

ntd 的**全局运营视图**。数据聚合所有工作区，不随当前 workspace 切换变化。一个屏幕看明白：
- 任务运行状况
- 跑了多少 token
- 飞书消息量
- 执行器使用分布
- 模型、技能、备份、贡献等多维统计

## 1. 时间范围

页面顶部 `TimeRangeSelector`（`SpecialCards.tsx:96-115`）控制全局时间窗口：

| 选项 | 含义 |
|------|------|
| 5 小时 | 5h |
| 7 天 | 7d |
| 14 天 | 14d |
| 30 天 | 30d（默认 720h） |
| 自定义 | RangePicker 自选起止 |

切换时间范围会重拉 `stats/dashboard`、`feishu/message-stats`、`usage-stats` 三组数据。**没有自动 setInterval 刷新**——只有切换时间范围或主动点「刷新」才更新数据。

## 2. 卡片清单

仪表盘使用 `Masonry` 网格（`xs:1 / sm:1 / md:2 / lg:2 / xl:3`）展示下列卡片。

### 2.1 关键指标（KeyMetrics，7 项）

来源：`dashboard/StatsGridCards.tsx:19-91`（`KeyMetricsCard`）。

| 指标 | 含义 |
|------|------|
| 今日执行 | 当日累计执行次数，环比 vs 昨日 |
| 总执行 | 历史累计执行次数，环比本周 |
| 成功率 | `success / total * 100%` |
| 总花费 | `total_cost_usd` 累计（USD） |
| 活跃天数 | 有过执行的累计天数 |
| 连续天数 | 截至当前的连续活跃天数 |
| 平均耗时 | `avg_duration_ms / 1000`（秒） |

### 2.2 亮点数据（HighlightStats，3 项）

`HighlightStatsCard`：`单日峰值` / `最高产模型`（含万 tokens 子标） / `活跃天数`。

### 2.3 任务概览（TaskStats，4 项）

`total_todos` / `running_todos` / `completed_todos` / `failed_todos`。

### 2.4 执行概览（ExecStats，4 项）

`total_tags` / `scheduled_todos` / `total_executions` / `total_cost_usd`。

### 2.5 推理统计（InferenceStats，4 项）

`total_input_tokens` / `total_output_tokens` / `total_cost_usd` / `outputRate = output/input * 100%`。

### 2.6 执行器分布（ExecutorChart）

**横向条形图**（`DistributionCards.tsx:13-55`），不是饼图。每行包含执行器名、Todo 数、成功率、次数、花费。

执行器色板（`EXECUTOR_COLORS`）：

| 执行器 | 颜色 | 说明 |
|--------|------|------|
| claudecode | `#e17055`（橙红） | Claude Code |
| hermes | `#0984e3`（蓝） | Hermes |
| codex | `#488597`（蓝灰） | Codex |
| codebuddy | `#00b894`（绿） | CodeBuddy |
| opencode | `#fdcb6e`（黄） | Opencode |
| mobilecoder | `#6c5ce7`（紫） | MobileCoder |
| atomcode | `#e84393`（粉） | AtomCode |
| kimi | `#d63031`（红） | Kimi |
| codewhale | `#00cec9`（青） | CodeWhale |

### 2.7 执行器平均耗时（ExecutorDuration）

横向条形图：每个执行器的 `avg_duration_ms`（自适应 ms / s 显示）+ 执行次数。

### 2.8 标签分布（TagChart）

横向条形图：按 `tag_id` 聚合，含 Todo 数 / 成功率 / 花费。

### 2.9 模型任务分布（ModelTaskChart）

横向条形图：每个模型的 Todo 数 + 执行次数 + 成功率。

### 2.10 模型推理统计（ModelTokenChart）

横向条形图：每个模型的 `input_tokens` + 成本 + 输出率。

### 2.11 缓存效率（ModelCache）

横向条形图：每个模型的 `cache_hit_rate`（按 `>50%` 绿 / `>20%` 黄 / 其他红着色），含 cache 读 / 输入子标。

### 2.12 活动热力图（ContributionHeatmap）

按 `daily_executions` 数据绘制的热力图（`ChartCards.tsx:135-145`）。

### 2.13 Token 趋势（TokenTrendChart）

**双线折线图**（`ChartCards.tsx:147-220`），input（`#3b82f6`）和 output（`#22c55e`）两条线，X 轴是日期，Y 轴按 `max(input+output)` 归一化；时间窗口跟随顶部 `TimeRangeSelector` 切换（5h / 7d / 14d / 30d / 自定义）。

### 2.14 触发来源（TriggerSource）

按 `trigger_type` 分组的执行统计（manual / cron / slash_command / default_response）。

### 2.15 状态 / 趋势图

`StatusChart`（按状态分布） + `TrendChart`（执行趋势折线）。

### 2.16 模型排行榜（Leaderboard）

`EnhancedCards.tsx` 中的 `Leaderboard` 组件展示模型排行。

### 2.17 活跃任务（ActiveTasksCard）

实时显示当前 running 的 Todo 列表（同「运行管理」面板）。**无任务时显示「Task In, Done Out.」禅意占位 + 一句随机引言**（`SpecialCards.tsx:55-67`）。

### 2.18 Skills 调用统计（SkillsStats）

包含「总调用 / 今日调用 / 成功率 / 平均耗时」4 个数字 + Top 5 Skills（`DistributionCards.tsx:257-327`）。

### 2.19 备份统计（BackupStats）

`database.file_count` / `todo.file_count` / `skills.file_count`。

### 2.20 使用统计（UsageStats）

`UsageStatsCard`（`dashboard/UsageStatsCard.tsx`）：顶部 `Segmented` 在 **日 / 周 / 月** 三个表格间切换，下方展示 `Input Tokens` / `Output Tokens` / `Total Cost` 三个汇总数 + 详细表格 + 按 model 分组的 breakdown 表。

数据由 `usage_stats` 表（来自 ccusage 集成）写入。

### 2.21 消息记录分析（MessageStats，4 项）

| 指标 | 含义 |
|------|------|
| 消息总量 | 飞书侧累计收到的消息 |
| 已处理 | 已回复 / 已触发 Todo 的数量 |
| 处理率 | `processed / total * 100%` |
| 触发任务 | 成功触发 Todo 的消息数 |

启用「飞书 Bot 绑定」后才有数据。

### 2.22 分享卡（ShareCardPanel）

底部「分享给朋友」按钮 → `ShareCard` 组件生成**一键复制 ntd 安装提示词**（`npm install -g @weibaohui/ntd...`），不是图片带 QR 码。

## 3. 最近执行记录

页面底部表格（`Dashboard.tsx:227-237`）展示 `recent_executions`（按时间倒序），含任务名、执行器、状态、触发类型、token、耗时、花费、开始时间。

## 4. API

| Method | Path |
|--------|------|
| GET | `/api/v1/stats/dashboard?hours=<n>` |
| GET | `/api/v1/usage-stats?since=&until=` |
| POST | `/api/v1/usage-stats/refresh` |
| GET | `/api/v1/feishu/message-stats?hours=<n>` |

## 5. 故障排查

### 5.1 数字全是 0

- 数据库是空的
- `usage_stats` 没开启 → 去「执行器管理」页底部的「AI 使用统计」卡片启用

### 5.2 趋势图没数据

- 历史不够所选时间窗口
- `usage_stats` 定时没跑（看后端日志）
- 手动触发：`POST /api/v1/usage-stats/refresh`

### 5.3 飞书统计没数据

- 没绑飞书 Bot
- 绑了但最近没消息
- 调 `GET /api/v1/feishu/message-stats?hours=168` 看原始数据
