// 集中管控 unsafe 边界：除 `sys` 模块外，全工作区禁止 `unsafe` 关键字。
// 新增 FFI 调用请放进 `src/sys.rs` 并在 CONTRIBUTING.md 的「Unsafe 治理」一节登记。
#![deny(unsafe_code)]

pub mod adapters;
pub mod cli;
pub mod config;
pub mod daemon;
pub mod db;
/// 执行反馈统一事件模块
///
/// 提供统一的事件抽象层，将各执行器的原始输出转换为结构化的 ExecutionEvent。
pub mod execution_events;
pub mod executor_service;
pub mod feishu;
pub mod handlers;
pub mod log_flusher;
pub mod models;
pub mod npm_utils;
pub mod scheduler;
pub mod service_context;
pub mod services;
/// 系统调用封装层：项目内唯一允许使用 `unsafe` 的模块。
/// 任何 libc/FFI 调用必须集中在此，对外暴露安全包装。
pub mod sys;
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
