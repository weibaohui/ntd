# 056-Todo 列表服务端分页化与质量修复-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| pi (AI) | 2026-08-01 | 初始版本 |

> 对应需求：`docs/requirements/056-Todo列表服务端分页化与质量修复-需求.md`
> 决策已确认：看板用 brief 全量（1a）、全局桶删除（2a）、旧接口直接强制分页（3b）、单 PR（4a）。

## 1. 总体思路

```
调用点分类治理：
  A 类(列表展示)  → todo-center 真分页（SQL CASE WHEN 分桶 + LIMIT/OFFSET）
  B 类(id 集合)   → SQL 子查询下推（IN (SELECT id FROM todos WHERE workspace_id=?)）
  C 类(计数)      → 新增 count 接口
  D 类(摘要/单条) → 新增 brief 接口 / 已有 by-id 接口
  E 类(真全量)    → id 游标分批（sync）或 LIMIT（feishu 卡片）
  F 类(全局缓存)  → 删除，改按需 by-id
```

## 2. R1 执行记录过滤下推 SQL

### 2.1 db 层（`db/execution.rs`）

`ExecutionRecordQuery` 增加字段：

```rust
pub struct ExecutionRecordQuery<'a> {
    pub todo_id: Option<i64>,
    pub step_id: Option<i64>,
    pub workspace_id: Option<i64>,   // 新增：SQL 子查询过滤
    pub limit: i64,
    pub offset: i64,
    pub status: Option<&'a str>,
    pub hours: Option<u32>,
}
```

SeaORM 条件（数据查询与 COUNT 共用同一 `Condition`）：

```rust
Condition::all().add(
    execution_records::Column::TodoId.in_subquery(
        Query::select()
            .column(todos::Column::Id)
            .from(todos::Entity)
            .and_where(todos::Column::WorkspaceId.eq(wid))
            .and_where(todos::Column::DeletedAt.is_null())
            .to_owned()
    )
)
```

- `get_running_execution_records` / `get_execution_records_by_session` 增加 `workspace_id: Option<i64>` 参数，同样子查询过滤。
- 注意 `(todo_id, step_id)` 组合分支：base_filter 含 `TodoId.is_not_null()` 兜底，workspace 条件用 AND 叠加，语义为"本 ws 的 todo 产生的记录"。

### 2.2 handler 层（`handlers/execution.rs`）

删除 6 处 `get_todos_by_workspace_id` + HashSet 内存过滤（行 68、190、363、415、838、876 附近），改为把 `ws_id` 传入 query。`total` 自然与过滤后一致，分页错乱 bug 消除。

## 3. R2 todo-center 服务端分页

### 3.1 bucket 的 SQL 表达式

推导规则（`models/mod.rs:207`，优先级：archived > loop > time > event > manual）翻译为：

```sql
CASE
  WHEN t.archived_at IS NOT NULL THEN 'archived'
  WHEN (SELECT COUNT(*) FROM loop_steps ls
        WHERE ls.todo_id = t.id AND ls.enabled = 1) > 0 THEN 'loop_driven'
  WHEN t.scheduler_config IS NOT NULL THEN 'time_driven'
  WHEN t.webhook_enabled = 1 THEN 'event_driven'
  ELSE 'manual'
END
```

依据：`count_enabled_loop_steps_by_todos`（db/loop_.rs:608）确认 loop 引用只查 `loop_steps.enabled = 1`，不 join loops 表，子查询可行。

### 3.2 查询策略：先分页后聚合

```
第 1 步：SELECT t.id, <bucket_expr> AS bucket FROM todos t
         WHERE deleted_at IS NULL [AND workspace_id=?] [AND bucket=?] [AND (LOWER(title) LIKE ? OR LOWER(prompt) LIKE ?)]
         ORDER BY <sort> LIMIT n OFFSET m
         → 当页 ids（≤ page_size 个）
第 2 步：COUNT(*) 同条件 → total；GROUP BY bucket_expr 同条件 → 各 bucket counts
第 3 步：build_center_aggregates(&ids) 现有批量聚合（仅当页 ≤ page_size 条，成本可控）
第 4 步：build_center_item 组装（复用现有纯函数）
```

- 排序白名单：`id`（默认 DESC）、`updated_at`、`title`、`status`、`computed_bucket`（bucket_expr 可排序）。拒绝白名单外字段防注入。
- `search`：`LOWER(title) LIKE '%kw%' ESCAPE '\'` 参数绑定，kw 先转义 `%`/`_`。
- 响应：`TodoCenterPage { items: Vec<TodoCenterItem>, total: i64, page: i64, page_size: i64, bucket_counts: HashMap<String, i64> }`。
- `page_size` 上限 200，超出截断；`page` 从 1 开始。
- 兼容：不传 `page` 时保持旧行为（全量返回，字段同构）——**仅限内部迁移期**，前端切完后随 R4 移除。决策 3b 仅针对 `/todos` 旧接口直接强制分页；todo-center 是 054 新接口，前端只有两处调用，直接切。

