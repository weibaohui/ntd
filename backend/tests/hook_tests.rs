//! Tests for the hook system models and logic

#[cfg(test)]
mod hook_trigger_tests {
    use ntd::hooks::HookTrigger;

    #[test]
    fn test_trigger_as_str() {
        assert_eq!(HookTrigger::BeforeCreate.as_str(), "before_create");
        assert_eq!(HookTrigger::AfterCreate.as_str(), "after_create");
        assert_eq!(HookTrigger::BeforeStatusChange.as_str(), "before_status_change");
        assert_eq!(HookTrigger::AfterStatusChange.as_str(), "after_status_change");
        assert_eq!(HookTrigger::BeforeDelete.as_str(), "before_delete");
        assert_eq!(HookTrigger::AfterDelete.as_str(), "after_delete");
        assert_eq!(HookTrigger::BeforeExecute.as_str(), "before_execute");
    }

    #[test]
    fn test_trigger_from_str() {
        assert_eq!(HookTrigger::from_str("before_create"), Some(HookTrigger::BeforeCreate));
        assert_eq!(HookTrigger::from_str("after_create"), Some(HookTrigger::AfterCreate));
        assert_eq!(HookTrigger::from_str("before_status_change"), Some(HookTrigger::BeforeStatusChange));
        assert_eq!(HookTrigger::from_str("after_status_change"), Some(HookTrigger::AfterStatusChange));
        assert_eq!(HookTrigger::from_str("before_delete"), Some(HookTrigger::BeforeDelete));
        assert_eq!(HookTrigger::from_str("after_delete"), Some(HookTrigger::AfterDelete));
        assert_eq!(HookTrigger::from_str("before_execute"), Some(HookTrigger::BeforeExecute));
    }

    #[test]
    fn test_trigger_from_str_invalid() {
        assert_eq!(HookTrigger::from_str("invalid"), None);
        assert_eq!(HookTrigger::from_str(""), None);
        assert_eq!(HookTrigger::from_str("BEFORE_CREATE"), None); // case sensitive
    }

    #[test]
    fn test_trigger_is_sync() {
        // Sync triggers (before_*)
        assert!(HookTrigger::BeforeCreate.is_sync());
        assert!(HookTrigger::BeforeStatusChange.is_sync());
        assert!(HookTrigger::BeforeDelete.is_sync());
        assert!(HookTrigger::BeforeExecute.is_sync());

        // Async triggers (after_*)
        assert!(!HookTrigger::AfterCreate.is_sync());
        assert!(!HookTrigger::AfterStatusChange.is_sync());
        assert!(!HookTrigger::AfterDelete.is_sync());
    }

    #[test]
    fn test_trigger_display() {
        let trigger = HookTrigger::BeforeCreate;
        assert_eq!(format!("{}", trigger), "before_create");
    }
}

#[cfg(test)]
mod hook_filter_tests {
    use ntd::hooks::HookFilter;

    fn empty_filter() -> HookFilter {
        HookFilter {
            status: vec![],
            title_contains: None,
            tags: vec![],
            executor: None,
        }
    }

    #[test]
    fn test_filter_empty_matches_everything() {
        let filter = empty_filter();
        assert!(filter.matches("Test Todo", "pending", &[], None));
        assert!(filter.matches("Any Title", "completed", &[1, 2, 3], Some("alice")));
    }

    #[test]
    fn test_filter_status_match() {
        let mut filter = empty_filter();
        filter.status = vec!["pending".to_string(), "in_progress".to_string()];

        assert!(filter.matches("Todo", "pending", &[], None));
        assert!(filter.matches("Todo", "in_progress", &[], None));
        assert!(!filter.matches("Todo", "completed", &[], None));
    }

    #[test]
    fn test_filter_status_empty_matches_all() {
        let mut filter = empty_filter();
        filter.status = vec![];

        assert!(filter.matches("Todo", "any_status", &[], None));
    }

    #[test]
    fn test_filter_title_contains_case_insensitive() {
        let mut filter = empty_filter();
        filter.title_contains = Some("important".to_string());

        assert!(filter.matches("This is IMPORTANT task", "pending", &[], None));
        assert!(filter.matches("Important meeting", "pending", &[], None));
        assert!(!filter.matches("This is a task", "pending", &[], None));
    }

