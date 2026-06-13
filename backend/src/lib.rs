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
//! 测试模块（`#[cfg(test)]`）默认允许 unwrap/expect，不影响主二进制。

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
