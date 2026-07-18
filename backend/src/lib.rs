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
/// 多 Agent 协作提取器：识别执行过程中派生的子 agent，写入 execution_records.agent_runs。
pub mod agent_progress;
/// Wiki 文件管理模块：黑板改为纯文件存储。
pub mod wiki;
/// WorkBuddy 专家系统集成模块
///
/// 完全兼容 WorkBuddy 的 plugin.json + MD 文件格式。
/// 采用纯文件存储 + 内存索引架构。
pub mod expert;
/// Git 同步模块
///
/// 提供从远程 Git 仓库同步内置资源（专家、模板、Skills）的能力。
pub mod git_sync;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../frontend/dist/"]
pub struct Assets;

/// Embedded ntd usage skill files, installed to executor skill directories.
#[derive(RustEmbed)]
#[folder = "../ntd-skills/"]
pub struct NtdSkills;
