# ntd Backend

Rust + Axum 后端，为 ntd (Now Task, Done) 提供 HTTP API、Cron 调度、飞书消息桥接、SQLite 持久化等能力。
前端构建产物通过 `rust-embed` 嵌入二进制，分发一个可执行文件即可运行整套应用。

> 配套文档：[ARCHITECTURE.md](./ARCHITECTURE.md) · [SEQUENCE.md](./SEQUENCE.md) · [CONFIG.md](./CONFIG.md)

---

## 1. 模块概览

`backend/src` 下的主要模块与职责：

| 模块 | 职责 |
|------|------|
| `main.rs` | CLI 入口（version / upgrade / server / todo / tag / daemon / skill），启动时调用 `run_server` 装配 HTTP 服务 |
| `config.rs` | 统一配置（`~/.ntd/config.yaml` 或 `~/.ntd/config.dev.yaml`），所有组件都从这里读，不直接读环境变量 |
| `daemon.rs` | `ntd daemon install/uninstall/start/stop/restart/status` 子命令，基于 launchd (macOS) / systemd (Linux) |
| `cli/` | 子命令级别的客户端（`ntd todo add`、`ntd tag list` 等），通过 HTTP 调用本机 API |
| `service_context.rs` | 跨模块共享的运行时依赖容器（db、executor_registry、tx、task_manager、config） |
| `adapters/` | AI 执行器适配层。统一 `CodeExecutor` trait + `ExecutorRegistry`，每个 CLI 工具（Claude Code / Codex / OpenCode / Hermes / Kimi / Mimo / MobileCoder / CodeWhale / Pi / CodeBuddy / AtomCode）一个子模块 |
| `executor_service.rs` | 启动 AI CLI 子进程、解析 stdout/stderr、调度并发上限、超时控制、子进程组级联终止 |
| `task_manager.rs` | 任务生命周期管理：基于 `mpsc::UnboundedSender` 广播取消信号；`TaskInfo` 用于 WebSocket 同步当前运行列表 |
| `scheduler.rs` | 基于 `tokio-cron-scheduler` 的 Cron 调度；启动时从 DB 加载；时区换算到 UTC |
| `handlers/` | Axum HTTP 路由。按业务域拆分（todo / tag / scheduler / session / config / execution / webhook / feishu_* / skills / agent_bot / sync / backup / usage_stats / executor_config / project_directory / todo_template / custom_template） |
| `hooks/` | Hook 触发引擎：父 Todo 完成事件可级联启动子 Todo；带 chain 检测避免循环 |
| `models/` | 与 DB / API 共用的数据结构（Todo、ExecutionRecord、ParsedLogEntry 等）+ 占位符替换等工具 |
| `db/` | SeaORM 数据访问层。所有公开方法 async；SQLite WAL + 外键开启 |
| `db/entity/` | SeaORM 实体（todos / execution_records / execution_logs / webhooks / webhook_records / tags / todo_tags / executors / agent_bots / feishu_* / project_directories / sync_records / todo_templates / skills / usage_* 等） |
| `feishu/` | 飞书长连接通道、消息编解码、配置、SDK 封装 |
| `services/` | 飞书消息监听、消息去抖、自动 review、推送、用量统计等业务服务 |
| `todo_progress.rs` | Todo 进度解析与展示 |
| `npm_utils.rs` | 与 npm 全局安装交互的工具（探测 prefix、定位 ntd 二进制路径） |
| `lib.rs` | 把以上模块 `pub mod` 出去，并嵌入前端 dist 与 ntd-skills 资源 |

---

## 2. 本地开发

### 2.1 前置依赖

- Rust 1.85+
- Node.js 20+（仅在改前端或跑 `make build` 时需要）
- Make

### 2.2 常用命令

在仓库根目录执行：

| 命令 | 说明 |
|------|------|
| `make setup` | 一次性安装 Rust / Node / cross 等依赖 |
| `make dev` | 启动开发模式（端口 18088，前后端分离，热重载） |
| `make stop` | 停止开发实例 |
| `make build` | 仅构建生产版本（编译前端 → 嵌入 → cargo build --release） |
| `make clean` | 清理构建产物 |
| `make install` | 构建并安装到 `~/.local/bin/ntd` |
| `make cross-build` | 交叉编译 win / mac / linux x86+arm |

### 2.3 端口区分

| 环境 | 端口 | 配置文件 | 数据库 |
|------|------|---------|--------|
| 生产 | 8088 | `~/.ntd/config.yaml` | `~/.ntd/data.db` |
| 开发 (`NTD_MODE=dev`) | 18088 | `~/.ntd/config.dev.yaml` | `~/.ntd/data.dev.db` |

> 通过设置 `NTD_MODE=dev` 环境变量切换开发/生产模式；`config.rs` 会据此选择配置文件路径和默认端口。

### 2.4 单独运行后端测试

