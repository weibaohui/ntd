# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：修复总结 |

# 1. 修复了什么

`compose_review_prompt` 改为同时兼容单大括号 `{original_prompt}` 和双大括号 `{{original_prompt}}` 两种占位符（先双后单），确保存量 DB 行和新模板都能正确替换。新增 5 条回归单测覆盖两种模板风格、边界条件、混用场景。

# 2. 改动文件

| 文件 | 改动 |
|------|------|
| `backend/src/executor_service/auto_review.rs` | `compose_review_prompt` 追加单大括号兼容替换（4 行）；新增 `tests` 模块 5 条单测 |
| `docs/bugs/002-review-placeholder-compat/` | 缺陷三件套文档 |

# 3. 验证结果

- `cargo clippy --all-targets -- -D warnings`：auto_review.rs 零新增告警（全仓库 43 个存量告警在未触碰文件，已用 main worktree 核对一致）
- `cargo test --lib auto_review`：13 passed（含 5 条新增）
- 复现脚本验证：存量单大括号模板 prompt 不再残留字面量，评审能正确看到原始任务/执行输出/验收标准

# 4. 已知限制

无。兼容层覆盖存量和新模板，用户自定义模板也不受影响。
