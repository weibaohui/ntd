use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::adapters::parse_executor_type;
use crate::db::Database;
use crate::execution_events::EventPipeline;
use crate::executor_service::log_capture;
use crate::executor_service::{
    run_todo_execution, run_todo_execution_with_params, RunTodoExecutionRequest,
};
use crate::executor_service::ExecEvent;
use crate::models::ParsedLogEntry;
use crate::service_context::ServiceContext;
use crate::task_manager::TaskManager;

#[derive(Debug, Clone)]
pub struct PendingMessage {
    pub bot_id: i64,
    pub chat_id: String,
    pub chat_type: String,
    pub sender: String,
    pub content: String,
    pub todo_id: i64,
    pub todo_prompt: String,
    pub executor: Option<String>,
    pub trigger_type: String,
    pub params: Option<HashMap<String, String>>,
    pub message_id: Option<String>,
    /// For project-bound resume: the session_id to resume
    pub resume_session_id: Option<String>,
    /// For project-bound resume: the message content as resume_message
    pub resume_message: Option<String>,
    /// feishu_project_bindings.id — set when this message comes from a bound chat
    pub binding_id: Option<i64>,
    /// 工作空间 ID，用于 FeishuPushService 按 workspace 隔离推送目标
    pub workspace_id: Option<i64>,
    /// 群聊 @提及的显式请求：跳过 debounce 等待，收到立即执行。
    /// 非 @mention 的群聊消息仍走 debounce 合并窗口。
    pub immediate: bool,
}

struct DebounceEntry {
    messages: Vec<PendingMessage>,
    timer: JoinHandle<()>,
}

/// 私聊串行队列的维度 key：(bot_id, workspace_id, sender)
/// 同一个用户在同一个工作空间下对同一个 bot 发的消息串行执行
type P2pQueueKey = (i64, i64, String);

/// workspace_id 缺失时的哨兵值。
/// 真实 workspace 自增 id 从 1 开始，用 -1 表示「无 workspace」既不会与真实值冲突，
/// 也比 0 更明确地表达「未设置」语义——0 容易被当成默认值，掩盖配置缺失。
const NO_WORKSPACE: i64 = -1;

/// p2p 串行队列单维度上限：防止恶意刷屏或误操作导致内存无限增长。
/// 超出则丢弃新消息并提示用户，正在执行的任务不受影响。50 足够缓冲正常多轮对话，
/// 又能兜住异常突发；可按需调整。
const MAX_P2P_QUEUE_LEN: usize = 50;

/// p2p 运行标记的 Drop guard。
///
/// 无论 p2p 处理是成功、返回 Err 还是 panic，guard 被 drop 时都会移除对应的
/// running 标记，防止异常退出导致该 (bot_id, workspace_id, sender) 维度被
/// 永久拦截（后续消息永远入队、永不执行，形成死锁）。
///
/// 关键约束：必须在「首次执行之前」创建，才能覆盖首次执行本身。若在首次执行
/// 之后才创建，首次执行 panic 时 guard 尚不存在，running 标记永远不会被清除，
/// 该维度就此死锁——这正是原实现把 guard 放在 drain 循环前的 bug。
struct P2pRunningGuard {
    key: P2pQueueKey,
    running: Arc<DashMap<P2pQueueKey, ()>>,
}

impl Drop for P2pRunningGuard {
    fn drop(&mut self) {
        self.running.remove(&self.key);
    }
}

/// 单条消息执行前的解析结果。
///
/// 把 resume session 的 TOCTOU 检查、exec_message/params 构建聚合在一起，
/// 让主路径（debounce batch）和 p2p 队列 drain 路径共用同一份解析逻辑，
/// 避免两份拷贝分叉——原实现就因分叉导致 drain 路径静默吞错、行为不一致。
struct ResolvedExecution {
    /// 替换好 {{message}} 的最终 prompt（resume 时含系统提示，否则原样 todo_prompt）
    exec_message: String,
    /// 执行参数；resume 时为 None（走 run_todo_execution），否则 Some（走 with_params）
    params: Option<HashMap<String, String>>,
    /// resume 用的 session_id；TOCTOU 检查后可能被降级为 None
    resume_session_id: Option<String>,
    /// resume_message 透传
    resume_message: Option<String>,
    /// 是否为 resume 执行（resume_session_id.is_some()）
    is_resume: bool,
    /// 给 binding 更新用的 session_id 快照（与 resume_session_id 同值，独立 clone 避免 move 后无法使用）
    sid_for_binding: Option<String>,
}

/// 从 p2p 队列取出下一条消息的结果。
///
/// 抽成枚举而非 Option<Option<PendingMessage>>，是为了让 drain 循环的三种状态
/// （取到消息 / 队列空已清理 / 并发插入需重试）各自有名分支，可读且便于单测。
#[derive(Debug)]
enum QueuePop {
    /// 取到下一条消息，继续执行
    /// 用 Box 装 PendingMessage：PendingMessage 含多个 String/HashMap 让 enum 体积达 344 字节，
    /// clippy large_enum_variant 要求大变体走堆；drain 循环里短暂持有，Box 的间接寻址可忽略。
    Message(Box<PendingMessage>),
    /// 队列已空并已清理条目，drain 循环应退出
    Drained,
    /// 并发 push 在 pop 之后插入了消息但本次未取到，需重新循环
    Retry,
}

pub struct MessageDebounce {
    entries: Arc<DashMap<(i64, String), DebounceEntry>>,
    ctx: ServiceContext,
    /// Loop Runner，用于处理 default_response_loop 类型的消息
    loop_runner: Option<Arc<crate::services::loop_runner::LoopRunner>>,
    /// 私聊串行队列：按 (bot_id, workspace_id, sender) 维度缓存等待执行的消息
    /// 当某维度已有执行器在运行时，新消息入队而非并行执行
    p2p_queue: Arc<DashMap<P2pQueueKey, VecDeque<PendingMessage>>>,
    /// 私聊运行状态标记：按 (bot_id, workspace_id, sender) 维度记录是否有执行器在运行
    /// 使用 DashMap 的 insert/remove 原子操作避免竞态条件
    p2p_running: Arc<DashMap<P2pQueueKey, ()>>,
}

impl MessageDebounce {
    /// 暴露 LoopRunner 给飞书卡片 act:/runloop 触发环路执行用。
    pub fn loop_runner(&self) -> Option<&Arc<crate::services::loop_runner::LoopRunner>> {
        self.loop_runner.as_ref()
    }

