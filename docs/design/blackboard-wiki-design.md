# 黑板 Wiki 化设计方案

> 分支：`feat/blackboard-wiki`
> 创建时间：2026-07-04
> 状态：设计阶段

---

## 1. 背景与动机

### 1.1 当前黑板的问题

当前黑板是一个**单文件大 Markdown**，由 LLM 全量重写维护。随着任务执行增多：

1. **信息过载** — 几十条结论堆在一个文档里，用户找不到关心的内容
2. **没有层次** — "认证模块的结论"和"数据库的结论"混在一起，无法按领域聚焦
3. **看不出演进** — 新旧结论并列，不知道某个方向最近有没有进展
4. **LLM 全量重写的代价** — 文档越长，每次更新 token 消耗越大，且容易丢失旧内容

### 1.2 参考：Karpathy 的 LLM Wiki 理念

核心思想：LLM 增量式构建和维护一个持久的知识库，而非每次从零检索。

**对 NTD 适用的部分：**
- 多页面结构（按主题拆分）
- 索引页（目录导航）
- 日志页（时间线记录）
- 增量更新（非全量重写）

**对 NTD 不适用的部分：**
- 双向链接 — Todo/Loop 执行结论之间无天然引用关系，硬造链接是牵强附会
- Lint 巡检 — 执行结论是历史事实，不太会"过时"或"矛盾"
- 查询反哺 — 当前阶段不做，但后期会通过"提问功能"引入

### 1.3 后期规划：提问功能

后期计划给黑板增加提问功能：用户基于黑板提问 → LLM 深挖更多执行历史 → 找出关键点 → 结果可保存为分析页。

这意味着黑板的来源将从单一的"自动摄入"扩展为：
- **自动摄入**：Todo/Loop 执行结论，归类到 topic 页
- **主动挖掘**：用户提问产出的分析结论，保存为 analysis 页（后期实现）

本方案在数据模型中预留 `analysis` 页类型，但当前阶段不实现提问功能。

---

## 2. 核心定位

> **黑板 Wiki 化 = 把无序的执行结论，变成按主题可导航的结构化视图。**

用户的价值体验：
- **之前**：打开黑板，看到一个长文档，滚动找关心的内容
- **之后**：打开黑板，左侧看到"认证模块(3)、性能优化(5)、数据库(2)"，点击进入只看这个领域的所有结论

---

## 3. 数据模型

### 3.1 新增表：blackboard_pages

```sql
CREATE TABLE IF NOT EXISTS blackboard_pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id INTEGER NOT NULL,
    page_type TEXT NOT NULL,        -- index / topic / log
    slug TEXT NOT NULL,             -- 页面唯一标识，LLM 生成，如 "auth-module"
    title TEXT NOT NULL,            -- 显示标题，如 "认证模块"
    content TEXT NOT NULL DEFAULT '', -- Markdown 内容
    source_refs TEXT NOT NULL DEFAULT '[]', -- JSON 数组：来源 execution_record_id 列表
    created_at TEXT,
    updated_at TEXT,
    FOREIGN KEY (workspace_id) REFERENCES project_directories(id) ON DELETE CASCADE,
    UNIQUE (workspace_id, slug)    -- 同一 workspace 内 slug 唯一
);
```

### 3.2 page_type 说明

| 类型 | 谁生成 | 谁更新 | 用途 |
|------|--------|--------|------|
| `index` | 后端 | 后端（每次页面变更后自动重新生成） | 目录页：所有主题页的 slug + title + 摘要 + 来源数 |
| `topic` | LLM | LLM（第二次调用产出内容） | 主题页：按领域归类的执行结论 |
| `log` | 后端 | 后端（每次摄入后追加） | 日志页：按时间记录每次摄入操作 |
| `analysis` | （预留） | （预留） | 后期提问功能产出的分析页 |

### 3.3 保留 blackboards 表为元信息表

