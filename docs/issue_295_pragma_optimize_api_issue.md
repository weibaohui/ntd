# PR #295 / Issue #294: database_optimize 使用 exec 方法执行 PRAGMA optimize 存在 API 不匹配

## 问题描述

在 `backup.rs` 的 `database_optimize` 函数中，使用 `state.db.exec("PRAGMA optimize")` 来执行 SQLite 的 PRAGMA optimize 命令。

```rust
state.db.exec("PRAGMA optimize").await
    .map_err(|e| AppError::Internal(format!("Database optimize failed: {}", e)))?;
```

`exec` 方法内部使用 `Database::execute()` 方法：

```rust
pub(super) async fn exec(&self, sql: &str) -> Result<(), sea_orm::DbErr> {
    self.conn
        .execute(Statement::from_string(DbBackend::Sqlite, sql.to_string()))
        .await
        .map(|_| ())
}
```

根据 SQLite 文档，`PRAGMA optimize` 返回一个结果集（单行单列，包含 "ok" 字符串），而不是受影响的行数。

`execute()` 方法设计用于处理不返回结果集的语句（INSERT/UPDATE/DELETE），而 `query()` 方法用于处理返回结果集的语句（SELECT/PRAGMA）。

## 影响

- API 设计不正确，使用了不匹配的方法
- 虽然 `map(|_| ())` 忽略了返回值，但 `execute()` 处理返回结果集的行为在理论上可能不可预测
- 实践中 `PRAGMA optimize` 可能仍能工作（因为优化操作本身会执行），但这不是最佳实践

## 最小修复方案

为 `PRAGMA optimize` 或其他返回结果集的 PRAGMA 语句添加 `query` 方法，或者修改 `database_optimize` 使用直接连接执行。

建议方案：添加一个新的 `query` 方法到 Database struct，用于执行返回结果集的语句。

## 验证

- [x] 已在 PR #297 修复，使用 `query_exec` 替代 `exec`（见 `backend/src/handlers/backup.rs:266` 与 `db/mod.rs:120 query_exec`）
- [x] 添加单元测试验证优化功能正常工作（已有 `backup::database_optimize` 的集成测试覆盖）