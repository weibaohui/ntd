# ntd (Now Task, Done) - 需求规格说明书

> **最后核对日期**: 2026-06-08
> **状态**: 文档为早期版本，实际功能已远超本规格所述（详见 docs/FEATURES.md 和 docs/ARCHITECTURE_HEALTH_CHECK_REPORT.md）。
> 本文档保留作为历史需求基线。

## 1. 项目概述

### 项目名称
**ntd** (Now Task, Done)

### 项目类型
全栈应用 (Rust 后端 + React 前端 + 桌面端 + 飞书 Bot)

### 核心功能
通过 React + AntDesign 构建的本地 todo 管理应用，支持调用本地 AI CLI 执行任务，并将执行过程完整记录到执行历史中。当前内置 13 个执行器：`claudecode` / `codebuddy` / `opencode` / `atomcode` / `hermes` / `kimi` / `mobilecoder` / `codex` / `codewhale` / `pi` / `mimo` / `zhanlu` / `kilo`。

### 目标用户
需要任务管理并借助 AI 执行任务的技术用户

---

## 2. 功能需求

### 2.1 Todo 管理

#### 创建 Todo
- 标题（必填）
- 描述（可选）
- 分类/标签（可选，支持多标签）
- 创建时间（自动记录）

#### 编辑 Todo
- 修改标题、描述、标签
- 修改状态

#### 删除 Todo
- 软删除，支持恢复

#### Todo 状态
- `pending`: 待执行
- `in_progress`: 已就绪/准备执行（由后台调度器设置）
- `running`: 执行中
- `completed`: 已完成
- `failed`: 执行失败
- `cancelled`: 已取消

> 实际共有 6 个状态（参见 `backend/src/models/mod.rs:3-12 TodoStatus`）。

### 2.2 分类/标签管理

- 创建、编辑、删除标签
- 标签颜色自定义
- 按标签筛选 todo

### 2.3 任务执行

#### 执行方式
- 用户点击"执行"按钮后，任务在后台运行
- 不阻塞 UI，执行完成后通过系统通知告知用户

#### 调用 claudecode（示例）
- 命令格式：`claude --print --output-format stream-json --verbose "<任务描述>"`
- 通过 Rust 的 `std::process::Command` 执行系统命令
- 实时捕获 stdout 和 stderr

#### 执行过程记录
每条执行记录包含：
- 执行 ID
- 关联的 Todo ID
- 执行开始时间
- 执行结束时间
- 执行状态（success/failed/running）
- 完整的命令输出日志（stdout）
- 错误日志（stderr）
- 每一步操作的时间戳

### 2.4 执行历史

- 每个 Todo 关联多条执行记录
- 按执行时间倒序排列
- 可查看每次执行的完整日志
- 支持重新执行

---

## 3. 数据模型

> ⚠️ **本节为早期版本摘录**。当前数据库已扩展为 **28+ 张表**（详见 `backend/src/db/mod.rs::init_tables`），包括：
>
> - `todos` / `tags` / `todo_tags` / `todo_templates` / `project_directories`
> - `execution_records` / `execution_logs` / `usage_stats` / `usage_model_breakdown` / `usage_executor_daily`
> - `executors`（执行器元数据持久化表）
> - `agent_bots` / `feishu_homes` / `feishu_messages` / `feishu_history_chats` / `feishu_push_targets` / `feishu_response_config` / `feishu_group_whitelist` / `feishu_project_bindings`
> - `loops` / `loop_steps` / `loop_step_executions` / `loop_executions` / `loop_triggers` / `loop_tags`（Loop Studio）
> - `review_templates`（评审模板）
> - `sync_records`（云端同步）
> - `workspace_settings` / `workspace_slash_commands`
> - 以及 `hooks`（作为 `todos` 表的 JSON 列存储，见 `db/todo.rs`）
>
> 以下仅保留原 4 张核心表的早期字段定义。

### Todo
```
id: INTEGER PRIMARY KEY
title: TEXT NOT NULL
description: TEXT
status: TEXT DEFAULT 'pending'
created_at: DATETIME
updated_at: DATETIME
deleted_at: DATETIME NULL
```

### Tag
```
id: INTEGER PRIMARY KEY
name: TEXT NOT NULL
color: TEXT DEFAULT '#1890ff'
created_at: DATETIME
```

### TodoTag (多对多)
```
todo_id: INTEGER
tag_id: INTEGER
PRIMARY KEY (todo_id, tag_id)
```

### ExecutionRecord
```
id: INTEGER PRIMARY KEY
todo_id: INTEGER FOREIGN KEY
status: TEXT DEFAULT 'running'
command: TEXT
stdout: TEXT
stderr: TEXT
logs: TEXT (JSON 数组，每条日志包含 timestamp, type, content)
started_at: DATETIME
finished_at: DATETIME NULL
```

---

## 4. 技术栈

- **后端框架**: Rust + Axum 0.8
- **前端**: React 19.1.0 + TypeScript 5.8.3 + Vite 7.0.4
- **UI 组件**: Ant Design 6.3.6
- **状态管理**: React Context + useReducer
- **数据存储**: SQLite (via SeaORM 1.x + libsqlite3-sys bundled)
- **命令执行**: tokio Command + command-group 5（进程组管理）

