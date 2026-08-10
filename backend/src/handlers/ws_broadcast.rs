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

use crate::executor_service::ExecEvent;

/// WS 广播信封：事件 JSON 文本 + workspace 过滤键。
///
/// `json` 用 `Arc<str>` 而非 String：广播语义下同一事件要发给 N 个客户端，
/// Arc 共享让 clone 只是引用计数自增，序列化产物全局唯一一份。
#[derive(Debug, Clone)]
pub struct WsEnvelope {
    /// 事件归属的 workspace；None = 全局事件（Sync/ReviewStatusChanged 等），全量转发
    pub workspace_id: Option<i64>,
    /// 预序列化的事件 JSON（与 ExecEvent 的 serde tag 格式一致）
    pub json: Arc<str>,
}

/// 连接声明与事件归属的匹配判定（events_handler 主循环的唯一过滤规则）。
///
/// 三态真值表（经用户确认的决策 1a/3a）：
/// - 连接未声明 workspace（None）→ 全推（兼容旧客户端/第三方接入）；
/// - 连接声明 Some(ws)，事件为全局（None）→ 推（全局事件不含 workspace 敏感信息）；
/// - 连接声明 Some(ws)，事件归属 Some(ev_ws)→ 仅相等才推（跨 workspace 隔离）。
pub fn envelope_matches(conn_workspace: Option<i64>, envelope_workspace: Option<i64>) -> bool {
    match (conn_workspace, envelope_workspace) {
        (None, _) => true,
        (Some(_), None) => true,
        (Some(conn), Some(ev)) => conn == ev,
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
                    // workspace_id 提取先于序列化：过滤键与载荷分离，
                    // 即使序列化失败也能决定归属（防御性取值顺序）
                    let workspace_id = event.workspace_id();
                    // 序列化失败理论不可达（serde_json 对纯数据枚举不失败）；
                    // 失败时跳过本条而非 panic——单事件丢失由前端 Sync 兜底，
                    // forwarder 进程级崩溃会让全量 WS 推送停摆，绝不允许
                    let Ok(json) = serde_json::to_string(&event) else {
                        tracing::error!("[ws-forwarder] 事件序列化失败，跳过本条: {event:?}");
                        continue;
                    };
                    let envelope = WsEnvelope {
                        workspace_id,
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

    /// envelope_matches 三态真值表全枚举（决策 1a/3a 的行为契约）
    #[test]
    fn test_envelope_matches_truth_table() {
        // 无参数连接：全推（兼容）
        assert!(envelope_matches(None, None));
        assert!(envelope_matches(None, Some(1)));
        // 有参数连接 + 全局事件：推
        assert!(envelope_matches(Some(1), None));
        // 有参数连接 + 归属事件：仅相等推
        assert!(envelope_matches(Some(1), Some(1)));
        assert!(!envelope_matches(Some(1), Some(2)));
    }

    /// ExecEvent::workspace_id 全 13 变体提取矩阵（漏变体编译期即报 non-exhaustive，
    /// 本测试锁运行时取值语义：Option 透传 / i64 包 Some / 全局组 None）
    #[test]
    fn test_workspace_id_all_variants() {
        let entry = ParsedLogEntry::new("info", "x");
        let cases: Vec<(ExecEvent, Option<i64>)> = vec![
            // Option<i64> 组：透传（Some 与 None 各取代表）
            (ExecEvent::Started { task_id: "t".into(), todo_id: 1, todo_title: "t".into(), executor: "e".into(), workspace_id: Some(7) }, Some(7)),
            (ExecEvent::Output { task_id: "t".into(), entry: entry.clone(), workspace_id: None }, None),
            (ExecEvent::Finished { task_id: "t".into(), todo_id: 1, todo_title: "t".into(), executor: "e".into(), success: true, result: None, feishu_bot_id: None, feishu_receive_id: None, feishu_receive_id_type: None, workspace_id: Some(3), duration_secs: 0, total_tokens: 0, trigger_type: None }, Some(3)),
            (ExecEvent::TodoProgress { task_id: "t".into(), progress: vec![], workspace_id: Some(2) }, Some(2)),
            (ExecEvent::ExecutionStats { task_id: "t".into(), stats: crate::models::ExecutionStats { tool_calls: 0, conversation_turns: 0, thinking_count: 0 }, workspace_id: Some(5) }, Some(5)),
            (ExecEvent::LoopFinished { loop_execution_id: 1, loop_id: 1, loop_title: "l".into(), status: "success".into(), total_steps: 1, completed_steps: 1, failed_steps: 0, duration_secs: 0, total_tokens: 0, workspace_id: Some(9) }, Some(9)),
            // i64 必填组：包 Some
            (ExecEvent::BlackboardDebounceStatus { workspace_id: 4, pending_count: 0, threshold: 1, debounce_secs: 1, remaining_secs: -1, refreshing: false }, Some(4)),
            (ExecEvent::WikiChatStarted { task_id: "t".into(), workspace_id: 6, executor: "e".into(), message: "m".into() }, Some(6)),
            (ExecEvent::WikiChatOutput { task_id: "t".into(), workspace_id: 8, entry }, Some(8)),
            (ExecEvent::WikiChatFinished { task_id: "t".into(), workspace_id: 10, success: true, result: None, duration_secs: 0 }, Some(10)),
            // 全局/飞书专用组：None
            (ExecEvent::Sync { tasks: vec![] }, None),
            (ExecEvent::ReviewStatusChanged { record_id: 1, todo_id: 1, review_status: "approved".into() }, None),
            (ExecEvent::DirectCardMessage { bot_id: 1, receive_id: "r".into(), receive_id_type: "open_id".into(), content: "c".into() }, None),
            (ExecEvent::DirectStreamMessage { bot_id: 1, receive_id: "r".into(), receive_id_type: "open_id".into(), entry: ParsedLogEntry::new("info", "x") }, None),
        ];
        assert_eq!(cases.len(), 14, "变体覆盖数（含 Output 的 None 代表）");
        for (event, expected) in cases {
            assert_eq!(event.workspace_id(), expected, "{event:?}");
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

    /// forwarder 端到端：飞书专用被拦截、正常事件携带正确 workspace_id 与 JSON 文本
    #[tokio::test]
    async fn test_spawn_ws_forwarder_filters_and_preserializes() {
        let (tx, _) = broadcast::channel(8);
        let (ws_tx, mut ws_rx) = broadcast::channel(8);
        spawn_ws_forwarder(&tx, ws_tx);

        // 飞书专用事件：不应出现在 WS channel
        tx.send(ExecEvent::DirectCardMessage { bot_id: 1, receive_id: "r".into(), receive_id_type: "open_id".into(), content: "c".into() }).unwrap();
        // 正常事件：应转发且 workspace_id/JSON 正确
        tx.send(ExecEvent::Started { task_id: "t".into(), todo_id: 1, todo_title: "T".into(), executor: "e".into(), workspace_id: Some(42) }).unwrap();

        let env = tokio::time::timeout(std::time::Duration::from_secs(2), ws_rx.recv())
            .await
            .expect("应收到正常事件信封")
            .unwrap();
        assert_eq!(env.workspace_id, Some(42));
        // 预序列化文本即前端收到的线上格式（serde tag=type）
        assert!(env.json.contains("\"type\":\"Started\""), "json: {}", env.json);
        // DirectCardMessage 已被拦截：channel 中无更多消息
        assert!(ws_rx.try_recv().is_err(), "飞书专用事件不应进入 WS channel");
    }
}
