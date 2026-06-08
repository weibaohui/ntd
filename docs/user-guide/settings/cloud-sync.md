# 云端同步

> **位置**：设置 → 云端同步（倒数第二个 tab）
> **前端**：`frontend/src/components/settings/CloudSyncPanel.tsx`
> **后端**：`backend/src/handlers/sync.rs`
> **API**：`/api/cloud/*`

云端同步让你在多台机器、多份 ntd 实例之间**双向同步 Todo**。数据走远端的 ntd-cloud 服务（你也可以自建），本地始终是 source of truth 的其中一份；通过「推送」把本地写上去，通过「拉取」把云端写下来。

---

## 1. 关键概念

| 概念 | 含义 |
|------|------|
| **server_url** | 云端 ntd-cloud 的 base URL，例如 `http://localhost:8089`。保存时自动去掉末尾 `/` |
| **sync_token** | Bearer Token，格式 `ntd_<uuid>`。后端只存「是否有」标志，**永远不返回明文**（丢失只能重写） |
| **推送（push）** | 本地 → 云端，把本地 todos 上传 |
| **拉取（pull）** | 云端 → 本地，把云端 todos 写进本地 DB（按冲突策略合并） |
| **冲突模式** | 拉取时本地已有同名 todo 怎么办：`overwrite` / `skip` / `rename` |
| **Dry Run** | 走完流程但不写库、不更新时间。常用于先看一眼「会发生什么」 |
| **同步历史** | 每次 push/pull 的结果记录，存 `sync_records` 表，可分页、可清空 |

---

## 2. 入口与配置

### 2.1 进入

打开 ntd → 右上角「⚙️」 → 选「**云端同步**」tab。

页面分为四块：
1. **顶部 Alert**：连接状态指示（绿/黄/蓝）
2. **配置表单**：服务器地址、Token、保存按钮
3. **推送/拉取按钮**：仅在已认证时显示
4. **同步历史表**：分页展示历史记录

### 2.2 获取 server_url 与 sync_token

如果你用的是 SaaS 版 ntd-cloud，去它的管理后台 → 设备列表 → 复制 base URL + API Token（`ntd_` 开头）。

如果你自建 ntd-cloud：去 `backend/src/handlers/token.rs::create` 注册新 token，或直接走 [飞书 Bot 绑定流程](messages-feishu.md) 同源的设备注册接口。

### 2.3 三种连接状态

| 状态 | 颜色 | 含义 |
|------|------|------|
| 已连接 + 已认证 | 🟢 绿 | server_url 已配、Token 已配。会主动 ping 云端拉真实 `last_sync_at` |
| 已连接但未配置 Token | 🟡 黄 | server_url 已配、Token 没配。**推送/拉取按钮不显示** |
| 未配置云端服务器地址 | 🔵 蓝 | server_url 空 |

### 2.4 填写并保存

1. 服务器地址：`http://localhost:8089`（末尾的 `/` 可加可不加）
2. 同步 Token：粘贴 `ntd_xxxx` 格式字符串
3. 点击「**保存配置**」
4. 顶部 Alert 变绿，出现「**推送**」「**拉取**」按钮 → 可以开始同步

---

## 3. 推送流程

1. 点击「**推送**」→ 弹 Modal「确认向上同步（推送至云端）」
2. 选择冲突策略（参考第 5 节）— 但 **推送场景下策略只在云端生效**，因为本地是 source of truth
3. 可选勾选「预览模式 (Dry Run)」
4. 点击「**执行同步**」
5. 弹出 toast：
   - 成功：「同步成功：推送 N 条」
   - 失败：「同步失败：<云端返回的 detail>」
6. 同步历史表自动刷新到当前页

### 推送数据流

```
本地 SQLite
   │ get_todos() + get_tags() 序列化
   ▼
CloudSyncData { version:1.0, todos, tags: [], skills: [] }
   │ serde_yaml 序列化为 YAML literal block
   ▼
POST {server_url}/api/v1/sync/push
   Header: Authorization: Bearer ntd_xxx
   Header: Content-Type: text/yaml
   Body:   data_type: todos
           conflict_mode: overwrite
           dry_run: false
           data: |
             version: "1.0"
             todos:
             - title: ...
   ▼
云端返回 YAML { success, summary: { new, overwritten, ... } }
```