`blackboards` 表保留，用途变化：
- `content` 字段：**废弃**（内容迁移到 blackboard_pages）
- `pending_record_ids`：保留（防抖队列不变）
- `blackboard_debounce_secs / count / update_prompt`：保留（配置不变）

### 3.4 Entity 定义

```rust
// backend/src/db/entity/blackboard_pages.rs
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "blackboard_pages")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub workspace_id: i64,
    /// 页面类型：index / topic / log（analysis 预留）
    #[sea_orm(column_type = "Text")]
    pub page_type: String,
    /// 页面唯一标识，同一 workspace 内唯一
    #[sea_orm(column_type = "Text")]
    pub slug: String,
    /// 显示标题
    #[sea_orm(column_type = "Text")]
    pub title: String,
    /// Markdown 内容
    #[sea_orm(column_type = "Text")]
    pub content: String,
    /// 来源 execution_record_id 列表（JSON 数组）
    #[sea_orm(column_type = "Text")]
    pub source_refs: String,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
}
```

---

## 4. 页面更新机制

### 4.1 更新责任矩阵

| 页面类型 | 谁更新内容 | 谁维护结构 | 更新触发时机 |
|---------|-----------|-----------|-------------|
| topic 主题页 | LLM（第二次调用） | LLM（第一次调用决定 create/update） | 新结论涉及该主题时 |
| index 目录页 | 后端自动生成 | 后端 | 任何页面新增/删除后 |
| log 日志页 | 后端自动生成 | 后端 | 每次摄入完成后追加 |
| analysis 分析页 | （预留） | （预留） | 后期提问功能触发 |

### 4.2 两次 LLM 调用流程

```text
触发：debounce 到期，pending_record_ids 非空

┌─ 第一次调用（分析阶段）─────────────────────────────┐
│ 输入：                                               │
│   - 新结论列表（record_ids → LLM 用 CLI 获取内容）   │
│   - 当前 index 页面（所有现有 topic 页 slug+title+摘要）│
│ LLM 任务：                                           │
│   1. 用 ntd todo execution get <id> 获取每条结论     │
│   2. 判断每条结论归入哪个主题页面（已有 or 新建）     │
│   3. 输出 JSON 操作列表                              │
│ 输出格式：                                           │
│   {                                                  │
│     "operations": [                                  │
│       {                                              │
│         "action": "create",                          │
│         "slug": "auth-module",                       │
│         "title": "认证模块",                         │
│         "summary": "关于JWT验证、token刷新...",      │
│         "record_ids": [42, 45]                       │
│       },                                             │
│       {                                              │
│         "action": "update",                          │
│         "slug": "performance",                       │
│         "title": "性能优化",                         │
│         "summary": "数据库查询优化、缓存策略...",    │
│         "record_ids": [47]                           │
│       }                                              │
│     ]                                                │
│   }                                                  │
└──────────────────────────┬──────────────────────────┘
                           │ 后端解析 JSON
┌─ 第二次调用（执行阶段）─────────────────────────────┐
│ 输入：                                               │
│   - 第一次的操作列表                                 │
│   - 待更新页面的当前完整内容（通过 CLI 获取）        │
│   - record_ids（LLM 再次用 CLI 获取结论详情）       │
│ LLM 任务：                                           │
│   1. 逐页面生成更新后的完整 Markdown                 │
│   2. 输出 JSON：slug → 新内容                        │
│ 输出格式：                                           │
│   {                                                  │
│     "auth-module": "# 认证模块\n\n## 已确认\n...",   │
│     "performance": "# 性能优化\n\n..."               │
│   }                                                  │
└──────────────────────────┬──────────────────────────┘
                           │ 后端执行写入
┌─ 后端自动操作 ──────────────────────────────────────┐
│ 1. upsert topic 页面到 blackboard_pages 表           │
│ 2. 合并 source_refs（追加新 record_ids）             │
│ 3. 重新生成 index 页面（所有 topic 页目录）          │
│ 4. 追加 log 条目（本次摄入记录）                     │
│ 5. 清空 pending 队列                                │
└──────────────────────────────────────────────────────┘
```

