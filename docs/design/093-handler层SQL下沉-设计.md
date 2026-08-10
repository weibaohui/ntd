# 093-handler层SQL下沉与注入面消除-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本（B5 重定范围） |

> 093 专项 B5（重构批次 5）。skill 扫描原计划「Database 拆 Repository（loop_ 先行）」，
> 实施前重论证（skill 终极原则：不为模式而模式）：
> - `Database` 跨文件 impl 是 Rust 惯用的领域分文件组织，测试已可用内存库整体替换，
>   全量 Repository 拆分（330 方法、全仓调用面）收益/风险比不佳——**不做**；
> - 真实痛点是 `Database.conn` 转义口：`pub(super)` 声明在 `db/mod.rs` 顶层 = 全 crate 可见，
>   handlers 直接写 SQL 绕过分层（违反后端规范 02 分层 + 禁止清单 #4 SQL 拼接）。

## 1. 现状证据（27 处 `.conn` 转义口使用）

| 文件 | 处数 | 性质 |
|------|------|------|
| `handlers/backup.rs:718-731` | 1 | `DELETE FROM execution_logs WHERE timestamp < '{cutoff}'` **format! 拼接** + `SELECT changes()` |
| `handlers/process.rs:774` | 1 | 模板安装次数聚合查询（raw SQL） |
| `handlers/tasks.rs:368` | 1 | `WHERE task_id={}` **format! 拼接**（i64 类型安全但违反禁止清单） |
| `handlers/tasks.rs:467-471` | 3 | 三级实体 find（artifact→step_exec→loop_exec→loop 取 workspace_path） |
| `handlers/tasks.rs:660` | 1 | 测试 helper（`bind_loop_template`） |
| `services/process/installer.rs` | 13 | 模板安装的实体 insert 族 |
| `services/process/phase_driver.rs` | 4 | phase 执行记录 insert/find/update |
| `services/loop_runner.rs` | 2 | 实体直查 |
| `services/process/audit.rs` | 1 | 实体直查 |

## 2. 本 PR 范围（C1-C3）

### C1：handlers 层 4 处下沉为 db 领域方法

| 新 DAO 方法 | 位置 | 收编的 handler SQL |
|------------|------|-------------------|
| `delete_execution_logs_before(cutoff) -> Result<u64>` | `db/execution.rs` | backup.rs 的 DELETE+changes()；**改参数绑定**，用 `ExecResult::rows_affected` 替代 `SELECT changes()` 二次查询 |
| `get_process_template_stats() -> Vec<TemplateStatRow>` | `db/process_template.rs` | process.rs 聚合查询 |
| `list_recent_loop_executions_for_task(task_id, limit) -> Vec<Model>` | `db/loop_.rs` | tasks.rs:368 的 format! 拼接 SQL，**改参数绑定** |
| `get_artifact_workspace_path(step_exec_id) -> Result<Option<String>>` | `db/loop_.rs` | tasks.rs:467 三级 find（纯搬移，不改逻辑） |

### C2：测试 helper 迁移

`tasks.rs` 测试 `bind_loop_template` 的 `db.conn.execute(...)` 改走 `db._conn_raw()`（该口子的既定用途）。

### C3：services 层残留登记

installer.rs 13 处 / phase_driver.rs 4 处 / loop_runner.rs 2 处 / audit.rs 1 处的 `.conn` 直操
**不在本 PR 搬移**——installer 的实体 insert 族若直接搬入会制造 11+ 参 DAO（刚在 B3 消灭的
Long Parameter List 换地方复发），需先设计参数对象，列为 B6 候选。
conn 可见性收口（`pub(self)`）随 B6 完成后再启用（届时编译器强制清零）。

## 3. 影响模块

- `db/execution.rs`、`db/process_template.rs`、`db/loop_.rs`：+4 领域方法（+单测）
- `handlers/backup.rs`、`handlers/process.rs`、`handlers/tasks.rs`：SQL 删除/改调 DAO

## 4. 验证方案

1. 4 个新 DAO 各配单测（内存库 seed → 断言返回）；handlers 既有集成测试全绿；
2. `EXPLAIN`/行为对照：tasks 详情页执行记录列表、备份清理行数、工艺统计接口输出不变；
3. clippy 零告警；假执行器冒烟（备份清理 API 手工 curl 验证行数正确）。

## 5. 安全反思

- **消除 2 处 SQL format! 拼接**（backup cutoff / tasks task_id）为参数绑定——
  虽两处当前均为类型安全值（i64/内部生成时间戳），但违反自家禁止清单 #4，属必须清零项；
- DAO 下沉后 handler 不再触碰 SQL，分层边界由「约定」变「结构」。
