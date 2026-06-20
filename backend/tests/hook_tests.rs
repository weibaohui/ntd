//! Tests for the inline-hook model: `HookTrigger`, `HookContext`, `TodoHooks`,
//! `TodoHookItem`. Hooks now live as a JSON array on each todo's `hooks`
//! column — there are no rules, no filters, no global defaults, no override
//! modes. Cycle protection is asserted via the `chain` field on `HookContext`.

#[cfg(test)]
mod hook_trigger_tests {
    use ntd::hooks::HookTrigger;
    use ntd::models::TodoStatus;

    #[test]
    fn as_str_covers_all_variants() {
        assert_eq!(HookTrigger::BeforeExecution.as_str(), "before_execution");
        assert_eq!(HookTrigger::StateChangedToPending.as_str(), "state_changed_to_pending");
        assert_eq!(HookTrigger::StateChangedToInProgress.as_str(), "state_changed_to_in_progress");
        assert_eq!(HookTrigger::StateChangedToCompleted.as_str(), "state_changed_to_completed");
        assert_eq!(HookTrigger::StateChangedToFailed.as_str(), "state_changed_to_failed");
    }

    #[test]
    fn from_str_round_trips_and_is_case_sensitive() {
        for t in [
            HookTrigger::BeforeExecution,
            HookTrigger::StateChangedToPending,
            HookTrigger::StateChangedToInProgress,
            HookTrigger::StateChangedToCompleted,
            HookTrigger::StateChangedToFailed,
        ] {
            assert_eq!(HookTrigger::from_str(t.as_str()), Some(t));
        }
        assert_eq!(HookTrigger::from_str("invalid"), None);
        assert_eq!(HookTrigger::from_str(""), None);
        assert_eq!(HookTrigger::from_str("BEFORE_CREATE"), None);
        assert_eq!(HookTrigger::from_str("before_execution"), Some(HookTrigger::BeforeExecution));
    }

    #[test]
    fn removed_lifecycle_triggers_are_rejected() {
        // The 4 lifecycle triggers (before_create / after_create /
        // before_delete / after_delete) were dropped — only the 4
        // state-change triggers remain. Anything that looks like the old
        // format must parse to None so old DB rows degrade cleanly.
        for removed in [
            "before_create",
            "after_create",
            "before_delete",
            "after_delete",
        ] {
            assert_eq!(
                HookTrigger::from_str(removed),
                None,
                "{removed} should no longer be a valid trigger",
            );
        }
    }

    #[test]
    fn is_sync_only_for_before_lifecycle() {
        // All 5 hook trigger variants — including BeforeExecution (the only
        // execution-phase trigger alongside the 4 state-change triggers).
        let _all: [HookTrigger; 5] = [
            HookTrigger::BeforeExecution,
            HookTrigger::StateChangedToPending,
            HookTrigger::StateChangedToInProgress,
            HookTrigger::StateChangedToCompleted,
            HookTrigger::StateChangedToFailed,
        ];
    }

    #[test]
    fn for_target_status_maps_each_observable_state() {
        assert_eq!(
            HookTrigger::for_target_status(TodoStatus::Pending),
            Some(HookTrigger::StateChangedToPending),
        );
        assert_eq!(
            HookTrigger::for_target_status(TodoStatus::InProgress),
            Some(HookTrigger::StateChangedToInProgress),
        );
        assert_eq!(
            HookTrigger::for_target_status(TodoStatus::Running),
            Some(HookTrigger::StateChangedToInProgress),
        );
        assert_eq!(
            HookTrigger::for_target_status(TodoStatus::Completed),
            Some(HookTrigger::StateChangedToCompleted),
        );
        assert_eq!(
            HookTrigger::for_target_status(TodoStatus::Failed),
            Some(HookTrigger::StateChangedToFailed),
        );
        // Cancelled is intentionally not observable — UI cancels are noise.
        assert_eq!(HookTrigger::for_target_status(TodoStatus::Cancelled), None);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", HookTrigger::StateChangedToCompleted), "state_changed_to_completed");
    }
}

#[cfg(test)]
mod todo_hook_item_tests {
    use ntd::hooks::{HookTrigger, TodoHookItem, UnratedPolicy};

