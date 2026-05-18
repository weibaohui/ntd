//! Extended adapter tests for untested code paths

#[cfg(test)]
mod codex_executor_extended_tests {
    use ntd::adapters::codex::CodexExecutor;
    use ntd::adapters::CodeExecutor;
    use ntd::models::ParsedLogEntry;

    #[test]
    fn test_parse_output_line_empty() {
        let executor = CodexExecutor::new("codex".to_string());
        assert!(executor.parse_output_line("").is_none());
        assert!(executor.parse_output_line("   ").is_none());
    }

    #[test]
    fn test_parse_output_line_non_json() {
        let executor = CodexExecutor::new("codex".to_string());
        // Non-JSON input returns None (codex expects JSON)
        assert!(executor.parse_output_line("plain text output").is_none());
    }

    #[test]
    fn test_parse_output_line_old_format_session_configured() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"msg":{"type":"session_configured","model":"gpt-4"}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "system");
        assert!(entry.content.contains("session configured"));
    }

    #[test]
    fn test_parse_output_line_old_format_agent_message() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"msg":{"type":"agent_message","message":"Hello from codex"}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello from codex");
    }

    #[test]
    fn test_parse_output_line_agent_reasoning() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"type":"agent_reasoning","message":"thinking step by step"}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "thinking");
        assert_eq!(entry.content, "thinking step by step");
    }

    #[test]
    fn test_parse_output_line_agent_reasoning_delta() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"type":"agent_reasoning_delta","delta":"more thinking..."}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "thinking");
        assert_eq!(entry.content, "more thinking...");
    }

    #[test]
    fn test_parse_output_line_exec_command_begin() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"type":"exec_command_begin","tool_name":"bash","command":"ls -la"}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert_eq!(entry.tool_name, Some("bash".to_string()));
        assert!(entry.content.contains("ls -la"));
    }

    #[test]
    fn test_parse_output_line_exec_command_end() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"type":"exec_command_end","stdout":"file1\nfile2","exit_code":0}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert!(entry.content.contains("file1"));
        assert!(entry.content.contains("exit_code=0"));
    }

    #[test]
    fn test_parse_output_line_tool_call_with_arguments() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"type":"tool_call","tool_name":"Read","arguments":"file.txt"}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert_eq!(entry.tool_name, Some("Read".to_string()));
    }

    #[test]
    fn test_parse_output_line_error_event() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"type":"error","message":"something went wrong"}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "error");
        assert!(entry.content.contains("something went wrong"));
    }

    #[test]
    fn test_parse_output_line_task_complete() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"type":"task_complete"}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "step_finish");
        assert!(entry.content.contains("finished"));
    }

    #[test]
    fn test_parse_output_line_task_started() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"type":"task_started"}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "system");
    }

    #[test]
    fn test_parse_output_line_turn_completed_with_cost() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50,"total_cost_usd":0.002},"duration_ms":1500}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tokens");
        let usage = executor.get_usage(&[]).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.total_cost_usd, Some(0.002));
        assert_eq!(usage.duration_ms, Some(1500));
    }

    #[test]
    fn test_parse_stderr_line_error() {
        let executor = CodexExecutor::new("codex".to_string());
        let entry = executor.parse_stderr_line("Error: connection failed").unwrap();
        assert_eq!(entry.log_type, "stderr");
        assert!(entry.content.contains("Error"));
    }

    #[test]
    fn test_parse_stderr_line_info() {
        let executor = CodexExecutor::new("codex".to_string());
        let entry = executor.parse_stderr_line("Starting execution...").unwrap();
        assert_eq!(entry.log_type, "info");
    }

    #[test]
    fn test_parse_stderr_line_empty() {
        let executor = CodexExecutor::new("codex".to_string());
        assert!(executor.parse_stderr_line("").is_none());
    }

    #[test]
    fn test_get_final_result_with_thinking() {
        let executor = CodexExecutor::new("codex".to_string());
        let logs = vec![
            ParsedLogEntry::new("thinking", "<think>thinking...</think>"),
            ParsedLogEntry::new("text", "Final answer"),
        ];
        let result = executor.get_final_result(&logs);
        assert_eq!(result, Some("Final answer".to_string()));
    }

    #[test]
    fn test_get_final_result_multiple_texts() {
        let executor = CodexExecutor::new("codex".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "First part"),
            ParsedLogEntry::new("text", "Second part"),
        ];
        let result = executor.get_final_result(&logs);
        assert!(result.is_some());
        assert!(result.unwrap().contains("First part"));
    }
}

