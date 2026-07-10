use std::sync::Arc;

use super::sdk::{AppType, CreateMessageRequest, CreateMessageRequestBody, LarkClient};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::codec::decode_message_content;
use super::config::{FeishuConfig, FeishuConnectionMode};
use super::message::ChannelMessage;

const MAX_RECONNECT_ATTEMPTS: u32 = 10;
const RECONNECT_BASE_DELAY_SECS: u64 = 2;

/// 去掉飞书群聊 @提及占位符（`@_user_N`），只保留用户实际输入的文本。
///
/// 飞书在用户 @某人 时，消息文本中会出现 `@_user_1` 等占位符，
/// 真正的提及信息在 `mentions` 数组里。executor 不需要这些占位符，
/// 去掉后 AI 收到的是干净的用户意图。
fn strip_mention_markers(text: &str) -> String {
    // 逐字符扫描：遇到 @_user_N 模式就跳过，其余字符原样保留
    let mut result = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '@' {
            // 检查是否匹配 @_user_ 后跟数字
            let rest = &text[i..];
            if rest.starts_with("@_user_") {
                // 跳过 @_user_ 前缀
                let after_prefix = i + "@_user_".len();
                // 跳过连续数字
                let mut end = after_prefix;
                while end < text.len() && text.as_bytes()[end].is_ascii_digit() {
                    end += 1;
                }
                // 跳过数字后的可选空格
                while end < text.len() && text.as_bytes()[end] == b' ' {
                    end += 1;
                }
                // 移动游标到跳过位置
                for _ in 0..(end - i - ch.len_utf8()) {
                    chars.next();
                }
                continue;
            }
        }
        result.push(ch);
    }
    result.trim().to_string()
}

/// Infer the Feishu `receive_id_type` from the ID prefix.
fn infer_receive_id_type(id: &str) -> &'static str {
    if id.starts_with("oc_") {
        "chat_id"
    } else if id.starts_with("on_") {
        "union_id"
    } else {
        "open_id"
    }
}

/// Simplified Feishu channel service.
///
/// Replaces the 4-crate clawrs-feishu workspace with direct open-lark usage.
/// No kernel, no event bus, no port/adapter pattern — just WebSocket listening + IM sending.
pub struct FeishuChannelService {
    client: Arc<LarkClient>,
    config: FeishuConfig,
}

impl FeishuChannelService {
    /// Create a new channel from config.
    pub fn new(config: FeishuConfig) -> Self {
        let base_url = config.domain.base_url().to_string();
        let client = LarkClient::builder(&config.app_id, &config.app_secret)
            .with_app_type(AppType::SelfBuild)
            .with_open_base_url(base_url)
            .with_enable_token_cache(true)
            .build();
        Self {
            client: Arc::new(client),
            config,
        }
    }

    /// Send a text message to a recipient (chat_id, open_id, or union_id).
    pub async fn send(&self, message: &str, recipient: &str) -> anyhow::Result<()> {
        let id_type = infer_receive_id_type(recipient);
        let body = CreateMessageRequestBody::builder()
            .receive_id(recipient)
            .msg_type("text")
            .content(super::codec::encode_text_message(message))
            .build();
        let request = CreateMessageRequest::builder()
            .receive_id_type(id_type)
            .request_body(body)
            .build();
        self.client
            .im
            .v1
            .message
            .create(request, None)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }

    /// Start listening for messages via WebSocket with automatic reconnection.
    ///
    /// Received messages are forwarded to `tx` as `ChannelMessage`.
    /// Runs the WebSocket client on a dedicated OS thread (because open-lark's
    /// `EventDispatcherHandler` is not `Send`).
    pub async fn listen(&self, tx: mpsc::Sender<ChannelMessage>) -> anyhow::Result<()> {
        if matches!(self.config.connection_mode, FeishuConnectionMode::Webhook) {
            anyhow::bail!("Webhook mode not yet implemented -- use websocket mode");
        }

        let config = self.client.config.clone();
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            if attempt > MAX_RECONNECT_ATTEMPTS {
                error!("WebSocket: exceeded max reconnect attempts ({})", MAX_RECONNECT_ATTEMPTS);
                return Err(anyhow::anyhow!("WebSocket: exceeded max reconnect attempts"));
            }

            if attempt > 1 {
                let delay = RECONNECT_BASE_DELAY_SECS * 2u64.pow((attempt - 2).min(5));
                warn!("WebSocket: reconnect attempt {}/{} in {}s", attempt, MAX_RECONNECT_ATTEMPTS, delay);
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            }

            let tx_clone = tx.clone();
            let config_clone = config.clone();

            let handle = std::thread::spawn(move || -> anyhow::Result<()> {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| anyhow::anyhow!("Failed to create tokio runtime for WS listener: {e}"))?;

                rt.block_on(async move {
                    use crate::feishu::sdk::{EventDispatcherHandler, LarkWsClient};

                    let tx = tx_clone;

                    let builder = EventDispatcherHandler::builder()
                        .register_p2_im_message_receive_v1(move |event| {
                            let msg = &event.event;
                            let sender_open_id = msg.sender.sender_id.open_id.clone().unwrap_or_default();
                            let raw_content = msg.message.content.clone();
                            let message_id = msg.message.message_id.clone();
                            let message_type = msg.message.message_type.clone();
                            let chat_type = msg.message.chat_type.clone();
                            let timestamp: u64 = msg.message.create_time.parse().unwrap_or(0);

                            let content = decode_message_content(&raw_content, &message_type);
                            // 飞书群聊 @提及 在文本中表现为 @_user_N 占位符，
                            // 实际提及数据在 mentions 数组中；这里去掉占位符，
                            // 只保留用户真正输入的内容向下传递。
                            let content = strip_mention_markers(&content);

                            let mentioned_open_ids: Vec<String> = msg
                                .message
                                .mentions
                                .as_ref()
                                .map(|ms| {
                                    ms.iter().filter_map(|m| m.id.open_id.clone()).collect()
                                })
                                .unwrap_or_default();

                            info!(
                                "WebSocket: received message from {} in {} ({}): {:?}",
                                sender_open_id, msg.message.chat_id, chat_type,
                                &content[..content.len().min(100)]
                            );

                            let channel_msg = ChannelMessage {
                                id: message_id,
                                sender: sender_open_id,
                                sender_type: msg.sender.sender_type.clone(),
                                content,
                                channel: msg.message.chat_id.clone(),
                                timestamp,
                                chat_type: Some(chat_type),
                                mentioned_open_ids,
                            };

                            if let Err(e) = tx.try_send(channel_msg) {
                                error!("Failed to forward message to channel bus: {e}");
                            } else {
                                debug!("Message forwarded to channel bus: chat_id={}", msg.message.chat_id);
                            }
                        });

                    let builder = match builder {
                        Ok(b) => b,
                        Err(e) => return Err(anyhow::anyhow!("Failed to register event handler: {e}")),
                    };

                    let builder = match builder
                        .register_p2_im_message_reaction_deleted_v1(|event| {
                            tracing::debug!(
                                "WebSocket: reaction deleted on message {} in chat {:?}",
                                event.event.message_id,
                                event.event.chat_id
                            );
                        }) {
                        Ok(b) => b,
                        Err(e) => return Err(anyhow::anyhow!("Failed to register event handler: {e}")),
                    };

                    let builder = match builder
                        .register_p2_im_message_reaction_created_v1(|event| {
                            tracing::debug!(
                                "WebSocket: reaction created on message {} in chat {:?}",
                                event.event.message_id,
                                event.event.chat_id
                            );
                        }) {
                        Ok(b) => b,
                        Err(e) => return Err(anyhow::anyhow!("Failed to register event handler: {e}")),
                    };

                    let event_handler = builder.build();

                    info!("WebSocket: connecting to Feishu event stream");
                    let cfg = Arc::new(config_clone);
                    LarkWsClient::open(cfg, event_handler)
                        .await
                        .map_err(|e| anyhow::anyhow!("{e}"))
                })
            });

            let result = tokio::task::spawn_blocking(move || {
                match handle.join() {
                    Ok(result) => result,
                    Err(panic_payload) => {
                        let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "unknown".to_string()
                        };
                        Err(anyhow::anyhow!("WS listener thread panicked: {msg}"))
                    }
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("Join error: {e}"))?;

            match result {
                Ok(()) => {
                    info!("WebSocket: connection closed normally, reconnecting...");
                }
                Err(e) => {
                    warn!("WebSocket: connection error: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::strip_mention_markers;

    /// 单个 @_user_1 占位符应被完整移除，剩余文本 trim 后返回
    #[test]
    fn test_strip_single_mention() {
        assert_eq!(strip_mention_markers("@_user_1 hi"), "hi");
    }

    /// 多个占位符连续出现时全部移除
    #[test]
    fn test_strip_multiple_mentions() {
        assert_eq!(strip_mention_markers("@_user_1 @_user_2 hello"), "hello");
    }

    /// 占位符在文本中间时，前后内容保留
    #[test]
    fn test_strip_mention_in_middle() {
        assert_eq!(strip_mention_markers("before @_user_1 after"), "before after");
    }

    /// 无占位符时原样返回（trim 后）
    #[test]
    fn test_no_mentions_unchanged() {
        assert_eq!(strip_mention_markers("hello world"), "hello world");
    }

    /// 空字符串返回空串
    #[test]
    fn test_empty_string() {
        assert_eq!(strip_mention_markers(""), "");
    }

    /// 只有占位符时返回空串
    #[test]
    fn test_only_mention() {
        assert_eq!(strip_mention_markers("@_user_1"), "");
    }

    /// 大数字的占位符也能正确移除
    #[test]
    fn test_large_user_id() {
        assert_eq!(strip_mention_markers("@_user_99999 test"), "test");
    }
}
