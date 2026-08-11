//! Opencode 执行器的事件提取器实现
//!
//! Opencode 与 Kilo 使用完全相同的事件格式（hyphenated event types）。
//!
//! 096-W2：Opencode 与 Kilo 的提取器实现已收敛为 `step_protocol::StepProtocolExtractor`
//! （两家协议与实现逐字相同，唯一差异是执行器名），本文件只保留类型别名壳，
//! 保持既有引用路径（`opencode::OpencodeExtractor`）不抖。

pub use super::step_protocol::StepProtocolExtractor as OpencodeExtractor;
