//! ntd-connect: 多 channel → 多 executor 的消息桥（Rust 版 cc-connect）。
//!
//! 详细架构与 v1 范围见 `docs/ntd-connect-design.md`。
//! 参考实现：`https://github.com/chenhg5/cc-connect`（Go）。
//!
//! # 模块组织（按 M1 里程碑）
//!
//! - M1.2 ✅：`error` + `types`（基础类型 + Error）
//! - M1.3 ✅：`http`（共享 reqwest client）
//! - M1.4 ✅：`dedup`（LRU + TTL）
//! - M1.5 ✅：`channel` / `agent` / `typing`（核心 trait）
//! - M2：`session` / `dispatcher`（运行时分发）
//! - M3：`platform::feishu`（飞书 channel 实现）
//! - M4：`agent_impl::claude_code`（Claude Code executor 实现）

#![warn(missing_docs)]

pub mod agent;
pub mod channel;
pub mod dedup;
pub mod dispatcher;
pub mod error;
pub mod http;
pub mod platform;
pub mod session;
pub mod types;
pub mod typing;
