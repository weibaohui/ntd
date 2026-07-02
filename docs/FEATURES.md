# ntd 功能清单

> 本文档是 ntd (Nothing Todo) 的功能总览,按能力域划分,便于使用者快速了解系统能做什么、开发者快速定位代码模块。
>
> 前端组件级细节请参考 [frontend-features.md](./frontend-features.md),后端 API 细节请参考 [ntd-api.md](./ntd-api.md),CLI 命令细节请参考 [ntd-cli.md](./ntd-cli.md)。

---

## 目录

- [一、任务管理 (Todo)](#一任务管理-todo)
- [二、执行器与 AI 集成](#二执行器与-ai-集成)
- [三、Loop Studio (自动化工作流)](#三loop-studio-自动化工作流)
- [四、执行与会话 (Execution & Session)](#四执行与会话-execution--session)
- [五、自动评审 (Auto Review)](#五自动评审-auto-review)
- [六、调度与触发 (Scheduling & Triggering)](#六调度与触发-scheduling--triggering)
- [七、仪表盘与统计 (Dashboard & Stats)](#七仪表盘与统计-dashboard--stats)
- [八、使用统计 (Usage Stats)](#八使用统计-usage-stats)
- [九、模板系统 (Template)](#九模板系统-template)
- [十、标签与分类 (Tag)](#十标签与分类-tag)
- [十一、项目目录 (Project Directory)](#十一项目目录-project-directory)
- [十二、Skills 管理](#十二skills-管理)
- [十三、备份与恢复 (Backup)](#十三备份与恢复-backup)
- [十四、飞书消息集成 (Feishu)](#十四飞书消息集成-feishu)
- [十五、Webhook 集成](#十五webhook-集成)
- [十六、Hook 系统](#十六hook-系统)
- [十七、云端同步 (Cloud Sync)](#十七云端同步-cloud-sync)
- [十八、系统与配置](#十八系统与配置)
- [十九、版本与升级](#十九版本与升级)
- [二十、守护进程管理 (Daemon)](#二十守护进程管理-daemon)
- [二十一、CLI 命令行](#二十一cli-命令行)
- [二十二、跨平台与分发](#二十二跨平台与分发)
- [二十三、前端 UI 能力](#二十三前端-ui-能力)
- [二十四、可观测性与错误处理](#二十四可观测性与错误处理)

---

## 一、任务管理 (Todo)

| 功能点 | 说明 | 涉及模块 |
|--------|------|----------|
| 任务 CRUD | 创建、读取、更新、删除任务 | `backend/src/handlers/todo.rs`、`backend/src/db/todo.rs` |
| 任务状态机 | 待办/进行中/已完成/已失败/已取消/已归档六态流转 | `backend/src/models` |
| 标签关联 | 任务可挂载多个标签,支持多标签筛选 | `tag` 模块 |
| Cron 调度 | 单个任务可绑定独立 cron 表达式 | `scheduler` 模块 |
| 执行器选择 | 每个任务指定一个执行器(claudecode/codex/...) | `executor_config` |
| 工作目录 | 任务执行上下文(workspace)可独立配置 | `todo` 表 |
| 智能创建 | 自然语言描述 → AI 自动生成任务 | `handlers::execution::smart_create_handler` |
| 模板套用 | 通过预设模板一键创建标准化任务 | `todo_template` 模块 |
| Worktree 隔离 | 执行前自动创建 Git Worktree,避免污染主分支 | `adapters/*` |
| 占位符替换 | Prompt 中 `{{content}}` / `{{message}}` 等占位符运行时替换 | `execution` 模块 |
| 带参执行 | 执行时通过 `--param key=value` 注入参数 | CLI / HTTP API |
| 继续对话 | 复用上一次的 Session 续连执行器 | `session` 模块 |
| 强制停止 | 运行时强制 kill 执行进程 | `execution` 处理器 |
| 强制失败 | 将卡住任务标记为失败,释放并发配额 | `execution` 处理器 |
| 状态强制变更 | 手动覆盖任务状态,修复异常状态 | `todo::force_update_todo_status` |

---

## 二、执行器与 AI 集成

支持多种 AI CLI 执行器,均通过统一的适配器接口对接。

| 执行器 | 适配器模块 | 关键能力 |
|--------|-----------|----------|
| Claude Code | `adapters/claude_code.rs` + `claude_protocol.rs` + `agent_event.rs` | 原生流式事件、Worktree、Session 续连 |
| Codex | `adapters/codex.rs` | OpenAI Codex CLI |
| Codebuddy | `adapters/codebuddy.rs` | 腾讯云代码助手 |
| OpenCode | `adapters/opencode.rs` + `opencode_event.rs` | 开源代码助手 |
| AtomCode | `adapters/atomcode.rs` | AI 代码编辑器 |
| MobileCoder | `adapters/mobilecoder.rs` | 移动端代码助手 |
| Hermes | `adapters/hermes.rs` | 通用 AI 适配、后置 Todo 进度提取 |
| Kimi | `adapters/kimi.rs` | 月之暗面 Kimi |
| Pi | `adapters/pi.rs` + `pi_event.rs` | Pi AI 助手 |
| CodeWhale | `adapters/codewhale.rs` | CodeWhale AI |
| MiMo | `adapters/mimo.rs` + `mimo_event.rs` | 小米 MiMo |
| Zhanlu | `adapters/zhanlu.rs` + `zhanlu_event.rs` | 斩路 AI（输出格式同 OpenCode） |
| Kilo | `adapters/kilo.rs` + `kilo_event.rs` | 千刻 AI |

通用能力:
- **执行器注册中心**:`executor_service.rs` 统一管理所有执行器
- **执行器配置**:`executor_config` 模块持久化各执行器路径/启用状态
- **可用性检测**:单个 / 批量检测执行器二进制是否可用
- **执行测试**:在设置页直接发送测试 Prompt 验证通路

---

## 三、Loop Studio (自动化工作流)

> Loop Studio 是 ntd 的多步骤自动化工作流系统，支持 Cron/飞书/Webhook/标签等多种触发方式。

| 功能点 | 说明 | 涉及模块 |
|--------|------|----------|
| Loop CRUD | 创建、读取、更新、删除 Loop | `handlers/loop_.rs`、`db/loop_.rs` |
| 步骤管理 | Loop 内的有序步骤列表，每步可独立配置执行器和 Prompt | `db/entity/loop_steps.rs` |
| 多种触发方式 | Cron 定时、飞书命令、飞书消息、Todo 完成、标签添加、Webhook | `db/entity/loop_triggers.rs` |
| 步骤评审 | 每步可配置 AI 评审或人工评审 | `services/auto_review.rs` |
| 执行记录 | 完整的 Loop 执行历史和步骤级执行记录 | `db/entity/loop_executions.rs` |
| 执行引擎 | `LoopRunner` 异步执行 Loop，支持步骤间上下文传递 | `services/loop_runner.rs` |
| 调度器 | `LoopScheduler` 基于 Cron 的 Loop 定时调度 | `services/loop_scheduler.rs` |
| 触发分发 | `LoopTrigger` 统一分发多种触发源到 Loop 执行 | `services/loop_trigger.rs` |
| 黑板上下文 | 步骤间共享的上下文数据（blackboard） | `db/entity/loop_step_executions.rs` |
| Token 统计 | Loop 级别的 Token 用量汇总 | `handlers/loop_.rs` |
| 流程图可视化 | 前端 Loop 流程图展示（@xyflow/react） | `components/loop-flow/` |
| 看板视图 | Loop 执行状态看板 | `components/loop-kanban/` |

---

## 四、执行与会话 (Execution & Session)

| 功能点 | 说明 | 模块 |
|--------|------|------|
| 执行记录 | 每次执行落库:开始/结束时间、状态、Token、成本、结论 | `db::execution` |
| 实时日志 | 通过 SSE 推送执行过程日志 | `handlers::events_handler` |
| 日志持久化 | 日志存数据库,支持后续查询与回放 | `db::execution` |
| 状态实时刷新 | 任务启动/完成/失败触发前端 UI 实时更新 | `useExecutionEvents` |
| Session 管理 | 聚合同一任务的所有历史会话,支持续连 | `db::session` |
| Session 统计 | 各 Session 的 Token 消耗、成功率、平均耗时 | `handlers::session` |
| 链式分组 | 历史记录按 Session 自动分组折叠 | `ChainGroupCard` |
| YAML 导出 | 单次会话可导出为 YAML 供复用 | `markdown.ts` |
| 并发控制 | 全局最大并发数,超出排队等待 | `task_manager.rs` |
| 超时控制 | 单任务最大执行时长,超时自动失败 | `RuntimePanel` |
| Token 统计 | 输入/输出/缓存读写分别计费 | `usage_stats` |
| 成本计算 | 按模型单价计算 USD 成本 | `usage_stats` |

---

## 五、自动评审 (Auto Review)

> 执行完成后自动触发 AI 评审，支持自定义评审模板。

| 功能点 | 说明 | 涉及模块 |
|--------|------|----------|
| 自动评审开关 | Todo 级别启用/禁用自动评审 | `todos.auto_review_enabled` |
| 评审模板 | 独立的评审模板管理（CRUD） | `handlers/review_template.rs`、`db/review_template.rs` |
| 评审实例 | 执行完成后自动创建评审 Todo（todo_type=2） | `services/auto_review.rs` |
| 评审状态 | pending → success/failed/interrupted/skipped | `execution_records.last_review_status` |
| 评审评分 | 评审完成后写入 rating 字段 | `execution_records.rating` |
| Loop 评审 | Loop 步骤级评审，支持 AI/人工评审类型 | `services/auto_review.rs` |

---

## 六、调度与触发 (Scheduling & Triggering)

| 触发方式 | 说明 | 入口 |
|----------|------|------|
| 手动触发 | UI / CLI `ntd todo execute` | 任意 |
| Cron 调度 | 单任务级 cron 表达式 | `TodoScheduler` |
| Webhook 触发 | `GET/POST /webhook/trigger/{todo_id}` | `handlers::webhook` |
| 飞书消息 | 飞书群里 @Bot 或私聊触发 | `feishu_listener` |
| 智能创建触发 | SmartCreate 自动创建并执行 | `smart_create_handler` |
| 斜杠命令 | 飞书消息中以 `/` 开头的命令解析 | `agent_bot` 配置 |

调度能力:
- **时区支持**:任务级 cron 可绑定时区
- **预设 Cron**:5分钟/30分钟/小时/天/周等常用预设
- **5 段 / 6 段 Cron 互转**:前后端均支持标准 5 段和带秒的 6 段格式

---

## 七、仪表盘与统计 (Dashboard & Stats)

> 完整图表/卡片清单见 [frontend-features.md § 三](./frontend-features.md#三仪表盘-dashboard),后端 API 见 [ntd-api.md § 16](./ntd-api.md)。

| 维度 | 指标 |
|------|------|
| 任务概览 | 总数 / 运行中 / 已完成 / 失败数 |
| 执行概览 | 标签数 / 定时任务数 / 总执行次数 / 总花费 |
| 亮点数据 | 单日峰值 / 最高产模型 / 活跃天数 |
| 状态分布 | 饼图展示六态占比 |
| 触发来源 | 手动 / 定时 / Cron / 命令占比 |
| 执行器分布 | 各执行器调用次数与成功率 |
| 模型分布 | 各模型执行次数 / Token / 成本 |
| Token 趋势 | 每日输入/输出/缓存读写 |
| 缓存效率 | 各模型缓存命中率 |
| 活动热力图 | GitHub 风格贡献热力图 |
| 模型排行榜 | 按执行次数排名的模型列表 |
| 飞书消息统计 | 消息处理量、群聊/单聊占比 |
| 分享卡片 | 一键生成统计图分享图片 |

时间范围筛选:5小时 / 7天 / 14天 / 30天 / 自定义。

---

## 八、使用统计 (Usage Stats)

> Token 用量、成本、模型维度统计，支持 ccusage 集成。

| 功能点 | 说明 | 涉及模块 |
|--------|------|----------|
| 使用统计快照 | 按日汇总 Token 用量和成本 | `db/entity/usage_stats.rs` |
| 模型维度 | 各模型的 Token/成本明细 | `db/entity/usage_model_breakdown.rs` |
| 执行器维度 | 各执行器的每日聚合统计 | `db/entity/usage_executor_daily.rs` |
| 统计刷新 | 手动触发统计重新计算 | `handlers/usage_stats.rs` |
| 采集配置 | 配置统计采集开关和参数 | `services/usage_stats.rs` |
| ccusage 集成 | 兼容 ccusage 格式的使用统计 | `frontend/src/utils/database/usage_stats.ts` |

---

## 九、模板系统 (Template)

| 功能点 | 说明 |
|--------|------|
| 系统模板 | 内置一批开箱即用的标准任务模板 |
| 自定义模板 | 用户自建/复制/编辑/删除模板 |
| 模板分类 | 侧边栏按分类导航 |
| 模板搜索 | 按标题/内容关键词搜索 |
| 模板远程订阅 | 订阅远程 URL 拉取模板,支持自动同步 |
| 模板手动同步 | 立即拉取最新远程模板 |
| 取消订阅 | 移除远程模板源,保留本地已同步内容 |
| 模板插入 | 在 Prompt 编辑器光标位置插入模板内容 |
| 占位符声明 | 模板中可声明 `{{key}}` 占位符,执行时提示用户输入 |

---

## 十、标签与分类 (Tag)

| 功能点 | 说明 |
|--------|------|
| 创建标签 | 自定义名称与颜色 (hex) |
| 颜色管理 | 标签可设置颜色,UI 着色显示 |
| 标签筛选 | 任务列表按标签筛选,支持多选 |
| 标签删除 | 删除标签(自动解除任务关联) |
| 标签统计 | Dashboard 显示各标签任务数与执行情况 |

---

## 十一、项目目录 (Project Directory)

| 功能点 | 说明 |
|--------|------|
| 多目录管理 | 维护多个项目根目录,任务选择其一作为工作区 |
| 目录命名 | 为目录设置易记名称 |
| 自动补全 | 输入时从历史目录补全 |
| 目录校验 | 启动时校验目录是否存在,失效提示 |
| 隔离执行 | 不同项目的任务在不同目录执行,互不干扰 |

---

## 十二、Skills 管理

> Skills 是 AI 执行器的能力扩展包 (例如 `~/.claude/skills/`),ntd 提供跨执行器的统一管理。

| 功能点 | 说明 |
|--------|------|
| Skills 总览 | 统计卡片展示各执行器 Skill 数量 |
| 树形列表 | 按执行器分组的 Skill 树 |
| 搜索 | 按名称/描述搜索 |
| 视图切换 | 树形 / 扁平 视图 |
| Skill 详情 | 查看元信息与 Markdown 内容 |
| 导出 | 单个 Skill 打包为 ZIP |
| 导入 | 从 ZIP 导入 Skill |
| 跨执行器对比 | 展示各执行器共有 / 独有 Skill |
| 同步 | 选源 Skill 同步到其他执行器 |
| 调用追踪 | 记录 Skill 调用历史,可按 Skill/执行器筛选分页 |
| ntd-usage Skill | 内置 ntd 使用技能,`ntd skill install` 一键安装到执行器 |

---

## 十三、备份与恢复 (Backup)

提供三类数据备份机制,均支持自动 + 手动 + 文件管理。

### 10.1 数据库备份
- 立即下载 SQLite 数据库
- 服务器本地备份 (zip 压缩)
- 数据库 PRAGMA 优化 (vacuum/optimize)
- 定时自动备份,可配置保留份数
- 备份文件列表、下载、删除

### 10.2 Todo 备份
- 全量导出为 YAML
- 选择性导出 (勾选部分 Todo)
- 导入预览 (上传后展示内容供选择)
- 选择性导入
- 定时自动备份

### 10.3 Skill 备份
- 立即备份 / 定时自动备份
- 各执行器 Skill 数量概览
- 备份文件管理

### 10.4 日志清理
- 配置旧日志保留天数
- 手动触发清理

---

## 十四、飞书消息集成 (Feishu)

> 通过飞书自建应用 Bot 实现移动端触发与推送。

| 功能点 | 说明 |
|--------|------|
| Bot 绑定 | 二维码扫码授权,完成 Bot 绑定 |
| 多 Bot 支持 | 同时绑定多个飞书 Bot |
| 群聊白名单 | 配置允许接收消息的群 |
| 单聊推送 | 配置是否向私聊用户推送结果 |
| 群聊推送 | 配置是否向群聊推送结果 |
| 合并策略 | 连续消息合并,减少打扰 |
| 斜杠命令 | `/create xxx` `/run xxx` 等命令路由 |
| 默认响应 | 收到不匹配消息时执行的兜底 Todo |
| 历史消息拉取 | 周期性拉取指定群聊历史消息入库 |
| 历史消息查询 | 按聊天/发送者/关键词筛选 |
| 发送者管理 | 维护消息发送者档案 |
| 历史群聊配置 | 配置需要拉取历史的群 |
| 消息统计 | Dashboard 展示消息处理统计 |
| WS / SSE 长连 | 与飞书开放平台保持长连接接收事件 |
| Token 自动管理 | tenant_access_token 缓存与刷新 |

---

## 十五、Webhook 集成

| 功能点 | 说明 |
|--------|------|
| Webhook 创建 | 绑定 Todo 与外部 URL/Token |
| 触发执行 | `GET/POST /webhook/trigger/{todo_id}` 远程触发 |
| 签名校验 | 可选 Token 签名校验 |
| 调用记录 | 所有调用落库,可查询/重放 |
| 失败重试 | 失败调用可在 UI 手动重试 |

---

## 十六、Hook 系统

> 任务生命周期钩子,可在 Todo 上声明:执行前、执行后、状态变更时触发其他 Todo。

| 触发点 | 说明 |
|--------|------|
| pre-execute | 主 Todo 执行前同步调用 |
| post-execute | 主 Todo 执行结束后异步调用 |
| on-status-change | 状态变更时调用 |
| 钩子链 | 钩子本身也是 Todo,可继续挂载钩子 |
| UI 编辑 | `TodoHooksEditor` 可视化编排 |

---

## 十七、云端同步 (Cloud Sync)

> push/pull 本地变更到云端，支持增量同步。

| 功能点 | 说明 | 涉及模块 |
|--------|------|----------|
| 同步配置 | 配置云端同步目标和参数 | `handlers/sync.rs` |
| 推送变更 | 将本地 Todo/Tag/Template/ProjectDirectory 变更推送到云端 | `db/sync_record.rs` |
| 拉取变更 | 从云端拉取变更到本地 | `handlers/sync.rs` |
| 同步记录 | 记录每次同步的 delta 日志 | `db/entity/sync_records.rs` |
| 增量同步 | 按 entity_type + entity_id 去重，只同步变更部分 | `db/sync_record.rs` |
| 同步状态 | 查看最近一次同步状态（成功/失败/时间戳） | `handlers/sync.rs` |

---

## 十八、系统与配置

| 配置项 | 说明 |
|--------|------|
| 服务端口 | 修改 HTTP 监听端口 |
| 服务地址 | 修改监听地址 (默认 0.0.0.0) |
| 数据库路径 | 修改 SQLite 文件位置 |
| 日志级别 | DEBUG / INFO / WARN / ERROR |
| 默认时区 | 调度任务默认时区 |
| 最大并发数 | 同时运行任务上限 |
| 任务超时 | 单任务最长执行时间 |
| CORS | 开发模式放行所有来源,生产仅同源 |
| 压缩 | 响应统一启用 gzip 压缩 |
| 请求体上限 | 默认 10MB |

---

## 十九、版本与升级

| 功能点 | 说明 |
|--------|------|
| 版本展示 | `ntd version` / 设置页显示当前版本与 Git 信息 |
| 检查更新 | 调用 `npm view` 获取最新版本 |
| 在线升级 | 一键 `npm install -g @weibaohui/nothing-todo@latest` + daemon restart |
| 升级日志回显 | 升级输出回显到 UI,失败可手动重试 |

---

## 二十、守护进程管理 (Daemon)

通过 `ntd daemon ...` 子命令管理后台服务。

| 子命令 | 说明 |
|--------|------|
| `ntd daemon install` | 注册为系统服务 (macOS launchd / Linux systemd / Windows Service) |
| `ntd daemon uninstall` | 卸载系统服务 |
| `ntd daemon start` | 启动服务 |
| `ntd daemon stop` | 停止服务 |
| `ntd daemon restart` | 重启服务 |
| `ntd daemon status` | 查看运行状态 (支持 `--verbose` 查看最近日志) |
| `--force` | 强制重新安装 / 强制系统级操作 |

---

## 二十一、CLI 命令行

> 完整命令参考见 [ntd-cli.md](./ntd-cli.md)。下表为高层概览。

```
ntd [全局选项] <子命令>

全局选项:
  --server <URL>   API 服务器地址
  -o, --output     json | pretty | raw
  -f, --fields     字段过滤(逗号分隔)

子命令:
  version                 查看版本
  upgrade                 升级 ntd
  server start            启动 API 服务
  stats                   全局统计
  todo                    Todo 管理 (create/list/get/update/delete/execute/stop/execution/stats)
  loop                    Loop 管理 (list/get/update/delete/stop/stats/execute/execution/results)
  tag                     标签管理 (list/create/delete)
  daemon                  守护进程管理
  skill install           安装 ntd-usage Skill
```

特点:
- **AI 友好**:`raw` 输出格式不带 ApiResponse 包装,便于 LLM 直接解析
- **stdin 支持**:`ntd todo create --stdin` 从 stdin 读取 JSON
- **文件 Prompt**:`--file path/to/prompt.md` 从文件载入 Prompt
- **占位符参数**:`--param key=value` 注入运行时参数

---

## 二十二、跨平台与分发

| 平台 | 架构 | 分发包 |
|------|------|--------|
| Linux | x86_64 / ARM64 | `nothing-todo-linux-x64.tar.gz` / `...-arm64.tar.gz` |
| macOS | ARM64 (Apple Silicon) | `nothing-todo-darwin-arm64.tar.gz` |
| Windows | x86_64 | `nothing-todo-windows-x64.zip` |
| npm | 跨平台 | `@weibaohui/nothing-todo` |
| Cargo | 跨平台 | `crates.io` (源码) |

- **安装方式**:`npm install -g @weibaohui/nothing-todo`(推荐)、手动下载二进制、源码 cargo build
- **跨平台服务管理**:macOS launchd / Linux systemd / Windows Service

---

## 二十三、前端 UI 能力

> 详细功能点参见 [frontend-features.md](./frontend-features.md)。下表为能力域总览。

| 能力域 | 关键能力 |
|--------|----------|
| 任务列表 | 列表展示、筛选、搜索、排序、快速执行 |
| 任务详情 | 信息/历史/统计/对话视图/日志视图 |
| 看板视图 | 四列拖拽、实时筛选、移动端 Tab + 滑动手势 |
| 纪念碑视图 | 瀑布流回顾已完成任务结论 |
| 仪表盘 | 16+ 图表与统计卡片,支持时间范围与分享图 |
| 设置页 | 系统/执行器/标签/项目目录/模板/备份/消息/版本 |
| Skills 面板 | 树形浏览、跨执行器对比、同步、调用追踪 |
| 主题系统 | 亮/暗色主题,localStorage 持久化 |
| 国际化 | 中文(预留多语言扩展点) |
| 响应式 | 桌面双栏 + 移动单栏 + FAB + 底部抽屉 |
| 实时更新 | SSE 推送任务状态/日志/完成事件 |
| 路由 | URL 参数 `view=...&todo=ID` 持久化 |
| 错误边界 | 组件级 ErrorBoundary 降级 |
| 关系图谱 | Todo / Session / Tag 关系可视化 |

---

## 二十四、可观测性与错误处理

| 功能点 | 说明 |
|--------|------|
| 结构化日志 | `tracing` + `tracing-subscriber` 输出 JSON/格式日志 |
| HTTP Trace | 接入 `tower-http` TraceLayer,自动记录请求/响应耗时 |
| 健康检查 | `GET /health` 返回服务存活状态 |
| 错误响应 | 统一 `ApiResponse { code, message, data }` 包装 |
| 数据库迁移 | 启动时自动检测 schema 版本并迁移 |
| 数据库优化 | `PRAGMA optimize` 定期触发 |
| 前端错误边界 | 组件崩溃降级为友好提示 |
| 网络重试 | 前端关键请求带重试与超时控制 |

---

## 附录:模块路径速查

| 域 | 后端模块 | 前端模块 |
|----|----------|----------|
| 任务 | `backend/src/handlers/todo.rs` | `frontend/src/components/TodoList.tsx`、`TodoDetail.tsx`、`TodoDrawer.tsx`、`TodoCard.tsx` |
| 执行 | `backend/src/handlers/execution.rs`、`executor_service/` | `ChatView.tsx`、`ExecutionPanel.tsx` |
| Loop | `backend/src/handlers/loop_.rs`、`services/loop_runner.rs`、`services/loop_scheduler.rs` | `LoopPage.tsx`、`components/loop-studio/*`、`components/loop-flow/*` |
| 调度 | `backend/src/scheduler.rs`、`handlers/scheduler.rs` | `SchedulerSection.tsx`、`CronPresetSelect.tsx` |
| 标签 | `backend/src/handlers/tag.rs` | `TagsPanel.tsx`、`TagCheckCard.tsx` |
| 模板 | `backend/src/handlers/todo_template.rs`、`custom_template.rs` | `TemplatesPanel.tsx`、`TemplateModal.tsx` |
| 评审模板 | `backend/src/handlers/review_template.rs` | `ReviewTemplatesPanel.tsx` |
| Skills | `backend/src/handlers/skills.rs` | `SkillsPanel.tsx` 及 `components/skills/*` |
| 备份 | `backend/src/handlers/backup.rs` | `BackupPanel.tsx` 及 `components/settings/backup/*` |
| 飞书 | `backend/src/feishu/*`、`handlers/agent_bot.rs`、`handlers/feishu_history.rs` | `components/settings/bot/*`、`components/settings/messages/*` |
| Webhook | `backend/src/handlers/webhook.rs` | - |
| Hook | `backend/src/db/todo.rs`（hooks JSON 列） | `todo-detail/TodoHooksEditor.tsx` |
| 项目目录 | `backend/src/handlers/project_directory.rs` | `ProjectDirectoriesPanel.tsx` |
| 配置/系统 | `backend/src/handlers/config.rs`、`config.rs` | `SystemSettingsPanel.tsx` |
| 执行器 | `backend/src/handlers/executor_config.rs` | `ExecutorsPanel.tsx` |
| 运行管理 | `backend/src/task_manager.rs` | `RunningBoard.tsx`、`components/running-board/*` |
| 会话 | `backend/src/handlers/session.rs` | `SessionManager.tsx` 及 `components/sessions/*` |
| 守护进程 | `backend/src/daemon/`、`main.rs` | `AboutPanel.tsx` |
| CLI | `backend/src/cli/` | - |
| 仪表盘 | `backend/src/handlers/execution.rs`(stats)、`handlers/usage_stats.rs` | `Dashboard.tsx` 及 `components/dashboard/*` |
| 使用统计 | `backend/src/services/usage_stats.rs` | `components/dashboard/UsageStatsCard.tsx` |
| 云端同步 | `backend/src/handlers/sync.rs` | `CloudSyncPanel.tsx` |
| 看板 | - | `KanbanBoard.tsx` 及 `components/kanban/*` |
| Running Board | - | `RunningBoard.tsx` 及 `components/running-board/*` |
| 纪念碑 | - | `MemorialBoard.tsx` |
| 主题 | - | `themes/`、`hooks/useTheme.tsx` |

---

> 文档维护:新增功能时,请同时更新本文件与对应的细分文档(API/CLI/frontend-features),保持单一信息源。
