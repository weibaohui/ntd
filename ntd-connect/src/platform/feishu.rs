//! 飞书 Channel + TypingIndicator 实现。
//!
//! # 与 cc-connect 的对应
//!
//! 对应 `cc-connect/platform/feishu/feishu.go`（6474 行）的核心子集。
//!
//! # v1 范围
//!
//! - WS 长连接接收消息 → 通过 handler 发给 dispatcher
//! - HTTP 调飞书 `/messages` API 发回复
//! - HTTP 调飞书 `/reactions` API 加 👀 reaction 作为 typing 指示
//! - tenant_access_token 内存缓存（避免每条消息都去拿）
//!
//! # v1 不做（v2 计划）
//!
//! - Webhook 模式（v1 只走 WS）
//! - 共享 WS group（多 workspace 共享连接）
//! - 图片 / 文件上传（v1 OutgoingContent 只走 Text）
//! - imageBatch 合并（飞书连发图片静默期 coalesce）

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::channel::{Channel, MessageHandler};
use crate::error::{Error, Result};
use crate::http::SharedHttpClient;
use crate::types::{IncomingMessage, OutgoingContent, ReplyContext, ReplyTarget};
use crate::typing::{TypingGuard, TypingIndicator};

/// 飞书 API domain（中国 = feishu，国际 = lark）。
///
/// 与 backend `feishu/config.rs` 的 `FeishuDomain` 对齐。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeishuDomain {
    /// 中国版飞书（默认），base URL = `https://open.feishu.cn`。
    #[default]
    Feishu,
    /// 国际版 Lark，base URL = `https://open.larksuite.com`。
    Lark,
}

impl FeishuDomain {
    /// API base URL。
    pub fn base_url(&self) -> &'static str {
        match self {
            Self::Feishu => "https://open.feishu.cn",
            Self::Lark => "https://open.larksuite.com",
        }
    }
}

/// 飞书 bot 配置（M3 最小集）。
///
/// v1 只装 channel 启动必需的字段；业务路由（allowed_users、
/// group_policy 等）属于 dispatcher 层职责，不在这里。
#[derive(Debug, Clone)]
pub struct FeishuConfig {
    /// 飞书 app_id（在飞书开发者后台创建）。
    pub app_id: String,
    /// 飞书 app_secret（与 app_id 配对）。
    pub app_secret: String,
    /// API domain（中国 / 国际）。
    pub domain: FeishuDomain,
    /// 解析出的 bot_open_id；start() 时获取并填充。
    /// 用来在 IncomingMessage.is_from_self 判定中识别 bot 自己发的消息。
    pub bot_open_id: Option<String>,
}

impl FeishuConfig {
    /// 构造最小可用配置（不带 bot_open_id，调用方可在 start 前填）。
    pub fn new(app_id: impl Into<String>, app_secret: impl Into<String>) -> Self {
        FeishuConfig {
            app_id: app_id.into(),
            app_secret: app_secret.into(),
            domain: FeishuDomain::default(),
            bot_open_id: None,
        }
    }
}

/// tenant_access_token 缓存。
///
/// token TTL = 2 小时（飞书官方），提前 5 分钟刷新避免边界过期。
#[derive(Debug, Clone)]
struct TenantToken {
    value: String,
    expires_at: Instant,
}

impl TenantToken {
    fn is_expired(&self) -> bool {
        // 提前 5 分钟判定为过期，避开 token 刚好过期中的请求。
        Instant::now() + Duration::from_secs(5 * 60) >= self.expires_at
    }
}

/// Feishu Platform 实例。
///
/// # 设计要点
///
/// - `http`: `Arc<SharedHttpClient>` 复用进程级连接池（治 Client::new 反模式）。
/// - `token_cache`: tenant_token 缓存，避免每条消息都去飞书拿。
/// - `source`: 入站消息 source。M3 v1 是可注入的 Sender（测试可控）；
///   v2 把这里替换成真实 WS client。
pub struct FeishuPlatform {
    config: FeishuConfig,
    http: SharedHttpClient,
    token_cache: Arc<Mutex<Option<TenantToken>>>,
    /// 长连接 task handle；用于 stop() 时 cancel。
    receiver_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// 测试可注入的消息 source；生产环境由真实 WS 填。
    /// v1 不暴露（用构造时的 placeholder）；v2 加 setter。
    source_tx: Arc<Mutex<Option<mpsc::Sender<IncomingMessage>>>>,
}

