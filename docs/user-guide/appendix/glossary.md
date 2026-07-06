# 术语表

| 术语 | 含义 |
|------|------|
| **Todo** | ntd 的核心实体。一个待办任务，可以被某个执行器跑起来 |
| **执行器（Executor）** | 实际跑 Todo 的外部 CLI 工具（Claude Code / Codex / Hermes / Kimi 等） |
| **Skill** | 预制的 prompt 模板，可以附加到 Todo 上。存在执行器各自的 skill 目录 |
| **Workspace** | Todo 跑起来时的工作目录，必须在「项目目录」白名单内 |
| **Worktree** | git worktree 模式，每个 Todo 一个独立分支 |
| **Hook** | Todo 跑前/跑后触发的钩子（脚本 / 命令） |
| **Webhook** | 外部 HTTP 触发器，路径带 todo_id |
| **SLASH 命令** | 飞书群里 `/xxx` 形式触发某个 Todo |
| **Cron 定时** | 周期性自动跑 Todo |
| **冲突模式** | 云端同步时，本地已有同名 Todo 怎么办（overwrite / skip / rename） |
| **Dry Run** | 走完流程但不真写库，常用于先看一眼 |
| **SSRF** | Server-Side Request Forgery，ntd 自定义模板订阅做的防御 |
| **WAL** | SQLite Write-Ahead Logging，并发读优化模式 |
| **VACUUM** | SQLite 命令，归还未使用空间 |
| **Soft Delete** | 软删，标记 `deleted_at` 而非真删 |
| **JWT** | JSON Web Token，ntd-cloud 用作登录态 |
| **Device Flow** | 飞书 OAuth 2.0 Device Authorization Grant |
| **AGI** | (本项目无关) Artificial General Intelligence — 别搞混 |
| **ntd** | Now Task, Done，本项目名 |
| **NPM** | Node.js 包管理，ntd 通过 npm 全局安装 |
| **daemon** | ntd 后台服务进程 |
| **CLA** | (本项目无关) 不要和 Claude Code 缩写混淆 |

## 文件 / 目录速查

| 名称 | 路径 |
|------|------|
| 生产配置 | `~/.ntd/config.yaml` |
| 开发配置 | `~/.ntd/config.dev.yaml` |
| 生产数据库 | `~/.ntd/data.db` |
| 开发数据库 | `~/.ntd/data.dev.db` |
| 备份目录 | `~/.ntd/backups/` |
| PID | 无（macOS launchd 由 launchctl 管理；Linux systemd 由 systemd 管理；仅 dev 模式有 `~/.ntd/dev.pid`） |
| Claude Code skill 目录 | `~/.claude/skills/` |
| Codex skill 目录 | `~/.codex/skills/` |
| 各执行器 skill 目录约定 | `~/.{executor}/skills/` |

## 端口速查

| 端口 | 用途 |
|------|------|
| 8088 | ntd 生产 |
| 18088 | ntd 开发 |
| 8089 | ntd-cloud（云端同步服务；ntd-cloud 默认端口，可在 ntd-cloud 自身配置覆盖） |

## API 速查前缀

| 前缀 | 用途 |
|------|------|
| `/api/todos` | Todo CRUD |
| `/api/execute` | 跑 Todo |
| `/api/execution-records` | 执行记录 |
| `/api/agent-bots` | 飞书 Bot |
| `/api/cloud/*` | 云端同步 |
| `/api/skills/*` | Skill 管理 |
| `/api/backup/*` | 备份 |
| `/api/sessions/*` | Session |
| `/api/custom-templates/*` | 远程模板 |
| `/api/usage-stats/*` | AI 使用统计 |
| `/api/version/*` | 版本管理 |
| `/api/events` (WS) | 实时事件流 |
| `/webhook/trigger/todo/{todo_id}` | 外网触发（事项） |
| `/webhook/trigger/loop/{loop_id}` | 外网触发（环路） |
