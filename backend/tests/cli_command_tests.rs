//! CLI command parsing and behavior tests

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
#[cfg(test)]
mod todo_create_command_tests {
    use clap::Parser;
    use ntd::cli::{Cli, Commands, TodoAction};

    #[test]
    fn test_todo_create_parsing() {
        let cli = Cli::try_parse_from([
            "ntd", "todo", "create", "My Task", "-p", "prompt", "-e", "kimi",
        ])
        .unwrap();
        match cli.command {
            Commands::Todo {
                action:
                    TodoAction::Create {
                        title,
                        prompt,
                        executor,
                        ..
                    },
            } => {
                assert_eq!(title, Some("My Task".to_string()));
                assert_eq!(prompt, Some("prompt".to_string()));
                assert_eq!(executor, Some("kimi".to_string()));
            }
            _ => panic!("Expected Todo::Create"),
        }
    }

    #[test]
    fn test_todo_create_with_schedule() {
        let cli = Cli::try_parse_from([
            "ntd",
            "todo",
            "create",
            "Task",
            "--schedule",
            "*/30 * * * *",
        ])
        .unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::Create { schedule, .. },
            } => {
                assert_eq!(schedule, Some("*/30 * * * *".to_string()));
            }
            _ => panic!("Expected Todo::Create with schedule"),
        }
    }

    #[test]
    fn test_todo_create_with_tags() {
        let cli =
            Cli::try_parse_from(["ntd", "todo", "create", "Task", "--tags", "1,2,3"]).unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::Create { tags, .. },
            } => {
                assert_eq!(tags, Some("1,2,3".to_string()));
            }
            _ => panic!("Expected Todo::Create with tags"),
        }
    }

    #[test]
    fn test_todo_create_with_workspace() {
        let cli =
            Cli::try_parse_from(["ntd", "todo", "create", "Task", "-w", "42"]).unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::Create { workspace_id, .. },
            } => {
                assert_eq!(workspace_id, Some(42));
            }
            _ => panic!("Expected Todo::Create with workspace"),
        }
    }

    #[test]
    fn test_todo_create_with_stdin() {
        let cli = Cli::try_parse_from(["ntd", "todo", "create", "--stdin"]).unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::Create { stdin, title, .. },
            } => {
                assert!(stdin);
                assert!(title.is_none());
            }
            _ => panic!("Expected Todo::Create with stdin"),
        }
    }

    #[test]
    fn test_todo_create_with_file_prompt() {
        let cli = Cli::try_parse_from(["ntd", "todo", "create", "Task", "-f", "/tmp/prompt.txt"])
            .unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::Create { file, .. },
            } => {
                assert_eq!(file, Some("/tmp/prompt.txt".to_string()));
            }
            _ => panic!("Expected Todo::Create with file"),
        }
    }
}

#[cfg(test)]
mod todo_execute_command_tests {
    use ntd::models::ExecuteRequest;

    #[test]
    fn test_execute_request_serialization() {
        let req = ExecuteRequest {
            todo_id: 123,
            message: Some("hello".to_string()),
            executor: Some("kimi".to_string()),
            params: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"todo_id\":123"));
        assert!(json.contains("\"message\":\"hello\""));
        assert!(json.contains("\"executor\":\"kimi\""));
    }

    #[test]
    fn test_execute_request_message_null() {
        let json = r#"{"todo_id": 123, "message": null, "executor": null}"#;
        let req: ExecuteRequest = serde_json::from_str(json).unwrap();
        assert!(req.message.is_none());
        assert!(req.executor.is_none());
    }

    #[test]
    fn test_execute_request_minimal() {
        let req = ExecuteRequest {
            todo_id: 1,
            message: None,
            executor: None,
            params: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"todo_id\":1"));
        assert!(json.contains("\"message\":null"));
    }
}

#[cfg(test)]
mod stop_execution_request_tests {
    #[test]
    fn test_stop_request_deserialization() {
        #[derive(serde::Deserialize)]
        struct StopRequest {
            record_id: i64,
        }

        let json = r#"{"record_id": 42}"#;
        let req: StopRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.record_id, 42);
    }
}

