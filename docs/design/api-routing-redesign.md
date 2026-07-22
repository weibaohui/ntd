# API 路由重构设计方案：K8s 风格 Workspace 层级嵌入

> 版本：v1
> 日期：2026-07-22
> 状态：已定稿

---

## 1. 设计目标

### 1.1 核心原则

```
/api/{version}/workspaces/{workspace_id}/{resource}[/{resource_id}[/{sub_resource}[/{sub_resource_id}]]]
```

| 层级 | 示例 | 说明 |
|------|------|------|
| 版本前缀 | `/api/v1/` | 统一版本管理，预留向后兼容 |
| Workspace 归集 | `/workspaces/{id}/` | 所有域资源的第一级路径，K8s namespace 风格 |
| 资源 | `todos/`、`loops/`、`executions/` | 复数名词，统一风格 |
| 资源 ID | `{id}` | 整数或字符串标识 |
| 子资源 | `/tags`、`/steps`、`/executions` | 所属关系清晰的嵌套 |
| 子资源 ID | `{tid}`、`{sid}`、`{eid}` | 子资源标识 |
| Action | `/{id}/stop`、`/{id}/archive` | POST 动词作为子路径 |

### 1.2 变换规则一览

| 维度 | 旧风格 | 新风格 |
|------|--------|--------|
| Workspace 传递 | query param `?workspace_id=N` | 路径参数 `/workspaces/{ws}/` |
| 单复数 | `workspace`（单）与 `workspaces`（复）混用 | 统一 `workspaces`（复） |
| 版本号 | 无 | `/api/v1/` |
| 执行操作 | `POST /api/execute` | `POST /api/v1/workspaces/{ws}/executions` |
| 执行控制 | `POST /api/execute/stop` | `POST /api/v1/workspaces/{ws}/executions/{id}/stop` |
| 批量操作 | `PUT /api/todos/batch-workspace` | `POST /api/v1/workspaces/{ws}/todos/batch/move-workspace` |
| Webhook 前缀 | `/webhook/trigger/...` | `/api/v1/webhooks/...` |
| 专家创建 | `POST /api/experts/create`（与 `{name}` 冲突） | `POST /api/v1/experts` |

---

## 2. 当前 API 现状分析

### 2.1 体量

后端约 **170 个 API 端点**，分布在 24 个领域子路由函数中。前端通过集中式 axios 客户端调用，少数场景用原生 fetch（文件下载/YAML 导出）。

### 2.2 RESTful 程度评估

#### ✅ 做得好的

- **CRUD 模式规范**：Todo、Loops、Sessions、Tags 等核心资源用了 `GET/POST/PUT/DELETE /api/resource/{id}` 模式
- **子资源嵌套正确**：如 `/api/loops/{id}/triggers/{tid}`、`/api/loops/{id}/executions/{eid}`
- **HTTP 方法语义正确**：GET 查、POST 创建、PUT 全量更新、DELETE 删除

#### ❌ 核心问题

**问题 1：Workspace ID 嵌入方式三套混用**

| 位置 | 示例 | 风格 |
|------|------|------|
| 黑板/Wiki | `/api/workspaces/{ws}/blackboard` | 复数+路径参数 ✅ |
| 斜杠命令/设置 | `/api/workspace/{ws}/slash-commands` | 单数 `workspace` ❌ |
| 绝大多数资源 | `/api/todos?workspace_id=xxx` | 扁平，workspace 是 query param ❌ |

**问题 2：单复数不一致**：`workspaces`（blackboard）vs `workspace`（slash-commands/settings）

**问题 3：Actions 用动词路径而非 REST 风格**

```yaml
POST /api/execute              # → 应 POST /api/executions
POST /api/execute/stop         # → 应 POST /api/executions/{id}/stop
POST /api/smart-create         # → 应 POST .../todos/smart
POST /api/experts/create       # → 与 /experts/{name} 路由冲突
```

**问题 4：Batch 操作散落**：`/api/todos/batch-executor`、`/api/loops/batch-workspace` 等

**问题 5：Webhook 缺 `/api` 前缀**：`/webhook/trigger/todo/{id}` vs 其他 `/api/xxx`

**问题 6：缺少版本前缀**：所有路由裸 `/api/xxx`，无法平滑升级

### 2.3 资源归属现状

当前大部分资源是"半 workspace-scoped"状态——数据模型中包含 `workspace_id` 字段，但 URL 中没有体现：

| 资源 | 模型中含 ws_id | URL 中有 ws 层级 |
|------|:---:|:---:|
| todos | ✅ | ❌ query param |
| loops | ✅ | ❌ query param |
| tags | ✅ | ❌ |
| executions | ✅ | ❌ |
| blackboard | ✅ | ✅ `/api/workspaces/{ws}/blackboard` |
| wiki | ✅ | ✅ |
| slash-commands | ✅ | ✅ `/api/workspace/{ws}/slash-commands` |
| settings | ✅ | ✅ |

---

## 3. 资源归属矩阵

### 3.1 Workspace 域资源

| 资源 | 新路径前缀 | 说明 |
|------|-----------|------|
| todos | `.../workspaces/{ws}/todos` | 核心任务 |
| tags | `.../workspaces/{ws}/tags` | 标签（按项目隔离） |
| loops | `.../workspaces/{ws}/loops` | Loop Studio |
| executions | `.../workspaces/{ws}/executions` | 执行记录 |
| blackboard | `.../workspaces/{ws}/blackboard` | 黑板（已有，加版本号） |
| wiki | `.../workspaces/{ws}/wiki` | 项目 Wiki（已有，加版本号） |
| slash-commands | `.../workspaces/{ws}/slash-commands` | 斜杠命令（迁移进 v1 + 统一复数） |
| settings | `.../workspaces/{ws}/settings` | 工作空间设置（迁移进 v1 + 统一复数） |
| quick-buttons | `.../workspaces/{ws}/quick-buttons` | 快捷话术 |
| actions | `.../workspaces/{ws}/actions` | 自定义动作 |
| stats/dashboard | `.../workspaces/{ws}/stats/dashboard` | 项目维度的统计 |

