# ntd 命令行文档

AI Todo CLI 完整命令参考手册。

## 全局选项

| 选项 | 简写 | 默认值 | 说明 |
|------|------|--------|------|
| `--server <URL>` | - | `http://localhost:8088` | API 服务器地址 |
| `--output <FORMAT>` | `-o` | `json` | 输出格式：`json`, `pretty`, `raw` |
| `--fields <FIELDS>` | `-f` | - | 指定输出的字段，逗号分隔 |

### 输出格式说明

- `json` - 标准 JSON 输出（带 ApiResponse 包装）
- `pretty` - 格式化后的 JSON（便于阅读）
- `raw` - 原始数据（无 ApiResponse 包装，适合 AI 解析）

---

## 命令分类

### 1. 信息命令

#### `ntd version`
显示版本信息。

```bash
ntd version
```

#### `ntd upgrade`
通过 npm 升级 ntd 到最新版本。

```bash
ntd upgrade
```

#### `ntd stats`
获取全局统计数据（仪表盘统计）。

```bash
ntd stats
```

---

### 2. 服务器命令

#### `ntd server start`
启动 API 服务器。

```bash
ntd server start [OPTIONS]

OPTIONS:
  -p, --port <PORT>  监听端口（默认: 8088）
```

**示例：**
```bash
ntd server start --port 8088
```

---

### 3. Todo 管理命令

#### `ntd todo create`
创建新的 Todo。

```bash
ntd todo create [TITLE] [OPTIONS]
```

> 注意：`<TITLE>` 是位置参数，**没有** `-t` 短别名。

**位置参数：**

| 参数 | 必填 | 说明 |
|------|------|------|
| `TITLE` | 否（与 `--stdin` 二选一必填） | Todo 标题 |

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--prompt <TEXT>` | `-p` | Prompt 内容 |
| `--file <PATH>`   | `-f` | 从文件读取 prompt |
| `--stdin`         | -   | 从 stdin 读取 JSON 数据（用于复杂字段如 `hooks`） |
| `--executor <TYPE>` | `-e` | 执行器类型 |
| `--workspace <PATH>` | `-w` | 工作目录 |
| `--tags <IDs>`    | -   | 标签 ID（逗号分隔） |
| `--schedule <CRON>` | - | 定时计划（Cron 表达式，传空字符串可清空） |

**执行器类型：**
- `claudecode` - Claude Code
- `mobilecoder` - MobileCoder
- `codebuddy` - CodeBuddy
- `opencode` - OpenCode
- `atomcode` - AtomCode
- `hermes` - Hermes
- `kimi` - Kimi
- `codex` - Codex
- `codewhale` - CodeWhale

**示例：**
```bash
# 创建简单 Todo（标题是位置参数）
ntd todo create "完成报告" --prompt "写一份季度报告"

# 从文件创建
ntd todo create "代码审查" --file ./prompt.txt

# 指定执行器和标签
ntd todo create "AI 任务" -p "使用 Claude 执行" -e claudecode --tags "1,2"

# 定时任务
ntd todo create "每日提醒" -p "检查日志" --schedule "0 9 * * *"

