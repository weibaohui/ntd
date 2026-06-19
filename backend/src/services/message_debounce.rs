use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::adapters::ExecutorRegistry;
use crate::config::Config;
use crate::db::Database;
use crate::executor_service::{ExecutionResult, RunTodoExecutionRequest};
use crate::handlers::{AppError, ExecEvent};
use crate::handlers::execution::start_todo_execution;
use crate::hooks::HookService;
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
}

struct DebounceEntry {
    messages: Vec<PendingMessage>,
    timer: JoinHandle<()>,
}

pub struct MessageDebounce {
    entries: Arc<DashMap<(i64, String), DebounceEntry>>,
    ctx: ServiceContext,
    /// 共享的 HookService 单例（来自 AppState）。
    ///
    /// debounce 触发的执行末段也要 fire 状态变更钩子。如果 debounce 在每次
    /// `new()` 时都重新 `Arc::new(HookService::new(...))` 会出现多份实例，
    /// 造成 hook 链路彼此看不见的问题（见 issue #509）。直接透传 AppState
    /// 里的单例即可。
    hook_service: Arc<HookService>,
}

impl MessageDebounce {
    pub fn new(ctx: ServiceContext, hook_service: Arc<HookService>) -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            ctx,
            hook_service,
        }
    }

    /// 移除旧 entry、abort 旧定时器，返回已有的消息列表。
    fn remove_old_entry(&self, key: &(i64, String)) -> Vec<PendingMessage> {
        self.entries
            .remove(key)
            .map(|(_, old)| { old.timer.abort(); old.messages })
            .unwrap_or_default()
    }

    /// 把 self 持有的 Arc 依赖打包成 DebounceTaskDeps，供定时器任务使用。
    fn collect_timer_deps(&self) -> DebounceTaskDeps {
        DebounceTaskDeps {
            entries: self.entries.clone(),
            db: self.ctx.db.clone(),
            executor_registry: self.ctx.executor_registry.clone(),
            tx: self.ctx.tx.clone(),
            task_manager: self.ctx.task_manager.clone(),
            config: self.ctx.config.clone(),
            hook_service: self.hook_service.clone(),
        }
    }

    /// 创建新的 debounce 定时器。到期后 drain 消息、构建执行上下文并执行。
    fn spawn_debounce_timer(&self, key: (i64, String), _messages: &[PendingMessage]) -> JoinHandle<()> {
        let deps = self.collect_timer_deps();
        let bot_id = key.0;
        let chat_id = key.1.clone();
        let target_type = _messages.first().map(|m| m.chat_type.clone()).unwrap_or_default();
        // 用独立 async 函数代替巨型闭包，便于阅读和测试
        tokio::spawn(async move {
            run_debounce_timer_task(deps, bot_id, chat_id, target_type).await;
        })
    }

    /// Push a message into the debounce buffer. Resets the timer for this key.
    pub fn push(&self, msg: PendingMessage) {
        let key = (msg.bot_id, msg.chat_id.clone());
        // 收集已有消息并 abort 旧定时器
        let mut all_msgs = self.remove_old_entry(&key);
        all_msgs.push(msg);
        // 新定时器到期后 drain 消息并执行
        let new_timer = self.spawn_debounce_timer(key.clone(), &all_msgs);
        self.entries.insert(key, DebounceEntry { messages: all_msgs, timer: new_timer });
    }

    pub fn pending_count(&self) -> usize {
        self.entries.iter().map(|e| e.messages.len()).sum()
    }
}

/// 定时器异步任务所需依赖的聚合。
/// 把 tokio::spawn 闭包捕获的 7 个 Arc 打包，避免函数签名过长。
struct DebounceTaskDeps {
    entries: Arc<DashMap<(i64, String), DebounceEntry>>,
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    hook_service: Arc<HookService>,
}

