# AI 使用统计

> **入口**：
> - 仪表盘 → 「Token 用量统计」卡片（`dashboard/UsageStatsCard.tsx`）
> - 设置 → 执行器管理 → 页面底部的「AI 使用统计」配置卡片
> **后端**：`backend/src/handlers/usage_stats.rs` + `services/usage_stats`

把 ntd 通过各执行器跑过的任务按 **时间 + model** 维度汇总 token 和成本。

## 1. 数据来源

每次 Todo 跑完，后端会记录：
- model 名
- input_tokens
- output_tokens
- cache_read_input_tokens
- cache_creation_tokens
- 单次成本（来自 ccusage 外部统计）

> 数据由 `usage_stats` 表（来自 ccusage 集成）写入；ntd **不内置** model 单价表，成本完全依赖外部 ccusage 写入。

汇总到 `usage_stats` 表。

## 2. 配置

设置 → 执行器管理 → 页面底部「AI 使用统计」卡片：

| 字段 | 默认 | 含义 |
|------|------|------|
| `enabled` | `false` | 是否开启定时汇总（`auto_usage_stats_enabled`） |
| `cron` | `0 0 1 * * *` | 每天凌晨 1 点汇总（`auto_usage_stats_cron`，`ExecutorsPanel.tsx:28`） |

## 3. 视图

仪表盘 → Token 用量统计卡片：

- **顶部 Segmented**：日 / 周 / 月 三个粒度切换
- **三个汇总数字**：Input Tokens / Output Tokens / Total Cost（按当前粒度求和）
- **明细表格**：日期 / Input / Output / Cache Read / Cache Create / Cost / Models（每个粒度一张）
- **按 model 分组的 breakdown 表**：日期 / Model / Input / Output / Cache / Cost
- 「**刷新**」按钮立即触发后端重新汇总（对应 `POST /api/usage-stats/refresh`）

## 4. 手动触发

`POST /api/usage-stats/refresh` 立即汇总一次（不依赖 Cron）。

## 5. 故障排查

### 5.1 数据全 0

- 没启用 → 设置里开关
- 没数据可汇总（最近 24h 没 Todo 跑过）
- 手动触发看：`POST /api/usage-stats/refresh` → 看返回

### 5.2 成本跟实际账单对不上

- 成本由 ccusage 外部统计写入；检查 ccusage 是否正常运行
- 缓存命中是否被 ccusage 正确计费
- 看云厂商账单明细

## 6. 隐私

- 数据全部存本地 SQLite
- 不上送任何服务器
- 想删全量：直接 `DELETE FROM usage_stats`

## 7. 相关 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/usage-stats?since=&until=` | 列表（按时间窗） |
| POST | `/api/usage-stats/refresh` | 手动汇总 |
| GET | `/api/usage-stats/settings` | 查配置 |
| PUT | `/api/usage-stats/settings` | 改配置 |
