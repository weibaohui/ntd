# 大黑板系统设计文档

## 概述

大黑板（Blackboard）是一个跨任务、跨 Loop 的信息共享与知识管理系统。它在现有 Loop 小黑板机制之上，提供一个工作空间级别的大黑板，自动采集、关联、归纳多个任务的执行结论，并基于分析结果生成行动建议。

### 背景与动机

**已有能力**：Loop 系统内置了"小黑板"机制：
- 每次 Loop 执行中，每个环节执行完毕后提取结论，写入 `loop_step_executions.conclusion`
- 下一环节的 Prompt 通过 `{{blackboard}}` 变量获得前面所有环节的结论
- 实现了**同一个 Loop 内部**的信息共享

**缺失的能力**：
- 小黑板是 Loop 级别、临时的（跟随一次 loop_execution 生命周期）
- 多个 Loop 之间、Loop 和独立 Todo 之间没有信息共享
- 没有跨任务的结论聚合和关联发现
- 无法从多次执行的碎片信息中提炼出高层次的知识

**核心价值**：从"执行了什么"转变为"知道了什么" —— 系统帮你执行任务，大黑板帮你从执行结果中提炼知识、发现关联、形成决策依据。

### 设计决策

| 决策项 | 方案 | 说明 |
|--------|------|------|
| 黑板范围 | 按工作空间隔离 | 每个 workspace 一张独立大黑板 |
| 智能程度 | AI 语义分析 | 用 LLM 做关联、聚类、归纳 |
| 触发方式 | 事件驱动采集 + 定时深度分析 | 实时采集结论，定时做语义分析 |
| 规划板与 Todo 关系 | 直接创建 Todo | 规划板分析结果一键转为 Todo |
| 智能体形态 | 后台自动运行 | tokio task 常驻，自动采分归整 |

### 典型场景

1. **模块脆弱性发现**：3 个不同 Loop 修复了同一个模块的 bug → 大黑板自动识别出"该模块近期脆弱"，生成关注建议
2. **代码质量问题归纳**：执行了多个代码评审 Loop → 大黑板归纳出"项目代码质量问题集中在错误处理"
3. **关联任务合并**：同一工作空间下有多个 Todo 结果相关 → 大黑板自动串联，提示合并处理
4. **长期知识沉淀**：经过多轮整理，每个快照记录了不同阶段的分析结论，形成项目知识图谱

---

## 架构总览

```
                    ┌──────────────────────────┐
                    │   监控智能体 (Background)   │
                    │  ┌──────────────────────┐ │
                    │  │ Collector 采集器      │ │
                    │  │ - 监听执行完成事件     │ │
                    │  │ - 提取结论写入黑板     │ │
                    │  │ - 计算语义向量        │ │
                    │  ├──────────────────────┤ │
                    │  │ Analyzer 分析器       │ │
                    │  │ - 语义相似度计算       │ │
                    │  │ - 聚类形成主题        │ │
                    │  │ - LLM 归纳总结        │ │
                    │  ├──────────────────────┤ │
                    │  │ Planner 规划器        │ │
                    │  │ - 分析主题生成建议     │ │
                    │  │ - 写入规划板条目       │ │
                    │  ├──────────────────────┤ │
                    │  │ GC 清理器             │ │
                    │  │ - 定时检查条目数量     │ │
                    │  │ - 生成快照并擦除旧条目 │ │
                    │  └──────────────────────┘ │
                    └──────────┬───────────────┘
                               │
  ┌────────────────────────────┼────────────────────────────┐
  │                            ▼                            │
  │   ┌──────────┐    ┌──────────────┐    ┌──────────┐     │
  │   │ 现有系统  │───▶│  大黑板        │───▶│  规划板   │     │
  │   │          │    │ (Blackboard)  │    │(Planning)│     │
  │   │ Loop执行  │    │ ┌──────────┐ │    │          │     │
  │   │ Todo执行  │    │ │ 条目存储  │ │    │ 建议任务  │     │
  │   │ 手动笔记  │    │ │ 关联图谱  │ │    │ 一键转Todo│     │
  │   │          │    │ │ 主题聚类  │ │    │          │     │
  │   └──────────┘    │ │ 快照版本  │ │    └────┬─────┘     │
  │                   │ └──────────┘ │         │           │
  │                   └──────────────┘         ▼           │
  │                                      ┌──────────┐     │
  │                                      │ Todo系统  │     │
  │                                      │ 创建/执行  │     │
  │                                      │ 结论回写   │     │
  │                                      └──────────┘     │
  └────────────────────────────────────────────────────────┘
```

