use ntd::adapters::CodeExecutor;
use ntd::adapters::kimi::KimiExecutor;
use ntd::adapters::claude_code::ClaudeCodeExecutor;
use ntd::adapters::hermes::HermesExecutor;
use ntd::adapters::opencode::OpencodeExecutor;
use ntd::adapters::atomcode::AtomcodeExecutor;
use ntd::adapters::mobilecoder::MobilecoderExecutor;
use ntd::adapters::codex::CodexExecutor;
use ntd::adapters::zhanlu::ZhanluExecutor;
use ntd::models::{ParsedLogEntry, ExecutorType};

#[cfg(test)]
mod kimi_executor_tests {
    use super::*;

    #[test]
    fn test_kimi_command_args() {
        let executor = KimiExecutor::new("kimi".to_string());
        let args = executor.command_args("say hello");
        assert_eq!(args, vec!["--print", "--output-format", "stream-json", "-p", "say hello"]);
    }

    #[test]
    fn test_kimi_command_args_with_session() {
        let executor = KimiExecutor::new("kimi".to_string());
        let args = executor.command_args_with_session("continue task", Some("abc123"), false);
        assert_eq!(args, vec!["--print", "--output-format", "stream-json", "-p", "continue task", "-S", "abc123"]);
    }

    #[test]
    fn test_kimi_executor_type() {
        let executor = KimiExecutor::new("kimi".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Kimi);
    }

    #[test]
    fn test_kimi_parse_output_line_assistant_text() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"assistant","content":[{"type":"text","text":"Hello world"}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_kimi_parse_output_line_tool_call() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"assistant","content":[],"tool_calls":[{"type":"function","id":"call_1","function":{"name":"Shell","arguments":"{\"command\":\"date\"}"}}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert!(entry.content.contains("Shell"));
    }

    #[test]
    fn test_kimi_parse_output_line_tool_result() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"tool","content":[{"type":"text","text":"Tue Apr 30 10:00:00 2026"}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert_eq!(entry.content, "Tue Apr 30 10:00:00 2026");
    }

    #[test]
    fn test_kimi_parse_output_line_skip_resume() {
        let executor = KimiExecutor::new("kimi".to_string());
        let line = "To resume this session: kimi -r abc123";
        assert!(executor.parse_output_line(line).is_none());
    }

    #[test]
    fn test_kimi_parse_output_line_empty() {
        let executor = KimiExecutor::new("kimi".to_string());
        assert!(executor.parse_output_line("").is_none());
        assert!(executor.parse_output_line("   ").is_none());
    }

    #[test]
    fn test_kimi_parse_output_line_non_json() {
        let executor = KimiExecutor::new("kimi".to_string());
        // Kimi skips non-JSON lines
        let line = "some plain text output";
        let entry = executor.parse_output_line(line);
        assert!(entry.is_none());
    }

    #[test]
    fn test_kimi_parse_output_line_think_content() {
        let executor = KimiExecutor::new("kimi".to_string());
        let json = r#"{"role":"assistant","content":[{"type":"think","think":"Let me think about this..."},{"type":"text","text":"Final answer"}]}"#;
        let entry = executor.parse_output_line(json).unwrap();
        // Text should come before think
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Final answer");
    }

    #[test]
    fn test_kimi_get_final_result() {
        let executor = KimiExecutor::new("kimi".to_string());
        let logs = vec![
            ParsedLogEntry::new("tool_call", "Calling tool"),
            ParsedLogEntry::new("tool_result", "Tool result"),
            ParsedLogEntry::new("text", "Final answer"),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("Final answer".to_string()));
    }

    #[test]
    fn test_kimi_get_final_result_no_text() {
        let executor = KimiExecutor::new("kimi".to_string());
        let logs = vec![
            ParsedLogEntry::new("tool_call", "Calling tool"),
        ];
        assert_eq!(executor.get_final_result(&logs), None);
    }

    #[test]
    fn test_kimi_check_success() {
        let executor = KimiExecutor::new("kimi".to_string());
        // Kimi uses default check_success which only considers 0 as success
        assert!(executor.check_success(0));
        assert!(!executor.check_success(1));
        assert!(!executor.check_success(130)); // SIGINT - not success for default impl
    }
}

#[cfg(test)]
mod codex_executor_tests {
    use super::*;

