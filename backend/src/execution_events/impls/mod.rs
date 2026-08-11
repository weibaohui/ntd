//! 各执行器的事件提取器实现
//!
//! 每个执行器对应一个独立的模块，实现 EventExtractor trait。

pub mod atomcode;
pub mod claude_code;
pub mod codebuddy;
pub mod codewhale;
pub mod codex;
pub mod default;
pub mod hermes;
pub mod kilo;
pub mod kimi;
pub mod mimo;
pub mod mobilecoder;
pub mod opencode;
pub mod pi;
// step 协议族统一提取器（kilo/opencode 的共享实现，096-W2 收敛产物）
pub mod step_protocol;
pub mod zhanlu;

pub use atomcode::AtomcodeExtractor;
pub use claude_code::ClaudeCodeExtractor;
pub use codebuddy::CodebuddyExtractor;
pub use codewhale::CodewhaleExtractor;
pub use codex::CodexExtractor;
pub use default::DefaultExtractor;
pub use hermes::HermesExtractor;
pub use kilo::KiloExtractor;
pub use kimi::KimiExtractor;
pub use mimo::MimoExtractor;
pub use mobilecoder::MobilecoderExtractor;
pub use opencode::OpencodeExtractor;
pub use pi::PiExtractor;
pub use zhanlu::ZhanluExtractor;
