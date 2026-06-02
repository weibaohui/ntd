# Hook 系统设计文档

## 概述

Hook 系统允许在 Todo 状态变化的各个关键节点自动触发预设的脚本/命令执行。支持全局默认配置和 Per-Todo 独立配置。

---

## 一、配置层级架构

```
┌─────────────────────────────────────────────────────────┐
│                  Global Hook Settings                    │
│                    (全局默认配置)                         │
│  ┌─────────────────────────────────────────────────┐    │
│  │ enabled: bool                                   │    │
│  │ default_timeout_secs: u64                       │    │
│  │ max_concurrency: u32                            │    │
│  │ default_rules: Vec<HookRuleRef>  ← 默认规则列表  │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
                           │
          ┌────────────────┼────────────────┐
          ▼                ▼                ▼
   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
   │  Rule 1     │  │  Rule 2     │  │  Rule 3     │
   │  (规则库)    │  │             │  │             │
   └─────────────┘  └─────────────┘  └─────────────┘
                           │
                           ▼
   ┌─────────────────────────────────────────────────┐
   │            Per-Todo Hook Override                │
   │  ┌───────────────────────────────────────────┐  │
   │  │ hooks_enabled: bool                       │  │
   │  │ hook_mode: inherit | custom | disabled    │  │
   │  │ rules: Vec<HookRuleRef | InlineHook>      │  │
   │  └───────────────────────────────────────────┘  │
   └─────────────────────────────────────────────────┘
```

### 层级说明

| 层级 | 说明 | 优先级 |
|------|------|--------|
| Global Hook Config | 系统级默认配置 | 最低 |
| Hook Rules (规则库) | 可复用的 hook 规则定义 | 中 |
| Per-Todo Hook | 单个 Todo 的独立配置 | 最高 |

### hook_mode 三种模式

- `inherit`: 继承全局默认 Hook 规则
- `custom`: 使用 Per-Todo 自定义的 Hook 规则
- `disabled`: 禁用此 Todo 的所有 Hook

---

## 二、触发点 (Trigger Points)

| 触发点 | 同步/异步 | 可取消操作 | 说明 |
|--------|-----------|------------|------|
| `before_create` | 同步 | 可取消创建 | 创建 Todo 前触发 |
| `after_create` | 异步 | 不可 | 创建 Todo 后触发 |
| `before_status_change` | 同步 | 可取消状态变更 | 状态变更前触发 |
| `after_status_change` | 异步 | 不可 | 状态变更后触发 |
| `before_delete` | 同步 | 可取消删除 | 删除 Todo 前触发 |
| `after_delete` | 异步 | 不可 | 删除 Todo 后触发 |
| `before_execute` | 同步 | 可取消执行 | Executor 执行前触发 |

### 执行上下文数据流

| 字段 | before_create | before_status_change | after_status_change | before_execute |
|------|---------------|---------------------|---------------------|----------------|
| todo_id | - | ✓ | ✓ | ✓ |
| todo_title | ✓ | ✓ | ✓ | ✓ |
| old_status | - | ✓ | ✓ | - |
| new_status | ✓ | ✓ | ✓ | ✓ |
| executor | ✓ | ✓ | ✓ | ✓ |
| workspace | ✓ | ✓ | ✓ | ✓ |
| task_id | - | - | - | - |
| trigger_time | ✓ | ✓ | ✓ | ✓ |

---

## 三、Hook 规则结构

### 3.1 Filter (过滤条件)

```json
{
  "status": ["pending", "in_progress"],
  "title_contains": "报告",
  "tags": [1, 2, 3],
  "executor": "claude"
}
```

所有条件为 AND 关系。

### 3.2 Action (执行动作)

```json
{
  "command": "curl",
  "args": ["-X", "POST", "https://example.com/notify"],
  "env": {
    "FROM_HOOK": "true",
    "TODO_TITLE": "{{todo_title}}"
  },
  "timeout_secs": 30
}
```

### 3.3 模板变量

| 变量 | 说明 | 示例 |
|------|------|------|
| `{{todo_id}}` | Todo ID | `123` |
| `{{todo_title}}` | Todo 标题 | "完成报告" |
| `{{todo_status}}` | 当前状态 | `pending` |
| `{{old_status}}` | 旧状态 | `pending` |
| `{{new_status}}` | 新状态 | `completed` |
| `{{executor}}` | 执行器名称 | `claude` |
| `{{workspace}}` | 工作目录 | `/home/user/project` |
| `{{task_id}}` | 任务 ID | `task_abc123` |
| `{{trigger_time}}` | 触发时间 ISO8601 | `2026-05-31T10:00:00Z` |
| `{{env.VAR_NAME}}` | 引用环境变量 | - |

