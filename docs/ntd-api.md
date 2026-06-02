# ntd API 接口文档

AI Todo 应用后端 API 参考手册。所有接口前缀为 `/api/`（WebSocket 为 `/api/events`）。

---

## 接口分类

### 1. Todo 管理
### 2. 标签管理
### 3. 执行记录
### 4. 执行操作
### 5. 调度器
### 6. 备份与恢复
### 7. 配置管理
### 8. 执行器管理
### 9. 技能管理
### 10. Agent Bot 管理
### 11. 飞书集成
### 12. 会话管理
### 13. 项目目录
### 14. Todo 模板
### 15. 自定义模板
### 16. 系统接口

---

## 1. Todo 管理

### 获取 Todo 列表
```
GET /api/todos
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `status` | string | 按状态筛选 |
| `tag_id` | number | 按标签 ID 筛选 |
| `search` | string | 搜索关键词 |
| `page` | number | 页码（默认 1） |
| `limit` | number | 每页数量（默认 20） |

**响应示例：**
```json
{
  "code": 0,
  "data": {
    "todos": [
      {
        "id": 1,
        "title": "完成报告",
        "status": "pending",
        "created_at": "2026-05-14T10:00:00Z"
      }
    ],
    "total": 100
  }
}
```

---

### 创建 Todo
```
POST /api/todos
```

**请求体：**
```json
{
  "title": "Todo 标题",
  "prompt": "Prompt 内容",
  "executor": "claudecode",
  "workspace": "/path/to/workspace",
  "tags": [1, 2],
  "schedule": "0 9 * * *"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `title` | string | 是 | Todo 标题 |
| `prompt` | string | 否 | Prompt 内容 |
| `executor` | string | 否 | 执行器类型 |
| `workspace` | string | 否 | 工作目录 |
| `tags` | number[] | 否 | 标签 ID 数组 |
| `schedule` | string | 否 | Cron 表达式 |

---

### 获取 Todo 详情
```
GET /api/todos/{id}
```

---

### 更新 Todo
```
PUT /api/todos/{id}
```

**请求体：**
```json
{
  "title": "新标题",
  "prompt": "新 prompt",
  "status": "completed",
  "executor": "joinai",
  "workspace": "/new/path",
  "tags": [1, 3]
}
```

---

### 删除 Todo
```
DELETE /api/todos/{id}
```

---

### 强制更新 Todo 状态
```
PUT /api/todos/{id}/force-status
```

**请求体：**
```json
{
  "status": "running"
}
```

---

### 更新 Todo 标签
```
PUT /api/todos/{id}/tags
```

**请求体：**
```json
{
  "tags": [1, 2, 3]
}
```

---

### 获取 Todo 执行摘要
```
GET /api/todos/{id}/summary
```

---

### 获取最近完成的 Todo
```
GET /api/todos/recent-completed
```

---

## 2. 标签管理

### 获取标签列表
```
GET /api/tags
```

**响应示例：**
```json
{
  "code": 0,
  "data": {
    "tags": [
      { "id": 1, "name": "重要", "color": "#ff4d4f" },
      { "id": 2, "name": "紧急", "color": "#faad14" }
    ]
  }
}
```

---

### 创建标签
```
POST /api/tags
```

**请求体：**
```json
{
  "name": "标签名",
  "color": "#1890ff"
}
```

---

### 删除标签
```
DELETE /api/tags/{id}
```

---

## 3. 执行记录

### 获取执行记录列表
```
GET /api/execution-records
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `todo_id` | number | Todo ID |
| `status` | string | 状态筛选 |
| `page` | number | 页码 |
| `limit` | number | 每页数量 |

---

### 获取运行中的执行记录
```
GET /api/execution-records/running
```

---

### 按会话 ID 获取执行记录
```
GET /api/execution-records/session/{session_id}
```

---

### 获取执行记录详情
```
GET /api/execution-records/{id}
```

---

### 恢复执行
```
POST /api/execution-records/{id}/resume
```

**请求体：**
```json
{
  "message": "继续执行"
}
```

---

## 4. 执行操作

### 执行 Todo
```
POST /api/execute
```

**请求体：**
```json
{
  "todo_id": 1,
  "message": "开始执行",
  "executor": "claudecode"
}
```

---

### 停止执行
```
POST /api/execute/stop
```

**请求体：**
```json
{
  "todo_id": 1
}
```

---

### 强制失败
```
POST /api/execute/force-fail
```

**请求体：**
```json
{
  "todo_id": 1,
  "reason": "超时"
}
```

---

### 获取运行中的 Todo
```
GET /api/running-todos
```

---

### 获取仪表盘统计
```
GET /api/dashboard-stats
```

---

## 5. 调度器

### 获取定时 Todo 列表
```
GET /api/scheduler/todos
```

---

### 更新 Todo 调度配置
```
PUT /api/todos/{id}/scheduler
```

**请求体：**
```json
{
  "schedule": "0 9 * * *",
  "enabled": true
}
```

---

## 6. 备份与恢复

### 导出全部数据
```
GET /api/backup/export
```

返回 YAML 格式的完整备份。

---

### 导出选定的 Todo
```
POST /api/backup/export-selected
```

**请求体：**
```json
{
  "todo_ids": [1, 2, 3]
}
```

---

### 导入备份（完整替换）
```
POST /api/backup/import
```

Content-Type: `multipart/form-data`

| 字段 | 类型 | 说明 |
|------|------|------|
| `file` | file | YAML 备份文件 |

---

### 合并导入
```
POST /api/backup/merge
```

Content-Type: `multipart/form-data`

| 字段 | 类型 | 说明 |
|------|------|------|
| `file` | file | YAML 备份文件 |

---

### 下载数据库
```
GET /api/backup/database/download
```

---

### 获取数据库备份状态
```
GET /api/backup/database/status
```

---

### 触发立即备份
```
POST /api/backup/database/trigger
```

---

### 更新自动备份配置
```
PUT /api/backup/database/auto
```

**请求体：**
```json
{
  "enabled": true,
  "interval_hours": 24
}
```

---

### 下载备份文件
```
GET /api/backup/database/file
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `filename` | string | 备份文件名 |

---

### 删除备份文件
```
DELETE /api/backup/database/file
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `filename` | string | 备份文件名 |

---

## 7. 配置管理

### 获取配置
```
GET /api/config
```

**响应示例：**
```json
{
  "code": 0,
  "data": {
    "server": {
      "port": 8088
    },
    "executors": {
      "claudecode": { "enabled": true },
      "joinai": { "enabled": true }
    },
    "backup": {
      "auto_backup": true,
      "interval_hours": 24
    }
  }
}
```

---

### 更新配置
```
PUT /api/config
```

**请求体：**
```json
{
  "server": { "port": 8088 },
  "backup": { "auto_backup": true }
}
```

---

## 8. 执行器管理

### 列出执行器
```
GET /api/executors
```

---

### 更新执行器配置
```
PUT /api/executors/{name}
```

**请求体：**
```json
{
  "enabled": true,
  "config": {
    "api_key": "xxx",
    "model": "claude-3-5-sonnet"
  }
}
```

---

### 检测执行器
```
POST /api/executors/{name}/detect
```

检测执行器二进制文件是否存在。

---

### 测试执行器
```
POST /api/executors/{name}/test
```

运行 `executor --version` 测试配置是否正确。

---

## 9. 技能管理

### 列出技能
```
GET /api/skills
```

按执行器分组列出所有技能。

---

### 比较技能
```
GET /api/skills/compare
```

跨执行器的技能对比矩阵。

---

### 同步技能
```
POST /api/skills/sync
```

**请求体：**
```json
{
  "skill_name": "code_review",
  "from_executor": "claudecode",
  "to_executors": ["joinai", "kimi"]
}
```

---

### 获取技能调用记录
```
GET /api/skills/invocations
```

---

### 记录技能调用
```
POST /api/skills/invocations
```

**请求体：**
```json
{
  "skill_name": "code_review",
  "executor": "claudecode",
  "success": true,
  "duration_ms": 5000
}
```

---

### 获取技能内容
```
GET /api/skills/content
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `name` | string | 技能名称 |
| `executor` | string | 执行器 |

---

### 导出技能
```
GET /api/skills/export
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `name` | string | 技能名称 |
| `executor` | string | 执行器 |

返回 `.zip` 格式的技能包。

---

### 导入技能
```
POST /api/skills/import
```

Content-Type: `multipart/form-data`

| 字段 | 类型 | 说明 |
|------|------|------|
| `file` | file | 技能 .zip 文件 |

---

## 10. Agent Bot 管理

### 列出 Agent Bot
```
GET /api/agent-bots
```

---

### 删除 Agent Bot
```
DELETE /api/agent-bots/{id}
```

---

### 更新 Agent Bot 配置
```
PUT /api/agent-bots/{id}/config
```

**请求体：**
```json
{
  "enabled": true,
  "name": "My Bot",
  "webhook_url": "https://..."
}
```

---

## 11. 飞书集成

### 初始化飞书 OAuth
```
POST /api/agent-bots/feishu/init
```

---

### 开始飞书 OAuth
```
POST /api/agent-bots/feishu/begin
```

---

### 轮询飞书 OAuth 状态
```
POST /api/agent-bots/feishu/poll
```

---

### 获取飞书推送配置
```
GET /api/agent-bots/feishu/push
```

---

### 更新飞书推送配置
```
PUT /api/agent-bots/feishu/push
```

**请求体：**
```json
{
  "enabled": true,
  "chat_id": "oc_xxx",
  "push_completed": true
}
```

---

### 获取群组白名单
```
GET /api/agent-bots/feishu/group-whitelist
```

---

### 添加群组到白名单
```
POST /api/agent-bots/feishu/group-whitelist
```

**请求体：**
```json
{
  "chat_id": "oc_xxx",
  "name": "开发群"
}
```

---

### 从白名单移除
```
DELETE /api/agent-bots/feishu/group-whitelist/{id}
```

---

## 12. 飞书历史

### 获取历史消息
```
GET /api/feishu/history-messages
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `chat_id` | string | 聊天室 ID |
| `start_time` | number | 开始时间戳 |
| `end_time` | number | 结束时间戳 |
| `page` | number | 页码 |
| `limit` | number | 每页数量 |

---

### 获取消息统计
```
GET /api/feishu/message-stats
```

---

### 获取消息发送者列表
```
GET /api/feishu/senders
```

---

### 获取历史聊天室列表
```
GET /api/feishu/history-chats
```

---

### 创建历史聊天室
```
POST /api/feishu/history-chats
```

**请求体：**
```json
{
  "chat_id": "oc_xxx",
  "name": "聊天室名称"
}
```

---

### 删除历史聊天室
```
DELETE /api/feishu/history-chats/{id}
```

---

### 更新历史聊天室
```
PUT /api/feishu/history-chats/{id}
```

**请求体：**
```json
{
  "name": "新名称",
  "enabled": true
}
```

---

## 13. 会话管理

### 列出会话
```
GET /api/sessions
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `page` | number | 页码 |
| `limit` | number | 每页数量 |

---

### 获取会话统计
```
GET /api/sessions/stats
```

---

### 获取会话详情
```
GET /api/sessions/{id}
```

包含会话消息和子代理信息。

---

### 删除会话
```
DELETE /api/sessions/{id}
```

---

## 14. 项目目录

### 列出项目目录
```
GET /api/project-directories
```

---

### 创建项目目录
```
POST /api/project-directories
```

**请求体：**
```json
{
  "name": "项目名称",
  "path": "/path/to/project"
}
```

---

### 更新项目目录
```
PUT /api/project-directories/{id}
```

**请求体：**
```json
{
  "name": "新名称"
}
```

---

### 删除项目目录
```
DELETE /api/project-directories/{id}
```

---

## 15. Todo 模板

### 获取模板列表
```
GET /api/todo-templates
```

---

### 创建模板
```
POST /api/todo-templates
```

**请求体：**
```json
{
  "name": "模板名称",
  "title": "Todo 标题模板",
  "prompt": "Prompt 模板",
  "executor": "claudecode",
  "tags": [1, 2]
}
```

---

### 更新模板
```
PUT /api/todo-templates/{id}
```

---

### 删除模板
```
DELETE /api/todo-templates/{id}
```

---

### 复制模板
```
POST /api/todo-templates/{id}/copy
```

---

## 16. 自定义模板

### 获取订阅状态
```
GET /api/custom-templates/status
```

---

### 订阅远程模板
```
POST /api/custom-templates/subscribe
```

**请求体：**
```json
{
  "url": "https://example.com/templates.yaml"
}
```

---

### 取消订阅
```
POST /api/custom-templates/unsubscribe
```

**请求体：**
```json
{
  "url": "https://example.com/templates.yaml"
}
```

---

### 手动同步
```
POST /api/custom-templates/sync
```

---

### 更新自动同步配置
```
PUT /api/custom-templates/auto-sync
```

**请求体：**
```json
{
  "enabled": true,
  "cron": "0 */6 * * *"
}
```

---

## 17. 系统接口

### 获取版本信息
```
GET /api/version
```

**响应示例：**
```json
{
  "code": 0,
  "data": {
    "version": "1.2.3",
    "build_time": "2026-05-14T10:00:00Z",
    "git_commit": "abc123"
  }
}
```

---

## 18. WebSocket 事件

### 事件订阅
```
GET /api/events
```

通过 WebSocket 连接接收实时事件。

**事件类型：**

| 事件 | 说明 |
|------|------|
| `todo.created` | Todo 创建 |
| `todo.updated` | Todo 更新 |
| `todo.deleted` | Todo 删除 |
| `execution.started` | 执行开始 |
| `execution.progress` | 执行进度 |
| `execution.completed` | 执行完成 |
| `execution.failed` | 执行失败 |

**消息格式：**
```json
{
  "type": "execution.progress",
  "data": {
    "todo_id": 1,
    "message": "正在分析代码..."
  }
}
```

---

## 通用响应格式

### 成功响应
```json
{
  "code": 0,
  "data": { ... },
  "message": "success"
}
```

### 错误响应
```json
{
  "code": 1001,
  "message": "错误描述",
  "data": null
}
```

### 分页响应
```json
{
  "code": 0,
  "data": {
    "items": [ ... ],
    "total": 100,
    "page": 1,
    "limit": 20
  }
}
```

---

## 状态码说明

| 状态值 | 说明 |
|--------|------|
| `pending` | 等待执行 |
| `running` | 执行中 |
| `completed` | 已完成 |
| `failed` | 执行失败 |
| `paused` | 已暂停 |
| `cancelled` | 已取消 |

---

## 执行器类型

| 类型 | 说明 |
|------|------|
| `claudecode` | Claude Code |
| `joinai` | JoinAI |
| `codebuddy` | CodeBuddy |
| `opencode` | OpenCode |
| `atomcode` | AtomCode |
| `hermes` | Hermes |
| `kimi` | Kimi |
| `codex` | Codex |
