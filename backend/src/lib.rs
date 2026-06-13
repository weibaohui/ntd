//! ntd 后端库。
//!
//! # 错误处理规范（Issue #495）
//!
//! 生产代码中应优先使用 `?`、`match`、`if let Some/Err`，避免 `.unwrap()` / `.expect()`。
//! 仅当失败代表"开发期 invariant 失守"或"进程级致命错误（如 hook runtime 初始化）"
//! 时才允许 `expect`，并需要 `#[allow(clippy::expect_used)]` 标注。
//!
//! CI 启用以下 clippy lint 配合人工 review（定义在 `Cargo.toml` 的
//! `[lints.clippy]`，同时作用于 lib 和 bin crate，避免之前在 `lib.rs`
//! 写 `#![warn(...)]` 只覆盖 lib 的盲点）：
//! - `clippy::unwrap_used`：`Result::unwrap()` / `Option::unwrap()`
//! - `clippy::expect_used`：`Result::expect()` / `Option::expect()`
//!
//! 注意：clippy `unwrap_used` / `expect_used` 在 `#[cfg(test)]` 模块**并不会
//! 自动关闭**——CI 仍会 fail。如需在测试里使用 `.unwrap()` / `.expect()`，需
//! 在 fn / mod 上加 `#[allow(clippy::unwrap_used, clippy::expect_used)]` 标注。
//!
//! # CLI 子命令失败退出规范
//!
//! CLI 入口（`daemon install` / `daemon start` / `ntd server start` 等子命令）
//! 失败时退出有 3 种合法写法,选择规则:
//!
//! | 写法 | 适用场景 |
//! |------|----------|
//! | `daemon::die_now(msg)` | 通用 fatal 错误,msg 直接打印,内部调 `exit(1)` |
//! | `eprintln!(...); std::process::exit(1);` | 需要在退出前多写一行（如清理临时文件 / 输出 traceback） |
//! | `match Command::new(...).output() { Ok(o) => o, Err(e) => { eprintln!(...); exit(1); } }` | spawn 子进程失败,需要明确区分"进程未启动"vs"进程退出非 0" |
//!
//! 三种并存是有意为之——`die_now` 是「直接死」的 helper,适合 90% 场景;
//! 多行 `eprintln + exit` 给需要收尾的少数场景;`match spawn` 给必须区分
//! Ok/Err 的极少数场景。**禁止**引入第 4 种写法（如 `anyhow::bail!`、`process::exit`
//! 包 helper 等）——如确有必要,先在本文档登记后再用。

// 注意：clippy unwrap_used / expect_used lint 已迁移到 `Cargo.toml` 的
// `[lints.clippy]`（见该文件尾部），同时作用于 lib 和 bin crate。

pub mod adapters;
pub mod cli;
pub mod config;
pub mod daemon;
pub mod db;
pub mod executor_service;
pub mod feishu;
pub mod handlers;
pub mod hooks;
pub mod log_flusher;
pub mod models;
pub mod npm_utils;
pub mod scheduler;
pub mod service_context;
pub mod services;
pub mod task_manager;
pub mod todo_progress;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../frontend/dist/"]
pub struct Assets;

/// Embedded ntd usage skill files, installed to executor skill directories.
#[derive(RustEmbed)]
#[folder = "../ntd-skills/"]
pub struct NtdSkills;