    pub fn new(
        ctx: ServiceContext,
        loop_runner: Option<Arc<crate::services::loop_runner::LoopRunner>>,
    ) -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            ctx,
            loop_runner,
            p2p_queue: Arc::new(DashMap::new()),
            p2p_running: Arc::new(DashMap::new()),
        }
    }

    /// Push a message into the debounce buffer. Resets the timer for this key.
    /// 对于私聊消息，如果该维度(bot_id, workspace_id, sender)已有执行器在运行，
    /// 则将消息入队并回复"正在运行中，稍后执行"，而非并行执行。
    /// 使用 DashMap 的 insert 原子操作避免竞态条件。
    pub fn push(&self, msg: PendingMessage) {
        // 私聊消息串行队列拦截：使用 p2p_running 的原子 insert 操作判断是否有执行器在运行
        if msg.chat_type == "p2p" {
            let workspace_id = msg.workspace_id.unwrap_or(NO_WORKSPACE);
            let queue_key = (msg.bot_id, workspace_id, msg.sender.clone());
            // 原子操作：insert 返回旧值
            // - Some(())：之前已在运行，新消息入队
            // - None：之前没运行，本次 insert 已标记为运行，直接执行
            let was_running = self.p2p_running.insert(queue_key.clone(), ()).is_some();
            if was_running {
                // 当前有执行器在运行：入队并回复"稍后执行"
                tracing::info!(
                    "[p2p-queue] 执行器运行中，消息入队: bot_id={}, workspace_id={}, sender={}, content_preview={:?}",
                    msg.bot_id, workspace_id, msg.sender, msg.content.chars().take(50).collect::<String>()
                );
                // 发送"正在运行中，稍后执行"提示
                let tx = self.ctx.tx.clone();
                let bot_id = msg.bot_id;
                let receive_id = msg.sender.clone();
                let receive_id_type = "open_id".to_string();
                // 先入队，再根据长度决定提示文案（包含本条消息后的队列长度）
                let mut queue = self.p2p_queue.entry(queue_key).or_default();
                // 队列上限保护：满则丢弃本条并提示，避免恶意刷屏导致内存无限增长。
                // 持有写锁期间只读 len；tx.send 走 broadcast 通道，不回调 DashMap，不会死锁。
                if queue.len() >= MAX_P2P_QUEUE_LEN {
                    let content = format!(
                        "⚠️ 排队消息过多（上限 {} 条），本条已丢弃，请稍后再试",
                        MAX_P2P_QUEUE_LEN
                    );
                    let _ = tx.send(ExecEvent::DirectCardMessage {
                        bot_id,
                        receive_id,
                        receive_id_type,
                        content,
                    });
                    return;
                }
                queue.push_back(msg);
                let queue_len = queue.len();
                let wait_hint = if queue_len <= 1 {
                    "⏳ 正在运行中，稍后执行".to_string()
                } else {
                    format!("⏳ 正在运行中，前面还有 {} 条消息排队，稍后执行", queue_len - 1)
                };
                let _ = tx.send(ExecEvent::DirectCardMessage {
                    bot_id,
                    receive_id,
                    receive_id_type,
                    content: wait_hint,
                });
                return;
            }
        }

        let key = (msg.bot_id, msg.chat_id.clone());

        // Remove old entry and collect existing messages
        let mut all_msgs = self
            .entries
            .remove(&key)
            .map(|(_, old)| {
                old.timer.abort();
                old.messages
            })
            .unwrap_or_default();
        all_msgs.push(msg);

        // @提及是显式点名请求，跳过 debounce 等待立即执行；
        // 在 all_msgs move 进闭包前先计算好。
        let has_immediate = all_msgs.iter().any(|m| m.immediate);

        // Create new timer
        let new_timer = {
            let entries = self.entries.clone();
            let db = self.ctx.db.clone();
            let executor_registry = self.ctx.executor_registry.clone();
            let tx = self.ctx.tx.clone();
            let task_manager = self.ctx.task_manager.clone();
            let config = self.ctx.config.clone();
            // 专家管理器 clone 进 timer 闭包，让 debounce 触发的执行也能注入专家上下文
            let expert_manager = self.ctx.expert_manager.clone();
            // loop_runner 需要在 async block 之前 clone，避免 self 生命周期问题
            let loop_runner = self.loop_runner.clone();
            let bot_id = key.0;
            let chat_id = key.1.clone();
            let target_type = all_msgs
                .first()
                .map(|m| m.chat_type.clone())
                .unwrap_or_default();
            // 私聊串行队列：clone Arc 引用，用于执行完成后检查队列
            let p2p_queue = self.p2p_queue.clone();
            // 私聊运行状态：clone Arc 引用，用于执行完成后清除运行标记
            let p2p_running = self.p2p_running.clone();

            tokio::spawn(async move {
                // 群聊需要 debounce 等待窗口，避免多条消息触发多次执行；
                // 但 @提及是显式点名请求，跳过等待立即执行。
                if target_type == "group" && !has_immediate {
                    let secs = db
                        .get_debounce_secs(bot_id, &target_type)
                        .await
                        .unwrap_or(20)
                        .max(1);
                    tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await;
                }

                // Timer fired: drain all pending messages for this key
                let key = (bot_id, chat_id);
                let pending = entries.remove(&key);
                if let Some((_, entry)) = pending {
                    if entry.messages.is_empty() {
                        return;
                    }

                    let merged_content = merge_pending_messages(&entry.messages);

                    // entry.messages 在上面已确认非空（is_empty 检查），last() 必然有值
                    let Some(last) = entry.messages.last() else { return; };

                    // p2p 运行标记 guard：必须在「首次执行之前」创建，覆盖「首次执行 + 队列 drain」
                    // 全过程。guard drop 时清除 running 标记——即使首次执行 panic，drop 也会执行，
                    // 保证该维度不被永久死锁。group 消息不进串行队列，guard 为 None。
                    let _p2p_guard = if target_type == "p2p" {
                        let qk = (bot_id, last.workspace_id.unwrap_or(NO_WORKSPACE), last.sender.clone());
                        Some(P2pRunningGuard { key: qk, running: p2p_running.clone() })
                    } else {
                        None
                    };

                    // 解析 resume/参数（TOCTOU 检查 + exec_message/params 构建），主路径与 drain 共用
                    let resolved = Self::resolve_execution(last, &merged_content, &db).await;

                    // 分发执行（trigger_type 路由），主路径与 drain 共用
                    let result = Self::dispatch_execution(
                        last, &merged_content, &resolved,
                        &db, &executor_registry, &task_manager, &config, &tx, &loop_runner,
                        &expert_manager,
                    )
                    .await;

                    let record_id = result.as_ref().ok().and_then(|r| r.record_id);
                    tracing::debug!(
                        "[debounce] timer fired for bot_id={}, chat_id={}, msg_count={}, record_id={:?}",
                        bot_id, key.1, entry.messages.len(), record_id
                    );
                    if let Err(e) = &result {
                        tracing::warn!("[debounce] failed to execute todo {}: {:?}", last.todo_id, e);
                    }
                    // binding 状态更新（成功设 RUNNING+session_id，失败重置 IDLE），主路径与 drain 共用
                    Self::update_binding(
                        last.binding_id, &result, resolved.is_resume,
                        resolved.sid_for_binding.as_deref(), &db,
                    )
                    .await;
                    // 批量标记整批消息（合并执行，整批都算已处理）
                    for msg in &entry.messages {
                        Self::mark_message(msg, &result, &db).await;
                    }

                    // 私聊串行队列：执行完成后检查队列，有下一条消息则自动执行
                    // 用 loop 循环逐条处理队列中的消息，直到队列清空
                    if target_type == "p2p" {
                        let workspace_id = last.workspace_id.unwrap_or(NO_WORKSPACE);
                        let queue_key = (bot_id, workspace_id, last.sender.clone());

                        // guard 已在首次执行前创建（见上方 _p2p_guard），此处仅做队列 drain。
                        // queue_key 与 guard 内的 key 同源；drain 完成、当前块结束后 guard drop，
                        // 自动清除 running 标记。
                        // 循环处理队列中的所有待执行消息
                        loop {
                            // 取出下一条消息（空则原子清理条目，避免与并发 push 竞态丢消息）
                            let next = match Self::pop_or_drain_queue(&p2p_queue, &queue_key) {
                                QueuePop::Message(m) => m,
                                QueuePop::Drained => break,
                                QueuePop::Retry => continue,
                            };
                            tracing::info!(
                                "[p2p-queue] 执行完成，从队列取出下一条消息: bot_id={}, workspace_id={}, sender={}, content_preview={:?}",
                                bot_id, workspace_id, next.sender, next.content.chars().take(50).collect::<String>()
                            );
                            // 直接在当前任务中执行队列消息，无需 debounce 等待
                            // 单条消息直接用其 content，无需走 batch 合并：
                            // merge_pending_messages 对单条等价于 content 本身，省一次 Vec 分配。
                            // 解析 + 分发执行 + binding 更新 + 单条标记，全部走与主路径相同的 helper，
                            // 消除原先与主路径重复的 ~80 行拷贝（并修掉 drain 静默吞错、缺 record_id 分支的分叉）
                            let merged = next.content.clone();
                            let resolved = Self::resolve_execution(&next, &merged, &db).await;
                            let r = Self::dispatch_execution(
                                &next, &merged, &resolved,
                                &db, &executor_registry, &task_manager, &config, &tx, &loop_runner,
                                &expert_manager,
                            )
                            .await;
                            Self::update_binding(
                                next.binding_id, &r, resolved.is_resume,
                                resolved.sid_for_binding.as_deref(), &db,
                            )
                            .await;
                            Self::mark_message(&next, &r, &db).await;
                            // 继续循环处理下一条
                        }
                    }
                }
            })
        };

        self.entries.insert(
            key,
            DebounceEntry {
                messages: all_msgs,
                timer: new_timer,
            },
        );
    }

    pub fn pending_count(&self) -> usize {
        self.entries.iter().map(|e| e.messages.len()).sum()
    }

    // ========================================================================
    // 单条消息执行链路：resume 检查 → 分发执行 → binding 更新 → 消息标记
    //
    // 这组 helper 把主路径（debounce batch，标记整批）和 p2p 队列 drain 路径
    // （单条标记）共用的逻辑抽出来，消除原先两份近乎相同的拷贝。两份拷贝已经
    // 出现分叉（drain 静默吞错、缺 record_id 分支），统一到此处后行为一致。
    // ========================================================================

    /// resume session 的 TOCTOU 检查。
    ///
    /// debounce/drain 期间 binding 可能被重新绑定到不同 todo_id；todo_id 变了才
    /// 降级为新执行（返回 None），只要 session_id 还在且 todo_id 未变就继续多轮对话。
    async fn check_resume_session(msg: &PendingMessage, db: &Arc<Database>) -> Option<String> {
        let mut resume_sid = msg.resume_session_id.clone();
        if resume_sid.is_some() {
            if let Some(binding_id) = msg.binding_id {
                if let Ok(Some(binding)) = db.get_feishu_project_binding_by_id(binding_id).await {
                    if binding.todo_id != msg.todo_id {
                        tracing::warn!(
                            "[debounce] binding {} todo_id changed ({} → {}), dropping resume",
                            binding_id, msg.todo_id, binding.todo_id
                        );
                        resume_sid = None;
                    }
                }
            }
        }
        resume_sid
    }

    /// 解析单条消息的执行参数：resume 检查 + exec_message/params 构建。
    ///
    /// 主路径与 drain 路径共用，确保两处的 resume 降级、prompt 替换、params 构造
    /// 完全一致，不会因分叉产生微妙差异。
    async fn resolve_execution(
        msg: &PendingMessage,
        merged_content: &str,
        db: &Arc<Database>,
    ) -> ResolvedExecution {
        let resume_message = msg.resume_message.clone();
        let resume_sid = Self::check_resume_session(msg, db).await;
        // resume：把用户内容拼进系统 prompt（保留项目上下文）；
        // 新执行：原样 todo_prompt，由 replace_placeholders 在执行时替换 {{message}}。
        let exec_message = if resume_sid.is_some() {
            msg.todo_prompt.replace("{{message}}", merged_content)
        } else {
            msg.todo_prompt.clone()
        };
        let mut params = msg.params.clone().unwrap_or_default();
        params.insert("content".to_string(), merged_content.to_string());
        params.insert("message".to_string(), merged_content.to_string());
        let is_resume = resume_sid.is_some();
        let sid_for_binding = resume_sid.clone();
        ResolvedExecution {
            exec_message,
            params: if is_resume { None } else { Some(params) },
            resume_session_id: resume_sid,
            resume_message,
            is_resume,
            sid_for_binding,
        }
    }

    /// 按消息来源决定飞书回复目标：群聊回群（chat_id），私聊回个人（open_id）。
    fn feishu_reply_target(msg: &PendingMessage) -> (String, String) {
        if msg.chat_type == "group" {
            (msg.chat_id.clone(), "chat_id".to_string())
        } else {
            (msg.sender.clone(), "open_id".to_string())
        }
    }

    /// 构造 todo 执行请求（_ 分支：普通默认响应或斜杠命令）。
    ///
    /// 抽出来让 dispatch_execution 的 _ 分支保持简短；clone Arc 引用是因为
    /// RunTodoExecutionRequest 需要 owned，而同一批依赖在 drain 循环里要复用。
    #[allow(clippy::too_many_arguments)]
    fn build_run_todo_request(
        msg: &PendingMessage,
        resolved: &ResolvedExecution,
        db: &Arc<Database>,
        executor_registry: &Arc<crate::adapters::ExecutorRegistry>,
        tx: &broadcast::Sender<ExecEvent>,
        task_manager: &Arc<TaskManager>,
        config: &Arc<std::sync::RwLock<crate::config::Config>>,
        expert_manager: &Arc<crate::expert::ExpertIndexManager>,
    ) -> RunTodoExecutionRequest {
        let (receive_id, receive_id_type) = Self::feishu_reply_target(msg);
        RunTodoExecutionRequest {
            db: db.clone(),
            executor_registry: executor_registry.clone(),
            tx: tx.clone(),
            task_manager: task_manager.clone(),
            config: config.clone(),
            todo_id: msg.todo_id,
            message: resolved.exec_message.clone(),
            req_executor: msg.executor.clone(),
            req_model: None,
            trigger_type: msg.trigger_type.clone(),
            params: resolved.params.clone(),
            resume_session_id: resolved.resume_session_id.clone(),
            resume_message: resolved.resume_message.clone(),
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,
            step_id: None,
            feishu_bot_id: Some(msg.bot_id),
            feishu_receive_id: Some(receive_id),
            feishu_receive_id_type: Some(receive_id_type),
            workspace_path: None,
            workspace_id: msg.workspace_id,
            // 飞书消息触发路径：注入专家上下文，让飞书消息触发的 todo 也加载专家 prompt
            expert_manager: Some(expert_manager.clone()),
        }
    }

    /// 按 trigger_type 分发执行单条消息。
    ///
    /// 三条分支：环路（loop）/ 执行器直连（executor）/ 普通 todo 执行。
    /// 主路径与 drain 路径共用，保证分发逻辑一致。
    #[allow(clippy::too_many_arguments)]
    async fn dispatch_execution(
        msg: &PendingMessage,
        merged_content: &str,
        resolved: &ResolvedExecution,
        db: &Arc<Database>,
        executor_registry: &Arc<crate::adapters::ExecutorRegistry>,
        task_manager: &Arc<TaskManager>,
        config: &Arc<std::sync::RwLock<crate::config::Config>>,
        tx: &broadcast::Sender<ExecEvent>,
        loop_runner: &Option<Arc<crate::services::loop_runner::LoopRunner>>,
        expert_manager: &Arc<crate::expert::ExpertIndexManager>,
    ) -> Result<crate::executor_service::ExecutionResult, Option<String>> {
        match msg.trigger_type.as_str() {
            "default_response_loop" | "slash_command_loop" => {
                let (rid, rtype) = Self::feishu_reply_target(msg);
                Self::handle_default_response_loop(
                    db.clone(),
                    loop_runner.clone(),
                    msg.todo_id,
                    merged_content,
                    Some(msg.bot_id),
                    Some(rid),
                    Some(rtype),
                )
                .await
            }
            "default_response_executor" => {
                let (rid, rtype) = Self::feishu_reply_target(msg);
                Self::handle_default_response_executor(
                    db,
                    executor_registry,
                    task_manager,
                    config,
                    tx,
                    msg.bot_id,
                    rid,
                    &rtype,
                    msg.executor.as_deref(),
                    msg.workspace_id,
                    merged_content,
                    resolved.resume_session_id.clone(),
                )
                .await
            }
            _ => {
                let request = Self::build_run_todo_request(
                    msg, resolved, db, executor_registry, tx, task_manager, config, expert_manager,
                );
                let result = if request.params.is_some() {
                    run_todo_execution_with_params(request).await
                } else {
                    run_todo_execution(request).await
                };
                Ok(result)
            }
        }
    }

    /// 执行后更新 binding 状态。
    ///
    /// 成功：设 RUNNING + session_id（resume 用原 sid，首次执行从执行记录读真实 sid）；
    /// record_id 缺失时仍更新 status。失败：重置 IDLE，让下次消息尝试新 session。
    /// 主路径与 drain 共用，统一了原 drain 路径静默吞错（let _ =）的问题——现在都 warn!。
    async fn update_binding(
        binding_id: Option<i64>,
        result: &Result<crate::executor_service::ExecutionResult, Option<String>>,
        is_resume: bool,
        sid_for_binding: Option<&str>,
        db: &Arc<Database>,
    ) {
        let Some(binding_id) = binding_id else { return };
        match result {
            Ok(exec_result) => match exec_result.record_id {
                Some(rid) => {
                    let sid = if is_resume {
                        sid_for_binding.map(str::to_string)
                    } else {
                        // 首次执行：从执行记录读真实 session_id，供后续消息 resume
                        db.get_execution_record(rid)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|r| r.session_id)
                    };
                    if let Err(e) = db
                        .update_feishu_project_binding_session(
                            binding_id,
                            sid.as_deref(),
                            rid,
                            crate::models::binding_status::RUNNING,
                        )
                        .await
                    {
                        tracing::warn!(
                            "[debounce] failed to update binding {} session: {:?}",
                            binding_id, e
                        );
                    }
                }
                None => {
                    // record_id 缺失：仍更新 status 为 RUNNING
                    if let Err(e) = db
                        .update_feishu_project_binding_status(
                            binding_id,
                            crate::models::binding_status::RUNNING,
                        )
                        .await
                    {
                        tracing::warn!(
                            "[debounce] failed to update binding {} status: {:?}",
                            binding_id, e
                        );
                    }
                }
            },
            Err(_) => {
                // 失败：重置 IDLE，让下次消息尝试新 session
                if let Err(e) = db
                    .update_feishu_project_binding_status(
                        binding_id,
                        crate::models::binding_status::IDLE,
                    )
                    .await
                {
                    tracing::warn!(
                        "[debounce] failed to reset binding {} to idle: {:?}",
                        binding_id, e
                    );
                }
            }
        }
    }

    /// 按执行结果标记单条消息。
    ///
    /// 成功 → processed；环路暂停等（Some 原因）→ processed_with_error；
    /// 其他错误（None）→ failed。主路径对 batch 每条消息调用（合并执行，整批都算
    /// 已处理），drain 路径对单条调用。统一了原 drain 路径静默吞错的问题。
    async fn mark_message(
        msg: &PendingMessage,
        result: &Result<crate::executor_service::ExecutionResult, Option<String>>,
        db: &Arc<Database>,
    ) {
        let Some(msg_id) = msg.message_id.as_ref() else { return };
        match result {
            Ok(exec_result) => {
                if let Err(e) = db
                    .mark_feishu_message_processed(
                        msg_id,
                        msg.todo_id,
                        exec_result.record_id,
                        Some(&msg.trigger_type),
                    )
                    .await
                {
                    tracing::warn!(
                        "[debounce] failed to mark message {} as processed: {:?}",
                        msg_id, e
                    );
                }
            }
            Err(e) => match e {
                Some(reason) => {
                    // 环路暂停等：标记已处理 + 记录错误原因
                    if let Err(mark_err) = db
                        .mark_feishu_message_processed_with_error(
                            msg_id,
                            msg.todo_id,
                            Some(&msg.trigger_type),
                            reason,
                        )
                        .await
                    {
                        tracing::warn!(
                            "[debounce] failed to mark message {} as processed_with_error: {:?}",
                            msg_id, mark_err
                        );
                    }
                }
                None => {
                    // 其他错误：标记为未处理，待重试
                    if let Err(mark_err) = db.mark_feishu_message_failed(msg_id).await {
                        tracing::warn!(
                            "[debounce] failed to mark message {} as failed: {:?}",
                            msg_id, mark_err
                        );
                    }
                }
            },
        }
    }

    /// 从 p2p 队列取出下一条消息；队列空时原子清理条目。
    ///
    /// 取到消息 → Message；队列空且已清理 → Drained；并发 push 让队列非空但
    /// 本次 pop 没取到 → Retry。抽出来便于单测 remove_if 的竞态修复逻辑
    /// （旧实现用单独 remove() 会丢失 pop 与 remove 之间插入的消息）。
    fn pop_or_drain_queue(
        queue: &DashMap<P2pQueueKey, VecDeque<PendingMessage>>,
        key: &P2pQueueKey,
    ) -> QueuePop {
        // 先尝试 pop；get_mut 持有的写锁在 pop_front 后即释放，不跨 await
        if let Some(next) = queue.get_mut(key).and_then(|mut q| q.pop_front()) {
            return QueuePop::Message(Box::new(next));
        }
        // 队列为空：用 remove_if 原子地「仅当为空时才删除」，避免与并发 push 竞态丢消息
        let drained = queue.remove_if(key, |_k, q| q.is_empty()).is_some();
        if drained || !queue.contains_key(key) {
            QueuePop::Drained
        } else {
            QueuePop::Retry
        }
    }
}

