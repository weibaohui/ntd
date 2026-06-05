# AI 使用统计

> **入口**：仪表盘 → AI 使用统计卡片 / 设置 → 执行器管理 → AI 使用统计配置
> **后端**：`backend/src/handlers/usage_stats.rs` + `services/usage_stats`

把 ntd 通过各执行器跑过的任务按 **时间 + model** 维度汇总 token 和成本。

## 1. 数据来源

每次 Todo 跑完，后端会记录：
- model 名
- input_tokens
- output_tokens
- cache_read_input_tokens
- cache_write_input_tokens
- 估算成本（按 model 单价表）

汇总到 `usage_stats` 表。

## 2. 配置

设置 → 执行器管理 → 底部「AI 使用统计」卡片：

| 字段 | 默认 | 含义 |
|------|------|------|
| `enabled` | false | 是否开启定时汇总 |
| `cron` | `0 0 2 * * *` | 每天凌晨 2 点汇总 |
| `retention_days` | 90 | 数据保留天数 |

## 3. 视图

仪表盘 → AI 使用统计卡片：

- **趋势图**：折线，X 轴时间，Y 轴 token
  - 7 天 / 30 天切换
  - 按 model 分线
- **饼图**：按 model 占比
- **Top N**：消耗最多的 Todo 列表

## 4. 手动触发

`POST /api/usage-stats/refresh` 立即汇总一次（不依赖 Cron）。

## 5. 成本估算

后端内置一份 model 单价表（每月更新）：

| Model | Input ($/1M) | Output ($/1M) |
|-------|--------------|---------------|
| claude-3.5-sonnet | 3 | 15 |
| claude-3-opus | 15 | 75 |
| gpt-4o | 5 | 15 |
| ... | ... | ... |

估算 = (input × 单价 + output × 单价) / 1,000,000

> ⚠️ 实际计费以云厂商账单为准，这里只用于预算参考。

## 6. 故障排查

### 6.1 数据全 0

- 没启用 → 设置里开关
- 没数据可汇总（最近 24h 没 Todo 跑过）
- 手动触发看：`POST /api/usage-stats/refresh` → 看返回

### 6.2 估算成本跟实际账单对不上

- 单价表可能过时
- 缓存命中不计费（但 ntd 不区分）
- 看云厂商账单明细

## 7. 隐私

- 数据全部存本地 SQLite
- 不上送任何服务器
- 想删全量：直接 `DELETE FROM usage_stats`

## 8. 相关 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/usage-stats?range=7d` | 列表 |
| POST | `/api/usage-stats/refresh` | 手动汇总 |
| GET | `/api/usage-stats/settings` | 查配置 |
| PUT | `/api/usage-stats/settings` | 改配置 |
