# Webhook

> **位置**：设置 → Webhook
> **前端**：`frontend/src/components/WebhooksPanel.tsx`
> **后端**：`backend/src/handlers/webhook.rs`

Webhook 让外部系统（CI、监控、GitHub Actions 等）通过 HTTP调用触发 ntd的 Todo执行。

---

## 1.关键概念

|概念 |含义 |
|------|------|
| **Webhook记录** | 一个命名的触发器，绑定一个 `default_todo_id` |
| **触发 URL** | `GET/POST /webhook/trigger/{todo_id}` — 注意 todo_id放在路径里 |
| **调用记录** |每次触发的请求/响应快照，存 `webhook_records` 表 |

> **设计取舍**：触发 URL把 `todo_id`放在路径上（而不是「找最近启用的 webhook」），避免多 webhook 并发时的竞态（issue295教训）。

---

## 2.创建 Webhook

### 2.1入口

设置 → Webhook →右上「**+ 新建**」

### 2.2字段（实际只有3 个）

`backend/src/handlers/webhook.rs::CreateWebhookRequest`：

|字段 |含义 |示例 |
|------|------|------|
| `name` | 显示名 |「GitHub PR触发器」 |
| `enabled` | 是否启用 | true |
| `default_todo_id` |触发后执行的 Todo |5 |

>文档之前提到的 `description` / `method_restrict` / `header_rules` / `rate_limit_per_min` 等字段在当前代码中**不存在**，未持久化。

---

## 3.触发 Webhook

### 3.1触发 URL

```
http://localhost:18088/webhook/trigger/{todo_id}
```

- `todo_id` 是要执行的 Todo 的 ID
-支持 GET和 POST，**无鉴权**（用 `todo_id`路径参数当"显式"鉴权）
- 如果 todo_id 没绑定的 enabled webhook：返回 **`400 BadRequest`**（**不是**404，参见 `webhook.rs::trigger_webhook_with_todo`）

### 3.2触发后会发生什么

1.查找 `default_todo_id == todo_id && enabled == true` 的 webhook
2.异步执行该 Todo
3.同步返回 **`200 OK`** + `{"success": true, "record_id": <id>}`
 -执行失败时返回 **`500 Internal Server Error`** + `{"success": false, "error": "Internal server error"}`
4. 调用记录写入 `webhook_records` 表

### 3.3触发示例

```bash
# 触发 todo5
curl http://localhost:18088/webhook/trigger/5

#  POST带 body（body 会作为额外参数传给执行器）
curl -X POST -H "Content-Type: application/json" \
 -d '{"branch": "main"}' \
 http://localhost:18088/webhook/trigger/5
```

---

## 4.调用记录

### 4.1查看

设置 → Webhook →选 webhook →「**调用记录**」Tab

每条记录展示：

|列 |含义 |
|----|------|
| 时间 |触发时间 |
|方法 | GET /POST / ... |
|路径 |请求路径 |
| Query |查询参数 |
| Body | 请求体（截断） |
| Content-Type | 请求头 |
|状态码 | HTTP响应码 |
|响应体 |响应截断 |
|触发的 Todo |关联的 todo_id |

### 4.2排查

- 看响应码：`200` =成功；`400` = 没找到匹配的 enabled webhook；`500` = 执行失败
- 看 body截断：检查参数是否解析
- 看响应体：ntd后端的 stdout（如果执行器在 body里报错）

---

## 5.内网穿透

外网触发需要 ntd能被公网访问。仓库根目录有 `tunnel.sh`：

```bash
./tunnel.sh #启动内网穿透
```

跑起来后输出一个公网 URL，把那个 URL + `/webhook/trigger/{todo_id}` 给外网系统用。

---

## 6.故障排查

### 6.1触发400

- todo_id 没绑定的 enabled webhook
- 检查 todo_id 是否正确（数字）
-检查 Webhook 的 `enabled`开关

### 6.2触发500

- 后端执行失败
- 看 Todo详情 → 执行记录，是否有新的
- 看后端日志 `execution::execute_handler`关键字

### 6.3触发没收到响应

- ntd 服务没运行：`ntd daemon status`
- 公网访问：检查 `tunnel.sh` 是否起来

### 6.4 header校验失败

- 当前实现**不**校验 header；如果将来加 `header_rules`字段，需要确保 key 大小写一致
-飞书/GitHub 的 webhook头可能不固定，建议用 IP白名单（如果有 reverse proxy）

---

## 7.安全建议

1. **最小暴露**：只暴露 `/webhook/trigger/*`，别把 `/api/*`暴露公网（ntd 没有完整鉴权）
2. **HTTPS**：套在 nginx/caddy后面开 TLS
3. **审计**：定期看调用记录，发现异常 IP立即 disable

---

## 8.相关 API

| Method | Path |用途 |
|--------|------|------|
| GET | `/api/webhooks` |列所有 |
| POST | `/api/webhooks` |新建 |
| GET | `/api/webhooks/{id}` |详情 |
| PUT | `/api/webhooks/{id}` |改 |
| DELETE | `/api/webhooks/{id}` |删 |
| GET | `/api/webhook-records` |调用记录 |
| GET | `/api/webhook-records/{id}` |单条记录 |
| GET/POST | `/webhook/trigger/{todo_id}` |**外网触发** |