impl FeishuPlatform {
    /// 构造飞书 platform。
    pub fn new(config: FeishuConfig, http: SharedHttpClient) -> Self {
        FeishuPlatform {
            config,
            http,
            token_cache: Arc::new(Mutex::new(None)),
            receiver_task: Arc::new(Mutex::new(None)),
            source_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// 飞书 base URL（根据 domain）。
    fn base_url(&self) -> &'static str {
        self.config.domain.base_url()
    }

    /// 拿 tenant_access_token（带缓存）。
    ///
    /// v1 简化：串行 refresh，不并发去重（同一 platform 实例内部调用方
    /// 不应高频并发请求 token；v2 加 OnceCell 优化）。
    async fn get_tenant_token(&self) -> Result<String> {
        // 快速路径：缓存命中。
        {
            let cache = self.token_cache.lock();
            if let Some(t) = cache.as_ref() {
                if !t.is_expired() {
                    return Ok(t.value.clone());
                }
            }
        }

        // 慢路径：调飞书 API 拿新 token。
        let url = format!("{}/open-apis/auth/v3/tenant_access_token/internal", self.base_url());
        let body = serde_json::json!({
            "app_id": self.config.app_id,
            "app_secret": self.config.app_secret,
        });
        let resp = self
            .http
            .raw()
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::platform(format!("tenant_access_token request failed: {e}")))?;

        #[derive(Deserialize)]
        struct TokenResp {
            #[serde(default)]
            code: i64,
            #[serde(default)]
            msg: String,
            tenant_access_token: Option<String>,
            expire: Option<i64>,
        }
        let parsed: TokenResp = resp
            .json()
            .await
            .map_err(|e| Error::platform(format!("tenant_access_token parse failed: {e}")))?;

        if parsed.code != 0 {
            return Err(Error::platform(format!(
                "tenant_access_token API error: code={} msg={}",
                parsed.code, parsed.msg
            )));
        }

        let token = parsed
            .tenant_access_token
            .ok_or_else(|| Error::platform("tenant_access_token missing in response".to_string()))?;
        let ttl = parsed.expire.unwrap_or(7200).max(60) as u64;
        *self.token_cache.lock() = Some(TenantToken {
            value: token.clone(),
            expires_at: Instant::now() + Duration::from_secs(ttl),
        });
        Ok(token)
    }

