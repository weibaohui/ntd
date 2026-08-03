use std::sync::Arc;

use super::config::{AppType, Config, ConfigBuilder};

pub struct LarkClient {
    pub config: Config,
    pub im: ImService,
}

pub struct LarkClientBuilder {
    config_builder: ConfigBuilder,
}

impl LarkClientBuilder {
    pub fn with_app_type(mut self, app_type: AppType) -> Self {
        self.config_builder = self.config_builder.app_type(app_type);
        self
    }

    pub fn with_open_base_url(mut self, base_url: String) -> Self {
        self.config_builder = self.config_builder.base_url(base_url);
        self
    }

    pub fn with_enable_token_cache(mut self, enable: bool) -> Self {
        self.config_builder = self.config_builder.enable_token_cache(enable);
        self
    }

    pub fn build(self) -> LarkClient {
        let config = self.config_builder.build();
        let shared = Arc::new(config.clone());
        LarkClient {
            config,
            im: ImService {
                v1: V1 {
                    message: MessageService { config: shared },
                },
            },
        }
    }
}

impl LarkClient {
    pub fn builder(app_id: &str, app_secret: &str) -> LarkClientBuilder {
        LarkClientBuilder {
            config_builder: Config::builder().app_id(app_id).app_secret(app_secret),
        }
    }
}

pub struct ImService {
    pub v1: V1,
}

pub struct V1 {
    pub message: MessageService,
}

pub struct MessageService {
    pub config: Arc<Config>,
}
