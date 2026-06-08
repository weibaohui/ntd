# 模板管理

> **位置**：设置 →模板管理
> **前端**：`frontend/src/components/settings/TemplatesPanel.tsx`
> **后端**：`backend/src/handlers/todo_template.rs`（本地）+ `backend/src/handlers/custom_template.rs`（远程）

ntd模板分两类：
- **本地模板**：你自己在 ntd里创建的 Todo模板
- **远程自定义模板**：你订阅的远端 URL 上的模板（定期拉取）

---

## 1.本地模板

### 1.1概念

模板 =预填好 title / prompt 的「Todo模板」。新建 Todo时点「**从模板**」一键套用。

### 1.2操作

入口：模板管理 →顶部「**本地模板**」子 Tab

|操作 |入口 |
|------|------|
| 新建 |右上「+ 新建模板」 |
|编辑 |列表点「编辑」 |
|删除 |列表点「删除」 |
|复制 |列表点「复制」→改个名 →另存 |

### 1.3模板字段

实际字段定义在 `backend/src/models/mod.rs::TodoTemplate`：

|字段 |含义 |
|------|------|
| `title` |套用时填的 Todo标题 |
| `prompt` |套用时填的 prompt全文 |
| `category` |分类（如「代码审查」「Bug Fix」） |
| `sort_order` |排序（数字小者靠前） |
| `id` |内部 ID（只读） |
| `is_system` | 是否系统模板（只读） |
| `source_url` | 来源 URL，订阅的远程模板才有（只读） |
| `last_sync_at` | 最后同步时间（只读） |
| `created_at` | 创建时间（只读） |
| `updated_at` | 更新时间（只读） |

>历史上曾有 `name` / `executor` / `tags` / `scheduler` / `hooks` 等字段，**当前模型已不存储**这些。

### 1.4 API

| Method | Path |
|--------|------|
| GET | `/api/todo-templates` |
| POST | `/api/todo-templates` |
| PUT | `/api/todo-templates/{id}` |
| DELETE | `/api/todo-templates/{id}` |
| POST | `/api/todo-templates/{id}/copy` |

---

## 2.远程自定义模板

>入口：模板管理 →底部「**自定义模板**」子 Tab

### 2.1概念

- 一个远端 URL提供模板（YAML格式）
- ntd定期去那个 URL拉取并解析
-拉下来的模板会出现在「**可用模板**」列表（实际就是订阅后写入到本地 `todo_templates` 表，标 `is_system=false` / `source_url=URL`）

### 2.2订阅流程

1. 点「**+ 添加订阅 URL**」
2.填 URL（HTTP/HTTPS）
3.后端 `POST /api/custom-templates/subscribe`立即拉一次
4.拉取的模板**全量替换**旧模板（详见2.5）

>订阅接口没有「勾选」交互：每次订阅都是「**重新拉取订阅 URL 的全部模板，删除旧的并插入新模板**」。

### 2.3自动同步

|配置 |默认 |含义 |
|------|------|------|
| `enabled` | false | 是否开启 |
| `cron` | `004 * * *` |每天4 点拉一次 |

开启后 ntd定时去订阅的 URL拉最新模板，可避免手动刷新。

### 2.4 SSRF防御

ntd 后端硬编码了一层 SSRF防御（`backend/src/handlers/custom_template.rs::is_private_host`）：

- **拒绝**的地址：localhost /127.0.0.0/8 /0.0.0.0 / `::1` /10.0.0.0/8 /172.16.0.0/12 /192.168.0.0/16
-也就是说：**不能订阅内网地址**的模板源

>无法关闭该防御（没有对应配置项，硬编码检查）。如需在内网测试，可用 ngrok 把内网地址暴露成公网。

### 2.5 API

| Method | Path |用途 |
|--------|------|------|
| GET | `/api/custom-templates/status` | 看订阅列表 +拉取状态 |
| POST | `/api/custom-templates/subscribe` |立即拉一次（存到模板表） |
| POST | `/api/custom-templates/unsubscribe` |取消订阅 |
| POST | `/api/custom-templates/sync` | 把订阅 URL的模板重新拉一遍并替换本地 |
| PUT | `/api/custom-templates/auto-sync` |改自动同步策略 |

GET `/api/custom-templates/status` 返回结构：
```json
{
 "subscribed":true,
 "source_url": "https://example.com/templates.yaml",
 "last_sync_at": "2026-06-04T20:00:00Z",
 "auto_sync_enabled":false,
 "auto_sync_cron": "004 * * *",
 "templates": [
 { "id":1, "title": "Code Review", "prompt": "...", "category": "...",
 "is_system":false, "source_url": "https://...", "last_sync_at": "...",
 "created_at": "...", "updated_at": "..." }
 ]
}
```

---

## 3.模板订阅源格式（YAML示例）

实际只接受3 个字段（`backend/src/handlers/custom_template.rs::RemoteTemplate`）：

```yaml
- title: "Code Review"
 prompt: |
 请帮我 review 以下 PR 的代码变更，重点关注：
1.是否有 bug
2.是否有性能问题
3. 是否符合项目规范
 category: "代码审查"
- title: "Bug Fix"
 prompt: |
 请修复以下 bug：
 {bug_description}
 category: "Bug Fix"
```

>源文件可以是 YAML数组（多个模板），也可以是单个对象。`category`缺省时填「自定义」。其他字段（如 `executor` / `tags` 等）会被**忽略**。

---

## 4.故障排查

### 4.1订阅失败「Private/internal hosts are not allowed」

-你的 URL是内网地址，被 SSRF防御挡了
-用公网 URL测试

### 4.2拉到的模板格式不对

- 检查 URL返回的 Content-Type
- ntd 默认按 YAML解析
- 看后端日志 `custom_template`关键字

### 4.3自动同步不触发

- 检查 `auto_sync_enabled`开关
- 检查 Cron表达式
-看后端日志

---

## 5.最佳实践

1. **团队共享**：把团队常用的 Todo模式建成模板，托管在公司 git repo，ntd订阅
2. **个人常用**：把「Code Review」「Bug Fix」「Refactor」等做成本地模板
3. **慎用 SSRF防御绕开**：本地测试用 ngrok 等暴露成公网
4. **版本管理**：模板源用 git维护，可以回滚