/// 从 pending 消息中提取执行所需的全部上下文。
/// 独立 struct 便于在 prepare / execute / handle 之间传递。
struct ExecutionContext {
    /// 发给 executor 的消息内容（resume 时含 system prompt，否则是原始 prompt）
    exec_message: String,
    /// 非 resume 时的参数（content + message + 用户自定义 params）
    params: Option<HashMap<String, String>>,
    /// 是否走 resume 路径（经过 TOCTOU 检查后的最终值）
    is_resume: bool,
    /// resume 路径下用于更新 binding 的 session_id
    sid_for_binding: Option<String>,
    /// 传给 start_todo_execution 的 resume_session_id
    resume_session_id: Option<String>,
    /// 传给 start_todo_execution 的 resume_message
    resume_message: Option<String>,
}

/// 定时器到期后的完整执行流程：等待 → drain → 构建上下文 → 执行 → 处理结果。
/// 从 tokio::spawn 闭包中提取为命名函数，职责：编排整个 debounce 触发链路。
async fn run_debounce_timer_task(
    deps: DebounceTaskDeps,
    bot_id: i64,
    chat_id: String,
    target_type: String,
) {
    // 等待 debounce 窗口过期
    wait_debounce_window(&deps.db, bot_id, &target_type).await;
    // 从 map 中 drain 消息
    let Some(entry) = drain_pending_entry(&deps.entries, bot_id, &chat_id) else {
        return;
    };
    // 构建执行上下文并执行
    execute_debounced_messages(&deps, &entry).await;
}

/// 等待 debounce 秒数（从 DB 读取，至少 1 秒）。
async fn wait_debounce_window(db: &Database, bot_id: i64, target_type: &str) {
    let secs = db.get_debounce_secs(bot_id, target_type).await.unwrap_or(20).max(1);
    tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await;
}

/// 从 map 中移除指定 key 的 entry，返回 entry（如果有且非空）。
fn drain_pending_entry(
    entries: &DashMap<(i64, String), DebounceEntry>,
    bot_id: i64,
    chat_id: &str,
) -> Option<DebounceEntry> {
    let key = (bot_id, chat_id.to_string());
    entries.remove(&key).map(|(_, entry)| entry).filter(|e| !e.messages.is_empty())
}

/// 对 drain 出来的消息执行：合并 → 构建上下文 → 调用执行 → 处理结果。
async fn execute_debounced_messages(deps: &DebounceTaskDeps, entry: &DebounceEntry) {
    let merged_content = merge_pending_messages(&entry.messages);
    let last = entry.messages.last().unwrap();
    let ctx = prepare_execution_context(&deps.db, last, &merged_content).await;
    let result = run_execution(deps, last, &ctx).await;
    log_execution_result(last, &result, entry.messages.len());
    handle_execution_result(result, last, &entry.messages, &deps.db, &ctx).await;
}

/// 防御 TOCTOU：检查 debounce 等待期间 binding 的 todo_id 是否发生了变化。
/// 如果变了，resume 路径需要降级为新执行，否则会发到错误的项目。
async fn check_binding_todo_changed(db: &Database, last: &PendingMessage) -> bool {
    let Some(binding_id) = last.binding_id else { return false; };
    let Ok(Some(binding)) = db.get_feishu_project_binding_by_id(binding_id).await else {
        return false;
    };
    if binding.todo_id != last.todo_id {
        tracing::warn!(
            "[debounce] binding {} todo_id changed ({} → {}), dropping resume",
            binding_id, last.todo_id, binding.todo_id
        );
        return true;
    }
    false
}

/// 根据 resume 状态构建发给 executor 的消息内容。
/// resume 时把用户消息内嵌到 system prompt；新执行时直接传 prompt 模板。
fn build_exec_message(last: &PendingMessage, merged_content: &str, is_resume: bool) -> String {
    if is_resume {
        last.todo_prompt.replace("{{message}}", merged_content)
    } else {
        last.todo_prompt.clone()
    }
}