/// 把一个 chat 在 debounce 窗口里攒下来的所有消息合并成一段文本。
///
/// 规则: 消息之间用 `\n---\n` 分隔(飞书用户复制粘贴的多段对话可读性最好,
///  Claude 也习惯用 `---` 识别章节边界)。原始消息里的换行不会做进一步处理,
/// 因为 `{{message}}` 替换的目标 prompt 大多有自己的格式。
///
/// 这是纯函数,只读 `messages` 字段,不触发任何 I/O —— `push` 在 debounce
/// 窗口到期时调用它,网络/DB 调用全部留在外面。
pub fn merge_pending_messages(messages: &[PendingMessage]) -> String {
    messages
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<&str>>()
        .join("\n---\n")
}

// ============================================================================
// 执行器直接响应的反馈消息格式化 + 超时运行辅助
// ============================================================================
//
// `handle_default_response_executor` 原先把"等子进程退出"和"发飞书消息"耦合在一起，
// 且用 `wait_with_output()` 无超时地等——provider 挂起时整条 debounce 任务永久卡死，
// 用户侧表现为"发了消息毫无反馈"。这一组纯函数/小函数把可测的逻辑抽出来：
// 三类反馈消息（开始/错误/空结束）的格式 + 带超时地运行子进程并在超时时 kill 回收。

/// 把 config 里的 `execution_timeout_secs` 解析成直连执行器要用的超时。
///
/// 飞书直连执行器与 todo 执行路径共用同一把全局超时旋钮（`execution_timeout_secs`），
/// 不再维护独立的写死阈值。`0` 在 todo pipeline 里表示「不限制」
/// （`timeout_enabled = v > 0`），这里遵循同一语义：`0` → `None`，走无超时等待分支。
/// 原因是 `tokio::time::timeout(Duration::from_secs(0), ..)` 会立刻返回 `Elapsed`，
/// 若把 0 直接当超时传进去，所有直连调用会被瞬间判死。
fn direct_executor_timeout(secs: u64) -> Option<std::time::Duration> {
    if secs == 0 {
        None
    } else {
        Some(std::time::Duration::from_secs(secs))
    }
}

/// 开始执行时发回飞书的标志文本。
///
/// 复用全仓 `status_icon`（`cli/commands.rs`）的 ⏳ 约定。preview 只取前 30 个
/// Unicode scalar 并加省略号：用户发来的 prompt 可能很长，原样刷到飞书会话里
/// 是噪声；这里只是让用户知道"开始处理了 + 处理的是哪段话"。
/// 按 char 而非 byte 截断，避免把多字节中文切成乱码。
fn executor_start_message(executor_type: &str, message_preview: &str) -> String {
    const PREVIEW_CHAR_LIMIT: usize = 30;
    let preview: String = message_preview.chars().take(PREVIEW_CHAR_LIMIT).collect();
    // 取到的长度等于原文长度说明没被截断，不加省略号；否则加 … 提示被裁了
    let suffix = if message_preview.chars().count() > PREVIEW_CHAR_LIMIT { "…" } else { "" };
    format!("⏳ {} 开始处理：{}{}", executor_type, preview, suffix)
}

