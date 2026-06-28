//! ntd-connect 共享类型定义。
//!
//! 设计要点：
//! - **enum 优先于 trait object**：`ReplyTarget` 用 enum 而非 `Box<dyn Any>`
//!   携带 platform-specific 字段，避免 Go 的 type erasure 模式。
//!   编译期就能检查 Channel 实现是否覆盖了所有 variant。
//! - **newtype 包装字符串**：平台 ID / sender / session key 等用 newtype
//!   包装，调用方不会把不同语义的字符串混用。
//! - **大字段用 `String` + `Option<String>`** 而不是 smart pointer：
//!   IncomingMessage 是热路径对象，避免 Arc/Box 引入额外分配。

use serde::{Deserialize, Serialize};

// =====================================================================
// 平台标识：哪个 channel、什么 chat 类型
// =====================================================================

/// 消息来源平台类型。
///
/// v1 只实现飞书；变体集是开放枚举（non_exhaustive），后续加 channel
/// 不会破坏调用方 match 的 exhaustive 检查（需显式 `_ =>` 兜底）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PlatformKind {
    /// 飞书 / Lark。
    Feishu,
}

/// 飞书 chat 类型。私聊与群聊的 reply 行为不同（mention、@、群白名单）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeishuChatType {
    /// 私聊（p2p）。
    P2p,
    /// 群聊。
    Group,
}

/// 消息发送者标识（通常是 open_id / user_id）。
/// 用 newtype 避免与其它字符串混用。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SenderId(pub String);

impl SenderId {
    /// 构造一个新的 sender id。
    pub fn new(id: impl Into<String>) -> Self {
        SenderId(id.into())
    }
    /// 取内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Session key：dispatcher 用它做 session 哈希，决定 per-session 锁的归属。
///
/// 格式约定（参考 cc-connect `core/session.go:788-804`）：
/// - DM 默认：`"{platform}:{chat_id}:{sender_id}"`
/// - 群聊共享：`"{platform}:{chat_id}"`（sender 留空表示群内共享 session）
/// - thread 隔离：`"{platform}:{chat_id}:root:{root_id}"`（v2 再用）
///
/// v1 只暴露 [`SessionKey::derive`] 构造和 [`SessionKey::parse`] 反解，
/// 不暴露可变 builder，避免调用方自己拼出非法格式。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionKey(pub String);

impl SessionKey {
    /// 派生 session key。
    ///
    /// `sender == None` 表示群聊共享模式；`Some(sender)` 表示 DM
    /// 或群内按 sender 隔离。`platform` 转成小写字符串写入 key。
    pub fn derive(platform: PlatformKind, chat_id: &str, sender: Option<&SenderId>) -> Self {
        let p = platform_name(platform);
        match sender {
            // 群聊共享：sender 为空。
            None => SessionKey(format!("{p}:{chat_id}")),
            // DM 或群内隔离。
            Some(s) => SessionKey(format!("{p}:{chat_id}:{}", s.as_str())),
        }
    }

    /// 解析 session key 为 (platform, chat_id, sender)。
    ///
    /// 解析失败返回 None（绝不 panic），让 dispatcher 走 fallback 分支。
    pub fn parse(&self) -> Option<(PlatformKind, &str, Option<&str>)> {
        let parts: Vec<&str> = self.0.split(':').collect();
        match parts.as_slice() {
            // "platform:chat_id"
            [p, chat] => Some((parse_platform(p)?, chat, None)),
            // "platform:chat_id:sender"
            [p, chat, sender] => Some((parse_platform(p)?, chat, Some(*sender))),
            // 其它（含 thread 模式）暂不识别。
            _ => None,
        }
    }

    /// 取内部字符串。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// PlatformKind → 写入 SessionKey 的小写字符串。
fn platform_name(kind: PlatformKind) -> &'static str {
    match kind {
        PlatformKind::Feishu => "feishu",
    }
}

/// SessionKey 字符串 → PlatformKind；未知字符串返回 None。
fn parse_platform(s: &str) -> Option<PlatformKind> {
    match s {
        "feishu" => Some(PlatformKind::Feishu),
        _ => None,
    }
}

// =====================================================================
// 内容类型：入站 / 出站消息体
// =====================================================================

/// 入站消息的内容负载。
///
/// 不同 IM 平台消息体格式差异大，先归一化成这几种类型；具体 channel
/// 实现负责把自家格式映射进来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum IncomingContent {
    /// 纯文本。feishu 的 text / post 都会归一到这里。
    Text(String),
    /// 图片附件。
    Image(Attachment),
    /// 文件附件。
    File(Attachment),
    /// 音频 / 语音消息。
    Audio(Attachment),
}

