//! 事件处理管道
//!
//! 负责接收原始输出行，调用 EventExtractor 转换为事件，累积元数据。

use super::db_adapter::DbLogEntry;
use super::event::ExecutionEvent;
use super::extractor::EventExtractor;
use super::metadata::ExecutionMetadata;

/// 事件处理管道
///
/// 负责：
/// - 接收原始输出行
/// - 调用 EventExtractor 转换为事件
/// - 累积元数据
/// - 生成下游所需的各类格式
pub struct EventPipeline {
    extractor: Box<dyn EventExtractor>,
    events: Vec<ExecutionEvent>,
}

impl EventPipeline {
    /// 创建新的管道（使用默认提取器）
    pub fn new(executor: impl Into<String>) -> Self {
        Self {
            extractor: Box::new(DefaultExtractor::new(executor)),
            events: Vec::new(),
        }
    }

    /// 使用自定义提取器创建管道
    pub fn with_extractor(extractor: impl EventExtractor + 'static) -> Self {
        Self {
            extractor: Box::new(extractor),
            events: Vec::new(),
        }
    }

    /// 处理一行标准输出
    pub fn feed(&mut self, line: &str) {
        let new_events = self.extractor.extract(line);
        for event in &new_events {
            self.extractor.metadata_mut().update_from(event);
        }
        self.events.extend(new_events);
    }

    /// 处理一行错误输出
    pub fn feed_stderr(&mut self, line: &str) {
        if let Some(event) = self.extractor.extract_stderr(line) {
            self.extractor.metadata_mut().update_from(&event);
            self.events.push(event);
        }
    }

    /// 处理多行输出（批量）
    pub fn feed_batch(&mut self, lines: &[&str]) {
        for line in lines {
            self.feed(line);
        }
    }

    /// 结束处理，生成元数据事件
    pub fn finalize(&mut self) {
        let metadata = self.extractor.metadata().clone();

        // 如果没有 ModelSwitch 事件，但 metadata 中有 model，则生成（兜底）
        let has_model_switch = self
            .events
            .iter()
            .any(|e| matches!(e, ExecutionEvent::ModelSwitch { .. }));
        if !has_model_switch {
            if let Some(model) = &metadata.model {
                self.events.push(ExecutionEvent::ModelSwitch {
                    model: model.clone(),
                });
            }
        }

        // 如果没有 Result 事件，从最后一个非空的 Assistant 事件提取结论
        let has_result = self
            .events
            .iter()
            .any(|e| matches!(e, ExecutionEvent::Result { .. }));
        if !has_result {
            // 从后往前找最后一个非空的 Assistant 内容作为 Result
            if let Some(last_assistant) = self.events.iter().rev().find_map(|e| {
                if let ExecutionEvent::Assistant { content, .. } = e {
                    if !content.trim().is_empty() {
                        Some(content.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }) {
                self.events.push(ExecutionEvent::Result {
                    summary: last_assistant,
                });
            }
        }

        // 生成会话结束事件
        if let Some(session_id) = &metadata.session_id {
            // 检查是否已有 SessionEnd 事件
            let has_session_end = self
                .events
                .iter()
                .any(|e| matches!(e, ExecutionEvent::SessionEnd { .. }));
            if !has_session_end {
                self.events.push(ExecutionEvent::SessionEnd {
                    session_id: session_id.clone(),
                });
            }
        }

        // 生成最终的 tokens 事件（如果之前没有且有数据）
        if metadata.input_tokens > 0 || metadata.output_tokens > 0 {
            let has_tokens = self
                .events
                .iter()
                .any(|e| matches!(e, ExecutionEvent::Tokens { .. }));
            if !has_tokens {
                self.events.push(ExecutionEvent::Tokens {
                    input: metadata.input_tokens,
                    output: metadata.output_tokens,
                    cache_read: Some(metadata.cache_read_tokens),
                    cache_write: Some(metadata.cache_write_tokens),
                });
            }
        }

        // 设置结束时间
        self.extractor.metadata_mut().set_finished_at();
    }

    /// 获取所有已累积的事件
    pub fn events(&self) -> &[ExecutionEvent] {
        &self.events
    }

    /// 获取最后一条事件
    pub fn latest_event(&self) -> Option<&ExecutionEvent> {
        self.events.last()
    }

    /// 获取累积的元数据
    pub fn metadata(&self) -> &ExecutionMetadata {
        self.extractor.metadata()
    }

    /// 获取提取器（可变）
    pub fn extractor_mut(&mut self) -> &mut Box<dyn EventExtractor> {
        &mut self.extractor
    }

    /// 直接推入一个事件（用于测试或特殊场景）
    ///
    /// 注意：此方法会同时更新元数据
    pub fn push_event(&mut self, event: ExecutionEvent) {
        self.extractor.metadata_mut().update_from(&event);
        self.events.push(event);
    }

    /// 获取事件数量转换为数据库格式
    pub fn to_db_logs(&self) -> Vec<DbLogEntry> {
        self.events.iter().map(DbLogEntry::from_event).collect()
    }

    /// 获取事件数量
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// 检查是否有事件
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// 按类型过滤事件
    pub fn filter_events<F>(&self, f: F) -> Vec<&ExecutionEvent>
    where
        F: Fn(&ExecutionEvent) -> bool,
    {
        self.events.iter().filter(|e| f(e)).collect()
    }

    /// 获取所有思考事件
    pub fn thinking_events(&self) -> Vec<&ExecutionEvent> {
        self.filter_events(|e| matches!(e, ExecutionEvent::Thinking { .. }))
    }

    /// 获取所有工具调用事件
    pub fn tool_call_events(&self) -> Vec<&ExecutionEvent> {
        self.filter_events(|e| matches!(e, ExecutionEvent::ToolCall { .. }))
    }

    /// 获取所有工具结果事件
    pub fn tool_result_events(&self) -> Vec<&ExecutionEvent> {
        self.filter_events(|e| matches!(e, ExecutionEvent::ToolResult { .. }))
    }

    /// 获取最终结果（如果有）
    pub fn final_result(&self) -> Option<String> {
        self.events
            .iter()
            .rev()
            .find(|e| matches!(e, ExecutionEvent::Result { .. }))
            .map(|e| match e {
                ExecutionEvent::Result { summary } => summary.clone(),
                _ => e.content_preview(),
            })
    }

    /// 提取会话 ID（从元数据或事件）
    pub fn session_id(&self) -> Option<&str> {
        self.metadata().session_id.as_deref()
    }

    /// 提取模型名称
    pub fn model(&self) -> Option<&str> {
        self.metadata().model.as_deref()
    }
}

/// 测试用的模拟提取器
#[cfg(test)]
mod test_extractor {
    use super::*;

    #[allow(dead_code)]
    pub struct TestExtractor {
        metadata: ExecutionMetadata,
    }

    impl TestExtractor {
        #[allow(dead_code)]
        pub fn new() -> Self {
            Self {
                metadata: ExecutionMetadata::new("test".to_string()),
            }
        }
    }

    impl EventExtractor for TestExtractor {
        fn executor_name(&self) -> &str {
            "test"
        }

        fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
            vec![ExecutionEvent::Info {
                message: line.to_string(),
            }]
        }

        fn metadata(&self) -> &ExecutionMetadata {
            &self.metadata
        }

        fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
            &mut self.metadata
        }
    }
}

/// 默认提取器实现（从 impls 模块导入）
use super::impls::DefaultExtractor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_basic() {
        let mut pipeline = EventPipeline::new("test");

        pipeline.feed("hello world");
        pipeline.feed("error occurred");

        assert_eq!(pipeline.len(), 2);
        assert!(matches!(
            pipeline.latest_event(),
            Some(ExecutionEvent::Info { .. })
        ));
    }

