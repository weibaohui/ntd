//! Tests for todo progress extraction logic

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
#[cfg(test)]
mod try_extract_todo_progress_tests {
    use ntd::models::ParsedLogEntry;
    use ntd::todo_progress::try_extract_todo_progress;

    fn make_entry(tool_name: &str, json: &str) -> ParsedLogEntry {
        ParsedLogEntry {
            timestamp: "2026-04-30T10:00:00Z".to_string(),
            log_type: "tool_call".to_string(),
            content: "".to_string(),
            tool_name: Some(tool_name.to_string()),
            tool_input_json: Some(json.to_string()),
            usage: None,
        }
    }

    #[test]
    fn test_extract_todos_from_todowrite() {
        let entry = make_entry("todowrite", r#"{"todos":[{"id":"1","content":"Task 1","status":"pending"},{"id":"2","content":"Task 2","status":"completed"}]}"#);
        let todos = try_extract_todo_progress(&entry).unwrap();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].id, Some("1".to_string()));
        assert_eq!(todos[0].content, "Task 1");
        assert_eq!(todos[0].status, "pending");
        assert_eq!(todos[1].id, Some("2".to_string()));
        assert_eq!(todos[1].content, "Task 2");
        assert_eq!(todos[1].status, "completed");
    }

    #[test]
    fn test_extract_todos_from_writetodo() {
        let entry = make_entry("writetodo", r#"{"todos":[{"content":"Test task","status":"in_progress"}]}"#);
        let todos = try_extract_todo_progress(&entry).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].content, "Test task");
        assert_eq!(todos[0].status, "in_progress");
    }

    #[test]
    fn test_extract_todos_from_items_array() {
        let entry = make_entry("settodolist", r#"{"items":[{"content":"Item 1","status":"pending"}]}"#);
        let todos = try_extract_todo_progress(&entry).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].content, "Item 1");
    }

    #[test]
    fn test_extract_with_title_field() {
        let entry = make_entry("todowrite", r#"{"todos":[{"title":"Task from title","status":"pending"}]}"#);
        let todos = try_extract_todo_progress(&entry).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].content, "Task from title");
    }

    #[test]
    fn test_extract_with_text_field() {
        let entry = make_entry("todowrite", r#"{"todos":[{"text":"Task from text","status":"pending"}]}"#);
        let todos = try_extract_todo_progress(&entry).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].content, "Task from text");
    }

    #[test]
    fn test_empty_content_filtered() {
        let entry = make_entry("todowrite", r#"{"todos":[{"content":"Valid task","status":"pending"},{"content":"","status":"pending"}]}"#);
        let todos = try_extract_todo_progress(&entry).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].content, "Valid task");
    }

    #[test]
    fn test_missing_tool_name_returns_none() {
        let mut entry = make_entry("todowrite", r#"{"todos":[{"content":"Task","status":"pending"}]}"#);
        entry.tool_name = None;
        assert!(try_extract_todo_progress(&entry).is_none());
    }

    #[test]
    fn test_non_todo_tool_returns_none() {
        let entry = make_entry("shell", r#"{"todos":[{"content":"Task","status":"pending"}]}"#);
        assert!(try_extract_todo_progress(&entry).is_none());
    }

    #[test]
    fn test_missing_input_json_returns_none() {
        let mut entry = make_entry("todowrite", r#"{"todos":[{"content":"Task","status":"pending"}]}"#);
        entry.tool_input_json = None;
        assert!(try_extract_todo_progress(&entry).is_none());
    }

    #[test]
    fn test_invalid_json_returns_none() {
        let entry = make_entry("todowrite", "not valid json");
        assert!(try_extract_todo_progress(&entry).is_none());
    }

    #[test]
    fn test_no_todos_array_returns_none() {
        let entry = make_entry("todowrite", r#"{"other_field":"value"}"#);
        assert!(try_extract_todo_progress(&entry).is_none());
    }

    #[test]
    fn test_empty_todos_array_returns_none() {
        let entry = make_entry("todowrite", r#"{"todos":[]}"#);
        assert!(try_extract_todo_progress(&entry).is_none());
    }

    #[test]
    fn test_all_items_empty_filtered() {
        // Only strictly empty strings are filtered, whitespace-only content is kept
        let entry = make_entry("todowrite", r#"{"todos":[{"content":"","status":"pending"},{"content":"","status":"pending"}]}"#);
        assert!(try_extract_todo_progress(&entry).is_none());
    }

    #[test]
    fn test_status_done_normalizes_to_completed() {
        let entry = make_entry("todowrite", r#"{"todos":[{"content":"Task","status":"done"}]}"#);
        let todos = try_extract_todo_progress(&entry).unwrap();
        assert_eq!(todos[0].status, "completed");
    }

    #[test]
    fn test_status_in_progress_variations() {
        let variations = vec![
            ("in_progress", "in_progress"),
            ("inprogress", "in_progress"),
            ("in-progress", "in_progress"),
            ("doing", "in_progress"),
            ("active", "in_progress"),
        ];
        for (input, expected) in variations {
            let entry = make_entry("todowrite", &format!(r#"{{"todos":[{{"content":"Task","status":"{}"}}]}}"#, input));
            let todos = try_extract_todo_progress(&entry).unwrap();
            assert_eq!(todos[0].status, expected, "{} should normalize to {}", input, expected);
        }
    }

    #[test]
    fn test_status_cancelled_variations() {
        for input in ["cancelled", "canceled", "abort", "aborted"] {
            let entry = make_entry("todowrite", &format!(r#"{{"todos":[{{"content":"Task","status":"{}"}}]}}"#, input));
            let todos = try_extract_todo_progress(&entry).unwrap();
            assert_eq!(todos[0].status, "cancelled", "{} should normalize to cancelled", input);
        }
    }

    #[test]
    fn test_status_failed_variations() {
        for input in ["failed", "fail", "error"] {
            let entry = make_entry("todowrite", &format!(r#"{{"todos":[{{"content":"Task","status":"{}"}}]}}"#, input));
            let todos = try_extract_todo_progress(&entry).unwrap();
            assert_eq!(todos[0].status, "failed", "{} should normalize to failed", input);
        }
    }

    #[test]
    fn test_status_unknown_defaults_to_pending() {
        let entry = make_entry("todowrite", r#"{"todos":[{"content":"Task","status":"unknown_status"}]}"#);
        let todos = try_extract_todo_progress(&entry).unwrap();
        assert_eq!(todos[0].status, "pending");
    }
}