    #[test]
    fn test_codex_command_args() {
        let executor = CodexExecutor::new("codex".to_string());
        let args = executor.command_args("say hello");
        assert_eq!(
            args,
            vec![
                "exec",
                "--json",
                "--dangerously-bypass-approvals-and-sandbox",
                "--skip-git-repo-check",
                "say hello"
            ]
        );
    }

    #[test]
    fn test_codex_executor_type() {
        let executor = CodexExecutor::new("codex".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Codex);
    }

    #[test]
    fn test_codex_parse_agent_message() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"msg":{"type":"agent_message","message":"done"}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "done");
    }

    #[test]
    fn test_codex_parse_exec_command_begin() {
        let executor = CodexExecutor::new("codex".to_string());
        let json = r#"{"msg":{"type":"exec_command_begin","command":["date"]}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_call");
        assert_eq!(entry.tool_name, Some("exec".to_string()));
        assert!(entry.content.contains("date"));
    }
}

#[cfg(test)]
mod claude_code_executor_tests {
    use super::*;

    #[test]
    fn test_claude_command_args() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let args = executor.command_args("say hello");
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"--verbose".to_string()));
        assert!(args.contains(&"say hello".to_string()));
    }

    #[test]
    fn test_claude_command_args_with_session_new() {
        // 新会话场景（is_resume=false）：故意不传 --session-id，
        // 让 Claude Code 自己生成 session_id，再从 system 事件里提取回写 DB。
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let args = executor.command_args_with_session("continue", Some("session123"), false);
        assert!(!args.contains(&"--session-id".to_string()));
        assert!(!args.contains(&"--resume".to_string()));
        assert!(args.contains(&"continue".to_string()), "原始 message 必须出现在 args 里");
    }

    #[test]
    fn test_claude_command_args_with_session_resume() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let args = executor.command_args_with_session("continue", Some("session123"), true);
        assert!(args.contains(&"--resume".to_string()));
        assert!(args.contains(&"session123".to_string()));
    }

    #[test]
    fn test_claude_executor_type() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Claudecode);
    }

    #[test]
    fn test_claude_parse_output_line_system() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let json = r#"{"type":"system","model":"claude-3-5-sonnet"}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "system");
        assert!(executor.get_model() == Some("claude-3-5-sonnet".to_string()));
    }

    #[test]
    fn test_claude_parse_output_line_assistant_text() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let json = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "assistant");
        assert_eq!(entry.content, "hello");
    }

    #[test]
    fn test_claude_parse_output_line_tool_use() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Shell","input":{"command":"date"}}]}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_use");
        assert_eq!(entry.tool_name, Some("Shell".to_string()));
    }

    #[test]
    fn test_claude_parse_output_line_thinking() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let json = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"thinking..."}]}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "thinking");
        assert_eq!(entry.content, "thinking...");
    }

    #[test]
    fn test_claude_parse_output_line_tool_result() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let json = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"result","is_error":false}]}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "tool_result");
        assert_eq!(entry.content, "result");
    }

    #[test]
    fn test_claude_parse_output_line_result_success() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let json = r#"{"type":"result","result":"success","is_error":false,"usage":{"input_tokens":10,"output_tokens":20}}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "result");
        assert_eq!(entry.content, "success");
        assert!(entry.usage.is_some());
    }

    #[test]
    fn test_claude_parse_output_line_result_error() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let json = r#"{"type":"result","result":"error","is_error":true}"#;
        let entry = executor.parse_output_line(json).unwrap();
        assert_eq!(entry.log_type, "error");
        assert!(entry.content.contains("error"));
    }

    #[test]
    fn test_claude_parse_output_line_empty() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        assert!(executor.parse_output_line("").is_none());
    }

    #[test]
    fn test_claude_check_success() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        assert!(executor.check_success(0));
        assert!(!executor.check_success(1));
        assert!(!executor.check_success(127));
    }
}

#[cfg(test)]
mod hermes_executor_tests {
    use super::*;

    #[test]
    fn test_hermes_command_args() {
        let executor = HermesExecutor::new("hermes".to_string());
        let args = executor.command_args("say hello");
        assert_eq!(args, vec!["chat".to_string(), "-q".to_string(), "say hello".to_string(), "--yolo".to_string()]);
    }

