# 093-todo列表排序索引-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |

> 093 优化扫描专项第 3 项。前两项见 `093-首屏bundle瘦身-设计.md`、`093-WS重连日志尾取-设计.md`。

## 1. 背景与问题

Todo 列表/看板是最高频读路径，两个互相关联的问题：

### 1.1 `ORDER BY updated_at DESC` 无索引支撑

`get_todos_page_by_workspace`（列表分页）、`get_todo_briefs`（看板）都按 `updated_at DESC` 排序分页，但 `todos` 表 10 个索引中**没有 updated_at 索引**（`loops` 表已有 `idx_loops_updated_at`，todos 遗漏）。每页查询 = 全表扫 + filesort。V90（091 专项）补了 `todos(workspace_id)` 等 11 个索引，独漏排序键。

### 1.2 hours 过滤在列上套函数，即使有索引也用不上

两处（`db/todo.rs:548` 原生 SQL、`db/todo.rs:628` SeaORM 条件）：

```sql
REPLACE(REPLACE(updated_at, 'T', ' '), 'Z', '') >= datetime('now', '-{h} hours')
```

对列做 `REPLACE` 函数运算使索引失效（无法走 range scan），且 `datetime('now', ...)` 每行求值。

### 1.3 为什么现在可以安全参数化（数据格式考据）

旧写法用 REPLACE 归一化是防御混合格式。实测现状：

| 写入方 | 格式 | 出处 |
|--------|------|------|
| 应用层 `ActiveModel` | `%Y-%m-%dT%H:%M:%S%.3fZ` | `models::utc_timestamp()` |
| SQLite 触发器（仅 NULL/空时兜底） | `strftime('%Y-%m-%dT%H:%M:%SZ')` | `migration/v1.rs:226` |

生产库统一为「T/Z ISO」形态，仅精度差（触发器秒级 vs 应用毫秒级）：直接字符串比较在秒级正确，毫秒边界误差 <1s，对 hours 级过滤可忽略。**唯一**写空格格式（`datetime('now')`）的是测试代码（`todo.rs:2655/2661`），应随本 PR 改成与生产一致的 ISO。

NULL 语义不变：旧表达式对 NULL 行比较结果为 NULL（排除），`updated_at >= ?` 同样排除 NULL 行。

## 2. 设计

### C1：迁移 V93 加排序索引

新建 `db/migration/v93.rs`（模板照抄 `v59.rs`）：

```sql
CREATE INDEX IF NOT EXISTS idx_todos_updated_at ON todos(updated_at DESC)
```

- 选 `DESC` 与 `loops` 表现存 `idx_loops_updated_at` 先例对齐（SQLite 对 ASC 索引也能反向扫，功能等价，统一风格优先）。
- 单列表而非 `(workspace_id, updated_at)` 复合：全局列表（无 ws 过滤）与看板（有 ws 过滤）共用此排序键，单列是两者的最大公约数；既有 `idx_todos_workspace_id` 继续服务 ws 过滤。
- 注册进 `migration/mod.rs` V92 之后；跑 `dbg_gen_schema` 重生成 `consolidated_schema.rs`；漂移守卫 `test_consolidated_schema_matches_incremental` 必须通过。
- 迁移单测照 v59 范式：索引存在性（sqlite_master）+ 幂等（重复执行不报错）。

### C2：hours 过滤参数化

`models/mod.rs` 新增（与 `utc_timestamp` 相邻，同一格式串）：

```rust
/// 与 utc_timestamp 同格式，取「hours 小时前」的 UTC  cutoff。
pub fn utc_timestamp_minus_hours(hours: u32) -> String
```

两处调用点改为参数绑定：

- `todo.rs:548`（原生 SQL）：`sql.push_str(" AND updated_at >= ?")` + `values.push(cutoff.into())`；
- `todo.rs:628`（SeaORM）：`cond.and(todos::Column::UpdatedAt.gte(cutoff))`。

效果：`updated_at >= ?` 是裸列比较，配合 C1 索引走 range scan；`ORDER BY ... DESC LIMIT n` 直接读索引尾部。

### C3：既有测试格式对齐

`test_todos_page_by_workspace_hours_and_total` 的 `datetime('now')` 改为 `strftime('%Y-%m-%dT%H:%M:%SZ','now')`（与生产触发器同格式），并补一条「边界外旧行被排除」断言。该测试同时成为 C2 的回归保障。

### C4：`utc_timestamp_minus_hours` 单测

格式形状（正则 `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$`）+ 与 `Utc::now()` 的差值在容差内。

## 3. 影响模块

| 层 | 文件 |
|----|------|
| 迁移 | `db/migration/v93.rs`（新）、`migration/mod.rs`（注册）、`consolidated_schema.rs`（生成器重生成） |
| DB | `db/todo.rs`（2 处查询改造 + 1 处测试格式对齐） |
| models | `models/mod.rs`（+1 helper +单测） |

## 4. 不做的事（范围边界）

同款 `REPLACE(REPLACE(...)) >= datetime(...)` 反模式还存在于 `execution.rs:237`、`loop_.rs` ×2、`dashboard.rs` ×4、`todo.rs:1935`（finished_at/started_at 列）。那些是聚合统计查询（全表扫由聚合语义决定，索引收益不同），**不在本 PR**，列入后续专项候选。

## 5. 验证方案

1. 迁移：V93 单测（存在性 + 幂等）+ 漂移守卫 + 全新库 bootstrap 路径验证。
2. 查询：`test_todos_page_by_workspace_hours_and_total` 改格式后通过即证明参数化正确；`EXPLAIN QUERY PLAN` 人工核对走 `idx_todos_updated_at`（开发库验证，文字记录入实现总结）。
3. `cargo clippy --all-targets -- -D warnings` 零告警；`cargo test` 全绿（预存量环境失败 `git_sync::test_sync_repo_restores_deleted_file` 除外，见 PR#1000 说明）。
4. `make dev` 起 18088，列表页选「最近 24 小时」过滤，结果正确且无异常日志。

## 6. 安全反思

- 过滤值改参数绑定后**消除了**原来的 `format!` 拼接（原 `h: u32` 类型已免疫注入，改后连类型依赖都不再需要）；
- 索引只增不删，无数据迁移风险；回滚 = drop index，无状态。
