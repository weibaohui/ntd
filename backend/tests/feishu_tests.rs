//! Tests for Feishu/Lark module - codec, message types, and SDK error handling

#[cfg(test)]
mod codec_tests {
    use ntd::feishu::codec::{decode_message_content, encode_text_message};

    #[test]
    fn test_decode_text_message_content() {
        let content = r#"{"text":"Hello world"}"#;
        let result = decode_message_content(content, "text");
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_decode_text_message_with_escaped_characters() {
        let content = r#"{"text":"Hello\nworld"}"#;
        let result = decode_message_content(content, "text");
        assert_eq!(result, "Hello\nworld");
    }

    #[test]
    fn test_decode_text_message_fallback_on_invalid_json() {
        let content = "plain text content";
        let result = decode_message_content(content, "text");
        assert_eq!(result, "plain text content");
    }

    #[test]
    fn test_decode_text_message_missing_text_field() {
        let content = r#"{"other":"value"}"#;
        let result = decode_message_content(content, "text");
        assert_eq!(result, content);
    }

    #[test]
    fn test_decode_text_message_empty_text_field() {
        let content = r#"{"text":""}"#;
        let result = decode_message_content(content, "text");
        assert_eq!(result, "");
    }

    #[test]
    fn test_decode_non_text_message_returns_content() {
        let content = "some content";
        let result = decode_message_content(content, "image");
        assert_eq!(result, "some content");
    }

    #[test]
    fn test_decode_non_text_message_with_json() {
        let content = r#"{"image_key":"img_v1_xxx"}"#;
        let result = decode_message_content(content, "image");
        assert_eq!(result, content);
    }

    #[test]
    fn test_encode_text_message() {
        let json = encode_text_message("Hello world");
        assert!(json.contains("\"text\":\"Hello world\""));
    }

    #[test]
    fn test_encode_text_message_empty() {
        let json = encode_text_message("");
        assert!(json.contains("\"text\":\"\""));
    }

    #[test]
    fn test_encode_text_message_with_newline() {
        let json = encode_text_message("line1\nline2");
        assert!(json.contains("line1\\nline2"));
    }
}

// Note: ChannelMessage does not implement serde::Deserialize, so we cannot test
// JSON deserialization directly. The struct is only used internally for message handling.

#[cfg(test)]
mod lark_api_error_tests {
    use ntd::feishu::sdk::error::LarkAPIError;

    #[test]
    fn test_lark_api_error_display_io() {
        let err = LarkAPIError::IOErr("connection refused".to_string());
        let display = format!("{}", err);
        assert!(display.contains("IO error"));
        assert!(display.contains("connection refused"));
    }

    #[test]
    fn test_lark_api_error_display_illegal_param() {
        let err = LarkAPIError::IllegalParamError("missing token".to_string());
        let display = format!("{}", err);
        assert!(display.contains("Invalid parameter"));
        assert!(display.contains("missing token"));
    }

    #[test]
    fn test_lark_api_error_display_deserialize() {
        let err = LarkAPIError::DeserializeError("invalid json".to_string());
        let display = format!("{}", err);
        assert!(display.contains("JSON deserialization error"));
    }

    #[test]
    fn test_lark_api_error_display_request() {
        let err = LarkAPIError::RequestError("timeout".to_string());
        let display = format!("{}", err);
        assert!(display.contains("HTTP request failed"));
    }

    #[test]
    fn test_lark_api_error_display_url_parse() {
        let err = LarkAPIError::UrlParseError("invalid url".to_string());
        let display = format!("{}", err);
        assert!(display.contains("URL parse error"));
    }

    #[test]
    fn test_lark_api_error_display_api() {
        let err = LarkAPIError::ApiError {
            code: 99991401,
            message: "invalid access_token".to_string(),
            request_id: Some("req_123".to_string()),
        };
        let display = format!("{}", err);
        assert!(display.contains("invalid access_token"));
        assert!(display.contains("99991401"));
        assert!(display.contains("req_123"));
    }

