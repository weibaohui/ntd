//! 事件提取器 Trait 定义
//!
//! 每个执行器对应一个 EventExtractor 实现，负责将原始输出转换为 ExecutionEvent。

use super::event::ExecutionEvent;
use super::metadata::ExecutionMetadata;

/// 事件提取器 Trait
///
/// # 设计原则
/// - Send + Sync：允许跨线程使用
/// - 无状态或最小状态：提取逻辑应尽可能纯函数化
/// - 返回 Vec：某些行可能产生多个事件（如同时输出文本和 token）
pub trait EventExtractor: Send + Sync {
    /// 执行器类型名称
    fn executor_name(&self) -> &str;

    /// 从原始输出行提取事件列表
    ///
    /// # 参数
    /// - line: 原始输出行（不含换行符）
    ///
    /// # 返回
    /// - Vec<ExecutionEvent>：可能为空（行不产生事件）
    ///
    /// # 注意
    /// 提取器内部状态（如 step_index, session_id 等）可在此方法中更新
    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent>;

    /// 从原始错误输出行提取事件
    ///
    /// 默认实现：将错误行包装为 Error 事件
    fn extract_stderr(&mut self, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        // 检查是否包含 error 关键字
        if trimmed.to_lowercase().contains("error") {
            Some(ExecutionEvent::Error {
                message: trimmed.to_string(),
            })
        } else {
            Some(ExecutionEvent::Info {
                message: trimmed.to_string(),
            })
        }
    }

    /// 获取当前累积的元数据（引用）
    fn metadata(&self) -> &ExecutionMetadata;

    /// 获取当前累积的元数据（可变引用）
    fn metadata_mut(&mut self) -> &mut ExecutionMetadata;

    /// 重置提取器状态（用于新的执行）
    fn reset(&mut self) {
        // 默认实现：只重置 metadata
        *self.metadata_mut() = ExecutionMetadata::new(self.executor_name().to_string());
    }
}

/// 简单的基于闭包的事件提取器
///
/// 用于快速创建不需要内部状态的提取器
pub struct SimpleExtractor<F, G>
where
    F: Fn(&str) -> Vec<ExecutionEvent> + Send + Sync,
    G: Fn(&str) -> Option<ExecutionEvent> + Send + Sync,
{
    name: String,
    extract_fn: F,
    extract_stderr_fn: G,
    metadata: ExecutionMetadata,
}

impl<F, G> SimpleExtractor<F, G>
where
    F: Fn(&str) -> Vec<ExecutionEvent> + Send + Sync,
    G: Fn(&str) -> Option<ExecutionEvent> + Send + Sync,
{
    /// 创建新的简单提取器
    pub fn new(
        name: impl Into<String>,
        extract_fn: F,
        extract_stderr_fn: G,
    ) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            extract_fn,
            extract_stderr_fn,
            metadata: ExecutionMetadata::new(name),
        }
    }
}

impl<F, G> EventExtractor for SimpleExtractor<F, G>
where
    F: Fn(&str) -> Vec<ExecutionEvent> + Send + Sync,
    G: Fn(&str) -> Option<ExecutionEvent> + Send + Sync,
{
    fn executor_name(&self) -> &str {
        &self.name
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        (self.extract_fn)(line)
    }

    fn extract_stderr(&mut self, line: &str) -> Option<ExecutionEvent> {
        (self.extract_stderr_fn)(line)
    }

    fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
        &mut self.metadata
    }
}