    #[test]
    fn test_pipeline_stderr() {
        let mut pipeline = EventPipeline::new("test");

        pipeline.feed_stderr("ERROR: something failed");

        assert_eq!(pipeline.len(), 1);
        assert!(matches!(
            pipeline.latest_event(),
            Some(ExecutionEvent::Error { .. })
        ));
    }

    #[test]
    fn test_pipeline_tokens() {
        let mut pipeline = EventPipeline::new("test");

        // 直接推入 Tokens 事件
        pipeline.push_event(ExecutionEvent::Tokens {
            input: 100,
            output: 200,
            cache_read: Some(50),
            cache_write: Some(10),
        });

        assert_eq!(pipeline.metadata().input_tokens, 100);
        assert_eq!(pipeline.metadata().output_tokens, 200);
    }

    #[test]
    fn test_finalize_generates_events() {
        let mut pipeline = EventPipeline::new("test");

        // 直接推入 SessionStart 事件
        pipeline.push_event(ExecutionEvent::SessionStart {
            session_id: "test-session-123".to_string(),
        });

        pipeline.finalize();

        // finalize 应该生成 SessionEnd 事件
        assert!(pipeline
            .events()
            .iter()
            .any(|e| matches!(e, ExecutionEvent::SessionEnd { .. })));
    }

    #[test]
    fn test_tool_call_events() {
        let mut pipeline = EventPipeline::new("test");

        // 直接推入 ToolCall 事件
        pipeline.push_event(ExecutionEvent::tool_call("1", "bash", serde_json::json!({})));

        let tool_calls = pipeline.tool_call_events();
        assert_eq!(tool_calls.len(), 1);
    }

    #[test]
    fn test_final_result() {
        let mut pipeline = EventPipeline::new("test");

        pipeline.feed("some output");
        // 直接推入 Result 事件
        pipeline.push_event(ExecutionEvent::result("final answer"));

        assert_eq!(pipeline.final_result(), Some("final answer".to_string()));
    }
}
