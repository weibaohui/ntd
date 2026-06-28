//! WS 收到的 `ChannelMessage` → ntd-connect `IncomingMessage` 转换。
//!
//! # 与 dry-run 步骤 7 的对应
//!
//! 步骤 7「WS 桥接到 FeishuPlatform」的核心：把 backend 飞书 SDK 的
//! 私有类型（`crate::feishu::ChannelMessage`）转成 ntd-connect 公开 trait
//! 能消费的 `IncomingMessage`。**这个转换必须在 backend 内做**，
//! 因为 ntd-connect crate 不能反向依赖 backend SDK。
//!
//! # 设计要点
//!
//! - `timestamp` 是 u64 秒，`timestamp_ms` 是 i64 毫秒——转换时 × 1000。
//! - `chat_type` 是 `Option<String>`，从 "p2p" / "group" 转 enum；
//!   未知值 fallback 到 Group（保守默认，匹配 backend 现有逻辑）。
//! - `content` 是 JSON 字符串（飞书富文本 / 文本事件统一编码）；
//!   v1 仅提取 `text` 字段做 Text，其它结构 fallback 到空字符串。
//! - `bot_open_id` 由调用方传入（决定 `is_from_self`）。
//! - `sender_type` ("user" / "app") → `SenderKind::User` / `SenderKind::Bot`。
//!
//! 后续步骤 9 把 `channel_message_to_incoming` + dispatcher.on_message 串起来，
//! 完成「WS 收消息 → dispatcher 分发 → worker 处理」整条流水线。

use crate::feishu::ChannelMessage;
use ntd_connect::types::{
    FeishuChatType, IncomingContent, IncomingMessage, PlatformKind, SenderId, SenderKind,
    SessionKey,
};

/// ChannelMessage → IncomingMessage 转换。
///
/// `bot_open_id`：用于设置 `is_from_self`（dispatcher 据此跳过 bot 复读）。
/// 传 `None` 表示还没解析到 bot_open_id，所有消息都视为「不是自己发的」；
/// 这在 v1 启动早期 / 未完成 resolve 阶段是安全 fallback。
pub fn channel_message_to_incoming(msg: &ChannelMessage, bot_open_id: Option<&str>) -> IncomingMessage {
    // sender_type 转 SenderKind
    let sender_kind = match msg.sender_type.as_deref() {
        Some("app") => SenderKind::Bot,
        _ => SenderKind::User,
    };

    // chat_type 转 FeishuChatType（未知值 fallback 到 Group）
    let chat_type = match msg.chat_type.as_deref() {
        Some("p2p") => FeishuChatType::P2p,
        _ => FeishuChatType::Group,
    };

    // content 是飞书富文本 JSON；v1 仅提取 text 字段。
    let content = parse_text_content(&msg.content);

    // is_from_self：sender == bot_open_id 时为 true。
    let is_from_self = bot_open_id
        .map(|bid| msg.sender == bid)
        .unwrap_or(false);

    // is_mention：群聊且 bot_open_id 在 mentioned_open_ids 里。
    let is_mention = matches!(chat_type, FeishuChatType::Group)
        && bot_open_id
            .map(|bid| msg.mentioned_open_ids.iter().any(|id| id == bid))
            .unwrap_or(false);

    IncomingMessage {
        platform: PlatformKind::Feishu,
        session_key: SessionKey::derive(PlatformKind::Feishu, &msg.channel, None),
        sender: SenderId::new(&msg.sender),
        content,
        reply_target: ntd_connect::types::ReplyTarget::feishu(&msg.channel, None, chat_type),
        timestamp_ms: msg.timestamp as i64 * 1000,
        raw_message_id: msg.id.clone(),
        is_mention,
        sender_kind,
        is_from_self,
        mentioned_open_ids: msg.mentioned_open_ids.clone(),
    }
}

