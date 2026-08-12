//! 卡片回调路由 + act: 动作族 + 卡片 patch/历史分页（096-W3-PR4：从 feishu_listener.rs 拆分，函数体逐字搬迁零改动）。
//!
//! 设计依据：docs/design/100-FeishuListener拆分-设计.md（拆分边界与字段归属）。
//! 本族函数均为无 self 关联函数（接 context 参数对象或显式传参），
//! unit struct 仅作命名空间承载，不持有状态。

use crate::db::Database;
use crate::feishu::ChannelMessage;
use crate::services::feishu_api_client::FeishuApiClient;
use crate::services::feishu_card::{
    build_help_console_card, build_history_card, render_card, Card, CardElement, CardMarkdown,
    HistoryItem,
};
use crate::services::feishu_listener::FeishuListener;
use crate::services::feishu_listener::{
    ActionOutcome, CardAction, ListenerMessageContext,
};
use crate::services::feishu_slash_commands::SlashCommandHandler;

/// 见模块头注释。unit struct：仅作命名空间（函数全部关联函数形态）。
pub(crate) struct CardActionHandler;

impl CardActionHandler {
/// 把 started_at（ISO，如 "2026-07-11T14:04:37Z"）格式化成可读时间：取前 16 位 + T 换空格。
/// 不引入 chrono 依赖，精度足够卡片展示。
pub(crate) fn format_record_time(started_at: &str) -> String {
    started_at.get(..16).unwrap_or(started_at).replace('T', " ")
}

    /// 处理飞书卡片按钮点击回调。
    /// card_callback 消息的 content 是 action value，按前缀分三种处理：
    /// - `nav:/help <group>`：原地 patch 原卡片，切到对应分组；
    /// - `cmd:/<command>`：转成命令文本，复用 handle_message 分发链路执行，
    ///   等价于点击者在会话里发送了 `/<command>`；
    /// - `act:/<action>`：执行动作 + patch 刷新控制台。
    pub(crate) async fn handle_card_callback(context: ListenerMessageContext<'_>, msg: &ChannelMessage) {
        let action = msg.content.trim();

        // nav: 前缀 - 原地 patch 刷新控制台/历史页，拦截后直接返回。
        if CardActionHandler::handle_nav_action(&context, msg, action).await {
            return;
        }

        // cmd: 前缀 - 把卡片点击转成命令执行。
        // 构造一条虚拟命令消息复用 handle_message 的完整分发链路（内置命令 try_route_builtin_command
        // + 自定义规则 route_slash_or_default_response），与用户在会话里手动发送该命令效果一致。
        if let Some(cmd_text) = CardActionHandler::parse_card_command(action) {
            tracing::info!(
                "[feishu:{}] card cmd → redispatch as message: {:?}",
                context.bot_id, cmd_text
            );
            // chat_type 改成 p2p：避免 handle_message 又把这条消息当作 card_callback 递归处理；
            // sender/channel/id 沿用卡片回调，让命令处理函数的回复落到原会话、指向点击者。
            let mut cmd_msg = msg.clone();
            cmd_msg.content = cmd_text;
            cmd_msg.chat_type = Some("p2p".to_string());
            // handle_message → handle_card_callback → handle_message 是静态递归，
            // async fn 递归必须 Box::pin 引入间接层，否则 future 大小无限无法编译。
            // 运行时 cmd_msg.chat_type 已是 p2p，不会再进 card_callback 分支，实际只递归一层。
            Box::pin(FeishuListener::handle_message(context, &cmd_msg)).await;
            return;
        }

        // act: 前缀 - 执行动作（新会话/停止/设推送/切绑定/解绑）+ patch 刷新控制台。
        // 解析失败（未知 verb）落到下面的 unknown warn。
        if let Some(parsed) = CardActionHandler::parse_card_action(action) {
            CardActionHandler::execute_card_action(context, msg, parsed).await;
            return;
        }

        tracing::warn!("[feishu:{}] unknown card action: {}", context.bot_id, action);
    }