### 3.3 新轻量接口

| 接口 | 响应 | 用途 |
|------|------|------|
| `GET /workspaces/{ws}/todos/ids` | `Vec<i64>` | B 类残留（如前端需要全 id 集） |
| `GET /workspaces/{ws}/todos/count` | `{ count: i64 }` | useConceptCounts |
| `GET /workspaces/{ws}/todos/brief?ids=1,2,3` | `Vec<TodoBrief>` | D 类下拉/摘要；ids 省略=全 ws（看板） |

`TodoBrief { id, title, status, executor, updated_at, archived_at }` —— 不含 prompt/acceptance_criteria 大文本。

### 3.4 旧 `/todos` 强制分页（决策 3b）

- 响应从 `Vec<Todo>` 改为 `TodoListPage { items, total, page, page_size }`（ApiResponse 包裹）。
- `hours` 过滤从内存改 SQL（`updated_at >= datetime('now', '-N hours')`，N 为 u32 绑参）。
- 默认 `page=1, page_size=50`，上限 200。
- CLI（`cli/commands.rs` list）同步适配新响应结构。

## 4. R4 收尾

- `handlers/sync.rs`：`merge_cloud_todos_to_local` 只需 `title→id` 映射，新增 db 方法 `get_todo_title_id_map()`（`SELECT id, title FROM todos WHERE deleted_at IS NULL`，两列轻量）；上传侧 `local_todos_to_cloud` 需全字段，改 id 游标分批（`WHERE id > ? ORDER BY id LIMIT 500` 循环）。
- `feishu_listener.rs:1853`：卡片 todo 列表改 brief + `ORDER BY updated_at DESC LIMIT 20`。
- 删除 `get_todos()` / `get_todos_by_workspace_id()`；测试改用新接口构造断言。

## 5. R5–R10 质量修复设计

### R5 L3（auto_update.rs）

```rust
fn resolve_local(dt: NaiveDateTime) -> DateTime<Local> {
    match dt.and_local_timezone(Local) {
        LocalResult::Single(t) => t,
        LocalResult::Ambiguous(earlier, _) => earlier, // 秋令时取较早，避免跳过检查
        LocalResult::None => dt.and_local_timezone(Local).single()
            .unwrap_or_else(|| Local::now()),          // 春令时顺延策略见实现注释
    }
}
```

实际实现：None 分支用 `dt + Duration::hours(1)` 重试 single，兜底 `Local::now()`。附 3 个单测（正常/歧义/不存在）。

### R6 L4

| 点位 | 现状 | 修法 |
|------|------|------|
| handlers/mod.rs:174 | WS 同步 records 查询失败→空 | `?` 传播前 log，WS 升级失败返回错误 |
| handlers/mod.rs:180 | logs 查询失败→空 | `tracing::error!` 后降级（WS 已升级，允许部分数据） |
| tasks.rs:237 | artifact workspace 解析→空串 | `?` 传播 |

逐个复核 skills.rs/backup.rs/process.rs/session.rs/agent_bot.rs 的其余点位，仅改"DB 故障与空数据无法区分"的，保留合法默认值。

### R7 L5（guid.rs + user_dir.rs）

- `insert_guid_after_name` 返回类型改 `Result<Option<String>, GuidInsertError>` 不准确；保持 `Option` 但新增 `insert_guid_serde_fallback(content, guid) -> Option<String>`：serde 解析 → 置 guid → 重序列化。文本路径失败时调用 fallback。
- `user_dir.rs` 维护 `static WARNED_FILES: OnceLock<Mutex<HashSet<PathBuf>>>`，每文件只 warn 一次。
- 单测：flow-style `process: {name: x}`、缺 name 行、正常 block 三种用例。

### R8 L6（useExecutionEvents.ts）

- `sharedRemoveTaskTimers` 从 `Set` 改 `Map<taskId, timer>`；`Finished` 时按 taskId 登记。
- `Sync` case：遍历 tasks，对存在的 taskId 清除其定时器（任务被 Sync 重置为 running，不应被旧定时器移除）。
- 定时器回调内检查任务仍存在再 dispatch（经 `sharedGetState` 或让 reducer 幂等——reducer `REMOVE_RUNNING_TASK` 对不存在 id 已是 no-op，只需 Sync 清定时器）。

### R9 P3（execution.rs dashboard）

```
GET /stats/dashboard:
  hit 且未过期 → 返回
  hit 但过期   → 返回旧值 + tokio::spawn 后台刷新（单飞：刷新中标记防并发击穿）
  miss         → 同步计算 + 写缓存
执行完成（executor_service/completion.rs）→ invalidate_dashboard_cache()
heatmap → 独立 HEATMAP_CACHE，TTL 10min
```