### 3.2 全局资源（系统级）

| 资源 | 新路径前缀 | 说明 |
|------|-----------|------|
| config | `.../config` | 系统配置 |
| executors | `.../executors` | 执行器配置 |
| skills | `.../skills` | 技能管理 |
| sessions | `.../sessions` | 会话管理 |
| backup | `.../backup` | 备份与恢复 |
| version | `.../version` | 版本查询/升级 |
| experts | `.../experts` | 专家库 |
| bundled | `.../bundled` | 内置资源/技能市场 |
| todo-templates | `.../todo-templates` | 任务模板 |
| review-templates | `.../review-templates` | 评审模板 |
| custom-templates | `.../custom-templates` | 云端订阅模板 |
| agent-bots | `.../agent-bots` | Agent Bot 管理 |
| feishu | `.../feishu` | 飞书集成 |
| usage-stats | `.../usage-stats` | 用量统计 |
| cloud | `.../cloud` | 云端同步 |
| project-directories | `.../project-directories` | 项目目录 CRUD（workspace 本身） |
| webhooks | `.../webhooks` | Webhook 配置与触发 |
| events | `.../events` | WebSocket 事件流 |

---

## 4. 完整路径映射

> 约定：`{++}` = 新增路径，`{--}` = 废弃路径（旧路径代码一并删除），`{~}` = 路径不变/仅有版本号变更
>
> 路径参数标记：`{ws}` = workspace_id, `{id}` = 资源主键

### 4.1 Todo & Tag

| 方法 | 旧路径 | 新路径 |
|------|--------|--------|
| GET | `/api/todos`{--} | `/api/v1/workspaces/{ws}/todos`{++} |
| POST | `/api/todos`{--} | `/api/v1/workspaces/{ws}/todos`{++} |
| GET | `/api/todos/center`{--} | `/api/v1/workspaces/{ws}/todos/center`{++} |
| GET | `/api/todos/recent-completed`{--} | `/api/v1/workspaces/{ws}/todos/recent-completed`{++} |
| GET | `/api/todos/{id}`{--} | `/api/v1/workspaces/{ws}/todos/{id}`{++} |
| PUT | `/api/todos/{id}`{--} | `/api/v1/workspaces/{ws}/todos/{id}`{++} |
| DELETE | `/api/todos/{id}`{--} | `/api/v1/workspaces/{ws}/todos/{id}`{++} |
| PUT | `/api/todos/{id}/force-status`{--} | `/api/v1/workspaces/{ws}/todos/{id}/force-status`{++} |
| PUT | `/api/todos/{id}/tags`{--} | `/api/v1/workspaces/{ws}/todos/{id}/tags`{++} |
| PUT | `/api/todos/{id}/scheduler`{--} | `/api/v1/workspaces/{ws}/todos/{id}/scheduler`{++} |
| POST | `/api/todos/{id}/archive`{--} | `/api/v1/workspaces/{ws}/todos/{id}/archive`{++} |
| POST | `/api/todos/{id}/restore`{--} | `/api/v1/workspaces/{ws}/todos/{id}/restore`{++} |
| PUT | `/api/todos/{id}/webhook`{--} | `/api/v1/workspaces/{ws}/todos/{id}/webhook`{++} |
| GET | `/api/todos/{id}/summary`{--} | `/api/v1/workspaces/{ws}/todos/{id}/summary`{++} |
| GET | `/api/tags`{--} | `/api/v1/workspaces/{ws}/tags`{++} |
| POST | `/api/tags`{--} | `/api/v1/workspaces/{ws}/tags`{++} |
| DELETE | `/api/tags/{id}`{--} | `/api/v1/workspaces/{ws}/tags/{id}`{++} |

#### 批量操作收敛（方案 B：子资源）

| 旧路径 | 新路径 |
|--------|--------|
| `PUT /api/todos/batch-executor`{--} | `POST /api/v1/workspaces/{ws}/todos/batch/executor`{++} |
| `PUT /api/todos/batch-workspace`{--} | `POST /api/v1/workspaces/{ws}/todos/batch/workspace`{++} |
| `POST /api/todos/batch-copy-workspace`{--} | `POST /api/v1/workspaces/{ws}/todos/batch/copy-workspace`{++} |
| `PUT /api/todos/batch-scheduler`{--} | `POST /api/v1/workspaces/{ws}/todos/batch/scheduler`{++} |

### 4.2 Executions（执行与操作）

| 方法 | 旧路径 | 新路径 |
|------|--------|--------|
| POST | `/api/execute`{--} | `POST /api/v1/workspaces/{ws}/executions`{++} |
| POST | `/api/execute/stop`{--} | `POST /api/v1/workspaces/{ws}/executions/{id}/stop`{++} |
| POST | `/api/execute/force-fail`{--} | `POST /api/v1/workspaces/{ws}/executions/{id}/force-fail`{++} |
| POST | `/api/smart-create`{--} | `POST /api/v1/workspaces/{ws}/todos/smart`{++} |
| GET | `/api/execution-records`{--} | `GET /api/v1/workspaces/{ws}/executions`{++} |
| GET | `/api/execution-records/running`{--} | `GET /api/v1/workspaces/{ws}/executions/running`{++} |
| GET | `/api/execution-records/session/{session_id}`{--} | `GET /api/v1/workspaces/{ws}/executions/session/{session_id}`{++} |
| GET | `/api/execution-records/{id}`{--} | `GET /api/v1/workspaces/{ws}/executions/{id}`{++} |
| GET | `/api/execution-records/{id}/logs`{--} | `GET /api/v1/workspaces/{ws}/executions/{id}/logs`{++} |
| POST | `/api/execution-records/{id}/resume`{--} | `POST /api/v1/workspaces/{ws}/executions/{id}/resume`{++} |
| PUT | `/api/execution-records/{id}/rating`{--} | `PUT /api/v1/workspaces/{ws}/executions/{id}/rating`{++} |
| GET | `/api/running-board`{--} | `GET /api/v1/workspaces/{ws}/executions/running-board`{++} |
| GET | `/api/running-todos`{--} | `GET /api/v1/workspaces/{ws}/executions/running-todos`{++} |
| GET | `/api/dashboard-stats`{--} | `GET /api/v1/workspaces/{ws}/stats/dashboard`{++} |
| POST | `/api/actions/execute`{--} | `POST /api/v1/workspaces/{ws}/actions/execute`{++} |
| GET | `/api/scheduler/todos`{--} | `GET /api/v1/workspaces/{ws}/scheduler/todos`{++} |

