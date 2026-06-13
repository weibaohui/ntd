# AGENTS.md

本文件是给 AI Agent 阅读的项目说明文档。它汇总了 ntd (Nothing Todo) 项目的关键背景、约定与操作规范，让 agent 在不熟悉代码的情况下也能快速进入工作。

> 与 `CLAUDE.md` 的关系：`CLAUDE.md` 是面向 Claude Code 的更细粒度提示；`AGENTS.md` 是面向所有 AI agent 的「项目地图 + 工作守则」，两者互为补充。若有冲突，以代码现状和 `CLAUDE.md` 为准。

## 项目概述

**ntd** (Nothing Todo) 是一个 AI 驱动的 Todo 任务管理应用：创建任务 → 选择 AI 执行器 → 自动执行 → 跟踪结果与历史。它将传统待办事项管理与多种 AI CLI 工具深度集成，让任务不仅能被记录，还能被实际完成。

> "无事可做" — 因为 AI 已经帮你做完了。

### 核心特性

- **多 AI 执行器** — 集成 10+ 种 AI CLI 执行器（详见「执行器」一节）
- **智能任务管理** — 创建、编辑、跟踪 Todo，5 种状态机（待办/进行中/已完成/已取消/已归档）
- **可视化仪表盘** — 实时统计完成情况，支持 6h/12h/24h/3d/7d 时间区间趋势
- **看板视图** — 瀑布流展示最近完成的任务及 AI 执行结论
- **Session 管理** — 任务会话历史追踪，支持会话续连和状态恢复
- **项目目录管理** — 多项目隔离，每个项目独立目录和工作空间
- **Worktree 隔离** — Claude Code/Codex 等执行时自动创建 Git Worktree，隔离分支操作
- **定时调度** — 内置 Cron 调度器，支持定时触发任务执行
- **Todo 模板** — 预设任务模板，一键创建标准化任务流程
- **自动备份** — 定时自动备份数据，支持保留数量限制和一键下载
- **Webhook 集成** — 支持入站/出站 Webhook，便于与其他系统对接
- **飞书集成** — 支持飞书机器人消息推送、群组白名单、历史会话拉取
- **跨平台** — Windows / macOS / Linux（x86_64 & ARM64），通过 `cross` 工具交叉编译

## 开发流程

**禁止直接在主分支 (main) 上写代码。所有代码改动必须先创建分支，在分支上完成开发后再通过 PR 合入 main。**

每个改动建议走以下流程：

1. 从最新的 `main` 创建工作分支（`feat/xxx`、`fix/xxx`、`chore/xxx`）。
2. 在分支上完成开发 + 必要的测试。
3. 运行 `make dev` / 单测 / Playwright 验证。
4. 提交后推送远端，发起 PR，CI 通过后合入。

## 代码注释规范

**强制要求：所有新增/修改的代码必须带注释。**

- **逐行注释**：每一行代码都要写注释，解释「为什么这么写」（不是「写了什么」）。说明意图、设计取舍、边界条件、踩过的坑，而不是复述语法。
- **段落总览注释**：在大段代码（如函数实现、复杂逻辑块、状态机分支）之前，先用一段注释说明整体的处理思路、输入输出和关键步骤，让读者不用读代码就能理解做了什么。
- **避免无意义注释**：`// 自增 i` 这类复述代码本身的注释属于噪音，要写成「为什么需要自增」「自增的边界是什么」。
- **修改既有代码时**：如果改动了原有逻辑，要同步更新或新增注释，不能让注释与代码脱节。

### 示例

❌ 反例（注释复述了代码，没解释为什么）：

```ts
// 调用 loadExecutionRecords
await loadExecutionRecords(1, historyLimit);
```

✅ 正例（注释解释了意图与取舍）：

```ts
// 执行成功后立即重新拉取列表，确保用户能看到刚创建的记录；
// 回到第 1 页是因为新记录按时间倒序排在最前面，停留在原页会看不到。
await loadExecutionRecords(1, historyLimit);
```