单飞用 `tokio::sync::OnceCell`/刷新中布尔 + 写锁双重检查。

### R10 规范

- daemon/linux.rs、daemon/redeploy.rs、cli/commands.rs 的 `println!/eprintln!`：模块级 `#![allow(clippy::print_stdout, clippy::print_stderr)]` + 注释说明"CLI 子命令 stdout 即用户界面"。（项目禁止清单#13 限 `#[cfg(test)]` 外用 allow——但 CLAUDE.md 豁免场景是 lint 抑制需说明理由；此处选择改为 `tracing` 不可行，daemon install 输出必须到用户终端。采用在函数/模块上最小范围 allow + 注释。）
- `custom_template.rs:328,338` `config.read().unwrap()` → `unwrap_or_else(|e| e.into_inner())`（与 mod.rs 既有模式一致）。
- `custom_template.rs:344` cron 解析 unwrap → 兜底默认 cron。
- `auto_update.rs` 3 处 unwrap → R5 修复。
- `experts.rs:605` file_name().unwrap() → `ok_or` 传播。
- `scheduler.rs:242`、其余已有不变量注释的 expect → 保留（已有 #[allow] + 理由）。
- 删除 dead code `ntd_dir`；修 `collapsible_match`（execution_events/metadata.rs、handlers/session.rs）、`needless_borrows_for_generic_args` 等。

## 6. 前端详细改动

| 文件 | 改动 |
|------|------|
| utils/database/todos.ts | `getAllTodos` 改分页响应；新增 `getTodoIds/getTodoCount/getTodoBriefs`；`getTodoCenter` 加分页参数 |
| utils/database/todos.ts | 新增 `getTodoById`（若已有则复用） |
| todo-list/TodoListPage.tsx | useTodoListData 改服务端分页：state 增加 page/page_size/sort/bucket/search，reload 携带；Table onChange 驱动；删除 filterBySearchKeyword |
| todo-list/TodoListView.tsx | pagination 改受控（current/pageSize/total/onChange）；bucket Tabs 用 bucket_counts 角标 |
| TodoCenterCardView.tsx | 同改服务端分页（卡片底部 ant Pagination） |
| KanbanBoard.tsx | 改 getTodoBriefs 全量（无 ids）；hours 过滤保留（brief 接口支持 hours 参数） |
| hooks/useTodoContext.tsx | 删除 todosByWorkspace 桶与 SET_TODOS_BY_WORKSPACE；state.todos 派生删除 |
| hooks/useConceptCounts.ts | getAllTodos→getTodoCount |
| MemorialBoard / RunningRecordDrawer / todo-post / running-board | state.todos.find(id) → getTodoById 按需 + 本地 useState 缓存 |
| App.tsx / TodoDetail.tsx | onSaved 等回调不再全量重拉，只 dispatch TODO_LIST_REFRESH_EVENT |
| hooks/useExecutionEvents.ts | R8 修复 |
| useApp.tsx:65 | 启动不再预拉 todos |

**by-id 缓存策略**：新增 `hooks/useTodoById.ts`（Map<id, Todo> + 请求中去重 + TODO_LIST_REFRESH_EVENT 失效），供 D 类组件使用，避免每个组件自造缓存。

## 7. 测试计划

### 后端（`backend/tests/` + 模块内单测）
- execution workspace 过滤：两 ws 各造记录，验证分页 total/条数正确（修复前稀释场景回归）
- todo-center：分页元数据、bucket 过滤、bucket_counts、search、排序白名单拒绝非法字段
- ids/count/brief 接口
- 旧 /todos 分页响应结构 + hours SQL 过滤
- sync 分批游标（造 1200 条验证两批）
- R5 DST 三单测；R7 flow-style/缺 name/正常三单测
- R9 缓存：过期 stale 返回、主动失效

### 前端
- `npx tsc --noEmit` 零错误
- TodoListPage 翻页/搜索/bucket 切换 Playwright 验证（`frontend/tests/check_056_pagination.spec.ts`）
- 看板 brief 加载渲染验证

### 质量门禁
- `cargo clippy --all-targets -- -D warnings` 零告警
- `cargo test` 全过

## 8. 风险与回滚

| 风险 | 缓解 |
|------|------|
| todo-center SQL 分桶与 Rust `compute_bucket` 语义漂移 | 同一优先级表驱动两边；集成测试对拍两实现结果一致 |
| 旧 /todos 响应结构破坏外部脚本 | CLI 同步升级；需求文档已记录 breaking change；版本号 minor bump |
| 全局桶删除后某读取点遗漏 | `state.todos` 编译期删除，tsc 报错即遗漏点清单 |
| dashboard 单飞实现复杂度 | 退化为"过期同步重算"（现状）也可接受，单飞失败不阻塞 |
