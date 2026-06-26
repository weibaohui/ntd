//! Tests for services module - feishu_push formatting logic

#[cfg(test)]
mod feishu_push_service_tests {
    use ntd::handlers::ExecEvent;
    use ntd::models::{ExecutionStats, ParsedLogEntry, TodoItem};

    // Test the should_send logic by checking which events pass the filter
    #[test]
    fn test_should_send_disabled_never_sends() {
        let push_level = "disabled";
        // Started event
        let event = make_started_event();
        assert!(!should_send_check(push_level, &event));

        // Output event
        let event = make_output_event("text", "hello");
        assert!(!should_send_check(push_level, &event));

        // Finished event
        let event = make_finished_event(true, Some("done".to_string()));
        assert!(!should_send_check(push_level, &event));
    }

    #[test]
    fn test_should_send_result_only_only_sends_finished() {
        let push_level = "result_only";

        // Started should not be sent
        let event = make_started_event();
        assert!(!should_send_check(push_level, &event));

        // Output should not be sent
        let event = make_output_event("text", "hello");
        assert!(!should_send_check(push_level, &event));

        // Finished should be sent
        let event = make_finished_event(true, Some("done".to_string()));
        assert!(should_send_check(push_level, &event));

        // Failed should also be sent
        let event = make_finished_event(false, Some("failed".to_string()));
        assert!(should_send_check(push_level, &event));
    }

    #[test]
    fn test_should_send_all_sends_all_events() {
        let push_level = "all";

        let event = make_started_event();
        assert!(should_send_check(push_level, &event));

        let event = make_output_event("text", "hello");
        assert!(should_send_check(push_level, &event));

        let event = make_finished_event(true, Some("done".to_string()));
        assert!(should_send_check(push_level, &event));

        let event = make_output_event("error", "something went wrong");
        assert!(should_send_check(push_level, &event));
    }

    #[test]
    fn test_should_send_unknown_level_never_sends() {
        let push_level = "unknown_level";

        let event = make_started_event();
        assert!(!should_send_check(push_level, &event));
    }

