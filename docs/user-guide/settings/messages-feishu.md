# 消息（飞书 Bot）

> **位置**：设置 → 消息
> **前端**：`frontend/src/components/settings/MessagesPanel.tsx` + `messages/*`
> **后端**：`backend/src/handlers/agent_bot.rs`、`backend/src/handlers/feishu_history.rs`
> **相关服务**：`backend/src/services/feishu_listener`、`feishu_history_fetcher`、`feishu_push`、`message_debounce`

飞书 Bot 让 ntd 与飞书群/单聊**双向联动**：你在飞书里 @机器人说话，机器人把它转成 Todo；ntd 里有任务进展，机器人把结果 push 到飞书。

---

## 1. 子 Tab 概览

| 子 Tab | 作用 |
|--------|------|
| **BindTab（绑定）** | 配飞书 app、扫码 device flow、群白名单、推送开关 |
| **RecordTab（历史）** | 看历史消息、按发送人/群聊筛选、关联 Todo / 执行记录 |

---

## 2. 绑定 Bot（Device Flow）

> 入口：消息 → 绑定 Tab

### 2.1 准备飞书 App

1. 去 [飞书开放平台](https://open.feishu.cn) → 创建企业自建应用
2. 开启「机器人」能力
3. 在「权限管理」勾上 `im:message`、`im:message:send_as_bot`、`contact:user.id:readonly` 等
4. 记下 `App ID` 和 `App Secret`

### 2.2 在 ntd 配 Bot

1. 设置 → 消息 → 绑定 Tab
2. 填 App ID、App Secret
3. 点击「**初始化**」→ 后端写入 `agent_bots` 表
4. 点击「**开始绑定**」→ 后端生成 device flow 凭证
5. 弹出二维码 / 链接，去飞书里扫码授权
6. 页面会轮询「**检查状态**」直到 `access_token` 拿到
7. 状态变绿「已绑定」→ 自动 `start_bot`，Bot 上线

### 2.3 后端时序

```
用户点 init    → POST /api/agent-bots/feishu/init     → 写 agent_bots
用户点 begin   → POST /api/agent-bots/feishu/begin    → 拿 device_code + user_code
用户扫码授权
前端轮询 poll  → POST /api/agent-bots/feishu/poll     → 拿到 access_token + refresh_token
后端存 token   → agent_bots.bot_credentials
start_bot      → 启动 feishu_listener tokio task
```

---

## 3. 群白名单

> 控制哪些群里 @Bot 会被处理（避免在大型群里被刷屏）

### 3.1 新增白名单

1. 在「群白名单」子表里点「**新增**」
2. 填 `chat_id`（飞书群的唯一 ID，bot 加入群后从飞书事件里能看到）
3. 填 `chat_name`（备注名，方便识别）
4. 保存

### 3.2 何时生效

- Bot 收到消息时，listener 检查 `chat_id` 是否在白名单
- **不在白名单**的消息会被静默丢弃
- 「**群可见性**」开关关闭时，**所有群消息**都丢弃（仅单聊生效）

### 3.3 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/agent-bots/feishu/group-whitelist` | 列白名单 |
| POST | `/api/agent-bots/feishu/group-whitelist` | 新增 |
| DELETE | `/api/agent-bots/feishu/group-whitelist/{id}` | 移除 |

---

## 4. 推送配置

> 把 ntd 的事件主动推到飞书

入口：消息 → 绑定 Tab → 底部「**推送配置**」卡片

### 4.1 可推送的事件

- Todo 状态变化（pending → running → completed/failed）
- 执行记录的输出/统计
- 关键错误告警

### 4.2 推送目标

- 默认所有白名单群
- 可指定单聊（用 `user_open_id`）

### 4.3 API

| Method | Path |
|--------|------|
| GET | `/api/agent-bots/feishu/push` |
| PUT | `/api/agent-bots/feishu/push` |

`PUT` body 示例：
```json
{
  "enabled": true,
  "target_type": "group",
  "target_ids": ["oc_xxxxx", "oc_yyyyy"]
}
```

---

## 5. 历史消息（RecordTab）

> 入口：消息 → 历史 Tab

### 5.1 视图

- **聊天列表**：左侧列出已知 chat（来自接收到的消息）
- **消息流**：右侧展示该聊天的消息，按时间倒序
- **筛选**：
  - 按发送人（`sender_open_id` + `sender_name`）
  - 按时间范围
  - 关键字搜索（消息内容）
- **关联查看**：点消息右上角「查看 Todo」可跳到对应 Todo / 执行记录

### 5.2 后端机制

- 启动时 spawn `feishu_history_fetcher`：定期从飞书拉历史消息写入 `feishu_history_messages`
- 同时按 `chat_id` 聚合到 `feishu_history_chats`
- `message_debounce` 服务：5 分钟内同一群的高频消息做去重，避免刷屏

### 5.3 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/feishu/history-messages` | 消息分页（带 chat_id 筛选） |
| GET | `/api/feishu/message-stats` | 消息统计（按日/周/月） |
| GET | `/api/feishu/senders` | 去重的发送人列表 |
| GET | `/api/feishu/history-chats` | 聊天列表 |
| POST | `/api/feishu/history-chats` | 手工建聊天 |
| DELETE | `/api/feishu/history-chats/{id}` | 删除 |
| PUT | `/api/feishu/history-chats/{id}` | 编辑 |

---

## 6. 故障排查

### 6.1 二维码点了无反应

- 检查回调地址是否在飞书后台的「事件订阅」里配对
- 检查 ntd 后端 `/api/agent-bots/feishu/poll` 返回的 `status`：`pending` / `authorized` / `expired`
- 过期需要重新「begin」

### 6.2 Bot 收不到消息

- 看 `feishu_history_messages` 表有没有写入
- 检查「群白名单」是否含目标 chat_id
- 后端日志搜 `feishu-listener` 关键字

### 6.3 推送报错「权限不足」

飞书后台缺权限。`Bot` 需要：
- `im:message` — 收消息
- `im:message:send_as_bot` — 发消息
- `im:message.group_at_msg` — 收 @ 消息
- `contact:user.id:readonly` — 拿发送人 ID

---

## 7. 卸载 Bot

1. 设置 → 消息 → 绑定 Tab → 底部「**删除 Bot**」
2. 后端会 stop 监听任务并删 `agent_bots` 记录
3. 历史消息会保留（除非你手工清）

API：`DELETE /api/agent-bots/{id}`
