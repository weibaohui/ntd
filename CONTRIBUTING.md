# Contributing to ntd (Nothing Todo)

感谢你愿意贡献！本文档列出本仓库（特别是 `backend/` 目录）当前的 Rust 开发规约。
如有冲突以本文档为准，并在 PR 描述里说明。

## 开发环境要求

- **Rust**: 1.81 或更高（与 `backend/Cargo.toml` 的 `rust-version` 字段一致）。
- **Node.js**: 20+（前端构建需要）。
- **OS**: macOS / Linux。Windows 下后端可以编译运行，但 daemon 子命令走 stub 分支。
- **构建工具**:
  - `cargo` 自带。
  - 可选：`cargo-clippy`、`cargo-deny`、`vergen-gitcl`（构建期依赖）。

## 本地启动流程

完整 dev 流程见根目录 `Makefile`：

```bash
make dev     # 启动开发模式（端口 18088）
make stop    # 停止开发实例
make build   # 构建生产版本（端口 8088）
```

如果只想跑后端测试：

```bash
cd backend
cargo test --lib
cargo test --tests
```

CI 上的强制检查：

```bash
cargo fmt --all -- --check        # 风格（默认 rustfmt 配置；见下方「代码风格」节）
cargo clippy --all-targets -- -D warnings  # lint（见下方）
cargo test --all-targets          # 全量测试
cargo deny check                  # 依赖治理（可选但推荐）
```

## 代码风格

- **风格**: 遵循 rustfmt 默认配置（`max_width = 100`、4 空格缩进、tab 禁止）。
  暂未引入 `rustfmt.toml`，因为本仓库存量代码与 rustfmt 1.9 stable 的格式输出存在
  几百处无关 diff；后续如需锁定风格，请单独开 PR + 一次性 `cargo fmt --all` 后再加入
  `rustfmt.toml`。提交前请 `cargo fmt --all` 修正你自己改动的文件。
- **注释规范**: 见根目录 `CLAUDE.md` 顶部「代码注释规范」一节。简言之：
  - 逐行注释解释「为什么」，不要复述代码做了什么。
  - 大段逻辑前写「段落总览」注释。
  - 修改既有代码时同步更新注释。
- **模块路径**: `frontend/src` 下跨目录导入用 `@/` 绝对路径别名；Rust 端跨模块用 `crate::` 前缀。

## Lint 策略

`backend/Cargo.toml` 中 `[lints.clippy]` / `[lints.rust]` 维护当前的 lint 集合。**新增 / 调整 lint 必须同步更新本节**。

### deny 级（CI 阻断）

| lint | 来源 | 理由 |
|------|------|------|
| `unwrap_used` | clippy | 生产代码不应 panic |
| `expect_used` | clippy | 同上 |
| `panic` | clippy | 同上 |
| `unsafe_code` | rust | 集中到 `src/sys.rs` 唯一出口 |
| `wildcards` | cargo-deny | 显式声明版本范围 |

### warn 级（PR 评审逐条 review）

| lint | 来源 | 建议动作 |
|------|------|----------|
| `todo` | clippy | 在 PR 描述里登记跟踪 |
| `dbg_macro` | clippy | 提交前删 |
| `print_stdout` / `print_stderr` | clippy | CLI 边界可用 `eprintln!`（加 `#[allow(clippy::print_stderr)]`），其他场景改 `tracing` |
| `missing_docs` | rust | 公开 API 必加 `///` |
| `too_many_arguments` | clippy | 抽出 `*Args` 结构体 |
| `needless_collect` / `redundant_clone` | clippy | 性能小优化 |

### 运行 clippy

```bash
cargo clippy --all-targets -- -D warnings
```

## 错误处理约定

- **模块边界**: 用 `thiserror::Error` 定义模块专属错误类型。
- **顶层入口**: 用 `anyhow::Result`，允许直接 `?` 传递。
- **生产代码**: **禁止** `.unwrap()` / `.expect()` / `panic!`。业务路径里能用 `?` + `?` 链就用。
- **可恢复错误**: 显式 `match` 或 `if let Some(...) = ...`；不要在错误恢复路径里 `panic!`。
- **错误日志**: 走 `tracing::error!`，**不要** `eprintln!`（CLI 边界除外）。

## Async 风格

- **运行时**: tokio。所有 I/O 必须 async。
- **锁选择**:
  - 读多写少 → `parking_lot::RwLock`（同步）或 `tokio::sync::RwLock`（跨 await）。
  - 写多读少 → `parking_lot::Mutex`。
  - **绝不在持锁时 await**。如需跨 await，用 `tokio::sync::Mutex`。
- **取消安全**: spawn 出去的 future 必须是 cancel-safe 的；如果取消可能丢失中间状态，先把状态写库再返回。
- **sleep**: 用 `tokio::time::sleep(...).await`；**不要** `std::thread::sleep`，会阻塞 tokio worker。

