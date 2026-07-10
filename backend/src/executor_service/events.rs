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
        /// 本次执行的触发类型（"manual" / "smart_create" / "auto_review" / "blackboard" 等），
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
    /// 执行器直接响应：消息经 executor 处理后直接把结果发回飞书，不存储执行记录。
    /// 用于工作空间默认响应配置中选择"执行器"类型的场景。
    ExecutorDirectResponse {
        /// Feishu bot_id
        bot_id: i64,
        /// 接收者 ID（open_id 或 chat_id）
        receive_id: String,
        /// 接收者类型（open_id / chat_id）
        receive_id_type: String,
        /// 要发送的文本内容
        content: String,
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
