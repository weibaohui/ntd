//! 各执行器的事件提取器实现
//!
//! 每个执行器对应一个独立的模块，实现 EventExtractor trait。

pub mod default;

pub use default::DefaultExtractor;