    #[test]
    fn test_filter_title_contains_empty_matches_all() {
        let mut filter = empty_filter();
        filter.title_contains = None;

        assert!(filter.matches("Any Title", "pending", &[], None));
    }

    #[test]
    fn test_filter_tags() {
        let mut filter = empty_filter();
        filter.tags = vec![1, 2, 3];

        // Match when todo has any of the filter tags
        assert!(filter.matches("Todo", "pending", &[1], None));
        assert!(filter.matches("Todo", "pending", &[2], None));
        assert!(filter.matches("Todo", "pending", &[1, 4], None));
        assert!(!filter.matches("Todo", "pending", &[4, 5], None));
        assert!(!filter.matches("Todo", "pending", &[], None));
    }

    #[test]
    fn test_filter_tags_empty_matches_all() {
        let mut filter = empty_filter();
        filter.tags = vec![];

        assert!(filter.matches("Todo", "pending", &[1, 2, 3], None));
        assert!(filter.matches("Todo", "pending", &[], None));
    }

    #[test]
    fn test_filter_executor_match() {
        let mut filter = empty_filter();
        filter.executor = Some("claude".to_string());

        assert!(filter.matches("Todo", "pending", &[], Some("claude")));
        assert!(!filter.matches("Todo", "pending", &[], Some("kimi")));
        assert!(!filter.matches("Todo", "pending", &[], None));
    }

    #[test]
    fn test_filter_executor_empty_matches_all() {
        let mut filter = empty_filter();
        filter.executor = None;

        assert!(filter.matches("Todo", "pending", &[], Some("any")));
        assert!(filter.matches("Todo", "pending", &[], None));
    }

    #[test]
    fn test_filter_combined_conditions() {
        let mut filter = empty_filter();
        filter.status = vec!["pending".to_string()];
        filter.tags = vec![1];
        filter.executor = Some("claude".to_string());

        // All conditions must match
        assert!(filter.matches("Todo", "pending", &[1], Some("claude")));
        // Status mismatch
        assert!(!filter.matches("Todo", "completed", &[1], Some("claude")));
        // Tag mismatch
        assert!(!filter.matches("Todo", "pending", &[2], Some("claude")));
        // Executor mismatch
        assert!(!filter.matches("Todo", "pending", &[1], Some("kimi")));
    }
}

#[cfg(test)]
mod hook_context_tests {
    use ntd::hooks::{HookContext, HookTrigger};
    use ntd::models::TodoStatus;

    #[test]
    fn test_context_for_create() {
        let ctx = HookContext::for_create(
            "New Todo".to_string(),
            Some("claude".to_string()),
            Some("/workspace".to_string()),
        );

        assert_eq!(ctx.todo_id, None);
        assert_eq!(ctx.todo_title, "New Todo");
        assert_eq!(ctx.old_status, None);
        assert_eq!(ctx.new_status, Some("pending".to_string()));
        assert_eq!(ctx.executor, Some("claude".to_string()));
        assert_eq!(ctx.workspace, Some("/workspace".to_string()));
        assert_eq!(ctx.task_id, None);
        assert_eq!(ctx.trigger, HookTrigger::BeforeCreate);
        assert!(!ctx.trigger_time.is_empty());
    }

    #[test]
    fn test_context_for_status_change() {
        let ctx = HookContext::for_status_change(
            42,
            "Test Todo".to_string(),
            TodoStatus::Pending,
            TodoStatus::Completed,
            Some("kimi".to_string()),
            Some("/project".to_string()),
        );

        assert_eq!(ctx.todo_id, Some(42));
        assert_eq!(ctx.todo_title, "Test Todo");
        assert_eq!(ctx.old_status, Some("pending".to_string()));
        assert_eq!(ctx.new_status, Some("completed".to_string()));
        assert_eq!(ctx.executor, Some("kimi".to_string()));
        assert_eq!(ctx.workspace, Some("/project".to_string()));
        assert_eq!(ctx.trigger, HookTrigger::BeforeStatusChange);
    }