## 日志约定

- 全部走 `tracing`。
- 错误：`tracing::error!`。
- 业务关键路径：`tracing::info!`。
- 调试/可观测：`tracing::debug!` 或 `tracing::trace!`。
- 慢路径 / 异常分支：`tracing::warn!`。
- 避免 `println!` / `eprintln!`（CLI 工具边界除外）。

## 测试规范

- **单元测试**: 与被测代码同文件 `mod tests`；纯函数 / 数据解析等纯逻辑必加。
- **集成测试**: `backend/tests/<module>_<feature>_tests.rs`；测跨模块交互与公共 API。
- **依赖**: 共享 fixture 用 `tests/common/` 风格；内存数据库用 `Database::new(":memory:")`。
- **命名**: `test_<function>_<scenario>`，覆盖正常路径 + 关键错误路径。
- **异步测试**: 用 `#[tokio::test]`。
- **新加 unsafe**: 必须在 `src/sys.rs` 里加单元测试，至少覆盖 Ok 与 Err 两条路径。

## API 设计

- **版本化**: 公共 HTTP API 加 `/api/v1/...` 前缀。
- **错误响应**: 统一格式：

  ```json
  { "error": true, "message": "Human readable", "code": "TODO_NOT_FOUND" }
  ```

- **WebSocket 事件**: 走统一的 `events_handler`；事件类型集中定义。
- **handler 不直写 SQL**: 一律通过 `db` 模块的方法访问。

## 数据库访问

- **禁止** handler / service 直写 SQL。
- 所有数据库读写必须经 `db::Database` 上的方法封装。
- **新加表**: 在 `db/mod.rs` 的迁移表里登记 schema_version；不要写 `CREATE TABLE IF NOT EXISTS` 然后 `.ok()` 吞错（参见 issue #498 的修复方式）。
- **连接池**: 用 SeaORM 的 `DatabaseConnection`；不要在 hot path 里临时连接。

## Feature flag

下列 feature 在 `Cargo.toml` 中**必须**保持开启（被业务代码依赖）：

- `serde` / `serde_json`（DTO 序列化）
- `chrono` with `serde`（时间字段）
- `tokio` full runtime
- `axum` 的 `ws` / `macros`

新加 feature 时在 PR 描述里说明用途，避免依赖爆炸。

## Unsafe 治理

> 集中管控 unsafe 边界：除 `sys` 模块外，全工作区禁止 `unsafe` 关键字。
> 新增 FFI 调用请放进 `src/sys.rs` 并在本节登记。

### 当前 unsafe 调用清单

| 函数 | 位置 | FFI | 用途 |
|------|------|-----|------|
| `set_socket_reuseaddr` | `src/sys.rs` | `libc::setsockopt` | daemon 重启避 TIME_WAIT |
| `current_uid` | `src/sys.rs` | `libc::getuid` | macOS launchd plist 路径 |
| `current_euid` | `src/sys.rs` | `libc::geteuid` | systemd 守护进程 root 检查 |
| `require_root_or_exit` | `src/sys.rs` | （组合 `current_euid`） | CLI 边界统一 root 守卫 |

### 添加新 unsafe 的流程

1. 在 `src/sys.rs` 写一个安全包装函数。
2. 给出 `// SAFETY:` 注释，解释所有 unsafe 前置条件。
3. 在同文件 `#[cfg(test)]` 块里加单测（至少 Ok / Err 两条路径）。
4. 更新本节表格。
5. PR 描述里单独 highlight unsafe 改动，请求二次 review。

## 依赖治理

- 引入新依赖前请在 PR 描述里说明：
  - 为什么不能复用现有依赖？
  - 是否活跃维护？最近一次 commit 距今多久？
  - 是否引入新 transitive dependencies？
- `deny.toml` 列出了允许的 license；超出白名单需要 PR 显式 `allow`。
- **不要** 使用 `*` 通配符版本（cargo-deny `wildcards = "deny"`）。

## 提交流程

1. 拉取 main：`git fetch origin main:main`。
2. 创建 worktree：`git worktree add /tmp/<branch-name>-hhmmss main`。
3. 在 worktree 中开发、测试。
4. commit message 格式：

   ```
   <type>(<scope>): <subject>

   <body>

   Fixes #<issue-number> (if applicable)
   ```

   `<type>` 取值：`feat` / `fix` / `refactor` / `docs` / `test` / `chore` / `perf`。

5. 推送：`git push -u origin HEAD`。
6. 创建 PR：`gh pr create --title "<subject>" --body "<body>" --base main`。
7. 清理 worktree：`git worktree remove /tmp/<branch-name>-hhmmss`。

## 反馈渠道

- GitHub Issue：bug / 需求 / 设计讨论
- PR Review：所有非 trivial 改动需至少一次 self review + 一次同行 review
- 紧急问题：联系 @weibaohui

再次感谢你的贡献 🙏
