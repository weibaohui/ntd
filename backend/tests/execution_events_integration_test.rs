//! 执行事件模块集成测试
//!
//! 测试 EventPipeline 与各提取器的完整集成。

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
#[cfg(test)]
mod tests {
    use ntd::execution_events::*;

    /// 测试 Kilo 管道的完整流程
    #[test]
    fn test_kilo_pipeline_full_flow() {
        let mut pipeline = EventPipeline::with_extractor(KiloExtractor::new());

        // step-start
        pipeline.feed(r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_test123"}"#);

        // text message
        pipeline.feed(r#"{"type":"text","timestamp":1777471473405,"sessionID":"ses_test123","part":{"type":"text","text":"I need to list files."}}"#);

        // tool-use
        pipeline.feed(r#"{"type":"tool-use","timestamp":1777471473410,"part":{"type":"tool_use","tool":"bash","state":{"status":"running","input":{"description":"list files","command":"ls -la"},"output":null}}}"#);

        // tool-result
        pipeline.feed(r#"{"type":"tool-result","timestamp":1777471473415,"part":{"type":"tool_result","call_id":"call_1","state":{"status":"completed","output":"file1.txt\nfile2.txt"}}}"#);

        // step-finish
        pipeline.feed(r#"{"type":"step-finish","timestamp":1777471473420,"part":{"type":"step-finish","reason":"stop","tokens":{"total":500,"input":300,"output":200,"reasoning":0,"cache":{"read":50,"write":20}},"cost":0.005}}"#);

        // finalize
        pipeline.finalize();

        // 验证结果
        let events = pipeline.events();
        assert!(events.len() >= 7); // SessionStart + StepStart + Assistant + ToolCall + ToolResult + StepFinish + Tokens + Cost + SessionEnd

        // 验证元数据
        let metadata = pipeline.metadata();
        assert_eq!(metadata.session_id.as_deref(), Some("ses_test123"));
        assert_eq!(metadata.input_tokens, 300);
        assert_eq!(metadata.output_tokens, 200);
        assert_eq!(metadata.cache_read_tokens, 50);
        assert_eq!(metadata.cache_write_tokens, 20);
        assert_eq!(metadata.cost_usd, 0.005);

        // 验证工具调用数量
        let tool_calls = pipeline.tool_call_events();
        assert_eq!(tool_calls.len(), 1);

        // 验证最终结果
        let session_end = events.iter().find(|e| matches!(e, ExecutionEvent::SessionEnd { .. }));
        assert!(session_end.is_some());
    }

    /// 测试 Claude Code 管道的完整流程
    #[test]
    fn test_claude_code_pipeline_full_flow() {
        let mut pipeline = EventPipeline::with_extractor(ClaudeCodeExtractor::new());

        // system
        pipeline.feed(r#"{"type":"system","subtype":"initialized","session_id":"ses_claude456","model":"claude-sonnet-4"}"#);

        // assistant with thinking and text
        pipeline.feed(r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"thinking","thinking":"Let me analyze this..."},{"type":"text","text":"Here's the plan."}]}}"#);

        // tool use
        pipeline.feed(r#"{"type":"assistant","message":{"id":"msg_2","type":"message","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"echo hello"}}]}}"#);

        // tool result (user message with tool_result)
        pipeline.feed(r#"{"type":"user","message":{"id":"msg_3","type":"message","role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"hello\n","is_error":false}]}}"#);

        // result
        pipeline.feed(r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":2500,"result":"Task done.","total_cost_usd":0.02,"usage":{"input_tokens":800,"output_tokens":400,"cache_read_input_tokens":100,"cache_creation_input_tokens":50}}"#);

        // finalize
        pipeline.finalize();

        // 验证结果
        let events = pipeline.events();
        assert!(events.len() >= 10);

        // 验证元数据
        let metadata = pipeline.metadata();
        assert_eq!(metadata.session_id.as_deref(), Some("ses_claude456"));
        assert_eq!(metadata.model.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(metadata.input_tokens, 800);
        assert_eq!(metadata.output_tokens, 400);
        assert_eq!(metadata.cache_read_tokens, 100);
        assert_eq!(metadata.cache_write_tokens, 50);
        assert_eq!(metadata.cost_usd, 0.02);
        assert_eq!(metadata.duration_ms, 2500);

        // 验证思考事件
        let thinking = pipeline.thinking_events();
        assert!(!thinking.is_empty());

        // 验证最终结果
        let final_result = pipeline.final_result();
        assert_eq!(final_result, Some("Task done.".to_string()));
    }

    /// 测试 Opencode 管道的完整流程
    #[test]
    fn test_opencode_pipeline_full_flow() {
        let mut pipeline = EventPipeline::with_extractor(OpencodeExtractor::new());

        // step-start
        pipeline.feed(r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_open789"}"#);

        // text with reasoning
        pipeline.feed(r#"{"type":"text","timestamp":1777471473405,"sessionID":"ses_open789","part":{"type":"text","text":"Let me solve this.","reason":"Analyzing the problem..."}}"#);

        // step-finish
        pipeline.feed(r#"{"type":"step-finish","timestamp":1777471473420,"part":{"type":"step-finish","reason":"stop","tokens":{"total":300,"input":200,"output":100,"reasoning":50,"cache":{"read":30,"write":10}},"cost":0.003}}"#);

        // finalize
        pipeline.finalize();

        // 验证结果
        let events = pipeline.events();
        assert!(events.len() >= 6);

        // 验证元数据
        let metadata = pipeline.metadata();
        assert_eq!(metadata.session_id.as_deref(), Some("ses_open789"));
        assert_eq!(metadata.input_tokens, 200);
        assert_eq!(metadata.output_tokens, 100);
    }

    /// 测试数据库适配转换
    #[test]
    fn test_db_adapter_conversion() {
        let mut pipeline = EventPipeline::with_extractor(KiloExtractor::new());

        pipeline.feed(r#"{"type":"step-start","sessionID":"ses_db1"}"#);
        pipeline.feed(r#"{"type":"tool-use","part":{"type":"tool_use","tool":"bash","state":{"input":{"command":"ls"}}}}"#);
        pipeline.feed(r#"{"type":"step-finish","part":{"tokens":{"total":100,"input":60,"output":40,"reasoning":0,"cache":{"read":0,"write":0}},"cost":0.001}}"#);

        let db_logs = pipeline.to_db_logs();
        assert!(!db_logs.is_empty());

        // 验证 log_type 值统一
        for log in &db_logs {
            let valid_types = [
                "session_start", "step_start", "tool_call",
                "step_finish", "tokens", "cost", "session_end",
            ];
            assert!(
                valid_types.contains(&log.log_type.as_str()),
                "未知的 log_type: {}",
                log.log_type
            );
        }

        // 验证 tool_call 有 tool_name
        let tool_call = db_logs.iter().find(|l| l.log_type == "tool_call");
        assert!(tool_call.is_some());
        assert_eq!(tool_call.unwrap().tool_name.as_deref(), Some("bash"));

        // 验证 tokens 有 usage
        let tokens_log = db_logs.iter().find(|l| l.log_type == "tokens");
        assert!(tokens_log.is_some());
        assert!(tokens_log.unwrap().usage.is_some());
    }

    /// 测试空管道 finalize
    #[test]
    fn test_empty_pipeline_finalize() {
        let mut pipeline = EventPipeline::new("test");
        pipeline.finalize();

        assert!(pipeline.is_empty());
    }

    /// 测试批量 feed
    #[test]
    fn test_feed_batch() {
        let mut pipeline = EventPipeline::new("test");

        let lines: Vec<&str> = vec!["line 1", "line 2", "line 3"];
        pipeline.feed_batch(&lines);

        assert_eq!(pipeline.len(), 3);
    }

    /// 测试 stderr 处理
    #[test]
    fn test_stderr_handling() {
        let mut pipeline = EventPipeline::new("test");

        pipeline.feed_stderr("ERROR: something went wrong");
        pipeline.feed_stderr("Just a warning");

        assert_eq!(pipeline.len(), 2);

        let events = pipeline.events();
        assert!(matches!(&events[0], ExecutionEvent::Error { .. }));
        assert!(matches!(&events[1], ExecutionEvent::Info { .. }));
    }

    /// 测试 content_preview 长度限制
    #[test]
    fn test_content_preview_truncation() {
        let long_thinking = "x".repeat(500);
        let event = ExecutionEvent::Thinking {
            content: long_thinking,
        };
        assert_eq!(event.content_preview().len(), 200);

        let long_result = "y".repeat(1000);
        let event = ExecutionEvent::Result {
            summary: long_result,
        };
        assert_eq!(event.content_preview().len(), 500);
    }

    /// 测试 is_interactive 和 is_message 辅助方法
    #[test]
    fn test_event_category_helpers() {
        // 交互类型
        assert!(ExecutionEvent::Thinking { content: "test".into() }.is_interactive());
        assert!(ExecutionEvent::ToolCall {
            id: "1".into(),
            name: "bash".into(),
            input: serde_json::json!({}),
        }
        .is_interactive());
        assert!(ExecutionEvent::ToolResult {
            call_id: "1".into(),
            output: "test".into(),
            is_error: false,
        }
        .is_interactive());

        // 消息类型
        assert!(ExecutionEvent::Assistant {
            content: "hi".into(),
            thinking: None,
            message_id: None,
        }
        .is_message());
        assert!(ExecutionEvent::User { content: "hello".into() }.is_message());
        assert!(ExecutionEvent::System { message: "boot".into() }.is_message());

        // 非交互非消息
        assert!(!ExecutionEvent::Info { message: "log".into() }.is_interactive());
        assert!(!ExecutionEvent::Info { message: "log".into() }.is_message());
    }
}
