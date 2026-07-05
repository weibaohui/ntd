use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct EventContext {
    pub ts: Option<String>,
    pub uuid: Option<String>,
    pub token: Option<String>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub schema: Option<String>,
    pub header: Option<EventHeader>,
    pub event: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventHeader {
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub create_time: Option<String>,
    pub token: Option<String>,
    pub app_id: Option<String>,
    pub tenant_key: Option<String>,
}

// --- P2ImMessageReceiveV1 ---

#[derive(Debug, Serialize, Deserialize)]
pub struct P2ImMessageReceiveV1 {
    pub schema: String,
    pub header: EventHeader,
    pub event: P2ImMessageReceiveV1Data,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct P2ImMessageReceiveV1Data {
    pub sender: EventSender,
    pub message: EventMessage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventSender {
    pub sender_id: UserId,
    #[serde(default)]
    pub sender_type: Option<String>,
    #[serde(default)]
    pub tenant_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserId {
    #[serde(default)]
    pub union_id: Option<String>,
    pub user_id: Option<String>,
    #[serde(default)]
    pub open_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventMessage {
    pub message_id: String,
    pub root_id: Option<String>,
    pub parent_id: Option<String>,
    pub create_time: String,
    pub update_time: String,
    pub chat_id: String,
    pub thread_id: Option<String>,
    pub chat_type: String,
    pub message_type: String,
    pub content: String,
    pub mentions: Option<Vec<MentionEvent>>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MentionEvent {
    pub key: String,
    pub id: UserId,
    pub name: String,
    pub tenant_key: String,
}

// --- P2ImMessageReactionDeletedV1 ---

#[derive(Debug, Serialize, Deserialize)]
pub struct P2ImMessageReactionDeletedV1 {
    pub schema: String,
    pub header: EventHeader,
    pub event: P2ImMessageReactionDeletedV1Data,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct P2ImMessageReactionDeletedV1Data {
    #[serde(default)]
    pub sender: Option<EventSender>,
    pub message_id: String,
    #[serde(default)]
    pub chat_id: Option<String>,
    #[serde(default)]
    pub reaction_type: serde_json::Value,
    #[serde(default)]
    pub create_time: Option<String>,
}

// --- P2ImMessageReactionCreatedV1 ---

#[derive(Debug, Serialize, Deserialize)]
pub struct P2ImMessageReactionCreatedV1 {
    pub schema: String,
    pub header: EventHeader,
    pub event: P2ImMessageReactionCreatedV1Data,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct P2ImMessageReactionCreatedV1Data {
    #[serde(default)]
    pub sender: Option<EventSender>,
    pub message_id: String,
    #[serde(default)]
    pub chat_id: Option<String>,
    #[serde(default)]
    pub reaction_type: serde_json::Value,
    #[serde(default)]
    pub create_time: Option<String>,
}

// --- Event dispatcher ---

pub trait EventHandler {
    fn handle(&self, payload: &[u8]) -> anyhow::Result<()>;
}

struct P2ImMessageReceiveV1Handler<F>
where
    F: Fn(P2ImMessageReceiveV1) + 'static,
{
    f: F,
}

impl<F> EventHandler for P2ImMessageReceiveV1Handler<F>
where
    F: Fn(P2ImMessageReceiveV1) + 'static + Sync + Send,
{
    fn handle(&self, payload: &[u8]) -> anyhow::Result<()> {
        let message: P2ImMessageReceiveV1 = serde_json::from_slice(payload)?;
        (self.f)(message);
        Ok(())
    }
}

struct P2ImMessageReactionDeletedV1Handler<F>
where
    F: Fn(P2ImMessageReactionDeletedV1) + 'static,
{
    f: F,
}

impl<F> EventHandler for P2ImMessageReactionDeletedV1Handler<F>
where
    F: Fn(P2ImMessageReactionDeletedV1) + 'static + Sync + Send,
{
    fn handle(&self, payload: &[u8]) -> anyhow::Result<()> {
        let event: P2ImMessageReactionDeletedV1 = serde_json::from_slice(payload)?;
        (self.f)(event);
        Ok(())
    }
}

struct P2ImMessageReactionCreatedV1Handler<F>
where
    F: Fn(P2ImMessageReactionCreatedV1) + 'static,
{
    f: F,
}

impl<F> EventHandler for P2ImMessageReactionCreatedV1Handler<F>
where
    F: Fn(P2ImMessageReactionCreatedV1) + 'static + Sync + Send,
{
    fn handle(&self, payload: &[u8]) -> anyhow::Result<()> {
        let event: P2ImMessageReactionCreatedV1 = serde_json::from_slice(payload)?;
        (self.f)(event);
        Ok(())
    }
}

pub struct EventDispatcherHandler {
    processor_map: HashMap<String, Box<dyn EventHandler>>,
}

impl EventDispatcherHandler {
    pub fn builder() -> EventDispatcherHandlerBuilder {
        EventDispatcherHandlerBuilder {
            processor_map: HashMap::new(),
        }
    }

    pub fn do_without_validation(&self, payload: &[u8]) -> anyhow::Result<()> {
        let mut context: EventContext = serde_json::from_slice(payload)?;
        let event_type = context
            .header
            .as_ref()
            .and_then(|h| h.event_type.clone())
            .unwrap_or_default();
        tracing::info!("Received Feishu event: {event_type}");

        if context.schema.is_some() {
            context.schema = Some("p2".to_string());
            context.type_ = context.header.as_ref().and_then(|h| h.event_type.clone());
            context.token = context.header.as_ref().and_then(|h| h.token.clone());
        } else if context.uuid.is_some() {
            context.schema = Some("p1".to_string());
            context.type_ = context.event.get("type").map(|v| v.to_string());
        }

        let handler_name = format!(
            "{}.{}",
            context.schema.unwrap_or_default(),
            context.type_.unwrap_or_default()
        );

        if let Some(handler) = self.processor_map.get(&handler_name) {
            handler.handle(payload)
        } else if Self::is_ignorable_event(&handler_name) {
            tracing::debug!("Ignoring Feishu event without processor: {handler_name}");
            Ok(())
        } else {
            tracing::warn!("No event processor found for event: {handler_name}");
            Err(anyhow::anyhow!("event processor {} not found", handler_name))
        }
    }

    /// 判断是否为已知但无需处理的事件。
    fn is_ignorable_event(handler_name: &str) -> bool {
        matches!(
            handler_name,
            "p2.im.message.message_read_v1"
        )
    }
}

pub struct EventDispatcherHandlerBuilder {
    processor_map: HashMap<String, Box<dyn EventHandler>>,
}

impl EventDispatcherHandlerBuilder {
    pub fn register_p2_im_message_receive_v1<F>(mut self, f: F) -> Result<Self, String>
    where
        F: Fn(P2ImMessageReceiveV1) + 'static + Sync + Send,
    {
        let key = "p2.im.message.receive_v1".to_string();
        if self.processor_map.contains_key(&key) {
            return Err(format!("processor already registered, type: {key}"));
        }
        let processor = P2ImMessageReceiveV1Handler { f };
        self.processor_map.insert(key, Box::new(processor));
        Ok(self)
    }

    pub fn register_p2_im_message_reaction_deleted_v1<F>(mut self, f: F) -> Result<Self, String>
    where
        F: Fn(P2ImMessageReactionDeletedV1) + 'static + Sync + Send,
    {
        let key = "p2.im.message.reaction.deleted_v1".to_string();
        if self.processor_map.contains_key(&key) {
            return Err(format!("processor already registered, type: {key}"));
        }
        let processor = P2ImMessageReactionDeletedV1Handler { f };
        self.processor_map.insert(key, Box::new(processor));
        Ok(self)
    }

    pub fn register_p2_im_message_reaction_created_v1<F>(mut self, f: F) -> Result<Self, String>
    where
        F: Fn(P2ImMessageReactionCreatedV1) + 'static + Sync + Send,
    {
        let key = "p2.im.message.reaction.created_v1".to_string();
        if self.processor_map.contains_key(&key) {
            return Err(format!("processor already registered, type: {key}"));
        }
        let processor = P2ImMessageReactionCreatedV1Handler { f };
        self.processor_map.insert(key, Box::new(processor));
        Ok(self)
    }

    pub fn build(self) -> EventDispatcherHandler {
        EventDispatcherHandler {
            processor_map: self.processor_map,
        }
    }
}