    #[test]
    fn test_context_for_delete() {
        let ctx = HookContext::for_delete(
            99,
            "Deleted Todo".to_string(),
            TodoStatus::Running,
            Some("claude".to_string()),
            Some("/tmp".to_string()),
        );

        assert_eq!(ctx.todo_id, Some(99));
        assert_eq!(ctx.todo_title, "Deleted Todo");
        assert_eq!(ctx.old_status, Some("running".to_string()));
        assert_eq!(ctx.new_status, None);
        assert_eq!(ctx.trigger, HookTrigger::BeforeDelete);
    }

    #[test]
    fn test_context_for_execute() {
        let ctx = HookContext::for_execute(
            7,
            "Run Task".to_string(),
            TodoStatus::Pending,
            Some("claude".to_string()),
            Some("/workspace".to_string()),
            Some("task_123".to_string()),
        );

        assert_eq!(ctx.todo_id, Some(7));
        assert_eq!(ctx.todo_title, "Run Task");
        assert_eq!(ctx.old_status, Some("pending".to_string()));
        assert_eq!(ctx.new_status, Some("pending".to_string()));
        assert_eq!(ctx.executor, Some("claude".to_string()));
        assert_eq!(ctx.workspace, Some("/workspace".to_string()));
        assert_eq!(ctx.task_id, Some("task_123".to_string()));
        assert_eq!(ctx.trigger, HookTrigger::BeforeExecute);
    }
}

#[cfg(test)]
mod hook_result_tests {
    use ntd::hooks::HookResult;

    #[test]
    fn test_hook_result_success_with_zero_exit_code() {
        let result = HookResult::success(0, "output".to_string(), "".to_string(), 100);
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout, "output");
        assert_eq!(result.stderr, "");
        assert_eq!(result.duration_ms, 100);
        assert_eq!(result.error_msg, None);
    }

    #[test]
    fn test_hook_result_success_with_non_zero_exit_code() {
        let result = HookResult::success(1, "".to_string(), "error".to_string(), 50);
        assert!(!result.success); // success is false when exit_code != 0
        assert_eq!(result.exit_code, Some(1));
        assert_eq!(result.stderr, "error");
    }

    #[test]
    fn test_hook_result_error() {
        let result = HookResult::error("command not found".to_string(), 10);
        assert!(!result.success);
        assert_eq!(result.exit_code, None);
        assert_eq!(result.stdout, "");
        assert_eq!(result.stderr, "");
        assert_eq!(result.duration_ms, 10);
        assert_eq!(result.error_msg, Some("command not found".to_string()));
    }
}

#[cfg(test)]
mod hook_mode_tests {
    use ntd::hooks::HookMode;

    #[test]
    fn test_hook_mode_as_str() {
        assert_eq!(HookMode::Inherit.as_str(), "inherit");
        assert_eq!(HookMode::Custom.as_str(), "custom");
        assert_eq!(HookMode::Disabled.as_str(), "disabled");
    }

    #[test]
    fn test_hook_mode_from_str() {
        assert_eq!(HookMode::from_str("inherit"), Some(HookMode::Inherit));
        assert_eq!(HookMode::from_str("custom"), Some(HookMode::Custom));
        assert_eq!(HookMode::from_str("disabled"), Some(HookMode::Disabled));
    }

    #[test]
    fn test_hook_mode_from_str_invalid() {
        assert_eq!(HookMode::from_str("invalid"), None);
        assert_eq!(HookMode::from_str(""), None);
        assert_eq!(HookMode::from_str("INHERIT"), None); // case sensitive
    }

    #[test]
    fn test_hook_mode_default() {
        let mode = HookMode::default();
        assert_eq!(mode, HookMode::Inherit);
    }
}

#[cfg(test)]
mod global_hook_config_tests {
    use ntd::hooks::GlobalHookConfig;

    #[test]
    fn test_global_hook_config_default() {
        let config = GlobalHookConfig::default();
        assert!(config.enabled);
        assert_eq!(config.default_timeout_secs, 30);
        assert_eq!(config.max_concurrency, 5);
    }