> **命名解释**：
> - `executions` 替代 `execution-records`：更短，语义一致（记录即执行）
> - `smart-create` → `todos/smart`：它创建的是 todo，AI 辅助是创建方式
> - `dashboard-stats` → `stats/dashboard`：统计归 stats 空间，dashboard 是统计类型

### 4.3 Loops（Loop Studio）

| 方法 | 旧路径 | 新路径 |
|------|--------|--------|
| GET | `/api/loops`{--} | `GET /api/v1/workspaces/{ws}/loops`{++} |
| POST | `/api/loops`{--} | `POST /api/v1/workspaces/{ws}/loops`{++} |
| GET | `/api/loops/stats`{--} | `GET /api/v1/workspaces/{ws}/loops/stats`{++} |
| GET | `/api/loops/{id}`{--} | `GET /api/v1/workspaces/{ws}/loops/{id}`{++} |
| PUT | `/api/loops/{id}`{--} | `PUT /api/v1/workspaces/{ws}/loops/{id}`{++} |
| DELETE | `/api/loops/{id}`{--} | `DELETE /api/v1/workspaces/{ws}/loops/{id}`{++} |
| PUT | `/api/loops/{id}/status`{--} | `PUT /api/v1/workspaces/{ws}/loops/{id}/status`{++} |
| PUT | `/api/loops/{id}/tags`{--} | `PUT /api/v1/workspaces/{ws}/loops/{id}/tags`{++} |
| POST | `/api/loops/{id}/duplicate`{--} | `POST /api/v1/workspaces/{ws}/loops/{id}/duplicate`{++} |
| POST | `/api/loops/{id}/trigger`{--} | `POST /api/v1/workspaces/{ws}/loops/{id}/trigger`{++} |
| GET | `/api/loops/{id}/triggers`{--} | `GET /api/v1/workspaces/{ws}/loops/{id}/triggers`{++} |
| POST | `/api/loops/{id}/triggers`{--} | `POST /api/v1/workspaces/{ws}/loops/{id}/triggers`{++} |
| PUT | `/api/loops/{id}/triggers/{tid}`{--} | `PUT /api/v1/workspaces/{ws}/loops/{id}/triggers/{tid}`{++} |
| DELETE | `/api/loops/{id}/triggers/{tid}`{--} | `DELETE /api/v1/workspaces/{ws}/loops/{id}/triggers/{tid}`{++} |
| GET | `/api/loops/{id}/steps`{--} | `GET /api/v1/workspaces/{ws}/loops/{id}/steps`{++} |
| POST | `/api/loops/{id}/steps`{--} | `POST /api/v1/workspaces/{ws}/loops/{id}/steps`{++} |
| POST | `/api/loops/{id}/steps/reorder`{--} | `POST /api/v1/workspaces/{ws}/loops/{id}/steps/reorder`{++} |
| PUT | `/api/loops/{id}/steps/{sid}`{--} | `PUT /api/v1/workspaces/{ws}/loops/{id}/steps/{sid}`{++} |
| DELETE | `/api/loops/{id}/steps/{sid}`{--} | `DELETE /api/v1/workspaces/{ws}/loops/{id}/steps/{sid}`{++} |
| GET | `/api/loops/{id}/executions`{--} | `GET /api/v1/workspaces/{ws}/loops/{id}/executions`{++} |
| GET | `/api/loops/{id}/executions/{eid}`{--} | `GET /api/v1/workspaces/{ws}/loops/{id}/executions/{eid}`{++} |
| POST | `/api/loops/{id}/executions/{eid}/steps/{seid}/approve`{--} | `POST /api/v1/workspaces/{ws}/loops/{id}/executions/{eid}/steps/{seid}/approve`{++} |
| GET | `/api/loop-executions/{eid}`{--} | `GET /api/v1/workspaces/{ws}/loop-executions/{eid}`{++} |

#### Loops 批量操作

| 旧路径 | 新路径 |
|--------|--------|
| `PUT /api/loops/batch-workspace`{--} | `POST /api/v1/workspaces/{ws}/loops/batch/workspace`{++} |
| `POST /api/loops/batch-copy-workspace`{--} | `POST /api/v1/workspaces/{ws}/loops/batch/copy-workspace`{++} |

#### Loops 导入导出

| 旧路径 | 新路径 |
|--------|--------|
| `GET /api/loops/export`{--} | `GET /api/v1/workspaces/{ws}/loops/export`{++} |
| `POST /api/loops/export-selected`{--} | `POST /api/v1/workspaces/{ws}/loops/export-selected`{++} |
| `GET /api/loops/{id}/export`{--} | `GET /api/v1/workspaces/{ws}/loops/{id}/export`{++} |
| `POST /api/loops/import/preview`{--} | `POST /api/v1/workspaces/{ws}/loops/import-preview`{++} |
| `POST /api/loops/import`{--} | `POST /api/v1/workspaces/{ws}/loops/import`{++} |
| `POST /api/loops/merge`{--} | `POST /api/v1/workspaces/{ws}/loops/merge`{++} |