    #[test]
    fn test_hermes_executor_type() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Hermes);
    }

    #[test]
    fn test_hermes_parse_output_line_text() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_output_line("Hello world").unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "Hello world");
    }

    #[test]
    fn test_hermes_parse_output_line_session_id() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_output_line("session_id: abc123").unwrap();
        assert_eq!(entry.log_type, "info");
        assert!(entry.content.contains("session_id"));
    }

    #[test]
    fn test_hermes_parse_output_line_skip_banner() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.parse_output_line("╭─ Hermes ─────────────────").is_none());
        assert!(executor.parse_output_line("│ some text").is_none());
        assert!(executor.parse_output_line("╰────────────────────────").is_none());
    }

    #[test]
    fn test_hermes_parse_output_line_skip_empty() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.parse_output_line("").is_none());
        assert!(executor.parse_output_line("   ").is_none());
        // These are the actual box characters that hermes skips: space, ┃, ╰, ╭, ━
        assert!(executor.parse_output_line("━│╰╭").is_none());
        // Note: em dash (─) is NOT skipped by hermes
    }

    #[test]
    fn test_hermes_parse_output_line_messages_count() {
        let executor = HermesExecutor::new("hermes".to_string());
        let entry = executor.parse_output_line("Messages: 4 (1 user, 2 tool calls)").unwrap();
        // This should be parsed but we can't directly check internal state
        assert_eq!(entry.log_type, "text");
    }

    #[test]
    fn test_hermes_get_final_result_with_text() {
        let executor = HermesExecutor::new("hermes".to_string());
        let logs = vec![
            ParsedLogEntry::new("text", "  hello world  "),
        ];
        assert_eq!(executor.get_final_result(&logs), Some("hello world".to_string()));
    }

    #[test]
    fn test_hermes_check_success() {
        let executor = HermesExecutor::new("hermes".to_string());
        assert!(executor.check_success(0));
        assert!(!executor.check_success(1));
    }
}

#[cfg(test)]
mod opencode_executor_tests {
    use super::*;

    #[test]
    fn test_opencode_executor_type() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Opencode);
    }

    #[test]
    fn test_opencode_command_args() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        let args = executor.command_args("say hello");
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(args.contains(&"--format".to_string()));
        assert!(args.contains(&"json".to_string()));
        assert!(args.contains(&"say hello".to_string()));
    }

    #[test]
    fn test_opencode_check_success() {
        let executor = OpencodeExecutor::new("opencode".to_string());
        assert!(executor.check_success(0));
        assert!(!executor.check_success(1));
    }
}

// Issue #673: Zhanlu 测试
// 行为与 opencode 完全一致：相同的命令行参数、相同的 JSON 输出格式、相同的退出码语义。
#[cfg(test)]
mod zhanlu_executor_tests {
    use super::*;

    #[test]
    fn test_zhanlu_executor_type() {
        let executor = ZhanluExecutor::new("zl".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Zhanlu);
    }

    #[test]
    fn test_zhanlu_command_args() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let args = executor.command_args("say hello");
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(args.contains(&"--format".to_string()));
        assert!(args.contains(&"json".to_string()));
        assert!(args.contains(&"say hello".to_string()));
    }

    #[test]
    fn test_zhanlu_check_success() {
        let executor = ZhanluExecutor::new("zl".to_string());
        assert!(executor.check_success(0));
        assert!(!executor.check_success(1));
    }

    #[test]
    fn test_zhanlu_supports_resume() {
        let executor = ZhanluExecutor::new("zl".to_string());
        assert!(executor.supports_resume());
    }

    #[test]
    fn test_zhanlu_parse_step_start() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"step_start","timestamp":1700000000000}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_start");
    }

    #[test]
    fn test_zhanlu_parse_text() {
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"text","timestamp":1700000000000,"part":{"type":"text","text":"hi from zhanlu"}}"#;
        let entry = executor.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "text");
        assert_eq!(entry.content, "hi from zhanlu");
    }

    #[test]
    fn test_zhanlu_extract_session_id_from_hyphenated_format() {
        // Issue #673 要求与 opencode 输出一致，所以 sessionID 也是 camelCase
        let executor = ZhanluExecutor::new("zl".to_string());
        let line = r#"{"type":"step-start","timestamp":1700000000000,"sessionID":"ses_zhanlu_001"}"#;
        assert_eq!(executor.extract_session_id(line), Some("ses_zhanlu_001".to_string()));
    }

    /// 行为对齐检查：与 opencode 完全一致地解析相同的 JSON 输入，应当产出相同的 ParsedLogEntry。
    /// 这是 Issue #673「输出 JSON 格式也一致」的端到端证据。
    #[test]
    fn test_zhanlu_matches_opencode_on_same_json() {
        let opencode = OpencodeExecutor::new("opencode".to_string());
        let zhanlu = ZhanluExecutor::new("zl".to_string());

        let line = r#"{"type":"tool-use","timestamp":1700000000000,"part":{"type":"tool","tool":"bash","state":{"status":"completed","input":{"description":"echo"},"output":"ok"}}}"#;
        let o = opencode.parse_output_line(line).unwrap();
        let z = zhanlu.parse_output_line(line).unwrap();
        assert_eq!(o.log_type, z.log_type);
        assert_eq!(o.content, z.content);
        assert_eq!(o.tool_name, z.tool_name);
    }
}