✅ 段落总览示例：

```ts
// 切换 Todo 时重新加载执行记录与汇总信息。
// 使用 cancelledRef 防御快速切换造成的竞态：晚返回的请求若发现已切换，直接丢弃结果。
// 依赖 historyLimit / historyStatusFilter，因为分页大小或筛选条件变化也要重拉。
useEffect(() => { ... }, [selectedTodoId, historyLimit, historyStatusFilter]);
```

## 生产环境 vs 开发环境

### 生产环境（端口 8088）

- 配置：`~/.ntd/config.yaml`
- 数据库：`~/.ntd/data.db`
- 日志：`~/.ntd/daemon.log`
- PID：`~/.ntd/daemon.pid`
- 管理命令：

```bash
ntd daemon install   # 安装为系统服务
ntd daemon start     # 启动
ntd daemon stop      # 停止
ntd daemon restart   # 重启
ntd daemon status    # 查看状态
```

### 开发环境（端口 18088）

- 配置：`~/.ntd/config.dev.yaml`（首次自动创建）
- 数据库：`~/.ntd/data.dev.db`
- 日志：`backend.dev.log`
- PID：`~/.ntd/dev.pid`
- 管理命令：

```bash
make dev    # 启动开发模式（构建前端 + 启动后端 embedded 模式）
make stop   # 停止开发实例
make build  # 构建生产版本
```

### 端口区分

| 环境 | 端口  | 配置             | 数据库         |
|------|-------|------------------|----------------|
| 生产 | 8088  | config.yaml      | data.db        |
| 开发 | 18088 | config.dev.yaml  | data.dev.db    |

## 技术栈

### 后端（`backend/`）

- 语言：Rust（edition 2021）
- Web 框架：Axum 0.8（含 WebSocket 支持）
- 异步运行时：Tokio（full feature）
- ORM：SeaORM 0.12（基于 sqlx-sqlite）
- 数据库：SQLite（`libsqlite3-sys` bundled 模式，零外部依赖）
- 序列化：serde / serde_json / serde_yaml
- HTTP 客户端：reqwest（rustls-tls，无 native-tls）
- 调度器：tokio-cron-scheduler + cron
- 静态资源嵌入：rust-embed（前端产物直接嵌入二进制）
- 日志：tracing + tracing-subscriber
- 命令行：clap（derive 模式）
- 错误处理：thiserror + anyhow
- 进程管理：command-group（tokio 集成，隔离子进程组）
- 并发原语：parking_lot / dashmap / quick_cache
- 压缩：zip（备份场景）

### 前端（`frontend/`）

- 框架：React 19 + Vite 7
- UI 组件库：Ant Design 6 + `@ant-design/icons`
- 特殊组件：
  - `@ant-design/x-markdown` — AI 风格 Markdown 渲染
  - `@uiw/react-md-editor` — Markdown 编辑器
  - `@xyflow/react` — 关系图（relation-map）
  - `react-countup` — 数字滚动动画
  - `react-icons` — 图标库
  - `react-js-cron` — Cron 表达式选择器
- HTTP：axios
- YAML：js-yaml
- 时间：date-fns
- 二维码：qrcode
- 等宽字体：@fontsource/jetbrains-mono
- 测试：Playwright（`@playwright/test`）
- 语言：TypeScript ~5.8

### 数据 & 工具链

- 数据库：SQLite + SeaORM
- 版本控制：Git + GitHub
- CI：GitHub Actions（`rust.yml` workflow）
- 包发布：npm（`@weibaohui/nothing-todo`）
- 跨平台编译：cross（`make cross-build`）
- 提交规范：依赖 PR review，无强制 commit message 规范

## 执行器

`backend/src/adapters/` 下每个文件对应一种 AI CLI 执行器的适配器。**新增执行器**时遵循 `docs/ADD_EXECUTOR_GUIDE.md`（该文档是开发新适配器的权威指南）。

