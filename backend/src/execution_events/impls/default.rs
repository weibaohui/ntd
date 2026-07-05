//! 默认事件提取器实现
//!
//! 用于无法识别为结构化格式的行，统一作为 Info 事件处理。

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// 默认提取器
///
/// 对于无法识别为结构化格式的行，统一作为 Info 事件处理。
/// 某些执行器可能直接使用此实现，其他执行器应实现自己的提取器。
#[derive(Debug, Clone)]
pub struct DefaultExtractor {
    /// 执行器名称
    name: String,
    /// 元数据
    metadata: ExecutionMetadata,
}

impl DefaultExtractor {
    /// 创建新的默认提取器
    pub fn new(executor: impl Into<String>) -> Self {
        let name = executor.into();
        Self {
            metadata: ExecutionMetadata::new(name.clone()),
            name,
        }
    }
}

impl EventExtractor for DefaultExtractor {
    fn executor_name(&self) -> &str {
        &self.name
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // 尝试解析为 JSON
        if trimmed.starts_with('{') {
            // JSON 行，可能包含结构化信息，但默认提取器不解析
            // 作为 info 处理
            vec![ExecutionEvent::Info {
                message: trimmed.to_string(),
            }]
        } else {
            // 普通文本行
            vec![ExecutionEvent::Info {
                message: trimmed.to_string(),
            }]
        }
    }

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

    fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
        &mut self.metadata
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_line() {
        let mut extractor = DefaultExtractor::new("test");
        assert!(extractor.extract("").is_empty());
        assert!(extractor.extract("   ").is_empty());
    }

    #[test]
    fn test_normal_text() {
        let mut extractor = DefaultExtractor::new("test");
        let events = extractor.extract("hello world");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Info { message } if message == "hello world"));
    }

    #[test]
    fn test_json_line() {
        let mut extractor = DefaultExtractor::new("test");
        let events = extractor.extract(r#"{"type": "assistant", "content": "hi"}"#);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::Info { .. }));
    }

    #[test]
    fn test_stderr_error() {
        let mut extractor = DefaultExtractor::new("test");
        let event = extractor.extract_stderr("ERROR: something failed");
        assert!(event.is_some());
        assert!(matches!(event.unwrap(), ExecutionEvent::Error { .. }));
    }

    #[test]
    fn test_stderr_info() {
        let mut extractor = DefaultExtractor::new("test");
        let event = extractor.extract_stderr("Just some warning");
        assert!(event.is_some());
        assert!(matches!(event.unwrap(), ExecutionEvent::Info { .. }));
    }
}