#[cfg(test)]
mod todo_list_command_tests {
    use clap::Parser;
    use ntd::cli::{Cli, Commands, TodoAction};

    #[test]
    fn test_todo_list_parsing() {
        let cli = Cli::try_parse_from(["ntd", "todo", "list"]).unwrap();
        match cli.command {
            Commands::Todo {
                action:
                    TodoAction::List {
                        status,
                        tag,
                        running,
                        search,
                    },
            } => {
                assert!(status.is_none());
                assert!(tag.is_none());
                assert!(!running);
                assert!(search.is_none());
            }
            _ => panic!("Expected Todo::List"),
        }
    }

    #[test]
    fn test_todo_list_with_status_filter() {
        let cli = Cli::try_parse_from(["ntd", "todo", "list", "--status", "completed"]).unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::List { status, .. },
            } => {
                assert_eq!(status, Some("completed".to_string()));
            }
            _ => panic!("Expected Todo::List with status"),
        }
    }

    #[test]
    fn test_todo_list_with_tag_filter() {
        let cli = Cli::try_parse_from(["ntd", "todo", "list", "--tag", "3"]).unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::List { tag, .. },
            } => {
                assert_eq!(tag, Some(3));
            }
            _ => panic!("Expected Todo::List with tag"),
        }
    }

    #[test]
    fn test_todo_list_with_running_filter() {
        let cli = Cli::try_parse_from(["ntd", "todo", "list", "--running"]).unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::List { running, .. },
            } => {
                assert!(running);
            }
            _ => panic!("Expected Todo::List with running"),
        }
    }

    #[test]
    fn test_todo_list_with_search() {
        let cli = Cli::try_parse_from(["ntd", "todo", "list", "-s", "rust"]).unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::List { search, .. },
            } => {
                assert_eq!(search, Some("rust".to_string()));
            }
            _ => panic!("Expected Todo::List with search"),
        }
    }

    #[test]
    fn test_todo_list_combined_filters() {
        let cli = Cli::try_parse_from([
            "ntd",
            "todo",
            "list",
            "--status",
            "pending",
            "--tag",
            "1",
            "--running",
            "--search",
            "bug",
        ])
        .unwrap();
        match cli.command {
            Commands::Todo {
                action:
                    TodoAction::List {
                        status,
                        tag,
                        running,
                        search,
                    },
            } => {
                assert_eq!(status, Some("pending".to_string()));
                assert_eq!(tag, Some(1));
                assert!(running);
                assert_eq!(search, Some("bug".to_string()));
            }
            _ => panic!("Expected Todo::List with combined filters"),
        }
    }

    #[test]
    fn test_todo_get_parsing() {
        let cli = Cli::try_parse_from(["ntd", "todo", "get", "123"]).unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::Get { id },
            } => {
                assert_eq!(id, 123);
            }
            _ => panic!("Expected Todo::Get"),
        }
    }
}

#[cfg(test)]
mod todo_update_command_tests {
    use clap::Parser;
    use ntd::cli::{Cli, Commands, TodoAction};

    #[test]
    fn test_todo_update_parsing() {
        let cli = Cli::try_parse_from([
            "ntd",
            "todo",
            "update",
            "1",
            "--title",
            "New Title",
            "--status",
            "completed",
            "--executor",
            "kimi",
        ])
        .unwrap();
        match cli.command {
            Commands::Todo {
                action:
                    TodoAction::Update {
                        id,
                        title,
                        status,
                        executor,
                        ..
                    },
            } => {
                assert_eq!(id, 1);
                assert_eq!(title, Some("New Title".to_string()));
                assert_eq!(status, Some("completed".to_string()));
                assert_eq!(executor, Some("kimi".to_string()));
            }
            _ => panic!("Expected Todo::Update"),
        }
    }

    #[test]
    fn test_todo_update_with_stdin() {
        let cli = Cli::try_parse_from(["ntd", "todo", "update", "1", "--stdin"]).unwrap();
        match cli.command {
            Commands::Todo {
                action: TodoAction::Update { id, stdin, .. },
            } => {
                assert_eq!(id, 1);
                assert!(stdin);
            }
            _ => panic!("Expected Todo::Update with stdin"),
        }
    }
}

