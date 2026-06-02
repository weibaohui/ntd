pub mod adapters;
pub mod cli;
pub mod config;
pub mod daemon;
pub mod db;
pub mod executor_service;
pub mod feishu;
pub mod handlers;
pub mod hooks;
pub mod models;
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
