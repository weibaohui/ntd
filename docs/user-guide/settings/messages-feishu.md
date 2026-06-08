# 消息（飞书 Bot）

> **位置**：设置 →消息
> **前端**：`frontend/src/components/settings/MessagesPanel.tsx` + `messages/*`
> **后端**：`backend/src/handlers/agent_bot.rs`、`backend/src/handlers/feishu_history.rs`
> **相关服务**：`backend/src/services/feishu_listener`、`feishu_history_fetcher`、`feishu_push`、`message_debounce`

飞书 Bot 让 ntd 与飞书群/单聊**双向联动**：你在飞书里 @机器人说话，机器人把它转成 Todo；ntd里有任务进展，机器人把结果 push 到飞书。

---

## 1. 子 Tab概览

| 子 Tab |作用 |
|--------|------|
| **BindTab（绑定）** |配飞书 app、扫码 device flow、群白名单、推送开关 |
| **RecordTab（历史）** | 看历史消息、按发送人/群聊筛选、关联 Todo / 执行记录 |

---

## 2.绑定 Bot（Device Flow）

>入口：消息 →绑定 Tab

### 2.1准备飞书 App

1. 去 [飞书开放平台](https://open.feishu.cn) → 创建企业自建应用
2.开启「机器人」能力
3. 在「权限管理」勾上 `im:message`、`im:message:send_as_bot`、`contact:user.id:readonly` 等
4.记下 `App ID` 和 `App Secret`

### 2.2 在 ntd配 Bot

1. 设置 →消息 →绑定 Tab
2.填 App ID、App Secret
3. 点击「**初始化**」→ 后端写入 `agent_bots` 表
4. 点击「**开始绑定**」→ 后端生成 device flow凭证
5.弹出二维码 /链接，去飞书里扫码授权
6.页面会轮询「**检查状态**」直到 `access_token`拿到
7.状态变绿「已绑定」→ 自动 `start_bot`，Bot 上线

### 2.3 后端时序

```
用户点 init → POST /api/agent-bots/feishu/init →写 agent_bots
用户点 begin → POST /api/agent-bots/feishu/begin →拿 device_code + user_code
用户扫码授权
前端 SSE → GET /api/agent-bots/feishu/poll-stream →收 ping / result / fail事件
后端存 token → agent_bots.app_id / agent_bots.app_secret
start_bot →启动 feishu_listener tokio task
```

>事件订阅走 SSE，事件名只有3 个：`ping`（等待扫码心跳）、`result`（终态，`error`字段可能为 `access_denied` / `expired_token` / `timeout`）、`fail`（HTTP 或 JSON解析失败等非授权错误，参见 `agent_bot.rs::feishu_poll_sse`）。

---

## 3.群白名单

> 控制哪些群里 @Bot会被处理（避免在大型群里被刷屏）

### 3.1 新增白名单

1. 在「群聊响应白名单」里点「**添加**」
2.填 `sender_open_id`（发送人 open_id，从「历史消息」Tab 的发送人列表里复制）
3.填 `sender_name`（备注名，方便识别）
4. 保存

### 3.2何时生效

- Bot收到消息时，listener 检查 `sender_open_id` 是否在白名单
- **不在白名单**的发送人消息会被静默丢弃
- 「**群聊仅处理@**」开关关闭时，**所有群消息**都丢弃（仅单聊生效）
- Bind Tab 的「消息配置」区域共4 个独立开关（`dm_enabled` / `group_enabled` / `group_require_mention` / `echo_reply`），无单一「群可见性」开关

### 3.3 API

| Method | Path |用途 |
|--------|------|------|
| GET | `/api/agent-bots/feishu/group-whitelist?bot_id={id}` | 列白名单 |
| POST | `/api/agent-bots/feishu/group-whitelist` | 新增（body需带 `bot_id`） |
| DELETE | `/api/agent-bots/feishu/group-whitelist/{id}` |移除 |

---

## 4.推送配置

> 把 ntd 的事件主动推到飞书

入口：消息 →绑定 Tab →底部「**推送配置**」卡片

### 4.1 可推送的事件

- Todo状态变化（pending → running → completed/failed）
- 执行记录的输出/统计
-关键错误告警

### 4.2推送目标

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
 "bot_id":1,
 "push_level": "result_only",
 "p2p_receive_id": "ou_xxxxx",
 "group_chat_id": "oc_xxxxx",
 "receive_id_type": "open_id",
 "p2p_response_enabled": true,
 "group_response_enabled": true,
 "p2p_debounce_secs":20,
 "group_debounce_secs":20
}
```

字段说明：
- `bot_id`：要更新的 Bot ID（必填）
- `push_level`：`disabled` / `result_only` / `all`
- `receive_id_type`：`open_id`（私聊）/ `chat_id`（群聊）
- 其余字段参考 Bind Tab 上的「推送目标」与「单聊/群聊响应」控件

---

## 5. 历史消息（RecordTab）

>入口：消息 → 历史 Tab

### 5.1视图

- **聊天列表**：左侧列出已知 chat（来自接收到的消息）
- **消息流**：右侧展示该聊天的消息，按时间倒序
- **筛选**：
 - 按发送人（`sender_open_id` + `sender_name`）
 - 按时间范围
 -关键字搜索（消息内容）
- **关联查看**：点消息右上角「查看 Todo」可跳到对应 Todo / 执行记录

### 5.2 后端机制

-启动时 spawn `feishu_history_fetcher`：定期从飞书拉历史消息写入 `feishu_history_messages`
- 同时按 `chat_id`聚合到 `feishu_history_chats`
- `message_debounce` 服务：默认20 秒内同一群的高频消息做去重（可在 Bind Tab 的「单聊响应 /群聊响应」开关旁调整合并秒数），避免刷屏

### 5.3 API

| Method | Path |用途 |
|--------|------|------|
| GET | `/api/feishu/history-messages` |消息分页（带 chat_id筛选） |
| GET | `/api/feishu/message-stats` |消息统计（按日/周/月） |
| GET | `/api/feishu/senders` | 去重的发送人列表 |
| GET | `/api/feishu/history-chats` |聊天列表 |
| POST | `/api/feishu/history-chats` |手工建聊天 |
| DELETE | `/api/feishu/history-chats/{id}` | 删除 |
| PUT | `/api/feishu/history-chats/{id}` | 编辑 |

> `feishu_messages` 表只用 `processed`字段标识处理状态（`true` = 已处理成功，`false` = 处理失败待重试），**没有** `failed`字段。处理失败时 `processed`会被重置为 `false`（详见 `backend/src/db/feishu_message.rs::mark_feishu_message_failed`）。

---

## 6.故障排查

### 6.1 二维码点了无反应

- 检查回调地址是否在飞书后台的「事件订阅」里配对
- 检查 ntd 后端 `/api/agent-bots/feishu/poll-stream` SSE 的事件名：`ping` / `result` / `fail`
-过期需要重新「begin」

### 6.2 Bot收不到消息

- 看 `feishu_history_messages` 表有没有写入
- 检查「群聊响应白名单」是否含目标 `sender_open_id`
- 后端日志搜 `feishu-listener`关键字

### 6.3推送报错「权限不足」

飞书后台缺权限。`Bot` 需要：
- `im:message` —收消息
- `im:message:send_as_bot` — 发消息
- `im:message.group_at_msg` —收 @消息
- `contact:user.id:readonly` —拿发送人 ID

---

## 7.卸载 Bot

1. 设置 →消息 →绑定 Tab →底部「**删除 Bot**」
2. 后端会 stop监听任务并删 `agent_bots`记录
3. 历史消息会保留（除非你手工清）

API：`DELETE /api/agent-bots/{id}`

> 级联清理的相关子表共6 个（不要凭直觉假设）：
> - `feishu_messages`
> - `feishu_push_targets`
> - `feishu_homes`
> - `feishu_response_config`
> - `feishu_group_whitelist`
> - `feishu_history_chats`
>
> **没有** `feishu_message_stats` / `feishu_debounce_state` 等子表。
