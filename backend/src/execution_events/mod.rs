//! 执行反馈统一事件模块
//!
//! 提供统一的事件抽象层，将各执行器的原始输出转换为结构化的 ExecutionEvent。
//!
//! # 核心类型
//! - [`ExecutionEvent`]: 统一的事件类型枚举
//! - [`ExecutionMetadata`]: 执行元数据
//! - [`EventExtractor`]: 事件提取器 trait
//! - [`EventPipeline`]: 事件处理管道
//!
//! # 数据流
//! ```text
//! Executor Output → EventPipeline → ExecutionEvent[]
//!                            ↓
//!                     ExecutionMetadata
//!                            ↓
//!              ┌─────────────┼─────────────┐
//!              ↓             ↓             ↓
//!           WebSocket      飞书         数据库
//! ```

pub mod event;
pub mod metadata;
pub mod extractor;
pub mod pipeline;
pub mod db_adapter;
pub mod impls;

pub use event::ExecutionEvent;
pub use metadata::ExecutionMetadata;
pub use extractor::EventExtractor;
pub use pipeline::EventPipeline;
pub use db_adapter::DbLogEntry;