#[cfg(test)]
mod hermes_executor_extended_tests {
    use ntd::adapters::hermes::HermesExecutor;
    use ntd::adapters::CodeExecutor;

    #[test]
    fn test_parse_stderr_line_error() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_stderr_line("Error: connection failed").unwrap();
        assert_eq!(entry.log_type, "stderr");
    }

    #[test]
    fn test_parse_stderr_line_error_uppercase() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_stderr_line("ERROR: something failed").unwrap();
        assert_eq!(entry.log_type, "stderr");
    }

    #[test]
    fn test_parse_stderr_line_failed() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_stderr_line("Build failed").unwrap();
        assert_eq!(entry.log_type, "stderr");
    }

    #[test]
    fn test_parse_stderr_line_info() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_stderr_line("Downloading dependencies...").unwrap();
        assert_eq!(entry.log_type, "info");
    }

    #[test]
    fn test_parse_stderr_line_empty() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.parse_stderr_line("").is_none());
    }

    #[test]
    fn test_parse_output_line_session_id_captured() {
        let executor = HermesExecutor::new("hermes".to_string());
        let _ = executor.parse_output_line("session_id: test_session_123");
        // Session ID is stored internally but not directly accessible
        // Just verify it doesn't panic
    }

    #[test]
    fn test_parse_output_line_messages_tool_calls() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_output_line("Messages: 5 (2 user, 3 tool calls)").unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(executor.get_tool_calls_count(), Some(3));
    }

    #[test]
    fn test_parse_output_line_session_capital_s() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_output_line("Session: my_session_id").unwrap();
        assert_eq!(entry.log_type, "info");
    }

    #[test]
    fn test_parse_output_line_skip_separators() {
        let executor = HermesExecutor::new("hermes".to_string());
        // Different separator characters have different behavior
        assert!(executor.parse_output_line("┃").is_some());
        assert!(executor.parse_output_line("╰──").is_none());
        assert!(executor.parse_output_line("━").is_none());
    }

    #[test]
    fn test_check_success() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.check_success(0));
        assert!(!executor.check_success(1));
    }

    #[test]
    fn test_supports_resume() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.supports_resume());
    }

    #[test]
    fn test_extract_session_id_resume_format() {
        let executor = HermesExecutor::new("hermes".to_string());
        let sid = executor.extract_session_id("hermes --resume 20260517_051220_95e4d6");
        assert_eq!(sid, Some("20260517_051220_95e4d6".to_string()));
    }

    #[test]
    fn test_extract_session_id_session_prefix() {
        let executor = HermesExecutor::new("hermes".to_string());
        let sid = executor.extract_session_id("Session: mysession");
        assert_eq!(sid, Some("mysession".to_string()));
    }

    #[test]
    fn test_extract_session_id_lowercase_prefix() {
        let executor = HermesExecutor::new("hermes".to_string());
        let sid = executor.extract_session_id("session_id: abc123");
        assert_eq!(sid, Some("abc123".to_string()));
    }

    #[test]
    fn test_extract_session_id_no_match() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.extract_session_id("random text").is_none());
        assert!(executor.extract_session_id("hermes chat -q test").is_none());
    }

    #[test]
    fn test_command_args_with_session_new() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args_with_session("do something", Some("task_id"), false);
        assert_eq!(args, vec!["chat", "-q", "do something", "--yolo"]);
    }

    #[test]
    fn test_command_args_with_session_resume() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args_with_session("continue", Some("session_123"), true);
        assert_eq!(args, vec!["chat", "-q", "continue", "--resume", "session_123", "--yolo"]);
    }
}

#[cfg(test)]
mod kimi_executor_extended_tests {
    use ntd::adapters::kimi::KimiExecutor;
    use ntd::adapters::CodeExecutor;
    use ntd::models::ParsedLogEntry;