**数据流闭环**：
```
执行完成 → 结论采集到黑板 → AI分析关联归纳 → 生成建议 → 创建Todo → Todo执行 → 结论再回黑板
```

---

## 数据模型

### ER 概览

```
workspace (1) ────< blackboard_entries (N)
workspace (1) ────< blackboard_topics (N)
workspace (1) ────< blackboard_snapshots (N)
workspace (1) ────< planning_items (N)

blackboard_entry ───> blackboard_topic (FK: topic_id, nullable)
blackboard_entry ───< blackboard_relations (N:N 关联)
```

### blackboard_entries（黑板条目）

黑板的核心数据载体，每一条都是一个独立的"知识碎片"。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | i64 PK | 主键 |
| `workspace_id` | i64 FK | 所属工作空间 |
| `source_type` | String | 来源类型：<br>`execution_record` — 独立 Todo 执行结论<br>`loop_conclusion` — Loop 环节结论<br>`manual` — 用户手动添加<br>`agent_observation` — 智能体观察 |
| `source_id` | Option\<i64\> | 来源记录的 ID（execution_record_id / loop_step_execution_id） |
| `content` | String | 原始内容（结论全文） |
| `summary` | String | AI 生成的摘要（便于快速浏览和聚类） |
| `embedding` | Option\<Vec\<u8\>\> | 语义向量（BLOB 格式，用于相似度计算） |
| `topic_id` | Option\<i64\> FK | 所属主题（nullable，未归类时为 null） |
| `importance_score` | i32 | 重要度评分（0-100，由 AI 判定） |
| `is_active` | bool | 是否活跃（快照整理后标记为 false） |
| `created_at` | DateTime | 创建时间 |

### blackboard_topics（主题）

由 Analyzer 自动聚类形成的主题，聚合了相关条目。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | i64 PK | 主键 |
| `workspace_id` | i64 FK | 所属工作空间 |
| `title` | String | 主题标题 |
| `summary` | String | 一句话描述 |
| `detail` | String | AI 生成的详细分析报告 |
| `entry_count` | i32 | 关联条目数量 |
| `created_at` | DateTime | 创建时间 |
| `updated_at` | DateTime | 最后更新时间 |

### blackboard_relations（条目关联）

记录条目之间的关联关系，构成知识图谱。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | i64 PK | 主键 |
| `entry_id_a` | i64 FK | 关联条目 A |
| `entry_id_b` | i64 FK | 关联条目 B |
| `relation_type` | String | 关联类型：<br>`semantic` — 语义相似<br>`tag` — 共享标签<br>`file` — 涉及相同文件<br>`workspace` — 同工作空间 |
| `strength` | f64 | 关联强度（0.0-1.0） |
| `auto_generated` | bool | 是否自动生成 |
| `created_at` | DateTime | 创建时间 |

### blackboard_snapshots（快照）

整理黑板时生成的版本记录，保存当时的所有条目和主题状态。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | i64 PK | 主键 |
| `workspace_id` | i64 FK | 所属工作空间 |
| `snapshot_data` | String | JSON 格式的快照数据（当时的条目和主题） |
| `entry_count` | i32 | 快照时的条目数 |
| `topic_count` | i32 | 快照时的主题数 |
| `summary` | String | 快照摘要 |
| `trigger` | String | 触发方式：`manual` / `auto_cleanup` / `cron` |
| `created_at` | DateTime | 创建时间 |

### planning_items（规划板条目）

智能体分析后生成的行动建议或任务。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | i64 PK | 主键 |
| `workspace_id` | i64 FK | 所属工作空间 |
| `type` | String | 类型：`suggestion` / `task` |
| `title` | String | 标题 |
| `description` | String | 详细描述（含分析理由） |
| `related_topic_id` | Option\<i64\> FK | 相关主题 |
| `source_entry_ids` | String | JSON 数组，分析依据的条目 ID 列表 |
| `assigned_todo_id` | Option\<i64\> FK | 转为 Todo 后的 ID |
| `status` | String | 状态：`pending` / `created` / `done` / `dismissed` |
| `created_at` | DateTime | 创建时间 |

---

## API 设计

### 黑板条目

