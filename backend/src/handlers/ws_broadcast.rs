//! 094：WS 广播通路——预序列化信封 + forwarder 任务。
//!
//! 背景：原 `events_handler` 主循环让每个 WS 客户端各自 `serde_json::to_string(&event)`，
//! N 个标签页 = N 次序列化同一事件；且不区分 workspace，全量事件推给全量客户端。
//!
//! 本模块把「序列化 + workspace 归属」收敛到单一 forwarder 任务：
//! 订阅现有 ExecEvent channel → 过滤飞书专用事件 → 序列化一次 →
//! 发 `WsEnvelope`（json 为 `Arc<str>`，全客户端共享零拷贝）。
//! 原 ExecEvent channel 的 12 个发送点与飞书推送订阅侧因此零改动。

use std::sync::Arc;
use tokio::sync::broadcast;

use crate::executor_service::{EventScope, ExecEvent};

/// WS 广播信封：事件 JSON 文本 + 作用域过滤键。
///
/// `json` 用 `Arc<str>` 而非 String：广播语义下同一事件要发给 N 个客户端，
/// Arc 共享让 clone 只是引用计数自增，序列化产物全局唯一一份。
#[derive(Debug, Clone)]
pub struct WsEnvelope {
    /// 事件作用域（Global/Workspace/Unscoped 三态语义见 EventScope doc）
    pub scope: EventScope,
    /// 预序列化的事件 JSON（与 ExecEvent 的 serde tag 格式一致）
    pub json: Arc<str>,
}

/// 连接声明与事件作用域的匹配判定（events_handler 主循环的唯一过滤规则）。
///
/// 行为契约（CodeRabbit #1011 评审修订后）：
/// - 连接未声明 workspace（None）→ 全推（兼容旧客户端/第三方接入，含未归属事件）；
/// - 连接声明 Some(ws)，事件 Global → 推（无 workspace 敏感信息）；
/// - 连接声明 Some(ws)，事件 Workspace(ev) → 仅相等才推（跨 workspace 隔离）；
/// - 连接声明 Some(ws)，事件 Unscoped（未归属：None/哨兵 0）→ 不推——
///   与 todo 列表的 workspace 过滤语义对齐（未归属在任何 workspace 视图均不显示），
///   同时避免未归属 loop/任务的标题日志流进所有面板。
pub fn envelope_matches(conn_workspace: Option<i64>, scope: EventScope) -> bool {
    match conn_workspace {
        // 无参连接全推：旧前端/第三方客户端的兼容口，本服务单用户本地工具定位，
        // 不构成新暴露面（决策 1a 时用户已确认）
        None => true,
        Some(conn) => match scope {
            // 全局事件无 workspace 敏感信息（Sync 是连接级握手产物不进 channel，
            // 实际走到这的只有 ReviewStatusChanged 类），带参连接照常收
            EventScope::Global => true,
            // 归属事件严格相等才推——跨 workspace 隔离的核心判定
            EventScope::Workspace(ev) => conn == ev,
            // 未归属事件（None/哨兵 0）不推带参连接：与 todo 列表过滤语义对齐，
            // 未归属任务在任何 workspace 视图本就不显示（CodeRabbit 安全评审项）
            EventScope::Unscoped => false,
        },
    }
}

