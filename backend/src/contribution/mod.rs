//! 专家贡献模块：把本地专家以 PR 形式提交回官方 GitCode 仓库。
//!
//! 职责划分：
//! - `gitcode.rs`：GitCode 平台常量、PAT 验证（`verify_pat`）。
//! - `pat.rs`：PAT 持久化到 `~/.ntd/contribution_pat.json`（0600）。
//!
//! 提交动作由前端「ActionButton + 提示词」驱动：AI 执行器读取本地 PAT、
//! 调用 GitCode API 完成 fork/建分支/写文件/建 PR，后端不再实现确定性调用。
//! Handler 层在 `handlers/contribution.rs`。

pub mod gitcode;
pub mod pat;
