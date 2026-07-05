//! Wiki 文件管理模块。
//!
//! 黑板改为纯文件存储，目录结构：
//! ~/.ntd/workspace/<workspace_id>/wiki/
//! ├── index.md      # 目录页（自动生成）
//! ├── log.md        # 执行日志（追加式）
//! └── topics/
//!     ├── auth-module.md
//!     └── performance.md

mod fs;
mod index;
mod log;

pub use fs::*;
pub use index::*;
pub use log::*;