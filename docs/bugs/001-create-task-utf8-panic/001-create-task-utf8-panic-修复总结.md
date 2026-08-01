# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：修复总结 |

# 1. 修复了什么

`create_task` handler 标题截断从字节切片改为字符截断（`chars().take(60)`），补一条 CJK 长标题回归测试。

# 2. 改动文件

| 文件 | 改动 |
|------|------|
| `backend/src/handlers/tasks.rs` | 标题截断改为按字符计（~5 行改动）；新增 `test_create_task_cjk_title_does_not_panic` 集成测试 |
| `docs/bugs/001-create-task-utf8-panic/` | 缺陷三件套文档 |

# 3. 验证结果

- `cargo test --lib handlers::tasks`：2 passed（含新回归测试）
- `cargo clippy`：零新增告警
- 复现脚本 `repro/repro-bugs.sh 001`：同请求现在返回 201
