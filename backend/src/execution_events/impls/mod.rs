//! 各执行器的事件提取器实现
//!
//! 每个执行器对应一个独立的模块，实现 EventExtractor trait。

pub mod claude_code;
pub mod default;
pub mod kilo;
pub mod opencode;

pub use claude_code::ClaudeCodeExtractor;
pub use default::DefaultExtractor;
pub use kilo::KiloExtractor;
pub use opencode::OpencodeExtractor;
