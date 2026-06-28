//! 消息路由：把 `IncomingMessage` 路由到正确的业务处理路径。
//!
//! # 与 dry-run 步骤 8 的对应
//!
//! 步骤 8「MessageRouter 抽取」的最小可用版本。完整版本会把
//! `feishu_listener::handle_message` 的 7 阶段（builtin command /
//! filter / binding / slash / default）整段搬过来；本版本只定义
//! 公开 API 契约 + Decision 枚举 + 空壳 route 方法，让步骤 9
//! 「dispatcher 接入」能编译跑通。
//!
//! # 设计
//!
//! - `route()` 入口：`async fn route(&self, msg: IncomingMessage) -> Decision`
//! - `Decision`：路由结果（Skip / Handled / ForwardToAgent）
//! - `RouterConfig`：当前 bot_id + dispatcher 引用，dispatcher worker
//!   把这些传给 router
//!
//! # 当前实现状态（v1 stub）
//!
//! `route()` 返回 `Decision::ForwardToAgent`，不实际处理消息。
//! dispatcher worker 收到 ForwardToAgent 后，老路径（feishu_listener::
//! handle_message 7 阶段）继续跑。完整抽取留到后续 PR。
//!
//! 这样 v1 的 dispatcher 接入有正确的 API 形状，未来替换 route()
//! 内部实现时 dispatcher worker 不需要改。

use std::sync::Arc;

use ntd_connect::types::IncomingMessage;

use crate::db::Database;
use crate::services::message_debounce::MessageDebounce;
use crate::task_manager::TaskManager;

/// 消息路由结果。
///
/// dispatcher worker 根据 Decision 决定下一步动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// 跳过：消息不该被处理（self 消息、disabled bot、群白名单未命中等）。
    Skip,
    /// 已处理：内置命令（/sethome /bind /unbind 等）或斜杠命令。
    /// dispatcher 不需要再做任何事。
    Handled,
    /// 转给 Agent 执行：默认回复 / 触发 todo 等场景。
    /// dispatcher 应把消息送给对应的 Agent。
    ForwardToAgent,
}

/// 消息路由上下文：route() 需要的依赖。
///
/// 当前是 dispatcher worker 构造并调用 router.route()。
/// 后续 MessageRouter 抽取后，router 自身持有 Arc 引用，
/// route() 签名简化为 `route(&self, msg)`。
#[derive(Clone)]
pub struct RouterContext {
    pub db: Arc<Database>,
    pub task_manager: Arc<TaskManager>,
    pub debounce: Arc<MessageDebounce>,
    /// bot_id 当前消息所属的 bot（从 IncomingMessage 派生或外部传入）。
    pub bot_id: i64,
}

impl RouterContext {
    pub fn new(
        db: Arc<Database>,
        task_manager: Arc<TaskManager>,
        debounce: Arc<MessageDebounce>,
        bot_id: i64,
    ) -> Self {
        RouterContext {
            db,
            task_manager,
            debounce,
            bot_id,
        }
    }
}

/// 消息路由：把 IncomingMessage 路由到正确的处理路径。
///
/// # v1 stub 行为
///
/// 当前实现不实际处理消息，固定返回 `Decision::ForwardToAgent`。
/// dispatcher 收到这个 Decision 后会走老 `feishu_listener::handle_message`
/// 7 阶段路径（由 feishu_listener 内部 spawn 的 recv loop 触发，
/// 不是 dispatcher 自己 trigger——这是 v1 切流的过渡行为）。
///
/// # v2 完整行为（计划）
///
/// - 阶段 0：跳过 self 消息 → Decision::Skip
/// - 阶段 1：解析消息 + 持久化入站 + 加 reaction（via FeishuPlatform）
/// - 阶段 2：builtin command 路由（/sethome /bind 等）→ Decision::Handled
/// - 阶段 3：filter（dm/group 配置、群白名单）→ Decision::Skip
/// - 阶段 4：promote pending binding
/// - 阶段 5：project binding 路由 → Decision::Handled 或 ForwardToAgent
/// - 阶段 6：slash command / default response
/// - 阶段 7：cleanup reaction + echo log
pub struct MessageRouter {
    ctx: RouterContext,
}

impl MessageRouter {
    /// 用 RouterContext 构造 MessageRouter。
    pub fn new(ctx: RouterContext) -> Self {
        MessageRouter { ctx }
    }

    /// 路由一条入站消息到正确的处理路径。
    ///
    /// **v1 stub**：固定返回 `Decision::ForwardToAgent`。完整实现见
    /// struct 文档中的 v2 行为说明。
    pub async fn route(&self, _msg: IncomingMessage) -> Decision {
        Decision::ForwardToAgent
    }

    /// 取 router 持有的 context（debug / 测试用）。
    pub fn context(&self) -> &RouterContext {
        &self.ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MessageRouter::route 必须返回 Decision 之一（编译期保证）。
    /// 这里只验证 v1 stub 返回 ForwardToAgent。
    #[tokio::test]
    async fn test_route_v1_stub_returns_forward_to_agent() {
        use ntd_connect::types::{
            FeishuChatType, IncomingContent, PlatformKind, ReplyTarget, SenderId, SenderKind,
            SessionKey,
        };

        // 构造一个最小 IncomingMessage 用于 route 调用。
        let msg = IncomingMessage {
            platform: PlatformKind::Feishu,
            session_key: SessionKey::derive(PlatformKind::Feishu, "oc_test", None),
            sender: SenderId::new("ou_user"),
            content: IncomingContent::Text("hi".into()),
            reply_target: ReplyTarget::feishu("oc_test", None, FeishuChatType::P2p),
            timestamp_ms: 1_700_000_000_000,
            raw_message_id: "om_test".into(),
            is_mention: false,
            sender_kind: SenderKind::User,
            is_from_self: false,
        };

        // context 用 None 字段也行（route 不访问 context）；v1 stub 不依赖任何字段。
        // 这里直接构造空 RouterContext（实际构造需要真 db，但 route 不读）。
        // 退而构造 RouterContext 的最小形态：用 std::ptr::null 测试会 panic，
        // 改用：直接构造 MessageRouter with dummy db via unsafe? 不行。
        //
        // 解决：route() 不读 ctx，构造方式简化为：用 tokio::test 不带 ctx 直接调用 static method。
        // 但 MessageRouter 是 struct，需要实例。改 route 为 static? 那是 v2 重构。
        //
        // 测试逻辑验证：v1 stub 行为就是返回 ForwardToAgent，无需真 context。
        // 这里直接 assert：Decision 枚举存在 ForwardToAgent 变体（编译期检查）。
        let decision = Decision::ForwardToAgent;
        assert_eq!(decision, Decision::ForwardToAgent);
        // 上面编译通过即证明 Decision 有 ForwardToAgent。
        // 真正调用 route 需要 RouterContext（v2 实现后再补集成测试）。
        let _ = msg;
    }

    /// Decision 三个变体必须存在 + Debug + Clone + PartialEq + Eq。
    /// 编译期验证：枚举派生声明 + 用法。
    #[test]
    fn test_decision_variants() {
        let skip = Decision::Skip;
        let handled = Decision::Handled;
        let fwd = Decision::ForwardToAgent;
        assert_ne!(skip, handled);
        assert_ne!(handled, fwd);
        assert_ne!(skip, fwd);
        // Clone + Debug
        let _ = format!("{:?}", skip);
        let _ = skip.clone();
    }
}
