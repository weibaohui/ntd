# 模板管理

> **位置**：设置 → 模板管理
> **前端**：`frontend/src/components/settings/TemplatesPanel.tsx`
> **后端**：`backend/src/handlers/todo_template.rs`（本地）+ `backend/src/handlers/custom_template.rs`（远程）

ntd 模板分两类：
- **本地模板**：你自己在 ntd 里创建的 Todo 模板
- **远程自定义模板**：你订阅的远端 URL 上的模板（定期拉取）

---

## 1. 本地模板

### 1.1 概念

模板 = 预填好 title / prompt / executor / scheduler 的「Todo 模板」。新建 Todo 时点「**从模板**」一键套用。

### 1.2 操作

入口：模板管理 → 顶部「**本地模板**」子 Tab

| 操作 | 入口 |
|------|------|
| 新建 | 右上「+ 新建模板」 |
| 编辑 | 列表点「编辑」 |
| 删除 | 列表点「删除」 |
| 复制 | 列表点「复制」 → 改个名 → 另存 |

### 1.3 模板字段

| 字段 | 含义 |
|------|------|
| `name` | 模板名（必填） |
| `title` | 套用时填的 Todo 标题前缀 |
| `prompt` | 套用时填的 prompt 全文 |
| `executor` | 默认执行器 |
| `tags` | 默认标签 |
| `scheduler` | 默认定时配置 |
| `hooks` | 默认前后置 hook（参考 [Hook 系统设计](../../../hook-system-design.md)） |

### 1.4 API

| Method | Path |
|--------|------|
| GET | `/api/todo-templates` |
| POST | `/api/todo-templates` |
| PUT | `/api/todo-templates/{id}` |
| DELETE | `/api/todo-templates/{id}` |
| POST | `/api/todo-templates/{id}/copy` |

---

## 2. 远程自定义模板

> 入口：模板管理 → 底部「**自定义模板**」子 Tab

### 2.1 概念

- 一个远端 URL 提供模板（YAML/JSON 格式）
- ntd 定期去那个 URL 拉取并解析
- 拉下来的模板会出现在「**可用模板**」列表
- 点「**订阅**」才会真正纳入你的模板库

### 2.2 订阅流程

1. 点「**+ 添加订阅 URL**」
2. 填 URL（HTTP/HTTPS）
3. 后端 `POST /api/custom-templates/subscribe` 立即拉一次
4. 拉取的模板出现在「可用模板」列表
5. 勾选想要的模板 → 「**订阅**」

### 2.3 自动同步

| 配置 | 默认 | 含义 |
|------|------|------|
| `enabled` | false | 是否开启 |
| `cron` | `0 0 4 * * *` | 每天凌晨 4 点拉一次 |

开启后 ntd 定时去订阅的 URL 拉最新模板，可避免手动刷新。

### 2.4 SSRF 防御（重要）

ntd 后端做了一层 SSRF 防御：
- **拒绝**的地址：localhost / 127.0.0.0/8 / 0.0.0.0 / `::1` / 10.0.0.0/8 / 172.16.0.0/12 / 192.168.0.0/16
- 也就是说：**不能订阅内网地址**的模板源

如果你要在本地测试：
- 把 URL 填成公网地址（即使是 file:// 也不行）
- 或者把 SSRF 防御关掉（不推荐生产）

### 2.5 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/custom-templates/status` | 看订阅列表 + 拉取状态 |
| POST | `/api/custom-templates/subscribe` | 立即拉一次（存到 available 列表） |
| POST | `/api/custom-templates/unsubscribe` | 取消订阅 |
| POST | `/api/custom-templates/sync` | 把 available 列表里勾选的同步到本地 |
| PUT | `/api/custom-templates/auto-sync` | 改自动同步策略 |

---

## 3. 模板订阅源格式（YAML 示例）

```yaml
version: "1.0"
templates:
  - name: "Code Review"
    title: "Code Review: "
    prompt: |
      请帮我 review 以下 PR 的代码变更，重点关注：
      1. 是否有 bug
      2. 是否有性能问题
      3. 是否符合项目规范
    executor: claudecode
    tags: ["review", "code-quality"]
  - name: "Bug Fix"
    title: "Fix Bug: "
    prompt: |
      请修复以下 bug：
      {bug_description}
    executor: claudecode
    tags: ["bug"]
```

订阅后这些模板会出现在「可用模板」列表。

---

## 4. 故障排查

### 4.1 订阅失败「地址不在白名单」

- 你的 URL 是内网地址，被 SSRF 防御挡了
- 用公网 URL 测试

### 4.2 拉到的模板格式不对

- 检查 URL 返回的 Content-Type
- ntd 默认按 YAML 解析，JSON 也可以但字段名要兼容
- 看后端日志 `custom_template` 关键字

### 4.3 自动同步不触发

- 检查 `auto_sync_enabled` 开关
- 检查 Cron 表达式
- 看后端日志

---

## 5. 最佳实践

1. **团队共享**：把团队常用的 Todo 模式建成模板，托管在公司 git repo，ntd 订阅
2. **个人常用**：把「Code Review」「Bug Fix」「Refactor」等做成本地模板
3. **慎用 SSRF 防御绕开**：本地测试用 ngrok 等暴露成公网
4. **版本管理**：模板源用 git 维护，可以回滚
