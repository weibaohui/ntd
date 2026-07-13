# WorkBuddy 专家系统集成设计文档

> 版本：v1.0 | 分支：feature-workbuddy-experts | 日期：2026-07-13

---

## 1. 概述

### 1.1 目标

将 WorkBuddy 的 Agent 模式引入 NTD 系统，支持**单个专家**和**专家团队**两种类型。复用 WorkBuddy 的 `plugin.json` + MD 文件格式，确保兼容性，用户可直接加载 WorkBuddy 格式的智能体定义。

### 1.2 核心需求

- 兼容 WorkBuddy 的 `plugin.json` 定义格式
- 在界面上展示两种类型：专家（agent）和专家团队（team）
- 创建/编辑 Todo 时可选定专家或专家团队
- 执行 Todo 时自动注入专家角色定义和技能信息
- 纯文件存储，方便用户直接编辑

### 1.3 非目标

- 不实现专家的在线安装/卸载（用户手动管理文件）
- 不实现专家之间的通信协调
- 不修改 WorkBuddy 的定义格式

---

## 2. 架构设计

### 2.1 整体架构

```
┌─────────────────────────────────────────────────────┐
│                    前端（React）                       │
│  ExpertPicker │ ExpertBadge │ ExpertSkillSelector    │
│  ExpertsPanel │ TodoDrawer  │ DetailHeader           │
└──────────────────────┬──────────────────────────────┘
                       │ REST API
┌──────────────────────▼──────────────────────────────┐
│                    后端（Rust）                       │
│  ┌───────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │  Handlers  │  │  Executor    │  │  Expert      │ │
│  │  /api/     │  │  Service     │  │  Module      │ │
│  │  experts   │  │  (注入prompt) │  │  (加载/索引)  │ │
│  └─────┬─────┘  └──────┬───────┘  └──────┬───────┘ │
│        │               │                  │         │
│  ┌─────▼───────────────▼──────────────────▼───────┐ │
│  │              ExpertIndexManager                 │ │
│  │         (内存索引: experts/skills/agents)        │ │
│  └─────────────────────────────────────────────────┘ │
│        │                                             │
│  ┌─────▼─────────────────────────────────────────┐  │
│  │           文件系统 (~/.ntd/experts/)           │  │
│  │    plugin.json / AGENT.md / SKILL.md          │  │
│  └────────────────────────────────────────────────┘ │
│        │                                             │
│  ┌─────▼─────────────────────────────────────────┐  │
│  │           数据库 (todos.expert_name)           │  │
│  └────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

### 2.2 存储策略：文件 + 内存索引 + 最小数据库

| 层级 | 存储位置 | 内容 | 特点 |
|------|---------|------|------|
| 文件 | `~/.ntd/experts/` | 专家定义、Agent MD、SKILL.md | 用户可直接编辑，兼容 WorkBuddy |
| 内存 | `ExpertIndexManager` | 专家/Skill/Agent 元数据索引 | 启动时构建，查询高效 |
| 数据库 | `todos.expert_name` | Todo 关联的专家名称 | 仅存引用，不存专家内容 |

---

## 3. 数据模型

### 3.1 WorkBuddy 格式兼容

#### 单个专家（agentType: "agent"）

```
~/.ntd/experts/senior-developer/
├── .codebuddy-plugin/
│   └── plugin.json        ← 入口定义
├── agents/
│   └── senior-developer.md ← Agent 角色定义
└── skills/
    ├── code-review/
    │   └── SKILL.md       ← 技能定义
    └── refactoring/
        └── SKILL.md
```

#### 专家团队（agentType: "team"）

```
~/.ntd/experts/software-company/
├── .codebuddy-plugin/
│   └── plugin.json        ← 入口定义，含 leadAgent + members
├── agents/
│   ├── ceo.md             ← 主理人（lead）
│   ├── cto.md             ← 成员
│   └── pm.md              ← 成员
└── skills/
    └── project-management/
        └── SKILL.md