    #[test]
    fn test_global_hook_config_serialization() {
        let config = GlobalHookConfig {
            enabled: true,
            default_timeout_secs: 60,
            max_concurrency: 10,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"enabled\":true"));
        assert!(json.contains("\"default_timeout_secs\":60"));
        assert!(json.contains("\"max_concurrency\":10"));
    }

    #[test]
    fn test_global_hook_config_deserialization() {
        let json = r#"{"enabled":false,"default_timeout_secs":120,"max_concurrency":20}"#;
        let config: GlobalHookConfig = serde_json::from_str(json).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.default_timeout_secs, 120);
        assert_eq!(config.max_concurrency, 20);
    }
}

#[cfg(test)]
mod hook_action_tests {
    use ntd::hooks::HookAction;
    use std::collections::HashMap;

    #[test]
    fn test_hook_action_default_timeout() {
        let action: HookAction = serde_json::from_str(r#"{
            "command": "echo hello",
            "args": [],
            "env": {}
        }"#).unwrap();
        assert_eq!(action.timeout_secs, 30); // default
    }

    #[test]
    fn test_hook_action_custom_timeout() {
        let action: HookAction = serde_json::from_str(r#"{
            "command": "sleep 10",
            "args": ["10"],
            "env": {},
            "timeout_secs": 60
        }"#).unwrap();
        assert_eq!(action.timeout_secs, 60);
    }

    #[test]
    fn test_hook_action_full() {
        let mut env = HashMap::new();
        env.insert("NODE_ENV".to_string(), "production".to_string());

        let action = HookAction {
            command: "node script.js".to_string(),
            args: vec!["--verbose".to_string()],
            env,
            timeout_secs: 120,
        };

        assert_eq!(action.command, "node script.js");
        assert_eq!(action.args, vec!["--verbose"]);
        assert_eq!(action.env.get("NODE_ENV"), Some(&"production".to_string()));
        assert_eq!(action.timeout_secs, 120);
    }
}

#[cfg(test)]
mod hook_rule_tests {
    use ntd::hooks::{HookAction, HookFilter, HookRule, HookTrigger};
    use std::collections::HashMap;

    fn sample_rule() -> HookRule {
        HookRule {
            id: Some(1),
            name: "Test Hook".to_string(),
            description: Some("A test hook".to_string()),
            enabled: true,
            trigger: HookTrigger::BeforeStatusChange,
            filter: HookFilter::default(),
            action: HookAction {
                command: "echo test".to_string(),
                args: vec![],
                env: HashMap::new(),
                timeout_secs: 30,
            },
            is_async: false,
        }
    }

    #[test]
    fn test_hook_rule_serialization() {
        let rule = sample_rule();
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("\"name\":\"Test Hook\""));
        assert!(json.contains("\"trigger\":\"before_status_change\""));
        assert!(json.contains("\"enabled\":true"));
    }

    #[test]
    fn test_hook_rule_deserialization() {
        let json = r#"{
            "id": 5,
            "name": "My Hook",
            "description": null,
            "enabled": true,
            "trigger": "after_create",
            "filter": {},
            "action": {
                "command": "curl http://example.com",
                "args": [],
                "env": {}
            },
            "is_async": true
        }"#;
        let rule: HookRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id, Some(5));
        assert_eq!(rule.name, "My Hook");
        assert_eq!(rule.trigger, HookTrigger::AfterCreate);
        assert!(rule.is_async);
    }

    #[test]
    fn test_hook_rule_without_id() {
        let json = r#"{
            "id": null,
            "name": "No ID Hook",
            "enabled": true,
            "trigger": "before_delete",
            "action": {
                "command": "rm -rf /tmp",
                "args": [],
                "env": {}
            },
            "is_async": false
        }"#;
        let rule: HookRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id, None);
        assert_eq!(rule.name, "No ID Hook");
    }
}

#[cfg(test)]
mod api_request_tests {
    use ntd::hooks::{
        CreateHookRequest, UpdateGlobalHookConfigRequest,
        UpdateHookRequest, UpdateTodoHookRequest, HookLogQuery,
    };