/// 构建执行参数。resume 时不传 params（Claude 保留项目上下文）；
/// 新执行时把 merged_content 注入 content/message 字段。
fn build_exec_params(last: &PendingMessage, merged_content: &str, is_resume: bool) -> Option<HashMap<String, String>> {
    if is_resume {
        return None;
    }
    let mut params = last.params.clone().unwrap_or_default();
    params.insert("content".to_string(), merged_content.to_string());
    params.insert("message".to_string(), merged_content.to_string());
    Some(params)
}

/// 汇总 TOCTOU 检查 + 执行参数构建。
/// 独立出来便于单测：给定 last message + merged_content，验证输出的 ExecutionContext。
async fn prepare_execution_context(db: &Database, last: &PendingMessage, merged_content: &str) -> ExecutionContext {
    let mut resume_sid = last.resume_session_id.clone();
    // TOCTOU：等 debounce 期间 binding 可能被重绑到不同项目
    if resume_sid.is_some() && check_binding_todo_changed(db, last).await {
        resume_sid = None;
    }
    let is_resume = resume_sid.is_some();
    let sid_for_binding = resume_sid.clone();
    let exec_message = build_exec_message(last, merged_content, is_resume);
    let params = build_exec_params(last, merged_content, is_resume);
    ExecutionContext {
        exec_message, params, is_resume, sid_for_binding,
        resume_session_id: resume_sid,
        resume_message: last.resume_message.clone(),
    }
}

/// 构建 RunTodoExecutionRequest 并调用 start_todo_execution。
/// 把 request 构造从定时器闭包中分离，便于审查参数组装逻辑。
async fn run_execution(deps: &DebounceTaskDeps, last: &PendingMessage, ctx: &ExecutionContext) -> Result<ExecutionResult, AppError> {
    start_todo_execution(RunTodoExecutionRequest {
        db: deps.db.clone(),
        executor_registry: deps.executor_registry.clone(),
        tx: deps.tx.clone(),
        task_manager: deps.task_manager.clone(),
        config: deps.config.clone(),
        hook_service: deps.hook_service.clone(),
        todo_id: last.todo_id,
        message: ctx.exec_message.clone(),
        req_executor: last.executor.clone(),
        trigger_type: last.trigger_type.clone(),
        params: ctx.params.clone(),
        resume_session_id: ctx.resume_session_id.clone(),
        resume_message: ctx.resume_message.clone(),
        chain: vec![],
        source_todo_id: None,
        source_todo_title: None,
        source_hook_id: None,
        feishu_bot_id: if last.binding_id.is_some() { Some(last.bot_id) } else { None },
        feishu_receive_id: if last.binding_id.is_some() { Some(last.sender.clone()) } else { None },
    }).await
}

/// 记录执行结果的 debug 日志。
fn log_execution_result(last: &PendingMessage, result: &Result<ExecutionResult, AppError>, msg_count: usize) {
    let record_id = result.as_ref().ok().and_then(|r| r.record_id);
    tracing::debug!(
        "[debounce] timer fired for todo_id={}, msg_count={}, record_id={:?}",
        last.todo_id, msg_count, record_id
    );
}

/// 分发执行结果到成功/失败处理分支。
async fn handle_execution_result(
    result: Result<ExecutionResult, AppError>,
    last: &PendingMessage,
    messages: &[PendingMessage],
    db: &Database,
    ctx: &ExecutionContext,
) {
    match result {
        Ok(exec_result) => handle_execution_success(&exec_result, last, messages, db, ctx).await,
        Err(e) => handle_execution_failure(e, last, messages, db).await,
    }
}

/// 执行成功：更新 binding 状态为 RUNNING + 记录 session_id，标记消息已处理。
async fn handle_execution_success(
    exec_result: &ExecutionResult,
    last: &PendingMessage,
    messages: &[PendingMessage],
    db: &Database,
    ctx: &ExecutionContext,
) {
    // 更新 binding 状态
    update_binding_on_success(db, last, exec_result, ctx).await;
    // 标记所有 pending 消息为已处理
    mark_messages_processed(db, messages, last.todo_id, exec_result.record_id).await;
}

