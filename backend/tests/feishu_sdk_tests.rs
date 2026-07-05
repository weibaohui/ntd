//! Tests for Feishu SDK types

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
#[cfg(test)]
mod base_response_tests {
    use ntd::feishu::sdk::api_types::{BaseResponse, RawResponse};

    #[test]
    fn test_success_returns_true_when_code_zero() {
        let response: BaseResponse<String> = BaseResponse {
            raw_response: RawResponse { code: 0, msg: "ok".to_string() },
            data: Some("hello".to_string()),
        };
        assert!(response.success());
        assert_eq!(response.code(), 0);
        assert_eq!(response.msg(), "ok");
    }

    #[test]
    fn test_success_returns_false_when_code_nonzero() {
        let response: BaseResponse<String> = BaseResponse {
            raw_response: RawResponse { code: 99999, msg: "error".to_string() },
            data: None,
        };
        assert!(!response.success());
        assert_eq!(response.code(), 99999);
        assert_eq!(response.msg(), "error");
    }

    #[test]
    fn test_data_or_api_error_success() {
        let response: BaseResponse<String> = BaseResponse {
            raw_response: RawResponse { code: 0, msg: "ok".to_string() },
            data: Some("hello".to_string()),
        };
        let result = response.data_or_api_error();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_data_or_api_error_empty_data() {
        let response: BaseResponse<String> = BaseResponse {
            raw_response: RawResponse { code: 0, msg: "ok".to_string() },
            data: None,
        };
        let result = response.data_or_api_error();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Response succeeded but data is empty"));
    }

    #[test]
    fn test_data_or_api_error_failure() {
        let response: BaseResponse<String> = BaseResponse {
            raw_response: RawResponse { code: 10001, msg: "invalid token".to_string() },
            data: None,
        };
        let result = response.data_or_api_error();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("10001"));
        assert!(err.to_string().contains("invalid token"));
    }

    #[test]
    fn test_into_result_success() {
        let response: BaseResponse<String> = BaseResponse {
            raw_response: RawResponse { code: 0, msg: "ok".to_string() },
            data: Some("hello".to_string()),
        };
        let result = response.into_result();
        assert!(result.is_ok());
    }

    #[test]
    fn test_into_result_failure() {
        let response: BaseResponse<String> = BaseResponse {
            raw_response: RawResponse { code: 10001, msg: "error".to_string() },
            data: None,
        };
        let result = response.into_result();
        assert!(result.is_err());
    }

    #[test]
    fn test_response_with_struct_data() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct MessageData {
            message_id: String,
        }
        let response: BaseResponse<MessageData> = BaseResponse {
            raw_response: RawResponse { code: 0, msg: "success".to_string() },
            data: Some(MessageData { message_id: "msg_123".to_string() }),
        };
        let result = response.data_or_api_error().unwrap();
        assert_eq!(result.message_id, "msg_123");
    }
}

#[cfg(test)]
mod raw_response_tests {
    use ntd::feishu::sdk::api_types::RawResponse;

    #[test]
    fn test_display_format() {
        let raw = RawResponse { code: 0, msg: "ok".to_string() };
        assert_eq!(format!("{}", raw), "code: 0, msg: ok");
    }

    #[test]
    fn test_display_with_comma_in_msg() {
        let raw = RawResponse { code: 10001, msg: "error, try again".to_string() };
        assert_eq!(format!("{}", raw), "code: 10001, msg: error, try again");
    }

    #[test]
    fn test_raw_response_serde() {
        let json = r#"{"code":0,"msg":"success"}"#;
        let raw: RawResponse = serde_json::from_str(json).unwrap();
        assert_eq!(raw.code, 0);
        assert_eq!(raw.msg, "success");
    }

    #[test]
    fn test_raw_response_default() {
        let raw = RawResponse::default();
        assert_eq!(raw.code, 0);
        assert_eq!(raw.msg, "");
    }
}

#[cfg(test)]
mod create_message_request_builder_tests {
    use ntd::feishu::sdk::message::{CreateMessageRequestBuilder, CreateMessageRequestBody, CreateMessageRequestBodyBuilder};

    #[test]
    fn test_build_request_with_all_fields() {
        let body = CreateMessageRequestBodyBuilder::default()
            .receive_id("chat_123")
            .msg_type("text")
            .content("Hello world")
            .build();

        let request = CreateMessageRequestBuilder::default()
            .receive_id_type("chat_id")
            .request_body(body)
            .build();

        assert!(!request.api_req.query_params.is_empty());
        assert_eq!(request.api_req.query_params.get("receive_id_type"), Some(&"chat_id".to_string()));
    }