> **说明**：import/export 放在集合路径 `/loops/` 下（如 `/loops/import`），不会与 `/loops/{id}` 冲突——axum 路由按注册顺序匹配，集合 action 在 `{id}` 之前注册即可。只在需要明确区分「子资源」和「RPC 操作」的场景用路径顺序保证即可，无需额外语法。

### 4.4 Blackboard & Wiki

现有路径已经包含了 `/api/workspaces/{ws}/` 前缀，只需要加版本号，并将路径统一为 v1：

| 方法 | 旧路径 | 新路径 |
|------|--------|--------|
| GET | `/api/workspaces/{ws}/blackboard`{~} | `/api/v1/workspaces/{ws}/blackboard` |
| PATCH | `/api/workspaces/{ws}/blackboard`{~} | `/api/v1/workspaces/{ws}/blackboard` |
| GET | `/api/workspaces/{ws}/blackboard/config`{~} | `/api/v1/workspaces/{ws}/blackboard/config` |
| GET | `/api/workspaces/{ws}/wiki/files`{~} | `/api/v1/workspaces/{ws}/wiki/files` |
| GET | `/api/workspaces/{ws}/wiki/files/{slug}`{~} | `/api/v1/workspaces/{ws}/wiki/files/{slug}` |
| DELETE | `/api/workspaces/{ws}/wiki/files/{slug}`{~} | `/api/v1/workspaces/{ws}/wiki/files/{slug}` |
| POST | `/api/workspaces/{ws}/wiki/chat`{~} | `/api/v1/workspaces/{ws}/wiki/chat` |

### 4.5 Workspace 设置与斜杠命令（单复数统一）

| 方法 | 旧路径（单数 workspace） | 新路径（复数 workspaces） |
|------|------------------------|--------------------------|
| GET | `/api/workspace/{ws}/slash-commands`{--} | `/api/v1/workspaces/{ws}/slash-commands`{++} |
| POST | `/api/workspace/{ws}/slash-commands`{--} | `/api/v1/workspaces/{ws}/slash-commands`{++} |
| PUT | `/api/workspace/{ws}/slash-commands/{cmd_id}`{--} | `/api/v1/workspaces/{ws}/slash-commands/{cmd_id}`{++} |
| DELETE | `/api/workspace/{ws}/slash-commands/{cmd_id}`{--} | `/api/v1/workspaces/{ws}/slash-commands/{cmd_id}`{++} |
| GET | `/api/workspace/{ws}/settings`{--} | `/api/v1/workspaces/{ws}/settings`{++} |
| PUT | `/api/workspace/{ws}/settings`{--} | `/api/v1/workspaces/{ws}/settings`{++} |

### 4.6 Quick Buttons（归入 workspace）

| 方法 | 旧路径 | 新路径 |
|------|--------|--------|
| GET | `/api/quick-buttons`{--} | `GET /api/v1/workspaces/{ws}/quick-buttons`{++} |
| POST | `/api/quick-buttons`{--} | `POST /api/v1/workspaces/{ws}/quick-buttons`{++} |
| PUT | `/api/quick-buttons/{id}`{--} | `PUT /api/v1/workspaces/{ws}/quick-buttons/{id}`{++} |
| DELETE | `/api/quick-buttons/{id}`{--} | `DELETE /api/v1/workspaces/{ws}/quick-buttons/{id}`{++} |

> **理由**：快捷话术按 workspace 隔离更合理——不同项目有不同的常用 prompt 模板。

### 4.7 全局资源（版本号升级，路径不变）

