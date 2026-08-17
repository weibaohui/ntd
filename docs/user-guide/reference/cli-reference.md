# ntd 命令行文档

ntd CLI 完整命令参考手册。

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
- `pi` - Pi
- `mimo` - MiMo
- `zhanlu` - Zhanlu
- `kilo` - Kilo

**示例：**
```bash
# 创建简单 Todo（标题是位置参数）
ntd todo create "完成报告" --prompt "写一份季度报告"

# 从文件创建
ntd todo create "代码审查" --file ./prompt.txt

# 指定执行器
ntd todo create "AI 任务" -p "使用 Claude 执行" -e claudecode

# 定时任务
ntd todo create "每日提醒" -p "检查日志" --schedule "0 9 * * *"

# 复杂字段用 --stdin
ntd todo create --stdin <<EOF
{
  "title": "复杂任务",
  "prompt": "...",
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
| `--running` | - | 仅显示运行中的 Todo |
| `--search <KEYWORD>` | `-s` | 搜索标题或 prompt 关键词 |

**示例：**
```bash
# 列出所有 Todo
ntd todo list

# 筛选进行中的
ntd todo list --status running

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
| `--schedule <CRON>` | - | 定时计划 |

**示例：**
```bash
# 更新标题和状态
ntd todo update 123 --title "新标题" --status completed

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

### 5. Loop 管理命令

#### `ntd loop list`
列出所有 Loop。

```bash
ntd loop list
```

---

#### `ntd loop get <ID>`
获取 Loop 详情。

```bash
ntd loop get <ID>
```

---

#### `ntd loop update <ID>`
更新 Loop 信息。

```bash
ntd loop update <ID> [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--name <NAME>` | - | 新名称 |
| `--description <DESC>` | - | 新描述 |
| `--enabled <BOOL>` | - | 是否启用 |

---

#### `ntd loop delete <ID>`
删除 Loop。

```bash
ntd loop delete <ID>
```

---

#### `ntd loop stop <ID>`
停止 Loop 执行。

```bash
ntd loop stop <ID>
```

---

#### `ntd loop stats <ID>`
获取 Loop 执行统计。

```bash
ntd loop stats <ID> [OPTIONS]
```

**选项：**

| 选项 | 简写 | 默认值 | 说明 |
|------|------|--------|------|
| `--recent <NUM>` | - | 5 | 显示最近执行次数 |

---

#### `ntd loop execute <ID>`
执行 Loop（立即触发）。

```bash
ntd loop execute <ID> [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--param KEY=VALUE` | - | 模板占位符替换键值对，可重复传多次 |

---

#### `ntd loop execution list <LOOP_ID>`
列出 Loop 执行记录。

```bash
ntd loop execution list <LOOP_ID>
```

---

#### `ntd loop execution get <EXECUTION_ID>`
获取 Loop 执行详情。

```bash
ntd loop execution get <EXECUTION_ID>
```

---

#### `ntd loop execution blackboard <EXECUTION_ID>`
查看 Loop 执行的黑板视图：每个 step 的状态、执行记录 ID（exec）和结论摘要。
**默认输出 JSON**（AI/脚本友好），加 `--human` 输出人类可读黑板文本。

**设计动机**：每一步的 `conclusion` 字段就是该步骤写入黑板的内容。
Loop 执行过程中，下一步的 prompt 通过 `{{blackboard}}`、`{{last_output}}`、`{{last_conclusion}}` 等占位符读取此前的累计输出。
此命令把这个机制从运行期搬到 CLI 调试期。

```bash
ntd loop execution blackboard <EXECUTION_ID>            # 默认 JSON
ntd loop execution blackboard <EXECUTION_ID> --human    # 人类可读黑板视图
```

**选项：**

| 选项 | 说明 |
|------|------|
| `<EXECUTION_ID>` | 必填，loop execution 主键 |
| `--human` | 输出人类可读黑板视图（默认是 JSON） |

**为什么默认 JSON**：CLI 主要消费者是 AI（Claude Code 等）和 shell 脚本，jq/grep 友好是绝对主流。人类有更好的 UI（前端 BlackboardDrawer），需要时显式加 `--human`。

**示例输出**（JSON，默认）：

```json
{
  "id": 1105,
  "loop_name": "笑话工厂",
  "status": "success",
  "step_executions": [
    {
      "sequence_index": 1,
      "step_name": "讲个笑话",
      "status": "success",
      "execution_record_id": 1137,
      "conclusion": "为什么程序员总是分不清万圣节和圣诞节？因为 Oct 31 等于 Dec 25！"
    }
  ],
  "token_summary": {
    "total_input_tokens": 13003,
    "total_output_tokens": 628
  }
}
```

**示例输出**（`--human`）：

```
═══ Loop Execution #42 ────────────────────────────────
循环: 每日代码 review
触发: cron @ 0 9 * * *
状态: ✅ success · 完成 3/3 步
开始: 2026-07-03 09:00:00 · 结束: 09:45:32

  #1 ✅ success          编写 CRUD 代码             评分 85
     exec: #1024
     完成了用户登录功能的 CRUD 代码

  #2 ✅ success          补充单元测试               评分 90
     exec: #1025
     新增 12 个测试用例，覆盖率提升到 87%

  #3 ⏭️ skipped          更新 README                 评分 -
     exec: -
     (无结论)

═══ 3 步 / Token: 输入 12k 输出 5k ════════════════════════
```

**状态图标**（仅 `--human` 模式）：`success` ✅ · `failed` ❌ · `running` ⏳ · `pending` ⏸ · `pending_approval` 🤔 · `skipped` ⏭️

**边界处理**：
- execution 不存在：返回 `{"error":true,"message":"..."}`，exit 1
- step_executions 为空：返回 `step_executions: []`，不报错
- step 失败且无 conclusion：渲染时用 `error_message` 替代
- step 待审批：渲染时显示 `approval_comment` + 「等待人工审批」

完整设计见 [`docs/loop-blackboard-cli.md`](./loop-blackboard-cli.md)。

---

#### `ntd loop results <EXECUTION_ID>`
获取 Loop 执行结果（步骤级摘要）。

```bash
ntd loop results <EXECUTION_ID>
```

---

### 7. 工艺（Process）管理命令

> **工艺（Process）** 是可复用的多环节任务流水线模板（有序环节 + 门禁 + 期望产物）。
> `ntd process` 让你在终端完成「选工艺 → 装到工作空间 → 跑 → 升级 → 版本回溯」全链路。
>
> **name 或 guid 都能标识工艺**：下面带 `<NAME_OR_GUID>` 的命令，位置参数既接受人类可读的 name（如 `4p12s-delivery`），也接受 UUID 形式的 guid。同名命中多条时 CLI 会列出候选 guid，复制其一改用 guid 重试即可。

#### `ntd process list`
列出所有工艺模板。

```bash
ntd process list [OPTIONS]
```

**选项：**

| 选项 | 说明 |
|------|------|
| `--system` | 只看系统工艺（bundled 同步来的） |
| `--user` | 只看用户自建工艺（与 `--system` 互斥） |

**示例：**
```bash
ntd process list                       # 全部
ntd process list --system              # 只看系统模板
ntd process list --user --output raw --fields "name,guid,version,is_system"
```

---

#### `ntd process show <NAME_OR_GUID>`
查看工艺模板详情（环节定义）。

```bash
ntd process show <NAME_OR_GUID>
```

**示例：**
```bash
ntd process show 4p12s-delivery                          # 用 name
ntd process show 11111111-1111-1111-1111-111111111111    # 用 guid
```

---

#### `ntd process recommend <DESCRIPTION>`
根据任务描述推荐合适的工艺（返回推荐工艺 + 匹配理由 + score）。不确定用哪个工艺时的首选入口。

```bash
ntd process recommend "<任务描述>"
```

**示例：**
```bash
ntd process recommend "给 Rust 项目搭持续交付流水线"
```

---

#### `ntd process create`
新建用户工艺。YAML 正文从 `--file` 读取，或用 `--stdin` 传完整 JSON body。

```bash
ntd process create [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--name <SLUG>` | `-n` | 工艺唯一标识，`^[a-zA-Z0-9_-]+$`（非 `--stdin` 时必填） |
| `--display-name <NAME>` | - | 人类可读名称（可空） |
| `--category <CAT>` | - | 分类（可空） |
| `--complexity <LVL>` | - | 复杂度（可空） |
| `--version <VER>` | - | 版本（可空，默认 `1.0.0`） |
| `--file <PATH>` | `-f` | 从文件读取工艺 YAML 正文（非 `--stdin` 时必填） |
| `--stdin` | - | 从 stdin 读取完整 JSON body |

**示例：**
```bash
# 从 YAML 文件创建
ntd process create --name my-delivery --display-name "我的交付" --file ./delivery.yaml

# 用 --stdin 传完整 body
ntd process create --stdin <<EOF
{ "name": "my-delivery", "definition": "process: ...", "version": "1.0.0" }
EOF
```

---

#### `ntd process delete <NAME_OR_GUID>`
删除用户工艺（系统工艺后端会拒绝）。

```bash
ntd process delete <NAME_OR_GUID>
```

**示例：**
```bash
ntd process delete my-delivery
```

---

#### `ntd process run <NAME_OR_GUID>`
安装工艺到工作空间并触发执行。`--workspace` 按项目路径指定，CLI 会自动反查 workspace_id。

```bash
ntd process run <NAME_OR_GUID> --workspace <PATH>
```

**选项：**

| 选项 | 说明 |
|------|------|
| `--workspace <PATH>` | 必填，目标工作空间路径 |

**示例：**
```bash
ntd process run 4p12s-delivery --workspace /Users/me/projects/myapp
```

---

#### `ntd process upgrade <NAME_OR_GUID>`
把指定 loop 升级到工艺模板最新版。

```bash
ntd process upgrade <NAME_OR_GUID> --loop-id <ID>
```

**选项：**

| 选项 | 说明 |
|------|------|
| `--loop-id <ID>` | 必填，要升级的 loop id（先用 `ntd process loops` 查到） |

**示例：**
```bash
ntd process loops 4p12s-delivery           # 先拿到 loop_id
ntd process upgrade 4p12s-delivery --loop-id 7
```

---

#### `ntd process loops <NAME_OR_GUID>`
列出该工艺实例化的所有 loop（含各 loop 执行次数）。

```bash
ntd process loops <NAME_OR_GUID>
```

---

#### `ntd process versions <NAME_OR_GUID>`
查看工艺版本历史。

```bash
ntd process versions <NAME_OR_GUID>
```

---

#### `ntd process diff <NAME_OR_GUID> <VERSION>`
对比两个版本的工艺正文逐行 diff。

```bash
ntd process diff <NAME_OR_GUID> <VERSION> --base <BASE_VERSION>
```

**参数 / 选项：**

| 参数 | 说明 |
|------|------|
| `<VERSION>` | 必填（位置参数），目标版本号 |
| `--base <BASE_VERSION>` | 必填，基准版本号 |

**示例：**
```bash
ntd process diff my-delivery 1.2.0 --base 1.1.0
```

---

#### `ntd process execution-status <ID>`
查看工艺实例审计状态（按 loop execution id 遍历工作空间查找）。

```bash
ntd process execution-status <LOOP_EXECUTION_ID>
```

---

### 8. 守护进程命令

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

## 9. 技能管理命令

> 用于将内嵌的 `ntd-usage` skill 安装到各执行器的技能目录，让 AI 助手在执行时能自动发现并加载 ntd 使用说明。
>
> 当前内嵌的 skill 是 `ntd-usage`，所有支持的执行器共享同一份内容，安装到各自的 skill 目录下。

#### `ntd skills install`
安装内嵌的 `ntd-usage` skill 到执行器技能目录。

```bash
ntd skills install [OPTIONS]
```

**选项：**

| 选项 | 简写 | 说明 |
|------|------|------|
| `--force` | `-f` | 强制重新安装（即使目录已存在） |
| `--executor <LIST>` | `-e` | 仅安装到指定执行器（逗号分隔，例如 `claudecode,atomcode`）；不传则安装到全部已知执行器 |

支持的执行器：`claudecode`、`hermes`、`codex`、`codebuddy`、`opencode`、`atomcode`、`kimi`、`mobilecoder`、`codewhale`、`pi`、`mimo`、`zhanlu`、`kilo`。

**示例：**
```bash
# 安装到所有执行器（首次安装或大版本更新后推荐执行一次）
ntd skills install

# 仅安装到 Claude Code
ntd skills install --executor claudecode

# 强制重新安装（升级 skill 内容后使用）
ntd skills install --force

# 强制重装到指定执行器
ntd skills install --force --executor claudecode,atomcode
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