    #[test]
    fn test_parse_output_line_think_only() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"assistant","content":[{"type":"think","think":"Let me think about this"}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "thinking");
        assert!(entry.content.contains("Let me think about this"));
    }

    #[test]
    fn test_parse_output_line_multiple_tool_calls() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"assistant","content":[],"tool_calls":[{"type":"function","id":"call_1","function":{"name":"Shell","arguments":"{\"command\":\"pwd\"}"}},{"type":"function","id":"call_2","function":{"name":"Read","arguments":"{\"file\":\"a.txt\"}"}}]}"#;
        // Only first tool call is returned
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert!(entry.content.contains("Shell"));
    }

    #[test]
    fn test_parse_output_line_assistant_with_text_and_think() {
        let executor = KimiExecutor::new("kimi".to_string());
        // Text comes first even if think appears first in JSON
        let json = r#"{"role":"assistant","content":[{"type":"think","think":"thinking..."},{"type":"text","text":"Final answer"}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Final answer");
    }

    #[test]
    fn test_parse_output_line_tool_result_multiple_items() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"tool","content":[{"type":"text","text":"result1"},{"type":"text","text":"result2"}]}"#;
        // Returns first text item
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert_eq!(entry.content, "result1");
    }

    #[test]
    fn test_parse_stderr_line_tool_streaming() {
        let executor = KimiExecutor::new("kimi".to_string());
        let entry = executor.parse_stderr_line("[tool-streaming] bash running").unwrap();
        assert_eq!(entry.log_type, "tool");
        assert!(entry.content.contains("tool-streaming"));
    }

    #[test]
    fn test_parse_stderr_line_error() {
        let executor = KimiExecutor::new("kimi".to_string());
        let entry = executor.parse_stderr_line("Error: something failed").unwrap();
        assert_eq!(entry.log_type, "stderr");
    }

    #[test]
    fn test_parse_stderr_line_error_uppercase() {
        let executor = KimiExecutor::new("kimi".to_string());
        let entry = executor.parse_stderr_line("ERROR: Critical failure").unwrap();
        assert_eq!(entry.log_type, "stderr");
    }

    #[test]
    fn test_parse_stderr_line_failed() {
        let executor = KimiExecutor::new("kimi".to_string());
        let entry = executor.parse_stderr_line("Build failed").unwrap();
        assert_eq!(entry.log_type, "stderr");
    }

    #[test]
    fn test_parse_stderr_line_info() {
        let executor = KimiExecutor::new("kimi".to_string());
        let entry = executor.parse_stderr_line("Processing request...").unwrap();
        assert_eq!(entry.log_type, "info");
    }

    #[test]
    fn test_parse_stderr_line_skip_resume() {
        let executor = KimiExecutor::new("kimi".to_string());
        assert!(executor.parse_stderr_line("To resume this session: kimi -r abc123").is_none());
    }

    #[test]
    fn test_parse_stderr_line_empty() {
        let executor = KimiExecutor::new("kimi".to_string());
        assert!(executor.parse_stderr_line("").is_none());
    }

    #[test]
    fn test_get_final_result_single_text() {
        let executor = KimiExecutor::new("kimi".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "Hello world"),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("Hello world".to_string()));
    }

    #[test]
    fn test_get_final_result_multiple_texts() {
        let executor = KimiExecutor::new("kimi".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "First"),
            ParsedLogEntry::new("text", "Second"),
        ];
        let result = executor.get_final_result(&logs).unwrap();
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
    }

    #[test]
    fn test_get_final_result_empty() {
        let executor = KimiExecutor::new("kimi".to_string());
        let logs = vec![
            ParsedLogEntry::new("info", "some info"),
        ];
        assert!(executor.get_final_result(&logs).is_none());
    }

    #[test]
    fn test_get_final_result_strips_whitespace() {
        let executor = KimiExecutor::new("kimi".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  hello  "),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello".to_string()));
    }

    #[test]
    fn test_supports_resume() {
        let executor = KimiExecutor::new("kimi".to_string());
        assert!(executor.supports_resume());
    }
}

#[cfg(test)]
mod joinai_executor_extended_tests {
    use ntd::adapters::joinai::JoinaiExecutor;
    use ntd::adapters::CodeExecutor;