| 旧路径 | 新路径 |
|--------|--------|
| `GET /api/config`{~} | `GET /api/v1/config` |
| `PUT /api/config`{~} | `PUT /api/v1/config` |
| `GET /api/executors`{~} | `GET /api/v1/executors` |
| `PUT /api/executors/{name}`{~} | `PUT /api/v1/executors/{name}` |
| `POST /api/executors/{name}/detect`{~} | `POST /api/v1/executors/{name}/detect` |
| `POST /api/executors/detect-all`{~} | `POST /api/v1/executors/detect-all` |
| `POST /api/executors/{name}/resolve`{~} | `POST /api/v1/executors/{name}/resolve` |
| `POST /api/executors/{name}/test`{~} | `POST /api/v1/executors/{name}/test` |
| `GET /api/executors/default`{~} | `GET /api/v1/executors/default` |
| `PUT /api/executors/{name}/default`{~} | `PUT /api/v1/executors/{name}/default` |
| `GET /api/skills`{~} | `GET /api/v1/skills` |
| `DELETE /api/skills`{~} | `DELETE /api/v1/skills` |
| `GET /api/skills/compare`{~} | `GET /api/v1/skills/compare` |
| `GET /api/skills/version-update`{~} | `GET /api/v1/skills/version-update` |
| `POST /api/skills/sync`{~} | `POST /api/v1/skills/sync` |
| `POST /api/skills/invocations`{~} | `POST /api/v1/skills/invocations` |
| `GET /api/skills/content`{~} | `GET /api/v1/skills/content` |
| `GET /api/skills/file`{~} | `GET /api/v1/skills/file` |
| `GET /api/skills/export`{~} | `GET /api/v1/skills/export` |
| `POST /api/skills/import`{~} | `POST /api/v1/skills/import` |
| `GET /api/sessions`{~} | `GET /api/v1/sessions` |
| `GET /api/sessions/stats`{~} | `GET /api/v1/sessions/stats` |
| `GET /api/sessions/{id}`{~} | `GET /api/v1/sessions/{id}` |
| `DELETE /api/sessions/{id}`{~} | `DELETE /api/v1/sessions/{id}` |
| `GET /api/version`{~} | `GET /api/v1/version` |
| `GET /api/version/latest`{~} | `GET /api/v1/version/latest` |
| `POST /api/version/upgrade`{~} | `POST /api/v1/version/upgrade` |
| `GET /api/usage-stats`{~} | `GET /api/v1/usage-stats` |
| `POST /api/usage-stats/refresh`{~} | `POST /api/v1/usage-stats/refresh` |
| `GET /api/usage-stats/settings`{~} | `GET /api/v1/usage-stats/settings` |
| `PUT /api/usage-stats/settings`{~} | `PUT /api/v1/usage-stats/settings` |
| `GET /api/events`{~} | `GET /api/v1/events`（WebSocket） |
| `GET /api/project-directories`{~} | `GET /api/v1/project-directories` |
| `POST /api/project-directories`{~} | `POST /api/v1/project-directories` |
| `PUT /api/project-directories/{id}`{~} | `PUT /api/v1/project-directories/{id}` |
| `DELETE /api/project-directories/{id}`{~} | `DELETE /api/v1/project-directories/{id}` |
| `GET /api/todo-templates`{~} | `GET /api/v1/todo-templates` |
| `POST /api/todo-templates`{~} | `POST /api/v1/todo-templates` |
| `PUT /api/todo-templates/{id}`{~} | `PUT /api/v1/todo-templates/{id}` |
| `DELETE /api/todo-templates/{id}`{~} | `DELETE /api/v1/todo-templates/{id}` |
| `POST /api/todo-templates/{id}/copy`{~} | `POST /api/v1/todo-templates/{id}/copy` |
| `GET /api/review-templates`{~} | `GET /api/v1/review-templates` |
| `GET /api/review-templates/options`{~} | `GET /api/v1/review-templates/options` |
| `POST /api/review-templates`{~} | `POST /api/v1/review-templates` |
| `GET /api/review-templates/{id}`{~} | `GET /api/v1/review-templates/{id}` |
| `PUT /api/review-templates/{id}`{~} | `PUT /api/v1/review-templates/{id}` |
| `DELETE /api/review-templates/{id}`{~} | `DELETE /api/v1/review-templates/{id}` |
| `GET /api/custom-templates/status`{~} | `GET /api/v1/custom-templates/status` |
| `POST /api/custom-templates/subscribe`{~} | `POST /api/v1/custom-templates/subscribe` |
| `POST /api/custom-templates/unsubscribe`{~} | `POST /api/v1/custom-templates/unsubscribe` |
| `POST /api/custom-templates/sync`{~} | `POST /api/v1/custom-templates/sync` |
| `PUT /api/custom-templates/auto-sync`{~} | `PUT /api/v1/custom-templates/auto-sync` |
| `GET /api/cloud/config`{~} | `GET /api/v1/cloud/config` |
| `POST /api/cloud/config`{~} | `POST /api/v1/cloud/config` |
| `GET /api/cloud/sync/status`{~} | `GET /api/v1/cloud/sync/status` |
| `GET /api/cloud/sync/records`{~} | `GET /api/v1/cloud/sync/records` |
| `DELETE /api/cloud/sync/records`{~} | `DELETE /api/v1/cloud/sync/records` |
| `POST /api/cloud/sync/push`{~} | `POST /api/v1/cloud/sync/push` |
| `POST /api/cloud/sync/pull`{~} | `POST /api/v1/cloud/sync/pull` |

### 4.8 Backup（全局，仅版本号变更）

所有备份路径从 `/api/backup/...` 改为 `/api/v1/backup/...`。路径结构保持不变（backup 下的子路径已经组织良好）：

```
/api/v1/backup/export
/api/v1/backup/export-selected
/api/v1/backup/import
/api/v1/backup/merge
/api/v1/backup/database/{action}
/api/v1/backup/todo/{action}
/api/v1/backup/skills/{action}
/api/v1/backup/log-cleanup/{action}
```

### 4.9 Experts（全局，修复 create 路由冲突）

| 旧路径 | 问题 | 新路径 |
|--------|------|--------|
| `GET /api/experts`{~} | — | `GET /api/v1/experts` |
| `POST /api/experts/create`{--} | 与 `{name}` 路径冲突 | `POST /api/v1/experts`{++}（与 GET 共用集合路径） |
| `POST /api/experts/reload`{--} | RPC | `POST /api/v1/experts/reload`{++} |
| `POST /api/experts/import`{--} | RPC | `POST /api/v1/experts/import`{++} |
| `POST /api/experts/import-from-directory`{--} | RPC | `POST /api/v1/experts/import-from-directory`{++} |
| `POST /api/experts/import-from-workbuddy`{--} | RPC | `POST /api/v1/experts/import-from-workbuddy`{++} |
| `GET /api/experts/{name}`{~} | — | `GET /api/v1/experts/{name}` |
| `PUT /api/experts/{name}`{~} | — | `PUT /api/v1/experts/{name}` |
| `DELETE /api/experts/{name}`{~} | — | `DELETE /api/v1/experts/{name}` |
| `GET /api/experts/{name}/plugin-json`{~} | — | `GET /api/v1/experts/{name}/plugin-json` |
| `GET /api/experts/{name}/agent-md`{~} | — | `GET /api/v1/experts/{name}/agent-md` |
| `GET /api/experts/{name}/skills`{~} | — | `GET /api/v1/experts/{name}/skills` |
| `GET /api/experts/{name}/avatar`{~} | — | `GET /api/v1/experts/{name}/avatar` |
| `GET /api/experts/{name}/export`{~} | — | `GET /api/v1/experts/{name}/export` |
| `GET /api/experts/{name}/members/{mid}/avatar`{~} | — | `GET /api/v1/experts/{name}/members/{mid}/avatar` |

### 4.10 Agent Bots & Feishu（全局）