当前已支持的执行器（以 `backend/src/adapters/*.rs` 为准）：

| 执行器类型  | 适配器文件                   | 备注                                  |
|-------------|------------------------------|---------------------------------------|
| `claudecode` | `claude_code.rs` + `claude_protocol.rs` | Claude Code CLI                  |
| `joinai`     | `joinai.rs` + `joinai_event.rs`         | JoinAI                          |
| `opencode`   | `opencode.rs` + `opencode_event.rs`     | OpenCode                         |
| `hermes`     | `hermes.rs`                                | Hermes                          |
| `codex`      | `codex.rs`                                 | OpenAI Codex                    |
| `codebuddy`  | `codebuddy.rs`                             | 腾讯云 CodeBuddy                |
| `codewhale`  | `codewhale.rs`                             | CodeWhale                       |
| `kimi`       | `kimi.rs`                                  | 月之暗面 Kimi CLI                |
| `mobilecoder`| `mobilecoder.rs` + `mobilecoder_event.rs` | MobileCoder                     |
| `atomcode`   | `atomcode.rs`                              | AtomCode                        |
| `mimo`       | `mimo.rs` + `mimo_event.rs`                | 小米 MiMo                       |
| `pi`         | `pi.rs` + `pi_event.rs`                    | Pi                              |
| （共享）     | `agent_event.rs`                           | 通用 Agent 事件协议             |

> 历史上 AGENTS.md 曾误写为「仅支持 Claude Code 和 JoinAI」，请勿再沿用该描述。

## 目录结构

```
nothing-todo/
├── AGENTS.md                      # 本文件 — AI Agent 项目说明
├── CLAUDE.md                      # 面向 Claude Code 的更细粒度提示
├── README.md                      # 用户向介绍 + 安装指南
├── DEVELOPMENT.md                 # 开发者向补充说明
├── Makefile                       # 顶层构建入口（setup/install/build/dev/stop/cross-build）
├── Cross.toml                     # cross 工具配置
├── package.json / package-lock.json
├── templates.example.yaml         # 模板订阅配置示例
├── tunnel.sh                      # 内网穿透脚本
├── backend/                       # Rust 后端
│   ├── Cargo.toml / Cargo.lock / build.rs
│   ├── src/
│   │   ├── main.rs                # 二进制入口
│   │   ├── lib.rs                 # 库入口（供 main + 集成测试复用）
│   │   ├── config.rs              # 配置加载（YAML + 环境变量）
│   │   ├── daemon.rs              # 守护进程 / 系统服务
│   │   ├── executor_service.rs    # 执行器服务封装
│   │   ├── scheduler.rs           # Cron 调度器
│   │   ├── task_manager.rs        # 任务管理（运行态）
│   │   ├── todo_progress.rs       # Todo 进度跟踪
│   │   ├── service_context.rs     # 服务上下文（依赖注入容器）
│   │   ├── npm_utils.rs           # npm 工具
│   │   ├── adapters/              # 各种 AI CLI 适配器（见「执行器」表）
│   │   ├── cli/                   # `ntd` 子命令实现
│   │   ├── db/                    # 数据库层（SeaORM entity + 查询）
│   │   │   └── entity/            # 自动生成的 entity
│   │   ├── feishu/                # 飞书集成模块
│   │   ├── handlers/              # HTTP / WebSocket handlers
│   │   ├── hooks/                 # 钩子系统（models + service）
│   │   ├── models/                # 业务模型
│   │   └── services/              # 业务服务（auto_review, feishu_history_fetcher 等）
│   └── tests/                     # 集成测试
├── frontend/                      # React 前端
│   ├── package.json / tsconfig.json / vite.config.ts
│   ├── playwright.config.ts
│   ├── index.html / public/       # 入口与静态资源
│   ├── dist/                      # 构建产物（git 忽略）
│   ├── node_modules/              # 依赖（git 忽略）
│   ├── test-results/              # Playwright 产物（git 忽略）
│   └── src/
│       ├── main.tsx               # React 入口
│       ├── App.tsx / App.css
│       ├── constants.ts
│       ├── vite-env.d.ts
│       ├── assets/                # 静态资源
│       ├── components/            # 业务组件
│       │   ├── dashboard/         # 仪表盘子组件
│       │   ├── kanban/            # 看板子组件
│       │   ├── relation-map/      # 关系图
│       │   ├── sessions/          # 会话管理
│       │   ├── settings/          # 设置页
│       │   ├── skills/            # 技能面板
│       │   ├── todo-detail/       # Todo 详情
│       │   └── todo-drawer/       # Todo 抽屉
│       ├── hooks/                 # 自定义 React Hooks
│       ├── themes/                # 主题（暗色/亮色）
│       ├── types/                 # TypeScript 类型
│       └── utils/                 # 工具函数
├── docs/                          # 设计/规范/特性文档
│   ├── SPEC.md
│   ├── FEATURES.md
│   ├── frontend-features.md
│   ├── ntd-cli.md / ntd-api.md
│   ├── ADD_EXECUTOR_GUIDE.md      # 新增执行器适配器指南（重要）
│   ├── CLI_DESIGN.md
│   ├── ARCHITECTURE_HEALTH_CHECK_REPORT.md
│   ├── OPTIMIZATION_RECOMMENDATIONS.md
│   ├── session-management-design.md
│   ├── hook-system-design.md
│   ├── plan-feishu-messaging.md
│   ├── issue_295_pragma_optimize_api_issue.md
│   ├── NPM_PUBLIST.md
│   ├── design-system/             # 设计系统资源
│   └── user-guide/                # 用户向文档
├── ntd-skills/                    # 随仓库发布的 ntd-usage skill
│   └── ntd-usage/SKILL.md
├── packages/                      # 跨平台预编译产物（不同 OS/arch 的 npm 包）
│   ├── nothing-todo/
│   ├── nothing-todo-darwin-arm64/
│   ├── nothing-todo-linux-arm64/
│   ├── nothing-todo-linux-x64/
│   └── nothing-todo-windows-x64/
└── script/                        # 杂项脚本
```