#[cfg(test)]
mod atomcode_executor_tests {
    use super::*;

    #[test]
    fn test_atomcode_executor_type() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Atomcode);
    }

    #[test]
    fn test_atomcode_command_args() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        let args = executor.command_args("say hello");
        // atomcode uses: -v -p <message>
        assert!(args.contains(&"-v".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"say hello".to_string()));
    }

    #[test]
    fn test_atomcode_check_success() {
        let executor = AtomcodeExecutor::new("atomcode".to_string());
        assert!(executor.check_success(0));
        assert!(!executor.check_success(1));
    }
}

#[cfg(test)]
mod mobilecoder_executor_tests {
    use super::*;

    #[test]
    fn test_mobilecoder_executor_type() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        assert_eq!(executor.executor_type(), ExecutorType::Mobilecoder);
    }

    #[test]
    fn test_mobilecoder_command_args() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        let args = executor.command_args("say hello");
        // mobilecoder uses: run --format json <message>
        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"--format".to_string()));
        assert!(args.contains(&"json".to_string()));
        assert!(args.contains(&"say hello".to_string()));
    }

    #[test]
    fn test_mobilecoder_check_success() {
        let executor = MobilecoderExecutor::new("mobile".to_string());
        assert!(executor.check_success(0));
        assert!(!executor.check_success(1));
    }
}

#[cfg(test)]
mod executor_registry_tests {
    use ntd::adapters::ExecutorRegistry;
    use ntd::adapters::kimi::KimiExecutor;
    use ntd::adapters::claude_code::ClaudeCodeExecutor;
    use ntd::models::ExecutorType;

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = ExecutorRegistry::new();
        registry.register(KimiExecutor::new("kimi".to_string())).await;
        registry.register(ClaudeCodeExecutor::new("claude".to_string())).await;

        let kimi = registry.get(ExecutorType::Kimi).await;
        assert!(kimi.is_some());
        assert_eq!(kimi.unwrap().executor_type(), ExecutorType::Kimi);
    }

    #[tokio::test]
    async fn test_get_default() {
        let registry = ExecutorRegistry::new();
        registry.register(KimiExecutor::new("kimi".to_string())).await;
        registry.register(ClaudeCodeExecutor::new("claude".to_string())).await;

        // Default is Claudecode
        let default = registry.get_default().await;
        assert!(default.is_some());
        assert_eq!(default.unwrap().executor_type(), ExecutorType::Claudecode);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let registry = ExecutorRegistry::new();
        let result = registry.get(ExecutorType::Kimi).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_executors() {
        let registry = ExecutorRegistry::new();
        registry.register(KimiExecutor::new("kimi".to_string())).await;
        registry.register(ClaudeCodeExecutor::new("claude".to_string())).await;

        let executors = registry.list_executors().await;
        assert!(executors.contains(&ExecutorType::Kimi));
        assert!(executors.contains(&ExecutorType::Claudecode));
    }
}

#[cfg(test)]
mod parsed_log_entry_tests {
    use ntd::models::ParsedLogEntry;

    #[test]
    fn test_new_helper() {
        let entry = ParsedLogEntry::new("info", "test message");
        assert_eq!(entry.log_type, "info");
        assert_eq!(entry.content, "test message");
    }

    #[test]
    fn test_info_helper() {
        let entry = ParsedLogEntry::info("info message".to_string());
        assert_eq!(entry.log_type, "info");
        assert_eq!(entry.content, "info message");
    }

    #[test]
    fn test_error_helper() {
        let entry = ParsedLogEntry::error("error message".to_string());
        assert_eq!(entry.log_type, "error");
        assert_eq!(entry.content, "error message");
    }

    #[test]
    fn test_stderr_helper() {
        let entry = ParsedLogEntry::stderr("stderr message".to_string());
        assert_eq!(entry.log_type, "stderr");
        assert_eq!(entry.content, "stderr message");
    }
}