/// 执行失败时发回飞书的错误文本，原因原样透传。
///
/// 调用方传入具体原因（超时秒数 / spawn 失败 / wait 失败 / 非零退出码+输出片段），
/// 让用户侧不再是静默失败，而是明确知道"执行器挂了 + 挂在哪一步"。
fn executor_error_message(executor_type: &str, reason: &str) -> String {
    format!("❌ {} 执行失败：{}", executor_type, reason)
}

/// 执行成功但没有任何输出时的结束标志。
///
/// 【待清理】已废弃：结束消息统一使用"✅ <执行器名称> 处理完成"，不再区分有无输出。
/// 保留此函数供参考，确认无用后可删除。
#[allow(dead_code)]
fn executor_empty_end_message(executor_type: &str) -> String {
    format!("✅ {} 执行完成（无输出）", executor_type)
}

/// 从执行日志中提取 session_id。
///
/// 流程：
/// 1. 先尝试从日志内容中提取（extract_session_id）
/// 2. 如果没有，尝试执行器内部缓存的 session_id（get_session_id）
///
/// 不同执行器暴露 session_id 的方式不同：
/// - Claude Code: stdout JSONL 行含 session_id
/// - Hermès: `session_id: <sid>` 行
/// - Pi: `{"type":"session","id":"<sid>"}` 行（通过 get_session_id 获取缓存值）
///
/// 返回 None 表示执行器不支持 session 或首次执行。
fn extract_session_from_logs(
    executor: &Arc<dyn crate::adapters::CodeExecutor>,
    logs: &[ParsedLogEntry],
) -> Option<String> {
    // 1. 优先从日志内容提取
    for entry in logs {
        if let Some(sid) = executor.extract_session_id(&entry.content) {
            return Some(sid);
        }
    }
    // 2. 回退到执行器内部缓存的 session_id（Pi 等执行器在 parse_output_line 时缓存）
    executor.get_session_id()
}

/// 根据执行结果决定发回飞书的最终内容。
///
/// 成功时统一返回简洁的结束标志（"✅ <执行器名称> 处理完成"），不再重复输出
/// result_text 或 stdout（过程消息已通过 DirectStreamMessage 实时推送，避免重复）。
/// 失败时返回错误消息 + 输出片段，方便诊断问题。
/// 把这段决策从 `handle_default_response_executor` 主体抽成纯函数，是为了能直接单测
/// 非零退出 + 多字节中文 stdout 的截断行为——原来内联时 `&stdout[..1500]` 按字节切片，
/// 落在中文中间会 panic，反而把"执行器非零退出"变成 debounce 任务崩溃。
fn build_executor_end_content(
    executor_type: &str,
    status: &std::process::ExitStatus,
    _result_text: Option<String>,
    _stdout: &str,
    stderr: &str,
) -> String {
    // 非零退出时 stderr 给用户的预览上限，按 char 计：与开始消息 preview 同语义，
    // 避免 `&str[..n]` 这种按字节切片切到多字节字符中间触发 panic。
    const STDERR_PREVIEW_CHAR_LIMIT: usize = 1500;
    if status.success() {
        // 进程退出 0：统一发简洁结束标志，过程内容已实时推送，不再重复输出
        format!("✅ {} 处理完成", executor_type)
    } else {
        // 非零退出：用 stderr 展示错误信息（执行器错误信息通常走 stderr），
        // stderr 为空则只报退出码。
        let diagnostic = stderr.chars().take(STDERR_PREVIEW_CHAR_LIMIT).collect::<String>();
        if diagnostic.trim().is_empty() {
            executor_error_message(executor_type, &format!("退出码 {:?}", status.code()))
        } else {
            executor_error_message(
                executor_type,
                &format!("退出码 {:?}\n{}", status.code(), diagnostic),
            )
        }
    }
}

/// `run_executor_with_timeout` 的失败原因。
///
/// 区分"超时"和"wait 本身出错"是因为两者的用户提示不同：超时基本是 provider
/// 挂起，wait 失败更可能是本地进程/系统层问题。调用方据此拼错误消息。
#[derive(Debug)]
enum ExecutorRunError {
    /// 子进程在 `timeout` 内未退出，已被 kill 回收。
    Timeout { secs: u64 },
    /// `child.wait()` 本身返回了错误（例如系统层 IO 失败）。
    WaitFailed(String),
}

/// 带超时地运行子进程：并发读取 stdout + `child.wait()` 与计时器竞赛。
///
/// 不用 `Child::wait_with_output`——它按值消费 `child`，超时分支就拿不到句柄
/// 去 kill，pi 会变成孤儿进程继续占资源。这里改成先 `take()` 出 stdout
/// 在独立 task 里 `read_to_end`，再用 `&mut child.wait()` 参与超时竞赛；
/// 超时则 `start_kill`（发 SIGKILL）+ `wait` 回收僵尸，返回 `Timeout`。
/// 成功则把 (退出状态, stdout 字节) 一起返回给上层解析。
///
/// `timeout` 为 `Some` 时按上述超时竞赛执行；为 `None` 时表示用户把
/// `execution_timeout_secs` 设为 `0`（不限制），此时直接 `child.wait()` 等到进程退出，
/// 不包 `tokio::time::timeout`、不 kill——语义与 todo pipeline 的
/// `timeout_enabled = v > 0` 对齐。
///
/// 注意：生产代码已改用 `wait_child_with_timeout` + `stream_executor_stdout` 替代此函数，
/// 保留此函数仅供测试覆盖超时/kill 行为。
#[allow(dead_code)]
async fn run_executor_with_timeout(
    mut child: tokio::process::Child,
    timeout: Option<std::time::Duration>,
) -> Result<(std::process::ExitStatus, Vec<u8>), ExecutorRunError> {
    // 先把 stdout 拿走，独立 task 里读完整缓冲。这样 wait 的计时竞赛只管
    // 进程退出，不阻塞在 stdout 读取上；超时分支也还能 kill child。
    let stdout_handle = child.stdout.take();
    let stdout_task = stdout_handle.map(|mut reader| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            // 读取失败时返回已收到的部分；调用方按字节解析，不因读错误整体失败
            let _ = reader.read_to_end(&mut buf).await;
            buf
        })
    });

    // Some → 计时竞赛；None → 无超时等待。两条分支成功后都要 join stdout task。
    // 错误分支用 `return Err(..)` 提前退出，故此处 `wait_outcome` 即 `ExitStatus`。
    let wait_outcome = match timeout {
        Some(t) => match tokio::time::timeout(t, child.wait()).await {
            Ok(Ok(status)) => status,
            // wait 本身出错：本地进程/系统层问题，带上错误信息让用户看得到
            Ok(Err(e)) => return Err(ExecutorRunError::WaitFailed(e.to_string())),
            Err(_) => {
                // 超时：先 SIGKILL 再 wait 回收，避免 pi 孤儿进程；两步失败都不致命，用 _ 忽略
                let _ = child.start_kill();
                let _ = child.wait().await;
                // 此处故意不 join stdout_task：kill 后子进程管道关闭，read_to_end 收到 EOF
                // 自行结束，task 变 detached 但不会泄漏；超时路径本就不要输出，buf 随 task 丢弃。
                return Err(ExecutorRunError::Timeout { secs: t.as_secs() });
            }
        },
        None => match child.wait().await {
            Ok(status) => status,
            // wait 本身出错的情况与 Some 分支一致，复用同一个错误变体
            Err(e) => return Err(ExecutorRunError::WaitFailed(e.to_string())),
        },
    };

    // 进程已退出，join 读 stdout 的 task 拿完整输出；task 若 panic 则当无输出。
    // 用 async block 而非闭包，才能在里面 await join handle。
    let buf = match stdout_task {
        Some(task) => task.await.unwrap_or_default(),
        None => Vec::new(),
    };
    Ok((wait_outcome, buf))
}

/// 仅等待子进程退出（不读 stdout），超时则 kill 回收
///
/// 与 run_executor_with_timeout 的区别：不负责读取 stdout，因为 stdout 句柄
/// 在调用前已被 take() 走用于流式读取。此函数只做 wait + timeout + kill。
async fn wait_child_with_timeout(
    mut child: tokio::process::Child,
    timeout: Option<std::time::Duration>,
) -> Result<std::process::ExitStatus, ExecutorRunError> {
    match timeout {
        // Some → 计时竞赛；None → 无超时等待
        Some(t) => match tokio::time::timeout(t, child.wait()).await {
            Ok(Ok(status)) => Ok(status),
            Ok(Err(e)) => Err(ExecutorRunError::WaitFailed(e.to_string())),
            Err(_) => {
                // 超时：SIGKILL + wait 回收，避免孤儿进程
                let _ = child.start_kill();
                let _ = child.wait().await;
                Err(ExecutorRunError::Timeout { secs: t.as_secs() })
            }
        },
        None => match child.wait().await {
            Ok(status) => Ok(status),
            Err(e) => Err(ExecutorRunError::WaitFailed(e.to_string())),
        },
    }
}

/// 流式读取执行器 stdout 的结果
///
/// logs：解析后的日志条目，用于提取最终结果（get_final_result_from_logs）
/// raw_stdout：原始 stdout 文本，用于错误诊断和兜底回复
struct StdoutStreamResult {
    logs: Vec<ParsedLogEntry>,
    raw_stdout: String,
}

