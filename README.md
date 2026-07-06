# ntd — Now Task, Done

[![CI](https://github.com/weibaohui/ntd/actions/workflows/rust.yml/badge.svg)](https://github.com/weibaohui/ntd/actions)
[![npm](https://img.shields.io/npm/v/@weibaohui/ntd.svg)](https://www.npmjs.com/package/@weibaohui/ntd)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**ntd**（Now Task, Done）是一个 AI 驱动的任务引擎——你只管创建任务，剩下的——分配、执行、完成——全部交给 AI。

> 创建即执行，执行即完成。

---

![info](docs/info.png)

---

## 什么是 ntd

ntd 让 AI 替你执行**真实的任务**：写代码、查资料、分析数据、生成报告——不是聊天机器人，而是能操作文件、运行命令、调用工具的 CLI 执行器。

**适合场景：**

- 需要 AI 帮你完成代码开发、数据分析、内容创作等实际工作
- 希望集中管理 AI 执行记录，方便回顾和追溯
- 需要定时执行 AI 任务，实现自动化工作流
- 通过飞书群触发 AI 任务、接收执行结果

---

## 快速开始

### 安装

需要先安装 [Node.js](https://nodejs.org/) 20+，然后执行：

```bash
npm install -g @weibaohui/ntd
```

ntd 会自动按你的平台拉取对应预编译二进制（macOS / Linux / Windows，x86_64 & ARM64）。

### 启动

```bash
ntd                      # 启动服务（默认端口 8088）
open http://localhost:8088  # 打开浏览器
```

就这么简单。一个二进制文件，一个浏览器标签，全部搞定。

### 让 AI 帮你装

把下面的提示词复制给你的 AI 助手（Claude Code、ChatGPT 等）：

```
请在我的电脑上全局安装 ntd (Now Task, Done) 这个工具，执行命令：
npm install -g @weibaohui/ntd
安装完成后运行 ntd 启动服务，然后打开浏览器访问 http://localhost:8088
```

---

## 核心概念

ntd 有两个核心概念，理解它们就能用好一切功能：

| 概念 | 是什么 | 一句话概括 |
|------|--------|------------|
| **Todo** | 一次 AI 执行任务 | 你写 prompt，AI 执行 |
| **Loop** | 多步骤自动化工作流 | 串联多个 Todo 组成有逻辑的流水线 |

### Todo：一次任务

创建一个 Todo，ntd 把你的 `prompt` 喂给一个 AI CLI 工具（Claude Code、Kimi、Codex 等），在目标目录中执行，实时推送到前端：

```
你 ←Todo→ AI 执行器 → 结果（代码 / 文件 / 报告）
```

### Loop：自动化工作流

把多个 Todo 编排成有逻辑的流水线——**有条件分支、有评分闸门、有步骤间传参**：

```
触发（Cron / 飞书 / Todo完成）
    │
    ▼
┌─────────────────────────────────────┐
│  Step 1: 编写代码                    │
│    → 成功后 → Step 2                │
└──────────────────┬──────────────────┘
                   │
                   ▼
┌─────────────────────────────────────┐
│  Step 2: Review 代码                 │
│    评分 ≥ 80 → Step 3（部署）        │
│    评分 < 80 → 回到 Step 1（重写）   │  ◄── 循环！
└──────────────────┬──────────────────┘
                   │
                   ▼
┌─────────────────────────────────────┐
│  Step 3: 部署                        │
│    → 结束                            │
└─────────────────────────────────────┘
```

这就是 ntd 与其他待办工具的本质区别——**Loop 让你写出真正的 AI 自动化流水线**，而不是简单地列清单。

---

## 工作原理

### 单任务（Todo）流程

```
┌──────────┐   创建任务    ┌──────────────┐
│   用户    │ ──────────▶  │  ntd 服务端   │
│  (浏览器) │              │  (HTTP+WS)   │
└──────────┘              └──────┬───────┘
       ▲                         │
       │ WebSocket 实时推送       │ 选择执行器
       │ (进度 / 工具调用 / Token)│  (Claude Code / Kimi ...)
       │                         ▼
       │              ┌──────────────────┐
       │              │  AI 执行器子进程   │
       │              │  (在目标目录执行)   │
       └──────────────┴──────────────────┘
```

### 工作流（Loop）流程

```
  触发器（Cron / 飞书 / Todo完成 / 标签 / Webhook）
                      │
                      ▼
           ┌──────────────────────┐
           │  LoopRunner 主循环     │
           │  - 加载步骤 + 限制     │
           │  - 注入黑板变量        │
           └──────┬───────────────┘
                  │
                  ▼
           ┌──────────────────────┐
           │  Step N: 执行 Todo     │
           │  (prompt 注入黑板)     │
           └──────┬───────────────┘
                  │
         ┌────────┴────────┐
         │                 │
      成功               评分不通过
         │                 │
         ▼                 ▼
   按 on_success       按 on_rating_fail
   策略跳转           策略跳转
         │                 │
         └────────┬────────┘
                  │
                  ▼
           ┌──────────────────────┐
           │  更新黑板（结论摘要）   │  ◄── 传给下一步
           │  检查全局限制          │
           │  确定下一步 → 回到顶部  │
           └──────────────────────┘
                  │
              直到结束 / 超限
                  │
                  ▼
           ┌──────────────────────┐
           │  推送结果（飞书等）     │
           └──────────────────────┘
```

---

## 功能概览

### Todo 管理

- **创建 / 编辑 / 删除** — 支持从 prompt 文件读取、模板快捷创建
- **执行与续连** — 任务执行后可续连之前的会话继续对话
- **5 种状态** — 待办 / 进行中 / 已完成 / 已失败 / 已取消
- **定时调度** — 支持 Cron 表达式 + IANA 时区（如 `Asia/Shanghai`），支持夏令时
- **Hook 系统** — Todo 完成后可级联触发子 Todo，带防环检测
- **标签分类** — 灵活打标签，快速筛选定位

### Loop Studio：可视化自动化

- **流程图编辑器** — DAG 自动布局，可视化步骤间的跳转关系
- **6 种触发方式** — 手动 / Cron 定时 / 飞书消息 / 飞书命令 / Todo 完成 / 标签新增
- **条件控制流** — 每步支持 `on_success`（next / goto / end）和 `on_rating_fail`（break / skip / goto / end）策略
- **黑板传参** — 上一步的结论自动注入下一步的 Prompt，实现有状态的流水线
- **AI 评分闸门** — 内置 AI 评审，不达标自动回退重做
- **执行回放** — 每次 Loop 执行的步骤轨迹、结论黑板、Token 用量一览无余
- **飞书结果推送** — Loop 完成后自动将结论发到飞书群

### 支持的 AI 执行器

ntd 支持 13 种 AI CLI 工具，选择你已有的或最喜欢的即可：

| 执行器 | 内部名 | 二进制 | Session 续连 | Token 统计 | Worktree |
|--------|--------|--------|:------------:|:----------:|:--------:|
| **Claude Code** | claudecode | claude | ✅ | ✅ | ✅ |
| **CodeBuddy** | codebuddy | codebuddy | ❌ | ✅ | ❌ |
| **OpenCode** | opencode | opencode | ✅ | ✅ | ❌ |
| **AtomCode** | atomcode | atomcode | ❌ | ✅ | ❌ |
| **Hermes** | hermes | hermes | ✅ | ❌ | ✅ |
| **Kimi** | kimi | kimi | ✅ | ❌ | ❌ |
| **MiMo** | mimo | mimo | ✅ | ✅ | ❌ |
| **MobileCoder** | mobilecoder | mobile | ✅ | ✅ | ❌ |
| **Codex** | codex | codex | ❌ | ✅ | ❌ |
| **CodeWhale** | codewhale | codewhale | ✅ | ❌ | ❌ |
| **Pi** | pi | pi | ✅ | ✅ | ❌ |
| **Zhanlu** | zhanlu | zl | ✅ | ✅ | ❌ |
| **Kilo** | kilo | kilo | ✅ | ✅ | ❌ |

> **默认执行器**：`claudecode`。可在 Settings → Executors 配置每个执行器的二进制路径与启用状态。

### 飞书集成

- **多 Bot 绑定** — 一个 ntd 可同时对接多个飞书应用
- **群聊白名单** — 只有指定群的消息才会触发任务
- **斜杠命令** — 在飞书群里发 `/run 帮我修个 bug` 直接触发任务
- **消息去抖** — 连续发多条消息只触发一次执行
- **结果推送** — 任务完成后自动把结果发到群里
- **历史消息拉取** — 支持从群历史中发现未处理的消息

### 其他能力

- **仪表盘** — 任务完成情况统计、趋势图、Token 用量分析
- **看板视图** — Todo 四列看板 + Running Board 执行状态看板
- **纪念板** — 回顾已完成任务的结论
- **Blackboard Wiki** — 每次 AI 执行的结论沉淀为知识库，支持 AI 对话式查询
- **工作空间** — 多项目隔离，每个项目独立目录和工作空间
- **Git Worktree** — Claude Code / Codex 执行时自动创建工作树，隔离分支操作
- **Todo 模板** — 预设任务模板，一键创建标准化流程
- **云端同步** — push / pull 本地变更到云端，支持增量同步
- **自动备份** — 定时自动备份数据库 / Todo / Skill
- **使用统计** — Token 用量、成本、模型维度统计
- **守护进程** — `ntd daemon` 跨平台服务管理（macOS launchd / Linux systemd）

---

## 使用方式

### 命令行

ntd CLI 同时支持本地子命令和远程 API 子命令（通过 HTTP 调用本机 API）：

```bash
ntd                  # 无子命令时直接启动服务（默认端口 8088）
ntd version          # 查看版本
ntd --help           # 查看完整帮助
```

#### Todo 操作

```bash
ntd todo create "完成报告" --prompt "写一份季度报告"
ntd todo create "代码审查" --file ./prompt.txt --executor claudecode
ntd todo list                                # 列出所有 Todo
ntd todo list --status pending --search bug  # 按状态/关键词筛选
ntd todo get 1                              # 查看详情
ntd todo update 1 --title "新标题"
ntd todo delete 1
ntd todo execute 1                          # 立即触发执行
ntd todo execution list 1                   # 执行历史
ntd todo execution resume 42                # 续连执行
```

#### Loop 操作

```bash
ntd loop list                       # 查看所有 Loop
ntd loop get <id>                   # 查看详情
ntd loop update <id> --name "新名称"
ntd loop stop <id>
ntd loop execute <id> --param message=hello   # 立即触发
ntd loop stats <id>                 # 执行统计
ntd loop execution blackboard <eid> --human   # 人类可读的黑板视图
```

#### 标签 / 统计 / 工作空间

```bash
ntd tag list                         # 列出标签
ntd tag create --name bug
ntd stats                            # 全局统计
ntd workspace list                   # 列出项目目录
ntd workspace create --path /tmp/proj-a --name proj-a
```

#### 守护进程管理

```bash
ntd daemon install      # 安装为系统服务（macOS launchd / Linux systemd）
ntd daemon start
ntd daemon stop
ntd daemon restart
ntd daemon status
```

#### Skill 安装

`ntd skills install` 把内置的 ntd 使用技能安装到各 AI 执行器的 skill 目录（如 `~/.claude/skills/ntd-usage/`），让 AI 执行器在执行任务时能更好地理解和使用 ntd：

```bash
ntd skills install              # 安装到所有已知执行器
ntd skills install -e claudecode,atomcode   # 仅指定执行器
```

#### 升级

```bash
ntd upgrade
# 或手动：
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


## 截图预览

![detail](docs/detail.png)
![dashboard](docs/dashboard.png)
![kanban](docs/kanban.png)

---


参与开发请参阅 [DEVELOPMENT.md](DEVELOPMENT.md)。架构总览见 [backend/ARCHITECTURE.md](backend/ARCHITECTURE.md)，关键流程时序图见 [backend/SEQUENCE.md](backend/SEQUENCE.md)，配置项完整说明见 [backend/CONFIG.md](backend/CONFIG.md)。

---

## 许可证

📄 本项目采用 [MIT 许可证](LICENSE) 开源。

---

<p align="center">
  用 Rust + React + AI 打造 | 让待办事项真正被「执行」
</p>
