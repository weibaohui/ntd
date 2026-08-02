# 056-Todo 列表服务端分页化与质量修复-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| pi (AI) | 2026-08-01 | 初始版本 |

> 对应需求：`docs/requirements/056-Todo列表服务端分页化与质量修复-需求.md`
> 对应设计：`docs/design/056-Todo列表服务端分页化与质量修复-设计.md`

## 1. 需求对应关系

| 需求 | 状态 | 实现说明 |
|------|------|---------|
| R1 执行记录 ws 过滤下推 SQL | ✅ | `ExecutionRecordQuery.workspace_id` 子查询；execution.rs 6 处内存过滤删除；**分页稀释 bug 修复**（有回归测试） |
| R2 todo-center 服务端分页 | ✅ | SQL CASE WHEN 分桶 + LIMIT/OFFSET；新增 ids/count/brief 三轻量接口；旧 `/todos` 强制分页（决策 3b） |
| R3 前端切换 | ✅ | 列表/卡片页服务端分页；看板 brief（决策 1a）；计数/摘要/下拉全部换轻量接口；全局桶删除（决策 2a） |
| R4 全量查询收尾 | ✅ | `get_todos`/`get_todos_by_workspace_id`/`get_todo_center` 三函数删除；sync 改游标分批 + title/id 映射；feishu 卡片 brief+take(20) |
| R5 L3 DST unwrap | ✅ | `resolve_local_datetime`：Ambiguous 取早、None 顺延 1h、兜底 now；3 个单测 |
| R6 L4 吞 DB 错误 | ✅ | `handlers/mod.rs` WS 同步两处改 error 日志降级；`tasks.rs` artifact 路径解析改 `?` 传播 |
| R7 L5 guid 刷屏 | ✅ | `insert_guid_with_serde_fallback`（flow 风格 serde 往返修复）；`warn_once_per_file` 进程内去重 |
| R8 L6 WS 竞争 | ✅ | 移除定时器改 Map 按 taskId 管理；Sync 到达撤销对应定时器；Started/Finished 改发列表刷新事件 |
| R9 P3 dashboard | ✅ | 单飞刷新锁 + 双重检查 + stale-on-error（续 5s 短 TTL 防穿透）；执行终态（Finished×3 + force_fail）主动失效 |
| R10 规范项 | ✅ | `cargo clippy --all-targets -- -D warnings` **零告警**；生产 unwrap 逐处处理 |

## 2. 关键实现点

### 2.1 后端

- **bucket SQL 表达式**（`db/todo.rs::TODO_CENTER_BUCKET_EXPR`）：与 Rust `compute_bucket` 同优先级（archived > loop > time > event > manual），并有**对拍测试** `test_center_bucket_sql_matches_rust` 防漂移。
- **先分页后聚合**：SQL 只查当页 ids，`build_center_aggregates` 仅对 ≤page_size 条批量补算，避免全表聚合。
- **排序白名单**：id/updated_at/title/status/computed_bucket，白名单外退化为 id（列名拼 SQL 无法绑参，白名单是唯一安全途径）。
- **search LIKE 转义**：`\`、`%`、`_` 先转义，`ESCAPE '\'` 参数绑定。
- **brief 接口**：`SELECT` 只取 8 个轻量列 + `(prompt != '') AS has_prompt`；`ids=None`（看板模式）隐藏已归档，`ids=Some`（定点模式）包含已归档；tag_ids 批量补算无 N+1。
- **CLI 适配**（决策 3b）：`ntd todo list` 逐页拉齐后客户端过滤（status/tag/running/search 原本就被后端忽略，现补齐为真正的客户端过滤）。
- **RecentCompletedTodo 增加 workspace_id**：纪念板项目名反查免去前端 N+1。

### 2.2 前端

