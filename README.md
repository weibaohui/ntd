# ntd — Now Task, Done

[![CI](https://github.com/weibaohui/nothing-todo/actions/workflows/rust.yml/badge.svg)](https://github.com/weibaohui/nothing-todo/actions)
[![npm](https://img.shields.io/npm/v/@weibaohui/ntd.svg)](https://www.npmjs.com/package/@weibaohui/ntd)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**ntd** (Now Task, Done) 是一个 AI 驱动的任务引擎。你只管创建任务，剩下的——分配、执行、完成——全部交给 AI，秒速清空。

> 创建即执行，执行即完成。

---

![info](docs/info.png)

## 什么是 ntd

ntd 是一个**让 AI 替你执行任务**的任务引擎。与传统的待办工具不同，你可以在 ntd 中创建任务，然后交给 AI 去真正完成——写代码、查资料、分析数据、生成报告。

**适合场景：**
- 需要 AI 帮你完成代码开发、数据分析、内容创作等实际工作
- 希望集中管理 AI 执行记录，方便回顾和追溯
- 需要定时执行 AI 任务，实现自动化工作流

**工作原理：**

```
┌─────────────┐     创建任务      ┌─────────────┐
│   使用者     │ ──────────────▶  │   ntd       │
└─────────────┘                  │   服务端    │
      ▲                          └──────┬──────┘
      │                                 │
      │  查看结果                        │ 转发任务
      │                                 ▼
┌─────────────┐                  ┌─────────────┐
│   浏览器     │ ◀──────────────  │  AI 执行器  │
│   UI        │     返回结果      │ (Claude/    │
└─────────────┘                  │  Codex...)  │
                                 └─────────────┘
```

---

## 特性

- **智能任务管理** — 创建、编辑、跟踪 Todo，支持多种状态（待办、进行中、已完成、已失败、已取消）
- **多 AI 执行器支持** — 集成 Claude Code、CodeBuddy、OpenCode、AtomCode、Zhanlu、Kilo 等 13 种 AI CLI 工具
- **Loop Studio** — 多步骤自动化工作流，支持 Cron/飞书/Webhook/标签等多种触发方式，步骤间可注入 AI 评审
- **自动评审** — 执行完成后自动触发 AI 评审，支持自定义评审模板
- **可视化仪表盘** — 实时统计任务完成情况，支持趋势图表和数据洞察，可按时间区间筛选
- **看板视图** — Todo 四列看板 + 执行状态看板（Running Board），瀑布流回顾已完成任务
- **标签系统** — 灵活的标签分类，快速筛选和定位任务
- **定时调度** — 内置 Cron 调度器，支持任务级独立调度与时区配置
- **Todo 模板** — 预设任务模板，一键创建标准化任务流程
- **Session 管理** — 任务会话历史追踪，支持会话续连和状态恢复
- **项目目录管理** — 多项目隔离，每个项目独立的目录和工作空间
- **Worktree 支持** — Claude Code/Codex 执行时自动创建 Git Worktree，隔离分支操作
- **Hook 系统** — 任务生命周期钩子（执行前/执行后/状态变更时），支持钩子链级联
- **飞书集成** — 多 Bot 绑定、群聊白名单、斜杠命令、消息去抖、历史消息拉取
- **Webhook 触发** — Todo/Loop 内建 Webhook 能力，支持外部系统远程触发执行
- **云端同步** — push/pull 本地变更到云端，支持增量同步
- **使用统计** — Token 用量、成本、模型维度统计，支持 ccusage 集成
- **自动备份** — 定时自动备份数据库/Todo/Skill，支持保留数量限制和一键下载
- **跨平台** — 支持 Windows、macOS、Linux（x86_64 & ARM64）

---

## 安装

### 方式一：让 AI 帮你安装

将下面的提示词复制给你的 AI 助手（Claude Code、ChatGPT 等）：

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

---

## 使用

```bash
# 启动服务
ntd

# 打开浏览器访问
# http://localhost:8088
```

### 命令行

```bash
ntd              # 启动服务（默认端口 8088）
ntd version      # 查看版本信息
ntd upgrade      # 升级到最新版本
ntd --help       # 查看帮助
```

### Loop 命令

Loop 是 ntd 的自动化循环任务功能，支持 Cron 调度和 AI 驱动的工作流。
命令结构与 Todo 保持一致，降低认知成本。

```bash
# 查看所有 loop
ntd loop list

# 查看 loop 详情
ntd loop get <id>

# 更新 loop
ntd loop update <id> --name "新名称" --description "描述"

# 删除 loop
ntd loop delete <id>

# 停止 loop
ntd loop stop <id>

# 查看 loop 执行统计和最近执行
ntd loop stats <id>                                   # 查看 + 最近5次执行
ntd loop stats <id> --recent 10                       # 查看最近10次执行

# 执行 loop（立即触发）
ntd loop execute <id> --param message=hello

# 执行记录
ntd loop execution list <loop_id>                     # 列出执行历史
ntd loop execution get <execution_id>                 # 查看执行详情

# 查看执行结果（步骤级摘要）
ntd loop results <execution_id>
```

### Skill 安装

`ntd skills install` 将内置的 ntd 使用技能安装到各 AI 执行器的 skill 目录（如 `~/.claude/skills/ntd-usage/`），让 AI 执行器在执行任务时能更好地理解和使用 ntd。

```bash
ntd skills install              # 安装到所有已知执行器
ntd skills install --all        # 安装到所有执行器（包括 agents 只读来源）
ntd skills install --force      # 强制重新安装（覆盖已有）
ntd skills install -e claudecode # 仅安装到指定执行器
```

支持的执行器：Claude Code、CodeBuddy、Opencode、MobileCoder、AtomCode、Hermes、Kimi、Pi、Codex、CodeWhale、MiMo、Zhanlu、Kilo 等（根据你配置的 AI 执行器自动适配）。

### 升级

```bash
ntd upgrade
# 或手动执行
npm install -g @weibaohui/ntd@latest
```

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

- **Node.js 20+** （用于安装和运行 npm 包）
- **AI 执行器** （至少安装一个，详见下方）

---

## 支持的 AI 执行器

ntd 支持多种 AI CLI 工具，选择你已有的或最喜欢的即可：

### 功能对比表

| 执行器 | 会话恢复 | 工具调用展示 | 思考过程展示 | Token 用量统计 | 模型名称 | Worktree | 安装命令 |
|--------|:--------:|:------------:|:------------:|:-------------:|:--------:|:--------:|----------|
| **Claude Code** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | `npm install -g @anthropic-ai/claude-code` |
| **CodeBuddy** | ❌ | ✅ | ✅ | ✅ | ✅ | ❌ | 官方渠道 |
| **Opencode** | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ | 官方渠道 |
| **MobileCoder** | ✅ | ✅ | ❌ | ✅ | ❌ | ❌ | 官方渠道 |
| **AtomCode** | ❌ | ✅ | ❌ | ✅ | ❌ | ❌ | 官方渠道 |
| **Hermes** | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | 官方渠道 |
| **Kimi** | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | 官方渠道 |
| **Pi** | ✅ | ✅ | ✅ | ❌ | ✅ | ❌ | 官方渠道 |
| **Codex** | ❌ | ✅ | ✅ | ✅ | ✅ | ❌ | 官方渠道 |
| **CodeWhale** | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | 官方渠道 |
| **MiMo** | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | 官方渠道 |
| **Zhanlu** | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ | 官方渠道 |
| **Kilo** | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ | 官方渠道 |

### 功能说明

- **会话恢复（Session Resume）**：支持通过 `--session-id` 或 `--resume` 恢复之前中断的对话，无需从头开始
- **工具调用展示**：实时显示 AI 执行过程中调用的工具（如 bash、write_file、read_file 等）
- **思考过程展示**：显示 AI 的推理思考过程（thinking block）
- **Token 用量统计**：记录 input/output tokens、缓存命中量及执行成本
- **Worktree**：执行时自动创建 Git worktree，隔离分支操作，适合仓库内多任务并行
- **后置 Todo 进度提取**：Hermes 特有功能，执行完成后从会话文件中提取内部 Todo 进度

---

## 功能概览

### 智能任务管理
创建、编辑、跟踪 Todo，支持多种状态（待办、进行中、已完成、已失败、已取消）

### Loop Studio
多步骤自动化工作流，支持 Cron/飞书/Webhook/标签等多种触发方式，步骤间可注入 AI 评审

### 可视化仪表盘
实时统计任务完成情况，支持趋势图表和数据洞察，可按时间区间筛选

### 看板视图
Todo 四列看板 + Running Board 执行状态看板，瀑布流回顾已完成任务

### 自动评审
执行完成后自动触发 AI 评审，支持自定义评审模板

### 定时调度
内置 Cron 调度器，支持任务级独立调度与时区配置

### 飞书集成
多 Bot 绑定、群聊白名单、斜杠命令、消息去抖、历史消息拉取

### 项目隔离
多项目独立管理，每个项目有独立的目录和工作空间

### Hook 系统
任务生命周期钩子（执行前/执行后/状态变更时），支持钩子链级联

### 云端同步
push/pull 本地变更到云端，支持增量同步

### 自动备份
定时自动备份数据库/Todo/Skill，支持保留数量限制和一键下载

### 跨平台
支持 Windows、macOS、Linux（x86_64 & ARM64）

---

## 截图预览

![detail](docs/detail.png)
![dashboard](docs/dashboard.png)
![kanban](docs/kanban.png)

---

## 开发

参与开发请参阅 [DEVELOPMENT.md](DEVELOPMENT.md)。

## 许可证

📄 本项目采用 [MIT 许可证](LICENSE) 开源。

---

<p align="center">
  用 Rust + React + AI 打造 | 让待办事项真正被「执行」
</p>