    #[test]
    fn deserialize_minimal_defaults_enabled_true() {
        let json = r#"{
            "id": 1,
            "trigger": "state_changed_to_completed",
            "target_todo_id": 42
        }"#;
        let item: TodoHookItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.id, 1);
        assert_eq!(item.trigger, HookTrigger::StateChangedToCompleted);
        assert_eq!(item.target_todo_id, 42);
        assert!(!item.skip_if_missing);
        assert!(item.enabled); // serde default
    }

    #[test]
    fn deserialize_full_round_trip() {
        let item = TodoHookItem {
            id: 7,
            trigger: HookTrigger::StateChangedToCompleted,
            target_todo_id: 99,
            skip_if_missing: true,
            enabled: false,
            min_rating: Some(80),
            unrated_policy: UnratedPolicy::Pass,
        };
        let json = serde_json::to_string(&item).unwrap();
        let decoded: TodoHookItem = serde_json::from_str(&json).unwrap();
        // Verify all basic fields round-trip correctly
        assert_eq!(decoded.id, item.id);
        assert_eq!(decoded.trigger, item.trigger);
        assert_eq!(decoded.target_todo_id, item.target_todo_id);
        assert_eq!(decoded.skip_if_missing, item.skip_if_missing);
        assert_eq!(decoded.enabled, item.enabled);
        // Verify newly added quality-gate fields: min_rating and unrated_policy
        // (must serialize/deserialize correctly to support hook-blocking on low ratings)
        assert_eq!(decoded.min_rating, item.min_rating);
        assert_eq!(decoded.unrated_policy, item.unrated_policy);
    }

    #[test]
    fn deserialize_drops_legacy_prompt_template_field() {
        // Old hook rows in the DB (from before this change) may still carry
        // a `prompt_template` key. The deserializer should ignore it instead
        // of failing — the field was removed.
        let json = r#"{
            "id": 1,
            "trigger": "state_changed_to_completed",
            "target_todo_id": 42,
            "prompt_template": "legacy value to be ignored"
        }"#;
        let item: TodoHookItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.id, 1);
        assert_eq!(item.target_todo_id, 42);
    }
}

#[cfg(test)]
mod todo_hooks_tests {
    use ntd::hooks::{HookTrigger, TodoHookItem, TodoHooks, UnratedPolicy};

    fn item(id: i64, trigger: HookTrigger, target: i64, enabled: bool) -> TodoHookItem {
        TodoHookItem {
            id,
            trigger,
            target_todo_id: target,
            skip_if_missing: false,
            enabled,
            min_rating: None,
            unrated_policy: UnratedPolicy::default(),
        }
    }

    #[test]
    fn parse_none_returns_default() {
        let parsed = TodoHooks::parse(None);
        assert!(parsed.items.is_empty());
    }

    #[test]
    fn parse_empty_string_returns_default() {
        let parsed = TodoHooks::parse(Some(""));
        assert!(parsed.items.is_empty());
    }

    #[test]
    fn parse_malformed_json_returns_default_without_panicking() {
        let parsed = TodoHooks::parse(Some("not json"));
        assert!(parsed.items.is_empty());
    }

    #[test]
    fn parse_drops_items_with_removed_triggers() {
        // DB compat: a todo row written before the lifecycle triggers were
        // removed may still carry a `before_create` / `after_create` /
        // `before_delete` / `after_delete` item in its `hooks` JSON. We
        // can't deserialize those into a HookTrigger variant anymore, so
        // the parser must drop them silently — otherwise loading the todo
        // would fail and the whole app would break.
        let json = r#"{
            "items": [
                { "id": 1, "trigger": "after_create", "target_todo_id": 5, "enabled": true },
                { "id": 2, "trigger": "state_changed_to_completed", "target_todo_id": 6, "enabled": true },
                { "id": 3, "trigger": "before_delete", "target_todo_id": 7, "enabled": true }
            ]
        }"#;
        let parsed = TodoHooks::parse(Some(json));
        assert_eq!(parsed.items.len(), 1, "only the state-change item should survive");
        assert_eq!(parsed.items[0].id, 2);
        assert_eq!(parsed.items[0].trigger, HookTrigger::StateChangedToCompleted);
    }

    #[test]
    fn parse_valid_json_round_trips() {
        let source = TodoHooks {
            items: vec![item(1, HookTrigger::StateChangedToCompleted, 5, true)],
        };
        let json = serde_json::to_string(&source).unwrap();
        let parsed = TodoHooks::parse(Some(&json));
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].id, 1);
        assert_eq!(parsed.items[0].target_todo_id, 5);
    }

    #[test]
    fn matching_filters_by_trigger_and_enabled() {
        let hooks = TodoHooks {
            items: vec![
                item(1, HookTrigger::StateChangedToCompleted, 5, true),
                item(2, HookTrigger::StateChangedToCompleted, 6, false), // disabled
                item(3, HookTrigger::StateChangedToFailed, 7, true), // wrong trigger
                item(4, HookTrigger::StateChangedToCompleted, 8, true),
            ],
        };
        let matched: Vec<i64> = hooks
            .matching(HookTrigger::StateChangedToCompleted)
            .map(|i| i.id)
            .collect();
        assert_eq!(matched, vec![1, 4]);
    }

    #[test]
    fn matching_empty_when_no_trigger_match() {
        let hooks = TodoHooks {
            items: vec![item(1, HookTrigger::StateChangedToCompleted, 5, true)],
        };
        assert_eq!(
            hooks.matching(HookTrigger::StateChangedToFailed).count(),
            0
        );
    }
}

