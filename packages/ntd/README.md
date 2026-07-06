# ntd — Now Task, Done

A cross-platform AI task engine built with Rust.

> 创建即执行，执行即完成。

## Preview

![Dashboard](https://raw.githubusercontent.com/weibaohui/ntd/main/docs/dashboard.png)

![Task Detail](https://raw.githubusercontent.com/weibaohui/ntd/main/docs/detail.png)

## Features

- **智能任务管理** — 创建、编辑、跟踪 Todo，支持多种状态（待办、进行中、已完成、已取消、已归档）
- **多 AI 执行器支持** — 集成 Claude Code、CodeBuddy、OpenCode、AtomCode 等多种 AI CLI 工具
- **可视化仪表盘** — 实时统计任务完成情况，支持趋势图表和数据洞察
- **标签系统** — 灵活的标签分类，快速筛选和定位任务
- **定时调度** — 内置 Cron 调度器，支持定时触发任务执行
- **跨平台** — 支持 Windows、macOS、Linux（x86_64 & ARM64）
- **执行历史** — 完整记录每次执行的日志，支持重新执行

## Installation

```bash
npm install -g @weibaohui/ntd
```

## Usage

```bash
ntd              # 启动服务（默认端口 8088）
ntd version      # 查看版本信息
ntd upgrade      # 升级到最新版本
ntd --help       # 查看帮助
```

启动后打开浏览器访问 http://localhost:8088

## Supported AI Executors

| Executor | Description |
|----------|-------------|
| Claude Code | Anthropic 官方 CLI |
| CodeBuddy | 代码助手 |
| OpenCode | 开源代码助手 |
| AtomCode | AI 代码编辑器 |

## Platform Support

- macOS (arm64)
- Linux (x64, arm64)
- Windows (x64)

## License

Polyform Noncommercial License 