    /// 处理 nav: 前缀的卡片动作，返回 true 表示已处理。
    /// nav 动作是只读 patch 刷新，不产生副作用。
    pub(crate) async fn handle_nav_action(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, action: &str) -> bool {
        // nav:/help <group> 重查最新状态后刷控制台（运行状态可能已变）。
        if let Some(group) = action.strip_prefix("nav:/help ") {
            let group_key = group.trim().to_lowercase();
            let state = SlashCommandHandler::assemble_help_card_state(context.ctx, context.db, context.bot_id, &group_key, 1).await;
            let card = build_help_console_card(&state);
            CardActionHandler::patch_rendered_card(context, msg, &card).await;
            return true;
        }
        // nav:/history [page] - 分页查看执行历史。
        if let Some(page_arg) = action.strip_prefix("nav:/history") {
            let page = page_arg.trim().parse::<usize>().unwrap_or(1).max(1);
            CardActionHandler::patch_history_page(context, msg, page).await;
            return true;
        }
        // nav:/todos <page> - 事项分页（每页 10）。
        if let Some(page_arg) = action.strip_prefix("nav:/todos") {
            let page = page_arg.trim().parse::<usize>().unwrap_or(1).max(1);
            let state = SlashCommandHandler::assemble_help_card_state(context.ctx, context.db, context.bot_id, "todo", page).await;
            let card = build_help_console_card(&state);
            CardActionHandler::patch_rendered_card(context, msg, &card).await;
            return true;
        }
        // nav:/loops <page> - 环路分页（每页 10）。
        if let Some(page_arg) = action.strip_prefix("nav:/loops") {
            let page = page_arg.trim().parse::<usize>().unwrap_or(1).max(1);
            let state = SlashCommandHandler::assemble_help_card_state(context.ctx, context.db, context.bot_id, "loop", page).await;
            let card = build_help_console_card(&state);
            CardActionHandler::patch_rendered_card(context, msg, &card).await;
            return true;
        }
        false
    }

    /// 执行卡片 act 动作，执行后 patch 刷新控制台。
    pub(crate) async fn execute_card_action(context: ListenerMessageContext<'_>, msg: &ChannelMessage, action: CardAction) {
        let outcome = CardActionHandler::run_card_action(&context, msg, &action).await;
        let group = CardActionHandler::action_target_group(&action);
        CardActionHandler::patch_after_action(&context, msg, group, &outcome).await;
    }

    /// 按 CardAction 分发到具体执行函数。
    pub(crate) async fn run_card_action(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        action: &CardAction,
    ) -> ActionOutcome {
        match action {
            CardAction::Push(level) => CardActionHandler::act_push(context, level).await,
            CardAction::New => CardActionHandler::act_new(context).await,
            CardAction::Stop => CardActionHandler::act_stop(context).await,
            CardAction::Bind(workspace_id) => CardActionHandler::act_bind(context, *workspace_id).await,
            CardAction::RunTodo(todo_id) => CardActionHandler::act_run_todo(context, msg, *todo_id).await,
            CardAction::RunLoop(loop_id) => CardActionHandler::act_run_loop(context, msg, *loop_id).await,
            CardAction::SetExecutor(name) => CardActionHandler::act_set_executor(context, name).await,
        }
    }