> 当前**推送流只包含 todos**（参见 `backend/src/handlers/sync.rs::local_todos_to_cloud`：`tags: vec![]` / `skills: vec![]`硬编码为空）。tag / skill同步**尚未实现**，不要把 tag / skill改动推送到云端。


---

## 4. 拉取流程

1. 点击「**拉取**」→ 弹 Modal「确认向下同步（拉取至本地）」
2. 选择冲突策略（关键，决定同名 todo 怎么处理）
3. 可选勾选「预览模式 (Dry Run)」
4. 点击「**执行同步**」
5. 弹出 toast：
   - 成功：「同步成功：拉取 N 条」
   - 失败：展示云端返回的 detail

### 拉取合并逻辑（merge_cloud_todos_to_local）

匹配键：todo 的 `title.trim().to_lowercase()`（小写比较，忽略前后空格）。

| 策略 | 本地有同名 todo | 本地无同名 todo |
|------|----------------|----------------|
| **overwrite** | 用云端的 prompt/status/executor 覆盖本地 | 新增 |
| **skip** | 跳过这条云端 todo（**本地不变**） | 新增 |
| **rename** | 追加 ` (云端)` 后缀再插入（保留本地） | 新增 |
| 未知策略 | 走 skip 逻辑 | — |

> ⚠️ rename 策略会把「云端 title = 我的 todo」转成「我的 todo (云端)」再插入，避免冲突。如果反复 rename，同名云端 todo 在本地会以「xxx (云端)」「xxx (云端) (云端)」层层叠加 — 这是一个需要后续优化的边界。

---

## 5. 冲突策略详解

| 策略值 | 推送文案 | 拉取文案 | 适用场景 |
|--------|----------|----------|----------|
| `overwrite` | 覆盖（以本地为准，覆盖云端） | 覆盖（以云端为准，覆盖本地） | 你确定远端（或本地）是更新版本 |
| `skip` | 跳过（保留云端，忽略本地冲突项） | 跳过（保留本地，忽略云端冲突项） | 你不想丢失任何一边的内容 |
| `rename` | 重命名（本地项重命名保留） | 重命名（云端项重命名保留） | 两边内容都想要 |

**推荐组合**：
- 团队协作：pull 选 `overwrite` + 推送前先看 pull 状态，避免覆盖别人
- 个人多端：pull 选 `rename`，push 选 `overwrite`
- 不确定时：勾上「预览模式」先跑一遍

---

## 6. Dry Run（预览模式）

弹窗底部的 Checkbox「预览模式 (Dry Run)」。

- 走完完整流程：HTTP 请求照发、云端照解析
- **不写本地库**、**不更新 last_sync_at**
- 历史记录 `status` 标为 `dry_run`（表格里显示橙色「预览」标签）
- 推送时返回「推送 N 条」、拉取时返回「拉取 N 条」（N 为云端会影响的条数）

**何时用**：
- 第一次配云端服务器，不确定连通性
- 换了新策略，怕误删数据
- 跨大版本前想看看会同步多少

---

## 7. 同步历史

###7.1表格列

`backend/src/db/entity/sync_records.rs::Model`字段：

|列 |含义 |
|----|------|
| `id` |自增主键 |
| `direction` | `push` / `pull` |
| `conflict_mode` |本次同步用的冲突策略 |
| `data_type` |同步数据类型（当前固定为 `todos`） |
| `status` | `success` / `failed` / `dry_run` |
| `details` |JSON字符串，含 `pushed_count` / `pulled_count` / `dry_run`等 |
| `error_message` |失败时的错误描述（成功时为空） |
| `created_at` |同步发生时间（毫秒精度） |

### 7.2 分页

- 固定 10 条/页，无「每页大小」切换器
- 翻页时按需请求 `GET /api/cloud/sync/records?limit=10&offset=N`
- 后端并行返回 `total`，分页器显示「共 X 条」

### 7.3 清空历史