```

### 3.2 plugin.json 格式

```json
{
  "name": "senior-developer",
  "version": "1.0.0",
  "agentType": "agent",  // 或 "team"
  "displayName": {
    "zh": "高级开发专家",
    "en": "Senior Developer"
  },
  "description": {
    "zh": "资深全栈开发工程师...",
    "en": "Senior full-stack developer..."
  },
  "profession": {
    "zh": "全栈开发",
    "en": "Full-stack Development"
  },
  "avatar": "avatar.png",
  "categoryId": "development",
  "agents": ["agents/senior-developer.md"],
  "skills": ["skills/code-review", "skills/refactoring"],
  "defaultInitPrompt": {
    "zh": "你是一位资深全栈开发专家...",
    "en": "You are a senior full-stack developer..."
  },
  "tags": [{"zh": "开发", "en": "Development"}],
  // 专家团队额外字段
  "leadAgent": "ceo",  // 仅 team 类型
  "members": [          // 仅 team 类型
    {
      "id": "ceo",
      "name": {"zh": "CEO", "en": "CEO"},
      "profession": {"zh": "首席执行官", "en": "Chief Executive Officer"},
      "role": "lead"
    }
  ]
}
```

### 3.3 数据库变更

**Migration V64**：`todos` 表新增 `expert_name` 列

```sql
ALTER TABLE todos ADD COLUMN expert_name TEXT;
```

- 仅存储专家名称（引用），不存专家内容
- NULL 表示未关联专家
- 专家内容由文件系统管理，通过名称从内存索引查询

---

## 4. 后端实现

### 4.1 模块结构

```
backend/src/expert/
├── mod.rs      ← 公共接口导出
├── types.rs    ← 核心类型定义（ExpertMetadata, SkillMetadata 等）
├── parser.rs   ← plugin.json / YAML frontmatter 解析
├── loader.rs   ← 目录扫描加载 + Skills 上下文构建
└── index.rs    ← ExpertIndexManager 内存索引
```

### 4.2 ExpertIndexManager

内存索引管理器，使用 `parking_lot::RwLock` 实现并发安全：

| 集合 | Key | Value | 用途 |
|------|-----|-------|------|
| `experts` | 专家名称 | ExpertMetadata | 按名称查专家 |
| `agent_files` | Agent 名称 | AgentFileMetadata | 按 Agent 名查 MD 路径 |
| `skills` | Skill 名称 | SkillMetadata | 按 Skill 名查元数据 |
| `expert_skills` | 专家名称 | Vec<Skill名称> | 查专家关联的技能 |
| `category_index` | 分类 ID | Vec<专家名称> | 按分类查专家 |

### 4.3 API 路由

| 方法 | 路径 | 功能 |
|------|------|------|
| GET | `/api/experts` | 获取所有专家列表 |
| GET | `/api/experts/{name}` | 获取单个专家详情 |
| GET | `/api/experts/{name}/agent-md` | 获取专家 Agent MD 内容 |
| GET | `/api/experts/{name}/skills` | 获取专家关联技能 |
| GET | `/api/experts/{name}/avatar` | 获取专家头像 |
| POST | `/api/experts/reload` | 重新加载专家定义 |

### 4.4 执行时 Prompt 注入

执行 Todo 时，如果关联了专家，自动将专家角色定义和技能信息拼接到 message 前面：

```
# 专家角色定义
{agent_md_content}

# 可用技能
{skill_name}: {skill_description}
...

# 任务
{original_message}
```

**关键设计决策**：
- 注入失败时静默返回原 message，不阻断执行
- 团队类型使用 `lead_agent` 的 Agent MD 内容
- 系统内部 Todo（wiki、auto_review）传 `expert_manager: None` 跳过注入

### 4.5 应用启动加载

```
main.rs:
  1. 创建 ExpertIndexManager
  2. 检查 ~/.ntd/experts/ 目录是否存在
  3. 扫描目录加载专家定义
  4. 将 expert_manager 注入 AppState
```

---

## 5. 前端实现

### 5.1 新增组件

| 组件 | 文件 | 功能 |
|------|------|------|
| `ExpertPicker` | `todo-drawer/ExpertPicker.tsx` | 专家选择器（TodoDrawer 中使用） |
| `ExpertBadge` | `ExpertBadge.tsx` | 专家标签（DetailHeader 中使用） |
| `ExpertSkillSelector` | `todo-drawer/ExpertSkillSelector.tsx` | 专家技能展示 |
| `ExpertsPanel` | `settings/ExpertsPanel.tsx` | 专家管理面板 |

### 5.2 TodoDrawer 集成

```
TodoDrawer 布局顺序：
  1. 执行器选择（ExecutorPicker）
  2. 专家/团队选择（ExpertPicker）
  3. 专家技能展示（ExpertSkillSelector，选择专家后显示）
  4. Prompt 编辑器
  5. 执行器技能选择（SkillSelector）
  6. 标签选择
  7. 工作空间选择
  8. Webhook 开关
  9. 调度器配置
  10. 验收标准
