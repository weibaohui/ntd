# ntd CLI 设计文档

## 概述

ntd CLI 是为 AI 交互设计的命令行工具，用于管理 AI 驱动的 Todo 任务。所有命令返回标准 JSON 格式结果，方便 AI 解析和处理。

## 设计原则

1. **参数简洁** - 适合人类通过对话与 AI 交互
2. **JSON 输出** - 所有返回结果均为标准 JSON
3. **文件支持** - 支持从文件读取长文本（如 prompt）
4. **状态透明** - 执行结果包含完整的上下文信息

---

## 全局参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `--server` | API 服务器地址 | `http://localhost:8088`（由 `config.rs:13 DEFAULT_PORT=8088` 决定） |
| `--output`, `-o` | 输出格式 (`json` / `pretty` / `raw`) | `json` |
| `--fields`, `-f` | 字段选择（仅 `--output raw` 生效，逗号分隔） | - |
| `--help`, `-h` | 显示帮助信息 | - |
| `--version`, `-v` | 显示版本信息 | - |

> **注意**: `--output pretty` 会格式化 JSON 输出，便于人类阅读；`--output json` 输出紧凑 JSON，适合 AI 解析；`--output raw` 仅输出 `ApiResponse.data` 内容（不包裹 `code` / `message` 包装），最适合作脚本管道。

---

## 子命令列表

> 完整子命令清单来自 `backend/src/main.rs` 的 `Commands` 枚举与 `backend/src/cli/commands.rs`：
>
> - 顶层：`version` / `upgrade` / `server`（含 `start`）/ `todo` / `tag` / `stats` / `daemon`（含 `install` / `uninstall` / `start` / `stop` / `restart` / `status`）/ `skill`（含 `install`）
> - `todo` 下：`create` / `list` / `get` / `update` / `delete` / `execute` / `stop` / `stats` / `execution`（含 `list` / `get` / `resume`）
> - `tag` 下：`list` / `create` / `delete`
>
> 下方罗列常用子命令的详细参数表。

### 1. `ntd todo create` - 创建 Todo

**功能**: 创建一个新的 AI 执行任务。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `title` | string | 是（与 `--stdin` 互斥时必填） | Todo 标题 |
| `prompt` / `-p` | string | 否 | 执行提示词（与 `--file` 二选一） |
| `--file`, `-f` | string | 否 | 从文件读取 prompt 内容 |
| `--stdin` | bool | 否 | 从 stdin 读取整个 Todo JSON（`title` 可省略） |
| `--executor` / `-e` | string | 否 | 执行器类型 (`claudecode`, `mobilecoder`, `codebuddy`, `opencode`, `atomcode`, `hermes`, `kimi`, `codex`, `codewhale`, `pi`) |
| `--workspace`, `-w` | string | 否 | 工作目录路径 |
| `--tags` | string | 否 | 标签 ID 列表，逗号分隔（如 `1,2,3`） |
| `--schedule` | string | 否 | Cron 表达式，启用定时执行（如 `0 9 * * 1` 表示每周一 9:00） |

**示例**:

```bash
# 基本创建
ntd todo create "完成周报" "请帮我整理本周工作并生成周报"

# 从文件读取 prompt
ntd todo create "重构代码" --file ./prompt.txt

# 指定执行器和工作目录
ntd todo create "代码审查" --executor claudecode --workspace /project --tags 1,3

# 启用定时执行
ntd todo create "每日报告" --file ./daily_report.txt --schedule "0 9 * * *"
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "id": 1,
    "title": "完成周报",
    "prompt": "请帮我整理本周工作并生成周报",
    "status": "pending",
    "executor": "claudecode",
    "scheduler_enabled": false,
    "workspace": null,
    "tag_ids": [],
    "created_at": "2024-01-15T08:00:00Z",
    "updated_at": "2024-01-15T08:00:00Z"
  }
}
```

---

### 2. `ntd todo list` - 查询 Todo 列表

**功能**: 获取所有 Todo 列表，支持筛选。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--status` | string | 否 | 按状态筛选 (`pending`, `in_progress`, `running`, `completed`, `failed`, `cancelled`) |
| `--tag` | integer | 否 | 按标签 ID 筛选 |
| `--running` | flag | 否 | 仅显示运行中的 Todo |
| `--search` / `-s` | string | 否 | 按关键字搜索 title 或 prompt |

**示例**:

```bash
# 获取所有 Todo
ntd todo list

