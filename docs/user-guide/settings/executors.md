# 执行器管理

> **位置**：设置 → 执行器管理
> **前端**：`frontend/src/components/settings/ExecutorsPanel.tsx`
> **后端**：`backend/src/handlers/executor_config.rs`

「执行器」（Executor）是 ntd 把 Todo 实际跑起来的桥梁。ntd 本身不执行代码，而是把 Todo 的 prompt 交给外部 CLI 工具（Claude Code、Codex、Hermes、Kimi 等）来跑。

---

## 1. 支持的执行器

| Name | 类型 | 默认路径 | 备注 |
|------|------|----------|------|
| `claudecode` | Claude Code CLI | `~/.local/bin/claude` | 主力 |
| `codebuddy` | CodeBuddy CLI | 系统 PATH | |
| `opencode` | OpenCode CLI | `~/.opencode/bin/opencode` | |
| `atomcode` | AtomCode CLI | 系统 PATH | |
| `hermes` | Hermes CLI | `~/.hermes/bin/hermes` | |
| `kimi` | Kimi CLI | `~/.kimi/bin/kimi` | |
| `joinai` | JoinAI CLI | 系统 PATH | |
| `codex` | Codex CLI | `~/.codex/bin/codex` | |

> 「系统 PATH」表示 ntd 会从 `$PATH` 里找 `which codebuddy` / `which opencode` 等。

---

## 2. 执行器状态

每个执行器有 4 个状态：

| 状态 | 含义 |
|------|------|
| **已启用** | 该执行器会出现在 Todo 创建时的下拉框中 |
| **未启用** | 不会出现在 UI；老的关联 Todo 仍能跑（如果你再启用） |
| **检测通过** | 自动检测的路径有效（能 `which` 到） |
| **检测失败** | 路径无效或 CLI 不存在 |

---

## 3. 配置项

每个执行器的设置项：

| 字段 | 含义 |
|------|------|
| **name** | 内部 ID（不可改） |
| **display_name** | UI 显示名（可改） |
| **binary_path** | CLI 绝对路径，留空走 `$PATH` |
| **enabled** | 启用开关 |
| **env** | 自定义环境变量（JSON），注入到子进程 |
| **extra_args** | 额外命令行参数 |

---

## 4. 关键操作

### 4.1 自动检测全部

入口：右上角「**自动检测全部**」按钮

- 遍历所有执行器
- 调用 `which` / 检查默认路径
- 自动填到 `binary_path`
- 启用能找到的执行器

### 4.2 单个检测

- 列表里每个执行器的「**检测**」按钮
- 立即跑一次检测，刷新状态

### 4.3 测试连接

- 列表里每个执行器的「**测试**」按钮
- 用该执行器跑一个 `hello world` 探针
- 显示 stdout/stderr + 退出码
- 用于确认配置正确（不只是路径对，CLI 还能跑）

### 4.4 修改配置

- 点列表项 → 编辑表单
- 改完点「**保存**」→ `PUT /api/executors/{name}`

---

## 5. AI 使用统计

> 入口：执行器管理 → 底部「**AI 使用统计**」卡片

### 5.1 作用

把 ntd 通过各执行器跑过的任务按 **日/周/月 + model** 汇总 token 数和成本，写入 `usage_stats` 表。

### 5.2 配置

| 字段 | 默认 | 含义 |
|------|------|------|
| `enabled` | false | 是否开启 |
| `cron` | `0 0 2 * * *` | 汇总周期（每天凌晨 2 点） |
| `retention_days` | 90 | 保留天数 |

### 5.3 查看

入口：仪表盘 → 「**AI 使用统计**」卡片

- 按日/周/月切换
- 按 model 分组
- 趋势图（折线）

### 5.4 API

| Method | Path |
|--------|------|
| GET | `/api/usage-stats` |
| POST | `/api/usage-stats/refresh` |
| GET | `/api/usage-stats/settings` |
| PUT | `/api/usage-stats/settings` |

---

## 6. 故障排查

### 6.1 检测失败「which: not found」

- CLI 没装：去执行器官网装
- PATH 不对：填绝对路径（`/Users/xxx/.local/bin/claude`）
- 权限不够：`chmod +x ~/.local/bin/claude`

### 6.2 测试连接「spawn failed」

- 看后端日志，找具体 `tokio::process::Command` 报错
- 检查 `binary_path` 是不是真的可执行文件
- macOS 上首次运行会弹「无法验证开发者」，需要去「系统设置 → 隐私与安全」允许

### 6.3 Todo 选了执行器但跑不起来

- 检查执行器是否启用
- 检查 binary 是否被删（`which` 看）
- 看 Todo 详情 → 执行记录 → 日志，开头会有 spawn 失败的 stderr

---

## 7. 相关 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/executors` | 列所有执行器 |
| PUT | `/api/executors/{name}` | 改单个 |
| POST | `/api/executors/{name}/detect` | 单个检测 |
| POST | `/api/executors/{name}/test` | 单个测试 |
| POST | `/api/executors/detect-all` | 全部检测 |