    #[test]
    fn test_lark_api_error_display_missing_token() {
        let err = LarkAPIError::MissingAccessToken;
        let display = format!("{}", err);
        assert!(display.contains("Missing access token"));
    }

    #[test]
    fn test_lark_api_error_display_bad_request() {
        let err = LarkAPIError::BadRequest("malformed request".to_string());
        let display = format!("{}", err);
        assert!(display.contains("Bad request"));
        assert!(display.contains("malformed request"));
    }

    #[test]
    fn test_lark_api_error_display_data_error() {
        let err = LarkAPIError::DataError("data not found".to_string());
        let display = format!("{}", err);
        assert!(display.contains("Data error"));
        assert!(display.contains("data not found"));
    }

    #[test]
    fn test_lark_api_error_display_api_error_variant() {
        let err = LarkAPIError::APIError {
            code: 99991661,
            msg: "permission denied".to_string(),
            error: Some("access_denied".to_string()),
        };
        let display = format!("{}", err);
        assert!(display.contains("permission denied"));
        assert!(display.contains("99991661"));
    }

    #[test]
    fn test_lark_api_error_from_io_error() {
        use std::io;
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let lark_err: LarkAPIError = io_err.into();
        match lark_err {
            LarkAPIError::IOErr(msg) => assert!(msg.contains("file not found")),
            _ => panic!("expected IOErr"),
        }
    }

    #[test]
    fn test_lark_api_error_from_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let lark_err: LarkAPIError = json_err.into();
        match lark_err {
            LarkAPIError::DeserializeError(_) => {},
            _ => panic!("expected DeserializeError"),
        }
    }

    #[test]
    fn test_lark_api_error_debug() {
        let err = LarkAPIError::MissingAccessToken;
        let debug = format!("{:?}", err);
        assert!(debug.contains("MissingAccessToken"));
    }
}

#[cfg(test)]
mod pending_message_tests {
    use ntd::services::message_debounce::PendingMessage;
    use std::collections::HashMap;

    #[test]
    fn test_pending_message_creation() {
        let msg = PendingMessage {
            bot_id: 1,
            chat_id: "chat_123".to_string(),
            chat_type: "p2p".to_string(),
            sender: "user_456".to_string(),
            content: "Hello".to_string(),
            todo_id: 10,
            todo_prompt: "Do something".to_string(),
            executor: Some("kimi".to_string()),
            trigger_type: "feishu".to_string(),
            params: None,
            message_id: Some("msg_789".to_string()),
            resume_session_id: None,
            resume_message: None,
            binding_id: None,
        };

        assert_eq!(msg.bot_id, 1);
        assert_eq!(msg.chat_id, "chat_123");
        assert_eq!(msg.content, "Hello");
        assert_eq!(msg.todo_id, 10);
        assert_eq!(msg.executor, Some("kimi".to_string()));
    }

    #[test]
    fn test_pending_message_with_params() {
        let mut params = HashMap::new();
        params.insert("key".to_string(), "value".to_string());

        let msg = PendingMessage {
            bot_id: 1,
            chat_id: "chat_123".to_string(),
            chat_type: "group".to_string(),
            sender: "user_456".to_string(),
            content: "Hello".to_string(),
            todo_id: 10,
            todo_prompt: "Do something".to_string(),
            executor: None,
            trigger_type: "feishu".to_string(),
            params: Some(params),
            message_id: None,
            resume_session_id: None,
            resume_message: None,
            binding_id: None,
        };

        assert!(msg.params.is_some());
        assert_eq!(msg.params.as_ref().unwrap().get("key"), Some(&"value".to_string()));
    }
}

#[cfg(test)]
mod cascade_delete_tests {
    use ntd::db::Database;