/// 根据执行路径（resume / 首次 / 无 record）更新 binding。
async fn update_binding_on_success(
    db: &Database,
    last: &PendingMessage,
    exec_result: &ExecutionResult,
    ctx: &ExecutionContext,
) {
    let Some(binding_id) = last.binding_id else { return; };
    match (ctx.is_resume, exec_result.record_id) {
        // resume 路径：保留原 session_id，更新 record_id + 状态
        (true, Some(rid)) => update_binding_resume(db, binding_id, ctx, rid).await,
        // 首次执行：从 execution record 读取真实 session_id 存入 binding
        (false, Some(rid)) => update_binding_first_exec(db, binding_id, rid).await,
        // record_id 缺失但仍需标记状态为 RUNNING
        (_, None) => update_binding_status_only(db, binding_id).await,
    }
}

/// Resume 路径：保留 sid_for_binding，更新 record_id 和状态。
async fn update_binding_resume(db: &Database, binding_id: i64, ctx: &ExecutionContext, rid: i64) {
    if let Err(e) = db.update_feishu_project_binding_session(
        binding_id, ctx.sid_for_binding.as_deref(), rid, crate::models::binding_status::RUNNING,
    ).await {
        tracing::warn!("[debounce] failed to update binding {} session on resume: {:?}", binding_id, e);
    }
}

/// 首次执行：从 execution record 读取 Claude Code 的真实 session_id。
async fn update_binding_first_exec(db: &Database, binding_id: i64, rid: i64) {
    let real_sid = db.get_execution_record(rid).await
        .ok().flatten().and_then(|r| r.session_id);
    if let Err(e) = db.update_feishu_project_binding_session(
        binding_id, real_sid.as_deref(), rid, crate::models::binding_status::RUNNING,
    ).await {
        tracing::warn!("[debounce] failed to update binding {} session on first exec: {:?}", binding_id, e);
    }
}

/// record_id 缺失时仅更新 binding 状态为 RUNNING。
async fn update_binding_status_only(db: &Database, binding_id: i64) {
    if let Err(e) = db.update_feishu_project_binding_status(binding_id, crate::models::binding_status::RUNNING).await {
        tracing::warn!("[debounce] failed to update binding {} status: {:?}", binding_id, e);
    }
}

/// 遍历所有 pending 消息，标记为已处理（关联 todo_id 和 execution_record_id）。
async fn mark_messages_processed(db: &Database, messages: &[PendingMessage], todo_id: i64, record_id: Option<i64>) {
    for msg in messages {
        if let Some(ref msg_id) = msg.message_id {
            if let Err(e) = db.mark_feishu_message_processed(msg_id, todo_id, record_id).await {
                tracing::warn!("[debounce] failed to mark message {} as processed: {:?}", msg_id, e);
            }
        }
    }
}

/// 执行失败：重置 binding 为 IDLE，标记消息为失败以便重试。
async fn handle_execution_failure(
    error: AppError,
    last: &PendingMessage,
    messages: &[PendingMessage],
    db: &Database,
) {
    tracing::warn!("[debounce] failed to execute todo {}: {:?}", last.todo_id, error);
    // 重置 binding 为 idle，让下次消息能开新 session
    reset_binding_on_failure(db, last).await;
    // 标记消息为失败（processed=false），允许后续重试
    mark_messages_failed(db, messages).await;
}

/// 失败时重置 binding 状态为 IDLE。
async fn reset_binding_on_failure(db: &Database, last: &PendingMessage) {
    let Some(binding_id) = last.binding_id else { return; };
    let _ = db.update_feishu_project_binding_status(binding_id, crate::models::binding_status::IDLE).await;
}

/// 遍历所有 pending 消息，标记为失败。
async fn mark_messages_failed(db: &Database, messages: &[PendingMessage]) {
    for msg in messages {
        if let Some(ref msg_id) = msg.message_id {
            if let Err(e) = db.mark_feishu_message_failed(msg_id).await {
                tracing::warn!("[debounce] failed to mark message {} as failed: {:?}", msg_id, e);
            }
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

#[cfg(test)]
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
