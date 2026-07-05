# 黑板 Wiki 混合存储方案设计

> **版本**: v2.0 | **日期**: 2026-07-04
>
> **核心思路**: 文件存储 Markdown 内容，数据库存储元数据。LLM 直接编辑文件，无需输出 YAML/JSON。

---

## 1. 方案对比

### 现有问题

| 问题 | 原因 | 影响 |
|------|------|------|
| YAML 解析失败 | LLM 输出格式不稳定（缩进、转义、截断） | 执行阶段整段失败 |
| 必须完整替换 | 数据库 UPDATE 整体覆盖 | 无法局部修改某一段 |
| 两阶段调用复杂 | 分析→执行→解析→落库，流程长 | 每一步都可能出错 |
| 调试困难 | 查数据库不如看文件直观 | 问题排查效率低 |

### 混合方案优势

| 优势 | 说明 |
|------|------|
| 局部更新 | 执行器内置 `grep` + `edit`，只改某一段 |
| 无需结构化输出 | LLM 直接读/写 Markdown 文件，不需要 YAML |
| 单次调用 | 读取 → 分析 → 直接编辑文件，一步完成 |
| 调试直观 | `cat ~/.ntd/workspace/3/wiki/auth-module.md` |
| 版本控制友好 | 文件可用 git 管理，有历史 diff |
| 元数据保留 | `source_refs` 等关联信息存数据库 |

---

## 2. 目录结构

```
~/.ntd/workspace/<workspace_id>/wiki/
├── index.md              # 目录页（后端自动生成，只读）
├── log.md                # 执行日志（追加式）
└── topics/
    ├── auth-module.md    # 主题页（LLM 生成/编辑）
    ├── performance.md
    └── database-query.md
```

**文件职责**：

| 文件 | 类型 | 来源 | 更新方式 |
|------|------|------|----------|
| `index.md` | 目录 | 后端生成 | 扫描 topics 目录，自动更新 |
| `log.md` | 日志 | 后端追加 | 每次 wiki 更新后追加一条 |
| `topics/*.md` | 主题 | LLM 编辑 | create 新文件 / edit 现有文件 |

---

## 3. 数据库表结构（调整后）

```sql
-- blackboard_pages：只存元数据，不存 content
CREATE TABLE blackboard_pages (
    id INTEGER PRIMARY KEY,
    workspace_id INTEGER NOT NULL,
    page_type TEXT NOT NULL,        -- 'index' / 'topic' / 'log'
    slug TEXT NOT NULL,             -- 'auth-module' / 'performance'
    title TEXT NOT NULL,            -- '认证模块'
    summary TEXT,                   -- 一句话摘要
    file_path TEXT NOT NULL,        -- 相对路径 'topics/auth-module.md'
    source_refs TEXT,               -- JSON 数组 [42, 45, 47]
    updated_at TEXT,
    created_at TEXT,
    UNIQUE(workspace_id, slug)
);
```

**关键变化**：

- **删除 `content` 字段**：内容存文件，不存数据库
- **新增 `file_path` 字段**：记录文件相对路径
- **保留 `source_refs` 字段**：关联执行记录，便于查询

---

## 4. 工作流程（单次 LLM 调用）

### 4.1 触发时机

执行记录完成后，debounce 触发 wiki 更新（与现有逻辑一致）。

### 4.2 LLM 调用流程

```
LLM 单次调用：
┌─────────────────────────────────────────────────────────┐
│ 1. 列出 wiki/topics 目录下所有文件                        │
│ 2. 读取每个文件的标题、摘要（从 frontmatter 或第一行）      │
│ 3. 获取待分析的 execution record IDs                     │
│ 4. 调用 `ntd todo execution get <id>` 获取每条结论        │
│ 5. 决定：创建新文件 / 编辑现有文件                         │
│ 6. 直接执行文件操作：                                      │
│    - create: 写入新文件 topics/<slug>.md                  │
│    - update: edit 现有文件，局部修改                       │
│ 7. 后端扫