    /// Regression test: deleting an agent_bot should cascade-delete all
    /// related rows in feishu child tables (ON DELETE CASCADE).
    /// Previously this raised FOREIGN KEY constraint failed.
    #[tokio::test]
    async fn test_delete_agent_bot_cascades_feishu_children() {
        let db = Database::new(":memory:").await.unwrap();

        // 1. Create a feishu bot (also creates feishu_response_config rows)
        let bot_id = db
            .create_agent_bot("feishu", "test-bot", "app_id", "app_secret", None, None)
            .await
            .unwrap();

        // 2. Insert a row into each feishu child table
        db.set_feishu_home(bot_id, "ou_user1", Some("oc_chat1"), "rid1", "open_id")
            .await
            .unwrap();

        db.save_feishu_message(ntd::db::NewFeishuMessage {
            bot_id,
            message_id: "msg_001",
            chat_id: "oc_chat1",
            chat_type: "p2p",
            sender_open_id: "ou_user1",
            sender_type: None,
            content: Some("hello"),
            msg_type: "text",
            is_mention: false,
        })
        .await
        .unwrap();

        db.create_feishu_history_chat(bot_id, "oc_chat2", Some("Test Group"))
            .await
            .unwrap();

        db.add_group_whitelist(bot_id, "ou_sender1", Some("Alice"))
            .await
            .unwrap();

        // Create push target row
        db.set_p2p_receive_id(bot_id, "ou_p2p_rid")
            .await
            .unwrap();

        // 3. Verify child data exists before delete
        assert!(db.get_feishu_home(bot_id, "ou_user1").await.unwrap().is_some());
        assert!(db.get_feishu_messages(bot_id, 100).await.unwrap().len() > 0);
        assert!(db.get_feishu_history_chats(bot_id).await.unwrap().len() > 0);
        assert!(db.get_feishu_push_target(bot_id).await.unwrap().is_some());
        assert!(db.get_group_whitelist(bot_id).await.unwrap().len() > 0);
        assert!(db.get_feishu_response_configs(bot_id).await.unwrap().len() > 0);

        // 4. Delete the bot — should NOT error due to FK constraint
        db.delete_agent_bot(bot_id).await.unwrap();

        // 5. Verify the bot is gone
        assert!(db.get_agent_bot(bot_id).await.unwrap().is_none());

        // 6. Verify child data is gone (cascade worked)
        assert!(db.get_feishu_home(bot_id, "ou_user1").await.unwrap().is_none(),
            "feishu_homes should be empty after cascade");
        assert!(db.get_feishu_messages(bot_id, 100).await.unwrap().is_empty(),
            "feishu_messages should be empty after cascade");
        assert!(db.get_feishu_history_chats(bot_id).await.unwrap().is_empty(),
            "feishu_history_chats should be empty after cascade");
        assert!(db.get_feishu_push_target(bot_id).await.unwrap().is_none(),
            "feishu_push_targets should be empty after cascade");
        assert!(db.get_group_whitelist(bot_id).await.unwrap().is_empty(),
            "feishu_group_whitelist should be empty after cascade");
        assert!(db.get_feishu_response_configs(bot_id).await.unwrap().is_empty(),
            "feishu_response_config should be empty after cascade");
    }
}

#[cfg(test)]
mod whitelist_and_message_tests {
    use ntd::db::{Database, NewFeishuHistoryMessage, NewFeishuMessage};

    /// add_group_whitelist with empty sender_open_id should return Err
    #[tokio::test]
    async fn test_add_group_whitelist_empty_sender_rejected() {
        let db = Database::new(":memory:").await.unwrap();
        let bot_id = db
            .create_agent_bot("feishu", "test-bot", "app_id", "app_secret", None, None)
            .await
            .unwrap();

        let result = db.add_group_whitelist(bot_id, "", Some("Empty Sender")).await;
        assert!(result.is_err(), "Empty sender_open_id should be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("cannot be empty"),
            "Error should mention 'cannot be empty', got: {}",
            err_msg
        );
    }