/// 解析飞书 content JSON，提取 `text` 字段；解析失败返回空文本。
///
/// 飞书富文本格式形如 `{"text": "hello @bot"}`，但也可能嵌套 segments
/// 或 image keys。v1 仅取 `text`，其它结构 fallback 到 JSON 原始字符串
/// （v2 加富文本解析时替换）。
fn parse_text_content(raw: &str) -> IncomingContent {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return IncomingContent::Text(String::new());
    }
    // 尝试 JSON 解析
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(text) = parsed.get("text").and_then(|v| v.as_str()) {
            return IncomingContent::Text(text.to_string());
        }
    }
    // 不是 JSON 或没有 text 字段：当作纯文本
    IncomingContent::Text(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feishu::ChannelMessage;

    fn make_msg(content: &str, chat_type: Option<&str>) -> ChannelMessage {
        ChannelMessage {
            id: "om_xxx".into(),
            sender: "ou_user_a".into(),
            sender_type: Some("user".into()),
            content: content.into(),
            channel: "oc_chat".into(),
            timestamp: 1_700_000_000,
            chat_type: chat_type.map(String::from),
            mentioned_open_ids: vec![],
        }
    }

    /// P2P 场景：chat_type=P2p、is_mention=false（私聊无 mention 概念）。
    #[test]
    fn test_p2p_conversion() {
        let msg = make_msg(r#"{"text":"hi"}"#, Some("p2p"));
        let incoming = channel_message_to_incoming(&msg, None);
        assert_eq!(incoming.sender.as_str(), "ou_user_a");
        assert!(matches!(incoming.content, IncomingContent::Text(t) if t == "hi"));
        assert!(!incoming.is_mention, "P2P 无 mention 概念");
        assert_eq!(incoming.sender_kind, SenderKind::User);
        assert!(!incoming.is_from_self);
        assert_eq!(incoming.timestamp_ms, 1_700_000_000_000); // 秒 → 毫秒
        // P2p chat_type
        if let ntd_connect::types::ReplyTarget::Feishu { chat_type, .. } = &incoming.reply_target {
            assert_eq!(*chat_type, FeishuChatType::P2p);
        } else {
            panic!("expected Feishu reply target");
        }
    }

    /// 群聊场景：chat_type=Group、bot_open_id 在 mentioned_open_ids 时 is_mention=true。
    #[test]
    fn test_group_with_mention() {
        let mut msg = make_msg(r#"{"text":"@bot help"}"#, Some("group"));
        msg.mentioned_open_ids = vec!["ou_bot".into()];
        let incoming = channel_message_to_incoming(&msg, Some("ou_bot"));
        assert!(incoming.is_mention, "bot 被 @ 时 is_mention=true");
        assert!(!incoming.is_from_self);
    }

    /// bot 自己发的消息：is_from_self=true（dispatcher 据此跳过）。
    #[test]
    fn test_is_from_self() {
        let mut msg = make_msg(r#"{"text":"echo"}"#, Some("p2p"));
        msg.sender = "ou_bot".into();
        msg.sender_type = Some("app".into());
        let incoming = channel_message_to_incoming(&msg, Some("ou_bot"));
        assert!(incoming.is_from_self);
        assert_eq!(incoming.sender_kind, SenderKind::Bot);
    }

    /// bot_open_id 在 mentioned_open_ids 但 sender 不是 bot：is_from_self=false。
    /// （罕见：群聊里其他人 @ bot 时不属于 self-sent）
    #[test]
    fn test_other_sender_with_bot_mention() {
        let mut msg = make_msg(r#"{"text":"@bot"}"#, Some("group"));
        msg.mentioned_open_ids = vec!["ou_bot".into()];
        let incoming = channel_message_to_incoming(&msg, Some("ou_bot"));
        assert!(!incoming.is_from_self);
        assert!(incoming.is_mention);
    }

    /// content 不是 JSON（裸文本）→ 当 Text 处理。
    #[test]
    fn test_non_json_content_falls_back_to_text() {
        let msg = make_msg("plain text message", Some("p2p"));
        let incoming = channel_message_to_incoming(&msg, None);
        assert!(matches!(incoming.content, IncomingContent::Text(t) if t == "plain text message"));
    }

    /// content JSON 无 text 字段 → fallback 到原始 JSON 字符串。
    #[test]
    fn test_json_without_text_field_falls_back_to_raw() {
        let msg = make_msg(r#"{"other":"value"}"#, Some("p2p"));
        let incoming = channel_message_to_incoming(&msg, None);
        // 原始 JSON 字符串保留（含花括号）
        if let IncomingContent::Text(t) = &incoming.content {
            assert!(t.contains("other"));
            assert!(t.contains("value"));
        } else {
            panic!("expected Text variant");
        }
    }

    /// content 是空字符串 → 不会 panic，content = Text("")。
    #[test]
    fn test_empty_content() {
        let msg = make_msg("", Some("p2p"));
        let incoming = channel_message_to_incoming(&msg, None);
        assert!(matches!(incoming.content, IncomingContent::Text(t) if t.is_empty()));
    }

    /// unknown chat_type（不是 "p2p" 也不是 "group"）→ fallback 到 Group。
    /// 保守默认：群聊路径通常更宽松（白名单等过滤在 dispatcher 做）。
    #[test]
    fn test_unknown_chat_type_falls_back_to_group() {
        let msg = make_msg("hi", Some("unknown_type"));
        let incoming = channel_message_to_incoming(&msg, None);
        if let ntd_connect::types::ReplyTarget::Feishu { chat_type, .. } = &incoming.reply_target {
            assert_eq!(*chat_type, FeishuChatType::Group);
        } else {
            panic!("expected Feishu reply target");
        }
    }
}

/// IncomingMessage → ChannelMessage 反向转换（步骤 11 切流用）。
///
/// 把 dispatcher 收到的 `IncomingMessage` 还原为 `feishu_listener`
/// stage 函数能吃的 `ChannelMessage`。
///
/// **精度损失**：
/// - `mentioned_open_ids` 默认空（IncomingMessage 只记 is_mention，不存 ID 列表）
/// - `sender_type` 从 `IncomingMessage::sender_kind` 推导（User→"user", Bot→"app"）
/// - `content` 解析 JSON 失败时 fallback 为原文本（与 `channel_message_to_incoming` 对称）
/// - `timestamp` 从 ms 转回 s（飞书 SDK 内部用秒）
///
/// v2 加 `IncomingMessage::mentioned_open_ids: Vec<String>` 字段可消除此损失。
pub fn incoming_to_channel_message(msg: &ntd_connect::types::IncomingMessage) -> crate::feishu::ChannelMessage {
    use ntd_connect::types::{IncomingContent, SenderKind};

    let chat_type = match &msg.reply_target {
        ntd_connect::types::ReplyTarget::Feishu { chat_type, .. } => match chat_type {
            ntd_connect::types::FeishuChatType::P2p => Some("p2p".to_string()),
            ntd_connect::types::FeishuChatType::Group => Some("group".to_string()),
        },
        // 未来 ReplyTarget 加新变体时这里的精度会丢；v1 没有 unknown 落点
        // 所以没加 _ 分支——加 #[non_exhaustive] 后编译器强制要求。
        #[allow(unreachable_patterns)]
        _ => None,
    };

    let sender_type = match msg.sender_kind {
        SenderKind::User => Some("user".to_string()),
        SenderKind::Bot => Some("app".to_string()),
        // 未来 SenderKind 加新变体时的 fallback
        _ => None,
    };

    // content 序列化：飞书 SDK 期望原始 JSON 字符串
    let content = match &msg.content {
        IncomingContent::Text(s) => s.clone(),
        IncomingContent::Image(_) | IncomingContent::File(_) | IncomingContent::Audio(_) => {
            // 飞书 v1 没富文本协议，降级为占位文本
            String::from("[ntd-connect v1: non-text content not supported]")
        }
        #[allow(unreachable_patterns)]
        _ => String::new(),
    };

    let (channel, sender) = match &msg.reply_target {
        ntd_connect::types::ReplyTarget::Feishu { chat_id, .. } => {
            (chat_id.clone(), msg.sender.as_str().to_string())
        }
        #[allow(unreachable_patterns)]
        _ => (String::new(), String::new()),
    };

    crate::feishu::ChannelMessage {
        id: msg.raw_message_id.clone(),
        sender,
        sender_type,
        content,
        channel,
        timestamp: (msg.timestamp_ms / 1000) as u64,
        chat_type,
        // 反向恢复 mentioned_open_ids，消除 v1 精度损失
        mentioned_open_ids: msg.mentioned_open_ids.clone(),
    }
}

#[cfg(test)]
mod reverse_tests {
    use super::*;
    use ntd_connect::types::{
        FeishuChatType, IncomingContent, PlatformKind, ReplyTarget, SenderId, SenderKind,
        SessionKey,
    };

    fn sample_incoming() -> ntd_connect::types::IncomingMessage {
        ntd_connect::types::IncomingMessage {
            platform: PlatformKind::Feishu,
            session_key: SessionKey::derive(PlatformKind::Feishu, "oc_test", None),
            sender: SenderId::new("ou_user"),
            content: IncomingContent::Text("hi".into()),
            reply_target: ReplyTarget::feishu("oc_test", None, FeishuChatType::P2p),
            timestamp_ms: 1_700_000_000_500,
            raw_message_id: "om_xyz".into(),
            is_mention: true,
            sender_kind: SenderKind::User,
            is_from_self: false,
            mentioned_open_ids: vec![],
        }
    }

    #[test]
    fn test_incoming_to_channel_roundtrip_basic() {
        let inc = sample_incoming();
        let ch = incoming_to_channel_message(&inc);
        assert_eq!(ch.id, "om_xyz");
        assert_eq!(ch.sender, "ou_user");
        assert_eq!(ch.channel, "oc_test");
        assert_eq!(ch.content, "hi");
        assert_eq!(ch.chat_type, Some("p2p".to_string()));
        assert_eq!(ch.sender_type, Some("user".to_string()));
        // timestamp: ms -> s
        assert_eq!(ch.timestamp, 1_700_000_000);
    }

    #[test]
    fn test_incoming_to_channel_group_chat() {
        let mut inc = sample_incoming();
        inc.reply_target =
            ReplyTarget::feishu("oc_group", None, FeishuChatType::Group);
        let ch = incoming_to_channel_message(&inc);
        assert_eq!(ch.chat_type, Some("group".to_string()));
    }

    #[test]
    fn test_incoming_to_channel_sender_kind_bot() {
        let mut inc = sample_incoming();
        inc.sender_kind = SenderKind::Bot;
        let ch = incoming_to_channel_message(&inc);
        assert_eq!(ch.sender_type, Some("app".to_string()));
    }
}