    #[test]
    fn test_build_request_body() {
        let body = CreateMessageRequestBodyBuilder::default()
            .receive_id("user_456")
            .msg_type("text")
            .content("Test message")
            .build();

        assert_eq!(body.receive_id, "user_456");
        assert_eq!(body.msg_type, "text");
        assert_eq!(body.content, "Test message");
    }

    #[test]
    fn test_request_body_serde() {
        let body = CreateMessageRequestBodyBuilder::default()
            .receive_id("chat_123")
            .msg_type("text")
            .content("Hello")
            .build();

        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("chat_123"));
        assert!(json.contains("text"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_request_body_with_uuid() {
        let body = CreateMessageRequestBody {
            receive_id: "chat_123".to_string(),
            msg_type: "text".to_string(),
            content: "Hello".to_string(),
            uuid: Some("unique_id_123".to_string()),
        };

        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("unique_id_123"));
    }

    #[test]
    fn test_request_body_default() {
        let body = CreateMessageRequestBody::default();
        assert!(body.receive_id.is_empty());
        assert!(body.msg_type.is_empty());
        assert!(body.content.is_empty());
        assert!(body.uuid.is_none());
    }
}

#[cfg(test)]
mod lark_api_error_tests {
    use ntd::feishu::sdk::error::LarkAPIError;

    #[test]
    fn test_io_error_display() {
        let err = LarkAPIError::IOErr("file not found".to_string());
        assert_eq!(err.to_string(), "IO error: file not found");
    }

    #[test]
    fn test_illegal_param_error_display() {
        let err = LarkAPIError::IllegalParamError("invalid id".to_string());
        assert_eq!(err.to_string(), "Invalid parameter: invalid id");
    }

    #[test]
    fn test_deserialize_error_display() {
        let err = LarkAPIError::DeserializeError("unexpected token".to_string());
        assert_eq!(err.to_string(), "JSON deserialization error: unexpected token");
    }

    #[test]
    fn test_request_error_display() {
        let err = LarkAPIError::RequestError("connection refused".to_string());
        assert_eq!(err.to_string(), "HTTP request failed: connection refused");
    }

    #[test]
    fn test_url_parse_error_display() {
        let err = LarkAPIError::UrlParseError("invalid url".to_string());
        assert_eq!(err.to_string(), "URL parse error: invalid url");
    }

    #[test]
    fn test_api_error_display() {
        let err = LarkAPIError::ApiError {
            code: 99999,
            message: "internal error".to_string(),
            request_id: Some("req_123".to_string()),
        };
        let s = err.to_string();
        assert!(s.contains("99999"));
        assert!(s.contains("internal error"));
        assert!(s.contains("req_123"));
    }

    #[test]
    fn test_missing_access_token_display() {
        let err = LarkAPIError::MissingAccessToken;
        assert_eq!(err.to_string(), "Missing access token");
    }

    #[test]
    fn test_bad_request_display() {
        let err = LarkAPIError::BadRequest("invalid format".to_string());
        assert_eq!(err.to_string(), "Bad request: invalid format");
    }

    #[test]
    fn test_data_error_display() {
        let err = LarkAPIError::DataError("missing field".to_string());
        assert_eq!(err.to_string(), "Data error: missing field");
    }

    #[test]
    fn test_api_error_without_request_id() {
        let err = LarkAPIError::ApiError {
            code: 10001,
            message: "error".to_string(),
            request_id: None,
        };
        let s = err.to_string();
        assert!(s.contains("10001"));
        assert!(s.contains("error"));
    }

    #[test]
    fn test_api_error_v2_display() {
        let err = LarkAPIError::APIError {
            code: 10002,
            msg: "invalid token".to_string(),
            error: Some("auth failed".to_string()),
        };
        let s = err.to_string();
        assert!(s.contains("10002"));
        assert!(s.contains("invalid token"));
    }

    #[test]
    fn test_error_clone() {
        let err = LarkAPIError::MissingAccessToken;
        let cloned = err.clone();
        assert_eq!(cloned.to_string(), err.to_string());
    }

    #[test]
    fn test_error_debug() {
        let err = LarkAPIError::IOErr("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("IOErr"));
    }
}
