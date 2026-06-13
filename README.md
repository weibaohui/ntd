# ntd — Nothing Todo

[![CI](https://github.com/weibaohui/nothing-todo/actions/workflows/rust.yml/badge.svg)](https://github.com/weibaohui/nothing-todo/actions)
[![npm](https://img.shields.io/npm/v/@weibaohui/nothing-todo.svg)](https://www.npmjs.com/package/@weibaohui/nothing-todo)
[![License](https://img.shields.io/badge/License-Polyform-green.svg)](LICENSE)

**ntd** (Nothing Todo) 是一个 AI 驱动的 Todo 任务管理应用。它将传统的待办事项管理与多 AI 执行器深度集成，让你的任务不仅能被记录，还能被自动执行。

> "无事可做" — 因为 AI 已经帮你做完了。x

---

![info](docs/info.png)

## 什么是 ntd

ntd 是一个**让 AI 替你执行任务**的 Todo 系统。与传统的待办工具不同，你可以在 ntd 中创建任务，然后交给 AI 去真正完成——写代码、查资料、分析数据、生成报告。

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

- **智能任务管理** — 创建、编辑、跟踪 Todo，支持多种状态（待办、进行中、已完成、已取消、已归档）
- **多 AI 执行器支持** — 集成 Claude Code、CodeBuddy、OpenCode、AtomCode 等多种 AI CLI 工具
- **可视化仪表盘** — 实时统计任务完成情况，支持趋势图表和数据洞察，可按时间区间筛选（6h/12h/24h/3d/7d）
- **看板视图** — 瀑布流展示最近完成的任务及其 AI 执行结论，方便回顾
- **标签系统** — 灵活的标签分类，快速筛选和定位任务
- **定时调度** — 内置 Cron 调度器，支持定时触发任务执行
- **Todo 模板** — 预设任务模板，一键创建标准化任务流程
- **Session 管理** — 任务会话历史追踪，支持会话续连和状态恢复
- **项目目录管理** — 多项目隔离，每个项目独立的目录和工作空间
- **Worktree 支持** — Claude Code/Codex 执行时自动创建 Git Worktree，隔离分支操作
- **自动备份** — 定时自动备份数据，支持保留数量限制和一键下载
- **跨平台** — 支持 Windows、macOS、Linux（x86_64 & ARM64）

---

## 安装

### 方式一：让 AI 帮你安装

将下面的提示词复制给你的 AI 助手（Claude Code、ChatGPT 等）：

```
请在我的电脑上全局安装 ntd (Nothing Todo) 这个工具，执行命令：
npm install -g @weibaohui/nothing-todo
安装完成后运行 ntd 启动服务，然后打开浏览器访问 http://localhost:8088
```

### 方式二：手动安装

需要先安装 [Node.js](https://nodejs.org/) 20+，然后执行：

```bash
npm install -g @weibaohui/nothing-todo
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

### Skill 安装

`ntd skill install` 将内置的 ntd 使用技能安装到各 AI 执行器的 skill 目录（如 `~/.claude/skills/ntd-usage/`），让 AI 执行器在执行任务时能更好地理解和使用 ntd。

```bash
ntd skill install              # 安装到所有已配置的执行器
ntd skill install --force       # 强制重新安装（覆盖已有）
ntd skill install -e claudecode # 仅安装到指定执行器
```

支持的执行器：Claude Code、CodeBuddy、Opencode、MobileCoder、AtomCode、Hermes、Kimi、Pi、Codex、CodeWhale、MiMo 等（根据你配置的 AI 执行器自动适配）。

### 升级

```bash
ntd upgrade
# 或手动执行
npm install -g @weibaohui/nothing-todo@latest
```

---

## 快速开始

```bash
# 一键安装
npm install -g @weibaohui/nothing-todo

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

### 功能说明

- **会话恢复（Session Resume）**：支持通过 `--session-id` 或 `--resume` 恢复之前中断的对话，无需从头开始
- **工具调用展示**：实时显示 AI 执行过程中调用的工具（如 bash、write_file、read_file 等）
- **思考过程展示**：显示 AI 的推理思考过程（thinking block）
- **Token 用量统计**：记录 input/output tokens、缓存命中量及执行成本
- **Worktree**：执行时自动创建 Git worktree，隔离分支操作，适合仓库内多任务并行
- **后置 Todo 进度提取**：Hermes 特有功能，执行完成后从会话文件中提取内部 Todo 进度

### 各执行器特点

| 执行器 | 特点 | 安装方式 |
|--------|------|----------|
| [Claude Code](https://docs.anthropic.com/en/docs/claude-code/overview) | 官方 CLI，最完善的功能支持，NDJSON 流式输出 | `npm install -g @anthropic-ai/claude-code` |
| [Codex](https://openai.com/codex) | OpenAI 代码助手，支持复杂推理 | 官方渠道 |
| [Codebuddy](https://codebuddy.com) | 与 Claude Code 协议兼容的工具调用展示 | 官方渠道 |
| [Opencode](https://opencode.ai) | 开源代码助手，自定义事件流格式 | 官方渠道 |
| [MobileCoder](https://github.com/nicheai/mobilecoder) | 移动端 AI 代码助手，支持事件流解析 | 官方渠道 |
| [AtomCode](https://atomcode.dev) | 轻量级 AI 代码编辑器，stderr 解析 | 官方渠道 |
| [Hermes](https://github.com/bhousai/hermes) | 支持 Todo 进度提取，适合任务分解场景 | 官方渠道 |
| [Kimi](https://kimi.moonshot.cn) | 国产大模型 CLI，支持思考过程展示 | 官方渠道 |
| [Pi](https://pi.ai) | 智能 AI 助手，支持 NDJSON 事件流，含思考过程展示 | 官方渠道 |
| [CodeWhale](https://codewhale.cn) | AI 代码助手，适合中文场景 | 官方渠道 |
| [MiMo](https://mimo.ai) | 多模态 AI 代码助手，支持思考过程与 Token 统计 | 官方渠道 |

---

## 功能概览

### 智能任务管理
创建、编辑、跟踪 Todo，支持多种状态（待办、进行中、已完成、已取消、已归档）

### 可视化仪表盘
实时统计任务完成情况，支持趋势图表和数据洞察，可按时间区间筛选（6h/12h/24h/3d/7d）

### 看板视图
瀑布流展示最近完成的任务及其 AI 执行结论，方便回顾

### 定时调度
内置 Cron 调度器，支持定时触发任务执行

### 项目隔离
多项目独立管理，每个项目有独立的目录和工作空间

### 自动备份
定时自动备份数据，支持保留数量限制和一键下载

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

[Polyform](LICENSE)

---

<p align="center">
  用 Rust + React + AI 打造 | 让待办事项真正被「执行」
</p>