| 旧路径 | 新路径 |
|--------|--------|
| `GET /api/agent-bots`{~} | `GET /api/v1/agent-bots` |
| `DELETE /api/agent-bots/{id}`{~} | `DELETE /api/v1/agent-bots/{id}` |
| `PUT /api/agent-bots/{id}/config`{~} | `PUT /api/v1/agent-bots/{id}/config` |
| `PUT /api/agent-bots/{id}/workspace`{~} | `PUT /api/v1/agent-bots/{id}/workspace` |
| `POST /api/agent-bots/feishu/init`{~} | `POST /api/v1/agent-bots/feishu/init` |
| `POST /api/agent-bots/feishu/begin`{~} | `POST /api/v1/agent-bots/feishu/begin` |
| `GET /api/agent-bots/feishu/poll-stream`{~} | `GET /api/v1/agent-bots/feishu/poll-stream` |
| `GET /api/agent-bots/feishu/push`{~} | `GET /api/v1/agent-bots/feishu/push` |
| `PUT /api/agent-bots/feishu/push`{~} | `PUT /api/v1/agent-bots/feishu/push` |
| `GET /api/agent-bots/feishu/group-whitelist`{~} | `GET /api/v1/agent-bots/feishu/group-whitelist` |
| `POST /api/agent-bots/feishu/group-whitelist`{~} | `POST /api/v1/agent-bots/feishu/group-whitelist` |
| `DELETE /api/agent-bots/feishu/group-whitelist/{id}`{~} | `DELETE /api/v1/agent-bots/feishu/group-whitelist/{id}` |

> 注意：Agent Bot 是全局资源（一个 bot 可关联多个 workspace），因此不 scope 到 workspace。

### 4.11 Feishu 集成（全局）

| 旧路径 | 新路径 |
|--------|--------|
| `GET /api/feishu/history-messages`{~} | `GET /api/v1/feishu/history-messages` |
| `GET /api/feishu/message-stats`{~} | `GET /api/v1/feishu/message-stats` |
| `GET /api/feishu/senders`{~} | `GET /api/v1/feishu/senders` |
| `GET /api/feishu/history-chats`{~} | `GET /api/v1/feishu/history-chats` |
| `POST /api/feishu/history-chats`{~} | `POST /api/v1/feishu/history-chats` |
| `DELETE /api/feishu/history-chats/{id}`{~} | `DELETE /api/v1/feishu/history-chats/{id}` |
| `PUT /api/feishu/history-chats/{id}`{~} | `PUT /api/v1/feishu/history-chats/{id}` |

### 4.12 Webhooks（修复缺 `/api` 前缀）

| 旧路径 | 问题 | 新路径 |
|------|------|--------|
| `GET /webhook/trigger/todo/{todo_id}`{--} | 缺 `/api` 前缀 | `GET /api/v1/webhooks/todo/{id}/trigger`{++} |
| `POST /webhook/trigger/todo/{todo_id}`{--} | 缺 `/api` 前缀 | `POST /api/v1/webhooks/todo/{id}/trigger`{++} |
| `GET /webhook/trigger/loop/{loop_id}`{--} | 缺 `/api` 前缀 | `GET /api/v1/webhooks/loop/{id}/trigger`{++} |
| `POST /webhook/trigger/loop/{loop_id}`{--} | 缺 `/api` 前缀 | `POST /api/v1/webhooks/loop/{id}/trigger`{++} |

> 注意：Webhook 路径交换了 `{resource}/{id}/trigger` 的顺序，将 trigger 作为子资源操作放在末尾，这样更符合 REST 的「资源 → 子资源」层级感。

### 4.13 Bundled Skills（全局）

| 旧路径 | 新路径 |
|--------|--------|
| `POST /api/bundled/sync`{~} | `POST /api/v1/bundled/sync` |
| `GET /api/bundled/status`{~} | `GET /api/v1/bundled/status` |
| `GET /api/bundled/config`{~} | `GET /api/v1/bundled/config` |
| `PUT /api/bundled/config`{~} | `PUT /api/v1/bundled/config` |
| `GET /api/bundled/skills`{~} | `GET /api/v1/bundled/skills` |
| `GET /api/bundled/skill-sources`{~} | `GET /api/v1/bundled/skill-sources` |
| `GET /api/bundled/skills/{name}/content`{~} | `GET /api/v1/bundled/skills/{name}/content` |
| `GET /api/bundled/skills/{name}/file`{~} | `GET /api/v1/bundled/skills/{name}/file` |
| `POST /api/bundled/skills/install`{~} | `POST /api/v1/bundled/skills/install` |

### 4.14 根级系统路由

| 旧路径 | 新路径 |
|--------|--------|
| `GET /`{~} | `GET /`（不变） |
| `GET /health`{~} | `GET /health`（不变） |
| `GET /assets/{*path}`{~} | `GET /assets/{*path}`（不变，前端静态资源） |

---

## 5. 命名规范

### 5.1 通用规则

| 规则 | 示例 |
|------|------|
| 路径全小写 | `/api/v1/workspaces/{ws}/todos` |
| 资源名词复数 | `todos`、`executions`、`loops`、`tags` |
| 多词用连字符（kebab-case） | `force-status`、`recent-completed`、`smart-create` |
| 路径参数用花括号 | `{ws}`、`{id}`、`{slug}` |
| Action/Command 用独立路径 | `/reload`、`/export`、`/archive` |
| 版本号用 `v{N}` | `/api/v1/`、`/api/v2/` |

### 5.2 新旧名称对照表

