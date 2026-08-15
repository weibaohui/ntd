use std::{collections::HashMap, ops::Deref, sync::Arc, time::Duration};
use tokio::sync::Mutex;

use super::token_manager::TokenManager;

pub const FEISHU_BASE_URL: &str = "https://open.feishu.cn";

pub const TENANT_ACCESS_TOKEN_INTERNAL_URL_PATH: &str =
    "/open-apis/auth/v3/tenant_access_token/internal";

pub const APP_ACCESS_TOKEN_KEY_PREFIX: &str = "app_access_token";
pub const EXPIRY_DELTA: i32 = 60 * 3;

pub const CONTENT_TYPE_JSON: &str = "application/json";

/// 飞书 HTTP 请求默认超时（106 体检：reqwest 默认无超时，TCP 半开 = await 永久悬挂）。
pub const DEFAULT_REQ_TIMEOUT: Duration = Duration::from_secs(15);

/// 构造带超时的 reqwest Client。
///
/// 全仓库飞书调用点此前直接 `reqwest::Client::new()`（无超时且每次新建、无连接复用），
/// 串行 await 的推送循环里一次挂起即可拖死整条通知链路。集中在此构造，保证
/// 所有调用点共享同一策略；构造是 infallible 的（超时参数总是合法）。
pub fn build_client_with_timeout(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[derive(Default, Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum AppType {
    #[default]
    SelfBuild,
    Marketplace,
}

#[derive(Default, Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum AccessTokenType {
    #[default]
    None,
    App,
    Tenant,
    User,
}

#[derive(Debug, Clone)]
pub struct Config {
    inner: Arc<ConfigInner>,
}

#[derive(Debug)]
pub struct ConfigInner {
    pub app_id: String,
    pub app_secret: String,
    pub base_url: String,
    pub enable_token_cache: bool,
    pub app_type: AppType,
    pub http_client: reqwest::Client,
    pub req_timeout: Option<Duration>,
    pub header: HashMap<String, String>,
    pub token_manager: Arc<Mutex<TokenManager>>,
}

impl Default for ConfigInner {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_secret: String::new(),
            base_url: FEISHU_BASE_URL.to_string(),
            enable_token_cache: true,
            app_type: AppType::SelfBuild,
            // 106 体检：reqwest 默认无超时，TCP 半开会让所有飞书 API await 永久悬挂
            // （token_manager 取 token 挂起 = 拖死全部调用方）。默认 15s 超时。
            http_client: build_client_with_timeout(DEFAULT_REQ_TIMEOUT),
            req_timeout: Some(DEFAULT_REQ_TIMEOUT),
            header: HashMap::new(),
            token_manager: Arc::new(Mutex::new(TokenManager::new())),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            inner: Arc::new(ConfigInner::default()),
        }
    }
}

impl Deref for Config {
    type Target = ConfigInner;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Config {
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    pub fn new(inner: ConfigInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

#[derive(Default, Clone)]
pub struct ConfigBuilder {
    app_id: Option<String>,
    app_secret: Option<String>,
    base_url: Option<String>,
    enable_token_cache: Option<bool>,
    app_type: Option<AppType>,
    http_client: Option<reqwest::Client>,
    req_timeout: Option<Duration>,
    header: Option<HashMap<String, String>>,
}

impl ConfigBuilder {
    pub fn app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = Some(app_id.into());
        self
    }

    pub fn app_secret(mut self, app_secret: impl Into<String>) -> Self {
        self.app_secret = Some(app_secret.into());
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn enable_token_cache(mut self, enable: bool) -> Self {
        self.enable_token_cache = Some(enable);
        self
    }

    pub fn app_type(mut self, app_type: AppType) -> Self {
        self.app_type = Some(app_type);
        self
    }

    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http_client = Some(client);
        self
    }

    pub fn req_timeout(mut self, timeout: Duration) -> Self {
        self.req_timeout = Some(timeout);
        self
    }

    pub fn header(mut self, header: HashMap<String, String>) -> Self {
        self.header = Some(header);
        self
    }

    pub fn build(self) -> Config {
        let default = ConfigInner::default();
        Config::new(ConfigInner {
            app_id: self.app_id.unwrap_or(default.app_id),
            app_secret: self.app_secret.unwrap_or(default.app_secret),
            base_url: self.base_url.unwrap_or(default.base_url),
            enable_token_cache: self
                .enable_token_cache
                .unwrap_or(default.enable_token_cache),
            app_type: self.app_type.unwrap_or(default.app_type),
            http_client: self.http_client.unwrap_or(default.http_client),
            req_timeout: self.req_timeout.or(default.req_timeout),
            header: self.header.unwrap_or(default.header),
            token_manager: default.token_manager,
        })
    }
}