- **全局桶删除**：`todosByWorkspace` 及 5 个相关 action 从 `useTodoContext` 移除；`useApp.state.todos` 删除（10 个读取点全部迁移）。
- **新增 `useTodoById`**：模块级缓存 + 请求中去重 + TODO_LIST_REFRESH_EVENT 失效，供 by-id 消费点。
- **TodoListPage**：page/pageSize/sort/search 全下推，搜索 300ms 防抖；`last_execution_at` 列移除 sorter（聚合字段不支持服务端排序）。
- **TodoCenterCardView**：bucket/status/actionType/search 全下推（后端为此新增 status/action_type 过滤与 `action_types` DISTINCT 聚合）；Tab 角标用服务端 bucket_counts；底部 Pagination（24/48/96）。
- **KanbanBoard**：brief 全量（决策 1a）；prompt 展开时按需拉全文（TodoCard 新增 hasPrompt/promptLoading props）；拖拽改走 `force-status` 单字段端点（不再携带 prompt 全量更新）；搜索收缩为标题。
- **WS 事件**：Started/Finished 不再写全局桶，改发 TODO_LIST_REFRESH_EVENT 让各页重拉当前页。

### 2.3 与设计文档的偏差

| 偏差 | 原因 |
|------|------|
| dashboard heatmap 独立长缓存未做 | SWR+主动失效后，heatmap 仅是 miss 成本的 1/20，YAGNI |
| 后台异步 SWR 改为「单飞 + stale-on-error」 | 无后台任务泄漏风险，实现更简单；刷新失败续 5s TTL 防穿透 |
| 看板搜索/prompt 搜索收缩为标题 | brief 不含 prompt（决策 1a 的取舍）；执行记录搜索同样收缩 |
| 看板卡片选中态跨页裁剪 | 服务端分页后批量选择仅对当前页有效 |

## 3. 验证结果

### 质量门禁

| 门禁 | 结果 |
|------|------|
| `cargo clippy --all-targets -- -D warnings` | ✅ 零告警（修复前 45 条） |
| `cargo test`（lib 1574 + 集成 293） | ✅ 全过；唯一失败 `git_sync::test_sync_repo_restores_deleted_file` 为**存量环境问题**（本机 git 2.20 不支持 `git init -b`，main 分支同样失败，与本 PR 无关） |
| `npx tsc --noEmit` | ✅ 零错误 |
| `npx vitest run`（227 用例） | ✅ 全过 |
| Playwright `tests/check_056_pagination.spec.ts` | ✅ 3/3（列表分页/卡片角标+分页/看板 brief） |

### 新增测试

- 后端：`test_center_bucket_sql_matches_rust`（SQL/Rust 分桶对拍）、`test_center_page_pagination_metadata`、`test_todo_briefs_archived_semantics`、`test_todos_page_by_workspace_hours_and_total`、`test_todos_batch_after_id_cursor`、`test_todo_ids_and_count_exclude_archived`、`test_execution_records_workspace_filter_pagination`（R1 稀释回归）、DST×3、guid fallback×5
- API 实测：`/todos` 分页元数据、`/center` bucket 过滤（manual total=2 且 items 全 manual）、`/ids`、`/count`、`/brief` 字段集、执行记录 ws 过滤（ws2 total=27 vs ws1 total=1）

### 存量问题（不在本 PR 范围）

- `db_pool_concurrency_tests` 在本机曾现 v85 迁移对 fresh file-DB 的重复建表失败（main 分支同样复现，与 056 无关；本次全量跑通过，疑似时序敏感）。
- `git_sync` 测试需 git ≥ 2.28。

## 4. Breaking Changes

1. `GET /api/v1/workspaces/{ws}/todos` 响应从 `Todo[]` 变为 `{ items, total, page, page_size }`（决策 3b）。CLI 已同步适配。
2. `GET /todos/center` 响应从 `TodoCenterItem[]` 变为 `TodoCenterPage` 结构（该接口为 054 新增，仅前端两处调用，已同步）。
3. 前端全局 `state.todos` 删除——若有外部插件依赖（无），需改 `useTodoById`。

## 5. 已知限制

- 看板/执行记录搜索不再匹配 prompt（brief 无此字段）。
- 表格「执行时间」列不再可排序（聚合字段）。
- 批量选择仅对当前页有效。
- todo-center page_size 上限 200；超过 200 条的 ws 在卡片墙需翻页。