| 旧名称 | 新名称 | 原因 |
|--------|--------|------|
| `execution-records` | `executions` | 简洁，记录即执行 |
| `workspace`（单数） | `workspaces`（复数） | 与 blackboard 现有一致 |
| ~~`execute/stop`~~ | `executions/{id}/stop` | 归入 executions 资源 |
| ~~`execute/force-fail`~~ | `executions/{id}/force-fail` | 归入 executions 资源 |
| ~~`backup/database/file`~~ | `backup/database/files` | 集合名词复数 |
| ~~`backup/todo/file`~~ | `backup/todo/files` | 同上 |
| ~~`backup/skills/file`~~ | `backup/skills/files` | 同上 |
| ~~`webhook/trigger/todo/{id}`~~ | `webhooks/todo/{id}/trigger` | 归入 webhooks + 资源前置 |
| ~~`running-board`~~ | `executions/running-board` | 归入 executions 域 |
| ~~`running-todos`~~ | `executions/running-todos` | 归入 executions 域 |
| ~~`dashboard-stats`~~ | `stats/dashboard` | stats 根域 |

### 5.3 HTTP 方法规范

| 方法 | 语义 | 示例 |
|------|------|------|
| GET | 查询/读取 | `GET /api/v1/workspaces/{ws}/todos` |
| POST | 创建/RPC action | `POST /api/v1/workspaces/{ws}/todos` |
| PUT | 全量替换 | `PUT /api/v1/workspaces/{ws}/todos/{id}` |
| PATCH | 部分更新 | `PATCH /api/v1/workspaces/{ws}/blackboard` |
| DELETE | 删除 | `DELETE /api/v1/workspaces/{ws}/todos/{id}` |

---

## 6. 迁移策略

### 6.1 原则：一次性切换，不保留旧路由

新旧路由不共存。后端实现新 v1 路由后，**同时移除旧 `/api/*` 路由**。前端和 CLI 在同一 PR 中完成路径更新，测试验证通过后合并。不设 Deprecation 窗口。

### 6.2 迁移节奏

| 阶段 | 内容 |
|------|------|
| Phase 1 | 后端实现所有 v1 路由 handler，旧 `/api/*` 路由代码一并删除 + 单元测试通过 |
| Phase 2 | 前端所有 API 调用路径切换到 v1 路径 |
| Phase 3 | CLI 命令路径切换到 v1 |
| Phase 4 | 集成测试验证全套路径 + Playwright 前端验证通过 |
| Phase 5 | 合并 PR |

### 6.3 前端迁移方式

**推荐做法：逐个文件直接修改路径字符串，不经过中间层转换。** 前端 API 调用集中在 `utils/database/*.ts` 中，每个文件职责单一，改起来是机械的字符串替换。

```typescript
// 改前： todos.ts
return unwrap(await api.get('/api/todos'));

// 改后： todos.ts
return unwrap(await api.get(`/api/v1/workspaces/${workspaceId}/todos`));
```

> workspaceId 作为参数从组件层传入（组件已经从上下文或路由中持有当前 workspace id）。

---

## 7. 实现要点

### 7.1 路由结构

用 Axum 的 `.nest()` 挂载 workspace-scoped 资源，`{ws}` 在 nest 层定义：

```rust
// create_app 中直接挂载 v1 路由，旧路由代码一并删除
fn create_app() -> Router<AppState> {
    Router::new()
        .merge(v1_routes())
        .layer(...)
}

fn v1_routes() -> Router<AppState> {
    Router::new()
        .nest("/api/v1/workspaces/{ws}/todos", todo::v1_routes())
        .nest("/api/v1/workspaces/{ws}/executions", execution::v1_routes())
        .nest("/api/v1/workspaces/{ws}/loops", loop_::v1_routes())
        // ... workspace-scoped 资源
        .merge(v1_global_routes())        // 全局资源（无 ws 层级）
        .merge(v1_system_routes())        // 根级 + static
}
```

Axum 的 `.nest()` 会移除路径前缀，子路由内的路径是相对路径：

```rust
// 子路由定义，{ws} 已在 nest 层被捕获
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_todos))                    // → /api/v1/workspaces/{ws}/todos
        .route("/{id}", get(get_todo))                  // → /api/v1/workspaces/{ws}/todos/{id}
        .route("/batch/executor", post(batch_executor)) // → /api/v1/workspaces/{ws}/todos/batch/executor
}
```

### 7.2 Handler 签名变化

workspace_id 从「可选查询参数」变为「必选路径参数」：

```rust
// 旧：Query 中可选
async fn list_todos(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<...> {
    let ws_id = params.get("workspace_id").and_then(|s| s.parse().ok());
}

// 新：Path 中必选
async fn list_todos(
    State(state): State<AppState>,
    Path(ws_id): Path<i64>,
    Query(params): Query<TodoQuery>,
) -> Result<...> {
    // ws_id 直接从路径获取
}
```

由于旧路由不保留，无需做 Option 兼容。所有 handler 原地改为从 `Path` 拿 workspace_id。

---

## 8. 设计决策记录（ADR）

### ADR-1：为什么用 `/api/v1/` 而不是 `/v1/`

- 现状：所有路由都有 `/api/` 前缀
- 选择：保留 `/api/` 前缀，插入 `v1/`
- 理由：迁移改动最小，新旧路径差异最小，K8s 也是 `/api/v1/`

### ADR-2：为什么用 `executions` 替代 `execution-records`

- 现状：`execution-records` 冗长，且 `POST /api/execute` 与 `GET /api/execution-records` 不一致
- 选择：统一为 `executions`
- 理由：REST 资源名应该是单一名词；`records` 是实现细节，用户关心的是「执行」本身

### ADR-3：为什么批量操作用子资源方式（方案 B）

- 选择：`POST .../batch/executor`、`POST .../batch/workspace`
- 理由：方案 A（`POST .../batch` + body.action）虽然更 REST，但 body.action 的枚举值会影响路由/权限的可见性；子资源方式让每个批量操作有明确的 URL，便于日志和监控

### ADR-4：为什么 tags 归入 workspace

- 业务语义：标签是项目维度的分类，不同项目有不同的标签体系
- 如果将来需要全局标签，可以在 `/api/v1/tags` 提供只读聚合，但写操作走 workspace 域