# 仅显示待处理的 Todo
ntd todo list --status pending

# 按标签筛选
ntd todo list --tag 1

# 仅显示运行中的
ntd todo list --running
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": [
    {
      "id": 1,
      "title": "完成周报",
      "prompt": "请帮我整理本周工作并生成周报",
      "status": "pending",
      "executor": "claudecode",
      "scheduler_enabled": false,
      "scheduler_next_run_at": null,
      "tag_ids": [1, 3],
      "created_at": "2024-01-15T08:00:00Z",
      "updated_at": "2024-01-15T08:00:00Z"
    },
    {
      "id": 2,
      "title": "代码重构",
      "prompt": "重构用户模块代码",
      "status": "running",
      "executor": "mobilecoder",
      "scheduler_enabled": true,
      "scheduler_next_run_at": "2024-01-22T09:00:00Z",
      "tag_ids": [2],
      "created_at": "2024-01-14T10:00:00Z",
      "updated_at": "2024-01-15T11:30:00Z"
    }
  ]
}
```

---

### 3. `ntd todo get` - 获取 Todo 详情

**功能**: 获取单个 Todo 的详细信息。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | integer | 是 | Todo ID |

**示例**:

```bash
ntd todo get 1
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "id": 1,
    "title": "完成周报",
    "prompt": "请帮我整理本周工作并生成周报",
    "status": "completed",
    "executor": "claudecode",
    "workspace": "/home/user/project",
    "scheduler_enabled": false,
    "scheduler_config": null,
    "scheduler_next_run_at": null,
    "task_id": null,
    "tag_ids": [1, 3],
    "created_at": "2024-01-15T08:00:00Z",
    "updated_at": "2024-01-15T09:30:00Z"
  }
}
```

---

### 4. `ntd todo update` - 更新 Todo

**功能**: 更新 Todo 的属性。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | integer | 是 | Todo ID |
| `--title` | string | 否 | 新标题 |
| `--prompt` / `-p` | string | 否 | 新 prompt（与 `--file` 二选一） |
| `--file`, `-f` | string | 否 | 从文件读取 prompt |
| `--stdin` | bool | 否 | 从 stdin 读取更新 JSON |
| `--status` | string | 否 | 新状态 |
| `--executor` / `-e` | string | 否 | 新执行器类型 |
| `--workspace`, `-w` | string | 否 | 新工作目录 |
| `--tags` | string | 否 | 新标签 ID 列表 |
| `--schedule` | string | 否 | 新 Cron 表达式（设为空取消调度） |

**示例**:

```bash
# 更新标题和 prompt
ntd todo update 1 --title "完成月报" --prompt "整理本月工作内容"

# 从文件更新 prompt
ntd todo update 1 --file ./new_prompt.txt

# 更新状态为 pending
ntd todo update 1 --status pending

# 取消定时调度
ntd todo update 1 --schedule ""
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "id": 1,
    "title": "完成月报",
    "prompt": "整理本月工作内容",
    "status": "pending",
    "executor": "claudecode",
    "scheduler_enabled": false,
    "updated_at": "2024-01-15T12:00:00Z"
  }
}
```

---

### 5. `ntd todo delete` - 删除 Todo

**功能**: 删除指定的 Todo。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | integer | 是 | Todo ID |

**示例**:

```bash
ntd todo delete 1
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": null
}
```

---

### 6. `ntd todo execute` - 执行 Todo

**功能**: 手动触发执行一个 Todo。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | integer | 是 | Todo ID |
| `--message`, `-m` | string | 否 | 执行时的额外消息 |
| `--executor` / `-e` | string | 否 | 覆盖 Todo 的执行器 |
| `--param key=value` | string，可重复 | 否 | 占位符参数（替换 prompt 中的占位变量） |

**示例**:

```bash
# 基本执行
ntd todo execute 1

# 带额外消息
ntd todo execute 1 --message "请优先处理用户反馈的问题"