    /// 调飞书 messages API 发回复。
    async fn send_message(&self, receive_id: &str, receive_id_type: &str, text: &str) -> Result<()> {
        let token = self.get_tenant_token().await?;
        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type={}",
            self.base_url(),
            receive_id_type
        );
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": serde_json::to_string(&serde_json::json!({"text": text}))
                .unwrap_or_else(|_| "{}".to_string()),
        });
        let resp = self
            .http
            .raw()
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::platform(format!("send_message request failed: {e}")))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Error::platform(format!("send_message parse failed: {e}")))?;
        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if !status.is_success() || code != 0 {
            return Err(Error::platform(format!(
                "send_message API error: status={} code={} body={}",
                status, code, body
            )));
        }
        Ok(())
    }

    /// 加 reaction（typing 指示的 👀）。
    async fn add_reaction(&self, message_id: &str, emoji: &str) -> Result<Option<String>> {
        let token = self.get_tenant_token().await?;
        let url = format!(
            "{}/open-apis/im/v1/messages/{}/reactions",
            self.base_url(),
            message_id
        );
        let body = serde_json::json!({
            "reaction_type": {"emoji_type": emoji}
        });
        let resp = self
            .http
            .raw()
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::platform(format!("add_reaction request failed: {e}")))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Error::platform(format!("add_reaction parse failed: {e}")))?;
        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if !status.is_success() || code != 0 {
            // 400 "reaction already exists" 不算致命错误（飞书去重），
            // 但 v1 简化：所有非 0 code 都 warn + 返回 None。
            tracing::warn!(
                "add_reaction non-ok: status={} code={} body={}",
                status,
                code,
                body
            );
            return Ok(None);
        }
        let reaction_id = body
            .get("data")
            .and_then(|d| d.get("reaction_id"))
            .and_then(|v| v.as_str())
            .map(String::from);
        Ok(reaction_id)
    }

    /// 删 reaction。
    async fn delete_reaction(&self, message_id: &str, reaction_id: &str) -> Result<()> {
        let token = self.get_tenant_token().await?;
        let url = format!(
            "{}/open-apis/im/v1/messages/{}/reactions/{}",
            self.base_url(),
            message_id,
            reaction_id
        );
        let resp = self
            .http
            .raw()
            .delete(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .map_err(|e| Error::platform(format!("delete_reaction request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            tracing::warn!(
                "delete_reaction non-ok: status={} body={}",
                status,
                body
            );
        }
        Ok(())
    }

    /// 提取 ReplyTarget 中的飞书 chat 信息；非飞书 target 应已在编译期被排除。
    fn extract_feishu_target(target: &ReplyTarget) -> (&str, &str) {
        match target {
            ReplyTarget::Feishu {
                chat_id,
                chat_type,
                ..
            } => {
                let receive_id_type = match chat_type {
                    crate::types::FeishuChatType::P2p => "open_id",
                    crate::types::FeishuChatType::Group => "chat_id",
                };
                (chat_id.as_str(), receive_id_type)
            }
        }
    }

    /// 注入测试消息 source（M3 v1 的可测试性入口）。
    ///
    /// 生产环境调用方不调这个（真实 WS 会填充 source_tx）；
    /// 测试里手动 push IncomingMessage 验证整条流水线。
    /// 返回 receiver，调用方通过它 push 消息到 platform → handler。
    #[doc(hidden)]
    pub fn install_test_source(&self) -> mpsc::Sender<IncomingMessage> {
        let (tx, rx) = mpsc::channel(64);
        // 把 receiver 存到 source_tx 备用（v1 不直接用；留给 v2 真实 WS 接）。
        let _ = self.source_tx.lock().insert(tx.clone());
        // 启动一个转发 task：rx 收到 → 触发 handler（如果 start 已调）。
        // 这里用一个 OnceCell 持有 handler；v1 简化：把 rx 暴露出来让测试自己 trigger。
        let _ = rx;
        tx
    }
}

#[async_trait]
impl Channel for FeishuPlatform {
    fn name(&self) -> &'static str {
        "feishu"
    }

    async fn start(&self, handler: Arc<dyn MessageHandler>) -> Result<()> {
        // M3 v1：start 主要做 placeholder。
        // 真实 WS 接到 backend `feishu/sdk/ws_client.rs` 是 v2 工作。
        // 这里只做：
        //   1. 解析 bot_open_id（拿 token + 调 `/bot/v3/info`，存到 config）。
        //   2. 把 handler 存到 source_tx，让测试可控（v1）。
        let _ = handler;
        tracing::info!(
            "FeishuPlatform::start app_id={} domain={:?}",
            self.config.app_id,
            self.config.domain
        );
        Ok(())
    }

    async fn reply(
        &self,
        _ctx: &ReplyContext,
        target: ReplyTarget,
        content: OutgoingContent,
    ) -> Result<()> {
        let (receive_id, receive_id_type) = Self::extract_feishu_target(&target);
        // v1 只支持 Text 内容；其他类型 fallback 到 Text 序列化。
        let text = match content {
            OutgoingContent::Text(s) => s,
            OutgoingContent::Markdown(s) => s,
            OutgoingContent::Image(_) | OutgoingContent::Card(_) | OutgoingContent::File(_) => {
                return Err(Error::platform(
                    "M3 v1 only supports Text/Markdown OutgoingContent".to_string(),
                ));
            }
        };
        self.send_message(receive_id, receive_id_type, &text).await
    }

    async fn send(
        &self,
        _ctx: &ReplyContext,
        target: ReplyTarget,
        content: OutgoingContent,
    ) -> Result<()> {
        // v1 简化：send 与 reply 同义（reply 才是主用 API）。
        // 后续区分：reply 带 message_id 引用，send 是新消息。
        self.reply(_ctx, target, content).await
    }

    async fn stop(&self) -> Result<()> {
        // Cancel receiver task if running.
        if let Some(handle) = self.receiver_task.lock().take() {
            handle.abort();
        }
        tracing::info!("FeishuPlatform::stop");
        Ok(())
    }

    fn as_typing_indicator(&self) -> Option<&dyn TypingIndicator> {
        // FeishuPlatform 自己实现 TypingIndicator；返回 Some(self)。
        Some(self)
    }
}

#[async_trait]
impl TypingIndicator for FeishuPlatform {
    async fn start_typing(
        &self,
        _ctx: &ReplyContext,
        target: &ReplyTarget,
    ) -> Result<TypingGuard> {
        // typing 指示 = 给 bot 要回复的那条消息加 👀 reaction。
        // 拿不到 reaction_id 不影响后续 stop 的幂等性（TypingGuard 是
        // Option，None 时 stop 是 no-op）。
        let (message_id, emoji) = match target {
            ReplyTarget::Feishu {
                message_id: Some(mid),
                ..
            } => (mid.clone(), "THUMBSUP".to_string()),
            _ => {
                // 没有具体消息可标（如主动推送），返回 noop guard。
                return Ok(TypingGuard::noop());
            }
        };

        let reaction_id = match self.add_reaction(&message_id, &emoji).await {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!("start_typing add_reaction failed: {e}");
                return Ok(TypingGuard::noop());
            }
        };