    /// is_sender_in_whitelist should check sender properly
    #[tokio::test]
    async fn test_sender_in_whitelist_with_entries() {
        let db = Database::new(":memory:").await.unwrap();
        let bot_id = db
            .create_agent_bot("feishu", "test-bot", "app_id", "app_secret", None, None)
            .await
            .unwrap();

        // Add a whitelisted sender
        db.add_group_whitelist(bot_id, "ou_alice", Some("Alice"))
            .await
            .unwrap();

        // Whitelisted sender should be allowed
        let result = db.is_sender_in_whitelist(bot_id, "ou_alice").await.unwrap();
        assert!(result, "Whitelisted sender should be allowed");

        // Non-whitelisted sender should be denied
        let result = db.is_sender_in_whitelist(bot_id, "ou_bob").await.unwrap();
        assert!(!result, "Non-whitelisted sender should be denied");
    }

    /// is_sender_in_whitelist with empty whitelist should allow all
    #[tokio::test]
    async fn test_sender_in_whitelist_empty_allows_all() {
        let db = Database::new(":memory:").await.unwrap();
        let bot_id = db
            .create_agent_bot("feishu", "test-bot", "app_id", "app_secret", None, None)
            .await
            .unwrap();

        // No whitelist entries — should allow any sender
        let result = db.is_sender_in_whitelist(bot_id, "ou_anyone").await.unwrap();
        assert!(result, "Empty whitelist should allow all senders");
    }

    /// mark_feishu_message_failed should set processed=false and processed_todo_id=None
    #[tokio::test]
    async fn test_mark_feishu_message_failed() {
        let db = Database::new(":memory:").await.unwrap();
        let bot_id = db
            .create_agent_bot("feishu", "test-bot", "app_id", "app_secret", None, None)
            .await
            .unwrap();

        // Save a message
        let msg_id = db
            .save_feishu_message(NewFeishuMessage {
                bot_id,
                message_id: "msg_fail_001",
                chat_id: "oc_chat1",
                chat_type: "p2p",
                sender_open_id: "ou_user1",
                sender_type: None,
                content: Some("hello"),
                msg_type: "text",
                is_mention: false,
            })
            .await
            .unwrap();

        // First mark it as processed
        db.mark_feishu_message_processed("msg_fail_001", 42, Some(100))
            .await
            .unwrap();

        // Then mark as failed
        db.mark_feishu_message_failed("msg_fail_001")
            .await
            .unwrap();

        // Verify processed=false and processed_todo_id=None
        let messages = db.get_feishu_messages(bot_id, 10).await.unwrap();
        let msg = messages.iter().find(|m| m.message_id == "msg_fail_001").unwrap();
        assert!(!msg.processed, "processed should be false after failure");
        assert!(
            msg.processed_todo_id.is_none(),
            "processed_todo_id should be None after failure"
        );
    }

    /// save_feishu_history_message should set processed=false (regression test)
    #[tokio::test]
    async fn test_save_feishu_history_message_processed_false() {
        let db = Database::new(":memory:").await.unwrap();
        let bot_id = db
            .create_agent_bot("feishu", "test-bot", "app_id", "app_secret", None, None)
            .await
            .unwrap();

        let msg_id = db
            .save_feishu_history_message(NewFeishuHistoryMessage {
                bot_id,
                message_id: "msg_hist_001",
                chat_id: "oc_chat1",
                chat_type: "group",
                sender_open_id: "ou_user1",
                sender_nickname: Some("Alice"),
                sender_type: None,
                content: Some("historical message"),
                msg_type: "text",
                created_at: "2025-01-01T00:00:00Z",
            })
            .await
            .unwrap();

        let messages = db.get_feishu_messages(bot_id, 10).await.unwrap();
        let msg = messages.iter().find(|m| m.message_id == "msg_hist_001").unwrap();
        assert!(!msg.processed, "History messages should have processed=false");
        assert!(msg.is_history, "Should be marked as history");
    }
}