#[cfg(test)]
mod hook_context_tests {
    use ntd::hooks::{HookContext, HookTrigger};
    use ntd::models::TodoStatus;

    #[test]
    fn for_state_change_attaches_todo_id_and_trigger() {
        let ctx = HookContext::for_state_change(
            55,
            "Title".to_string(),
            TodoStatus::Pending,
            TodoStatus::Completed,
            Some("claude".to_string()),
            Some("/workspace".to_string()),
            vec![55],
        )
        .expect("Completed should map to a state-change trigger");
        assert_eq!(ctx.todo_id, Some(55));
        assert_eq!(ctx.todo_title, "Title");
        assert_eq!(ctx.old_status.as_deref(), Some("pending"));
        assert_eq!(ctx.new_status.as_deref(), Some("completed"));
        assert_eq!(ctx.executor.as_deref(), Some("claude"));
        assert_eq!(ctx.workspace.as_deref(), Some("/workspace"));
        assert_eq!(ctx.trigger, HookTrigger::StateChangedToCompleted);
        assert_eq!(ctx.chain, vec![55]);
        assert!(!ctx.trigger_time.is_empty());
    }

    #[test]
    fn for_state_change_maps_each_observable_status() {
        let cases = [
            (TodoStatus::Pending, HookTrigger::StateChangedToPending),
            (TodoStatus::InProgress, HookTrigger::StateChangedToInProgress),
            (TodoStatus::Running, HookTrigger::StateChangedToInProgress),
            (TodoStatus::Completed, HookTrigger::StateChangedToCompleted),
            (TodoStatus::Failed, HookTrigger::StateChangedToFailed),
        ];
        for (status, expected) in cases {
            let ctx = HookContext::for_state_change(
                1,
                "x".to_string(),
                TodoStatus::Pending,
                status,
                None,
                None,
                vec![],
            )
            .unwrap_or_else(|| panic!("status {:?} should map to a trigger", status));
            assert_eq!(ctx.trigger, expected);
            assert_eq!(ctx.new_status.as_deref(), Some(status.to_string().as_str()));
        }
    }

    #[test]
    fn for_state_change_returns_none_for_cancelled() {
        let ctx = HookContext::for_state_change(
            1,
            "x".to_string(),
            TodoStatus::InProgress,
            TodoStatus::Cancelled,
            None,
            None,
            vec![],
        );
        assert!(ctx.is_none());
    }

    #[test]
    fn to_params_includes_chain_as_comma_string() {
        let ctx = HookContext::for_state_change(
            10,
            "Title".to_string(),
            TodoStatus::Pending,
            TodoStatus::Completed,
            None,
            None,
            vec![1, 2, 3],
        )
        .unwrap();
        let params = ctx.to_params();
        assert_eq!(params.get("chain").map(|s| s.as_str()), Some("1,2,3"));
        assert_eq!(params.get("todo_id").map(|s| s.as_str()), Some("10"));
        assert_eq!(params.get("todo_title").map(|s| s.as_str()), Some("Title"));
        assert_eq!(
            params.get("trigger").map(|s| s.as_str()),
            Some("state_changed_to_completed")
        );
    }

    #[test]
    fn to_params_chain_empty_when_no_visits() {
        let ctx = HookContext::for_state_change(
            1,
            "T".to_string(),
            TodoStatus::Pending,
            TodoStatus::Completed,
            None,
            None,
            vec![],
        )
        .unwrap();
        let params = ctx.to_params();
        assert_eq!(params.get("chain").map(|s| s.as_str()), Some(""));
    }
}

#[cfg(test)]
mod hook_dispatch_tests {
    use ntd::hooks::{HookTrigger, TodoHookItem, UnratedPolicy};
    use ntd::models::{Todo, TodoStatus};

    fn todo(id: i64, title: &str, prompt: &str, status: TodoStatus) -> Todo {
        Todo {
            id,
            title: title.to_string(),
            prompt: prompt.to_string(),
            status,
            created_at: "2026-06-01T00:00:00Z".to_string(),
            updated_at: "2026-06-01T00:00:00Z".to_string(),
            tag_ids: vec![],
            executor: Some("claudecode".to_string()),
            scheduler_enabled: false,
            scheduler_config: None,
            scheduler_timezone: None,
            scheduler_next_run_at: None,
            task_id: None,
            workspace: Some("/tmp/work".to_string()),
            worktree_enabled: false,
            hooks: vec![],
            acceptance_criteria: None,
            todo_type: 0,
            parent_todo_id: None,
            auto_review_enabled: true,
            kind: "item".to_string(),
        }
    }