| 方法 | 路径 | 功能 |
|------|------|------|
| `GET` | `/api/workspaces/{ws_id}/blackboard/entries` | 条目列表（支持 topic_id / source_type / is_active 筛选 + 分页） |
| `POST` | `/api/workspaces/{ws_id}/blackboard/entries` | 手动添加条目 |
| `DELETE` | `/api/workspaces/{ws_id}/blackboard/entries/{id}` | 删除条目 |

### 黑板主题

| 方法 | 路径 | 功能 |
|------|------|------|
| `GET` | `/api/workspaces/{ws_id}/blackboard/topics` | 主题列表 |
| `GET` | `/api/workspaces/{ws_id}/blackboard/topics/{id}` | 主题详情（含关联的条目列表） |

### 智能体操作

| 方法 | 路径 | 功能 |
|------|------|------|
| `POST` | `/api/workspaces/{ws_id}/blackboard/analyze` | 手动触发 AI 分析（聚类归纳） |
| `POST` | `/api/workspaces/{ws_id}/blackboard/cleanup` | 手动触发整理（生成快照 + 擦除旧条目） |

### 快照

| 方法 | 路径 | 功能 |
|------|------|------|
| `GET` | `/api/workspaces/{ws_id}/blackboard/snapshots` | 快照列表 |
| `GET` | `/api/workspaces/{ws_id}/blackboard/snapshots/{id}` | 快照详情 |

### 规划板

| 方法 | 路径 | 功能 |
|------|------|------|
| `GET` | `/api/workspaces/{ws_id}/planning/items` | 规划条目列表 |
| `POST` | `/api/workspaces/{ws_id}/planning/items` | 手动创建规划条目 |
| `POST` | `/api/workspaces/{ws_id}/planning/items/{id}/convert` | 转为 Todo（创建 Todo 并标记状态） |
| `PUT` | `/api/workspaces/{ws_id}/planning/items/{id}` | 更新条目状态 |

### 智能体状态

| 方法 | 路径 | 功能 |
|------|------|------|
| `GET` | `/api/workspaces/{ws_id}/blackboard/agent-status` | 查询智能体运行状态 |

---

## 前端设计

### 菜单变更

在 `LeftRail.tsx` 的"工作区"分区中，环路下方新增"黑板"菜单项：

```
工作区
├── 事项          (items)
├── 环路          (loops)
├── 黑板          (blackboard)    ← 新增
├── 仪表盘        (dashboard)
└── 看板          (memorial)
```

### 路由变更

| URL | 视图 |
|-----|------|
| `/?view=blackboard` | 黑板主页面（默认显示黑板视图） |
| `/?view=blackboard&tab=board` | 黑板视图 |
| `/?view=blackboard&tab=planning` | 规划板视图 |
| `/?view=blackboard&tab=snapshots` | 快照列表视图 |
| `/?view=blackboard&snapshot={id}` | 查看某个快照详情 |

### 页面布局

```
┌─────────────────────────────────────────────────────────┐
│  大黑板 - [工作空间名称]              [触发整理] [查看快照] │
├───────────────────────────────┬─────────────────────────┤
│                               │                         │
│   主题视图 / 时间线视图 Tab     │   规划板 Tab             │
│                               │                         │
│   ┌── [主题1] ──────────┐    │   ┌ 待处理 ──────────┐  │
│   │ 条目1 摘要          │    │   │ [建议1] → 创建Todo │  │
│   │ 条目2 摘要          │    │   │ [建议2] → 创建Todo │  │
│   │ [关联: 3个条目]     │    │   └──────────────────┘  │
│   └─────────────────────┘    │   ┌ 已创建 ──────────┐  │
│   ┌── [主题2] ──────────┐    │   │ Todo#15: 执行中   │  │
│   │ 条目3 摘要          │    │   │ Todo#18: 等待中   │  │
│   │ 条目4 摘要          │    │   └──────────────────┘  │
│   └─────────────────────┘    │   ┌ 已完成 ──────────┐  │
│   ┌── 未归类条目 ───────┐    │   │ Todo#12: 结论已回写 │  │
│   │ 条目5 摘要          │    │   └──────────────────┘  │
│   │ 条目6 摘要          │    │                         │
│   └─────────────────────┘    │                         │
│                               │                         │
│   [+ 手动添加条目]            │   [+ 手动创建规划条目]    │
└───────────────────────────────┴─────────────────────────┘
```

### 交互流程