# 复杂字段用 --stdin
ntd todo create --stdin <<EOF
{
  "title": "复杂任务",
  "prompt": "...",
  "tag_ids": [1, 2],
  "scheduler_enabled": true,
  "scheduler_config": "0 0 9 * * *",
  "hooks": []
}
EOF
```

---

#### `ntd todo list`
列出 Todo 列表。

```bash
ntd todo list [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--status <STATUS>` | - | 按状态筛选 |
| `--tag <ID>` | - | 按标签 ID 筛选 |
| `--running` | - | 仅显示运行中的 Todo |
| `--search <KEYWORD>` | `-s` | 搜索标题或 prompt 关键词 |

**示例：**
```bash
# 列出所有 Todo
ntd todo list

# 筛选进行中的
ntd todo list --status running

# 按标签筛选
ntd todo list --tag 1

# 搜索
ntd todo list -s "报告"
```

---

#### `ntd todo get <ID>`
获取 Todo 详情。

```bash
ntd todo get <ID>
```

**示例：**
```bash
ntd todo get 123
```

---

#### `ntd todo update <ID>`
更新 Todo 信息。

```bash
ntd todo update <ID> [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--title <TITLE>` | - | 新标题 |
| `--prompt <TEXT>` | - | 新 prompt 内容 |
| `--file <PATH>`   | `-f` | 从文件读取 prompt |
| `--stdin`         | - | 从 stdin 读取 JSON 数据 |
| `--status <STATUS>` | - | 新状态 |
| `--executor <TYPE>` | - | 执行器类型 |
| `--workspace <PATH>` | - | 工作目录 |
| `--tags <IDs>`    | - | 标签 ID（逗号分隔） |
| `--schedule <CRON>` | - | 定时计划 |

**示例：**
```bash
# 更新标题和状态
ntd todo update 123 --title "新标题" --status completed

# 更新标签
ntd todo update 123 --tags "1,3"

# 复杂字段用 --stdin
ntd todo update 123 --stdin <<EOF
{
  "scheduler_enabled": true,
  "scheduler_config": "0 0 9 * * *",
  "hooks": []
}
EOF
```

---

#### `ntd todo delete <ID>`
删除 Todo。

```bash
ntd todo delete <ID>
```

**示例：**
```bash
ntd todo delete 123
```

---

#### `ntd todo execute <ID>`
执行 Todo。

```bash
ntd todo execute <ID> [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--message <MSG>` | `-m` | 附加消息 |
| `--executor <TYPE>` | - | 指定执行器 |
| `--param KEY=VALUE` | - | 模板占位符替换键值对，可重复传多次 |

> `--param` 接受 `key=value` 形式，可重复传递；最终会作为 `params` 字段一起发到 `POST /api/execute`，后端用 `{{key}}` 替换 prompt 中的占位符。

**示例：**
```bash
# 简单执行
ntd todo execute 123 -m "开始执行"

# 传占位符参数
ntd todo execute 123 \
  --param project_name=myproject \
  --param env=production \
  -m "部署到 {{env}}"
```

---

#### `ntd todo stop <ID>`
停止 Todo 执行。

```bash
ntd todo stop <ID>
```

**示例：**
```bash
ntd todo stop 123
```

---

#### `ntd todo stats <ID>`
获取 Todo 执行统计。

```bash
ntd todo stats <ID>
```

调用 `GET /api/todos/{id}/summary`，返回 `ExecutionSummary`：

| 字段 | 说明 |
|------|------|
| `total_executions` | 累计执行次数 |
| `success_count` / `failed_count` / `running_count` | 各状态计数 |
| `total_input_tokens` / `total_output_tokens` / `total_cache_read_tokens` / `total_cache_creation_tokens` | Token 用量 |
| `total_cost_usd` | 累计费用（USD） |

**示例：**
```bash
ntd todo stats 123
```

---

### 4. 执行记录命令

#### `ntd todo execution list <TODO_ID>`
列出 Todo 的执行记录。

```bash
ntd todo execution list <TODO_ID> [OPTIONS]
```

**选项：**

| 选项 | 默认值 | 说明 |
|------|--------|------|
| `--status <STATUS>` | - | 按状态筛选 |
| `--page <NUM>` | 1 | 页码 |
| `--limit <NUM>` | 20 | 每页数量 |

**示例：**
```bash
ntd todo execution list 123 --page 1 --limit 20
```

---

#### `ntd todo execution get <ID>`
获取执行记录详情。

```bash
ntd todo execution get <ID>
```

**示例：**
```bash
ntd todo execution get 456
```

---

#### `ntd todo execution resume <ID>`
从执行记录恢复对话。

```bash
ntd todo execution resume <ID> [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--message <MSG>` | `-m` | 发送的消息 |

**示例：**
```bash
ntd todo execution resume 456 -m "继续执行"
```

---

### 5. 标签管理命令

#### `ntd tag list`
列出所有标签。

```bash
ntd tag list
```

---

#### `ntd tag create <NAME>`
创建新标签。

```bash
ntd tag create <NAME> [OPTIONS]
```

**选项：**

| 选项 | 简写 | 默认值 | 说明 |
|------|------|--------|------|
| `--color <COLOR>` | `-c` | `#1890ff` | 标签颜色 |

**示例：**
```bash
ntd tag create "重要" --color "#ff4d4f"
```

---

#### `ntd tag delete <ID>`
删除标签。

```bash
ntd tag delete <ID>
```

**示例：**
```bash
ntd tag delete 1
```

---

### 6. 守护进程命令

#### `ntd daemon install`
安装 ntd 为系统守护进程。

```bash
ntd daemon install [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--force` | `-f` | 强制重新安装 |
| `--system` | - | 安装为系统级服务 |
| `--run-as-user <USER>` | - | 指定运行用户（仅 Linux 系统服务） |

**示例：**
```bash
# 安装为用户服务
ntd daemon install

# 强制重新安装
ntd daemon install --force

# 安装为系统服务（需要 sudo）
sudo ntd daemon install --system
```

---

#### `ntd daemon uninstall`
卸载守护进程服务。

```bash
ntd daemon uninstall [OPTIONS]
```

**选项：**

| 选项 | 说明 |
|------|------|
| `--system` | 卸载系统级服务 |

**示例：**
```bash
ntd daemon uninstall
sudo ntd daemon uninstall --system
```

---

#### `ntd daemon start`
启动守护进程。

```bash
ntd daemon start [OPTIONS]
```

**选项：**

| 选项 | 说明 |
|------|------|
| `--system` | 启动系统级服务 |

**示例：**
```bash
ntd daemon start
```

---

#### `ntd daemon stop`
停止守护进程。

```bash
ntd daemon stop [OPTIONS]
```

**选项：**

| 选项 | 说明 |
|------|------|
| `--system` | 停止系统级服务 |

**示例：**
```bash
ntd daemon stop
```

---

#### `ntd daemon restart`
重启守护进程。

```bash
ntd daemon restart [OPTIONS]
```

**选项：**

| 选项 | 说明 |
|------|------|
| `--system` | 重启系统级服务 |

**示例：**
```bash
ntd daemon restart
```

---

#### `ntd daemon status`
查看守护进程状态。

```bash
ntd daemon status [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--verbose` | `-v` | 显示详细状态和最近日志 |

**示例：**
```bash
# 简单状态
ntd daemon status

# 详细状态
ntd daemon status -v
```

---

## 7. 技能管理命令

> 用于将内嵌的 `ntd-usage` skill 安装到各执行器的技能目录，让 AI 助手在执行时能自动发现并加载 ntd 使用说明。
>
> 当前内嵌的 skill 是 `ntd-usage`，所有支持的执行器共享同一份内容，安装到各自的 skill 目录下。

#### `ntd skill install`
安装内嵌的 `ntd-usage` skill 到执行器技能目录。

```bash
ntd skill install [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--force` | `-f` | 强制重新安装（即使目录已存在） |
| `--executor <LIST>` | `-e` | 仅安装到指定执行器（逗号分隔，例如 `claudecode,atomcode`）；不传则安装到全部已知执行器 |

支持的执行器：`claudecode`、`hermes`、`codex`、`codebuddy`、`opencode`、`atomcode`、`kimi`、`mobilecoder`、`codewhale`。

**示例：**
```bash
# 安装到所有执行器（首次安装或大版本更新后推荐执行一次）
ntd skill install

# 仅安装到 Claude Code
ntd skill install --executor claudecode

# 强制重新安装（升级 skill 内容后使用）
ntd skill install --force

# 强制重装到指定执行器
ntd skill install --force --executor claudecode,atomcode
```

> `--executor` 显式传值时遇到未知执行器会报错退出；不传时未知执行器会被跳过并打印警告。

---

## 使用示例

### 完整工作流

```bash
# 1. 创建 Todo（标题是位置参数）
ntd todo create "开发新功能" -p "实现用户认证模块" -e claudecode

# 2. 查看列表
ntd todo list

# 3. 执行 Todo
ntd todo execute 1 -m "开始开发"

# 4. 查看执行记录
ntd todo execution list 1

# 5. 停止执行
ntd todo stop 1

# 6. 更新 Todo 状态
ntd todo update 1 --status in_progress

# 7. 删除 Todo
ntd todo delete 1
```

### 标签管理

```bash
# 创建标签
ntd tag create "重要" -c "#ff4d4f"
ntd tag create "紧急" -c "#faad14"

# 创建带标签的 Todo
ntd todo create "处理投诉" -p "回复用户投诉" --tags "1,2"

# 按标签筛选
ntd todo list --tag 1
```

### 定时任务

```bash
# 创建每小时执行的任务
ntd todo create "健康检查" -p "检查系统状态" --schedule "0 * * * *"

# 创建每天早上 9 点执行的任务
ntd todo create "日报" -p "发送每日报告" --schedule "0 9 * * *"
```

---

## 退出码

| 退出码 | 说明 |
|--------|------|
| 0 | 成功 |
| 1 | 错误（命令执行失败） |
| 2 | 参数错误 |
