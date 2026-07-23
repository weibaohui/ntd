# ntd API 接口文档

ntd 后端 API 参考手册。所有业务接口前缀为 `/api/v1/`：workspace 作用域资源嵌套在 `/api/v1/workspaces/{ws}/` 下（如 todos、executions、loops），全局资源直接挂在 `/api/v1/` 下（如 tags、config、experts）。WebSocket 事件流 `/api/events` 为升级端点，后端不版本化，保持原路径。

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
### 12. 飞书历史
### 13. 会话管理
### 14. 项目目录
### 15. Todo 模板
### 16. 评审模板
### 17. Loop Studio
### 18. 自定义模板
### 19. 系统接口
### 20. WebSocket 事件
### 21. Webhook 触发
### 22. 使用统计
### 23. 云端同步

---

## 1. Todo 管理

### 获取 Todo 列表
```
GET /api/v1/workspaces/{ws}/todos
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `status` | string | 按状态筛选 |
| `tag_id` | number | 按标签 ID 筛选 |
| `running` | boolean | 仅显示运行中的 Todo（`true`） |
| `search` | string | 搜索关键词（标题或 prompt 包含匹配，由前端/CLI 在内存中过滤） |

注意：后端 `GET /api/v1/workspaces/{ws}/todos` 本身不支持 `page`/`limit` 分页参数；如需分页请在客户端做。

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
    ]
  }
}
```

---

### 创建 Todo
```
POST /api/v1/workspaces/{ws}/todos
```