1. **浏览**：用户打开黑板页面，看到按主题聚类或按时间排序的条目列表
2. **手动添加**：用户可以手动添加笔记型条目到黑板上
3. **查看主题**：点击某个主题展开，查看该主题下的所有条目和关联关系
4. **触发分析**：点击"触发整理"调用 Analyzer + Planner，完成后看到更新的主题和建议
5. **转为 Todo**：在规划板中找到有意义的建议，点击"创建 Todo"，弹出 Todo 创建表单
6. **查看快照**：在快照视图中查看历史分析版本
7. **跟踪闭环**：规划板中显示了 Todo 的执行状态，执行完成后结论自动回到黑板

### 组件树

```
BlackboardPage.tsx
├── BlackboardTabs (主题/规划/快照 三个 Tab)
│   ├── BlackboardTopicView
│   │   ├── TopicCard（每个主题一个卡片）
│   │   │   ├── TopicHeader（标题 + 摘要 + 条目计数）
│   │   │   └── EntryList
│   │   │       └── EntryItem（每条一个可展开的行）
│   │   ├── UncategorizedSection（未归类条目）
│   │   └── AddEntryButton（手动添加条目）
│   ├── PlanningBoardView
│   │   ├── PlanningColumn（待处理列）
│   │   ├── PlanningColumn（已创建列）
│   │   └── PlanningColumn（已完成列）
│   └── SnapshotListView
│       └── SnapshotCard（每个快照一个卡片）
└── BlackboardToolbar（顶部操作栏）
    ├── WorkspaceSelector（工作空间选择器）
    ├── TriggerAnalyzeButton
    ├── TriggerCleanupButton
    └── ViewSnapshotButton
```

---

## 监控智能体设计

智能体是一个后台 tokio task，在 ntd 启动时 spawn，运行在独立的任务循环中。

### 整体结构

```
┌─────────────────────────────────────────────────┐
│           BlackboardAgent (tokio task)            │
│                                                   │
│  启动时注册 ExecEvent 监听器                       │
│                                                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌───┐ │
│  │ Collector │  │ Analyzer │  │ Planner  │  │GC │ │
│  │          │  │          │  │          │  │   │ │
│  │ 事件驱动  │  │ 每30分钟  │  │ 每60分钟  │  │每天 │ │
│  │ 实时采集  │  │ 语义聚类  │  │ 生成建议  │  │清理 │ │
│  └──────────┘  └──────────┘  └──────────┘  └───┘ │
│                                                   │
│  所有子模块共享 agent 状态和 DB 连接               │
└─────────────────────────────────────────────────┘
```

### Collector（采集器）

**触发方式**：监听 `ExecEvent::ExecutionCompleted` 事件

**处理流程**：
1. 收到事件 → 读取 execution_record 获取完整输出
2. 提取结论（复用 Loop 系统的 `extract_conclusion()` 逻辑）
3. 去重检查（同一 source_id 不重复采集）
4. 调用 AI executor 生成摘要 → 写入 `summary`
5. 调用 AI executor 计算语义向量 → 写入 `embedding`
6. 创建 `blackboard_entry` 记录
7. 若该条目所在 workspace 已有 Agent 在运行，通知其有新条目到达

### Analyzer（分析器）

**触发方式**：每 30 分钟（可配置）定时触发，且满足"新未分析条目 > 阈值（默认 5 条）"

**处理流程**：
1. 查询 `is_active=true AND topic_id IS NULL` 的条目
2. 与已有主题的条目进行语义向量相似度计算
3. 高相似度条目 → 归入已有主题，建立 `blackboard_relation`
4. 剩余条目 → 尝试聚类形成新主题
5. 调用 LLM 为每个更新/新建的主题生成 `title`、`summary`、`detail`
6. 更新条目重要度评分

**LLM Prompt 设计思路**：
```
你是项目知识分析助手。以下是一个工作空间中 [新条目] 和 [已有主题] 的结论摘要。
请分析：
1. 每个新条目应该归入哪个已有主题（或需要创建新主题）
2. 对每个主题，生成一句话标题、摘要、详细分析
3. 评估每个条目对项目的重要程度（1-100）
```

### Planner（规划器）

**触发方式**：每 60 分钟（可配置）定时触发，在 Analyzer 之后执行

**处理流程**：
1. 遍历所有活跃主题，调用 LLM 分析是否需要行动
2. 对于需要行动的主题，生成一条 `planning_item`
3. title 为建议标题，description 包含分析理由和关联条目
4. 去重检查（相同或高度相似的建议不重复生成）

