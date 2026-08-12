//! Kilo 执行器的事件提取器实现
//!
//! Kilo 复用了 OpenCode 的事件格式（hyphenated event types，例如 step-start、tool-use），
//! 并额外使用 camelCase 字段名（如 sessionID）。
//!
//! 096-W2：Kilo 与 Opencode 的提取器实现已收敛为 `step_protocol::StepProtocolExtractor`
//! （两家协议与实现逐字相同，唯一差异是执行器名），本文件只保留类型别名壳，
//! 保持既有引用路径（`kilo::KiloExtractor`）不抖。

pub use super::step_protocol::StepProtocolExtractor as KiloExtractor;