/// 流式读取执行器 stdout：逐行解析并发送 Output 事件
///
/// 复用 log_capture.rs 的 EventPipeline 创建和解析逻辑，确保 executor 直连执行
/// 与 todo 执行产生完全相同格式的事件。发送的 ExecEvent::Output 会被
/// FeishuPushService 按 push_level 推送到飞书（"all" 时推送过程事件）。
///
/// `direct_output_info` 为 Some 时，每条解析出的日志额外发一条
/// DirectStreamMessage 直接推送给触发用户（一对一私聊场景），
/// 这样即使 push target 未配置该用户，用户也能在聊天中看到执行过程。
async fn stream_executor_stdout<R: tokio::io::AsyncRead + Unpin + Send>(
    stdout_handle: Option<R>,
    executor: &Arc<dyn crate::adapters::CodeExecutor>,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    workspace_id: Option<i64>,
    direct_output_info: Option<DirectOutputInfo>,
) -> StdoutStreamResult {
    let Some(stdout) = stdout_handle else {
        return StdoutStreamResult { logs: Vec::new(), raw_stdout: String::new() };
    };
    // 复用 log_capture 的 pipeline 创建逻辑，确保与 todo 执行路径一致
    let mut pipeline = log_capture::create_pipeline_for_executor(executor.as_ref())
        .unwrap_or_else(|| EventPipeline::new(executor.executor_type().as_str()));
    let mut reader = BufReader::new(stdout).lines();
    let mut result = StdoutStreamResult { logs: Vec::new(), raw_stdout: String::new() };

    while let Ok(Some(line)) = reader.next_line().await {
        process_executor_stdout_line(
            &line, &mut pipeline, executor, tx, task_id, workspace_id,
            direct_output_info.as_ref(), &mut result,
        );
    }
    finalize_pipeline_by_path(
        &mut pipeline, tx, task_id, workspace_id,
        direct_output_info.as_ref(), &mut result,
    );
    result
}

/// 一对一私聊场景下，过程消息直接推送的目标信息
struct DirectOutputInfo {
    bot_id: i64,
    receive_id: String,
    receive_id_type: String,
}

/// 处理单行 stdout：先尝试 pipeline 解析，回退到 executor 自定义解析
///
/// 该函数参数较多是因为需要同时处理 pipeline 解析、事件广播、结果收集三个职责，
/// 拆分成多个函数反而会导致数据结构重复传递，故用 allow 抑制参数数量告警。
#[allow(clippy::too_many_arguments)]
fn process_executor_stdout_line(
    line: &str,
    pipeline: &mut EventPipeline,
    executor: &Arc<dyn crate::adapters::CodeExecutor>,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    workspace_id: Option<i64>,
    direct_output: Option<&DirectOutputInfo>,
    result: &mut StdoutStreamResult,
) {
    result.raw_stdout.push_str(line);
    result.raw_stdout.push('\n');
    // 一对一私聊场景：手动解析，只发送 DirectStreamMessage，不调用 parse_and_broadcast（避免发送 Output 导致重复）
    if let Some(info) = direct_output {
        let parsed_list = parse_for_direct_stream(pipeline, line);
        if !parsed_list.is_empty() {
            for entry in &parsed_list {
                emit_direct_stream(tx, info, entry.clone());
            }
            result.logs.extend(parsed_list);
            return;
        }
        // 回退到 executor 自定义解析
        let Some(parsed) = executor.parse_output_line(line) else { return };
        emit_direct_stream(tx, info, parsed.clone());
        result.logs.push(parsed);
        return;
    }
    // 非私聊场景：走标准流程，调用 parse_and_broadcast 发送 Output 事件
    let parsed_list = log_capture::parse_and_broadcast(
        pipeline, line, tx, task_id, workspace_id,
    );
    if !parsed_list.is_empty() {
        result.logs.extend(parsed_list);
        return;
    }
    // 回退到 executor 自定义解析（非 JSONL 格式的行）
    let Some(parsed) = executor.parse_output_line(line) else { return };
    log_capture::send_event(
        tx,
        ExecEvent::Output {
            task_id: task_id.to_string(),
            entry: parsed.clone(),
            workspace_id,
        },
    );
    result.logs.push(parsed);
}

/// 手动解析 pipeline：只返回解析结果，不发送 Output 事件（由调用方决定发送方式）
///
/// 与 parse_and_broadcast 的区别：后者会发送 ExecEvent::Output，
/// 前者只返回 ParsedLogEntry，用于私聊场景避免重复发送。
fn parse_for_direct_stream(
    pipeline: &mut EventPipeline,
    line: &str,
) -> Vec<ParsedLogEntry> {
    let line_trimmed = line.trim();
    if line_trimmed.is_empty() {
        return Vec::new();
    }
    let len_before = pipeline.len();
    pipeline.feed(line_trimmed);
    let new_events: Vec<&crate::execution_events::ExecutionEvent> = pipeline.events()[len_before..].iter().collect();
    if new_events.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    for event in &new_events {
        // 只处理对用户有价值的事件类型（与 emit_direct_stream 的过滤规则一致）
        match event {
            crate::execution_events::ExecutionEvent::Info { message } => {
                if message.starts_with('{') || message.is_empty() {
                    continue;
                }
                let parsed = crate::execution_events::DbLogEntry::from_event_to_parsed_log_entry(event);
                results.push(parsed);
            }
            crate::execution_events::ExecutionEvent::Thinking { .. }
            | crate::execution_events::ExecutionEvent::ToolCall { .. }
            | crate::execution_events::ExecutionEvent::ToolResult { .. }
            | crate::execution_events::ExecutionEvent::Assistant { .. }
            | crate::execution_events::ExecutionEvent::Result { .. } => {
                let parsed = crate::execution_events::DbLogEntry::from_event_to_parsed_log_entry(event);
                results.push(parsed);
            }
            // 跳过内部状态事件（与 emit_direct_stream 的过滤规则一致）
            crate::execution_events::ExecutionEvent::SessionStart { .. }
            | crate::execution_events::ExecutionEvent::SessionEnd { .. }
            | crate::execution_events::ExecutionEvent::StepStart { .. }
            | crate::execution_events::ExecutionEvent::StepFinish { .. }
            | crate::execution_events::ExecutionEvent::Tokens { .. }
            | crate::execution_events::ExecutionEvent::Cost { .. }
            | crate::execution_events::ExecutionEvent::Duration { .. }
            | crate::execution_events::ExecutionEvent::ModelSwitch { .. }
            | crate::execution_events::ExecutionEvent::Error { .. }
            | crate::execution_events::ExecutionEvent::Progress { .. }
            | crate::execution_events::ExecutionEvent::User { .. }
            | crate::execution_events::ExecutionEvent::System { .. } => {}
        }
    }
    results
}

/// finalize pipeline 并收集剩余事件（SessionEnd 等）
fn finalize_pipeline_by_path(
    pipeline: &mut EventPipeline,
    tx: &broadcast::Sender<ExecEvent>,
    task_id: &str,
    workspace_id: Option<i64>,
    direct_output: Option<&DirectOutputInfo>,
    result: &mut StdoutStreamResult,
) {
    let len_before = pipeline.len();
    pipeline.finalize();
    for event in &pipeline.events()[len_before..] {
        // 一对一私聊场景：手动转换，只发送 DirectStreamMessage
        if let Some(info) = direct_output {
            match event {
                crate::execution_events::ExecutionEvent::Thinking { .. }
                | crate::execution_events::ExecutionEvent::ToolCall { .. }
                | crate::execution_events::ExecutionEvent::ToolResult { .. }
                | crate::execution_events::ExecutionEvent::Assistant { .. }
                | crate::execution_events::ExecutionEvent::Result { .. } => {
                    let parsed = crate::execution_events::DbLogEntry::from_event_to_parsed_log_entry(event);
                    emit_direct_stream(tx, info, parsed.clone());
                    result.logs.push(parsed);
                }
                _ => {}
            }
            continue;
        }
        // 非私聊场景：走标准流程，发送 Output 事件
        let parsed = log_capture::emit_broadcast_event(event, tx, task_id, workspace_id);
        result.logs.push(parsed);
    }
}

/// 发送 DirectStreamMessage 事件：把单条日志直接推送给触发用户
///
/// 与 Output 事件的区别：绕过 push target 过滤和 workspace 隔离，
/// 一对一直接发送，用户在聊天中就能看到执行过程。
///
/// 过滤规则（参考 cc-connect 的简洁风格）：
/// - 保留：thinking（思考）、tool_call（工具调用）、tool_result（工具结果）、assistant/text（助手回复）、result（最终结果）
/// - 跳过：session_start/session_end、step_start/step_finish、tokens、model_switch、info、error（内部状态事件不打扰用户）
fn emit_direct_stream(
    tx: &broadcast::Sender<ExecEvent>,
    info: &DirectOutputInfo,
    entry: ParsedLogEntry,
) {
    // 过滤掉内部状态事件，只保留对用户有价值的思考和工具交互
    match entry.log_type.as_str() {
        "session_start" | "session_end" | "step_start" | "step_finish" => return,
        "tokens" | "model_switch" | "cost" | "duration" => return,
        "info" | "error" | "stderr" | "warning" => return,
        _ => {}
    }
    let _ = tx.send(ExecEvent::DirectStreamMessage {
        bot_id: info.bot_id,
        receive_id: info.receive_id.clone(),
        receive_id_type: info.receive_id_type.clone(),
        entry,
    });
}

// ============================================================================
// 默认响应处理器：处理 loop 和 executor 类型的默认响应
// ============================================================================

impl MessageDebounce {
    /// 处理默认响应类型为 loop 的情况
    /// 直接通过 LoopRunner 触发环路执行（fire-and-forget）
    async fn handle_default_response_loop(
        db: Arc<Database>,
        loop_runner: Option<Arc<crate::services::loop_runner::LoopRunner>>,
        loop_id: i64,
        message: &str,
        feishu_bot_id: Option<i64>,
        feishu_receive_id: Option<String>,
        feishu_receive_id_type: Option<String>,
    ) -> Result<crate::executor_service::ExecutionResult, Option<String>> {
        // 检查环路是否存在且状态为 enabled
        let loop_ = match db.get_loop(loop_id).await {
            Ok(Some(l)) => l,
            Ok(None) => {
                tracing::warn!("[debounce] loop {} not found", loop_id);
                return Err(None);
            }
            Err(e) => {
                tracing::error!("[debounce] failed to get loop {}: {}", loop_id, e);
                return Err(None);
            }
        };

        // 环路状态不是 enabled（暂停或禁用），返回 loop_paused 错误
        if loop_.status != "enabled" {
            tracing::warn!("[debounce] loop {} is not enabled (status={})", loop_id, loop_.status);
            return Err(Some("环路未开启".to_string()));
        }

        // 构建 trigger_meta
        let meta = serde_json::json!({
            "source": "default_response",
            "message": message,
        });

        // 通过 LoopRunner 触发环路执行
        let Some(runner) = loop_runner else {
            tracing::error!("[debounce] loop_runner not available");
            return Err(None);
        };

        // spawn_run 消费 Arc<Self>，runner 后续不再使用，直接 move 而非 clone
        let execution_id = runner.spawn_run(
            loop_id,
            None, // trigger_id
            "default_response",
            meta,
            feishu_bot_id,
            feishu_receive_id,
            feishu_receive_id_type,
        );

        if execution_id < 0 {
            tracing::error!("[debounce] loop_runner.spawn_run failed for loop {}", loop_id);
            return Err(None);
        }

        tracing::info!(
            "[debounce] triggered loop {} as default response, execution_id={}",
            loop_id,
            execution_id
        );

        Ok(crate::executor_service::ExecutionResult {
            task_id: format!("loop-{}", execution_id),
            record_id: Some(execution_id),
        })
    }