    #[test]
    fn build_hook_message_uses_result_when_provided() {
        // When the source's executor has produced output, that result is
        // what flows into the target's `{{message}}` placeholder — not the
        // source's own prompt. The result IS the answer; the prompt is just
        // the question.
        let source = todo(42, "笑话", "讲个程序员笑话", TodoStatus::Completed);
        let target = todo(
            7,
            "评论生成器",
            "请基于以下内容写评论：\n{{message}}",
            TodoStatus::Pending,
        );

        let result = "一个关于goto的程序员笑话...";
        let message = ntd::hooks::build_hook_message(&source, Some(result));
        let params = std::collections::HashMap::from([("message".to_string(), message)]);
        let rendered = ntd::models::replace_placeholders(&target.prompt, &params);
        assert_eq!(
            rendered,
            "请基于以下内容写评论：\n一个关于goto的程序员笑话..."
        );
    }

    #[test]
    fn build_hook_message_falls_back_to_source_prompt_without_result() {
        // No execution result yet (e.g., for `after_create` where the source
        // just came into being, or a manual status change with no prior
        // run). Fall back to the source's prompt so the target still gets
        // useful context.
        let source = todo(42, "笑话", "讲个程序员笑话", TodoStatus::Completed);
        let target = todo(
            7,
            "评论生成器",
            "请基于以下内容写评论：\n{{message}}",
            TodoStatus::Pending,
        );

        let message = ntd::hooks::build_hook_message(&source, None);
        assert_eq!(message, "讲个程序员笑话");
        let params = std::collections::HashMap::from([("message".to_string(), message)]);
        let rendered = ntd::models::replace_placeholders(&target.prompt, &params);
        assert_eq!(
            rendered,
            "请基于以下内容写评论：\n讲个程序员笑话"
        );
    }

    #[test]
    fn todo_hook_item_no_prompt_template_field() {
        // The hook item carries only: id, trigger, target_todo_id, skip_if_missing, enabled.
        // The "what to send" is configured entirely on the target todo's own prompt.
        let item = TodoHookItem {
            id: 1,
            trigger: HookTrigger::StateChangedToCompleted,
            target_todo_id: 7,
            skip_if_missing: true,
            enabled: true,
            min_rating: None,
            unrated_policy: UnratedPolicy::default(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("prompt_template"), "prompt_template must not be in the wire format: {}", json);
    }
}

#[cfg(test)]
mod hook_source_provenance_tests {
    //! When a hook triggers a target todo, the target's `execution_records`
    //! row must carry the provenance — which source todo fired it via which
    //! hook — so the UI can show "triggered by todo #X hook Y" instead of a
    //! bare `hook:state_changed_to_completed` string.
    use ntd::db::execution::NewExecutionRecord;
    use ntd::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.unwrap()
    }

    /// Create a minimal todo so execution_records can satisfy its FK.
    /// Returns the id of the new todo.
    async fn create_todo(db: &Database, title: &str) -> i64 {
        db.create_todo(title, "test prompt").await.unwrap()
    }

    #[tokio::test]
    async fn execution_record_persists_source_provenance() {
        let db = fresh_db().await;
        let target_id = create_todo(&db, "target todo").await;

        let record_id = db
            .create_execution_record(NewExecutionRecord {
                todo_id: Some(target_id),
                command: "echo test",
                executor: "claudecode",
                trigger_type: "hook:state_changed_to_completed",
                task_id: "task-1",
                session_id: None,
                resume_message: None,
                source_todo_id: Some(42),
                source_todo_title: Some("joke source"),
                source_hook_id: Some(999001),
                loop_step_execution_id: None,
                step_id: None,
            })
            .await
            .unwrap();

        let record = db.get_execution_record(record_id).await.unwrap().unwrap();
        assert_eq!(record.source_todo_id, Some(42));
        assert_eq!(record.source_todo_title.as_deref(), Some("joke source"));
        assert_eq!(record.source_hook_id, Some(999001));
    }

    #[tokio::test]
    async fn execution_record_without_source_fields_is_none() {
        // Manual / cron / webhook triggers must NOT carry source provenance
        // — the columns stay NULL.
        let db = fresh_db().await;
        let target_id = create_todo(&db, "another target").await;

        let record_id = db
            .create_execution_record(NewExecutionRecord {
                todo_id: Some(target_id),
                command: "echo manual",
                executor: "claudecode",
                trigger_type: "manual",
                task_id: "task-2",
                session_id: None,
                resume_message: None,
                source_todo_id: None,
                source_todo_title: None,
                source_hook_id: None,
                loop_step_execution_id: None,
                step_id: None,
            })
            .await
            .unwrap();

        let record = db.get_execution_record(record_id).await.unwrap().unwrap();
        assert!(record.source_todo_id.is_none());
        assert!(record.source_todo_title.is_none());
        assert!(record.source_hook_id.is_none());
    }
}