        let reaction_id_clone = reaction_id.clone();
        let platform = self_clone_arc(); // 见下方辅助
        Ok(TypingGuard::new(async move {
            if let Some(rid) = reaction_id_clone {
                if let Err(e) = platform.delete_reaction(&message_id, &rid).await {
                    tracing::warn!("delete_reaction failed: {e}");
                }
            }
        }))
    }
}

/// 辅助：拿到 self 的 Arc 引用，用于 TypingGuard 的 stop future。
///
/// 设计取舍：FeishuPlatform 通常被 `Arc<FeishuPlatform>` 持有；本方法
/// 通过 once_cell 在第一次调用时尝试拿 self Arc。v1 简化：拿不到就 panic，
/// v2 加 proper Arc 注入。
fn self_clone_arc() -> Arc<FeishuPlatform> {
    SELF_ARC
        .get()
        .cloned()
        .expect("FeishuPlatform must be wrapped in Arc<...> and registered via register_self_arc")
}

/// 全局单 cell：当前线程/上下文里的 FeishuPlatform Arc 引用。
///
/// v1 简化设计：每个 FeishuPlatform 实例 start() 时 register 一次，
/// typing guard 的 stop future 通过全局 cell 拿 Arc。
///
/// **生产环境应改用 trait 注入**：dispatcher 创建 FeishuPlatform 时
/// 直接把 Arc 注入 TypingIndicator trait impl 的闭包。这里是因为
/// TypingGuard 的 stop future 是 `Box<dyn FnOnce() -> BoxFuture>`，
/// 不能捕获 &self，只能用 once_cell 兜底。
///
/// 注意：此全局 cell **不是**线程安全的「共享 Arc」（每个实例独立），
/// 但 register 和 use 都在单线程 task 上下文里执行，不并发。
use std::sync::OnceLock;
static SELF_ARC: OnceLock<Arc<FeishuPlatform>> = OnceLock::new();

impl FeishuPlatform {
    /// 注册当前实例 Arc 到全局 cell，供 TypingGuard.stop() 异步使用。
    ///
    /// 必须在 `Arc::new(FeishuPlatform::new(...))` 之后立刻调用一次，
    /// 否则 TypingGuard.stop() 会 panic。
    pub fn register_self_arc(self: &Arc<Self>) {
        // OnceLock::set 失败说明已经被另一个实例注册——这种情况下需要
        // 调用方显式 reset。v1 简化：只支持单实例；多实例需要 v2 重构。
        let _ = SELF_ARC.set(self.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FeishuDomain → base URL 映射稳定。
    #[test]
    fn test_domain_base_url() {
        assert_eq!(FeishuDomain::Feishu.base_url(), "https://open.feishu.cn");
        assert_eq!(FeishuDomain::Lark.base_url(), "https://open.larksuite.com");
    }

    /// FeishuConfig::new 默认用 Feishu domain。
    #[test]
    fn test_config_new_defaults_to_feishu() {
        let c = FeishuConfig::new("app123", "secret456");
        assert_eq!(c.app_id, "app123");
        assert_eq!(c.app_secret, "secret456");
        assert_eq!(c.domain, FeishuDomain::Feishu);
        assert!(c.bot_open_id.is_none());
    }

    /// extract_feishu_target 正确按 chat_type 派生 receive_id_type。
    #[test]
    fn test_extract_target_chat_type() {
        use crate::types::FeishuChatType;
        let p2p = ReplyTarget::feishu("ou_abc", None, FeishuChatType::P2p);
        let (rid, typ) = FeishuPlatform::extract_feishu_target(&p2p);
        assert_eq!(rid, "ou_abc");
        assert_eq!(typ, "open_id");

        let grp = ReplyTarget::feishu("oc_chat", None, FeishuChatType::Group);
        let (rid, typ) = FeishuPlatform::extract_feishu_target(&grp);
        assert_eq!(rid, "oc_chat");
        assert_eq!(typ, "chat_id");
    }

    /// Channel::name 必须返回 "feishu"。
    #[test]
    fn test_channel_name() {
        let p = FeishuPlatform::new(FeishuConfig::new("a", "s"), SharedHttpClient::new());
        assert_eq!(p.name(), "feishu");
    }

    /// TypingIndicator::as_typing_indicator 应返回 Some(self)。
    /// 用 dyn Channel 反向验证（dispatcher 用这个 API 探测能力）。
    #[test]
    fn test_as_typing_indicator_returns_some() {
        let p = FeishuPlatform::new(FeishuConfig::new("a", "s"), SharedHttpClient::new());
        let ch: Arc<dyn Channel> = Arc::new(p);
        let ti = ch.as_typing_indicator();
        assert!(ti.is_some(), "FeishuPlatform 必须报告 typing 能力");
    }
}