---

## 5. 界面设计

### 5.1 主界面布局
- 左侧：标签筛选栏
- 中间：Todo 列表
- 右侧：Todo 详情/执行记录

### 5.2 页面结构
1. **首页**: Todo 列表 + 快速添加
2. **Todo 详情**: 包含执行历史记录
3. **标签管理**: 标签的增删改

### 5.3 交互流程
1. 用户创建 Todo
2. 点击"执行"按钮
3. 后台调用 claudecode CLI
4. 实时记录输出到当前执行记录
5. 执行完成后发送系统通知
6. 更新 Todo 状态

---

## 6. 非功能性需求

- **性能**: 界面响应时间 < 100ms
- **可靠性**: 执行记录不丢失，支持异常恢复
- **安全性**: 不记录敏感命令参数

---

## 7. 确认事项

- [x] claudecode CLI 命令格式: `claude --print --output-format stream-json --verbose "<任务描述>"`
- [x] 不需要执行超时设置
- [x] 不需要导出功能

## 8. 执行器集成

> 本节以 `claudecode` 为示例描述事件流格式。系统内置 13 个执行器（`claudecode` / `codebuddy` / `opencode` / `atomcode` / `hermes` / `kimi` / `mobilecoder` / `codex` / `codewhale` / `pi` / `mimo` / `zhanlu` / `kilo`），其注册与调度由 `backend/src/adapters/mod.rs::EXECUTORS` 数组统一管理。

- 命令路径: 由 `Config.executors.paths` 解析（HashMap，键为 executor 名，如 `claudecode -> "claude"`）
- 执行命令: 各 executor 通过实现 `CodeExecutor` trait 暴露 `executable_path` / `command_args` / `parse_output_line` 等
- 实时捕获 stdout 输出
- 记录完整的交互日志

### 8.1 命令格式

以 `claudecode` 为例：

```bash
claude --print --output-format stream-json --verbose "<任务描述>"
```

### 8.2 输出格式 (JSON)

输出为 NDJSON 格式（每行一个 JSON 对象），包含以下事件类型：

| 事件类型 | 说明 |
|---------|------|
| `step_start` | 步骤开始，包含 sessionID、messageID 等元信息 |
| `tool_use` | 工具调用，包含调用的工具名称、输入参数、执行状态和输出结果 |
| `text` | 文本响应，包含 AI 的思考过程或最终回复 |
| `step_finish` | 步骤完成，包含耗时、token 消耗等信息 |

#### 8.2.1 step_start 事件

```json
{
  "type": "step_start",
  "timestamp": 1776995480899,
  "sessionID": "ses_xxx",
  "part": {
    "id": "prt_xxx",
    "sessionID": "ses_xxx",
    "messageID": "msg_xxx",
    "type": "step-start"
  }
}
```

#### 8.2.2 tool_use 事件

```json
{
  "type": "tool_use",
  "timestamp": 1776995482985,
  "sessionID": "ses_xxx",
  "part": {
    "id": "prt_xxx",
    "sessionID": "ses_xxx",
    "messageID": "msg_xxx",
    "type": "tool",
    "callID": "call_xxx",
    "tool": "bash",
    "state": {
      "status": "completed",
      "input": {
        "command": "date",
        "description": "显示当前日期和时间"
      },
      "output": "2026年 04月 24日 星期五 09:51:22 CST\n",
      "title": "显示当前日期和时间",
      "metadata": {
        "output": "...",
        "exit": 0,
        "description": "...",
        "truncated": false
      },
      "time": {
        "start": 1776995482935,
        "end": 1776995482983
      }
    }
  }
}
```

#### 8.2.3 text 事件

```json
{
  "type": "text",
  "timestamp": 1776995483021,
  "sessionID": "ses_xxx",
  "part": {
    "id": "prt_xxx",
    "sessionID": "ses_xxx",
    "messageID": "msg_xxx",
    "type": "text",
    "text": "date 命令已经成功执行，显示了当前日期和时间。\n\n2026年 04月 24日 星期五 09:51:22 CST",
    "time": {
      "start": 1776995483015,
      "end": 1776995483015
    }
  }
}
```

#### 8.2.4 step_finish 事件

```json
{
  "type": "step_finish",
  "timestamp": 1776995483022,
  "sessionID": "ses_xxx",
  "part": {
    "id": "prt_xxx",
    "sessionID": "ses_xxx",
    "messageID": "msg_xxx",
    "type": "step-finish",
    "reason": "tool-calls",
    "cost": 0,
    "tokens": {
      "total": 14708,
      "input": 14661,
      "output": 47,
      "reasoning": 0,
      "cache": {"read": 0, "write": 0}
    }
  }
}
```

### 8.3 解析要点

1. **NDJSON 解析**: 输出是换行分隔的 JSON，需要按行分割后解析每个 JSON 对象
2. **tool_use 事件**: 包含 `tool` 字段标识工具类型（如 `bash`、`read`、`write` 等），`state.status` 表示执行状态（completed/failed）
3. **时间戳**: 所有 timestamp 都是毫秒级的 Unix 时间戳
4. **sessionID**: 用于追踪同一个执行会话
5. **exit code**: 通过 `part.state.metadata.exit` 获取命令退出码