Divider 右侧红字「**清空**」按钮（无记录时禁用）：
1. 点击 → 弹确认框（显示当前共 X 条）
2. 二次确认 → 调 `DELETE /api/cloud/sync/records`
3. 后端 `sync_records::Entity::delete_many()` 物理删除
4. 回到第 1 页重新加载

> ⚠️ 清空是**物理删除**，不可恢复。但只清历史，不影响云端/本地的 Todo 本身。

---

## 8. 故障排查

### 8.1 一直转圈，最后「同步失败：timeout of 15000ms exceeded」

前端 axios 默认 15s 超时（`client.ts::api`），但 push/pull 调的是 `api.post(..., { timeout: 0 })`，应该不会被前端截断。

**真实原因**：后端打云端服务器时阻塞了。检查：
1. 云端服务器 (`server_url`) 是否在运行：`curl http://localhost:8089/health`
2. Token 是否过期：在 ntd-cloud 后台重新签发
3. 网络是否能通：本地能 `ping` 远端 IP 吗

### 8.2 同步失败：「请先配置同步 Token」

服务器地址填了但 Token 没填（表单 required 校验只挡前端，没填提交后端会拒绝）。重新保存 Token 即可。

### 8.3 拉取成功「拉取 5 条」但本地 Todo 没增加

**这是早期 bug（commit 92a200c 之前）**，`cloud_sync_pull` 只返回 `pulled_count` 但没真正写库。已修复，现在会按冲突策略合并。

如果你看不到新增 todo：
1. 检查冲突策略：选 `overwrite` 时，如果本地**有同名 todo**，云端同名项不会新增（会被覆盖）；想看新增，临时改用 `skip` 或 `rename`
2. 大小写差异：「Hello」和「hello」会被视为冲突（同 title 不区分大小写）

### 8.4 「Token 无效或已过期」（云端返回）

云端 ntd-cloud 校验 `ntd_xxx` token 时，先按 JWT 解码，失败再按 `DefaultHasher` 计算哈希查 `api_tokens` 表。**确保 Token 格式是 `ntd_<uuid>`，且数据库里有对应哈希**。

如果你手工造 token 测：用 Rust 代码生成（`DefaultHasher::finish()` 是 SipHash-1-0，不是常见的 MD5/SHA256）。

### 8.5 推送后云端没出现

- 检查云端 `/api/v1/sync/push` 的实际逻辑（不在本仓库 `nothing-todo` 里，而在 `nothing-todo-cloud` 仓库）
- 看后端日志 `backend.dev.log` 中 `pull:` 或 `push:` 前缀的追踪行
- 用 `curl` 手动打云端，复现后端请求体（`text/yaml`、literal block 缩进 2 空格）

---

## 9. 安全 & 权限

- **Token 保护**：后端 `CloudConfigResponse` 只返 `has_token: bool`，不返明文。UI 显示为空是正常的（不是 bug）
- **明文存储**：`~/.ntd/config.dev.yaml` 里 `cloud_sync.sync_token` 字段是明文。机器共享时注意权限 `chmod 600`
- **HTTPS**：自建 ntd-cloud 时务必上 TLS，否则 Token 走明文 HTTP
- **SSRF**：ntd-cloud 端做了 SSRF 防御（拒绝 127/10/172.16/192.168 等），但本仓库 ntd 的 `server_url` 字段没限制，**别把不可信 URL 填进去**

---

## 10. 相关 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/cloud/config` | 获取当前配置（不含明文 token） |
| POST | `/api/cloud/config` | 保存 server_url / sync_token |
| GET | `/api/cloud/sync/status` | 连接状态 + last_sync_at |
| GET | `/api/cloud/sync/records?limit=&offset=` | 同步历史分页 |
| DELETE | `/api/cloud/sync/records` | 清空历史 |
| POST | `/api/cloud/sync/push?conflict_mode=&dry_run=` | 推送 |
| POST | `/api/cloud/sync/pull?conflict_mode=&dry_run=` | 拉取 |

后端代码位置：
- `backend/src/handlers/sync.rs` — 全部云端同步 handler
- `backend/src/db/sync_record.rs` — sync_records 表 CRUD
- `backend/src/db/entity/sync_records.rs` — 实体定义