    #[test]
    fn test_extract_session_id_from_event() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"step_start","sessionID":"ses_abc123"}"#;
        let session = executor.extract_session_id(line);
        assert_eq!(session, Some("ses_abc123".to_string()));
    }

    #[test]
    fn test_extract_session_id_from_part() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"text","part":{"sessionID":"ses_xyz789"},"text":"hello"}"#;
        let session = executor.extract_session_id(line);
        assert_eq!(session, Some("ses_xyz789".to_string()));
    }

    #[test]
    fn test_extract_session_id_not_found() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"text","text":"hello"}"#;
        let session = executor.extract_session_id(line);
        assert!(session.is_none());
    }

    #[test]
    fn test_extract_session_id_invalid_json() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let session = executor.extract_session_id("not json");
        assert!(session.is_none());
    }

    #[test]
    fn test_supports_resume() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        assert!(executor.supports_resume());
    }

    #[test]
    fn test_extract_session_id_from_real_api_response() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let line = r#"{"type":"step_start","timestamp":1779069382034,"sessionID":"ses_1c73b5ee0ffef5UHX2DuxZj9uF","part":{"id":"prt_e38cc6d8f001nXvt3aI6D6IfT7","sessionID":"ses_1c73b5ee0ffef5UHX2DuxZj9uF","messageID":"msg_e38cb2991001z0vtz9Wng3siRh","type":"step-start","snapshot":"1779069382030"}}"#;
        let session = executor.extract_session_id(line);
        assert_eq!(session, Some("ses_1c73b5ee0ffef5UHX2DuxZj9uF".to_string()));
    }

    #[test]
    fn test_command_args_with_session() {
        let executor = JoinaiExecutor::new("joinai".to_string());
        let args = executor.command_args_with_session("continue", Some("ses_abc123"), true);
        assert!(args.contains(&"-s".to_string()));
        assert!(args.contains(&"ses_abc123".to_string()));
        assert_eq!(args[0], "run");
        assert_eq!(args[1], "--format");
        assert_eq!(args[2], "json");
    }
}

#[cfg(test)]
mod opencode_executor_extended_tests {
    use ntd::adapters::opencode::OpencodeExecutor;
    use ntd::adapters::CodeExecutor;

    #[test]
    fn test_extract_session_id_from_event() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        let line = r#"{"type":"step-start","sessionID":"ses_open_123"}"#;
        let session = executor.extract_session_id(line);
        assert_eq!(session, Some("ses_open_123".to_string()));
    }

    #[test]
    fn test_extract_session_id_from_part() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        let line = r#"{"type":"text","part":{"session_id":"ses_part_456"},"text":"hello"}"#;
        let session = executor.extract_session_id(line);
        assert_eq!(session, Some("ses_part_456".to_string()));
    }

    #[test]
    fn test_extract_session_id_not_found() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        let line = r#"{"type":"text","text":"hello"}"#;
        let session = executor.extract_session_id(line);
        assert!(session.is_none());
    }

    #[test]
    fn test_extract_session_id_invalid_json() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        let session = executor.extract_session_id("not json");
        assert!(session.is_none());
    }

    #[test]
    fn test_supports_resume() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        assert!(executor.supports_resume());
    }

    #[test]
    fn test_command_args_with_session_new() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        let args = executor.command_args_with_session("continue", Some("session_new"), false);
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        // For new session (is_resume=false), it shouldn't add -s flag
        assert!(!args.contains(&"-s".to_string()));
    }

    #[test]
    fn test_command_args_with_session_resume() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        let args = executor.command_args_with_session("continue", Some("existing_session"), true);
        assert!(args.contains(&"-s".to_string()));
        assert!(args.contains(&"existing_session".to_string()));
    }
}

#[cfg(test)]
mod codebuddy_executor_extended_tests {
    use ntd::adapters::codebuddy::CodebuddyExecutor;
    use ntd::adapters::CodeExecutor;

    #[test]
    fn test_parse_output_line_assistant_tool_use() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"pwd"},"id":"tool_1"}]}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "assistant");
        assert!(entry.content.contains("[tool]"));
        assert!(entry.tool_name.is_some());
    }

    #[test]
    fn test_parse_output_line_assistant_multiple_blocks() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let json = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello"},{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "assistant");
        // First block (text) is processed
        assert!(entry.content.contains("Hello"));
    }

    #[test]
    fn test_parse_output_line_empty_content() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        let json = r#"{"type":"assistant","message":{"content":[]}}"#;
        let entry = executor.parse_output_line(json);
        assert!(entry.is_none());
    }

    #[test]
    fn test_supports_resume() {
        let executor = CodebuddyExecutor::new("codebuddy".to_string());
        assert!(!executor.supports_resume());
    }
}