/// 出站消息内容。
///
/// 同样的内容类型在不同的 channel 上渲染不同：飞书的 `Card` 是交互卡片，
/// Telegram 没有原生 Card 会降级为 Markdown + 按钮 URL。Channel 实现
/// 决定如何把 [`OutgoingContent`] 映射到自家 API。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OutgoingContent {
    /// 纯文本。
    Text(String),
    /// Markdown 文本（部分 channel 降级为纯文本 + 链接）。
    Markdown(String),
    /// 图片附件（必须可公网访问或本 channel 接受的上传方式）。
    Image(Attachment),
    /// 飞书 / 钉钉的原生卡片（自由 JSON）。
    Card(serde_json::Value),
    /// 通用文件。
    File(Attachment),
}

/// 通用附件描述。
///
/// `url` 是 IM 平台可访问的链接；channel 实现层可能需要先上传到自家
/// 服务拿到 url 再发。`mime_type` 缺省时由 channel 自己 sniff。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// 附件 URL（http/https 或 channel 接受的 scheme）。
    pub url: String,
    /// MIME 类型，例如 `image/png`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// 原始文件名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// 字节数（可选，channel 实现可用它做大小校验）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

impl Attachment {
    /// 最小构造：只指定 URL，其余字段 None。
    pub fn new(url: impl Into<String>) -> Self {
        Attachment {
            url: url.into(),
            mime_type: None,
            filename: None,
            size_bytes: None,
        }
    }
}

// =====================================================================
// 消息 / 回复目标
// =====================================================================

/// Channel → Dispatcher 传递的入站消息。
///
/// 这是 dispatcher 看到的唯一入参；channel 实现把自家消息解码后构造
/// 这个结构。`raw_message_id` 用于 dispatcher 的 dedup。
///
/// # 字段填充责任
///
/// 所有字段都**必须**由 channel 实现层填好，dispatcher 不应再做猜测：
/// - `is_mention` / `sender_kind`：channel 解码自家协议帧时直接拿
///   到，比 dispatcher 事后推断准确。
/// - `is_from_self`：channel 已知 resolved bot_open_id，
///   `sender == bot_open_id` 时填 `true`；dispatcher 不应持有
///   bot_open_id（多 bot 场景会泄露）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncomingMessage {
    /// 来源平台。
    pub platform: PlatformKind,
    /// session key（由 [`SessionKey::derive`] 生成）。
    pub session_key: SessionKey,
    /// 消息发送者。
    pub sender: SenderId,
    /// 消息内容。
    pub content: IncomingContent,
    /// 回复目标（channel 实现填好，用于后续 reply/send）。
    pub reply_target: ReplyTarget,
    /// 消息时间戳（毫秒）。用于 watermark 防陈旧消息。
    pub timestamp_ms: i64,
    /// 平台原始 message_id，用于 dedup。
    pub raw_message_id: String,
    /// 群聊是否 @ 了 bot。仅群聊消息有意义；私聊固定 `false`。
    ///
    /// dispatcher 闸 3 用它判断群聊是否需要响应（参考
    /// `feishu_listener.rs:306 should_skip_for_message_filters`）。
    #[serde(default)]
    pub is_mention: bool,
    /// 发送者类型。多数 IM 平台在事件帧里直接给 `sender_type`
    /// （user / bot / app），channel 实现直接透传即可。
    #[serde(default)]
    pub sender_kind: SenderKind,
    /// 是否 bot 自己发的消息。channel 解码时由
    /// `sender == resolved_bot_open_id` 判断；dispatcher 据此跳过
    /// （避免 bot 复读自己的回复触发死循环）。
    #[serde(default)]
    pub is_from_self: bool,
}

/// 发送者类型。
///
/// 区分 user / bot 是多数 IM 协议的 first-class 字段（飞书
/// `sender_type: "user" | "app"` / Slack `subtype: "bot"`）。
/// dispatcher 用它做几件事：
/// - 跳过 `is_from_self`（已在 `IncomingMessage` 字段里集中判断）
/// - 渲染消息时区分「智能体」和「用户」（前端表格标签）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SenderKind {
    /// 普通用户。默认值，覆盖「未知 / 平台未分类」场景。
    #[default]
    User,
    /// 机器人 / 应用账号。
    Bot,
}