    /// 处理默认响应类型为 executor 的情况
    /// 直接调用执行器进行交互，不创建执行记录
    #[allow(clippy::too_many_arguments)] // 参数来自上游 handler 的独立数据源，合并为 struct 增加认知负担
    async fn handle_default_response_executor(
        db: &Arc<Database>,
        executor_registry: &Arc<crate::adapters::ExecutorRegistry>,
        _task_manager: &Arc<TaskManager>,
        config: &Arc<std::sync::RwLock<crate::config::Config>>,
        tx: &broadcast::Sender<ExecEvent>,
        bot_id: i64,
        receive_id: String,
        receive_id_type: &str,
        executor_type: Option<&str>,
        workspace_id: Option<i64>,
        message: &str,
        resume_session_id: Option<String>,
    ) -> Result<crate::executor_service::ExecutionResult, Option<String>> {
        let executor_type = executor_type.unwrap_or("claudecode");

        // 统一的飞书回复出口：开始/结束/错误三类消息都走 DirectCardMessage，
        // FeishuPushService 会绕过 workspace 过滤直接 send_raw 发回用户（feishu_push.rs:65）。
        // 每次 clone receive_id：原来函数末尾一次性 move，改成多次 clone 语义不变，
        // 换来能在多个分支复用同一个发送出口。
        let send_msg = |content: String| {
            let _ = tx.send(ExecEvent::DirectCardMessage {
                bot_id,
                receive_id: receive_id.clone(),
                receive_id_type: receive_id_type.to_string(),
                content,
            });
        };

        // 获取工作空间路径
        let workspace_path = if let Some(wid) = workspace_id {
            match db.get_project_directory_by_id(wid).await {
                Ok(Some(pd)) => pd.path,
                Ok(None) => {
                    tracing::warn!("[debounce] workspace {} not found", wid);
                    send_msg(executor_error_message(executor_type, &format!("工作空间 {} 不存在", wid)));
                    return Err(None);
                }
                Err(e) => {
                    tracing::error!("[debounce] failed to get workspace {}: {}", wid, e);
                    send_msg(executor_error_message(executor_type, &format!("读取工作空间失败：{}", e)));
                    return Err(None);
                }
            }
        } else {
            tracing::warn!("[debounce] no workspace_id for executor default response");
            send_msg(executor_error_message(executor_type, "未配置工作空间"));
            return Err(None);
        };

        // 获取执行器
        let exec_type = match parse_executor_type(executor_type) {
            Some(t) => t,
            None => {
                tracing::warn!("[debounce] unknown executor type: {}", executor_type);
                send_msg(executor_error_message(executor_type, &format!("未知执行器类型：{}", executor_type)));
                return Err(None);
            }
        };
        let executor = match executor_registry.get(exec_type).await {
            Some(e) => e,
            None => {
                tracing::warn!("[debounce] executor {} not found", executor_type);
                send_msg(executor_error_message(executor_type, "执行器未注册"));
                return Err(None);
            }
        };

        // 确定本次使用的 session_id：
        // 1. 优先使用调用方传入的 resume_session_id（如绑定 todo 场景）
        // 2. 否则从 workspace 的 executor_sessions 中读取（私聊多轮对话场景）
        let mut session_id = resume_session_id.clone();
        if session_id.is_none() {
            if let Some(wid) = workspace_id {
                match db.get_executor_session(wid, executor_type).await {
                    Ok(Some(Some(sid))) => {
                        session_id = Some(sid);
                        tracing::info!(
                            "[debounce] resumed executor session for {}: {:?}",
                            executor_type,
                            session_id
                        );
                    }
                    Ok(Some(None)) => {
                        tracing::debug!("[debounce] no saved session for executor {}", executor_type);
                    }
                    Ok(None) => {
                        tracing::debug!("[debounce] workspace not found for session lookup");
                    }
                    Err(e) => {
                        tracing::warn!("[debounce] failed to get executor session: {}", e);
                    }
                }
            }
        }

        tracing::info!(
            "[debounce] executor {} direct response in workspace {:?}, message len={}, session={:?}",
            executor_type,
            workspace_path,
            message.len(),
            session_id.as_deref().map(|s| s.chars().take(20).collect::<String>())
        );

        // 构建执行器命令（带 session_id，支持 resume 多轮对话）
        let is_resume = session_id.is_some();
        let command_args = executor.command_args_with_session(message, session_id.as_deref(), is_resume);
        let program = executor.executable_path();
        tracing::info!(
            "[debounce] spawning: {} {:?} (cwd={:?})",
            program, command_args, workspace_path
        );
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(&command_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped())
            .current_dir(&workspace_path);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("[debounce] failed to spawn executor {}: {}", executor_type, e);
                send_msg(executor_error_message(executor_type, &format!("启动进程失败：{}", e)));
                return Err(None);
            }
        };

        // 提取 stdout 句柄用于流式读取：与 todo 执行路径一致，逐行解析并通过
        // EventPipeline 发送 ExecEvent::Output 事件，让 FeishuPushService 按 push_level 推送。
        let stdout_handle = child.stdout.take();
        // 提取 stderr 句柄：非零退出时把 stderr 内容带进错误消息
        let stderr_handle = child.stderr.take();

        // 为流式 Output 事件生成 task_id（提前生成，供 streaming reader 和返回值共用）
        let task_id = format!("executor-{}-{}", executor_type, uuid::Uuid::new_v4());

        // spawn 成功立即发"开始处理"标志，让飞书侧知道请求已被接收并在跑
        send_msg(executor_start_message(executor_type, message));

        // stdin 处理：take() 并 drop 关闭 stdin，避免 CLI 进程挂起等待 EOF。
        // 注意：这里不写 stdin payload——debounce 流式路径不是 worktree 场景，
        // pi 的 "y" 应答只在切换 worktree 目录时才有意义，乱写会污染 stdin。
        if child.stdin.take().is_some() {
            tracing::debug!("[debounce] stdin 已关闭");
        }

        // 启动流式读取任务：并发读取 stdout 并通过 EventPipeline 发送 Output 事件。
        // 复用 log_capture 的 pipeline 创建和解析逻辑，确保与 todo 执行产生完全相同格式的事件。
        // FeishuPushService 按 push_level 配置推送（"all" 时推送过程事件到飞书）。
        let tx_for_stream = tx.clone();
        let executor_for_stream = executor.clone();
        let tid_for_stream = task_id.clone();
        // 一对一私聊场景：先查一次 push_level，仅当配置为 "all" 时才发送过程消息。
        // 在发送端判断而非 FeishuPushService 端判断，避免每条日志都查一次 DB。
        let push_level = match db.get_feishu_push_target(bot_id).await {
            Ok(Some(target)) => target.push_level,
            Ok(None) => "result_only".to_string(),
            Err(e) => {
                tracing::warn!("[debounce] failed to get push_level for bot {}: {}", bot_id, e);
                "result_only".to_string()
            }
        };
        let direct_output_info = if push_level == "all" {
            Some(DirectOutputInfo {
                bot_id,
                receive_id: receive_id.clone(),
                receive_id_type: receive_id_type.to_string(),
            })
        } else {
            None
        };
        let stream_task = tokio::spawn(async move {
            stream_executor_stdout(
                stdout_handle, &executor_for_stream, &tx_for_stream, &tid_for_stream,
                workspace_id, direct_output_info,
            ).await
        });

        // 带超时地等待子进程退出（stdout 已被流式读取，此处只管 wait + timeout + kill）
        let timeout_secs = {
            #[allow(clippy::expect_used)]
            let cfg = config
                .read()
                .expect("config RwLock poisoned in handle_default_response_executor");
            cfg.execution_timeout_secs
        };
        let timeout = direct_executor_timeout(timeout_secs);
        let status = match wait_child_with_timeout(child, timeout).await {
            Ok(s) => {
                tracing::info!("[debounce] executor {} finished, exit_code={:?}", executor_type, s.code());
                s
            }
            Err(ExecutorRunError::Timeout { secs }) => {
                tracing::error!("[debounce] executor {} timed out after {}s", executor_type, secs);
                send_msg(executor_error_message(executor_type, &format!("执行超时（{}s）", secs)));
                return Err(None);
            }
            Err(ExecutorRunError::WaitFailed(msg)) => {
                tracing::error!("[debounce] failed to wait for executor {}: {}", executor_type, msg);
                send_msg(executor_error_message(executor_type, &format!("等待进程失败：{}", msg)));
                return Err(None);
            }
        };

        // 等待流式读取任务完成，收集解析结果和原始 stdout
        let stream_result = stream_task.await.unwrap_or(StdoutStreamResult {
            logs: Vec::new(),
            raw_stdout: String::new(),
        });

        // 读取 stderr：非零退出时用于诊断失败原因
        let stderr_text = match stderr_handle {
            Some(mut reader) => {
                let mut buf = Vec::new();
                let _ = reader.read_to_end(&mut buf).await;
                String::from_utf8_lossy(&buf).to_string()
            }
            None => String::new(),
        };
        if !stderr_text.is_empty() {
            // 按 char 截断日志预览，避免按字节切片切断多字节中文导致 panic
            let stderr_preview: String = stderr_text.chars().take(2000).collect();
            tracing::info!(
                "[debounce] executor {} stderr:\n{}",
                executor_type,
                stderr_preview
            );
        }

        // 从流式解析收集的日志中提取最终结果（与 todo 执行路径一致）
        // 按 char 截断日志预览，避免按字节切片切断多字节中文导致 panic
        let stdout_preview: String = stream_result.raw_stdout.chars().take(2000).collect();
        tracing::info!(
            "[debounce] executor {} stdout:\n{}",
            executor_type,
            stdout_preview
        );
        let result_text = crate::executor_service::completion::get_final_result_from_logs(&stream_result.logs);

        // 构建结束消息：成功有解析结果就用结果；无结果但退出 0 且有 stdout 就用 stdout 兜底；
        // 非 0 退出走错误通道带退出码+输出片段
        let content = build_executor_end_content(
            executor_type, &status, result_text, &stream_result.raw_stdout, &stderr_text,
        );

        tracing::info!(
            "[debounce] executor {} result_text={:?}",
            executor_type,
            content.chars().take(200).collect::<String>()
        );
        // 注释掉发送给飞书：最后一条 assistant 消息已经是结论，FeiShu 推送会导致重复
        // send_msg(content);

        // 执行成功时从日志中提取 session_id 并持久化到数据库，
        // 下次私聊时可直接 resume 该 session，实现多轮对话上下文保持。
        if status.success() {
            if let Some(wid) = workspace_id {
                if let Some(new_session_id) = extract_session_from_logs(&executor, &stream_result.logs) {
                    tracing::info!(
                        "[debounce] extracted session_id={} for executor={}, saving to DB",
                        new_session_id,
                        executor_type
                    );
                    if let Err(e) = db.set_executor_session(wid, executor_type, Some(new_session_id)).await {
                        tracing::warn!("[debounce] 保存 executor session 失败: {:?}", e);
                    }
                }
            }
        }

        Ok(crate::executor_service::ExecutionResult {
            task_id,
            record_id: None, // executor 类型不存储执行记录
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod merge_pending_messages_tests {
    //! 验证 debounce 窗口内多条消息的合并规则。`push` 把消息丢进 bucket,
    //! 定时器到期时再调 `merge_pending_messages` 合并成一段 ——
    //! 如果合并规则错了,Claude 收到的就是几段被错误拼接的脏文本。
    use super::{merge_pending_messages, PendingMessage};

    fn msg(content: &str) -> PendingMessage {
        PendingMessage {
            bot_id: 1,
            chat_id: "chat-1".to_string(),
            chat_type: "group".to_string(),
            sender: "user-1".to_string(),
            content: content.to_string(),
            todo_id: 42,
            todo_prompt: "stub".to_string(),
            executor: Some("claudecode".to_string()),
            trigger_type: "feishu".to_string(),
            params: None,
            message_id: None,
            resume_session_id: None,
            resume_message: None,
            binding_id: None,
            workspace_id: None,
            immediate: false,
        }
    }

    /// 单条消息: 合并后应该和原文一致,不应该被加上 `---` 之类装饰。
    /// 边界用例 —— 如果 join 永远会加 separator,空 list 都得返回 `---`,
    /// 显然不对。
    #[test]
    fn test_single_message_returns_content_unchanged() {
        let merged = merge_pending_messages(&[msg("hello world")]);
        assert_eq!(merged, "hello world");
    }

    /// 两条消息: 中间用 `\n---\n` 分隔。这是飞书用户连续发"前一行/后一行"
    /// 时 AI 收到的样子。
    #[test]
    fn test_two_messages_joined_with_separator() {
        let merged = merge_pending_messages(&[msg("line A"), msg("line B")]);
        assert_eq!(merged, "line A\n---\nline B");
    }

    /// 任意 N 条都按顺序 join,顺序必须保持稳定 —— AI 会把它当成时间序列读。
    #[test]
    fn test_many_messages_preserve_order() {
        let merged = merge_pending_messages(&[
            msg("first"),
            msg("second"),
            msg("third"),
            msg("fourth"),
        ]);
        assert_eq!(merged, "first\n---\nsecond\n---\nthird\n---\nfourth");
    }

    /// 空切片: 返回空串。这是 `pending_count` 防御 if-empty 的上游,
    /// 如果空切片返回 `---`,filter 那行能放行但 Claude 会收到莫名其妙的
    /// 标点符号。
    #[test]
    fn test_empty_slice_returns_empty_string() {
        let merged = merge_pending_messages(&[]);
        assert_eq!(merged, "");
    }

    /// 消息内的换行不参与合并规则 —— 内部换行应该原样保留,只在消息之间
    /// 插入 `---`。这跟 `replace_placeholders` 的占位符替换是配套的:
    /// 用户的多行消息在 `{{message}}` 位置原样展开。
    #[test]
    fn test_internal_newlines_preserved_verbatim() {
        let merged = merge_pending_messages(&[
            msg("line 1\nline 2"),
            msg("single line"),
        ]);
        assert_eq!(merged, "line 1\nline 2\n---\nsingle line");
    }
}

/// 飞书默认响应执行器的反馈消息格式化与超时运行逻辑测试。
///
/// 这组测试覆盖 `handle_default_response_executor` 里抽出来的纯逻辑：
/// 三类飞书反馈消息（开始/错误/空结束）的格式，以及带超时地运行子进程
/// 并在超时时 kill 回收的行为。把 I/O 隔在 `handle_default_response_executor`
/// 主体里，这里只测可复现的纯函数 + 用 `echo`/`sleep` 真进程验证超时分支。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod executor_feedback_tests {
    use super::*;

    /// 短消息：开始标志应包含执行器名 + 原文 preview，前缀 ⏳ 与全仓 status_icon 约定一致。
    #[test]
    fn test_executor_start_message_basic() {
        let msg = executor_start_message("pi", "你叫啥");
        assert_eq!(msg, "⏳ pi 开始处理：你叫啥");
    }

    /// 长 preview 必须在 30 字（按 Unicode scalar，不切断多字节中文）处截断，
    /// 避免把整段用户 prompt 原样刷到飞书会话里造成噪声。
    #[test]
    fn test_executor_start_message_truncates_long() {
        let long = "一二三四五六七八九十一二三四五六七八九十一二三四五六七八九十多余";
        let msg = executor_start_message("pi", long);
        // 前 30 个 char + 省略号，提示用户内容被裁了
        let expected_preview: String = long.chars().take(30).collect();
        assert_eq!(msg, format!("⏳ pi 开始处理：{}…", expected_preview));
    }

    /// 错误消息格式：前缀 ❌ + 执行器名 + 原因，原因原样透传（含超时秒数、wait 错误等）。
    #[test]
    fn test_executor_error_message_format() {
        let msg = executor_error_message("pi", "执行超时（300s）");
        assert_eq!(msg, "❌ pi 执行失败：执行超时（300s）");
    }

    /// 成功但无输出时的结束标志，让用户知道执行跑完了只是没产出文本，
    /// 而不是静默无响应（这正是本次要消除的"静默失败"体验）。
    #[test]
    fn test_executor_empty_end_message() {
        let msg = executor_empty_end_message("pi");
        assert_eq!(msg, "✅ pi 执行完成（无输出）");
    }

    /// 成功路径：`echo hi` 应在超时内退出，stdout 含 `hi`。
    /// 用 `sh -c` 包一层保证跨平台（macOS/Linux 都有 sh）。
    #[tokio::test]
    async fn test_run_executor_with_timeout_success() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "echo hi"]).stdout(std::process::Stdio::piped());
        let child = cmd.spawn().expect("spawn sh echo");
        let (status, stdout) = run_executor_with_timeout(child, Some(std::time::Duration::from_secs(5)))
            .await
            .expect("echo should succeed within timeout");
        assert!(status.success(), "echo should exit 0");
        let out = String::from_utf8_lossy(&stdout);
        assert!(out.contains("hi"), "stdout should contain hi, got: {}", out);
    }

    /// 超时路径：`sleep 30` 在 1s 超时后应被 kill，返回 Timeout 而不是挂起整个测试。
    /// 这是本次修复的核心——挂起的执行器不再永久卡死 debounce 任务。
    #[tokio::test]
    async fn test_run_executor_with_timeout_kills_on_timeout() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "sleep 30"]).stdout(std::process::Stdio::piped());
        let child = cmd.spawn().expect("spawn sh sleep");
        let start = std::time::Instant::now();
        let err = run_executor_with_timeout(child, Some(std::time::Duration::from_secs(1)))
            .await
            .expect_err("sleep 30 should time out");
        // 超时分支应在略超 1s 处返回，而不是等满 30s
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "should return shortly after timeout");
        match err {
            ExecutorRunError::Timeout { secs } => assert_eq!(secs, 1),
            other => panic!("expected Timeout, got {:?}", other),
        }
    }

    /// 无超时路径：`execution_timeout_secs == 0` 表示「不限制」，传 `None` 时
    /// `echo hi` 应正常退出且 stdout 含 `hi`，不能因为没设超时就把进程卡死或判死。
    /// 这是 `0 = 不限制` 语义在直连执行器路径的回归保护。
    #[tokio::test]
    async fn test_run_executor_with_timeout_no_timeout_completes() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "echo hi"]).stdout(std::process::Stdio::piped());
        let child = cmd.spawn().expect("spawn sh echo");
        let (status, stdout) = run_executor_with_timeout(child, None)
            .await
            .expect("echo should complete when timeout is disabled");
        assert!(status.success(), "echo should exit 0 even without timeout");
        let out = String::from_utf8_lossy(&stdout);
        assert!(out.contains("hi"), "stdout should contain hi, got: {}", out);
    }

    /// `0` 必须解析成 `None`：`tokio::time::timeout` 吃 0 秒会立刻 Elapsed，
    /// 0 走 None 分支才能正确表达「不限制」语义。
    #[test]
    fn test_direct_executor_timeout_zero_is_none() {
        assert!(direct_executor_timeout(0).is_none(), "0 must map to None (no timeout)");
    }

    /// 正值必须解析成 `Some(Duration)`，且秒数原样透传。
    #[test]
    fn test_direct_executor_timeout_positive_is_some() {
        let d = direct_executor_timeout(3600).expect("positive secs must map to Some");
        assert_eq!(d.as_secs(), 3600);
    }

    /// 成功时有解析结果：统一返回简洁结束标志，result_text 已通过过程消息实时推送。
    #[test]
    fn test_build_executor_end_content_success_with_result_text() {
        let status = std::process::Command::new("sh")
            .args(["-c", "true"])
            .status()
            .expect("spawn sh true");
        let content = build_executor_end_content("pi", &status, Some("最终答案".to_string()), "ignored stdout", "");
        assert_eq!(content, "✅ pi 处理完成");
    }

    /// 退出 0 + 有原始 stdout：统一返回简洁结束标志，stdout 已通过过程消息实时推送。
    #[test]
    fn test_build_executor_end_content_success_returns_marker() {
        let status = std::process::Command::new("sh")
            .args(["-c", "true"])
            .status()
            .expect("spawn sh true");
        let content = build_executor_end_content("pi", &status, None, "hello world", "");
        assert_eq!(content, "✅ pi 处理完成");
    }

    /// 退出 0 + stdout 为空：统一返回简洁结束标志，与有输出时保持一致。
    #[test]
    fn test_build_executor_end_content_success_empty_returns_marker() {
        let status = std::process::Command::new("sh")
            .args(["-c", "true"])
            .status()
            .expect("spawn sh true");
        let content = build_executor_end_content("pi", &status, None, "   \n  ", "");
        assert_eq!(content, "✅ pi 处理完成");
    }

    /// 非零退出 + 远超上限的多字节中文 stderr：必须按 char 截断前 1500 个，
    /// 不能用 `&str[..1500]` 按字节切片——那会落在中文中间触发 panic。
    /// 这是本次修复的核心回归保护：执行器非零退出不能再变成 debounce 任务崩溃。
    #[test]
    fn test_build_executor_end_content_nonzero_chinese_no_panic() {
        // 1600 个全中文字符，确保命中截断分支
        let chinese = "执行出错".repeat(400);
        let status = std::process::Command::new("sh")
            .args(["-c", "exit 1"])
            .status()
            .expect("spawn sh exit 1");
        // 关键断言：这一行不 panic 即说明按 char 截断生效
        let content = build_executor_end_content("pi", &status, None, "", &chinese);
        assert!(
            content.starts_with("❌ pi 执行失败：退出码 Some(1)"),
            "should start with error prefix, got: {}",
            content
        );
        // 输出区被裁到 1500 个 char：总长度远小于原文 1600 + 前缀
        assert!(
            content.chars().count() < 1600,
            "stderr preview should be truncated to 1500 chars, got len {}",
            content.chars().count()
        );
    }
}