/// 启动 WS 转发任务：ExecEvent channel → WS channel 的单向桥。
///
/// 在 `build_app_state` 中随 AppState 初始化启动；进程级单例（多实例会重复推送）。
/// Lagged 策略与 feishu_push 对齐：warn 并继续（WS 客户端对事件丢失的容忍度
/// 高于飞书推送——前端有 Sync 握手兜底重建状态，且 Lagged 本身意味着客户端
/// 已经跟不上，跳积压比追积压更符合实时性诉求）。
pub fn spawn_ws_forwarder(
    // 引用取向：函数体只做 subscribe()（ Sender 的 &self 方法），
    // 调用方保留 owned 句柄写 AppState，无消费即不取所有权
    tx: &broadcast::Sender<ExecEvent>,
    ws_tx: broadcast::Sender<WsEnvelope>,
) {
    // subscribe 必须在 spawn 之前：保证转发从启动点之后的事件开始，
    // 不受 channel 里积压历史事件影响（ring buffer 语义，订阅即定位到当前 head）
    let mut rx = tx.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // 飞书专用定向事件不进 WS channel：前端无消费分支，纯带宽浪费
                    if event.is_feishu_direct() {
                        continue;
                    }
                    // 作用域提取先于序列化：过滤键与载荷分离，
                    // 即使序列化失败也能决定归属（防御性取值顺序）
                    let scope = event.event_scope();
                    // 序列化失败理论不可达（serde_json 对纯数据枚举不失败）；
                    // 失败时跳过本条而非 panic——单事件丢失由前端 Sync 兜底，
                    // forwarder 进程级崩溃会让全量 WS 推送停摆，绝不允许
                    let Ok(json) = serde_json::to_string(&event) else {
                        tracing::error!("[ws-forwarder] 事件序列化失败，跳过本条: {event:?}");
                        continue;
                    };
                    let envelope = WsEnvelope {
                        scope,
                        json: Arc::from(json.as_str()),
                    };
                    // send 失败 = 当前零订阅者（无 WS 客户端在线），正常场景，吞掉即可
                    let _ = ws_tx.send(envelope);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    // 见函数 doc：warn 并继续，不重订阅（与 feishu_push 对齐）
                    tracing::warn!("[ws-forwarder] lagged, skipped {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    // 上游 channel 关闭 = 进程关停中，退出任务
                    tracing::info!("[ws-forwarder] ExecEvent channel closed, forwarder exiting");
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use crate::models::ParsedLogEntry;

    /// envelope_matches 行为契约全枚举（CodeRabbit #1011 修订版）：
    /// 无参全推 / Global 全推 / Workspace 相等才推 / Unscoped 不推带参连接
    #[test]
    fn test_envelope_matches_scope_matrix() {
        // 无参数连接：全推（兼容口，含未归属事件）
        assert!(envelope_matches(None, EventScope::Global));
        assert!(envelope_matches(None, EventScope::Workspace(1)));
        assert!(envelope_matches(None, EventScope::Unscoped));
        // 有参数连接 + 全局事件：推
        assert!(envelope_matches(Some(1), EventScope::Global));
        // 有参数连接 + 归属事件：仅相等推
        assert!(envelope_matches(Some(1), EventScope::Workspace(1)));
        assert!(!envelope_matches(Some(1), EventScope::Workspace(2)));
        // 有参数连接 + 未归属事件：不推（CodeRabbit 安全评审项——
        // 未归属 loop/任务的标题日志不应流进任何带参面板）
        assert!(!envelope_matches(Some(1), EventScope::Unscoped));
    }

    /// event_scope 全 14 变体提取矩阵（漏变体编译期即报 non-exhaustive）：
    /// 锁三态归类语义——Some(N>0)→Workspace、None/Some(0)→Unscoped、全局组→Global
    #[test]
    fn test_event_scope_all_variants() {
        let entry = ParsedLogEntry::new("info", "x");
        let cases: Vec<(ExecEvent, EventScope)> = vec![
            // 生命周期组（Option<i64>）：N>0 归属
            (ExecEvent::Started { task_id: "t".into(), todo_id: 1, todo_title: "t".into(), executor: "e".into(), workspace_id: Some(7) }, EventScope::Workspace(7)),
            // None → Unscoped（未归属 loop 路径）
            (ExecEvent::Output { task_id: "t".into(), entry: entry.clone(), workspace_id: None }, EventScope::Unscoped),
            // Some(0) 哨兵 → Unscoped（DB 默认值语义：未分配工作空间）
            (ExecEvent::Finished { task_id: "t".into(), todo_id: 1, todo_title: "t".into(), executor: "e".into(), success: true, result: None, feishu_bot_id: None, feishu_receive_id: None, feishu_receive_id_type: None, workspace_id: Some(0), duration_secs: 0, total_tokens: 0, trigger_type: None }, EventScope::Unscoped),
            (ExecEvent::TodoProgress { task_id: "t".into(), progress: vec![], workspace_id: Some(2) }, EventScope::Workspace(2)),
            (ExecEvent::ExecutionStats { task_id: "t".into(), stats: crate::models::ExecutionStats { tool_calls: 0, conversation_turns: 0, thinking_count: 0 }, workspace_id: Some(5) }, EventScope::Workspace(5)),
            (ExecEvent::LoopFinished { loop_execution_id: 1, loop_id: 1, loop_title: "l".into(), status: "success".into(), total_steps: 1, completed_steps: 1, failed_steps: 0, duration_secs: 0, total_tokens: 0, workspace_id: Some(9) }, EventScope::Workspace(9)),
            // 黑板/WikiChat 组（i64 必填）：0 同按未归属哨兵
            (ExecEvent::BlackboardDebounceStatus { workspace_id: 4, pending_count: 0, threshold: 1, debounce_secs: 1, remaining_secs: -1, refreshing: false }, EventScope::Workspace(4)),
            (ExecEvent::WikiChatStarted { task_id: "t".into(), workspace_id: 0, executor: "e".into(), message: "m".into() }, EventScope::Unscoped),
            (ExecEvent::WikiChatOutput { task_id: "t".into(), workspace_id: 8, entry }, EventScope::Workspace(8)),
            (ExecEvent::WikiChatFinished { task_id: "t".into(), workspace_id: 10, success: true, result: None, duration_secs: 0 }, EventScope::Workspace(10)),
            // 全局组
            (ExecEvent::Sync { tasks: vec![] }, EventScope::Global),
            (ExecEvent::ReviewStatusChanged { record_id: 1, todo_id: 1, review_status: "approved".into() }, EventScope::Global),
            // 飞书专用组：防御兜底 Global（实际在 forwarder 已被拦截）
            (ExecEvent::DirectCardMessage { bot_id: 1, receive_id: "r".into(), receive_id_type: "open_id".into(), content: "c".into() }, EventScope::Global),
            (ExecEvent::DirectStreamMessage { bot_id: 1, receive_id: "r".into(), receive_id_type: "open_id".into(), entry: ParsedLogEntry::new("info", "x") }, EventScope::Global),
        ];
        assert_eq!(cases.len(), 14, "全 14 变体覆盖");
        for (event, expected) in cases {
            assert_eq!(event.event_scope(), expected, "{event:?}");
        }
    }

    /// is_feishu_direct：仅 DirectCard/DirectStream 两变体为 true
    #[test]
    fn test_is_feishu_direct_only_direct_variants() {
        assert!(ExecEvent::DirectCardMessage { bot_id: 1, receive_id: "r".into(), receive_id_type: "open_id".into(), content: "c".into() }.is_feishu_direct());
        assert!(ExecEvent::DirectStreamMessage { bot_id: 1, receive_id: "r".into(), receive_id_type: "open_id".into(), entry: ParsedLogEntry::new("info", "x") }.is_feishu_direct());
        assert!(!ExecEvent::Sync { tasks: vec![] }.is_feishu_direct());
        assert!(!ExecEvent::Started { task_id: "t".into(), todo_id: 1, todo_title: "t".into(), executor: "e".into(), workspace_id: None }.is_feishu_direct());
    }

    /// forwarder 端到端：飞书专用被拦截、正常事件携带正确作用域与 JSON 文本
    #[tokio::test]
    async fn test_spawn_ws_forwarder_filters_and_preserializes() {
        let (tx, _) = broadcast::channel(8);
        let (ws_tx, mut ws_rx) = broadcast::channel(8);
        spawn_ws_forwarder(&tx, ws_tx);

        // 飞书专用事件：不应出现在 WS channel
        tx.send(ExecEvent::DirectCardMessage { bot_id: 1, receive_id: "r".into(), receive_id_type: "open_id".into(), content: "c".into() }).unwrap();
        // 正常事件：应转发且 scope/JSON 正确
        tx.send(ExecEvent::Started { task_id: "t".into(), todo_id: 1, todo_title: "T".into(), executor: "e".into(), workspace_id: Some(42) }).unwrap();

        let env = tokio::time::timeout(std::time::Duration::from_secs(2), ws_rx.recv())
            .await
            .expect("应收到正常事件信封")
            .unwrap();
        assert_eq!(env.scope, EventScope::Workspace(42));
        // 预序列化文本即前端收到的线上格式（serde tag=type）
        assert!(env.json.contains("\"type\":\"Started\""), "json: {}", env.json);
        // DirectCardMessage 已被拦截：channel 中无更多消息
        assert!(ws_rx.try_recv().is_err(), "飞书专用事件不应进入 WS channel");
    }
}
