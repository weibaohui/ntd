//! ntd 后端库。
//!
//! # 错误处理规范（Issue #495）
//!
//! 生产代码中应优先使用 `?`、`match`、`if let Some/Err`，避免 `.unwrap()` / `.expect()`。
//! 仅当失败代表"开发期 invariant 失守"或"进程级致命错误（如 hook runtime 初始化）"
//! 时才允许 `expect`，并需要 `#[allow(clippy::expect_used)]` 标注。
//!
//! CI 启用以下 clippy lint 配合人工 review：
//! - `clippy::unwrap_used`：`Result::unwrap()` / `Option::unwrap()`
//! - `clippy::expect_used`：`Result::expect()` / `Option::expect()`
//!
//! 测试模块（`#[cfg(test)]`）默认允许 unwrap/expect，不影响主二进制。

// 启用 unwrap_used/expect_used lint，但放在 warn 级别而非 deny，便于迁移期
// 渐进修复。后续 review 阶段可考虑提升到 deny。Issue #495 关注的是「让 panic
// 可被代码扫描器发现」，warn 同样能让 CI 报警。
#![warn(clippy::unwrap_used, clippy::expect_used)]

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
