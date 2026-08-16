//! 执行事件模块：定义执行状态变化的事件类型。
//!
//! `ExecEvent` 是核心事件枚举，用于在执行器、handler、前端之间传递实时状态。
//! 所有事件通过 broadcast 通道分发，支持多个订阅者（WebSocket、飞书推送等）。

use serde::Serialize;

use crate::models::{ParsedLogEntry, ExecutionStats, TodoItem};
use crate::task_manager::TaskInfo;

/// 执行事件枚举，涵盖所有可能的执行状态变化。
///
/// 使用 `#[serde(tag = "type")]` 实现标签联合，前端可按 type 字段区分事件。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ExecEvent {
    /// 执行开始事件：任务启动时发送
    Started {
        task_id: String,
        todo_id: i64,
        todo_title: String,
        executor: String,
        /// 执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
        workspace_id: Option<i64>,
    },
    /// 执行输出事件：实时推送 stdout/stderr 内容
    Output {
        task_id: String,
        entry: ParsedLogEntry,
        /// 执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
        workspace_id: Option<i64>,
    },
    /// 执行完成事件：任务结束时发送
    Finished {
        task_id: String,
        todo_id: i64,
        todo_title: String,
        executor: String,
        success: bool,
        result: Option<String>,
        /// Feishu bot_id to use for sending result directly to binding chat
        feishu_bot_id: Option<i64>,
        /// Feishu receive_id (user open_id for p2p, chat_id for group)
        feishu_receive_id: Option<String>,
        /// Feishu receive_id_type: "open_id" for p2p, "chat_id" for group
        feishu_receive_id_type: Option<String>,
        /// 执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
        workspace_id: Option<i64>,
        /// 执行时长（秒），用于推送统计摘要
        duration_secs: i64,
        /// 累计 Token 消耗（input + output），用于推送统计摘要
        total_tokens: i64,
        /// 本次执行的触发类型（"manual" / "butler_chat" / "auto_review" / "blackboard" 等），
        /// 用于黑板更新等场景在 Finished 钩子中识别"自身"以避免递归触发。
        /// 旧代码路径未传时为 None。
        trigger_type: Option<String>,
    },
    /// 同步事件：连接时发送当前实际运行的任务列表
    /// 前端收到此事件后应清空 runningTasks 并用此列表初始化
    Sync {
        tasks: Vec<TaskInfo>,
    },
    /// Todo 进度事件：推送子任务拆解列表
    TodoProgress {
        task_id: String,
        progress: Vec<TodoItem>,
        /// 执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
        workspace_id: Option<i64>,
    },
    /// 执行统计事件：推送 Token 消耗、耗时等统计数据
    ExecutionStats {
        task_id: String,
        stats: ExecutionStats,
        /// 执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
        workspace_id: Option<i64>,
    },
    /// 评审状态变更事件：自动评审完成后发送
    ReviewStatusChanged {
        record_id: i64,
        todo_id: i64,
        review_status: String,
    },
    /// 私聊直达卡片消息：消息经 executor 处理后直接把结果发回飞书，不存储执行记录。
    /// 用于聊天直连（dm_chat 单聊直聊 / butler_chat 群聊管家）场景（开始/结束/错误等关键节点）。
    DirectCardMessage {
        /// Feishu bot_id
        bot_id: i64,
        /// 接收者 ID（open_id 或 chat_id）
        receive_id: String,
        /// 接收者类型（open_id / chat_id）
        receive_id_type: String,
        /// 要发送的文本内容
        content: String,
    },
    /// 私聊直达流式消息：管家聊天场景下，执行过程中每条日志直接推送给触发用户。
    /// 与 DirectCardMessage 的区别：后者是开始/结束等关键节点的卡片消息，
    /// 前者是执行过程中流式输出的日志消息（push_level="all" 时发送）。
    DirectStreamMessage {
        /// Feishu bot_id
        bot_id: i64,
        /// 接收者 ID（open_id 或 chat_id）
        receive_id: String,
        /// 接收者类型（open_id / chat_id）
        receive_id_type: String,
        /// 解析后的日志条目（含 type / content / tool_name 等）
        entry: ParsedLogEntry,
    },
    /// Loop 执行完成事件：loop 执行完成后广播此事件，
    /// 用于 FeishuPushService 按 workspace 配置推送执行结果。
    LoopFinished {
        /// loop 执行记录 ID
        loop_execution_id: i64,
        /// loop ID
        loop_id: i64,
        /// loop 标题
        loop_title: String,
        /// 执行状态（终态枚举值，共 5 种）：
        /// - success：全部成功
        /// - partial：部分成功（有成功也有失败）
        /// - failed：全部失败
        /// - capped_step：因步数限制被截断终止
        /// - capped_token：因 Token 限制被截断终止
        status: String,
        /// 总步数
        total_steps: i32,
        /// 成功步数
        completed_steps: i32,
        /// 失败步数
        failed_steps: i32,
        /// 执行时长（秒）
        duration_secs: i64,
        /// 累计 Token 消耗（input + output）
        total_tokens: i64,
        /// 执行所在的工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
        workspace_id: Option<i64>,
    },
