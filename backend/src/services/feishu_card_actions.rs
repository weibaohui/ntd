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
        // + 自定义规则/空间管家 route_slash_or_butler），与用户在会话里手动发送该命令效果一致。
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
        // nav:/todos|loops|tasks|processes <page> - 四个列表 Tab 的通用分页（每页 10）。
        // 与 nav:/help 分开：help 是无页码切 Tab，这里带页码重查后 patch；
        // tasks/processes 是 104 新增，与 todos/loops 同形态，共用 parse_nav_page 解析。
        if let Some((group, page)) = CardActionHandler::parse_nav_page(action) {
            let state = SlashCommandHandler::assemble_help_card_state(context.ctx, context.db, context.bot_id, group, page).await;
            let card = build_help_console_card(&state);
            CardActionHandler::patch_rendered_card(context, msg, &card).await;
            return true;
        }
        false
    }

    /// 把 nav 分页动作解析为 (Tab group, page)。纯函数便于单测。
    /// 覆盖 todos/loops/tasks/processes 四个列表 Tab；页码非法（非数字/0/负）统一回退第 1 页。
    pub(crate) fn parse_nav_page(action: &str) -> Option<(&'static str, usize)> {
        // 前缀 → Tab key 映射；按前缀匹配避免与 nav:/help、nav:/history 分支冲突
        let candidates: [(&str, &'static str); 4] = [
            ("nav:/todos", "todo"),
            ("nav:/loops", "loop"),
            ("nav:/tasks", "task"),
            ("nav:/processes", "process"),
        ];
        let (prefix, group) = candidates.into_iter().find(|(p, _)| action.starts_with(p))?;
        // strip_prefix 在前缀已命中时恒为 Some，unwrap_or 仅作防御；空余部/非数字 → 第 1 页
        let page = action
            .strip_prefix(prefix)
            .unwrap_or("")
            .trim()
            .parse::<usize>()
            .unwrap_or(1)
            .max(1);
        Some((group, page))
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
            CardAction::RunTask(task_id) => CardActionHandler::act_run_task(context, msg, *task_id).await,
            CardAction::SetButlerExecutor(name) => CardActionHandler::act_set_butler_executor(context, name).await,
        }
    }

    /// act 动作执行后刷新到的目标 Tab。
    pub(crate) fn action_target_group(action: &CardAction) -> &'static str {
        match action {
            CardAction::Bind(_) | CardAction::Push(_) | CardAction::SetButlerExecutor(_) => "workspace",
            CardAction::RunTodo(_) => "todo",
            CardAction::RunLoop(_) => "loop",
            CardAction::RunTask(_) => "task",
            _ => "status",
        }
    }

    /// bot 所属 workspace 的管家执行器（如 dev 的 pi）。
    /// 返回 None 表示未配置管家（含空串），调用方按「管家不可用」处理。
    pub(crate) async fn workspace_butler_executor(db: &Database, bot_id: i64) -> Option<String> {
        let wid = db.get_agent_bot_workspace_id(bot_id).await.ok().flatten()?;
        let settings = crate::db::workspace_setting::get_workspace_settings(db, wid).await.ok().flatten()?;
        settings.butler_executor.filter(|e| !e.is_empty())
    }

    /// 设置推送级别（直接设值，不走 /feishupush 循环）。
    pub(crate) async fn act_push(context: &ListenerMessageContext<'_>, level: &str) -> ActionOutcome {
        match context.db.update_feishu_push_level(context.bot_id, level).await {
            Ok(_) => ActionOutcome { success: true, message: format!("推送级别已更新为 {level}") },
            Err(e) => ActionOutcome { success: false, message: format!("设置失败：{e}") },
        }
    }

    /// 开启新会话：清当前 workspace 管家执行器的 session（108：session 键是 (workspace, 执行器)，
    /// 未配置管家时清兜底 claudecode 的 session——清了不存在的 session 无害）。
    pub(crate) async fn act_new(context: &ListenerMessageContext<'_>) -> ActionOutcome {
        let Some(wid) = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten() else {
            return ActionOutcome { success: false, message: "未设置工作空间".to_string() };
        };
        let executor = CardActionHandler::workspace_butler_executor(context.db, context.bot_id)
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

    /// 切换工作空间：级联清旧 binding + 改 agent_bot.workspace_id。
    /// 108 起不再 auto-seed 默认响应——管家配置属于工作空间自身数据，切换不触碰。
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

    /// 触发任务再执行（104 新增）：复用任务的 requirement 描述，经任务绑定的环路重跑一次。
    /// 仅环路模式任务可执行——委派任务走讨论区 @处理人 接力，卡片不提供触发入口。
    /// 校验顺序对齐设计 §3.6：workspace → 任务实体 → 可执行校验 → runner → 触发 → 绑定。
    pub(crate) async fn act_run_task(context: &ListenerMessageContext<'_>, msg: &ChannelMessage, task_id: i64) -> ActionOutcome {
        // 先拿 workspace：区分「未设置」与「查询失败」，避免把 DB 错误伪装成归属错误
        let workspace_id = match CardActionHandler::bot_workspace_id(context).await {
            Ok(Some(id)) => id,
            Ok(None) => return ActionOutcome { success: false, message: "未设置工作空间".to_string() },
            Err(e) => return ActionOutcome { success: false, message: format!("查询工作空间失败：{e}") },
        };
        // 任务不存在与 DB 失败分开报：后者不该让用户误以为任务被删了
        let task = match context.db.get_task(task_id).await {
            Ok(Some(t)) => t,
            Ok(None) => return ActionOutcome { success: false, message: format!("任务 #{task_id} 不存在") },
            Err(e) => return ActionOutcome { success: false, message: format!("查询任务失败：{e}") },
        };
        // 三重校验抽成纯函数：可单测，也让 act_run_task 主干保持线性
        let loop_id = match CardActionHandler::validate_task_runnable(&task, workspace_id) {
            Ok(id) => id,
            Err(msg) => return ActionOutcome { success: false, message: msg },
        };
        let Some(runner) = context.debounce.loop_runner() else {
            return ActionOutcome { success: false, message: "环路执行器未就绪".to_string() };
        };
        let (receive_id, receive_id_type) = CardActionHandler::resolve_receive_target(msg);
        // meta 口径与 Web 端 dispatch_manual_with_meta 一致（requirement + source），
        // 飞书层没有 loop_trigger_dispatcher，LoopRunner::spawn_run 是等价触发路径。
        let meta = serde_json::json!({ "requirement": task.description, "source": "task" });
        let exec_id = runner.clone().spawn_run(
            loop_id,
            None,
            "feishu_card",
            meta,
            Some(context.bot_id),
            Some(receive_id.to_string()),
            Some(receive_id_type.to_string()),
        );
        // spawn_run 失败返回 -1：此时根本没有可绑定的执行，必须报失败，不能假成功
        if exec_id < 0 {
            return ActionOutcome {
                success: false,
                message: format!("创建环路执行失败（任务「{}」）", task.title),
            };
        }
        // 把新执行绑定回任务，让任务详情页的执行记录能挂到该任务；
        // 绑定失败只 warn——执行已触发，回滚执行比留一条未绑定的记录代价更大（需求 §10）。
        if let Err(e) = context.db.update_loop_execution_task_id(exec_id, task_id).await {
            tracing::warn!(
                "[feishu:{}] 绑定 loop_execution {} 到任务 {} 失败: {}",
                context.bot_id, exec_id, task_id, e
            );
        }
        let title = task.title;
        ActionOutcome { success: true, message: format!("已触发任务「{title}」") }
    }

    /// 查询 bot 当前绑定 workspace，把 DB 错误与「未绑定」区分开交给调用方提示（104 新增）。
    pub(crate) async fn bot_workspace_id(context: &ListenerMessageContext<'_>) -> Result<Option<i64>, String> {
        context
            .db
            .get_agent_bot_workspace_id(context.bot_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// 校验任务能否从卡片触发再执行（104 新增）。纯函数便于单测。
    /// 返回 Ok(loop_id) 仅当：任务属于当前 workspace、execution_mode=loop、且关联环路。
    pub(crate) fn validate_task_runnable(task: &crate::db::entity::tasks::Model, workspace_id: i64) -> Result<i64, String> {
        // 归属校验防旧卡片跨 workspace 触发（与 act_run_todo/act_run_loop 同口径）
        if task.workspace_id != Some(workspace_id) {
            return Err(format!("任务 #{} 不属于当前工作空间，无法执行", task.id));
        }
        // 只允许环路模式执行：委派任务没有卡片侧可触发的执行路径
        if task.execution_mode != "loop" {
            return Err(format!("任务 #{} 是委派任务，请到 Web 端讨论区执行", task.id));
        }
        match task.loop_id {
            Some(id) => Ok(id),
            None => Err(format!("任务 #{} 未关联环路，无法执行", task.id)),
        }
    }

    /// 把当前 workspace 的「管家执行器」设为指定 executor（108）。
    /// 只写 workspace_settings.butler_executor 单字段——默认响应机制已退役，
    /// 管家执行器是唯一消费该字段的通路，不再有 type 联动。
    /// executor 名必须是已注册的（ExecutorType::as_str），否则视为无效拒绝写入。
    pub(crate) async fn act_set_butler_executor(context: &ListenerMessageContext<'_>, executor_name: &str) -> ActionOutcome {
        let Some(wid) = context.db.get_agent_bot_workspace_id(context.bot_id).await.ok().flatten() else {
            return ActionOutcome { success: false, message: "未设置工作空间".to_string() };
        };
        // 校验 executor 已注册，避免把无效名写进 settings 让管家通路下次执行失败
        let registered: Vec<String> = CardActionHandler::registered_executor_names(context).await;
        if !registered.iter().any(|s| s == executor_name) {
            return ActionOutcome {
                success: false,
                message: format!("执行器 {executor_name} 未注册（可用：{}）", registered.join(", ")),
            };
        }
        // 管家专家 / 共识 prompt 传 None 不动旧值——本动作只管执行器一个关注点。
        match crate::db::workspace_setting::upsert_workspace_settings(
            context.db,
            wid,
            None,
            Some(executor_name.to_string()),
            None,
        )
        .await
        {
            Ok(_) => ActionOutcome { success: true, message: format!("管家执行器已设为 {executor_name}") },
            Err(e) => ActionOutcome { success: false, message: format!("设置失败：{e}") },
        }
    }

    /// 从 executor_registry 拉出所有已注册执行器名（ExecutorType::as_str），返回 Vec<String>。
    /// 抽出来一是让 act_set_butler_executor 更短，二是注册名查询本身也是可复用的小步骤。
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
            // "setexecutor" 是 108 前的旧 verb：历史控制台卡片上的按钮仍可点，
            // 别名保留一代，避免旧卡片按钮变死按钮。
            "setbutlerexecutor" | "setexecutor" => CardAction::SetButlerExecutor(arg?.to_string()),
            "bind" => CardAction::Bind(arg?.parse().ok()?),
            "runtodo" => CardAction::RunTodo(arg?.parse().ok()?),
            "runloop" => CardAction::RunLoop(arg?.parse().ok()?),
            "runtask" => CardAction::RunTask(arg?.parse().ok()?),
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
        // 各 verb 正常解析（bind/runtodo/runloop/runtask 参数是 i64）
        assert_eq!(CardActionHandler::parse_card_action("act:/stop"), Some(CardAction::Stop));
        assert_eq!(CardActionHandler::parse_card_action("act:/new"), Some(CardAction::New));
        assert_eq!(CardActionHandler::parse_card_action("act:/bind 5"), Some(CardAction::Bind(5)));
        assert_eq!(CardActionHandler::parse_card_action("act:/runtodo 10"), Some(CardAction::RunTodo(10)));
        assert_eq!(CardActionHandler::parse_card_action("act:/runloop 20"), Some(CardAction::RunLoop(20)));
        assert_eq!(CardActionHandler::parse_card_action("act:/runtask 30"), Some(CardAction::RunTask(30)));
        assert_eq!(
            CardActionHandler::parse_card_action("act:/push result_only"),
            Some(CardAction::Push("result_only".to_string()))
        );
        // 管家执行器（108）：新 verb 与旧 verb 别名都要解析为 SetButlerExecutor，
        // 旧别名保证历史控制台卡片上的按钮不失效
        assert_eq!(
            CardActionHandler::parse_card_action("act:/setbutlerexecutor pi"),
            Some(CardAction::SetButlerExecutor("pi".to_string()))
        );
        assert_eq!(
            CardActionHandler::parse_card_action("act:/setexecutor pi"),
            Some(CardAction::SetButlerExecutor("pi".to_string()))
        );
        // 缺参数 → None
        assert_eq!(CardActionHandler::parse_card_action("act:/bind"), None);
        assert_eq!(CardActionHandler::parse_card_action("act:/runtodo"), None);
        assert_eq!(CardActionHandler::parse_card_action("act:/runtask"), None);
        assert_eq!(CardActionHandler::parse_card_action("act:/push"), None);
        // 非 i64 参数 / 未知 verb / 非 act 前缀 → None
        assert_eq!(CardActionHandler::parse_card_action("act:/bind abc"), None);
        assert_eq!(CardActionHandler::parse_card_action("act:/runtask abc"), None);
        assert_eq!(CardActionHandler::parse_card_action("act:/unknown"), None);
        assert_eq!(CardActionHandler::parse_card_action("nav:/help task"), None);
        assert_eq!(CardActionHandler::parse_card_action("cmd:/new"), None);
    }

    /// 动作 → 刷新目标 Tab：RunTask 回任务页，与 RunTodo/RunLoop 口径一致。
    #[test]
    pub(crate) fn test_action_target_group_runtask_returns_task() {
        use super::CardAction;
        assert_eq!(CardActionHandler::action_target_group(&CardAction::RunTask(30)), "task");
        assert_eq!(CardActionHandler::action_target_group(&CardAction::RunTodo(10)), "todo");
        assert_eq!(CardActionHandler::action_target_group(&CardAction::RunLoop(20)), "loop");
        assert_eq!(CardActionHandler::action_target_group(&CardAction::Stop), "status");
    }

    /// nav 分页解析：四个列表 Tab 的正常页码与 Tab key 映射。
    #[test]
    pub(crate) fn test_parse_nav_page_four_kinds() {
        assert_eq!(CardActionHandler::parse_nav_page("nav:/todos 2"), Some(("todo", 2)));
        assert_eq!(CardActionHandler::parse_nav_page("nav:/loops 3"), Some(("loop", 3)));
        assert_eq!(CardActionHandler::parse_nav_page("nav:/tasks 4"), Some(("task", 4)));
        assert_eq!(CardActionHandler::parse_nav_page("nav:/processes 5"), Some(("process", 5)));
    }

    /// nav 分页解析：缺页码/非数字/0/负数统一回退第 1 页，未知前缀返回 None。
    #[test]
    pub(crate) fn test_parse_nav_page_fallbacks() {
        assert_eq!(CardActionHandler::parse_nav_page("nav:/tasks"), Some(("task", 1)));
        assert_eq!(CardActionHandler::parse_nav_page("nav:/tasks abc"), Some(("task", 1)));
        assert_eq!(CardActionHandler::parse_nav_page("nav:/processes 0"), Some(("process", 1)));
        assert_eq!(CardActionHandler::parse_nav_page("nav:/help task"), None);
        assert_eq!(CardActionHandler::parse_nav_page("nav:/history 2"), None);
        assert_eq!(CardActionHandler::parse_nav_page("act:/runtask 1"), None);
    }

    /// 任务可执行校验测试夹具：workspace=7 的环路任务（loop_id=10），
    /// 各用例只改关心的字段——避免每个分支重复构造 16 个字段的全量 Model。
    fn validate_task_fixture() -> crate::db::entity::tasks::Model {
        crate::db::entity::tasks::Model {
            id: 1,
            title: "t".to_string(),
            description: "d".to_string(),
            status: "pending".to_string(),
            workspace_id: Some(7),
            template_id: None,
            loop_id: Some(10),
            created_by: "u".to_string(),
            created_at: None,
            updated_at: None,
            execution_mode: "loop".to_string(),
            assignee_kind: None,
            assignee_name: None,
            auto_continue: 0,
            continue_rounds: 0,
            delegate_max_rounds: None,
        }
    }

    /// 环路模式 + 归属正确 + 有 loop_id → Ok(loop_id)（正常路径）。
    #[test]
    pub(crate) fn test_validate_task_runnable_loop_mode_ok() {
        assert_eq!(CardActionHandler::validate_task_runnable(&validate_task_fixture(), 7), Ok(10));
    }

    /// 任务归属其它 workspace → 拒绝（防旧卡片跨工作空间执行）。
    /// 含 None 分支：孤儿任务（workspace_id NULL）不得因 None==None 旁路通过。
    #[test]
    pub(crate) fn test_validate_task_runnable_wrong_workspace_rejected() {
        let mut t = validate_task_fixture();
        t.workspace_id = Some(8);
        assert!(CardActionHandler::validate_task_runnable(&t, 7).is_err(), "跨 workspace 应拒绝");
        t.workspace_id = None;
        assert!(
            CardActionHandler::validate_task_runnable(&t, 7).is_err(),
            "NULL workspace 的孤儿任务不得通过 Some(wid)!=None 的归属校验"
        );
    }

    /// 委派模式任务 → 拒绝（卡片侧无委派执行路径）。
    #[test]
    pub(crate) fn test_validate_task_runnable_delegate_mode_rejected() {
        let mut t = validate_task_fixture();
        t.execution_mode = "delegate".to_string();
        assert!(CardActionHandler::validate_task_runnable(&t, 7).is_err(), "委派任务应拒绝");
    }

    /// 环路模式但未关联环路（loop_id None）→ 拒绝（无环路可触）。
    #[test]
    pub(crate) fn test_validate_task_runnable_missing_loop_id_rejected() {
        let mut t = validate_task_fixture();
        t.loop_id = None;
        assert!(CardActionHandler::validate_task_runnable(&t, 7).is_err(), "缺 loop_id 应拒绝");
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
