# 仪表盘

> **位置**：主界面左侧栏 → 「仪表盘」
> **前端**：`frontend/src/components/Dashboard.tsx` + `dashboard/*`
> **后端**：`backend/src/handlers/mod.rs`（dashboard-stats）+ `usage_stats.rs`

ntd 的**全局运营视图**。一个屏幕看明白：
- 现在跑了多少
- 跑了多少 token
- 飞书消息量
- 执行器使用分布

## 1. 主要卡片

### 1.1 关键指标（顶部 4 块）

| 指标 | 含义 |
|------|------|
| 总 Todo 数 | 所有非软删的 Todo |
| 本周新增 | 本周创建的 |
| 跑成功率 | 最近 7 天 completed / total |
| 今日 Token | 今日所有执行器消耗 |

### 1.2 执行器分布（饼图）

按执行器分组，**最近 7 天**跑过的 Todo 数。
- claudecode 蓝
- codex 紫
- hermes 绿
- ...

### 1.3 Token 趋势（折线）

- 7 天 / 30 天切换
- 按 model 分多条线
- 数据来源：`usage_stats` 表

### 1.4 飞书消息统计

- 今日接收
- 今日回复
- 关联 Todo 数

启用「飞书 Bot 绑定」后才有数据。

### 1.5 活动任务

实时显示当前 running 的 Todo 列表（同「运行管理」面板）。

### 1.6 分享卡

底部「分享给朋友」按钮 → 生成图片带版本号、QR 码、安装指令。

## 2. 自动刷新

- 关键指标 30s 自动刷新
- 活动任务 5s 自动刷新
- Token 趋势靠 `usage_stats` 定时任务，不实时

## 3. API

| Method | Path |
|--------|------|
| GET | `/api/dashboard-stats` |
| GET | `/api/usage-stats?range=7d` |
| GET | `/api/feishu/message-stats` |

## 4. 故障排查

### 4.1 数字全是 0

- 数据库是空的
- `usage_stats` 没开启 → 去执行器管理 → 启用 AI 使用统计

### 4.2 趋势图没数据

- 历史不够 7 天
- `usage_stats` 定时没跑（看后端日志）
- 手动触发：`POST /api/usage-stats/refresh`

### 4.3 飞书统计没数据

- 没绑飞书 Bot
- 绑了但最近没消息
- 调 `GET /api/feishu/message-stats?range=7d` 看原始数据