/// 黑板防抖状态事件：定期推送倒计时和 pending 数量，前端据此渲染双进度条。
/// 由 blackboard_flush_listener 转发 debouncer 的状态到 WebSocket。
BlackboardDebounceStatus {
    /// 工作空间 ID
    workspace_id: i64,
    /// 当前 pending 队列条数
    pending_count: u64,
    /// 触发阈值（条数）
    threshold: u64,
    /// 配置的防抖周期（秒）
    debounce_secs: u64,
    /// Timer 剩余秒数（-1 表示无 active timer，即等待中）
    remaining_secs: i64,
    /// 是否正在刷新（LLM 调用中）
    refreshing: bool,
},
/// Wiki 对话开始事件：用户发起对话、执行器启动时发送。
/// 用于前端对话面板初始化状态、显示"执行中"指示器。
WikiChatStarted {
    /// 对话任务 ID（形如 "wiki-chat-{uuid}"）
    task_id: String,
    /// 工作空间 ID
    workspace_id: i64,
    /// 使用的执行器名称
    executor: String,
    /// 用户输入的原始消息
    message: String,
},
/// Wiki 对话输出事件：执行器 stdout 每解析出一行日志就推送一次。
/// 前端收到后追加到对话面板的日志列表中，实现流式展示中间过程。
WikiChatOutput {
    /// 对话任务 ID
    task_id: String,
    /// 工作空间 ID
    workspace_id: i64,
    /// 解析后的日志条目（含 type / content / timestamp 等）
    entry: ParsedLogEntry,
},
/// Wiki 对话完成事件：执行器退出时发送，携带最终结果。
/// 前端收到后标记对话结束、显示最终结果高亮块。
WikiChatFinished {
    /// 对话任务 ID
    task_id: String,
    /// 工作空间 ID
    workspace_id: i64,
    /// 是否成功（退出码为 0）
    success: bool,
    /// 最终结果文本（从日志中提取的 result/text/assistant 类型内容）
    result: Option<String>,
    /// 执行时长（秒）
    duration_secs: i64,
},
}

impl ExecEvent {
    /// 094：事件的 workspace 作用域，供 WS 广播按连接声明过滤。
    ///
    /// 三态区分的动机（CodeRabbit #1011 评审）：`Option<i64>` 把「未归属」与「全局」
    /// 混在一个 None 里，且 DB 层 `workspace_id=0` 是「未分配工作空间」的哨兵值
    /// （loop_runner.rs 的既有约定：Some(0) 与 None 语义等价），必须显式区分：
    /// - `Workspace(N)`（N>0）：归属确定 workspace，仅推给声明该 workspace 的连接；
    /// - `Unscoped`：未归属任务/loop（None 或 Some(0)）——不推给任何带参连接，
    ///   与 todo 列表的 workspace 过滤语义对齐（未归属在任何 workspace 视图均不显示）；
    /// - `Global`：无 workspace 概念的全局事件（Sync/ReviewStatusChanged），全推。
    pub fn event_scope(&self) -> EventScope {
        match self {
            // 生命周期组：Option<i64> 字段——None 与 Some(0) 经 scope_from_optional 归一为
            // Unscoped（DB 层 0 是「未分配」哨兵，见 loop_runner.rs 的等价约定）
            ExecEvent::Started { workspace_id, .. }
            | ExecEvent::Output { workspace_id, .. }
            | ExecEvent::Finished { workspace_id, .. }
            | ExecEvent::TodoProgress { workspace_id, .. }
            | ExecEvent::ExecutionStats { workspace_id, .. }
            | ExecEvent::LoopFinished { workspace_id, .. } => scope_from_optional(*workspace_id),
            // 黑板/WikiChat 组：i64 必填字段——0 同样按未归属哨兵处理
            ExecEvent::BlackboardDebounceStatus { workspace_id, .. }
            | ExecEvent::WikiChatStarted { workspace_id, .. }
            | ExecEvent::WikiChatOutput { workspace_id, .. }
            | ExecEvent::WikiChatFinished { workspace_id, .. } => scope_from_optional(Some(*workspace_id)),
            // 全局组：无 workspace 概念
            ExecEvent::Sync { .. } | ExecEvent::ReviewStatusChanged { .. } => EventScope::Global,
            // 飞书专用组：在 forwarder 已被 is_feishu_direct 拦截，Global 仅为防御兜底
            ExecEvent::DirectCardMessage { .. } | ExecEvent::DirectStreamMessage { .. } => {
                EventScope::Global
            }
        }
    }

    /// 094：飞书专用定向事件判定。这类事件只服务 FeishuPushService 的 bot 定向发送，
    /// 前端 WS 客户端无对应消费分支——forwarder 据此拦截，不再推入 WS channel
    /// （消除带宽浪费，也让「WS 消息 = 前端可消费消息」的语义单一化）。
    pub fn is_feishu_direct(&self) -> bool {
        matches!(
            self,
            ExecEvent::DirectCardMessage { .. } | ExecEvent::DirectStreamMessage { .. }
        )
    }
}

/// 094：事件作用域三态（见 `ExecEvent::event_scope` doc）。
/// Copy/Eq 供匹配判定零成本使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventScope {
    /// 无 workspace 概念的全局事件：全量转发
    Global,
    /// 归属确定 workspace（N>0）：仅推给声明该 workspace 的连接
    Workspace(i64),
    /// 未归属（workspace_id 为 None 或哨兵 0）：不推给任何带参连接
    Unscoped,
}

/// Option<i64> workspace_id → EventScope 的归一化：
/// None 与 Some(0) 都是「未分配工作空间」（loop_runner.rs 既有约定），统一为 Unscoped；
/// 只有 Some(N>0) 才是真正的 workspace 归属。
/// pub(crate)：handlers 的 Sync 握手过滤（sync_task_visible）复用同一归一口径。
pub(crate) fn scope_from_optional(workspace_id: Option<i64>) -> EventScope {
    match workspace_id {
        // 仅 Some(N>0) 算真实归属——Some(0) 落到下一臂（哨兵语义见 fn doc）
        Some(id) if id > 0 => EventScope::Workspace(id),
        _ => EventScope::Unscoped,
    }
}
