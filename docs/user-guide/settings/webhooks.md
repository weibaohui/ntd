# Webhook

> **位置**：设置 → Webhook
> **前端**：`frontend/src/components/WebhooksPanel.tsx`
> **后端**：`backend/src/handlers/webhook.rs`

Webhook 让外部系统（CI、监控、GitHub Actions 等）通过 HTTP 调用触发 ntd 的 Todo 执行。

---

## 1. 关键概念

| 概念 | 含义 |
|------|------|
| **Webhook 记录** | 一个命名的触发器，绑定一个 `default_todo_id` |
| **触发 URL** | `GET/POST /webhook/trigger/{todo_id}` — 注意 todo_id 放在路径里 |
| **调用记录** | 每次触发的请求/响应快照，存 `webhook_records` 表 |

> **设计取舍**：触发 URL 把 `todo_id` 放在路径上（而不是「找最近启用的 webhook」），避免多 webhook 并发时的竞态（issue 295 教训）。

---

## 2. 创建 Webhook

### 2.1 入口

设置 → Webhook → 右上「**+ 新建**」

### 2.2 必填字段

| 字段 | 含义 | 示例 |
|------|------|------|
| `name` | 显示名 | 「GitHub PR 触发器」 |
| `default_todo_id` | 触发后执行的 Todo | 5 |
| `enabled` | 是否启用 | true |

### 2.3 可选字段

| 字段 | 含义 |
|------|------|
| `description` | 备注 |
| `method_restrict` | 限制允许的方法（GET/POST/PUT/DELETE，留空 = 全部允许） |
| `header_rules` | 必带 header 校验（JSON，例如 `{"X-Token": "secret"}`） |
| `rate_limit_per_min` | 每分钟限流次数 |

---

## 3. 触发 Webhook

### 3.1 触发 URL

```
http://localhost:18088/webhook/trigger/{todo_id}
```

- `todo_id` 是要执行的 Todo 的 ID
- 支持 GET 和 POST，**无鉴权**（用 `todo_id` 路径参数当"显式"鉴权）
- 如果 webhook 是 disabled 状态：返回 404

### 3.2 触发后会发生什么

1. 验证 webhook 存在 + enabled
2. 验证 method / header（如果配了）
3. 验证 rate limit
4. 异步执行 `todo_id` 对应的 Todo（不阻塞 HTTP 响应）
5. 立即返回 202 Accepted + `{"triggered": true, "todo_id": 5}`
6. 异步记录到 `webhook_records`

### 3.3 触发示例

```bash
# 触发 todo 5
curl http://localhost:18088/webhook/trigger/5

# 带 header 校验
curl -H "X-Token: secret" http://localhost:18088/webhook/trigger/5

# POST 带 body（body 会作为额外参数传给执行器）
curl -X POST -H "Content-Type: application/json" \
  -d '{"branch": "main"}' \
  http://localhost:18088/webhook/trigger/5
```

---

## 4. 调用记录

### 4.1 查看

设置 → Webhook → 选 webhook → 「**调用记录**」Tab

每条记录展示：

| 列 | 含义 |
|----|------|
| 时间 | 触发时间 |
| 方法 | GET / POST / ... |
| 路径 | 请求路径 |
| Query | 查询参数 |
| Body | 请求体（截断） |
| Content-Type | 请求头 |
| 状态码 | HTTP 响应码 |
| 响应体 | 响应截断 |
| 触发的 Todo | 关联的 todo_id |

### 4.2 排查

- 看响应码：404 = webhook 不存在或 disabled；429 = rate limit
- 看 body 截断：检查参数是否解析
- 看响应体：ntd 后端的 stdout（如果执行器在 body 里报错）

---

## 5. 内网穿透

外网触发需要 ntd 能被公网访问。仓库根目录有 `tunnel.sh`：

```bash
./tunnel.sh   # 启动内网穿透
```

跑起来后输出一个公网 URL，把那个 URL + `/webhook/trigger/{todo_id}` 给外网系统用。

---

## 6. 故障排查

### 6.1 触发 404

- Webhook 不存在（todo_id 没绑）
- Webhook 被禁用
- 检查 todo_id 是否正确（数字）

### 6.2 触发 200 但 Todo 没跑

- 后端返回 202 是「已接收，异步执行」—— 这是正常的
- 看 Todo 详情 → 执行记录，是否有新的
- 看后端日志 `execution::execute_handler` 关键字

### 6.3 触发 429

- rate_limit_per_min 限流到了
- 等 1 分钟，或调高限流

### 6.4 header 校验失败

- 配的 `header_rules` 校验：检查 key 大小写、`X-Token` vs `x-token` 是否一致
- 飞书/GitHub 的 webhook 头可能不固定，建议**不配** header 规则，用 IP 白名单（如果有 reverse proxy）

---

## 7. 安全建议

1. **最小暴露**：只暴露 `/webhook/trigger/*`，别把 `/api/*` 暴露公网（ntd 没有完整鉴权）
2. **header 校验**：生产环境必加，secret 用 `openssl rand -hex 32` 生成
3. **限流**：rate_limit_per_min 设个保守值（如 10/分钟）
4. **HTTPS**：套在 nginx/caddy 后面开 TLS
5. **审计**：定期看调用记录，发现异常 IP 立即 disable

---

## 8. 相关 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/webhooks` | 列所有 |
| POST | `/api/webhooks` | 新建 |
| GET | `/api/webhooks/{id}` | 详情 |
| PUT | `/api/webhooks/{id}` | 改 |
| DELETE | `/api/webhooks/{id}` | 删 |
| GET | `/api/webhook-records` | 调用记录 |
| GET | `/api/webhook-records/{id}` | 单条记录 |
| GET/POST | `/webhook/trigger/{todo_id}` | **外网触发** |
