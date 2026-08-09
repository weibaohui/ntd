# 093-handler层SQL下沉与注入面消除-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |

> 对应设计：`docs/design/093-handler层SQL下沉-设计.md`。093 专项 B5（重构批次 5，重定范围版）。

## 1. 范围重定说明（与设计文档一致）

skill 扫描原计划「Database 拆 Repository（loop_ 先行）」。实施前重论证（不为模式而模式）：
`Database` 跨文件 impl 是 Rust 惯用的领域分文件组织，全量拆分 330 方法收益/风险比不佳。
**真实痛点**是 `Database.conn` 转义口（`pub(super)` 声明于 crate 顶层模块 = 全 crate 可见），
handlers 直接写 SQL 绕过分层（违反后端规范 02 + 禁止清单 #4）。本 PR 收编 handlers 层全部 4 处。

## 2. 实现了什么

### 新增 4 个 DAO（各配单测）

| DAO | 位置 | 收编的 handler SQL |
|-----|------|-------------------|
| `delete_execution_logs_before(cutoff) -> u64` | `db/execution.rs` | backup.rs 的 format! 拼接 DELETE + `SELECT changes()` 二次查询 → **参数绑定 + rows_affected** |
| `get_process_template_stats()` | `db/process_template.rs` | process.rs 聚合查询原样下沉 |
| `list_recent_loop_executions_for_task(task_id, limit)` | `db/loop_.rs` | tasks.rs `WHERE task_id={}` format! 拼接 → **SeaORM 参数绑定** |
| `get_artifact_workspace_path(step_exec_id)` | `db/loop_.rs` | tasks.rs 三级实体 find 纯搬移 |

### handler 侧

- `backup.rs::cleanup_old_logs`：SQL 全删，调 DAO；
- `process.rs::get_process_stats`：只做 JSON 映射；
- `tasks.rs::build_task_executions` / `resolve_artifact_workspace`：调 DAO + 字段映射；
- `tasks.rs` 测试 helper `bind_loop_template`：改走 `_conn_raw()`（该口子既定用途）。

## 3. 测试与验证结果

- 新增单测 4 例：日志清理行数/幂等/保留新日志、模板聚合倒序与 LEFT JOIN 零计数、
  任务执行记录过滤/倒序/limit、artifact 三级跳取路径/断链 NotFound ✅
- `cargo clippy --all-targets -- -D warnings`：零告警 ✅
- `cargo test --no-fail-fast`：1718 通过（唯一失败为预存量环境问题 git_sync，本机 git 过老）✅

## 4. 已知限制 / B6 候选

- `services/process/installer.rs`（13 处）/`phase_driver.rs`（4 处）/`loop_runner.rs`（2 处）/
  `audit.rs`（1 处）的 `.conn` 实体直操**未搬移**——installer 的 insert 族直接搬入会制造
  11+ 参 DAO（B3 刚消灭的 Long Parameter List 换地复发），需先设计参数对象，列为 B6；
- `conn` 可见性收口（`pub(self)`）随 B6 清零后启用，届时编译器强制无转义口。

## 5. 安全反思

- 本 PR **消除 2 处 SQL format! 拼接**（backup cutoff、tasks task_id）为参数绑定，
  虽原值类型安全（i64/内部时间戳），但自家禁止清单 #4 要求清零；
- handler 不再触碰 SQL，分层边界由约定变结构；无接口/行为变化。