    #[test]
    fn test_create_hook_request_deserialization() {
        let json = r#"{
            "name": "Create Hook",
            "description": "Runs on todo creation",
            "enabled": true,
            "trigger": "after_create",
            "filter": {
                "status": ["pending"]
            },
            "action": {
                "command": "notify.sh",
                "args": ["created"],
                "env": {"CHANNEL": "general"}
            },
            "is_async": true
        }"#;
        let req: CreateHookRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "Create Hook");
        assert_eq!(req.trigger, "after_create");
        assert!(req.enabled);
        assert!(req.is_async);
        assert!(req.filter.is_some());
    }

    #[test]
    fn test_create_hook_request_minimal() {
        let json = r#"{
            "name": "Minimal Hook",
            "trigger": "before_execute",
            "action": {
                "command": "echo done"
            }
        }"#;
        let req: CreateHookRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "Minimal Hook");
        assert!(req.description.is_none());
        assert!(req.filter.is_none());
        assert!(req.is_async); // default is true based on default_async
    }

    #[test]
    fn test_update_hook_request_partial() {
        let json = r#"{
            "enabled": false,
            "name": "Updated Name"
        }"#;
        let req: UpdateHookRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, Some("Updated Name".to_string()));
        assert_eq!(req.enabled, Some(false));
        assert!(req.trigger.is_none());
        assert!(req.filter.is_none());
    }

    #[test]
    fn test_update_global_config_request() {
        let json = r#"{
            "enabled": false,
            "default_timeout_secs": 60,
            "max_concurrency": 20
        }"#;
        let req: UpdateGlobalHookConfigRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.enabled, Some(false));
        assert_eq!(req.default_timeout_secs, Some(60));
        assert_eq!(req.max_concurrency, Some(20));
    }

    #[test]
    fn test_update_todo_hook_request() {
        let json = r#"{
            "hook_mode": "custom",
            "override_enabled": true,
            "rule_ids": [1, 2, 3]
        }"#;
        let req: UpdateTodoHookRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.hook_mode, Some("custom".to_string()));
        assert_eq!(req.override_enabled, Some(true));
        assert_eq!(req.rule_ids, Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_hook_log_query_defaults() {
        let query = HookLogQuery {
            hook_id: None,
            todo_id: None,
            status: None,
            page: 0,
            limit: 50,
        };
        assert!(query.hook_id.is_none());
        assert_eq!(query.limit, 50);
    }

    #[test]
    fn test_hook_log_query_with_filters() {
        let json = r#"{
            "hook_id": 5,
            "todo_id": 10,
            "status": "success",
            "page": 2,
            "limit": 20
        }"#;
        let query: HookLogQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.hook_id, Some(5));
        assert_eq!(query.todo_id, Some(10));
        assert_eq!(query.status, Some("success".to_string()));
        assert_eq!(query.page, 2);
        assert_eq!(query.limit, 20);
    }
}

#[cfg(test)]
mod hook_log_entry_tests {
    use ntd::hooks::HookLogEntry;

    #[test]
    fn test_hook_log_entry_serialization() {
        let entry = HookLogEntry {
            id: 1,
            hook_id: Some(5),
            hook_name: Some("Test Hook".to_string()),
            trigger: "before_create".to_string(),
            todo_id: Some(42),
            args_sent: Some("arg1 arg2".to_string()),
            env_sent: Some("KEY=value".to_string()),
            exit_code: Some(0),
            stdout: Some("output".to_string()),
            stderr: None,
            duration_ms: Some(150),
            success: Some(true),
            error_msg: None,
            created_at: "2026-05-31T10:00:00.000Z".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"hook_name\":\"Test Hook\""));
        assert!(json.contains("\"success\":true"));
    }

    #[test]
    fn test_hook_log_entry_deserialization() {
        let json = r#"{
            "id": 99,
            "hook_id": null,
            "hook_name": null,
            "trigger": "after_delete",
            "todo_id": 7,
            "args_sent": null,
            "env_sent": null,
            "exit_code": null,
            "stdout": null,
            "stderr": null,
            "duration_ms": null,
            "success": null,
            "error_msg": "command failed",
            "created_at": "2026-05-31T12:00:00.000Z"
        }"#;
        let entry: HookLogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.id, 99);
        assert!(entry.hook_id.is_none());
        assert_eq!(entry.error_msg, Some("command failed".to_string()));
    }
}