**请求体：**
```json
{
  "title": "Todo 标题",
  "prompt": "Prompt 内容",
  "tag_ids": [1, 2],
  "executor": "claudecode",
  "scheduler_enabled": true,
  "scheduler_config": "0 9 * * *",
  "scheduler_timezone": "Asia/Shanghai",
  "hooks": []
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `title` | string | 是 | Todo 标题 |
| `prompt` | string | 否 | Prompt 内容（空时回退为 `title`） |
| `tag_ids` | number[] | 否 | 标签 ID 数组 |
| `executor` | string | 否 | 执行器类型，默认 `claudecode` |
| `scheduler_enabled` | boolean | 否 | 是否启用调度 |
| `scheduler_config` | string | 否 | Cron 表达式（6 字段：秒 + 标准 5 字段） |
| `scheduler_timezone` | string | 否 | 调度时区，缺省时回退到系统默认时区 |
| `hooks` | TodoHookItem[] | 否 | 内联钩子列表（替换默认空列表） |

---

### 获取 Todo 详情
```
GET /api/v1/workspaces/{ws}/todos/{id}
```

---

### 更新 Todo
```
PUT /api/v1/workspaces/{ws}/todos/{id}
```

**请求体：**
```json
{
  "title": "新标题",
  "prompt": "新 prompt",
  "status": "completed",
  "executor": "mobilecoder",
  "scheduler_enabled": false,
  "scheduler_config": "0 9 * * *",
  "scheduler_timezone": "Asia/Shanghai",
  "workspace": "/new/path",
  "worktree_enabled": false,
  "hooks": []
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `title` | string | 否 | 新标题 |
| `prompt` | string | 否 | 新 prompt（空字符串会回退为当前 `title`） |
| `status` | string | 否 | 新状态（`pending`/`in_progress`/`running`/`completed`/`failed`/`cancelled`） |
| `executor` | string | 否 | 执行器类型 |
| `scheduler_enabled` | boolean | 否 | 是否启用调度 |
| `scheduler_config` | string | 否 | Cron 表达式 |
| `scheduler_timezone` | string | 否 | 调度时区 |
| `workspace` | string | 否 | 工作目录 |
| `worktree_enabled` | boolean | 否 | 是否启用 git worktree 隔离 |
| `hooks` | TodoHookItem[] | 否 | 内联钩子列表（`null` 保留原列表） |

---

### 删除 Todo
```
DELETE /api/v1/workspaces/{ws}/todos/{id}
```

---

### 强制更新 Todo 状态
```
PUT /api/v1/workspaces/{ws}/todos/{id}/force-status
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
PUT /api/v1/workspaces/{ws}/todos/{id}/tags
```

**请求体：**
```json
{
  "tag_ids": [1, 2, 3]
}
```

---

### 获取 Todo 执行摘要
```
GET /api/v1/workspaces/{ws}/todos/{id}/summary
```

---

### 获取最近完成的 Todo
```
GET /api/v1/workspaces/{ws}/todos/recent-completed
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `hours` | number | 时间窗口小时数（默认 24，范围 1-720） |

---

### 批量修改执行器
```
PUT /api/v1/workspaces/{ws}/todos/batch-executor
```

**请求体：**
```json
{
  "todo_ids": [1, 2, 3],
  "executor": "claudecode"
}
```

---

### 批量移动工作目录
```
PUT /api/v1/workspaces/{ws}/todos/batch-workspace
```

**请求体：**
```json
{
  "todo_ids": [1, 2, 3],
  "workspace": "/new/workspace/path"
}
```

---

### 批量复制工作目录
```
POST /api/v1/workspaces/{ws}/todos/batch-copy-workspace
```

**请求体：**
```json
{
  "todo_ids": [1, 2, 3],
  "source_workspace": "/source/path",
  "target_workspace": "/target/path"
}
```

---

## 2. 标签管理

### 获取标签列表
```
GET /api/v1/tags
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
POST /api/v1/tags
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
DELETE /api/v1/tags/{id}
```

---

## 3. 执行记录

### 获取执行记录列表
```
GET /api/v1/workspaces/{ws}/executions
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `todo_id` | number | Todo ID |
| `status` | string | 状态筛选（`all` 表示不过滤） |
| `page` | number | 页码（默认 1） |
| `limit` | number | 每页数量（默认 10，范围 1-100） |

---

### 获取运行中的执行记录
```
GET /api/v1/workspaces/{ws}/executions/running
```

---

### 按会话 ID 获取执行记录
```
GET /api/v1/workspaces/{ws}/executions/session/{session_id}
```

---

### 获取执行记录详情
```
GET /api/v1/workspaces/{ws}/executions/{id}
```

---

### 获取执行记录日志
```
GET /api/v1/workspaces/{ws}/executions/{id}/logs
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `page` | number | 页码（默认 1） |
| `per_page` | number | 每页条数（默认 200，范围 10-1000） |

---

### 恢复执行
```
POST /api/v1/workspaces/{ws}/executions/{id}/resume
```

**请求体：**
```json
{
  "message": "继续执行"
}
```

---

### 评分执行记录
```
PUT /api/v1/workspaces/{ws}/executions/{id}/rating
```

**请求体：**
```json
{
  "rating": 85
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `rating` | number | 是 | 评分（0-100） |

---

### 获取 Running Board
```
GET /api/v1/workspaces/{ws}/executions/running-board
```

返回按状态分组的执行记录看板视图，包含 scheduled/running/completed/reviewing/review_passed/failed 六列。

---

## 4. 执行操作

### 执行 Todo
```
POST /api/v1/workspaces/{ws}/executions
```

**请求体：**
```json
{
  "todo_id": 1,
  "message": "开始执行",
  "executor": "claudecode",
  "params": {
    "project_name": "myproject",
    "env": "production"
  }
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `todo_id` | number | 是 | 要执行的 Todo ID |
| `message` | string | 否 | 附加消息，会注入到 prompt 的 `{{message}}` 占位符 |
| `executor` | string | 否 | 临时覆盖执行器 |
| `params` | object<string,string> | 否 | 模板占位符替换键值对，键名对应 `{{key}}` |

---

### 智能创建
```
POST /api/v1/workspaces/{ws}/todos/smart
```

**请求体：**
```json
{
  "content": "请帮我写一份季度报告"
}
```

**响应示例：**
```json
{
  "code": 0,
  "data": {
    "task_id": "...",
    "record_id": 456,
    "todo_id": 1,
    "todo_title": "请帮我写一份季度报告"
  }
}
```

---

### 停止执行
```
POST /api/v1/workspaces/{ws}/executions/{id}/stop
```

**请求体：**
```json
{
  "record_id": 456
}
```

---

### 强制失败
```
POST /api/v1/workspaces/{ws}/executions/{id}/force-fail
```

**请求体：**
```json
{
  "record_id": 456
}
```

### 获取运行中的 Todo
```
GET /api/v1/workspaces/{ws}/executions/running-todos
```

---

### 获取仪表盘统计
```
GET /api/v1/workspaces/{ws}/stats/dashboard
```

---

### 执行 Action
```
POST /api/v1/actions/execute
```

**请求体：**
```json
{
  "action_type": "title_optimize",
  "content": "原始内容"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `action_type` | string | 是 | 操作类型（title_optimize / prompt_optimize 等） |
| `content` | string | 是 | 输入内容 |

---

## 5. 调度器

### 获取定时 Todo 列表
```
GET /api/v1/workspaces/{ws}/scheduler/todos
```

---

### 更新 Todo 调度配置
```
PUT /api/v1/workspaces/{ws}/todos/{id}/scheduler
```

**请求体：**
```json
{
  "scheduler_enabled": true,
  "scheduler_config": "0 9 * * *",
  "scheduler_timezone": "Asia/Shanghai"
}
```

---

## 6. 备份与恢复

### 导出全部数据
```
GET /api/v1/backup/export
```

返回 YAML 格式的完整备份。

---

### 导出选定的 Todo
```
POST /api/v1/backup/export-selected
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
POST /api/v1/backup/import
```

Content-Type: `multipart/form-data`

| 字段 | 类型 | 说明 |
|------|------|------|
| `file` | file | YAML 备份文件 |

---

### 合并导入
```
POST /api/v1/backup/merge
```

Content-Type: `multipart/form-data`

| 字段 | 类型 | 说明 |
|------|------|------|
| `file` | file | YAML 备份文件 |

---

### 下载数据库
```
GET /api/v1/backup/database/download
```

---

### 获取数据库备份状态
```
GET /api/v1/backup/database/status
```

---

### 触发立即备份
```
POST /api/v1/backup/database/trigger
```

---

### 更新自动备份配置
```
PUT /api/v1/backup/database/auto
```

**请求体：**
```json
{
  "enabled": true,
  "cron": "0 0 * * * *",
  "max_files": 30
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `enabled` | boolean | 是 | 是否启用自动备份 |
| `cron` | string | 是 | Cron 表达式（6 字段：秒 + 标准 5 字段） |
| `max_files` | number | 否 | 保留的最大备份文件数（默认 30，0 非法） |

---

### 优化数据库
```
POST /api/v1/backup/database/optimize
```

执行 SQLite `VACUUM`，回收已删除记录占用的磁盘空间。

---

---

### 下载备份文件
```
GET /api/v1/backup/database/file
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `filename` | string | 备份文件名 |

---

### 删除备份文件
```
DELETE /api/v1/backup/database/file
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `filename` | string | 备份文件名 |

---

### Todo 备份

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET`  | `/api/v1/backup/todo/status`  | 获取 Todo 备份状态与文件列表 |
| `POST` | `/api/v1/backup/todo/trigger` | 立即触发一次 Todo 备份 |
| `PUT`  | `/api/v1/backup/todo/auto`    | 更新 Todo 自动备份配置 |
| `GET`  | `/api/v1/backup/todo/file`    | 下载指定 Todo 备份文件（query `filename`） |
| `DELETE` | `/api/v1/backup/todo/file`  | 删除指定 Todo 备份文件（query `filename`） |

---

### Skill 备份

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET`  | `/api/v1/backup/skills/status`  | 获取 Skill 备份状态与文件列表 |
| `POST` | `/api/v1/backup/skills/trigger` | 立即触发一次 Skill 备份 |
| `PUT`  | `/api/v1/backup/skills/auto`    | 更新 Skill 自动备份配置 |
| `GET`  | `/api/v1/backup/skills/file`    | 下载指定 Skill 备份文件（query `filename`） |
| `DELETE` | `/api/v1/backup/skills/file`  | 删除指定 Skill 备份文件（query `filename`） |

---

### 日志清理

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET`  | `/api/v1/backup/log-cleanup/status`  | 获取日志清理策略与上次执行时间 |
| `PUT`  | `/api/v1/backup/log-cleanup`         | 更新日志清理策略 |
| `POST` | `/api/v1/backup/log-cleanup/trigger` | 立即触发一次日志清理 |

---

## 7. 配置管理

### 获取配置
```
GET /api/v1/config
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
      "mobilecoder": { "enabled": true }
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
PUT /api/v1/config
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
GET /api/v1/executors
```

---

### 更新执行器配置
```
PUT /api/v1/executors/{name}
```

**请求体：**
```json
{
  "path": "/usr/local/bin/claudecode",
  "enabled": true,
  "display_name": "Claude Code",
  "session_dir": "~/.ntd/sessions/claudecode"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | string | 否 | 可执行文件路径 |
| `enabled` | boolean | 否 | 是否启用 |
| `display_name` | string | 否 | 显示名 |
| `session_dir` | string | 否 | 会话/历史目录 |

---

### 检测执行器
```
POST /api/v1/executors/{name}/detect
```

检测执行器二进制文件是否存在。

---

### 批量检测所有执行器
```
POST /api/v1/executors/detect-all
```

遍历所有已知执行器，报告每个执行器的探测结果，便于一次刷新 UI 状态。

**响应示例：**
```json
{
  "code": 0,
  "data": {
    "results": [
      { "name": "claudecode", "display_name": "Claude Code", "binary_found": true, "path_resolved": "/usr/local/bin/claudecode", "enabled": true }
    ],
    "total": 9,
    "found_count": 7
  }
}
```

---

### 测试执行器
```
POST /api/v1/executors/{name}/test
```

运行 `executor --version` 测试配置是否正确。

---

## 9. 技能管理

### 列出技能
```
GET /api/v1/skills
```

按执行器分组列出所有技能。

---

### 比较技能
```
GET /api/v1/skills/compare
```

跨执行器的技能对比矩阵。

---

### 同步技能
```
POST /api/v1/skills/sync
```

**请求体：**
```json
{
  "skill_name": "code_review",
  "from_executor": "claudecode",
  "to_executors": ["mobilecoder", "kimi"]
}
```

---

### 获取技能调用记录
```
GET /api/v1/skills/invocations
```

---

### 记录技能调用
```
POST /api/v1/skills/invocations
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
GET /api/v1/skills/content
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `name` | string | 技能名称 |
| `executor` | string | 执行器 |

---

### 导出技能
```
GET /api/v1/skills/export
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
POST /api/v1/skills/import
```

Content-Type: `multipart/form-data`

| 字段 | 类型 | 说明 |
|------|------|------|
| `file` | file | 技能 .zip 文件 |

---

## 10. Agent Bot 管理

### 列出 Agent Bot
```
GET /api/v1/agent-bots
```

---

### 删除 Agent Bot
```
DELETE /api/v1/agent-bots/{id}
```

---

### 更新 Agent Bot 配置
```
PUT /api/v1/agent-bots/{id}/config
```

**请求体：**
```json
{
  "config": "{\"dm_enabled\":true,\"group_enabled\":true,\"group_require_mention\":true,\"echo_reply\":true}"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `config` | string | 是 | JSON 字符串，内容为 `BotConfig`：`dm_enabled` / `group_enabled` / `group_require_mention` / `echo_reply` 四个布尔字段 |

后端会校验 `config` 必须是合法 JSON，成功后会重启该 bot 的 listener 以应用新配置。

---

## 11. 飞书集成

### 初始化飞书 OAuth
```
POST /api/v1/agent-bots/feishu/init
```

---

### 开始飞书 OAuth
```
POST /api/v1/agent-bots/feishu/begin
```

---

### 轮询飞书 OAuth 状态（SSE 事件流）
```
GET /api/v1/agent-bots/feishu/poll-stream
```

Server-Sent Events 端点。建立连接后，后台会持续向飞书 OAuth 端点轮询授权状态，结果以事件形式推回前端，避免前端轮询时序问题。

查询参数：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `device_code` | string | 是 | `feishu/begin` 返回的设备码 |
| `interval` | number | 否 | 轮询间隔秒数（默认 5，范围 1-30） |
| `expire_in` | number | 否 | 总期限秒数（默认 600，范围 60-1800） |

事件名：

| 事件 | 说明 |
|------|------|
| `ping`  | 心跳，授权未完成时定期发送，数据为 `{"status":"waiting"}` |
| `result` | 终态结果（成功 / 超时 / `access_denied` / `expired_token`），payload 字段见下表 |
| `fail`  | 致命错误（HTTP 请求失败、响应解析失败、数据库写入失败等） |

`result` 事件 payload：

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | boolean | 是否授权成功 |
| `app_id` | string | 成功时返回飞书 `app_id` |
| `domain` | string | `feishu` / `lark` |
| `open_id` | string | 用户 open_id |
| `bot_name` | string | 探测到的 bot 显示名 |
| `bot_id` | number | 新建 bot 的数据库 ID |
| `error` | string | 失败原因（如 `timeout`、`access_denied`、`expired_token`） |

---

### 获取飞书推送配置
```
GET /api/v1/agent-bots/feishu/push
```

返回所有飞书 bot 的推送状态数组，每条记录字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `bot_id` | number | Bot ID |
| `push_level` | string | 推送级别：`disabled` / `p2p` / `group` / `both` |
| `p2p_receive_id` | string | 单聊推送目标的 open_id |
| `group_chat_id` | string | 群聊推送目标的 chat_id |
| `receive_id_type` | string | ID 类型：`open_id` / `chat_id` / `email` 等 |
| `p2p_response_enabled` | boolean | 是否响应单聊消息 |
| `group_response_enabled` | boolean | 是否响应群聊消息 |
| `p2p_debounce_secs` | number | 单聊消息合并去抖秒数（默认 20） |
| `group_debounce_secs` | number | 群聊消息合并去抖秒数（默认 20） |

---

### 更新飞书推送配置
```
PUT /api/v1/agent-bots/feishu/push
```

**请求体：**
```json
{
  "bot_id": 1,
  "push_level": "both",
  "p2p_receive_id": "ou_xxx",
  "group_chat_id": "oc_xxx",
  "receive_id_type": "open_id",
  "p2p_response_enabled": true,
  "group_response_enabled": true,
  "p2p_debounce_secs": 20,
  "group_debounce_secs": 20
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `bot_id` | number | 是 | 目标 bot ID |
| `push_level` | string | 否 | 推送级别 |
| `p2p_receive_id` | string | 否 | 单聊推送目标 |
| `group_chat_id` | string | 否 | 群聊推送目标 |
| `receive_id_type` | string | 否 | ID 类型 |
| `p2p_response_enabled` | boolean | 否 | 是否响应单聊 |
| `group_response_enabled` | boolean | 否 | 是否响应群聊 |
| `p2p_debounce_secs` | number | 否 | 单聊去抖秒数 |
| `group_debounce_secs` | number | 否 | 群聊去抖秒数 |

---

### 获取群组白名单
```
GET /api/v1/agent-bots/feishu/group-whitelist
```

查询参数：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `bot_id` | number | 必填 | 机器人 ID |

---

### 添加群组到白名单
```
POST /api/v1/agent-bots/feishu/group-whitelist
```

**请求体：**
```json
{
  "bot_id": 1,
  "sender_open_id": "ou_xxx",
  "sender_name": "张三"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `bot_id` | number | 是 | 机器人 ID |
| `sender_open_id` | string | 是 | 发送者 open_id（不允许为空） |
| `sender_name` | string | 否 | 发送者备注名 |

---

### 从白名单移除
```
DELETE /api/v1/agent-bots/feishu/group-whitelist/{id}
```

---

## 12. 飞书历史

### 获取历史消息
```
GET /api/v1/feishu/history-messages
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
GET /api/v1/feishu/message-stats
```

---

### 获取消息发送者列表
```
GET /api/v1/feishu/senders
```

---

### 获取历史聊天室列表
```
GET /api/v1/feishu/history-chats
```

---

### 创建历史聊天室
```
POST /api/v1/feishu/history-chats
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
DELETE /api/v1/feishu/history-chats/{id}
```

---

### 更新历史聊天室
```
PUT /api/v1/feishu/history-chats/{id}
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
GET /api/v1/sessions
```

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `page` | number | 页码 |
| `limit` | number | 每页数量 |

---

### 获取会话统计
```
GET /api/v1/sessions/stats
```

---

### 获取会话详情
```
GET /api/v1/sessions/{id}
```

包含会话消息和子代理信息。

---

### 删除会话
```
DELETE /api/v1/sessions/{id}
```

---

## 14. 项目目录

### 列出项目目录
```
GET /api/v1/project-directories
```

---

### 创建项目目录
```
POST /api/v1/project-directories
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
PUT /api/v1/project-directories/{id}
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
DELETE /api/v1/project-directories/{id}
```

---

## 15. Todo 模板

### 获取模板列表
```
GET /api/v1/todo-templates
```

---

### 创建模板
```
POST /api/v1/todo-templates
```

**请求体：**
```json
{
  "title": "Todo 标题模板",
  "prompt": "Prompt 模板",
  "category": "writing",
  "sort_order": 100
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `title` | string | 是 | 模板标题 |
| `prompt` | string | 否 | 模板 prompt 内容 |
| `category` | string | 是 | 模板分类（用于分组） |
| `sort_order` | number | 否 | 排序权重，数字越小越靠前 |

---

### 更新模板
```
PUT /api/v1/todo-templates/{id}
```

---

### 删除模板
```
DELETE /api/v1/todo-templates/{id}
```

---

### 复制模板
```
POST /api/v1/todo-templates/{id}/copy
```

---

## 16. 评审模板

### 获取评审模板列表
```
GET /api/v1/review-templates
```

---

### 创建评审模板
```
POST /api/v1/review-templates
```

**请求体：**
```json
{
  "name": "代码质量评审",
  "prompt_template": "请评审以下代码执行结果：\n{{output}}",
  "is_default": false
}
```

---

### 更新评审模板
```
PUT /api/v1/review-templates/{id}
```

---

### 删除评审模板
```
DELETE /api/v1/review-templates/{id}
```

---

### 获取评审模板选项
```
GET /api/v1/review-templates/options
```

返回所有评审模板的 id + name 列表，用于前端下拉选择。

---

## 17. Loop Studio

### 获取 Loop 列表
```
GET /api/v1/workspaces/{ws}/loops
```

---

### 创建 Loop
```
POST /api/v1/workspaces/{ws}/loops
```

**请求体：**
```json
{
  "name": "每日代码审查",
  "description": "自动审查当日提交的代码",
  "enabled": true
}
```

---

### 获取 Loop 详情
```
GET /api/v1/workspaces/{ws}/loops/{id}
```

---

### 更新 Loop
```
PUT /api/v1/workspaces/{ws}/loops/{id}
```

---

### 删除 Loop
```
DELETE /api/v1/workspaces/{ws}/loops/{id}
```

---

### 获取 Loop 步骤列表
```
GET /api/v1/workspaces/{ws}/loops/{id}/steps
```

---

### 创建 Loop 步骤
```
POST /api/v1/workspaces/{ws}/loops/{id}/steps
```

**请求体：**
```json
{
  "name": "代码分析",
  "prompt": "分析最近的代码变更",
  "executor": "claudecode",
  "sort_order": 100
}
```

---

### 更新 Loop 步骤
```
PUT /api/v1/workspaces/{ws}/loops/{loop_id}/steps/{step_id}
```

---

### 删除 Loop 步骤
```
DELETE /api/v1/workspaces/{ws}/loops/{loop_id}/steps/{step_id}
```

---

### 获取 Loop 触发器列表
```
GET /api/v1/workspaces/{ws}/loops/{id}/triggers
```

---

### 创建 Loop 触发器
```
POST /api/v1/workspaces/{ws}/loops/{id}/triggers
```

**请求体：**
```json
{
  "trigger_type": "cron",
  "config": "{\"cron\": \"0 9 * * *\", \"timezone\": \"Asia/Shanghai\"}",
  "enabled": true
}
```

支持的触发类型：`cron`、`feishu_command`、`feishu_message`、`todo_completed`、`tag_added`、`webhook`

---

### 更新 Loop 触发器
```
PUT /api/v1/workspaces/{ws}/loops/{loop_id}/triggers/{trigger_id}
```

---

### 删除 Loop 触发器
```
DELETE /api/v1/workspaces/{ws}/loops/{loop_id}/triggers/{trigger_id}
```

---

### 执行 Loop
```
POST /api/v1/workspaces/{ws}/loops/{id}/execute
```

**请求体：**
```json
{
  "message": "立即执行"
}
```

---

### 停止 Loop 执行
```
POST /api/v1/workspaces/{ws}/loops/{id}/stop
```

---

### 获取 Loop 执行记录
```
GET /api/v1/workspaces/{ws}/loops/{id}/executions
```

---

### 获取 Loop 执行详情
```
GET /api/v1/workspaces/{ws}/loops/executions/{execution_id}
```

---

### 获取 Loop 步骤执行记录
```
GET /api/v1/workspaces/{ws}/loops/executions/{execution_id}/steps
```

---

## 18. 自定义模板

### 获取订阅状态
```
GET /api/v1/custom-templates/status
```

---

### 订阅远程模板
```
POST /api/v1/custom-templates/subscribe
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
POST /api/v1/custom-templates/unsubscribe
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
POST /api/v1/custom-templates/sync
```

---

### 更新自动同步配置
```
PUT /api/v1/custom-templates/auto-sync
```

**请求体：**
```json
{
  "enabled": true,
  "cron": "0 */6 * * *"
}
```

---

## 19. 系统接口

### 健康检查
```
GET /health
```

无鉴权、无前缀；返回 `200 OK` 与 `{"status":"ok"}`，供负载均衡 / 探针使用。

---

### 获取版本信息
```
GET /api/v1/version
```

**响应示例：**
```json
{
  "code": 0,
  "data": {
    "version": "1.2.3",
    "git_sha": "abc123",
    "git_describe": "v1.2.3-4-gabc123"
  }
}
```

---

### 查询 npm 最新版本
```
GET /api/v1/version/latest
```

后端调用 `npm view @weibaohui/ntd version` 获取远程最新版本号，供前端做升级提示。

**响应示例：**
```json
{
  "code": 0,
  "data": { "latest": "1.2.4" }
}
```

失败时返回 `{"latest": null, "error": "..."}`，前端应做容错。

---

### 触发远程升级
```
POST /api/v1/version/upgrade
```

流程：调用 `npm install -g @weibaohui/ntd@latest` → fork 子进程执行 `daemon stop → uninstall → install --force → start`，让当前进程先返回响应后再重启服务。

**响应示例：**
```json
{
  "code": 0,
  "data": {
    "upgraded": true,
    "restarted": true,
    "npmOutput": "...",
    "restartMessage": "npm 升级成功，正在后台重新部署服务，请稍后刷新页面"
  }
}
```

---

## 20. WebSocket 事件

### 事件订阅
```
GET /api/events
```

通过 WebSocket 连接接收实时事件。

**事件类型：**

| 事件 | 说明 | 关键 payload 字段 |
|------|------|------------------|
| `sync` | 客户端连接后立即发送的当前任务快照，前端应据此重建 `runningTasks` 列表 | `tasks: TaskInfo[]` |
| `todo.created` | Todo 创建 | - |
| `todo.updated` | Todo 更新 | - |
| `todo.deleted` | Todo 删除 | - |
| `execution.started` | 执行开始（对应 `ExecEvent::Started`） | `task_id`, `todo_id`, `todo_title`, `executor` |
| `execution.progress` | 执行进度（对应 `ExecEvent::Output`） | `task_id`, `entry: ParsedLogEntry` |
| `todo.progress` | Todo 子项进度（对应 `ExecEvent::TodoProgress`） | `task_id`, `progress: TodoItem[]` |
| `execution.stats` | 执行统计（对应 `ExecEvent::ExecutionStats`） | `task_id`, `stats: { tool_calls, conversation_turns, thinking_count }` |
| `execution.completed` | 执行完成（对应 `ExecEvent::Finished`, `success=true`） | `task_id`, `todo_id`, `result` |
| `execution.failed` | 执行失败（对应 `ExecEvent::Finished`, `success=false`） | `task_id`, `todo_id`, `result` |

> 后端用 `#[serde(tag = "type")]` 把枚举变体序列化为对应事件名，因此前端可通过 `event.type` 直接判别。

**消息格式：**
```json
{
  "type": "execution.progress",
  "task_id": "abc-123",
  "entry": {
    "timestamp": "2026-05-14T10:00:00.000Z",
    "type": "info",
    "content": "正在分析代码..."
  }
}
```

---

## 21. Webhook 触发

Webhook 不再作为“设置中心里的独立资源”管理，而是 Todo / Loop 的内建能力开关。

### 外部触发 Todo
```
GET  /api/v1/webhooks/todo/{id}/trigger
POST /api/v1/webhooks/todo/{id}/trigger
```

### 外部触发 Loop
```
GET  /api/v1/webhooks/loop/{id}/trigger
POST /api/v1/webhooks/loop/{id}/trigger
```

注意：路径带 `/api/v1/webhooks` 前缀，触发动作作为子资源放在末尾（`{resource}/{id}/trigger`），更符合 REST 的「资源 → 子资源」层级感。当目标 Todo/Loop 未启用 webhook 时会返回 400。

---

## 22. 使用统计

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET`  | `/api/v1/usage-stats`            | 获取当前使用统计快照 |
| `POST` | `/api/v1/usage-stats/refresh`    | 主动触发一次使用统计刷新 |
| `GET`  | `/api/v1/usage-stats/settings`   | 获取统计采集配置 |
| `PUT`  | `/api/v1/usage-stats/settings`   | 更新统计采集配置 |

---

## 23. 云端同步

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET`  | `/api/v1/cloud/config`         | 获取云端同步配置 |
| `POST` | `/api/v1/cloud/config`         | 保存云端同步配置 |
| `GET`  | `/api/v1/cloud/sync/status`    | 获取最近一次同步状态（成功/失败/时间戳） |
| `GET`  | `/api/v1/cloud/sync/records`   | 列出已同步到云端的记录 |
| `DELETE` | `/api/v1/cloud/sync/records` | 清空云端同步记录 |
| `POST` | `/api/v1/cloud/sync/push`      | 推送本地变更到云端 |
| `POST` | `/api/v1/cloud/sync/pull`      | 从云端拉取变更到本地 |

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
  "code": 40001,
  "message": "错误描述",
  "data": null
}
```

### 业务错误码

| 常量 | 值 | HTTP 状态 | 说明 |
|------|------|-----------|------|
| `NOT_FOUND`  | `40001` | 404 | 资源不存在 |
| `BAD_REQUEST` | `40002` | 400 | 请求参数错误 |
| `INTERNAL`   | `50001` | 500 | 服务器内部错误 |

> 业务错误码定义在 `backend/src/models/mod.rs` 的 `codes` 模块；自定义业务错误应继续按该范围扩展，避免与 `0`（成功）冲突。

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

`TodoStatus`（`backend/src/models/mod.rs`）的合法取值：

| 状态值 | 说明 |
|--------|------|
| `pending` | 等待执行 |
| `in_progress` | 已开始但尚未运行（中间态） |
| `running` | 执行中 |
| `completed` | 已完成 |
| `failed` | 执行失败 |
| `cancelled` | 已取消 |

`ExecutionStatus` 的合法取值：`running` / `success` / `failed`。

---

## 执行器类型

| 类型 | 说明 |
|------|------|
| `claudecode` | Claude Code |
| `mobilecoder` | MobileCoder |
| `codebuddy` | CodeBuddy |
| `opencode` | OpenCode |
| `atomcode` | AtomCode |
| `hermes` | Hermes |
| `kimi` | Kimi |
| `codex` | Codex |
| `codewhale` | CodeWhale |
| `pi` | Pi |
| `mimo` | MiMo |
| `zhanlu` | Zhanlu |
| `kilo` | Kilo |
