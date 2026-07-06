# ntd — Now Task, Done

[![CI](https://github.com/weibaohui/ntd/actions/workflows/rust.yml/badge.svg)](https://github.com/weibaohui/ntd/actions)
[![npm](https://img.shields.io/npm/v/@weibaohui/ntd.svg)](https://www.npmjs.com/package/@weibaohui/ntd)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**ntd**（Now Task, Done）是一个 AI 驱动的任务引擎。你只管创建任务，剩下的——分配、执行、完成——全部交给 AI，秒速清空。

> 创建即执行，执行即完成。

---

## 什么是 ntd

ntd 是一个**让 AI 替你执行任务**的任务引擎。与传统的待办工具不同，你在 ntd 中创建的任务会由真实的 AI CLI 工具（Claude Code、Codex、OpenCode 等）完成——写代码、查资料、分析数据、生成报告，并实时回传执行过程与产物。

**适合场景：**

- 需要 AI 帮你完成代码开发、数据分析、内容创作等实际工作
- 希望集中管理 AI 执行记录，方便回顾和追溯
- 需要定时执行 AI 任务，实现自动化工作流
- 团队协作场景下通过飞书群触发 AI 任务、接收执行结果

**工作原理：**

```
┌─────────────┐     创建任务      ┌─────────────┐
│   使用者     │ ──────────────▶  │   ntd       │
└─────────────┘                  │   服务端    │
      ▲                          └──────┬──────┘
      │                                 │ 转发任务
      │  查看结果                        │
      │                                 ▼
┌─────────────┐                  ┌─────────────┐
│   浏览器     │ ◀──────────────  │  AI 执行器  │
│   UI        │     实时推送      │ (Claude /   │
└─────────────┘                  │  Codex...)  │
                                 └─────────────┘
```

ntd 把每个「用户视角的工作单元」抽象为 Todo，执行时把 Todo 的 `prompt` 喂给一个 AI CLI 子进程，解析 stdout/stderr，通过 WebSocket 把进度、产物、Token 统计实时推到前端。一套进程内同时提供 HTTP API、CLI、Cron 调度、飞书长连接、多执行器子进程池。

---

## 特性

- **智能任务管理** — 创建、编辑、跟踪 Todo，支持多种状态（待办、进行中、已完成、已失败、已取消）
- **13 种 AI 执行器** — 集成 Claude Code、CodeBuddy、OpenCode、AtomCode、Hermes、Kimi、MiMo、MobileCoder、Codex、CodeWhale、Pi、Zhanlu、Kilo
- **Loop Studio** — 多步骤自动化工作流，支持 Cron / 飞书 / Webhook / 标签 / Todo 状态变更等多种触发方式，步骤间可注入 AI 评审与黑板传递
- **自动评审** — 执行完成后自动触发 AI 评审，支持自定义评审模板与 Loop 评分闸门
- **可视化仪表盘** — 实时统计任务完成情况，支持趋势图表和数据洞察，可按时间区间筛选
- **看板视图** — Todo 四列看板 + Running Board 执行状态看板，纪念板回顾已完成任务结论
- **标签系统** — 灵活的标签分类，快速筛选和定位任务
- **定时调度** — 内置 Cron 调度器，支持任务级独立调度与时区配置（IANA 时区名）
- **Todo 模板** — 预设任务模板，一键创建标准化任务流程，支持远程订阅 YAML 模板
- **Session 管理** — 任务会话历史追踪，支持会话续连和状态恢复
- **项目目录管理** — 多项目隔离，每个项目独立的目录和工作空间
- **Worktree 支持** — Claude Code / Codex 执行时自动创建 Git Worktree，隔离分支操作
- **Hook 系统** — 任务生命周期钩子（执行前/执行后/状态变更时），父 Todo 完成可级联触发子 Todo，带 chain 检测防环
- **飞书集成** — 多 Bot 绑定、群聊白名单、斜杠命令、消息去抖、历史消息拉取
- **Webhook 触发** — Todo / Loop 内建 Webhook 能力，支持外部系统远程触发执行
- **云端同步** — push / pull 本地变更到云端，支持增量同步与冲突策略
- **使用统计** — Token 用量、成本、模型维度统计，支持 ccusage 集成
- **自动备份** — 定时自动备份数据库 / Todo / Skill，支持保留数量限制和一键下载
- **守护进程** — `ntd daemon` 跨平台服务管理（macOS launchd / Linux systemd / Windows）
- **跨平台** — 支持 Windows、macOS、Linux（x86_64 & ARM64）

---

## 安装

### 方式一：让 AI 帮你安装

把下面的提示词复制给你的 AI 助手（Claude Code、ChatGPT 等）：

```
请在我的电脑上全局安装 ntd (Now Task, Done) 这个工具，执行命令：
npm install -g @weibaohui/ntd
安装完成后运行 ntd 启动服务，然后打开浏览器访问 http://localhost:8088
```

### 方式二：手动安装

需要先安装 [Node.js](https://nodejs.org/) 20+，然后执行：

```bash
npm install -g @weibaohui/ntd
```

ntd 会自动按你的平台拉取对应预编译二进制：`@weibaohui/ntd-linux-x64` / `@weibaohui/ntd-linux-arm64` / `@weibaohui/ntd-darwin-arm64` / `@weibaohui/ntd-windows-x64`。

---

## 使用

```bash
# 启动服务（默认端口 8088）
ntd

# 打开浏览器访问
# http://localhost:8088
```

### 命令行

ntd CLI 同时支持本地子命令（`version` / `upgrade` / `server` / `daemon` / `skills`）和远程 API 子命令（`todo` / `loop` / `tag` / `stats` / `blackboard` / `workspace`），远程子命令通过 HTTP 调用本机 API。

```bash
ntd                  # 无子命令时直接启动服务（默认端口 8088）
ntd version          # 查看版本与 git SHA
ntd upgrade          # 通过 npm 升级到最新版本并重新部署 daemon
ntd server start     # 显式启动 API 服务（可用 --port 指定端口）
ntd --help           # 查看完整帮助
```

#### Todo 管理

```bash
ntd todo create "完成报告" --prompt "写一份季度报告"
ntd todo create "代码审查" --file ./prompt.txt --executor claudecode
ntd todo list                                # 列出所有 todo
ntd todo list --status pending --search bug  # 按状态/关键词筛选
ntd todo get 1                              # 查看 todo 详情
ntd todo update 1 --title "新标题"
ntd todo delete 1
ntd todo execute 1                          # 立即触发执行
ntd todo execution list 1                   # 列出该 todo 的执行历史
ntd todo execution resume 42                # 续连执行（支持 --message）
```

#### Loop 管理

Loop 是 ntd 的自动化循环任务功能，命令结构与 Todo 保持一致，降低认知成本。

```bash
ntd loop list                       # 查看所有 loop
ntd loop get <id>                   # 查看 loop 详情
ntd loop update <id> --name "新名称"
ntd loop delete <id>
ntd loop stop <id>

ntd loop stats <id>                 # 查看执行统计 + 最近 5 次执行
ntd loop stats <id> --recent 10     # 查看最近 10 次执行
ntd loop execute <id> --param message=hello   # 立即触发执行

ntd loop execution list <loop_id>           # 列出执行历史
ntd loop execution get <execution_id>       # 查看执行详情
ntd loop execution blackboard <eid>         # 查看该次执行的黑板（JSON）
ntd loop execution blackboard <eid> --human # 人类可读黑板视图
ntd loop results <execution_id>             # 查看步骤级结果摘要
```

#### 标签 / 统计 / Blackboard / Workspace

```bash
ntd tag list                 # 列出所有标签
ntd tag create --name bug
ntd tag delete <id>

ntd stats                    # 全局统计

ntd blackboard wiki list --workspace-id 1
ntd blackboard wiki get <slug> --workspace-id 1

ntd workspace list                       # 列出所有注册的项目目录
ntd workspace create --path /tmp/proj-a --name proj-a
ntd workspace delete <id>
```

#### Daemon 服务管理

```bash
ntd daemon install      # 安装为系统服务（macOS launchd / Linux systemd）
ntd daemon uninstall
ntd daemon start
ntd daemon stop
ntd daemon restart
ntd daemon status
```

#### Skill 安装

`ntd skills install` 把内置的 ntd 使用技能安装到各 AI 执行器的 skill 目录（如 `~/.claude/skills/ntd-usage/`），让 AI 执行器在执行任务时能更好地理解和使用 ntd。

```bash
ntd skills install              # 安装到所有已知执行器
ntd skills install --all        # 包括 agents 只读来源
ntd skills install --force      # 强制重新安装（覆盖已有）
ntd skills install -e claudecode,atomcode   # 仅安装到指定执行器
```

#### 升级

```bash
ntd upgrade
# 或手动执行
npm install -g @weibaohui/ntd@latest
```

---

## 支持的 AI 执行器

ntd 支持 13 种 AI CLI 工具，选择你已有的或最喜欢的即可。`RESUMABLE_EXECUTORS` 标记的执行器支持会话续连（`--session-id` / `--resume`）。

| 执行器 | 内部名 | 二进制名 | Session 续连 | Token 统计 | Worktree | 安装命令 |
|--------|--------|----------|:------------:|:----------:|:--------:|----------|
| **Claude Code** | `claudecode` | `claude` | ✅ | ✅ | ✅ | `npm install -g @anthropic-ai/claude-code` |
| **CodeBuddy** | `codebuddy` | `codebuddy` | ❌ | ✅ | ❌ | 官方渠道 |
| **OpenCode** | `opencode` | `opencode` | ✅ | ✅ | ❌ | 官方渠道 |
| **AtomCode** | `atomcode` | `atomcode` | ❌ | ✅ | ❌ | 官方渠道 |
| **Hermes** | `hermes` | `hermes` | ✅ | ❌ | ✅ | 官方渠道 |
| **Kimi** | `kimi` | `kimi` | ✅ | ❌ | ❌ | 官方渠道 |
| **MiMo** | `mimo` | `mimo` | ✅ | ✅ | ❌ | 官方渠道 |
| **MobileCoder** | `mobilecoder` | `mobile` | ✅ | ✅ | ❌ | 官方渠道 |
| **Codex** | `codex` | `codex` | ❌ | ✅ | ❌ | 官方渠道 |
| **CodeWhale** | `codewhale` | `codewhale` | ✅ | ❌ | ❌ | 官方渠道 |
| **Pi** | `pi` | `pi` | ✅ | ✅ | ❌ | 官方渠道 |
| **Zhanlu** | `zhanlu` | `zl` | ✅ | ✅ | ❌ | 官方渠道 |
| **Kilo** | `kilo` | `kilo` | ✅ | ✅ | ❌ | 官方渠道 |

> **默认执行器**：`claudecode`。可在 Settings → Executors 页面配置每个执行器的二进制路径与启用状态，ntd 启动时会自动探测 `$PATH` 上的可用执行器。

### 功能说明

- **Session 续连**：支持通过 `--session-id` 或 `--resume` 恢复之前中断的对话，无需从头开始
- **工具调用展示**：实时显示 AI 执行过程中调用的工具（如 bash、write_file、read_file 等）
- **思考过程展示**：显示 AI 的推理思考过程（thinking block）
- **Token 用量统计**：记录 input/output tokens、缓存命中量及执行成本
- **Worktree**：执行时自动创建 Git worktree，隔离分支操作，适合仓库内多任务并行
- **后置 Todo 进度提取**：Hermes 特有功能，执行完成后从会话文件中提取内部 Todo 进度

---

## 配置

ntd 采用统一的 YAML 配置文件，首次启动自动生成：

| 模式 | 配置文件 | 数据库 | 默认端口 |
|------|----------|--------|---------|
| 生产 | `~/.ntd/config.yaml` | `~/.ntd/data.db` | 8088 |
| 开发（`NTD_MODE=dev`） | `~/.ntd/config.dev.yaml` | `~/.ntd/data.dev.db` | 18088 |

完整字段说明见 [backend/CONFIG.md](backend/CONFIG.md)。常用项：

```yaml
port: 8088
host: 0.0.0.0
db_path: ~/.ntd/data.db
log_level: INFO

max_concurrent_todos: 3              # 单 todo 并发上限
execution_timeout_secs: 3600         # 单次执行超时，0=不限
scheduler_default_timezone: null     # 默认时区（IANA 名，如 Asia/Shanghai）

auto_backup_enabled: false           # 数据库自动备份
auto_backup_cron: "0 0 3 * * *"
auto_backup_max_files: 30

cloud_sync:
  server_url: ""                     # 空 = 不启用云端同步
  sync_token: null
  default_conflict_mode: "overwrite" # 或 "skip"
```

HTTP `PUT /api/config` 可在运行时修改白名单字段，修改后立即生效（cron 任务除外，需重启 scheduler）。

---

## 快速开始

```bash
# 一键安装
npm install -g @weibaohui/ntd

# 启动服务
ntd

# 浏览器打开
open http://localhost:8088
```

### 前置要求

- **Node.js 20+**（用于安装和运行 npm 包）
- **AI 执行器**（至少安装一个，详见上方表格）

### 故障排查

| 症状 | 检查点 |
|------|-------|
| 启动报 `Failed to bind to port` | `~/.ntd/config.yaml` 里 `port` 是否被占用；`ntd daemon stop` 清掉残留进程 |
| DB 报 `database is locked` | SQLite WAL 已强制开启；通常 5 秒 `busy_timeout` 后自愈 |
| Todo 执行后立刻 Finished 无日志 | 检查 executor 二进制是否在 `$PATH`；Settings → Executors 里 `enabled=1` 且 `path` 正确 |
| WebSocket 连不上 | 浏览器代理拦截了 upgrade；生产模式下 `Host` 是否能从外部访问 |
| 飞书无响应 | `agent_bots` 表里 `app_id/app_secret` 是否填写；`feishu_project_bindings` 是否启用 |

更多运维指南见 [docs/user-guide](docs/user-guide/README.md)。

---

## 截图预览

![info](docs/info.png)
![detail](docs/detail.png)
![dashboard](docs/dashboard.png)
![kanban](docs/kanban.png)

---

## 开发

ntd 后端用 Rust + Axum，前端 React 19 + Vite + Ant Design，数据库 SQLite + SeaORM，前端构建产物通过 `rust-embed` 嵌入二进制，分发一个可执行文件即可运行整套应用。

### 技术栈

- **后端**：Rust 1.85+ / Axum / Tokio / SeaORM / SQLite（WAL + 外键）
- **前端**：React 19 / Vite 7 / Ant Design 6 / TypeScript 5
- **调度**：tokio-cron-scheduler
- **进程管理**：command-group（setpgid + 进程组级联终止）
- **守护进程**：macOS launchd / Linux systemd

### 前置要求

- [Rust](https://www.rust-lang.org/tools/install) 1.85+
- [Node.js](https://nodejs.org/) 20+
- [Make](https://www.gnu.org/software/make/)

### 常用命令

```bash
make setup     # 一次性安装 Rust / Node / cross 等依赖
make dev       # 开发模式（端口 18088，前后端分离，热重载）
make stop      # 停止开发实例
make build     # 仅构建生产版本（编译前端 → 嵌入 → cargo build --release）
make install   # 构建并安装到 ~/.local/bin/ntd
make cross-build   # 交叉编译 win / mac / linux x86+arm
make clean     # 清理构建产物
```

### 目录结构

```
backend/           Rust 后端
  src/
    adapters/        AI 执行器适配器（13 种）
    handlers/        HTTP 路由处理
    db/              数据库操作（SeaORM entity + 查询）
    services/        业务服务（飞书监听、自动评审、Loop 引擎等）
    executor_service/  执行器服务（进程管理、日志捕获、超时）
    execution_events/  事件管道（broadcast → WebSocket）
    cli/             CLI 子命令
    daemon/          守护进程管理（macOS/Linux/Windows）
    feishu/          飞书 SDK 集成
    config.rs        统一配置管理
    scheduler.rs     Cron 调度器
    task_manager.rs  任务生命周期管理
  tests/             集成测试
frontend/          React 前端
  src/
    components/      UI 组件（120+ 文件）
    hooks/           自定义 Hooks
  tests/             Playwright 测试
packages/         npm 跨平台分发包
docs/             文档和截图
ntd-skills/       内置 Skill 定义
```

参与开发请参阅 [DEVELOPMENT.md](DEVELOPMENT.md)。架构总览见 [backend/ARCHITECTURE.md](backend/ARCHITECTURE.md)，关键流程时序图见 [backend/SEQUENCE.md](backend/SEQUENCE.md)，配置项完整说明见 [backend/CONFIG.md](backend/CONFIG.md)。

---

## 许可证

📄 本项目采用 [MIT 许可证](LICENSE) 开源。

---

<p align="center">
  用 Rust + React + AI 打造 | 让待办事项真正被「执行」
</p>