### ADR-5：为什么 webhook 加 `/api/` 前缀并重新组织

- 现状：`/webhook/trigger/todo/{todo_id}` 没有 `/api/` 前缀，且资源顺序是「动作→资源」
- 选择：`/api/v1/webhooks/todo/{todo_id}/trigger`，让 webhook 归入 API 体系
- 理由：降低运维复杂度（防火墙/反向代理规则统一）；路径语义向 REST 靠拢（先资源后动作）

### ADR-6：为什么 experts 的 action 用路径风格而非冒号

- 选择：`POST /api/v1/experts/reload`、`POST /api/v1/experts/import`
- 理由：路径风格更传统，对开发者更熟悉。`reload` 不会与 `{name}` 冲突——axum 按注册顺序匹配，集合 action 在 `{name}` 之前注册即可。既然 `{name}` 是动态段，静态路径 `/reload` 优先匹配。

### ADR-7：为什么旧路由不保留，直接删除

- 选择：新旧路由不共存。测试验证通过后，旧路由代码直接删除
- 理由：两套路由的维护成本（双 handler、双测试、双文档）会持续累加，且本项目无外部 API 消费者（社区插件等），迁移是可控的原子操作。前后端 + CLI 在同一 PR 中完成切换即可

---

## 9. 变更清单（影响文件）

### 后端（Rust）

| 文件 | 变更内容 |
|------|----------|
| `backend/src/handlers/mod.rs` | `create_app` 追加 v1 路由 merge；`mount_domain_routes` 改为两套 |
| `backend/src/handlers/todo.rs` | 新增 `v1_routes()`，handler 支持 Path ws_id |
| `backend/src/handlers/execution.rs` | 同上 |
| `backend/src/handlers/loop_.rs` | 同上 |
| `backend/src/handlers/tag.rs` | 同上 |
| `backend/src/handlers/backup.rs` | 仅版本号变更 |
| `backend/src/handlers/skills.rs` | 仅版本号变更 |
| `backend/src/handlers/agent_bot.rs` | 仅版本号变更 + workspace 路由改复数 |
| `backend/src/handlers/blackboard.rs` | 仅版本号变更 |
| `backend/src/handlers/experts.rs` | 修复 create 冲突 + 版本号 |
| `backend/src/handlers/bundled.rs` | 仅版本号变更 |
| `backend/src/handlers/webhook.rs` | 重写路径 + 加 /api/v1 前缀 |
| `backend/src/cli/client.rs` | CLI 路径常量更新 |
| 各 handler 的单元测试 | 路径更新 |

### 前端（TypeScript）

| 文件 | 变更内容 |
|------|----------|
| `frontend/src/utils/database/todos.ts` | 路径改为 v1 |
| `frontend/src/utils/database/executions.ts` | 路径改为 v1 + 命名变更 |
| `frontend/src/utils/database/loops.ts` | 路径改为 v1 |
| `frontend/src/utils/database/backup.ts` | 路径改为 v1 |
| `frontend/src/utils/database/skills.ts` | 路径改为 v1 |
| `frontend/src/utils/database/bots.ts` | 路径改为 v1 + 单复数修复 |
| `frontend/src/utils/database/blackboard.ts` | 路径改为 v1 |
| `frontend/src/utils/database/experts.ts` | 路径改为 v1 + create 修复 |
| `frontend/src/utils/database/quickButtons.ts` | 路径改为 v1 + workspace scope |
| `frontend/src/utils/database/usage_stats.ts` | 路径改为 v1 |
| `frontend/src/utils/database/sessions.ts` | 路径改为 v1 |
| `frontend/src/utils/database/reviewTemplates.ts` | 路径改为 v1 |
| `frontend/src/utils/database/sync.ts` | 路径改为 v1 |
| `frontend/src/api/bundled.ts` | 路径改为 v1 |
| `frontend/src/components/BlackboardPage.tsx` | fetch 路径改为 v1 |
| `frontend/src/components/WikiViewPage.tsx` | fetch 路径改为 v1 |
| `frontend/src/components/settings/BackupPanel.tsx` | fetch 路径改为 v1 |

---

## 附录：K8s API 风格参考

Kubernetes 的 API 设计原则是此方案的灵感来源：

```
# K8s 路径模板
/api/v1/namespaces/{namespace}/{resource}[/{name}[/{sub-resource}]]

# K8s 示例
GET  /api/v1/namespaces/default/pods                     # 列出 pod
GET  /api/v1/namespaces/default/pods/{name}               # 查询 pod
POST /api/v1/namespaces/default/pods                      # 创建 pod
DELETE /api/v1/namespaces/default/pods/{name}              # 删除 pod
GET  /api/v1/namespaces/default/pods/{name}/log            # pod 日志（子资源）

# 集群级别资源（无 namespace）
GET  /api/v1/nodes                                        # 集群节点
GET  /api/v1/namespaces                                   # 列 namespace 本身
```

核心原则：
1. **Namespace 作为租户边界**：所有 namespaced 资源都挂在 `/namespaces/{ns}/` 下
2. **资源名词复数**：`pods`、`services`、`deployments`
3. **子资源 = 资源的相关视角**：`/pods/{name}/log`、`/pods/{name}/status`
4. **集群资源不挂 namespace**：`/nodes`、`/namespaces`
5. **版本前缀**：`/api/v1/`、`/apis/apps/v1/`

ntd 的映射：

| K8s | ntd |
|-----|-----|
| `namespace` | `workspace` |
| `pods` | `todos`、`loops`、`executions` |
| `nodes`（集群） | `config`、`experts`、`executors`（全局） |
| `/api/v1/namespaces/{ns}/pods` | `/api/v1/workspaces/{ws}/todos` |
| `/api/v1/namespaces/{ns}/pods/{name}/log` | `/api/v1/workspaces/{ws}/executions/{id}/logs` |