## 前端导入规范

**强制要求：在 `frontend/src` 目录内编写或修改前端代码时，跨目录导入统一使用 `@/` 绝对路径别名，不要使用 `../`、`../../` 这类相对路径回退。**

- 推荐写法：`import { useTheme } from '@/hooks/useTheme';`
- 禁止写法：`import { useTheme } from '../hooks/useTheme';`
- 适用范围：`frontend/src` 下的组件、hooks、utils、types、themes 等源码文件。
- 例外情况：同目录内的短相对导入可以保留，例如 `./constants`、`./helpers`。
- 修改旧代码时：如果顺手触达已有相对路径导入，优先一并改成 `@/`，保持项目风格一致。

## 前端测试验证

**重要：修改前端 UI 后，必须使用 Playwright 进行自动化验证，再通知用户。**

### 测试脚本位置（强制要求）

**所有使用 Playwright 编写的前端功能测试（含正式 spec 和调试脚本）必须统一放在 `frontend/tests/` 目录下，禁止散落到 `frontend/` 根目录、`/tmp/` 或其他位置。**

- 目录约定：与后端 `backend/tests/` 保持一致，前端对应 `frontend/tests/`。
- 文件命名：
  - 正式 spec：`frontend/tests/**/*.spec.ts`，由 `@playwright/test` 直接驱动。
  - 临时调试脚本：`frontend/tests/check_*.cjs` 或 `frontend/tests/check_*.js`，按需保留/清理。
- Playwright 配置：`frontend/playwright.config.ts` 必须将 `testDir` 指向 `frontend/tests`，并在 `testMatch` 中覆盖 spec 与调试脚本。
- 禁止放在 `/tmp/` 等系统临时目录：CI、他人复跑、回归对比都依赖仓库内可追溯的脚本。

### 当前实际情况