# 使用指定执行器
ntd todo execute 1 --executor mobilecoder
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "todo_id": 1,
    "task_id": "550e8400-e29b-41d4-a716-446655440000",
    "status": "running",
    "executor": "claudecode",
    "started_at": "2024-01-15T14:30:00Z"
  }
}
```

---

### 7. `ntd todo stop` - 停止 Todo 执行

**功能**: 停止正在运行的 Todo 执行。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | integer | 是 | Todo ID 或 Task ID |

**示例**:

```bash
ntd todo stop 1
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "todo_id": 1,
    "task_id": "550e8400-e29b-41d4-a716-446655440000",
    "status": "cancelled",
    "finished_at": "2024-01-15T14:35:00Z"
  }
}
```

---

### 8. `ntd todo stats` - 获取 Todo 执行统计

**功能**: 获取 Todo 的执行统计摘要。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | integer | 是 | Todo ID |

**示例**:

```bash
ntd todo stats 1
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "todo_id": 1,
    "title": "完成周报",
    "total_executions": 5,
    "success_executions": 4,
    "failed_executions": 1,
    "avg_duration_ms": 45000,
    "total_input_tokens": 12500,
    "total_output_tokens": 8900,
    "total_cost_usd": 0.25
  }
}
```

---

### 9. `ntd todo execution list` - 查询执行记录列表

**功能**: 获取 Todo 的历史执行记录。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | integer | 是 | Todo ID |
| `--status` | string | 否 | 按状态筛选 (`running`, `success`, `failed`) |
| `--page` | integer | 否 | 页码（从 1 开始） |
| `--limit` | integer | 否 | 每页数量 |

**示例**:

```bash
# 获取 Todo #1 的执行记录
ntd todo execution list 1

# 获取失败记录
ntd todo execution list 1 --status failed

# 分页
ntd todo execution list 1 --page 2 --limit 10
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "records": [
      {
        "id": 10,
        "todo_id": 1,
        "status": "success",
        "trigger_type": "manual",
        "executor": "claudecode",
        "model": "claude-sonnet-4-20250514",
        "started_at": "2024-01-15T14:30:00Z",
        "finished_at": "2024-01-15T14:35:00Z",
        "duration_ms": 300000,
        "usage": {
          "input_tokens": 1500,
          "output_tokens": 800,
          "total_cost_usd": 0.02
        }
      }
    ],
    "pagination": {
      "page": 1,
      "limit": 20,
      "total": 45
    }
  }
}
```

---

### 10. `ntd todo execution get` - 获取执行记录详情

**功能**: 获取单条执行记录的完整信息。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | integer | 是 | 执行记录 ID |

**示例**:

```bash
ntd todo execution get 10
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "id": 10,
    "todo_id": 1,
    "status": "success",
    "command": "claudecode --resume ...",
    "stdout": "正在分析代码结构...",
    "stderr": "",
    "logs": [
      {"timestamp": "2024-01-15T14:30:01Z", "type": "info", "content": "开始执行"},
      {"timestamp": "2024-01-15T14:30:05Z", "type": "output", "content": "正在分析..."}
    ],
    "result": "已完成代码重构",
    "usage": {
      "input_tokens": 1500,
      "output_tokens": 800,
      "total_cost_usd": 0.02,
      "cache_read_input_tokens": 500
    },
    "executor": "claudecode",
    "model": "claude-sonnet-4-20250514",
    "started_at": "2024-01-15T14:30:00Z",
    "finished_at": "2024-01-15T14:35:00Z",
    "trigger_type": "manual",
    "task_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

---

### 11. `ntd tag list` - 查询标签列表

**功能**: 获取所有标签。

**参数**: 无

**示例**:

```bash
ntd tag list
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": [
    {"id": 1, "name": "工作", "color": "#1890ff", "created_at": "2024-01-10T08:00:00Z"},
    {"id": 2, "name": "个人", "color": "#52c41a", "created_at": "2024-01-10T08:00:00Z"}
  ]
}
```

---

### 12. `ntd tag create` - 创建标签

**功能**: 创建一个新标签。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 标签名称 |
| `--color`, `-c` | string | 否 | 标签颜色（十六进制，如 `#FF5733`） |

**示例**:

```bash
ntd tag create "紧急" --color "#FF5733"
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "id": 3,
    "name": "紧急",
    "color": "#FF5733",
    "created_at": "2024-01-15T15:00:00Z"
  }
}
```

---

### 13. `ntd tag delete` - 删除标签

**功能**: 删除指定的标签。

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | integer | 是 | 标签 ID |

**示例**:

```bash
ntd tag delete 3
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": null
}
```

---

### 14. `ntd stats` - 获取仪表盘统计

**功能**: 获取全局统计数据。

**参数**: 无

**示例**:

```bash
ntd stats
```

**返回示例**:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "total_todos": 10,
    "pending_todos": 3,
    "running_todos": 1,
    "completed_todos": 5,
    "failed_todos": 1,
    "total_tags": 4,
    "scheduled_todos": 2,
    "total_executions": 45,
    "success_executions": 40,
    "failed_executions": 5,
    "total_input_tokens": 125000,
    "total_output_tokens": 89000,
    "total_cost_usd": 2.50,
    "avg_duration_ms": 45000
  }
}
```

---

## 错误响应格式

当命令执行失败时，实际的 stderr 输出格式为（参见 `backend/src/main.rs:250-256 print_structured_error`）：

```json
{"error": true, "message": "Not found"}
```

> 旧文档把错误响应写成 `{"code":40001, "message":"...", "data":null}` 的形式，**该格式未在代码中实现**。当前 ntd 的错误响应只有 `error: bool` + `message: string` 两个字段。
> 注意：HTTP API（`/api/...`）会返回 `ApiResponse<T>` 包装（包含 `code` / `message` / `data`），但 CLI 自身失败（如网络错误、参数错误）走 `print_structured_error`，格式是上方的简化版。

---

## 补充：缺失子命令速查

| 子命令 | 用途 |
|--------|------|
| `ntd version` | 输出版本号、git SHA、tag |
| `ntd upgrade` | 通过 npm 升级到最新版；执行后自动重装 daemon |
| `ntd server start [--port N]` | 显式启动 API server（不传子命令时也是默认行为） |
| `ntd daemon install [-f] [--system] [--run-as-user U]` | 安装为系统服务（launchd/systemd/Task Scheduler） |
| `ntd daemon uninstall [--system]` | 卸载服务 |
| `ntd daemon start [--system]` | 启动服务 |
| `ntd daemon stop [--system]` | 停止服务 |
| `ntd daemon restart [--system]` | 重启服务 |
| `ntd daemon status [--system] [-v]` | 查看状态（`-v` 追加近期日志） |
| `ntd skills install [-f] [--all] [-e EX1,EX2,...]` | 安装 ntd-usage skill 到执行器 skills 目录 |
| `ntd todo execution resume <id> [-m MSG]` | 沿用原 `session_id` 继续对话（仅 `claudecode` / `codex` / `hermes` / `kimi` / `atomcode` / `opencode` / `mobilecoder` / `codewhale` / `pi` 支持） |

---

## AI 交互示例

以下是 AI 与 CLI 交互的典型场景：

### 场景 1: AI 帮助用户创建并执行 Todo

```
用户: 请帮我创建一个 Todo，任务是根据 prompts/analyze.txt 文件中的要求，分析 /project 代码库。

AI:
$ ntd todo create "代码分析" --file ./prompts/analyze.txt --workspace /project --executor claudecode

{
  "code": 0,
  "message": "ok",
  "data": {
    "id": 15,
    "title": "代码分析",
    "status": "pending",
    ...
  }
}

$ ntd todo execute 15

{
  "code": 0,
  "message": "ok",
  "data": {
    "todo_id": 15,
    "task_id": "123e4567-e89b-12d3-a456-426614174000",
    "status": "running"
  }
}
```

### 场景 2: AI 查询执行状态

```
用户: 查看 Todo #1 的最近执行情况

AI:
$ ntd todo execution list 1

{
  "code": 0,
  "message": "ok",
  "data": {
    "records": [
      {
        "id": 10,
        "status": "success",
        "started_at": "2024-01-15T14:30:00Z",
        "finished_at": "2024-01-15T14:35:00Z"
      }
    ]
  }
}
```

### 场景 3: AI 停止卡住的任务

```
用户: 有一个任务好像卡住了，ID 是 5

AI:
$ ntd todo stop 5

{
  "code": 0,
  "message": "ok",
  "data": {
    "todo_id": 5,
    "status": "cancelled"
  }
}
```

---

## 实现建议

1. **使用 clap 或structopt** 进行 CLI 参数解析
2. **使用 reqwest** 发送 HTTP 请求到后端 API
3. **使用 serde_json** 处理 JSON 序列化/反序列化
4. **添加 `--output pretty`** 选项，格式化输出便于调试
5. **添加进度条** 用于长时间运行的命令（可选）
6. **支持配置文件** (`~/.ntd/config.toml`) 存储默认服务器地址等设置
