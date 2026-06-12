# Session管理

> **位置**：设置 →Session管理
> **前端**：`frontend/src/components/SessionManager.tsx`
> **后端**：`backend/src/handlers/session.rs`

Session 是 ntd 把**跨执行器的会话**统一抽象出来的视图。原先每个执行器（Claude Code / Codex / Hermes / Kimi）各自有各自的 session文件，ntd把它们都收集到一个列表里方便查看。

---

## 1.Session 的含义

一个 Session 代表**一次会话**，包含：

|字段 |含义 |
|------|------|
| `session_id` | ntd内部 ID（hash of source + project + first_prompt） |
| `source` |来源：`claudecode` / `codex` / `hermes` / `kimi` / `atomcode` |
| `project_path` |关联项目目录 |
| `executor` |实际跑的执行器 |
| `model` |用的模型（`claude-3.5-sonnet` 等） |
| `first_prompt` |第一条 prompt（前200字符） |
| `last_active_at` |最后活跃时间 |
| `message_count` |消息条数 |
| `token_usage` |总 token消耗（input/output/cache） |
| `todos` |关联的 ntd todo列表（可点击跳转） |

---

## 2.Session 来源

|来源 |文件位置 |
|------|----------|
| `claudecode` | `~/.claude/projects/**/*.jsonl` |
| `codex` | `~/.codex/sessions/**/*.jsonl` |
| `hermes` | `~/.hermes/sessions/**/*.jsonl` |
| `kimi` | `~/.kimi/sessions/**/*.jsonl` |
| `atomcode` | `~/.atomcode/sessions/**/*.jsonl` |

后端 `session.rs`启动时 +定期扫描这些目录，解析 jsonl文件提取元信息。

> `codebuddy` / `opencode` / `mobilecoder` / `codewhale` **不**在扫描范围内（参见 `session.rs::scan_for_executors` 的 `match executor`）。原文档提到的 `cc-connect` 来源也已移除。

---

## 3.视图

入口：设置 → Session管理

### 3.1列表

|列 |含义 |
|----|------|
| 来源 |徽标（claudecode蓝、codex紫 ...） |
| 项目 |路径或简称（过长省略） |
| 模型 | `claude-3.5-sonnet` 等 |
|第一条 prompt |截断 |
|消息数 |总消息条数 |
| Token |总消耗（in/out） |
|最后活跃 |相对时间（3 分钟前、2 天前） |
|关联 Todo |可点击跳转 |

### 3.2筛选

-按 source（多选）
- 按 project_path
- 按时间范围
-关键字搜索 prompt内容

### 3.3详情

点列表项 →抽屉显示完整 prompt、消息流、Token详细分布。

### 3.4删除

- 单条删除：`DELETE /api/sessions/{id}`
 - 后端会尝试删除 `~/.claude/projects/*/{id}.jsonl`源文件以及对应的 `.meta` 子目录
-批量删除：勾选多条 →删除

>删除接口**会**删 Claude Code源 jsonl 文件（参见 `session.rs::delete_session`），**不是**仅删 ntd记录。

---

## 4.Session统计

>入口：Session管理 →顶部「**统计**」

`GET /api/sessions/stats` 返回：

```json
{
 "total_sessions":123,
 "active_sessions":10,
 "today_sessions":3,
 "total_input_tokens":5678901,
 "total_output_tokens":1234567,
 "by_source": { "claudecode":80, "codex":30, "kimi":13 },
 "by_executor": { "claudecode":80, "codex":30, "kimi":13 },
 "by_project": { "/Users/me/proj-a":50, "/Users/me/proj-b":30 }
}
```

>返回的实际字段是 `total_sessions` / `active_sessions` / `today_sessions` / `total_input_tokens` / `total_output_tokens` / `by_source` / `by_executor` / `by_project`，**没有** `by_model` / `recent_7_days`。

---

## 5.Session 与 Todo的关系

- ntd的 Todo跑起来时如果指定了执行器 +workspace，后端会**把执行记录关联到对应的 Session**
-这样可以从 Session反查「这次跑 ntd之前的相关对话」，反之亦然
- 不是所有 Session都有 ntd Todo关联（用户可能直接用 Claude Code没用 ntd）

---

## 6.故障排查

### 6.1Session列表为空

- 检查 `~/.claude/projects/`目录有没有 jsonl文件
-跑过至少一次 Claude Code才会有
- 后端日志搜 `session_scanner`关键字

### 6.2Token数不对

-解析逻辑：累加每条消息的 `usage.input_tokens` + `usage.output_tokens`
- 有些 jsonl文件格式不标准，解析可能为0
- 后端有兜底逻辑：解析失败时显示「?」

### 6.3关联 Todo跳不过去

- 该 Session跑时**没有** ntd介入（用户直接用 CLI）
- 显示「无关联 Todo」是正常的

---

## 7.相关 API

| Method | Path |用途 |
|--------|------|------|
| GET | `/api/sessions` |列表（支持 source/project/时间筛选） |
| GET | `/api/sessions/stats` |统计 |
| GET | `/api/sessions/{id}` |详情 |
| DELETE | `/api/sessions/{id}` |删除（同时删 Claude Code源 jsonl） |

---

## 8.概念背景

设计文档：[docs/session-management-design.md](../../../session-management-design.md)
