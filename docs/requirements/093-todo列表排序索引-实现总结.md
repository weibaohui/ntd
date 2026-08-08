# 093-todo列表排序索引-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |

> 对应设计：`docs/design/093-todo列表排序索引-设计.md`。093 优化扫描专项第 3 项。

## 1. 实现了什么

Todo 列表/看板（最高频读路径）的排序与时间窗过滤从「全表扫 + filesort」改为索引 range scan：

1. **新增 `idx_todos_updated_at ON todos(updated_at DESC)`**（迁移 V93）——此前 todos 表 10 个索引独漏排序键；
2. **hours 过滤参数化**：`REPLACE(REPLACE(updated_at,'T',' '),'Z','') >= datetime('now','-N hours')`（列上套函数，索引恒失效）→ Rust 侧算 cutoff，`updated_at >= ?` 参数绑定裸列比较；
3. **新增 `models::utc_timestamp_minus_hours`**：cutoff 与存储格式（`utc_timestamp` 毫秒级 ISO / 触发器秒级 ISO）严格同构，字符串序=时间序。

## 2. 与设计的对应关系

| 设计项 | 落地 | 状态 |
|--------|------|------|
| C1 迁移 V93 | `db/migration/v93.rs`（仿 v59 模板，存在性+幂等单测）+ 注册 + `consolidated_schema.rs` 重生成 | ✅ |
| C2 hours 参数化 | `db/todo.rs` 两处（`get_todo_briefs` 原生 SQL / `get_todos_page_by_workspace` SeaORM） | ✅ |
| C3 测试格式对齐 | `test_todos_page_by_workspace_hours_and_total` 的 `datetime('now')` 改 `strftime('%Y-%m-%dT%H:%M:%SZ')`（与生产触发器同格式） | ✅ |
| C4 cutoff helper | `models::utc_timestamp_minus_hours` + 格式/偏移单测 | ✅ |

### 顺带修复（实施期发现的工具链 bug）

- `tests/dbg_gen_schema.rs`：**硬编码原作者机器绝对路径**（`/Users/weibh/...`），其他人运行必 panic → 改 `env!("CARGO_MANIFEST_DIR")` 相对推导；头部注释版本号硬编码 `v1-v87` 失真 → 改版本中立表述。
- 发现生成器工作方式：`Database::new(":memory:")` 走 bootstrap 直接建 consolidated DDL 并盖章最新版本，**新索引必须先手写进 `CONSOLIDATED_SCHEMA` 再跑生成器**归一化（与 091 文档记载的流程一致）。

## 3. 关键实现点

- **NULL 语义不变**：新旧写法对 `updated_at IS NULL` 行都排除（NULL 比较结果为 NULL）。
- **格式混排边界**：生产库并存毫秒级（应用层）与秒级（触发器）两种 ISO 形态，字符串比较在秒级等价，毫秒边界误差 <1s，hours 级过滤可忽略（设计 §1.3 有完整考据）。
- **EXPLAIN QUERY PLAN 实测**（1000 行样本库）：`SEARCH todos USING INDEX idx_todos_updated_at (updated_at>?)`——range scan 命中，排序由索引序天然满足，无 filesort。

## 4. 测试与验证结果

- `cargo clippy --all-targets -- -D warnings`：零告警 ✅
- `cargo test --no-fail-fast`：1710 通过；唯一失败 `git_sync::test_sync_repo_restores_deleted_file` 为预存量环境问题（本机 git 过老不支持 `git init -b`，main 上同样失败）✅
- 漂移守卫 `test_consolidated_schema_matches_incremental` 与 v67→latest 升级链测试通过 ✅
- `make dev` 真实库（`~/.ntd/data.dev.db`）端到端验证 ✅：
  - V93 迁移落库日志可见，schema_version=93；
  - `GET /api/v1/workspaces/1/todos?hours=96` → total=2（id 8,27，与手工 SQL 推算一致）；`hours=48` → 0（ws 内最近一条在 91h 前，正确排除）；`hours=720` → 10（全量）；
  - `GET /api/v1/workspaces/1/todos/brief?hours=96` → id [8,27]；无过滤 → 10。

## 5. 已知限制 / 后续候选

- 同款 `REPLACE(REPLACE(...)) >= datetime(...)` 反模式仍存在于 `execution.rs:237`、`loop_.rs:844/1686`、`dashboard.rs` ×4、`todo.rs:1935`（execution_records 的 started_at/finished_at 列）——聚合统计查询为主，列入后续专项候选（设计 §4 已声明不在本 PR）。
- 复合索引 `(workspace_id, updated_at)` 可进一步覆盖看板过滤+排序，当前数据量（万行级）单列已足，留待数据量增长后评估。

## 6. 安全反思

- 过滤值从 `format!` 拼接（原靠 `u32` 类型免疫注入）改为参数绑定，注入面彻底归零；
- 迁移只增索引不改数据，回滚 = `DROP INDEX`，无状态风险；
- 无接口 schema 变化，前端零改动。