    /// act 动作执行后刷新到的目标 Tab。
    pub(crate) fn action_target_group(action: &CardAction) -> &'static str {
        match action {
            CardAction::Bind(_) | CardAction::Push(_) | CardAction::SetExecutor(_) => "workspace",
            CardAction::RunTodo(_) => "todo",
            CardAction::RunLoop(_) => "loop",
            _ => "status",
        }
    }

    /// bot 所属 workspace 的默认执行器（如 dev 的 pi）。
    pub(crate) async fn workspace_default_executor(db: &Database, bot_id: i64) -> Option<String> {
        let wid = db.get_agent_bot_workspace_id(bot_id).await.ok().flatten()?;
        let settings = crate::db::workspace_setting::get_workspace_settings(db, wid).await.ok().flatten()?;
        settings.default_response_executor
    }

    /// auto-seed：确保 workspace 的 default_response_type=executor。
    /// 移除 binding 路径后 chat 全走 default_response，只 executor 分支可靠回复；切换 workspace 时兜底。
    pub(crate) async fn ensure_default_response_executor(db: &Database, workspace_id: i64) {
        let existing = crate::db::workspace_setting::get_workspace_settings(db, workspace_id).await.ok().flatten();
        let need_seed = existing.as_ref().map(|s| s.default_response_type != "executor").unwrap_or(true);
        if need_seed {
            // executor 用该 workspace 已配的（若有），否则 None（dispatch 时兜底 claudecode）
            let executor = existing.and_then(|s| s.default_response_executor);
            let _ = crate::db::workspace_setting::upsert_workspace_settings(
                db,
                workspace_id,
                Some("executor".to_string()),
                None,
                None,
                executor,
                // 飞书 listener 仅切 default_response，不动 workspace 共识 prompt
                None,
            )
            .await;
        }
    }

    /// 设置推送级别（直接设值，不走 /feishupush 循环）。
    pub(crate) async fn act_push(context: &ListenerMessageContext<'_>, level: &str) -> ActionOutcome {
        match context.db.update_feishu_push_level(context.bot_id, level).await {
            Ok(_) => ActionOutcome { success: true, message: format!("推送级别已更新为 {level}") },
            Err(e) => ActionOutcome { success: false, message: format!("设置失败：{e}") },
        }
    }

    /// 开启新会话：清当前 workspace 默认执行器的 session。
    pub(crate) async fn act_new(context: &ListenerMessageContext<'_>) -> ActionOutcome {
        let Some(wid) = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten() else {
            return ActionOutcome { success: false, message: "未设置工作空间".to_string() };
        };
        let executor = CardActionHandler::workspace_default_executor(context.db, context.bot_id)
            .await
            .unwrap_or_else(|| "claudecode".to_string());
        match context.db.set_executor_session(wid, &executor, None).await {
            Ok(_) => ActionOutcome { success: true, message: "已开启新会话".to_string() },
            Err(e) => ActionOutcome { success: false, message: format!("失败：{e}") },
        }
    }

    /// 停止当前 workspace 的运行任务（by workspace + ExecutionStatus::Running 直接查）。
    pub(crate) async fn act_stop(context: &ListenerMessageContext<'_>) -> ActionOutcome {
        let Some(wid) = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten() else {
            return ActionOutcome { success: false, message: "未设置工作空间".to_string() };
        };
        // 直接按 workspace 查运行中的记录，不依赖最近 N 条（避免旧记录淹没了 running 记录）
        let Ok(records) = context.db.get_running_records_by_workspace(wid).await else {
            return ActionOutcome { success: false, message: "查询失败".to_string() };
        };
        let Some(running) = records.into_iter().next() else {
            return ActionOutcome { success: false, message: "没有运行中的任务".to_string() };
        };
        let Some(task_id) = running.task_id.as_deref() else {
            return ActionOutcome { success: false, message: "任务缺少 task_id".to_string() };
        };
        if context.task_manager.cancel(task_id).await {
            ActionOutcome { success: true, message: "已发送停止信号，任务即将终止".to_string() }
        } else {
            let _ = context.db.force_fail_execution_record(running.id).await;
            ActionOutcome { success: true, message: "任务未在运行，已强制标记结束".to_string() }
        }
    }

    /// 切换工作空间：级联清旧 binding + 改 agent_bot.workspace_id + auto-seed default_response。
    pub(crate) async fn act_bind(context: &ListenerMessageContext<'_>, workspace_id: i64) -> ActionOutcome {
        let bot_id = context.bot_id;
        // 级联（对齐 move_bot_to_workspace）：删 pending binding / disable 旧 binding
        if let Ok(bindings) = context.db.get_feishu_project_bindings(bot_id).await {
            for b in bindings {
                if b.chat_id == crate::models::PENDING_CHAT_ID {
                    let _ = context.db.delete_feishu_project_binding(b.id).await;
                } else {
                    let _ = context.db.update_feishu_project_binding_enabled(b.id, false).await;
                }
            }
        }
        if let Err(e) = context.db.update_agent_bot_workspace_id(bot_id, workspace_id).await {
            return ActionOutcome { success: false, message: format!("切换工作空间失败：{e}") };
        }
        // auto-seed default_response_type=executor，确保切完后 chat 消息有回复
        CardActionHandler::ensure_default_response_executor(context.db, workspace_id).await;
        let name = context
            .db
            .get_workspace_name_by_id(workspace_id)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| format!("#{workspace_id}"));
        ActionOutcome { success: true, message: format!("已切换到工作空间「{name}」") }
    }

    /// 触发事项：后台跑该 todo，结果通过 ExecEvent::Finished → FeishuPushService 推回当前 chat。
    pub(crate) async fn act_run_todo(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, todo_id: i64) -> ActionOutcome {
        use crate::executor_service::{run_todo_execution, RunTodoExecutionRequest};
        let (receive_id, receive_id_type) = CardActionHandler::resolve_receive_target(msg);
        let workspace_id = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten();
        let todo = match context.db.get_todo(todo_id).await {
            Ok(Some(t)) => t,
            _ => return ActionOutcome { success: false, message: format!("事项 #{todo_id} 不存在") },
        };
        // 校验 todo 的 workspace_id 与 bot 当前 workspace 一致，防止旧卡片跨 workspace 执行
        if todo.workspace_id != workspace_id {
            return ActionOutcome {
                success: false,
                message: format!("事项 #{todo_id} 不属于当前工作空间，无法执行"),
            };
        }
        let title = todo.title.clone();
        let req = RunTodoExecutionRequest {
            db: context.db.clone(),
            executor_registry: context.ctx.executor_registry.clone(),
            tx: context.ctx.tx.clone(),
            task_manager: context.task_manager.clone(),
            config: context.ctx.config.clone(),
            blackboard_debouncer: context.ctx.blackboard_debouncer.clone(),
            todo_id,
            message: todo.prompt,
            req_executor: todo.executor,
            req_model: None,
            trigger_type: "feishu_card".to_string(),
            params: None,
            resume_session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,
            step_id: None,
            feishu_bot_id: Some(context.bot_id),
            feishu_receive_id: Some(receive_id.to_string()),
            feishu_receive_id_type: Some(receive_id_type.to_string()),
            workspace_path: None,
            workspace_id,
            // 飞书卡片触发路径：注入专家上下文，卡片触发也需尊重 todo 的专家绑定
            expert_manager: Some(context.ctx.expert_manager.clone()),
        };
        // fire-and-forget：后台执行不阻塞卡片 patch；结果由推送通道发回 chat
        tokio::spawn(async move {
            let _ = run_todo_execution(req).await;
        });
        ActionOutcome { success: true, message: format!("已触发事项「{title}」") }
    }

    /// 触发环路：LoopRunner::spawn_run 后台执行，整环结束推回 chat。
    pub(crate) async fn act_run_loop(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, loop_id: i64) -> ActionOutcome {
        let Some(runner) = context.debounce.loop_runner() else {
            return ActionOutcome { success: false, message: "环路执行器未就绪".to_string() };
        };
        let (receive_id, receive_id_type) = CardActionHandler::resolve_receive_target(msg);
        let loop_ = match context.db.get_loop(loop_id).await {
            Ok(Some(l)) => l,
            _ => return ActionOutcome { success: false, message: format!("环路 #{loop_id} 不存在") },
        };
        // 校验 loop 的 workspace_id 与 bot 当前 workspace 一致，防止旧卡片跨 workspace 执行
        let workspace_id = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten();
        if loop_.workspace_id != workspace_id {
            return ActionOutcome {
                success: false,
                message: format!("环路 #{loop_id} 不属于当前工作空间，无法执行"),
            };
        }
        let name = loop_.name;
        runner.clone().spawn_run(
            loop_id,
            None,
            "feishu_card",
            serde_json::json!({}),
            Some(context.bot_id),
            Some(receive_id.to_string()),
            Some(receive_id_type.to_string()),
        );
        ActionOutcome { success: true, message: format!("已触发环路「{name}」") }
    }

    /// 把当前 workspace 的「默认响应执行器」设为指定 executor：
    /// 既写 workspace_settings.default_response_executor，又把 default_response_type 改为 "executor"。
    /// 只写 executor 字段而不改 type 是有 bug 的——dispatch_default_response 按 default_response_type
    /// 分发，type 仍是 todo/loop 时点了「默认响应执行器」按钮也不会真的切到执行器，所以两字段必须同改。
    /// executor 名必须是已注册的（ExecutorType::as_str），否则视为无效拒绝写入。
    pub(crate) async fn act_set_executor(context: &ListenerMessageContext<'_>, executor_name: &str) -> ActionOutcome {
        let Some(wid) = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten() else {
            return ActionOutcome { success: false, message: "未设置工作空间".to_string() };
        };
        // 校验 executor 已注册，避免把无效名写进 settings 让下次 dispatch 失败
        let registered: Vec<String> = CardActionHandler::registered_executor_names(context).await;
        if !registered.iter().any(|s| s == executor_name) {
            return ActionOutcome {
                success: false,
                message: format!("执行器 {executor_name} 未注册（可用：{}）", registered.join(", ")),
            };
        }
        // type 与 executor 同改：type=executor 让 dispatch 走执行器分支，executor 字段指定具体执行器。
        // todo_id/loop_id 传 None 表示不动旧值——切回 todo/loop 时这些旧值还有用，不在这里清。
        match crate::db::workspace_setting::upsert_workspace_settings(
            context.db,
            wid,
            Some("executor".to_string()),
            None,
            None,
            Some(executor_name.to_string()),
            // 仅切默认执行器，不动 workspace 共识 prompt
            None,
        )
        .await
        {
            Ok(_) => ActionOutcome { success: true, message: format!("默认响应执行器已设为 {executor_name}") },
            Err(e) => ActionOutcome { success: false, message: format!("设置失败：{e}") },
        }
    }

    /// 从 executor_registry 拉出所有已注册执行器名（ExecutorType::as_str），返回 Vec<String>。
    /// 抽出来一是让 act_set_executor 更短，二是注册名查询本身也是可复用的小步骤。
    pub(crate) async fn registered_executor_names(context: &ListenerMessageContext<'_>) -> Vec<String> {
        context
            .ctx
            .executor_registry
            .list_executors()
            .await
            .into_iter()
            .map(|t| t.as_str().to_string())
            .collect()
    }

    /// act 执行后 patch 刷新控制台：assemble 最新状态 + 顶部插入操作结果提示。
    pub(crate) async fn patch_after_action(
        context: &ListenerMessageContext<'_>,
        msg: &ChannelMessage,
        group: &str,
        outcome: &ActionOutcome,
    ) {
        let state = SlashCommandHandler::assemble_help_card_state(context.ctx, context.db, context.bot_id, group, 1).await;
        let mut card = build_help_console_card(&state);
        let icon = if outcome.success { "✅" } else { "⚠️" };
        let tip = CardElement::Markdown(CardMarkdown { content: format!("{icon} {}", outcome.message) });
        card.elements.insert(0, tip);
        CardActionHandler::patch_rendered_card(context, msg, &card).await;
    }

    /// 渲染卡片并 patch 到原消息（nav/act 刷新共用）。
    pub(crate) async fn patch_rendered_card(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, card: &Card) {
        let session_key = format!("feishu:{}", msg.sender);
        let card_json = render_card(card, &session_key);
        if let Err(e) = FeishuApiClient::patch_card(context.credentials, context.token_manager, context.bot_id, &msg.id, &card_json).await {
            tracing::warn!("[feishu:{}] patch card failed: {e}", context.bot_id);
        }
    }

    /// 历史子页：按当前 workspace 分页查执行记录后 patch。
    pub(crate) async fn patch_history_page(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, page: usize) {
        const PER_PAGE: i64 = 10;
        let offset = page.saturating_sub(1) as i64 * PER_PAGE;
        let (items, total) = CardActionHandler::query_history(context.db, context.bot_id, PER_PAGE, offset).await;
        let total_pages = (total.max(0) as usize).div_ceil(PER_PAGE as usize);
        let card = build_history_card(&items, page, total_pages.max(1));
        CardActionHandler::patch_rendered_card(context, msg, &card).await;
    }

    /// 按 bot 的 workspace 分页查执行记录 → HistoryItem + 总数。
    pub(crate) async fn query_history(db: &Database, bot_id: i64, limit: i64, offset: i64) -> (Vec<HistoryItem>, i64) {
        let Some(wid) = db.get_agent_bot_workspace_id(bot_id).await.ok().flatten() else {
            return (vec![], 0);
        };
        match db.get_execution_records_by_workspace(wid, limit, offset).await {
            Ok((records, total)) => (records.into_iter().map(CardActionHandler::record_to_history_item).collect(), total),
            Err(_) => (vec![], 0),
        }
    }

    /// ExecutionRecord → 历史子页列表项（状态 emoji + 标题 + 触发类型 + 时间）。
    pub(crate) fn record_to_history_item(r: crate::models::ExecutionRecord) -> HistoryItem {
        use crate::models::ExecutionStatus;
        let status_icon = match r.status {
            ExecutionStatus::Success => "✅",
            ExecutionStatus::Running => "⏳",
            ExecutionStatus::Failed => "❌",
        };
        HistoryItem {
            status_icon: status_icon.to_string(),
            title: r.source_todo_title.clone().unwrap_or_else(|| r.command.clone()),
            trigger: r.trigger_type,
            time_desc: CardActionHandler::format_record_time(&r.started_at),
        }
    }

    /// 解析卡片回调 action 里的命令文本，供 handle_card_callback 的 cmd: 分支使用。
    /// `cmd:/new` → `Some("/new")`；`cmd:/bind foo` → `Some("/bind foo")`（保留参数）；
    /// 非 `cmd:/` 前缀（nav:/act:/未知/空）→ None。
    /// 抽成纯函数便于单测命令文本拼装，也让 handle_card_callback 的 cmd: 分支保持简洁。
    pub(crate) fn parse_card_command(action: &str) -> Option<String> {
        action.strip_prefix("cmd:/").map(|cmd| format!("/{}", cmd))
    }

    /// 解析卡片 act:/ 动作字符串为 CardAction。
    /// "act:/stop"→Stop；"act:/bind myapp"→Bind("myapp")；"act:/push result_only"→Push("result_only")。
    /// bind/push 需要参数，缺参数返回 None；未知 verb 返回 None。纯函数便于单测。
    pub(crate) fn parse_card_action(action: &str) -> Option<CardAction> {
        let rest = action.strip_prefix("act:/")?;
        // splitn(2) 让参数部分可含空格（虽然当前不会，但留余地），verb 与 arg 用首个空白分隔。
        let mut parts = rest.splitn(2, char::is_whitespace);
        let verb = parts.next()?.trim();
        let arg = parts.next().map(|s| s.trim()).filter(|s| !s.is_empty());
        Some(match verb {
            "stop" => CardAction::Stop,
            "new" => CardAction::New,
            "push" => CardAction::Push(arg?.to_string()),
            "setexecutor" => CardAction::SetExecutor(arg?.to_string()),
            "bind" => CardAction::Bind(arg?.parse().ok()?),
            "runtodo" => CardAction::RunTodo(arg?.parse().ok()?),
            "runloop" => CardAction::RunLoop(arg?.parse().ok()?),
            _ => return None,
        })
    }

    /// 把卡片回调消息解析成回信接收者 (receive_id, receive_id_type)。
    /// card_callback 的 chat_type 不是 p2p/group：msg.channel(chat_id)非空 → 群聊用 chat_id；
    /// 否则回退到点击者 open_id（私聊）。供 act:/runtodo、act:/runloop 等「回信给点击者」的动作复用。
    pub(crate) fn resolve_receive_target(msg: &ChannelMessage) -> (&str, &str) {
        if !msg.channel.is_empty() {
            (msg.channel.as_str(), "chat_id")
        } else {
            (msg.sender.as_str(), "open_id")
        }
    }

}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::CardActionHandler;

    #[test]
    pub(crate) fn test_parse_card_command_extracts_command_text() {
        // 无参命令：cmd:/new → /new
        assert_eq!(
            CardActionHandler::parse_card_command("cmd:/new"),
            Some("/new".to_string())
        );
        // 带参命令：参数原样保留，交由后续 parse_slash_command 解析
        assert_eq!(
            CardActionHandler::parse_card_command("cmd:/bind my-project"),
            Some("/bind my-project".to_string())
        );
    }

    #[test]
    pub(crate) fn test_parse_card_command_returns_none_for_non_cmd_prefix() {
        // nav:/act:/未知前缀/空串都不是命令点击，返回 None
        assert_eq!(CardActionHandler::parse_card_command("nav:/help common"), None);
        assert_eq!(CardActionHandler::parse_card_command("act:/delete-mode cancel"), None);
        assert_eq!(CardActionHandler::parse_card_command(""), None);
    }

    #[test]
    pub(crate) fn test_parse_card_action_variants() {
        use super::CardAction;
        // 各 verb 正常解析（bind/runtodo/runloop 参数是 i64）
        assert_eq!(CardActionHandler::parse_card_action("act:/stop"), Some(CardAction::Stop));
        assert_eq!(CardActionHandler::parse_card_action("act:/new"), Some(CardAction::New));
        assert_eq!(CardActionHandler::parse_card_action("act:/bind 5"), Some(CardAction::Bind(5)));
        assert_eq!(CardActionHandler::parse_card_action("act:/runtodo 10"), Some(CardAction::RunTodo(10)));
        assert_eq!(CardActionHandler::parse_card_action("act:/runloop 20"), Some(CardAction::RunLoop(20)));
        assert_eq!(
            CardActionHandler::parse_card_action("act:/push result_only"),
            Some(CardAction::Push("result_only".to_string()))
        );
        // 缺参数 → None
        assert_eq!(CardActionHandler::parse_card_action("act:/bind"), None);
        assert_eq!(CardActionHandler::parse_card_action("act:/runtodo"), None);
        assert_eq!(CardActionHandler::parse_card_action("act:/push"), None);
        // 非 i64 参数 / 未知 verb / 非 act 前缀 → None
        assert_eq!(CardActionHandler::parse_card_action("act:/bind abc"), None);
        assert_eq!(CardActionHandler::parse_card_action("act:/unknown"), None);
        assert_eq!(CardActionHandler::parse_card_action("nav:/help task"), None);
        assert_eq!(CardActionHandler::parse_card_action("cmd:/new"), None);
    }

    #[test]
    pub(crate) fn test_resolve_receive_target_group_vs_private() {
        use crate::feishu::ChannelMessage;
        // 群聊：channel(chat_id)非空 → 用 chat_id 作为推送目标
        let group_msg = ChannelMessage {
            id: "om1".to_string(),
            sender: "ou_user".to_string(),
            sender_type: None,
            content: "act:/stop".to_string(),
            channel: "oc_group".to_string(),
            timestamp: 0,
            chat_type: Some("card_callback".to_string()),
            mentioned_open_ids: vec![],
        };
        assert_eq!(CardActionHandler::resolve_receive_target(&group_msg), ("oc_group", "chat_id"));
        // 私聊：channel 空 → 回退到点击者 open_id
        let private_msg = ChannelMessage { channel: String::new(), ..group_msg.clone() };
        assert_eq!(CardActionHandler::resolve_receive_target(&private_msg), ("ou_user", "open_id"));
    }
}