### 4.3 为什么分两次调用？

- **第一次轻量**：只看摘要和新结论，token 消耗少，主要做"分类决策"
- **第二次精准**：只加载需要更新的页面内容，不加载整个 wiki，token 效率高
- **解耦清晰**："改什么"和"怎么改"是两个独立的认知任务

### 4.4 index 页面结构（后端生成）

```markdown
# 工作空间知识库

## 主题页面

- [认证模块](ntd://blackboard/auth-module) — 关于JWT验证、token刷新、权限控制的结论汇总（3 条来源）
- [性能优化](ntd://blackboard/performance) — 数据库查询优化、缓存策略、接口响应时间（5 条来源）
- [数据库设计](ntd://blackboard/database) — 表结构、索引、迁移策略（2 条来源）

## 统计

- 页面总数：3
- 来源总数：10
- 最后更新：2026-07-04 15:30
```

### 4.5 log 页面结构（后端生成）

```markdown
# 更新日志

## [2026-07-04 15:30] 摄入 | 执行记录 #47

- 涉及页面：性能优化（更新）
- 新结论：数据库连接池优化方案
- 来源：execution_record_47

## [2026-07-04 14:20] 摄入 | 执行记录 #42, #45

- 涉及页面：认证模块（新建）
- 新结论：JWT 验证流程 + token 刷新机制
- 来源：execution_record_42, execution_record_45
```

---

## 5. CLI 命令设计

LLM 在两次调用中需要通过 CLI 获取数据。新增以下命令：

### 5.1 列出所有页面

```bash
ntd blackboard page list --workspace <id>
```

输出：
```json
[
  {"slug": "auth-module", "title": "认证模块", "page_type": "topic", "source_count": 3, "updated_at": "2026-07-04T15:30:00Z"},
  {"slug": "performance", "title": "性能优化", "page_type": "topic", "source_count": 5, "updated_at": "2026-07-04T14:20:00Z"}
]
```

### 5.2 获取页面内容

```bash
ntd blackboard page get <slug> --workspace <id>
```

输出页面的完整 Markdown 内容。

### 5.3 获取现有命令（复用）

```bash
# 获取执行记录结论（已有）
ntd todo execution get <id>
```

---

## 6. 前端布局

从单页滚动改为 **左侧目录 + 右侧内容** 的 Wiki 风格：

```text
┌──────────────┬──────────────────────────────────┐
│ 📑 目录       │ # 认证模块                        │
│              │                                   │
│ 📋 综合进展   │ ## 已确认                         │
│ 🔑 认证模块 ● │ - JWT token 验证逻辑已确认...     │
│ ⚡ 性能优化   │   (来源: [record_42](ntd://...))  │
│ 🗄 数据库     │                                   │
│ 📝 更新日志   │ ## 新发现                         │
│              │ - 发现 token 刷新有竞态条件...     │
│              │                                   │
└──────────────┴──────────────────────────────────┘
```

### 6.1 左侧目录

- 按 page_type 分组：主题页面 / 更新日志
- 主题页面按更新时间倒序排列
- 当前选中的页面高亮
- 点击切换右侧内容

### 6.2 右侧内容

- 渲染 Markdown（复用现有 XMarkdown 组件）
- 保留 `ntd://todo/{id}` 内部链接跳转
- 顶部显示页面标题 + 来源数 + 更新时间

### 6.3 倒计时进度条

保留现有双进度条组件，位置不变（顶部）。

---

## 7. API 设计

### 7.1 获取所有页面列表

```http
GET /api/workspaces/{workspace_id}/blackboard/pages
```

响应：
```json
{
  "data": [
    {
      "id": 1,
      "slug": "auth-module",
      "title": "认证模块",
      "page_type": "topic",
      "source_count": 3,
      "updated_at": "2026-07-04T15:30:00Z"
    }
  ]
}
```

### 7.2 获取单个页面内容

```http
GET /api/workspaces/{workspace_id}/blackboard/pages/{slug}
```