---

## 四、数据库设计

### 4.1 Hooks 表 (规则库)

```sql
CREATE TABLE hooks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    enabled INTEGER DEFAULT 1,
    trigger TEXT NOT NULL,
    filter TEXT,
    action TEXT NOT NULL,
    async INTEGER DEFAULT 1,
    created_at TEXT,
    updated_at TEXT
);
```

### 4.2 Global Hook Config 表

```sql
CREATE TABLE global_hook_config (
    id INTEGER PRIMARY KEY,
    enabled INTEGER DEFAULT 1,
    default_timeout_secs INTEGER DEFAULT 30,
    max_concurrency INTEGER DEFAULT 5,
    updated_at TEXT
);

CREATE TABLE global_default_hooks (
    id INTEGER PRIMARY KEY,
    hook_id TEXT NOT NULL,
    priority INTEGER DEFAULT 0,
    FOREIGN KEY (hook_id) REFERENCES hooks(id)
);
```

### 4.3 Per-Todo Hook 配置

```sql
CREATE TABLE todo_hooks (
    id INTEGER PRIMARY KEY,
    todo_id INTEGER NOT NULL UNIQUE,
    hook_mode TEXT DEFAULT 'inherit',
    override_enabled INTEGER DEFAULT 1,
    created_at TEXT,
    updated_at TEXT,
    FOREIGN KEY (todo_id) REFERENCES todos(id)
);

CREATE TABLE todo_hook_rules (
    id INTEGER PRIMARY KEY,
    todo_hook_id INTEGER NOT NULL,
    hook_id TEXT,
    inline_hook TEXT,
    priority INTEGER DEFAULT 0,
    FOREIGN KEY (todo_hook_id) REFERENCES todo_hooks(id)
);
```

### 4.4 Hook 执行日志表

```sql
CREATE TABLE hook_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hook_id TEXT NOT NULL,
    trigger TEXT NOT NULL,
    todo_id INTEGER,
    args_sent TEXT,
    env_sent TEXT,
    exit_code INTEGER,
    stdout TEXT,
    stderr TEXT,
    duration_ms INTEGER,
    success INTEGER,
    error_msg TEXT,
    created_at TEXT,
    FOREIGN KEY (hook_id) REFERENCES hooks(id)
);
```

---

## 五、API 设计

### 5.1 Hook 规则 CRUD

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/xyz/hooks` | 列出所有 hook 规则 |
| POST | `/xyz/hooks` | 创建 hook 规则 |
| GET | `/xyz/hooks/{id}` | 获取单个 hook 规则 |
| PUT | `/xyz/hooks/{id}` | 更新 hook 规则 |
| DELETE | `/xyz/hooks/{id}` | 删除 hook 规则 |
| POST | `/xyz/hooks/{id}/test` | 测试 hook (dry run) |

### 5.2 全局默认配置

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/xyz/hooks/config` | 获取全局默认配置 |
| PUT | `/xyz/hooks/config` | 更新全局默认配置 |

### 5.3 Per-Todo Hook

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/xyz/todos/{id}/hooks` | 获取 todo 的 hook 配置 |
| PUT | `/xyz/todos/{id}/hooks` | 更新 todo 的 hook 配置 |

### 5.4 执行日志

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/xyz/hook-logs` | 查看 hook 执行日志 |
| DELETE | `/xyz/hook-logs` | 清除执行日志 |
| GET | `/xyz/hook-logs/{id}` | 查看单条日志详情 |

---

## 六、管理界面设计

### 6.1 导航结构

```
📁 设置
   ├── 系统设置 (包含 [Hooks 设置] Tab)
   └── Hook 管理
           ├── Hook 列表
           ├── [新建/编辑 Hook] → 弹窗
           └── Hook 日志
```

### 6.2 Hook 管理页面 (`/hooks`)

采用卡片布局，展示所有 hook 规则。

### 6.3 Hook 规则编辑弹窗

- 名称
- 触发时机 (下拉选择)
- 过滤条件 (状态、标题、标签)
- 执行动作 (命令、参数、超时)
- 异步执行开关

### 6.4 全局默认 Hook 设置 (系统设置 Tab)

- 启用/禁用 Hook 系统
- 默认超时
- 最大并发数
- 默认应用的 Hook 规则列表

