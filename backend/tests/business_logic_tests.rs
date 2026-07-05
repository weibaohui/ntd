//! Business logic tests for prompt fallback behavior and execute handler

// 测试代码允许 unwrap/expect/panic 等写法以简化断言逻辑，统一放宽以下 clippy 检查
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
#[cfg(test)]
mod prompt_fallback_tests {
    use ntd::models::{ExecuteRequest, UpdateTodoRequest, TodoStatus};

    #[test]
    fn test_execute_request_message_none() {
        let req: ExecuteRequest = serde_json::from_str(r#"{"todo_id": 1, "message": null, "executor": null}"#).unwrap();
        assert!(req.message.is_none());
    }

    #[test]
    fn test_execute_request_message_empty_string() {
        let req: ExecuteRequest = serde_json::from_str(r#"{"todo_id": 1, "message": "", "executor": null}"#).unwrap();
        assert!(req.message.is_some());
        assert_eq!(req.message.unwrap(), "");
    }

    #[test]
    fn test_execute_request_message_with_value() {
        let req: ExecuteRequest = serde_json::from_str(r#"{"todo_id": 1, "message": "hello", "executor": "kimi"}"#).unwrap();
        assert!(req.message.is_some());
        assert_eq!(req.message.unwrap(), "hello");
    }

    #[test]
    fn test_update_todo_request_prompt_none() {
        let req: UpdateTodoRequest = serde_json::from_str(r#"{"title": "Test"}"#).unwrap();
        assert!(req.prompt.is_none());
    }

    #[test]
    fn test_update_todo_request_prompt_empty() {
        let req: UpdateTodoRequest = serde_json::from_str(r#"{"title": "Test", "prompt": ""}"#).unwrap();
        assert!(req.prompt.is_some());
        assert_eq!(req.prompt.unwrap(), "");
    }

    #[test]
    fn test_update_todo_request_prompt_with_value() {
        let req: UpdateTodoRequest = serde_json::from_str(r#"{"title": "Test", "prompt": "actual prompt"}"#).unwrap();
        assert!(req.prompt.is_some());
        assert_eq!(req.prompt.unwrap(), "actual prompt");
    }

    #[test]
    fn test_update_todo_request_status() {
        let req: UpdateTodoRequest = serde_json::from_str(r#"{"status": "in_progress"}"#).unwrap();
        assert_eq!(req.status, Some(TodoStatus::InProgress));
    }

    #[test]
    fn test_update_todo_request_all_fields() {
        let json = r#"{
            "title": "Test",
            "prompt": "Do this",
            "status": "running",
            "executor": "claudecode",
            "scheduler_enabled": true,
            "scheduler_config": "0 0 * * *"
        }"#;
        let req: UpdateTodoRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, Some("Test".to_string()));
        assert_eq!(req.prompt, Some("Do this".to_string()));
        assert_eq!(req.status, Some(TodoStatus::Running));
        assert_eq!(req.executor, Some("claudecode".to_string()));
        assert_eq!(req.scheduler_enabled, Some(true));
        assert_eq!(req.scheduler_config, Some("0 0 * * *".to_string()));
    }

    #[test]
    fn test_update_todo_request_partial_update() {
        // Only updating status, other fields should be None
        let req: UpdateTodoRequest = serde_json::from_str(r#"{"status": "completed"}"#).unwrap();
        assert!(req.title.is_none());
        assert!(req.prompt.is_none());
        assert_eq!(req.status, Some(TodoStatus::Completed));
        assert!(req.executor.is_none());
        assert!(req.scheduler_enabled.is_none());
        assert!(req.scheduler_config.is_none());
    }

    #[test]
    fn test_create_todo_request_deserialization() {
        use ntd::models::CreateTodoRequest;
        let json = r#"{
            "title": "Test Todo",
            "prompt": "This is the prompt",
            "executor": "kimi",
            "tag_ids": [1, 2, 3],
            "workspace_id": 1
        }"#;
        let req: CreateTodoRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, "Test Todo");
        assert_eq!(req.prompt, "This is the prompt");
        assert_eq!(req.executor, Some("kimi".to_string()));
        assert_eq!(req.tag_ids, vec![1, 2, 3]);
        assert_eq!(req.workspace_id, 1);
    }

    #[test]
    fn test_create_todo_request_minimal() {
        use ntd::models::CreateTodoRequest;
        let json = r#"{"title": "Minimal Todo", "prompt": "", "workspace_id": 1}"#;
        let req: CreateTodoRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, "Minimal Todo");
        assert!(req.prompt.is_empty());
        assert!(req.executor.is_none());
        assert!(req.tag_ids.is_empty());
        assert_eq!(req.workspace_id, 1);
    }
}

#[cfg(test)]
mod executor_type_tests {
    use ntd::models::ExecutorType;

    #[test]
    fn test_executor_type_to_string() {
        assert_eq!(ExecutorType::Claudecode.to_string(), "claudecode");
        assert_eq!(ExecutorType::Kimi.to_string(), "kimi");
        assert_eq!(ExecutorType::Hermes.to_string(), "hermes");
        assert_eq!(ExecutorType::Opencode.to_string(), "opencode");
        assert_eq!(ExecutorType::Atomcode.to_string(), "atomcode");
        assert_eq!(ExecutorType::Mobilecoder.to_string(), "mobilecoder");
        assert_eq!(ExecutorType::Codebuddy.to_string(), "codebuddy");
        assert_eq!(ExecutorType::Codex.to_string(), "codex");
        // Issue #673: Zhanlu 应当序列化为 "zhanlu"
        assert_eq!(ExecutorType::Zhanlu.to_string(), "zhanlu");
    }

    #[test]
    fn test_parse_executor_type() {
        use ntd::adapters::parse_executor_type;

        assert_eq!(parse_executor_type("claudecode"), Some(ExecutorType::Claudecode));
        assert_eq!(parse_executor_type("kimi"), Some(ExecutorType::Kimi));
        assert_eq!(parse_executor_type("hermes"), Some(ExecutorType::Hermes));
        assert_eq!(parse_executor_type("opencode"), Some(ExecutorType::Opencode));
        assert_eq!(parse_executor_type("atomcode"), Some(ExecutorType::Atomcode));
        assert_eq!(parse_executor_type("mobilecoder"), Some(ExecutorType::Mobilecoder));
        assert_eq!(parse_executor_type("codebuddy"), Some(ExecutorType::Codebuddy));
        assert_eq!(parse_executor_type("codex"), Some(ExecutorType::Codex));
        // Issue #673: parse_executor_type 必须能识别 zhanlu（含别名 zhanlucode / zl）
        assert_eq!(parse_executor_type("zhanlu"), Some(ExecutorType::Zhanlu));
        assert_eq!(parse_executor_type("zhanlucode"), Some(ExecutorType::Zhanlu));
        assert_eq!(parse_executor_type("zl"), Some(ExecutorType::Zhanlu));
    }

    #[test]
    fn test_parse_executor_type_with_whitespace() {
        use ntd::adapters::parse_executor_type;

        assert_eq!(parse_executor_type("kimi  "), Some(ExecutorType::Kimi));
        assert_eq!(parse_executor_type("  claude"), Some(ExecutorType::Claudecode));
    }

    #[test]
    fn test_parse_executor_type_invalid() {
        use ntd::adapters::parse_executor_type;

        assert_eq!(parse_executor_type("invalid"), None);
        assert_eq!(parse_executor_type(""), None);
        assert_eq!(parse_executor_type("nonexistent"), None);
        // Note: the function lowercases input, so "KIMI" becomes "kimi" and matches
    }
}

#[cfg(test)]
mod todo_status_tests {
    use ntd::models::TodoStatus;

    #[test]
    fn test_todo_status_as_str() {
        assert_eq!(TodoStatus::Pending.as_str(), "pending");
        assert_eq!(TodoStatus::InProgress.as_str(), "in_progress");
        assert_eq!(TodoStatus::Completed.as_str(), "completed");
        assert_eq!(TodoStatus::Failed.as_str(), "failed");
        assert_eq!(TodoStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn test_todo_status_from_str() {
        assert_eq!("pending".parse::<TodoStatus>().ok(), Some(TodoStatus::Pending));
        assert_eq!("in_progress".parse::<TodoStatus>().ok(), Some(TodoStatus::InProgress));
        assert_eq!("completed".parse::<TodoStatus>().ok(), Some(TodoStatus::Completed));
        assert_eq!("failed".parse::<TodoStatus>().ok(), Some(TodoStatus::Failed));
        assert_eq!("cancelled".parse::<TodoStatus>().ok(), Some(TodoStatus::Cancelled));
    }

    #[test]
    fn test_todo_status_invalid() {
        assert_eq!("invalid".parse::<TodoStatus>().ok(), None);
    }
}

#[cfg(test)]
mod api_response_tests {
    use ntd::models::ApiResponse;

    #[test]
    fn test_api_response_ok() {
        let response: ApiResponse<i32> = ApiResponse::ok(42);
        assert_eq!(response.code, 0);
        assert_eq!(response.data, Some(42));
        assert_eq!(response.message, "ok");
    }

    #[test]
    fn test_api_response_with_message() {
        let response: ApiResponse<String> = ApiResponse {
            code: 0,
            data: Some("test".to_string()),
            message: "success".to_string(),
        };
        assert_eq!(response.code, 0);
        assert_eq!(response.data.unwrap(), "test");
        assert_eq!(response.message, "success");
    }

    #[test]
    fn test_api_response_error() {
        let response: ApiResponse<()> = ApiResponse {
            code: 40001,
            data: None,
            message: "Not found".to_string(),
        };
        assert_eq!(response.code, 40001);
        assert!(response.data.is_none());
        assert_eq!(response.message, "Not found");
    }
}

#[cfg(test)]
mod execution_record_tests {
    use ntd::models::ExecutionRecord;

    #[test]
    fn test_execution_record_deserialization() {
        let json = r#"{
            "id": 1,
            "todo_id": 10,
            "status": "success",
            "command": "kimi --print -p hello",
            "stdout": "",
            "stderr": "",
            "logs": "[]",
            "result": "done",
            "started_at": "2026-04-30T10:00:00Z",
            "finished_at": "2026-04-30T10:01:00Z",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_read_input_tokens": null,
                "cache_creation_input_tokens": null,
                "total_cost_usd": 0.01,
                "duration_ms": 60000
            },
            "executor": "kimi",
            "model": "kimi",
            "trigger_type": "manual",
            "pid": 12345,
            "task_id": "abc-123",
            "execution_stats": {
                "tool_calls": 5,
                "conversation_turns": 3,
                "thinking_count": 2
            }
        }"#;

        let record: ExecutionRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.id, 1);
        assert_eq!(record.todo_id, 10);
        assert_eq!(record.status, ntd::models::ExecutionStatus::Success);
        assert_eq!(record.executor, Some("kimi".to_string()));
        assert!(record.usage.is_some());
        let usage = record.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    #[test]
    fn test_execution_record_minimal() {
        use ntd::models::ExecutionRecord;

        let json = r#"{
            "id": 1,
            "todo_id": 10,
            "status": "running",
            "command": "kimi -p hello",
            "stdout": "",
            "stderr": "",
            "logs": "[]",
            "result": "",
            "started_at": "2026-04-30T10:00:00Z",
            "finished_at": null,
            "usage": null,
            "executor": "kimi",
            "model": null,
            "trigger_type": "manual",
            "pid": null,
            "task_id": "abc-123",
            "execution_stats": null
        }"#;

        let record: ExecutionRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.id, 1);
        assert_eq!(record.status, ntd::models::ExecutionStatus::Running);
        assert!(record.usage.is_none());
        assert!(record.pid.is_none());
    }
}

#[cfg(test)]
mod execution_stats_tests {
    use ntd::models::ExecutionStats;

