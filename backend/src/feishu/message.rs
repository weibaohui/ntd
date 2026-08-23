/// A decoded Feishu message received from the WebSocket event stream.
#[derive(Debug, Clone)]
pub struct ChannelMessage {
    pub id: String,
    pub sender: String,
    pub sender_type: Option<String>,
    pub content: String,
    pub channel: String,
    pub timestamp: u64,
    pub chat_type: Option<String>,
    pub mentioned_open_ids: Vec<String>,
    /// NTD-019：卡片回调的原始会话类型（p2p/group）。
    /// 卡片回调的 chat_type 被路由占用（写死 "card_callback" 供监听器分发），
    /// 真实会话类型经本字段透传；普通消息恒为 None（chat_type 字段已足够）。
    /// 字段在 act_new 中被读取（scope 三级推断的第一级），无需 dead_code 豁免——
    /// 若此处告警说明读取链路被删，应恢复链路而非加 allow（禁止清单 #13）。
    pub origin_chat_type: Option<String>,
}
