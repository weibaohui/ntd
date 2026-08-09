//! Zhanlu 事件类型别名壳（093-B1：结构体已收敛到 `step_event` 统一模型）。
//!
//! 本文件只保留类型别名再导出，保持既有引用路径（adapters / execution_events::impls）
//! 零改动。新代码请直接使用 `super::step_event` 的 canonical 类型。
#![allow(deprecated)]
#![allow(missing_docs)]

pub use super::step_event::StepAgentCacheTokens as ZhanluAgentCacheTokens;
pub use super::step_event::StepAgentEvent as ZhanluAgentEvent;
pub use super::step_event::StepAgentPart as ZhanluAgentPart;
pub use super::step_event::StepAgentTokens as ZhanluAgentTokens;
pub use super::step_event::StepAgentToolInput as ZhanluAgentToolInput;
pub use super::step_event::StepAgentToolState as ZhanluAgentToolState;
