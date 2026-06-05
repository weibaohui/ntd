# Webhook 与自动化

ntd 提供了**三层**触发机制让 Todo 跑起来。本文档梳理清楚它们的差异和组合用法。

## 1. 三层触发

| 层 | 触发方式 | 鉴权 | 适用 |
|----|----------|------|------|
| **Webhook** | `GET/POST /webhook/trigger/{todo_id}` | 路径 todo_id（可选 header） | CI/CD、外部系统 |
| **Slash 命令** | 飞书群里 `/command args` | 飞书 OAuth | 群协作 |
| **Cron 定时** | 配在 Todo 的 `scheduler` 字段 | 本机 | 周期任务 |

## 2. Webhook 详解

详见 [settings/webhooks.md](../settings/webhooks.md)

要点：
- 触发 URL 必须带 `todo_id` 路径参数
- 无内置鉴权（生产环境用 header 校验 + 反代 IP 白名单）
- 异步执行，立即返回 202

## 3. Slash 命令详解

### 3.1 配置

设置 → 系统设置 → SLASH 命令规则：

```yaml
slash_command_rules:
  - command: "/review"
    todo_id: 5
  - command: "/deploy"
    todo_id: 12
```

### 3.2 触发

在飞书群或单聊里：
```
/review 请帮我看 PR #42
```

Bot 会：
1. 匹配 `/review` → 找到 todo_id=5
2. 把 `请帮我看 PR #42` 作为 prompt 拼接到 todo 5 的 prompt
3. 执行 todo 5

### 3.3 关联记录

执行记录里会标「来自 /review」便于追溯。

## 4. Cron 定时详解

### 4.1 配置

TodoDrawer → 「定时」开关 → 填 Cron 表达式：

| 表达式 | 含义 |
|--------|------|
| `0 0 9 * * *` | 每天 9:00 |
| `0 0 9 * * 1` | 每周一 9:00 |
| `0 0 0 1 * *` | 每月 1 号 0:00 |
| `*/30 * * * * *` | 每 30 秒（慎用） |

### 4.2 时区

按「系统设置 → timezone」解析。默认 `Asia/Shanghai`。

### 4.3 跳过策略

如果上次还在跑，新触发**直接跳过**（不排队）。可去「运行管理」手动看状态。

## 5. 组合用法

### 5.1 场景：GitHub PR 自动化 review

1. 配一个 Todo「PR Review」prompt + workspace 指向 repo
2. 在 GitHub 仓库 → Settings → Webhooks → 配 `https://你的ntd/webhook/trigger/{review_todo_id}`
3. GitHub 事件选 `Pull request` → opened
4. 每次开 PR → ntd 收到 → 跑 review Todo

### 5.2 场景：定时汇报

1. 配 Todo「日报生成」prompt + scheduler `0 0 18 * * *`（每天 6 点）
2. 加后置 hook → 把结果 push 到飞书群
3. 每天 6 点自动跑 + 自动 push

### 5.3 场景：手动 @机器人查数据

1. 配 Todo「查数据库」prompt：连 MySQL + 跑 SQL
2. 配 SLASH 命令 `/db <sql>`
3. 群里 `/db SELECT * FROM users LIMIT 10` → Bot 跑 Todo → 把结果回群里

## 6. 安全清单

- [ ] Webhook 触发 URL 用 header secret 校验
- [ ] 反代层做 IP 白名单
- [ ] TLS（https）
- [ ] 飞书 Bot 配群白名单
- [ ] Cron 任务设 `execution_timeout_secs`
- [ ] 监控 `webhook_records` 表异常

## 7. 故障排查

| 现象 | 排查 |
|------|------|
| Webhook 404 | todo_id 错 / webhook 禁用 |
| Webhook 200 但没跑 | 后端日志，execution 启动失败 |
| Slash 命令无反应 | 飞书 Bot 白名单 / 命令拼错 |
| Cron 不触发 | Cron 表达式 / 时区 / 服务停了吗 |
| 频繁触发被打挂 | rate_limit_per_min 限流 |
