//! MiMo 事件类型别名壳（093-B1：结构体已收敛到 `step_event` 统一模型）。
//!
//! 本文件只保留类型别名再导出，保持既有引用路径零改动。
//! mimo 的 camelCase 键（callID/messageID/sessionID）由统一模型的 serde alias 兼容。
#![allow(deprecated)]
#![allow(missing_docs)]

pub use super::step_event::StepAgentCacheTokens as MimoCacheTokens;
pub use super::step_event::StepAgentEvent as MimoEvent;
pub use super::step_event::StepAgentPart as MimoPart;
pub use super::step_event::StepAgentTokens as MimoTokens;
pub use super::step_event::StepAgentToolInput as MimoToolInput;
pub use super::step_event::StepAgentToolState as MimoToolState;