/// Reply 调用附带的上下文（超时、trace 等）。
///
/// v1 只有超时；后续可扩展 trace_id、retry policy。
#[derive(Debug, Clone)]
pub struct ReplyContext {
    /// 单次 reply 调用的最长等待时间。
    pub timeout: std::time::Duration,
}

impl Default for ReplyContext {
    fn default() -> Self {
        ReplyContext {
            timeout: std::time::Duration::from_secs(30),
        }
    }
}

/// 回复目标。channel 用这个结构定位「把消息发到哪里 / 回哪条」。
///
/// v1 只支持飞书；新增 channel 时加新变体即可。`#[non_exhaustive]` 让
/// match 在编译期报错，强制 dispatcher 显式处理每个 channel。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReplyTarget {
    /// 飞书回复目标。
    Feishu {
        /// chat_id（p2p 是 open_id，群聊是 chat_id）。
        chat_id: String,
        /// 要回复的消息 id（首条 reply 时填，后续 edit/update 可空）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        /// p2p 还是群聊。
        chat_type: FeishuChatType,
    },
}

impl ReplyTarget {
    /// 飞书 reply target 的便捷构造。
    pub fn feishu(chat_id: impl Into<String>, message_id: Option<String>, chat_type: FeishuChatType) -> Self {
        ReplyTarget::Feishu {
            chat_id: chat_id.into(),
            message_id,
            chat_type,
        }
    }

    /// 命中目标所属的平台。
    pub fn platform(&self) -> PlatformKind {
        match self {
            ReplyTarget::Feishu { .. } => PlatformKind::Feishu,
        }
    }
}

// =====================================================================
// Agent 相关类型
// =====================================================================

/// Agent 权限请求的回执结果。
///
/// Agent 在执行过程中可能弹出权限请求（例如 Claude Code 的
/// `permission-prompt-tool`），dispatcher 把请求转给上层（用户 / 规则引擎），
/// 拿到结果后回传给 Agent。
///
/// 默认值是 `Allow`：v1 阶段 ntd-connect 不实现 permission hook 引擎
/// （设计稿 v1 不做项），上层如果想要更严格的安全策略需要在 dispatcher
/// 之外加 rule layer。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PermissionResult {
    /// 允许这一次调用。
    #[default]
    Allow,
    /// 拒绝这一次调用。
    Deny,
    /// 总是允许同类型调用（写规则到 settings）。
    AllowAlways,
}

/// Agent 一次 turn 的 usage 统计。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// 输入 token 数。
    #[serde(default)]
    pub input_tokens: u64,
    /// 输出 token 数。
    #[serde(default)]
    pub output_tokens: u64,
    /// 缓存命中 token（Claude API 才有）。
    #[serde(default)]
    pub cache_read_tokens: u64,
}

/// Agent session 列表接口返回的元信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionInfo {
    /// session ID（Claude Code 是磁盘 JSONL 文件名）。
    pub session_id: String,
    /// session 显示标题（首条 user message 截断）。
    pub title: String,
    /// 最后活跃时间（毫秒）。
    pub last_active_ms: i64,
}

/// Agent 调用上下文。
///
/// 传入 `Agent::start_session` 的环境信息。v1 只有 work_dir；
/// 后续可加 env vars / model override。
#[derive(Debug, Clone, Default)]
pub struct AgentContext {
    /// 工作目录（Claude Code 在这个目录下读 `.claude/` 配置、读文件）。
    pub work_dir: Option<std::path::PathBuf>,
}

