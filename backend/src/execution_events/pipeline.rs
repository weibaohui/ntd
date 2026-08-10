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

    /// 093-B4：从已装箱的 trait 对象构造（注册表工厂返回 `Box<dyn EventExtractor>`）。
    /// 与 `with_extractor` 并存：泛型版服务具名构造点，本版服务注册表查表路径。
    pub fn with_boxed_extractor(extractor: Box<dyn EventExtractor>) -> Self {
        Self {
            extractor,
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

    /// 093-B2：「trim 空行守卫 → 记录簿记 → feed → 返回本次新增切片」的共享骨架。
    ///
    /// 原在 `parse_and_broadcast`（log_capture）与 `parse_for_direct_stream`
    /// （message_debounce）各写一遍；stderr 路径因 atomcode 特判选择
    /// feed/feed_stderr 不同构，刻意不适用本方法。
    pub fn feed_stdout_new(&mut self, line: &str) -> &[ExecutionEvent] {
        // trim 后判空：执行器输出常见"  \n"类空白行，feed 进去只会产生噪声事件；
        // 返回 &[] 让调用方 for 循环天然零迭代，省一个 is_empty 分支
        let line_trimmed = line.trim();
        if line_trimmed.is_empty() {
            return &[];
        }
        // 簿记：feed 前记录事件总数，feed 后切片 [len_before..] 即「本次新增」——
        // 调用方只应处理增量，全量重发会把历史事件重复广播/落库
        let len_before = self.events.len();
        self.feed(line_trimmed);
        &self.events[len_before..]
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
        // 但如果最后一个非空事件已经是 Assistant，则不再生成 Result，避免内容重复
        let has_result = self
            .events
            .iter()
            .any(|e| matches!(e, ExecutionEvent::Result { .. }));
        if !has_result {
            // 检查最后一个非空事件是否已经是 Assistant
            let last_non_empty_is_assistant = self.events.iter().rev().find(|e| {
                match e {
                    ExecutionEvent::Assistant { content, .. } => !content.trim().is_empty(),
                    ExecutionEvent::Result { summary } => !summary.trim().is_empty(),
                    ExecutionEvent::Thinking { content } => !content.trim().is_empty(),
                    ExecutionEvent::ToolCall { .. } => true,
                    ExecutionEvent::ToolResult { .. } => true,
                    _ => false,
                }
            }).map(|e| matches!(e, ExecutionEvent::Assistant { .. })).unwrap_or(false);

            // 只有当最后一个非空事件不是 Assistant 时，才从之前的 Assistant 提取结论
            if !last_non_empty_is_assistant {
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

/// 默认提取器实现（从 impls 模块导入）
use super::impls::DefaultExtractor;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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

    // ─── 093-B2：feed_stdout_new（CodeRabbit 评审补充）───

    /// 空行/纯空白行守卫：不产生事件、不触发送侧逻辑，返回空切片
    #[test]
    fn test_feed_stdout_new_empty_line_returns_empty_slice() {
        let mut pipeline = EventPipeline::new("claudecode");
        // "  \n  " 这类行在旧两处调用点靠手写 guard 拦截，收口后由本方法统一负责
        assert!(pipeline.feed_stdout_new("   \t  ").is_empty());
        assert!(pipeline.events().is_empty(), "空白行不应产生任何事件");
    }

    /// 正常 feed：返回的切片只含本次新增事件（簿记语义），不含历史事件
    #[test]
    fn test_feed_stdout_new_returns_only_new_events() {
        let mut pipeline = EventPipeline::new("claudecode");
        // 第一行喂入一条 assistant 文本，第二次调用只应返回第二行新增的部分
        let first = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"a"}]}}"#;
        let second = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"b"}]}}"#;
        let n_first = pipeline.feed_stdout_new(first).len();
        assert!(n_first > 0, "首行应产生事件");
        let total_after_first = pipeline.events().len();
        let new_slice = pipeline.feed_stdout_new(second);
        assert!(!new_slice.is_empty(), "第二行应产生新增事件");
        // 簿记正确性：新增切片长度 + 之前总数 = 当前总数
        assert_eq!(new_slice.len() + total_after_first, pipeline.events().len());
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