### 6.5 Per-Todo Hook 配置 (Todo 编辑面板 Tab)

- hook_mode 选择 (inherit/custom/disabled)
- 自定义规则列表
- 禁用所有 Hook 开关

### 6.6 Hook 日志页面

- 过滤器 (hook、状态)
- 日志列表 (时间、hook、todo、状态、耗时)
- 详情展开

---

## 七、执行流程

```
Todo 状态变更请求
       │
       ▼
┌─────────────────┐
│  获取变更上下文  │
│  (old_status,   │
│   new_status)   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  获取 Todo 的   │
│  Hook 配置      │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  解析 hook_mode │
│  inherit/custom │
│  /disabled      │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  查询匹配的     │
│  before_* hooks │
└────────┬────────┘
         │
         ▼
   ┌─────────────┐
   │  同步执行    │──────── 失败 ──────▶ 返回 error，不变更状态
   │  before_*   │
   └──────┬──────┘
          │ 成功
          ▼
   ┌─────────────┐
   │  变更 Todo  │
   │  状态       │
   └──────┬──────┘
          │
          ▼
   ┌──────────────────┐
   │ 查询匹配的       │
   │ after_* hooks    │
   └────────┬────────┘
            │
            ▼
     ┌─────────────┐
     │  异步入队    │─────────▶ 异步执行器 (不阻塞)
     │  after_*    │
     └─────────────┘
```

---

## 八、错误处理策略

| 场景 | 行为 |
|------|------|
| before_* hook 执行失败 | 拒绝操作，返回 error 给调用方 |
| after_* hook 执行失败 | 仅记录日志，不影响主流程 |
| hook 执行超时 | 杀死进程，记录超时日志 |
| hook 命令不存在 | 记录 error 日志，跳过执行 |

---

## 九、安全性考虑

1. **命令白名单** - 可配置允许执行的命令列表
2. **参数校验** - 防止命令注入
3. **环境隔离** - hook 执行环境与主进程隔离
4. **资源限制** - 超时 + 并发数限制
5. **敏感信息** - env 中的敏感值可标记为 secret（不记录日志）

---

## 十、与现有 Webhook 的关系

| 特性 | Webhook | Hook |
|------|---------|------|
| 目标 | 外部 HTTP 回调 | 本地命令执行 |
| 触发方式 | 外部系统订阅 | 本地事件触发 |
| 典型用途 | 通知外部系统 | 执行清理脚本、通知等 |

两者互为补充，可独立使用。

---

## 十一、文件结构

```
backend/src/
├── hooks/                      # 新增 Hook 模块
│   ├── mod.rs                 # 模块入口
│   ├── models.rs               # Hook 相关数据模型
│   ├── service.rs              # Hook 执行服务
│   ├── executor.rs             # 命令执行器
│   ├── template.rs             # 模板变量渲染
│   ├── filter.rs               # 过滤条件匹配
│   └── db/                      # 数据库操作
│       ├── mod.rs
│       ├── hooks.rs            # hooks 表操作
│       ├── global_config.rs    # 全局配置操作
│       ├── todo_hooks.rs       # per-todo 配置操作
│       └── hook_logs.rs        # 执行日志操作
│
├── handlers/
│   └── hook.rs                 # Hook API handlers  新增
│
frontend/src/
├── pages/
│   └── Hooks/                  # 新增 Hook 管理页面
│       ├── index.tsx           # Hook 列表页
│       ├── HookForm.tsx        # 新建/编辑弹窗
│       └── HookLogs.tsx       # 执行日志页
│
├── components/
│   └── HookConfigTab.tsx       # Per-Todo Hook 配置 Tab
│
└── api/
    └── hooks.ts                # Hook API 调用
```

---

## 十二、开发任务拆分

### Phase 1: 基础设施
1. 创建数据库表
2. 实现 Hook 数据模型
3. 实现 Hook 数据库操作层
4. 实现模板变量渲染引擎

### Phase 2: 核心执行
5. 实现 Hook 执行器
6. 实现过滤条件匹配
7. 在 Todo 状态变更处集成 Hook 触发点

### Phase 3: API 层
8. 实现 Hook 规则 CRUD API
9. 实现全局配置 API
10. 实现 Per-Todo Hook API
11. 实现执行日志 API

### Phase 4: 前端
12. Hook 管理页面
13. Per-Todo Hook 配置 Tab
14. Hook 日志页面
15. 系统设置中集成 Hook 配置

### Phase 5: 测试
16. 单元测试
17. 集成测试
18. E2E 测试