**LLM Prompt 设计思路**：
```
以下是项目黑板上某个主题的分析报告。请判断：
1. 该主题是否需要采取行动（创建 Todo）？
2. 如有必要，建议创建什么样的 Todo？（标题、描述、优先级）
3. 不需要行动的主题，简单说明原因
```

### GC（整理器）

**触发方式**：每天自动触发一次，或手动触发

**清理策略**：
1. 当活跃条目 > `max_active_entries`（默认 200）时触发清理
2. 生成一条 `blackboard_snapshot` 记录（包含当前所有活跃条目和主题的 JSON）
3. 将旧条目标记为 `is_active=false`
4. 保留最近 N 条条目为活跃状态
5. 清理无关联条目的旧主题

**保留规则**：
- 保留最近创建的 100 条条目
- 保留 importance_score > 70 的高价值条目
- 其余标记为 inactive

---

## 存量系统集成点

### 1. Event 系统集成

智能体需要监听的事件：
- `ExecEvent::ExecutionCompleted { record_id, todo_id, ... }` — Todo 执行完成
- `ExecEvent::LoopStepCompleted { ... }` — Loop 环节执行完成
- `ExecEvent::LoopFinished { ... }` — Loop 整体执行完成

这些事件通过现有的 `EventBus` (broadcast channel) 分发。

### 2. Todo 系统集成

规划板的 `POST /convert` 接口内部需要：
1. 调用现有 `TodoService::create_todo()` 创建 Todo
2. 同时复制必要的上下文信息（工作空间、标签等）
3. 更新 `planning_item.assigned_todo_id` 和 `status`

### 3. AI Executor 集成

黑板的 AI 分析（嵌入式计算、摘要生成、语义分析）需要复用现有的 executor 系统：
- 使用 workspace 中配置的默认 AI executor
- 若未配置 AI executor，退化为基础规则匹配模式

---

## 后端模块组织

新增文件：

```
backend/src/
├── handlers/
│   └── blackboard.rs          # 黑板 API 处理器
├── models/
│   └── blackboard.rs          # 黑板数据模型（DTO）
├── db/
│   ├── entity/
│   │   ├── blackboard_entries.rs
│   │   ├── blackboard_topics.rs
│   │   ├── blackboard_relations.rs
│   │   ├── blackboard_snapshots.rs
│   │   └── planning_items.rs
│   └── blackboard.rs          # 黑板业务方法
└── services/
    └── blackboard_agent.rs    # 监控智能体
```

---

## 实施阶段

| 阶段 | 内容 | 优先级 | 预估涉及 |
|------|------|--------|----------|
| **Phase 1** | 数据模型定义 + 数据库迁移 + API 基础框架 | P0 | 5 张新表 + 基础 CRUD API |
| **Phase 2** | 前端黑板页面（条目展示 + 主题视图 + 规划板布局） | P0 | 新增 BlackboardPage + 路由 + 菜单 |
| **Phase 3** | Collector 实现（事件驱动自动采集结论 + 语义向量） | P1 | 集成 ExecEvent + AI 调用 |
| **Phase 4** | Analyzer + Planner 实现（AI 语义分析 + 建议生成） | P1 | LLM Prompt 工程 + 聚类算法 |
| **Phase 5** | GC 整理器 + 快照系统 | P2 | 快照生成 + 清理策略 |
| **Phase 6** | 集成测试 + 端到端验证 + 文档完善 | P2 | Playwright 测试 + 使用文档 |

---

## 与其他系统的区别

| 系统 | 层级 | 生命周期 | 信息范围 |
|------|------|---------|---------|
| Loop 小黑板 | Loop 内部 | 跟随一次 loop_execution | 单次执行的所有环节 |
| 执行记录 (execution_records) | Todo 级别 | 持久化 | 单次 Todo 执行的完整日志 |
| **大黑板** | 工作空间级别 | 持久化（有快照机制） | 跨 Loop、跨 Todo 的结论聚合 |
| 仪表盘 (dashboard) | 全局 | 实时统计 | 数量统计、趋势图表 |

---

## 关键设计原则

1. **YAGNI 优先**：不预先实现"可能用到"的分析维度，只做确定的语义聚类和主题归纳
2. **可降级**：无 AI executor 时降级为规则匹配（标签、文件路径、关键词），保证基础可用
3. **按工作空间隔离**：所有数据通过 workspace_id 分区，不跨空间混淆
4. **事件驱动解耦**：采集通过事件总线和智能体解耦，不阻塞执行主流程
5. **版本化快照**：每次整理都是可回溯的版本，不会丢失历史信息
