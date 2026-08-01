# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：修复总结 |

# 1. 修复内容

新增迁移 v85：重建 `loop_phase_executions` 表，`phase_id` 外键改为 `ON DELETE SET NULL`。

# 2. 改动文件

| 文件 | 改动 |
|------|------|
| `backend/src/db/migration/v85.rs` | 新迁移 + 1 条单测 |
| `backend/src/db/migration/mod.rs` | 注册 v85 |
| `docs/bugs/008-phase-cascade/` | 缺陷三件套 |

# 3. 验证

- cargo test --lib v85：1 passed（验证新表 FK 含 SET NULL）
- cargo clippy：零新增告警
