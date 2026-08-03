use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};

use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::config::AccessTokenType;
use super::error::{LarkAPIError, SDKResult};

#[derive(Debug, Clone, Default)]
pub struct ApiRequest {
    pub(crate) http_method: Method,
    pub api_path: String,
    pub body: Vec<u8>,
    pub query_params: HashMap<&'static str, String>,
    pub(crate) supported_access_token_types: Vec<AccessTokenType>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BaseResponse<T> {
    #[serde(flatten)]
    pub raw_response: RawResponse,
    pub data: Option<T>,
}

impl<T> BaseResponse<T> {
    pub fn success(&self) -> bool {
        self.raw_response.code == 0
    }

    pub fn code(&self) -> i32 {
        self.raw_response.code
    }

    pub fn msg(&self) -> &str {
        &self.raw_response.msg
    }

    pub fn data_or_api_error(self) -> SDKResult<T> {
        if self.success() {
            self.data
                .ok_or_else(|| LarkAPIError::ApiError {
                    code: 0,
                    message: "Response succeeded but data is empty".to_string(),
                    request_id: None,
                })
        } else {
            Err(LarkAPIError::ApiError {
                code: self.code(),
                message: self.msg().to_string(),
                request_id: None,
            })
        }
    }

    pub fn into_result(self) -> SDKResult<Self> {
        if self.success() {
            Ok(self)
        } else {
            Err(LarkAPIError::ApiError {
                code: self.code(),
                message: self.msg().to_string(),
                request_id: None,
            })
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RawResponse {
    pub code: i32,
    pub msg: String,
}

impl Display for RawResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "code: {}, msg: {}", self.code, self.msg)
    }
}
