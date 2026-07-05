use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::api_types::{ApiRequest, BaseResponse};
use super::client::MessageService;
use super::config::AccessTokenType;
use super::error::SDKResult;
use super::http::Transport;

const IM_V1_SEND_MESSAGE: &str = "/open-apis/im/v1/messages";

#[derive(Debug, Clone, Default)]
pub struct CreateMessageRequest {
    pub api_req: ApiRequest,
}

impl CreateMessageRequest {
    pub fn builder() -> CreateMessageRequestBuilder {
        CreateMessageRequestBuilder::default()
    }
}

#[derive(Default)]
pub struct CreateMessageRequestBuilder {
    request: CreateMessageRequest,
}

#[allow(clippy::needless_pass_by_value)]
impl CreateMessageRequestBuilder {
    pub fn receive_id_type(mut self, receive_id_type: impl ToString) -> Self {
        self.request
            .api_req
            .query_params
            .insert("receive_id_type", receive_id_type.to_string());
        self
    }

    pub fn request_body(mut self, body: CreateMessageRequestBody) -> Self {
        match serde_json::to_vec(&body) {
            Ok(bytes) => {
                self.request.api_req.body = bytes;
            }
            Err(e) => {
                tracing::error!("Failed to serialize request body: {}", e);
            }
        }
        self
    }

    pub fn build(self) -> CreateMessageRequest {
        self.request
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CreateMessageRequestBody {
    pub receive_id: String,
    pub msg_type: String,
    pub content: String,
    pub uuid: Option<String>,
}

impl CreateMessageRequestBody {
    pub fn builder() -> CreateMessageRequestBodyBuilder {
        CreateMessageRequestBodyBuilder::default()
    }
}

#[derive(Default)]
pub struct CreateMessageRequestBodyBuilder {
    request: CreateMessageRequestBody,
}

#[allow(clippy::needless_pass_by_value)]
impl CreateMessageRequestBodyBuilder {
    pub fn receive_id(mut self, receive_id: impl ToString) -> Self {
        self.request.receive_id = receive_id.to_string();
        self
    }

    pub fn msg_type(mut self, msg_type: impl ToString) -> Self {
        self.request.msg_type = msg_type.to_string();
        self
    }

    pub fn content(mut self, content: impl ToString) -> Self {
        self.request.content = content.to_string();
        self
    }

    pub fn build(self) -> CreateMessageRequestBody {
        self.request
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub message_id: String,
    pub msg_type: String,
    pub chat_id: String,
}

impl MessageService {
    pub async fn create(
        &self,
        create_message_request: CreateMessageRequest,
        _option: Option<()>,
    ) -> SDKResult<Message> {
        let mut api_req = create_message_request.api_req;
        api_req.http_method = Method::POST;
        api_req.api_path = IM_V1_SEND_MESSAGE.to_string();
        api_req.supported_access_token_types = vec![AccessTokenType::Tenant, AccessTokenType::User];

        let api_resp: BaseResponse<Message> = Transport::request(api_req, &self.config).await?;

        api_resp.data_or_api_error()
    }
}