/// p2p 串行队列相关纯逻辑测试。
///
/// 覆盖三块关键修复：
/// - feishu_reply_target：群聊/私聊回复目标路由（Task 4 抽取的 helper）
/// - P2pRunningGuard：Drop 时清除 running 标记（Task 1 修复的核心，防 panic 死锁）
/// - pop_or_drain_queue：取出/空清理/重试三态（Task 2 修复的核心，防 remove 竞态丢消息）
///
/// 这三块都是纯逻辑/纯函数，不依赖 DB，可独立验证。push 的整体入队/drain 流程
/// 涉及 ServiceContext + Database + executor，属集成测试范畴，不在此覆盖。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod p2p_queue_tests {
    use super::*;
    use dashmap::DashMap;
    use std::collections::VecDeque;
    use std::sync::Arc;

    /// 构造私聊消息：chat_type=p2p，回复目标应为 sender。
    fn p2p_msg(content: &str, sender: &str) -> PendingMessage {
        PendingMessage {
            bot_id: 1,
            chat_id: format!("chat-{sender}"),
            chat_type: "p2p".to_string(),
            sender: sender.to_string(),
            content: content.to_string(),
            todo_id: 42,
            todo_prompt: "stub".to_string(),
            executor: Some("claudecode".to_string()),
            trigger_type: "feishu".to_string(),
            params: None,
            message_id: None,
            resume_session_id: None,
            resume_message: None,
            binding_id: None,
            workspace_id: Some(1),
            immediate: false,
        }
    }

    /// 构造群聊消息：chat_type=group，回复目标应为 chat_id。
    fn group_msg(content: &str, chat_id: &str) -> PendingMessage {
        PendingMessage {
            bot_id: 1,
            chat_id: chat_id.to_string(),
            chat_type: "group".to_string(),
            sender: "user-1".to_string(),
            content: content.to_string(),
            todo_id: 42,
            todo_prompt: "stub".to_string(),
            executor: Some("claudecode".to_string()),
            trigger_type: "feishu".to_string(),
            params: None,
            message_id: None,
            resume_session_id: None,
            resume_message: None,
            binding_id: None,
            workspace_id: Some(1),
            immediate: false,
        }
    }

    // ---- feishu_reply_target：回复目标路由 ----

    /// 群聊消息回复到群：返回 (chat_id, "chat_id")。
    /// 私聊与群聊的回复通道不同，路由错了消息会发到错误会话。
    #[test]
    fn test_feishu_reply_target_group_returns_chat_id() {
        let msg = group_msg("hi", "oc_abc");
        let (rid, rtype) = MessageDebounce::feishu_reply_target(&msg);
        assert_eq!(rid, "oc_abc");
        assert_eq!(rtype, "chat_id");
    }

    /// 私聊消息回复到个人：返回 (sender open_id, "open_id")。
    #[test]
    fn test_feishu_reply_target_p2p_returns_open_id() {
        let msg = p2p_msg("hi", "ou_sender");
        let (rid, rtype) = MessageDebounce::feishu_reply_target(&msg);
        assert_eq!(rid, "ou_sender");
        assert_eq!(rtype, "open_id");
    }

    // ---- P2pRunningGuard：Drop 清理 running 标记 ----

    /// guard drop 后清除 running 标记。
    /// 这是 Task 1 修复的核心：guard 必须在首次执行前创建、覆盖整个处理过程，
    /// 这样即使 panic，drop 也会执行、清除标记，避免该维度永久死锁。
    /// 此处直接验证 drop 的清理行为（panic 安全性的基础）。
    #[test]
    fn test_p2p_running_guard_drop_clears_marker() {
        let running: Arc<DashMap<P2pQueueKey, ()>> = Arc::new(DashMap::new());
        let key: P2pQueueKey = (1, 1, "ou_sender".to_string());
        // 模拟 push 设置 running 标记（push 用 insert 原子标记）
        running.insert(key.clone(), ());
        assert!(running.contains_key(&key), "running 标记应已设置");

        // guard 存活期间标记保留；drop 后标记清除
        {
            let _guard = P2pRunningGuard { key: key.clone(), running: running.clone() };
            assert!(running.contains_key(&key), "guard 存活期间标记应保留");
        } // guard 在此 drop

        assert!(
            !running.contains_key(&key),
            "guard drop 后标记必须清除，否则 panic 时该维度会永久死锁"
        );
    }

    /// guard 只清自己的 key，不误清其他维度的标记。
    /// 不同 sender/workspace 是独立队列，清理不能串。
    #[test]
    fn test_p2p_running_guard_only_clears_own_key() {
        let running: Arc<DashMap<P2pQueueKey, ()>> = Arc::new(DashMap::new());
        let key_a: P2pQueueKey = (1, 1, "a".to_string());
        let key_b: P2pQueueKey = (1, 1, "b".to_string());
        running.insert(key_a.clone(), ());
        running.insert(key_b.clone(), ());

        {
            let _guard = P2pRunningGuard { key: key_a.clone(), running: running.clone() };
        }

        assert!(!running.contains_key(&key_a), "key_a 应被清理");
        assert!(running.contains_key(&key_b), "key_b 不应受影响");
    }

    // ---- pop_or_drain_queue：取出 / 空清理 / 重试三态 ----

    /// 队列有消息时返回 Message 并移除队首（FIFO）。
    #[test]
    fn test_pop_or_drain_queue_returns_message_when_nonempty() {
        let queue: DashMap<P2pQueueKey, VecDeque<PendingMessage>> = DashMap::new();
        let key: P2pQueueKey = (1, 1, "ou".to_string());
        {
            let mut q = queue.entry(key.clone()).or_default();
            q.push_back(p2p_msg("first", "ou"));
            q.push_back(p2p_msg("second", "ou"));
        }

        match MessageDebounce::pop_or_drain_queue(&queue, &key) {
            QueuePop::Message(m) => assert_eq!(m.content, "first"),
            other => panic!("期望 Message，得到 {:?}", other),
        }
        // 取出一条后队列应剩 1 条
        assert_eq!(queue.get(&key).map(|q| q.len()), Some(1));
    }

    /// 队列为空时返回 Drained 并清理条目。
    /// 空条目若不清理会残留，长期运行下 active 用户的空 VecDeque 累积成内存泄漏。
    #[test]
    fn test_pop_or_drain_queue_drains_and_removes_empty_entry() {
        let queue: DashMap<P2pQueueKey, VecDeque<PendingMessage>> = DashMap::new();
        let key: P2pQueueKey = (1, 1, "ou".to_string());
        // 插入空队列（模拟 push 后消息被 pop 光的中间态）
        queue.entry(key.clone()).or_default();

        match MessageDebounce::pop_or_drain_queue(&queue, &key) {
            QueuePop::Drained => {}
            other => panic!("期望 Drained，得到 {:?}", other),
        }
        assert!(!queue.contains_key(&key), "空队列条目应被清理");
    }

    /// key 不存在时返回 Drained（防御，不 panic）。
    /// 正常流程不会发生（push 会 or_default 创建），但 drain 逻辑必须对缺失 key 健壮。
    #[test]
    fn test_pop_or_drain_queue_drains_when_key_absent() {
        let queue: DashMap<P2pQueueKey, VecDeque<PendingMessage>> = DashMap::new();
        let key: P2pQueueKey = (1, 1, "ou".to_string());

        match MessageDebounce::pop_or_drain_queue(&queue, &key) {
            QueuePop::Drained => {}
            other => panic!("key 不存在时应 Drained，得到 {:?}", other),
        }
    }

    /// 逐条 pop 直到 Drained：验证 FIFO 顺序 + 最终清理。
    #[test]
    fn test_pop_or_drain_queue_drains_in_fifo_order() {
        let queue: DashMap<P2pQueueKey, VecDeque<PendingMessage>> = DashMap::new();
        let key: P2pQueueKey = (1, 1, "ou".to_string());
        {
            let mut q = queue.entry(key.clone()).or_default();
            q.push_back(p2p_msg("first", "ou"));
            q.push_back(p2p_msg("second", "ou"));
            q.push_back(p2p_msg("third", "ou"));
        }

        // 按入队顺序取出
        let first = match MessageDebounce::pop_or_drain_queue(&queue, &key) {
            QueuePop::Message(m) => m.content,
            other => panic!("期望 Message，得到 {:?}", other),
        };
        let second = match MessageDebounce::pop_or_drain_queue(&queue, &key) {
            QueuePop::Message(m) => m.content,
            other => panic!("期望 Message，得到 {:?}", other),
        };
        let third = match MessageDebounce::pop_or_drain_queue(&queue, &key) {
            QueuePop::Message(m) => m.content,
            other => panic!("期望 Message，得到 {:?}", other),
        };
        assert_eq!((first.as_str(), second.as_str(), third.as_str()), ("first", "second", "third"));

        // 全部取完后应 Drained 且条目已清理
        match MessageDebounce::pop_or_drain_queue(&queue, &key) {
            QueuePop::Drained => {}
            other => panic!("取完后应 Drained，得到 {:?}", other),
        }
        assert!(!queue.contains_key(&key), "全部 drain 后条目应被清理");
    }
}