```

### 5.3 导航入口

在左侧导航栏「配置」区域添加「专家」入口：
- LeftRailKey: `settings_experts`
- 图标: TeamOutlined
- 位置: Skills 下方

### 5.4 DetailHeader 显示

在 Todo 详情头部，ExecutorBadge 后显示 ExpertBadge：
- 显示专家名称和类型标识
- 悬停显示职业和描述

---

## 6. 文件目录规范

### 6.1 专家定义目录

```
~/.ntd/experts/                    ← 专家定义根目录
├── senior-developer/              ← 单个专家
│   ├── .codebuddy-plugin/
│   │   └── plugin.json
│   ├── agents/
│   │   └── senior-developer.md
│   ├── skills/
│   │   ├── code-review/
│   │   │   └── SKILL.md
│   │   └── refactoring/
│   │       └── SKILL.md
│   └── avatar.png
├── software-company/              ← 专家团队
│   ├── .codebuddy-plugin/
│   │   └── plugin.json
│   ├── agents/
│   │   ├── ceo.md
│   │   ├── cto.md
│   │   └── pm.md
│   └── skills/
│       └── project-management/
│           └── SKILL.md
└── ...更多专家
```

### 6.2 SKILL.md 格式

```markdown
---
name: code-review
description: 代码审查技能
description_zh: 代码审查技能
description_en: Code review skill
version: 1.0.0
emoji: 🔍
allowedTools:
  - read_file
  - write_file
---

# 代码审查技能

技能的详细说明内容...
```

---

## 7. 已知限制与后续规划

### 7.1 当前限制

1. **专家目录不支持软链接**：加载器不追踪符号链接
2. **不支持专家在线安装**：用户需手动放置专家定义文件
3. **QuickCaptureModal 不支持专家选择**：快速创建入口空间有限
4. **Todo 列表项不显示专家信息**：仅详情页展示
5. **专家间无协调机制**：团队类型仅注入主理人的定义
6. **Skill 注入仅写名称**：不调整启动命令的 `--allowedTools`

### 7.2 后续规划

| 优先级 | 功能 | 描述 |
|--------|------|------|
| P1 | QuickCaptureModal 专家选择 | 在快速创建入口也支持选择专家 |
| P1 | Todo 列表项显示专家标签 | 在 TodoCard/TodoItemRow 中显示专家信息 |
| P2 | 专家在线安装 | 支持从 Git 仓库或压缩包安装专家 |
| P2 | 团队成员协调执行 | 多成员按角色分工执行 |
| P3 | 专家使用统计 | 记录专家使用频次和成功率 |
| P3 | 专家版本管理 | 支持专家定义的版本更新 |

---

## 8. 测试覆盖

### 8.1 后端单元测试

| 测试模块 | 覆盖内容 |
|---------|---------|
| `expert/parser.rs` | plugin.json 解析、YAML frontmatter 提取 |
| `expert/loader.rs` | 目录扫描加载、Skills 上下文构建 |
| `expert/index.rs` | 索引 CRUD、查询 |
| `executor_service/pre_spawn.rs` | `inject_expert_context` 注入逻辑 |

### 8.2 前端类型检查

- `npx tsc --noEmit` 零错误
- 所有新增组件通过 TypeScript 严格模式

---

## 9. 变更清单

### 后端

| 文件 | 变更 |
|------|------|
| `expert/types.rs` | 新增：核心类型定义 |
| `expert/parser.rs` | 新增：plugin.json / YAML 解析 |
| `expert/index.rs` | 新增：内存索引管理器 |
| `expert/loader.rs` | 新增：目录扫描加载 |
| `expert/mod.rs` | 新增：模块入口 |
| `handlers/experts.rs` | 新增：API 路由处理函数 |
| `db/migration/v64.rs` | 新增：todos 表 expert_name 字段 |
| `models/mod.rs` | 修改：Todo/Request 添加 expert_name |
| `db/todo.rs` | 修改：DAO 方法支持 expert_name |
| `executor_service/mod.rs` | 修改：RunTodoExecutionRequest 添加 expert_manager |
| `executor_service/pre_spawn.rs` | 修改：新增 inject_expert_context |
| `executor_service/stages.rs` | 修改：执行时注入专家上下文 |
| 所有构造 RunTodoExecutionRequest 的地方 | 修改：传入 expert_manager |

### 前端

| 文件 | 变更 |
|------|------|
| `types/expert.ts` | 新增：专家类型定义 |
| `utils/database/experts.ts` | 新增：专家 API 接口 |
| `components/todo-drawer/ExpertPicker.tsx` | 新增：专家选择器 |
| `components/ExpertBadge.tsx` | 新增：专家标签 |
| `components/todo-drawer/ExpertSkillSelector.tsx` | 新增：专家技能展示 |
| `components/settings/ExpertsPanel.tsx` | 新增：专家管理面板 |
| `types/todo.ts` | 修改：添加 expert_name 字段 |
| `utils/database/todos.ts` | 修改：API 支持 expert_name |
| `components/todo-drawer/reducer.ts` | 修改：添加 expertName 状态 |
| `components/TodoDrawer.tsx` | 修改：集成 ExpertPicker + ExpertSkillSelector |
| `components/todo-detail/DetailHeader.tsx` | 修改：显示 ExpertBadge |
| `components/shell/LeftRail.tsx` | 修改：添加专家导航入口 |
| `hooks/useViewState.ts` | 修改：添加 experts 视图 |
| `App.tsx` | 修改：注册专家面板路由 |