impl AgentContext {
    /// 构造带 work_dir 的 context。
    pub fn with_work_dir(path: impl Into<std::path::PathBuf>) -> Self {
        AgentContext {
            work_dir: Some(path.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SenderId newtype 应能正确转换字符串，且 as_str 取内部引用。
    #[test]
    fn test_sender_id_newtype() {
        let s = SenderId::new("ou_abc");
        assert_eq!(s.as_str(), "ou_abc");
        assert_eq!(s.0, "ou_abc");
    }

    /// SessionKey::derive 在不同 (platform, sender) 下应产出可解析的 key。
    #[test]
    fn test_session_key_derive_group() {
        let key = SessionKey::derive(PlatformKind::Feishu, "oc_chat", None);
        assert_eq!(key.as_str(), "feishu:oc_chat");
        let parsed = key.parse().unwrap();
        assert_eq!(parsed.0, PlatformKind::Feishu);
        assert_eq!(parsed.1, "oc_chat");
        assert_eq!(parsed.2, None);
    }

    /// DM 模式：sender 存在时三段式 key。
    #[test]
    fn test_session_key_derive_dm() {
        let sender = SenderId::new("ou_123");
        let key = SessionKey::derive(PlatformKind::Feishu, "oc_chat", Some(&sender));
        assert_eq!(key.as_str(), "feishu:oc_chat:ou_123");
        let parsed = key.parse().unwrap();
        assert_eq!(parsed.2, Some("ou_123"));
    }

    /// 非法 platform 字符串 parse 返回 None（不 panic）。
    #[test]
    fn test_session_key_parse_invalid() {
        let key = SessionKey("slack:oc_chat".into());
        assert!(key.parse().is_none());
        // 段数过多也解析失败。
        let key = SessionKey("feishu:a:b:c".into());
        assert!(key.parse().is_none());
    }

    /// ReplyTarget::platform 应能正确返回所属平台。
    #[test]
    fn test_reply_target_platform() {
        let rt = ReplyTarget::feishu("oc_x", None, FeishuChatType::P2p);
        assert_eq!(rt.platform(), PlatformKind::Feishu);
    }

    /// IncomingMessage 必能 serde 序列化（保证 channel ↔ dispatcher 协议稳定）。
    /// 必须把 M1.5 followup 新增的 3 个字段（is_mention / sender_kind /
    /// is_from_self）一起验证，确保 channel ↔ dispatcher JSON 协议对得上。
    #[test]
    fn test_incoming_message_serialize_roundtrip() {
        let msg = IncomingMessage {
            platform: PlatformKind::Feishu,
            session_key: SessionKey::derive(PlatformKind::Feishu, "oc", None),
            sender: SenderId::new("ou_a"),
            content: IncomingContent::Text("hi".into()),
            reply_target: ReplyTarget::feishu("oc", None, FeishuChatType::P2p),
            timestamp_ms: 1234567890,
            raw_message_id: "om_xxx".into(),
            is_mention: true,
            sender_kind: SenderKind::User,
            is_from_self: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: IncomingMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    /// 新增字段必须能 serde 反序列化缺失场景（兼容老 JSON），
    /// 防止 dispatcher 重启后吃老消息炸掉。
    /// `#[serde(default)]` 在每个新字段上保证这一点。
    #[test]
    fn test_incoming_message_backward_compat_missing_new_fields() {
        // 模拟旧版 JSON（没有新字段）。
        let old_json = r#"{
            "platform": "Feishu",
            "session_key": "feishu:oc",
            "sender": "ou_a",
            "content": {"Text": "hi"},
            "reply_target": {"Feishu": {"chat_id": "oc", "chat_type": "P2p"}},
            "timestamp_ms": 1234567890,
            "raw_message_id": "om_xxx"
        }"#;
        let msg: IncomingMessage = serde_json::from_str(old_json).unwrap();
        // 缺省值应是安全的「中性」值。
        assert!(!msg.is_mention, "缺省 is_mention 应为 false");
        assert_eq!(msg.sender_kind, SenderKind::User);
        assert!(!msg.is_from_self);
    }

    /// SenderKind 默认值必须是 User（保守默认：用户）。
    #[test]
    fn test_sender_kind_default_is_user() {
        assert_eq!(SenderKind::default(), SenderKind::User);
    }

    /// OutgoingContent::Card 必须能容纳任意 JSON（飞书卡片 schema 是动态的）。
    #[test]
    fn test_outgoing_card_with_dynamic_json() {
        let card = serde_json::json!({
            "header": {"title": {"tag": "plain_text", "content": "Hello"}},
            "elements": []
        });
        let content = OutgoingContent::Card(card.clone());
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("header"));
        let back: OutgoingContent = serde_json::from_str(&json).unwrap();
        if let OutgoingContent::Card(v) = back {
            assert_eq!(v, card);
        } else {
            panic!("expected Card variant");
        }
    }

    /// Attachment 的 Optional 字段 skip_serializing_if 必须生效。
    #[test]
    fn test_attachment_skip_none_fields() {
        let att = Attachment::new("https://example.com/x.png");
        let json = serde_json::to_string(&att).unwrap();
        // 仅 url 字段被序列化，mime_type/filename/size_bytes 因 None 被跳过。
        assert_eq!(json, r#"{"url":"https://example.com/x.png"}"#);
    }

    /// AgentContext::with_work_dir 构造路径后字段正确。
    #[test]
    fn test_agent_context_work_dir() {
        let ctx = AgentContext::with_work_dir("/tmp/proj");
        assert_eq!(ctx.work_dir.as_deref().unwrap().to_str(), Some("/tmp/proj"));
    }
}