#[cfg(test)]
mod config_parsing_tests {
    use ntd::config::{Config, ExecutorPaths};
    use std::collections::HashMap;

    #[test]
    fn test_executor_paths_default() {
        let paths = ExecutorPaths::default();
        // Default is empty HashMap - actual defaults come from EXECUTORS
        assert!(paths.paths.is_empty());
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.port, 8088);
        assert_eq!(config.host, "0.0.0.0");
        assert!(config.log_level.contains("INFO") || config.log_level == "INFO");
    }

    #[test]
    fn test_config_executor_paths() {
        let mut paths_map = HashMap::new();
        paths_map.insert("opencode".to_string(), "custom-opencode".to_string());
        paths_map.insert("hermes".to_string(), "custom-hermes".to_string());
        paths_map.insert("claudecode".to_string(), "claude".to_string());
        let paths = ExecutorPaths { paths: paths_map };
        assert_eq!(paths.paths.get("opencode"), Some(&"custom-opencode".to_string()));
        assert_eq!(paths.paths.get("hermes"), Some(&"custom-hermes".to_string()));
    }

    #[test]
    fn test_executor_paths_legacy_flat_deserialization() {
        // Test that legacy flat config shape deserializes correctly
        let legacy_json = r#"{"claude_code":"/usr/bin/claude","opencode":"/usr/local/bin/opencode","hermes":"hermes"}"#;
        let paths: ExecutorPaths = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(paths.paths.get("claude_code"), Some(&"/usr/bin/claude".to_string()));
        assert_eq!(paths.paths.get("opencode"), Some(&"/usr/local/bin/opencode".to_string()));
        assert_eq!(paths.paths.get("hermes"), Some(&"hermes".to_string()));
    }

    #[test]
    fn test_executor_paths_new_wrapper_deserialization() {
        // Test that new wrapper shape deserializes correctly
        let new_json = r#"{"paths":{"claudecode":"/custom/claude","opencode":"/custom/opencode"}}"#;
        let paths: ExecutorPaths = serde_json::from_str(new_json).unwrap();
        assert_eq!(paths.paths.get("claudecode"), Some(&"/custom/claude".to_string()));
        assert_eq!(paths.paths.get("opencode"), Some(&"/custom/opencode".to_string()));
    }
}

#[cfg(test)]
mod output_format_tests {
    use clap::Parser;
    use ntd::cli::{Cli, OutputFormat};

    #[test]
    fn test_output_format_json() {
        let cli = Cli::try_parse_from(["ntd", "-o", "json", "todo", "list"]).unwrap();
        assert_eq!(cli.output, OutputFormat::Json);
    }

    #[test]
    fn test_output_format_pretty() {
        let cli = Cli::try_parse_from(["ntd", "-o", "pretty", "todo", "list"]).unwrap();
        assert_eq!(cli.output, OutputFormat::Pretty);
    }

    #[test]
    fn test_output_format_raw() {
        let cli = Cli::try_parse_from(["ntd", "-o", "raw", "todo", "list"]).unwrap();
        assert_eq!(cli.output, OutputFormat::Raw);
    }

    #[test]
    fn test_output_format_default_is_json() {
        let cli = Cli::try_parse_from(["ntd", "todo", "list"]).unwrap();
        assert_eq!(cli.output, OutputFormat::Json);
    }
}

#[cfg(test)]
mod fields_tests {
    use clap::Parser;
    use ntd::cli::Cli;

    #[test]
    fn test_fields_parsing() {
        let cli = Cli::try_parse_from(["ntd", "-f", "id,title,status", "todo", "list"]).unwrap();
        assert_eq!(cli.fields, Some("id,title,status".to_string()));
    }

    #[test]
    fn test_fields_empty() {
        let cli = Cli::try_parse_from(["ntd", "todo", "list"]).unwrap();
        assert_eq!(cli.fields, None);
    }
}