    #[test]
    fn test_execution_stats_serialization() {
        let stats = ExecutionStats {
            tool_calls: 10,
            conversation_turns: 5,
            thinking_count: 3,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"tool_calls\":10"));
        assert!(json.contains("\"conversation_turns\":5"));
        assert!(json.contains("\"thinking_count\":3"));
    }

    #[test]
    fn test_execution_stats_deserialization() {
        let json = r#"{"tool_calls": 5, "conversation_turns": 3, "thinking_count": 2}"#;
        let stats: ExecutionStats = serde_json::from_str(json).unwrap();
        assert_eq!(stats.tool_calls, 5);
        assert_eq!(stats.conversation_turns, 3);
        assert_eq!(stats.thinking_count, 2);
    }
}

#[cfg(test)]
mod todo_item_tests {
    use ntd::models::TodoItem;

    #[test]
    fn test_todo_item_deserialization() {
        let json = r#"{"id": "1", "content": "Task 1", "status": "pending"}"#;
        let item: TodoItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.id, Some("1".to_string()));
        assert_eq!(item.content, "Task 1");
        assert_eq!(item.status, "pending");
    }

    #[test]
    fn test_todo_item_minimal() {
        let json = r#"{"content": "Task 1", "status": "pending"}"#;
        let item: TodoItem = serde_json::from_str(json).unwrap();
        assert!(item.id.is_none());
        assert_eq!(item.content, "Task 1");
        assert_eq!(item.status, "pending");
    }

    #[test]
    fn test_todo_item_status_variations() {
        // done variations
        let done = r#"{"content": "t", "status": "done"}"#;
        let item: TodoItem = serde_json::from_str(done).unwrap();
        assert_eq!(item.status, "done");

        let completed = r#"{"content": "t", "status": "completed"}"#;
        let item: TodoItem = serde_json::from_str(completed).unwrap();
        assert_eq!(item.status, "completed");

        // in_progress variations
        let in_progress = r#"{"content": "t", "status": "in_progress"}"#;
        let item: TodoItem = serde_json::from_str(in_progress).unwrap();
        assert_eq!(item.status, "in_progress");
    }
}
