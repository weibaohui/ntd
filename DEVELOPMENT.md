# 开发指南

## 技术栈

- **后端**: Rust (Axum 框架)
- **前端**: React 19 + Vite + Ant Design
- **数据库**: SQLite + SeaORM

## 前置要求

- [Rust](https://www.rust-lang.org/tools/install) 1.85+
- [Node.js](https://nodejs.org/) 20+
- [Make](https://www.gnu.org/software/make/)

## 常用命令

```bash
make install    # 构建并安装到 ~/.local/bin/ntd
make start      # 启动服务（需先 install）
make stop       # 停止服务
make restart    # 重启服务（开发调试常用）
make dev        # 开发模式：前后端分离，支持热重载
make build      # 仅构建
make clean      # 清理构建产物
```

## 端口

| 模式 | 前端 | 后端 |
|------|------|------|
| 生产模式 | 嵌入后端（8088） | 8088 |
| 开发模式 | 5173 | 8088 |

## 目录结构

```
backend/           # Rust 后端代码
  src/
    adapters/      # AI 执行器适配器
    handlers/      # HTTP 路由处理
    db.rs          # 数据库操作
    scheduler.rs   # Cron 调度器
    task_manager.rs # 任务生命周期管理
frontend/          # React 前端代码
packages/          # npm 跨平台分发包
docs/              # 文档和截图
tunnel.sh          # 内网穿透脚本
```

## 核心架构

ntd 是一个单 Agent 调度系统，不包含多 Agent 协作或长链推理编排。

```
用户创建 Todo → 触发执行 → ExecutorRegistry 选择执行器
                                      → 启动 AI CLI 子进程
                                      → 通过 broadcast channel 实时推送进度
                                      → Cron 调度器定时触发（可选）
```

| 组件 | 职责 |
|------|------|
| **ExecutorRegistry** | 适配器模式，统一管理多种 AI CLI 工具，每种实现 `CodeExecutor` trait |
| **Executor Service** | 启动 AI CLI 子进程，分离 stdout/stderr，解析日志，处理取消与异常终止 |
| **TaskManager** | 基于 tokio `mpsc` 通道的任务生命周期管理，支持取消信号广播 |
| **Scheduler** | 基于 `tokio-cron-scheduler` 的定时调度器，服务启动时从 DB 加载 |
| **事件总线** | `broadcast::channel<ExecEvent>` 实时推送事件到前端 |

**关键设计决策：**

- **单执行器单任务**：每个 Todo 同一时刻只由一个 AI 执行器处理
- **子进程隔离**：Unix 通过 `setpgid` 创建独立进程组，取消时级联杀死子进程树
- **孤儿清理**：程序崩溃后自动清理残留的 running 状态记录

**执行流程：**

```
1. 用户创建/触发 Todo
   ↓
2. 确定执行器（请求指定 > Todo 存储 > 默认 Claude Code）
   ↓
3. 创建 ExecutionRecord，Todo 状态 → running
   ↓
4. 启动 AI CLI 子进程
   ↓
5. 异步读取 stdout/stderr → broadcast channel → 前端
   ↓
6. 子进程结束 → 更新 ExecutionRecord
   ↓
7. Todo 状态 → completed/failed
```

## npm 发布

详见 [docs/NPM_PUBLIST.md](docs/NPM_PUBLIST.md)。

快速发布：

```bash
make cross-build                    # 交叉编译所有平台
./script/npm_publish.sh v0.1.2      # 一键发布到 npm
```
