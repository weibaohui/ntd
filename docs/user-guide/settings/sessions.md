# Session 管理

> **位置**：设置 → Session 管理
> **前端**：`frontend/src/components/SessionManager.tsx`
> **后端**：`backend/src/handlers/session.rs`

Session 是 ntd 把**跨执行器的会话**统一抽象出来的视图。原先每个执行器（Claude Code / Codex / Hermes / Kimi）各自有各自的 session 文件，ntd 把它们都收集到一个列表里方便查看。

---

## 1. Session 的含义

一个 Session 代表**一次会话**，包含：

| 字段 | 含义 |
|------|------|
| `id` | ntd 内部 ID（hash of source + project + first_prompt） |
| `source` | 来源：`claude-code` / `codex` / `hermes` / `kimi` / `atomcode` / `cc-connect` |
| `project_path` | 关联项目目录 |
| `executor` | 实际跑的执行器 |
| `model` | 用的模型（`claude-3.5-sonnet` 等） |
| `first_prompt` | 第一条 prompt（前 200 字符） |
| `last_active_at` | 最后活跃时间 |
| `message_count` | 消息条数 |
| `token_usage` | 总 token 消耗（input/output/cache） |
| `todos` | 关联的 ntd todo 列表（可点击跳转） |

---

## 2. Session 来源

| 来源 | 文件位置 |
|------|----------|
| `claude-code` | `~/.claude/projects/**/*.jsonl` |
| `codex` | `~/.codex/sessions/**/*.jsonl` |
| `hermes` | `~/.hermes/sessions/**/*.jsonl` |
| `kimi` | `~/.kimi/sessions/**/*.jsonl` |
| `atomcode` | `~/.atomcode/sessions/**/*.jsonl` |
| `cc-connect` | 来自 cc-connect 桥接的消息 |

后端 `session.rs` 启动时 + 定期扫描这些目录，解析 jsonl 文件提取元信息。

---

## 3. 视图

入口：设置 → Session 管理

### 3.1 列表

| 列 | 含义 |
|----|------|
| 来源 | 徽标（claude-code 蓝、codex 紫 ...） |
| 项目 | 路径或简称（过长省略） |
| 模型 | `claude-3.5-sonnet` 等 |
| 第一条 prompt | 截断 |
| 消息数 | 总消息条数 |
| Token | 总消耗（in/out） |
| 最后活跃 | 相对时间（3 分钟前、2 天前） |
| 关联 Todo | 可点击跳转 |

### 3.2 筛选

- 按 source（多选）
- 按 project_path
- 按时间范围
- 关键字搜索 prompt 内容

### 3.3 详情

点列表项 → 抽屉显示完整 prompt、消息流、Token 详细分布。

### 3.4 删除

- 单条删除：从 ntd 数据库里删记录（**不删源 jsonl 文件**）
- 批量删除：勾选多条 → 删除

---

## 4. Session 统计

> 入口：Session 管理 → 顶部「**统计**」

`GET /api/sessions/stats` 返回：

```json
{
  "total_sessions": 123,
  "total_tokens": 5678901,
  "by_source": {
    "claude-code": 80,
    "codex": 30,
    "kimi": 13
  },
  "by_model": {
    "claude-3.5-sonnet": 70,
    "gpt-4o": 30
  },
  "recent_7_days": [12, 15, 8, 20, 25, 18, 22]
}
```

---

## 5. Session 与 Todo 的关系

- ntd 的 Todo 跑起来时如果指定了执行器 + workspace，后端会**把执行记录关联到对应的 Session**
- 这样可以从 Session 反查「这次跑 ntd 之前的相关对话」，反之亦然
- 不是所有 Session 都有 ntd Todo 关联（用户可能直接用 Claude Code 没用 ntd）

---

## 6. 故障排查

### 6.1 Session 列表为空

- 检查 `~/.claude/projects/` 目录有没有 jsonl 文件
- 跑过至少一次 Claude Code 才会有
- 后端日志搜 `session_scanner` 关键字

### 6.2 Token 数不对

- 解析逻辑：累加每条消息的 `usage.input_tokens` + `usage.output_tokens`
- 有些 jsonl 文件格式不标准，解析可能为 0
- 后端有兜底逻辑：解析失败时显示「?」

### 6.3 关联 Todo 跳不过去

- 该 Session 跑时**没有** ntd 介入（用户直接用 CLI）
- 显示「无关联 Todo」是正常的

---

## 7. 相关 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/sessions` | 列表（支持 source/project/时间筛选） |
| GET | `/api/sessions/stats` | 统计 |
| GET | `/api/sessions/{id}` | 详情 |
| DELETE | `/api/sessions/{id}` | 删除（ntd 记录，不删源文件） |

---

## 8. 概念背景

设计文档：[docs/session-management-design.md](../../../session-management-design.md)
