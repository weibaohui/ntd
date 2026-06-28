//! Channel / Agent 的具体平台实现。
//!
//! 每个子模块实现一对 trait：
//! - `feishu`：`Channel` + `TypingIndicator`（飞书机器人）
//!
//! 后续会加：
//! - `dingtalk`：钉钉
//! - `telegram`：Telegram Bot
//! - `wechat`：企业微信
//! - `slack`：Slack Bot
//!
//! 每个子模块的设计原则：
//! 1. 持有 `Arc<SharedHttpClient>` 复用连接池（治 `Client::new()` 反模式）
//! 2. tenant_token / bot_token 自己缓存，避免每条消息都去拿
//! 3. 长连接（WS / webhook listener）的生命周期由 `Channel::start/stop` 管
//! 4. 反 API 调用失败返回 `Error::Platform(String)`，不 panic

pub mod feishu;
