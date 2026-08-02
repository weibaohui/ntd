# 056-Todo 列表服务端分页化与质量修复-需求

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| pi (AI) | 2026-08-01 | 初始版本 |

## 1. 背景与问题

代码审查发现 `get_todos` / `get_todos_by_workspace_id` 全量查询存在三层架构错位，并附带一个正确性 bug：

1. **API 层**：`GET /workspaces/{ws}/todos` 与 `GET /workspaces/{ws}/todo-center` 仅返回全量数组，无分页参数。
2. **前端层**：Ant Table 的 `pagination` 是浏览器内存切片（假分页），首屏需下载全表（含 prompt 大文本）。
3. **复用层**：`get_todos_by_workspace_id` 被后端 9 处、前端 13 处当作"万能取数口"，其中约 80% 调用点只需要 id 集合 / 计数 / 单条摘要。
4. **正确性 bug（优先修）**：`handlers/execution.rs` 6 处先 `LIMIT/OFFSET` 分页、再按 workspace 内存过滤，导致执行记录列表在多 workspace 下每页条数稀释、`total` 与实际不符。

同时修复审查确认的 6 项质量缺陷（L3–L6、P3、规范项）。

## 2. 需求条目

### R1 执行记录 workspace 过滤下推 SQL（修正确性 bug）

- `get_execution_records` / `get_running_execution_records` / `get_execution_records_by_session` 支持 workspace_id 过滤，SQL 层使用 `todo_id IN (SELECT id FROM todos WHERE workspace_id = ? AND deleted_at IS NULL)` 子查询。
- `COUNT(*)` 与数据查询使用同一过滤条件，保证分页元数据一致。
- 删除 execution.rs 中 6 处「拉全量 todos 建 HashSet 内存过滤」代码。

### R2 Todo 列表接口服务端分页化

- `GET /workspaces/{ws}/todo-center` 新增 `page`/`page_size`/`sort_by`/`sort_order`/`bucket`/`search` 参数，全部下推 SQL；`computed_bucket` 由内存推导改为 SQL `CASE WHEN` 表达式；响应携带 `total` 与各 bucket `counts`。
- `GET /workspaces/{ws}/todos`（旧全量接口，CLI 在用）直接改为强制分页语义（决策 3b），响应结构从 `Vec<Todo>` 变为 `{ items, total, page, page_size }`；CLI 同步升级适配。
- 新增轻量接口：
  - `GET /workspaces/{ws}/todos/ids` → `Vec<i64>`
  - `GET /workspaces/{ws}/todos/count` → `{ count }`
  - `GET /workspaces/{ws}/todos/brief?ids=` → `Vec<{id, title, status, updated_at}>`（不含 prompt 大字段；ids 为空时返回该 ws 全部 brief，供看板使用）

### R3 前端切换

- TodoListPage / TodoCenterCardView：翻页 / 排序 / 搜索 / 切 bucket 时请求对应页（服务端分页），删除前端内存分页与搜索过滤。
- KanbanBoard：保持全量渲染，改用 brief 接口（决策 1a）。
- useConceptCounts 改用 count 接口；RunningRecordDrawer / todo-post / MemorialBoard / running-board / settings 下拉改用 by-id 或 brief 接口。
- 全局 `state.todos` 全量桶彻底删除，10 个读取点改按需 by-id 查询（决策 2a）。

### R4 全量查询收尾

- db 层删除 `get_todos()` / `get_todos_by_workspace_id()`：
  - `handlers/sync.rs` 云同步改 id 游标分批（每批 500）。
  - `services/feishu_listener.rs` 卡片数据改 brief + `LIMIT 20`。
- `SET_TODOS_BY_WORKSPACE` 全量 action 退役。

### R5 L3：auto_update DST unwrap panic

`services/auto_update.rs:146,160,164` 的 `and_local_timezone(Local).unwrap()` 在夏令时切换日（本地时刻不存在/歧义）会 panic。改用 `LocalResult::single()`，歧义取较早值、不存在顺延 1 小时，附 DST 单测。

### R6 L4：DB 错误被 unwrap_or_default 吞掉

仅修复"DB 读取失败后降级为 200 空数据、调用方无法区分真空与故障"的点位（`handlers/mod.rs:174,180` WS 同步等），改为 `?` 传播或 `tracing::error!` 后降级。合法默认值语义不动。

### R7 L5：工艺模板 guid 插入失败日志刷屏

根因：flow-style YAML（`process: {name: x}`）下 `process_block_range` 找不到块起始行，`insert_guid_after_name` 永远失败且每次 PUT 重试。

- `insert_guid_after_name` 文本插入失败时 fallback：serde 结构体重序列化回写（保留数据，格式标准化）。
- 对仍无法修复的文件，进程内去重 warn（每文件只告警一次）。

### R8 L6：WS Finished 移除定时器与重连 Sync 竞争

- Sync 到达时清除对应 task 的待执行移除定时器。
- 移除定时器触发时若任务已不在 running 列表则跳过。

### R9 P3：dashboard 缓存增强

现状已有 30s TTL + try_join 并发。改进为：

- stale-while-revalidate：过期仍先返回旧值，后台异步刷新。
- 执行完成事件主动失效缓存。
- heatmap（当年口径，年内缓慢变化）结果单独长缓存（10 分钟）。

### R10 规范项：clippy 告警与生产 unwrap

- 45 个 clippy 告警清零：daemon/cli 的 36 个 `println!/eprintln!` 属 CLI 用户输出，显式 `#[allow]` 标注理由；2 个生产 expect 改错误传播；删除 dead code `ntd_dir`；修复 collapsible_match 等。
- 生产代码 26 处 unwrap/expect/panic 逐个审查：有不变量保护的补 `#[allow]` + 注释，无保护的改错误传播。
- 提交前 `cargo clippy --all-targets -- -D warnings` 零告警、`cargo test` 全过、`npx tsc --noEmit` 零错误。

## 3. 已确认决策

| # | 决策点 | 结论 |
|---|--------|------|
| 1 | KanbanBoard 数据策略 | 保持全量渲染，改用 brief 接口（a） |
| 2 | 全局 state.todos 桶 | 彻底删除，改按需 by-id（a） |
| 3 | 旧全量接口去留 | 本版本直接改为强制分页，CLI 同步升级（b） |
| 4 | 实施节奏 | 单 PR 全量交付（a） |

## 4. 非目标（Out of Scope）

- BlackboardPage.tsx 1285 行巨型组件拆分（用户明确排除）。
- 认证体系、CORS、webhook secret 等安全项（另立项）。
- keyset/cursor 分页（数据量未达需要，OFFSET 足够）。