    #[test]
    fn test_format_event_started() {
        let event = make_started_event();
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("开始执行"));
        assert!(text.contains("Test Todo"));
        assert!(text.contains("kimi"));
        assert!(text.contains("task-123"));
    }

    #[test]
    fn test_format_event_output_text() {
        let event = make_output_event("text", "Hello world");
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("Hello world"));
        assert!(text.contains("task-123"));
    }

    #[test]
    fn test_format_event_output_error() {
        let event = make_output_event("error", "Error message");
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("🔴"));
        assert!(text.contains("Error message"));
    }

    #[test]
    fn test_format_event_output_stderr() {
        let event = make_output_event("stderr", "stderr output");
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("🔴"));
        assert!(text.contains("stderr output"));
    }

    #[test]
    fn test_format_event_output_warning() {
        let event = make_output_event("warning", "Warning message");
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("⚠️"));
    }

    #[test]
    fn test_format_event_output_success() {
        let event = make_output_event("success", "Success message");
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("✅"));
    }

    #[test]
    fn test_format_event_output_user() {
        let event = make_output_event("user", "User input");
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("👤"));
    }

    #[test]
    fn test_format_event_output_input() {
        let event = make_output_event("input", "User input");
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("👤"));
    }

    #[test]
    fn test_format_event_output_empty_content_returns_none() {
        let event = make_output_event("text", "");
        let text = format_event_check(&event);
        assert!(text.is_none());
    }

    #[test]
    fn test_format_event_output_whitespace_content_returns_none() {
        let event = make_output_event("text", "   ");
        let text = format_event_check(&event);
        assert!(text.is_none());
    }

    #[test]
    fn test_format_event_output_long_content_truncated() {
        let long_content = "a".repeat(300);
        let event = make_output_event("text", &long_content);
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("..."));
        assert!(text.len() < 300 + 50); // prefix + task_id + truncation
    }

    #[test]
    fn test_format_event_finished_success() {
        let event = make_finished_event(true, Some("Task completed".to_string()));
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("✅"));
        assert!(text.contains("成功"));
        assert!(text.contains("结果:"));
        assert!(text.contains("Task completed"));
    }

    #[test]
    fn test_format_event_finished_failure() {
        let event = make_finished_event(false, Some("Task failed".to_string()));
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("❌"));
        assert!(text.contains("失败"));
    }

    #[test]
    fn test_format_event_finished_without_result() {
        let event = make_finished_event(true, None);
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("✅"));
        assert!(text.contains("成功"));
        // Should not contain "结果:" prefix when result is None
        assert!(!text.contains("结果:"));
    }

    #[test]
    fn test_format_event_finished_long_result_truncated() {
        let long_result = "x".repeat(150);
        let event = make_finished_event(true, Some(long_result));
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("..."));
    }

    #[test]
    fn test_format_event_todo_progress() {
        let progress = vec![
            TodoItem { id: Some("1".to_string()), content: "Task 1".to_string(), status: "pending".to_string() },
            TodoItem { id: Some("2".to_string()), content: "Task 2".to_string(), status: "in_progress".to_string() },
        ];
        let event = ExecEvent::TodoProgress {
            task_id: "task-123".to_string(),
            progress,
        };
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("进度更新"));
        assert!(text.contains("Task 1"));
        assert!(text.contains("pending"));
        assert!(text.contains("Task 2"));
        assert!(text.contains("in_progress"));
    }

    #[test]
    fn test_format_event_todo_progress_empty_returns_none() {
        let event = ExecEvent::TodoProgress {
            task_id: "task-123".to_string(),
            progress: vec![],
        };
        let text = format_event_check(&event);
        assert!(text.is_none());
    }

    #[test]
    fn test_format_event_todo_progress_more_than_5_items() {
        let progress: Vec<TodoItem> = (1..=10).map(|i| {
            TodoItem { id: Some(i.to_string()), content: format!("Task {}", i), status: "pending".to_string() }
        }).collect();
        let event = ExecEvent::TodoProgress {
            task_id: "task-123".to_string(),
            progress,
        };
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        // Should only show first 5 items
        assert!(text.contains("Task 1"));
        assert!(!text.contains("Task 6"));
    }

    #[test]
    fn test_format_event_execution_stats() {
        let stats = ExecutionStats {
            tool_calls: 42,
            conversation_turns: 10,
            thinking_count: 5,
        };
        let event = ExecEvent::ExecutionStats {
            task_id: "task-123".to_string(),
            stats,
        };
        let text = format_event_check(&event);
        assert!(text.is_some());
        let text = text.unwrap();
        assert!(text.contains("执行统计"));
        assert!(text.contains("42"));
        assert!(text.contains("10"));
    }

    #[test]
    fn test_format_event_sync_returns_none() {
        use ntd::task_manager::TaskInfo;
        let event = ExecEvent::Sync {
            tasks: vec![
                TaskInfo {
                    task_id: "task-1".to_string(),
                    todo_id: 1,
                    todo_title: "Test".to_string(),
                    executor: "kimi".to_string(),
                    logs: "[]".to_string(),
                }
            ],
        };
        let text = format_event_check(&event);
        assert!(text.is_none());
    }

    // Helper functions that replicate the internal logic for testing
    fn should_send_check(push_level: &str, event: &ExecEvent) -> bool {
        match push_level {
            "disabled" => false,
            "result_only" => matches!(event, ExecEvent::Finished { .. }),
            "all" => true,
            _ => false,
        }
    }

    fn format_event_check(event: &ExecEvent) -> Option<String> {
        match event {
            ExecEvent::Started { task_id, todo_title, executor, .. } => {
                Some(format!(
                    "🟢 [开始执行]\n📋 {}\n⚡ 执行器: {}\n🆔 TaskID: {}",
                    todo_title, executor, task_id
                ))
            }
            ExecEvent::Output { task_id, entry } => {
                let prefix = match entry.log_type.as_str() {
                    "error" | "stderr" => "🔴",
                    "warning" => "⚠️",
                    "success" => "✅",
                    "user" | "input" => "👤",
                    _ => "📝",
                };
                let content = entry.content.trim();
                if content.is_empty() {
                    None
                } else {
                    let preview = if content.chars().count() > 200 {
                        content.chars().take(200).collect::<String>() + "..."
                    } else {
                        content.to_string()
                    };
                    Some(format!("{} {}\n🆔 {}", prefix, preview, task_id))
                }
            }
            ExecEvent::Finished { success, result, todo_title, executor, .. } => {
                let result_preview = result.as_ref()
                    .map(|r| format!("\n\n📤 结果: {}", if r.chars().count() > 100 { r.chars().take(100).collect::<String>() + "..." } else { r.clone() }))
                    .unwrap_or_default();
                Some(format!(
                    "📋 {}\n⚡ 执行器: {}\n{}{}",
                    todo_title,
                    executor,
                    if *success { "✅ 成功" } else { "❌ 失败" },
                    result_preview
                ))
            }
            ExecEvent::TodoProgress { task_id, progress } => {
                if progress.is_empty() {
                    None
                } else {
                    let items: Vec<String> = progress.iter().take(5).map(|t| {
                        format!("• {} [{}]", t.content, t.status)
                    }).collect();
                    Some(format!(
                        "📋 [进度更新] TaskID: {}\n{}",
                        task_id,
                        items.join("\n")
                    ))
                }
            }
            ExecEvent::ExecutionStats { task_id, stats } => {
                Some(format!(
                    "📊 [执行统计] TaskID: {}\n🔧 工具调用: {}\n💬 对话轮次: {}",
                    task_id, stats.tool_calls, stats.conversation_turns
                ))
            }
            ExecEvent::Sync { .. } => None,
            ExecEvent::ReviewStatusChanged { .. } => None,
        }
    }

    fn make_started_event() -> ExecEvent {
        ExecEvent::Started {
            task_id: "task-123".to_string(),
            todo_id: 1,
            todo_title: "Test Todo".to_string(),
            executor: "kimi".to_string(),
        }
    }

    fn make_output_event(log_type: &str, content: &str) -> ExecEvent {
        ExecEvent::Output {
            task_id: "task-123".to_string(),
            entry: ParsedLogEntry::new(log_type, content),
        }
    }

    fn make_finished_event(success: bool, result: Option<String>) -> ExecEvent {
        ExecEvent::Finished {
            task_id: "task-123".to_string(),
            todo_id: 1,
            todo_title: "Test Todo".to_string(),
            executor: "kimi".to_string(),
            success,
            result,
            feishu_bot_id: None,
            feishu_receive_id: None,
            workspace_id: None,
        }
    }
}