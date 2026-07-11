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

// --- P2ImCardActionTriggerV1 ---

#[derive(Debug, Serialize, Deserialize)]
pub struct P2ImCardActionTriggerV1 {
    pub schema: String,
    pub header: EventHeader,
    pub event: P2ImCardActionTriggerV1Data,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct P2ImCardActionTriggerV1Data {
    pub action: CardAction,
    #[serde(default)]
    pub context: Option<CardActionContext>,
    #[serde(default)]
    pub operator: Option<CardActionOperator>,
}

/// 飞书 card.action.trigger 事件里的 operator（点击卡片的用户）。
/// 注意：与 message 事件的 EventSender 不同——这里 open_id / user_id / union_id 是**平铺**字段，
/// 而非嵌套在 sender_id 下。飞书这两类事件的 operator schema 不同，若复用 EventSender 会因缺少
/// sender_id 字段报 `missing field sender_id`，导致整个事件反序列化失败、卡片点击静默失效。
/// 字段一律 Option + serde default，保证飞书增删字段也不会让反序列化失败。
#[derive(Debug, Serialize, Deserialize)]
pub struct CardActionOperator {
    #[serde(default)]
    pub open_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub union_id: Option<String>,
    #[serde(default)]
    pub tenant_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CardAction {
    pub tag: String,
    pub name: Option<String>,
    pub value: Option<HashMap<String, Value>>,
    #[serde(default)]
    pub option: Option<String>,
    #[serde(default)]
    pub form_value: Option<HashMap<String, Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CardActionContext {
    #[serde(rename = "open_chat_id")]
    pub open_chat_id: Option<String>,
    #[serde(rename = "open_message_id")]
    pub open_message_id: Option<String>,
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

struct P2ImCardActionTriggerV1Handler<F>
where
    F: Fn(P2ImCardActionTriggerV1) + 'static + Sync + Send,
{
    f: F,
}

impl<F> EventHandler for P2ImCardActionTriggerV1Handler<F>
where
    F: Fn(P2ImCardActionTriggerV1) + 'static + Sync + Send,
{
    fn handle(&self, payload: &[u8]) -> anyhow::Result<()> {
        let event: P2ImCardActionTriggerV1 = serde_json::from_slice(payload)?;
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
            // 已注册的事件处理器：正常分发
            handler.handle(payload)
        } else {
            // 未注册的事件：飞书可能推送任意类型事件（如 bot.added、chat.updated 等），
            // 不需要全部处理，warn 级别记录后静默跳过，避免 error + backtrace 噪音。
            tracing::warn!("No event processor found for event, ignoring: {handler_name}");
            Ok(())
        }
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

    pub fn register_p2_im_card_action_trigger_v1<F>(mut self, f: F) -> Result<Self, String>
    where
        F: Fn(P2ImCardActionTriggerV1) + 'static + Sync + Send,
    {
        // 飞书长连接推送「点击卡片按钮」回调时，header.event_type 的实际值是
        // `card.action.trigger`（不带 `im.` 前缀、也不带 `_v1` 后缀）。
        // do_without_validation 按 `{schema}.{event_type}` 拼出 handler_name 查表分发，
        // 因此这里的 key 必须是 `p2.card.action.trigger`，与线上事件对齐；
        // 早先误写成 `p2.im.card.action.trigger_v1` 会导致事件命中
        // "No event processor found" 分支被丢弃，表现为点击卡片菜单无任何反应。
        let key = "p2.card.action.trigger".to_string();
        if self.processor_map.contains_key(&key) {
            return Err(format!("processor already registered, type: {key}"));
        }
        let processor = P2ImCardActionTriggerV1Handler { f };
        self.processor_map.insert(key, Box::new(processor));
        Ok(self)
    }

    pub fn build(self) -> EventDispatcherHandler {
        EventDispatcherHandler {
            processor_map: self.processor_map,
        }
    }
}

#[cfg(test)]
// 单测里用 .expect() 表达「注册/分发必须成功」是合理的失败模式，故放行 expect_used。
// 项目整体策略见 Cargo.toml [lints.clippy]：expect_used 默认 warn，CI `-D warnings` 会升级为 deny，
// 注释虽声明单测除外，但配置未落地，故沿用现有 test 模块的 allow 写法（参考 config.rs）。
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// 复现飞书卡片点击事件的分发链路。
    /// 飞书长连接推送「点击卡片按钮」时，header.event_type 的实际值是 `card.action.trigger`
    /// （无 `im.` 前缀、无 `_v1` 后缀）。do_without_validation 按 `{schema}.{event_type}` 拼出
    /// handler_name 查 processor_map 分发，因此注册 key 必须是 `p2.card.action.trigger`，
    /// 否则事件被 "No event processor found" 丢弃，表现为点击 /help 菜单卡片无任何反应。
    /// 本测试用与线上一致的 event_type 构造 payload，断言处理器被真正调用。
    #[test]
    fn test_do_without_validation_dispatches_card_action_trigger() {
        // 用 AtomicBool 捕获处理器是否被调用。
        // 相比在闭包里直接 assert，它能清晰区分「事件没被分发」与「分发后反序列化失败」两种失败路径。
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let handler = EventDispatcherHandler::builder()
            .register_p2_im_card_action_trigger_v1(move |event| {
                // operator.open_id 必须被正确解析为平铺字段（而非 sender_id 嵌套），
                // 覆盖生产环境飞书推送的真实结构。
                assert_eq!(
                    event
                        .event
                        .operator
                        .as_ref()
                        .and_then(|op| op.open_id.as_deref()),
                    Some("ou_test_user"),
                    "operator.open_id 未正确解析（应为平铺字段）"
                );
                called_clone.store(true, Ordering::SeqCst);
            })
            .expect("register card action handler must succeed")
            .build();

        // 构造飞书实际推送的 card.action.trigger 事件 payload，覆盖与 message 事件不同的结构点：
        // - schema + header.event_type 决定分发 key（`card.action.trigger`，无 `im.`/`_v1`）；
        // - event.operator.open_id 是**平铺**字段，而非 message 事件的 sender_id.open_id 嵌套结构，
        //   若复用 EventSender 会报 `missing field sender_id`，故此处必须用平铺 operator 校验。
        let payload = br#"{
            "schema": "2.0",
            "header": { "event_type": "card.action.trigger" },
            "event": {
                "action": { "tag": "button", "value": { "action": "nav:/help common" } },
                "context": { "open_message_id": "om1", "open_chat_id": "oc1" },
                "operator": { "open_id": "ou_test_user" }
            }
        }"#;

        handler
            .do_without_validation(payload)
            .expect("dispatch should not error");

        assert!(
            called.load(Ordering::SeqCst),
            "card.action.trigger 事件未被分发到处理器（注册 key 与飞书 event_type 不匹配）"
        );
    }
}