响应：
```json
{
  "data": {
    "id": 1,
    "workspace_id": 1,
    "page_type": "topic",
    "slug": "auth-module",
    "title": "认证模块",
    "content": "# 认证模块\n\n...",
    "source_refs": [42, 45, 48],
    "updated_at": "2026-07-04T15:30:00Z"
  }
}
```

### 7.3 保留现有 API

```text
GET  /api/workspaces/{workspace_id}/blackboard          → 获取元信息（配置）
GET  /api/workspaces/{workspace_id}/blackboard/config   → 获取配置
PATCH /api/workspaces/{workspace_id}/blackboard         → 更新配置
```

---

## 8. 迁移策略

### 8.1 清空重建

当前黑板内容直接清空（当前黑板为空，无损失）。从下一条任务执行结论开始用新架构积累。

### 8.2 数据库迁移

1. 创建 `blackboard_pages` 表
2. `blackboards.content` 字段保留但不再使用（向后兼容，不删字段）
3. 为每个 workspace 初始化空的 index 页面和 log 页面

---

## 9. 实现步骤

| Phase | 任务 | 说明 |
|-------|------|------|
| **Phase 1** | 数据库层 | Entity + Migration + DB 方法（blackboard_pages 表） |
| **Phase 2** | CLI 命令 | `ntd blackboard page list/get`（供 LLM 调用） |
| **Phase 3** | 后端 Service | 两次 LLM 调用流程（分析+执行）+ index/log 自动生成 |
| **Phase 4** | 后端 API | 页面列表 + 页面内容接口 |
| **Phase 5** | 前端页面 | 左侧目录 + 右侧内容 Wiki 布局 |
| **Phase 6** | Prompt 模板 | 分析阶段 + 执行阶段两份 prompt |

---

## 10. 边界情况

### 10.1 首次使用

- 新工作空间无任何页面
- 第一次任务完成后，LLM 创建首个 topic 页面 + index + log

### 10.2 页面内容过长

- 当前不做限制
- 未来可考虑：LLM 提示中要求精简，或页面分页

### 10.3 LLM 返回格式错误

- 第一次调用 JSON 解析失败 → 记录日志，跳过本次更新，不清空 pending 队列
- 第二次调用 JSON 解析失败 → 记录日志，保留旧页面内容，清空 pending 队列

### 10.4 并发更新

- 依赖 SQLite 的串行写入
- pending 队列的防抖机制天然避免并发

### 10.5 任务结论为空

- Finished.result 为空 → 不入 pending 队列（现有逻辑已处理）

---

## 11. 后期扩展（不在本次实现范围）

### 11.1 提问功能

- 用户在黑板页面提问
- LLM 基于黑板内容 + 深挖更多执行历史
- 产出的分析保存为 analysis 页面
- analysis 页面可单向引用 topic 页面

### 11.2 用户编辑

- 允许用户直接编辑 topic 页面
- LLM 后续更新时尊重用户修改

### 11.3 版本历史

- 每次页面更新前存旧版本
- 支持查看历史版本和回滚

### 11.4 搜索功能

- 搜索黑板页面内容
- 可能需要引入 qmd 或自建搜索脚本

---

## 12. 决策记录

| 决策点 | 选择 | 理由 |
|--------|------|------|
| 页面分类维度 | LLM 动态生成主题 | 最接近 Karpathy 理念，主题随知识积累自然生长 |
| 现有内容迁移 | 清空重建 | 当前黑板为空，无损失 |
| LLM 调用策略 | 两次调用（分析+执行） | 最精准，分析轻量决策，执行精准更新 |
| 双向链接 | 不做 | Todo/Loop 执行结论之间无天然引用关系 |
| Lint 巡检 | 不做 | 执行结论是历史事实，不太会过时 |
| page_type 字段类型 | String 而非枚举 | 为后期 analysis 类型预留扩展空间 |
| blackboards.content 字段 | 保留不删 | 向后兼容，避免 migration 风险 |