#[cfg(test)]
mod cron_validation_tests {
    use std::str::FromStr;

    #[test]
    fn test_valid_cron_expressions() {
        let expressions = vec![
            "*/30 * * * * *", // every 30 seconds
            "0 */12 * * * *", // every 12 hours
            "0 0 * * * *",    // every hour
            "0 0 9 * * *",    // daily at 9am
            "0 0 0 * * *",    // midnight
        ];

        for expr in expressions {
            let result = cron::Schedule::from_str(expr);
            assert!(result.is_ok(), "Cron expression '{}' should be valid", expr);
        }
    }

    #[test]
    fn test_invalid_cron_expressions() {
        let expressions = vec![
            "invalid", "", "* * *", // too few fields (6 required)
        ];

        for expr in expressions {
            let result = cron::Schedule::from_str(expr);
            assert!(
                result.is_err(),
                "Cron expression '{}' should be invalid",
                expr
            );
        }
    }
}

#[cfg(test)]
mod tag_command_tests {
    use clap::Parser;
    use ntd::cli::{Cli, Commands, TagAction};

    #[test]
    fn test_tag_list_parsing() {
        let cli = Cli::try_parse_from(["ntd", "tag", "list"]).unwrap();
        match cli.command {
            Commands::Tag {
                action: TagAction::List,
            } => {}
            _ => panic!("Expected Tag::List"),
        }
    }

    #[test]
    fn test_tag_create_parsing() {
        let cli = Cli::try_parse_from(["ntd", "tag", "create", "Bugfix", "-c", "#ff0000"]).unwrap();
        match cli.command {
            Commands::Tag {
                action: TagAction::Create { name, color },
            } => {
                assert_eq!(name, "Bugfix");
                assert_eq!(color, "#ff0000");
            }
            _ => panic!("Expected Tag::Create"),
        }
    }

    #[test]
    fn test_tag_create_default_color() {
        let cli = Cli::try_parse_from(["ntd", "tag", "create", "Feature"]).unwrap();
        match cli.command {
            Commands::Tag {
                action: TagAction::Create { name, color },
            } => {
                assert_eq!(name, "Feature");
                assert_eq!(color, "#1890ff");
            }
            _ => panic!("Expected Tag::Create with default color"),
        }
    }

    #[test]
    fn test_tag_delete_parsing() {
        let cli = Cli::try_parse_from(["ntd", "tag", "delete", "5"]).unwrap();
        match cli.command {
            Commands::Tag {
                action: TagAction::Delete { id },
            } => {
                assert_eq!(id, 5);
            }
            _ => panic!("Expected Tag::Delete"),
        }
    }
}

#[cfg(test)]
mod combined_options_tests {
    use clap::Parser;
    use ntd::cli::{Cli, Commands, OutputFormat, TodoAction};

    #[test]
    fn test_full_ai_workflow_list() {
        // AI-friendly command: list todos with raw output, filtered fields, and search
        let cli = Cli::try_parse_from([
            "ntd",
            "--server",
            "http://localhost:8088",
            "-o",
            "raw",
            "-f",
            "id,title,status,executor",
            "todo",
            "list",
            "--status",
            "pending",
            "--search",
            "bug",
        ])
        .unwrap();

        assert_eq!(cli.server, Some("http://localhost:8088".to_string()));
        assert_eq!(cli.output, OutputFormat::Raw);
        assert_eq!(cli.fields, Some("id,title,status,executor".to_string()));

        match cli.command {
            Commands::Todo {
                action: TodoAction::List { status, search, .. },
            } => {
                assert_eq!(status, Some("pending".to_string()));
                assert_eq!(search, Some("bug".to_string()));
            }
            _ => panic!("Expected Todo::List"),
        }
    }

    #[test]
    fn test_raw_fields_combination() {
        let cli =
            Cli::try_parse_from(["ntd", "-o", "raw", "-f", "id", "todo", "get", "42"]).unwrap();
        assert_eq!(cli.output, OutputFormat::Raw);
        assert_eq!(cli.fields, Some("id".to_string()));
    }
}