```bash
cd backend
cargo test
```

---

## 3. 数据库迁移与初始化

数据库文件位置由 `Config.db_path` 决定，默认 `~/.ntd/data.db`。

### 3.1 启动流程（按顺序执行）

1. `Database::new(path)` → SeaORM 连接 + 设置 PRAGMA (`busy_timeout=5000` / `foreign_keys=ON` / `journal_mode=WAL`)
2. `init_tables()` — 通过 SeaORM `create_table_from_entity` 同步创建所有表（幂等）
3. `seed_default_templates()` — 首次启动写入内置 Todo 模板
4. `migrate_feishu_fk_cascade()` — 给飞书子表补 `ON DELETE CASCADE`（SQLite 不支持 ALTER 外键，需要新建 → 拷贝 → 删除 → 重命名）
5. `migrate_logs_to_execution_logs()` — 老的 `logs` 表拆到 `execution_logs`（按 `record_id` 关联）
6. `migrate_from_config(&cfg.executors)` — 老的 `config.yaml` 里 `executors.paths` 一次性迁到 `executors` 表
7. `seed_default_executors()` — 如果 `executors` 表为空，写入内置执行器列表
8. `sync_new_executors()` — 比对代码中 `EXECUTORS` 常量与 DB，添加新增的、删除内置已移除的
9. `backfill_session_dir()` — 给历史执行器补 `session_dir`
10. `cleanup_orphan_execution_records()` — 启动时清理 status=running 但进程已退出的孤儿
11. `cleanup_old_webhook_records(30)` — 删除超过 30 天的 webhook 投递记录

### 3.2 新增字段 / 表的标准做法

- 优先改 `db/entity/*.rs` 实体定义，`init_tables()` 会自动建表（缺失列不会自动补，需要写迁移）
- 如需新增列：写一个类似 `migrate_feishu_fk_cascade()` 的函数，在 `Database::new` 末尾调用，包裹在事务中
- 删除字段：直接删实体 + 同步删 handler + 测试；旧数据保留但不再读写

### 3.3 开发环境数据库

首次启动会自动创建 `~/.ntd/data.dev.db`。如果 schema 改了想从零开始：

```bash
rm ~/.ntd/data.dev.db
make dev
```

---

## 4. CLI 命令速查

| 命令 | 说明 |
|------|------|
| `ntd` | 无子命令时直接启动服务（默认端口读取 config） |
| `ntd version` | 打印版本与 git SHA |
| `ntd upgrade` | 通过 npm 拉取最新版并重启 daemon |
| `ntd server start [--port 8088]` | 显式启动服务 |
| `ntd todo ...` | Todo CRUD、子命令见 `cli/commands.rs` |
| `ntd tag ...` | 标签管理 |
| `ntd stats` | 全局统计 |
| `ntd daemon install\|uninstall\|start\|stop\|restart\|status` | 服务管理 |
| `ntd skills install [--force] [--executor claudecode,...]` | 把内置的 `ntd-usage` 技能安装到各执行器技能目录 |

---

## 5. 故障排查

| 症状 | 检查点 |
|------|-------|
| 启动报 `Failed to bind to port` | `~/.ntd/config.yaml` 里 `port` 是否被占用；`make stop` 清掉残留进程 |
| DB 报 `database is locked` | SQLite WAL 模式已被强制开启；通常 5 秒 `busy_timeout` 后自愈；若持续，检查是否有别的进程在写同一个 db 文件 |
| Todo 执行后立刻 Finished 无日志 | 检查 executor 二进制是否在 `$PATH`；`Executors` 表 `enabled=1` 且 `path` 正确 |
| WebSocket 连不上 | 浏览器代理拦截了 upgrade；生产模式下 `Host` 是否能从外部访问 |
| 飞书无响应 | `agent_bots` 表里 `app_id/app_secret` 是否填写；`feishu_project_bindings` 是否启用 |

---

## 6. 测试

```bash
# 全部测试
cd backend && cargo test

# 仅某个模块
cd backend && cargo test --lib config::

# 集成测试（需要 tempfile）
cd backend && cargo test --test '*'
```

CI 期望所有 `cargo test` 通过后再合并；新增模块请同步加单元测试，至少覆盖：
- 公共 API 的正常路径
- 配置/路径归一化
- 时区/并发上限等边界条件

---

## 7. 相关文档

- [ARCHITECTURE.md](./ARCHITECTURE.md) — 模块依赖图、关键流程、数据模型、配置项
- [SEQUENCE.md](./SEQUENCE.md) — 5 个关键流程的 mermaid sequenceDiagram
- [CONFIG.md](./CONFIG.md) — `~/.ntd/config.yaml` 全部字段说明
- 仓库根 `DEVELOPMENT.md` — 跨前后端的开发指南
- 仓库根 `CLAUDE.md` — Claude Code 协作约定（注释规范、分支策略等）