- 历史上曾把 spec（如 `frontend/e2e-test.spec.ts`）和调试脚本（`test_*.cjs`、`debug_click.cjs`、`inspect.cjs` 等）直接放在 `frontend/` 根目录，违反上述约定，需要在改动 UI 时顺手迁回 `frontend/tests/` 并同步更新 `playwright.config.ts`。
- `frontend/test-results/` 是 Playwright 产物目录，由运行自动生成，已在 `.gitignore` 中忽略（除错误上下文外的报告），不要手动提交。

### 运行方式

由于 Playwright 依赖位于 `frontend/node_modules/`，需要在 `frontend/` 目录下执行：

```bash
cd frontend && npx playwright test --reporter=list
```

针对单个调试脚本（仍位于 `frontend/tests/` 下）：

```bash
cd frontend && npx playwright test tests/check_xxx.spec.ts --reporter=list
```

### 验证流程

1. 修改前端代码后，执行 `make dev` 重启开发服务（默认监听 `http://localhost:18088`）。
2. 在 `frontend/tests/` 下编写或更新对应的 Playwright spec / 调试脚本。
3. 若新增或移动了 spec 文件，同步更新 `frontend/playwright.config.ts` 的 `testDir` / `testMatch`。
4. 运行 Playwright 验证 UI 效果；不通过则继续修复，直到用例稳定。
5. 验证通过、确保无遗留 `/tmp/` 散落脚本后再通知用户。

### 常用验证脚本示例

```typescript
// 文件位置：frontend/tests/check_theme.spec.ts
// 用途：验证深色模式组件渲染
import { test, expect } from '@playwright/test';

test('深色模式渲染校验', async ({ page }) => {
  // 通过 localStorage 写入主题键，刷新后由 ThemeProvider 接管，
  // 避免仅依赖系统色导致用例在 CI 上不稳定。
  await page.goto('http://localhost:18088');
  await page.evaluate(() => localStorage.setItem('app_theme', 'dark'));
  await page.reload();
  await page.waitForTimeout(2000);

  // 采集目标节点的实际样式，作为断言依据；
  // 这里以背景色为例，验证主题色板生效。
  const result = await page.evaluate(() => {
    const el = document.querySelector('.target-class');
    return { bg: el ? getComputedStyle(el).backgroundColor : null };
  });
  console.log('验证结果:', result);

  // 截图留档，便于在 PR 中附图说明。
  await page.screenshot({ path: 'frontend/tests/__screenshots__/verify.png' });
});
```

## 后端测试

- 单元/集成测试位于 `backend/tests/`，通过 `cargo test` 运行。
- 调试 cargo 改动时可用 `cd backend && cargo check` 快速验证编译。
- 重要模块（`executor_service.rs`、`scheduler.rs`、`task_manager.rs`）改动后必须跑对应测试。

## 常用命令速查

```bash
# 首次环境准备
make setup                                # 安装 Rust/Node 依赖

# 开发
make dev                                  # 启动 dev 实例（端口 18088）
make stop                                 # 停止 dev 实例

# 构建
make build                                # 本地 release 构建
make install                              # 构建并安装到 ~/.local/bin/ntd
make cross-build                          # 跨平台构建（win/mac/linux x86+arm）

# 测试
cd frontend && npx playwright test --reporter=list    # 前端 Playwright
cd backend && cargo test                              # 后端测试
```

## 内网穿透

如需远程验证，可使用 `tunnel.sh` 启动公网访问：

```bash
./tunnel.sh
```

## 改动 AGENTS.md 的注意事项

- 本文件是「项目地图 + 工作守则」，**与代码现状保持同步是最高优先级**。任何目录、依赖、执行器、技术栈、命令的变动都应同步到本文件。
- 修改时遵循本文件自身的「代码注释规范」一节：写「为什么这么写」，而不是复述内容。
- 提交 PR 时如果新增/移动了目录或新增了执行器，记得在 PR 描述中点出 AGENTS.md 的对应章节已更新。
