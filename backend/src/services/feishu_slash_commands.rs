//! 斜杠命令处理族（/new /list /stop /help /feishupush）+ help 卡片辅助（096-W3-PR4：从 feishu_listener.rs 拆分，函数体逐字搬迁零改动）。
//!
//! 设计依据：docs/design/100-FeishuListener拆分-设计.md（拆分边界与字段归属）。
//! 本族函数均为无 self 关联函数（接 context 参数对象或显式传参），
//! unit struct 仅作命名空间承载，不持有状态。

use std::sync::Arc;

use crate::db::Database;
use crate::service_context::ServiceContext;
use crate::services::feishu_api_client::FeishuApiClient;
use crate::services::feishu_card::{
    build_help_console_card, render_card, ExecutorOption, HelpCardState, LoopItem,
    ProcessItem, RecentTaskItem, TaskItem, TodoItem, WorkspaceItem, WorkspaceSummary,
};
use crate::services::feishu_card_actions::CardActionHandler;
use crate::services::feishu_listener::FeishuCommandContext;
use crate::task_manager::TaskManager;

/// 见模块头注释。unit struct：仅作命名空间（函数全部关联函数形态）。
pub(crate) struct SlashCommandHandler;

impl SlashCommandHandler {
    /// Handle /feishupush - cycle push level: disabled -> result_only -> all -> disabled.
    pub(crate) async fn handle_feishupush(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            bot_id,
            chat_type,
            sender,
            channel,
            message_id,
            reaction_id,
            ..
        } = context;
        let (receive_id, receive_id_type) = match chat_type {
            "p2p" => (sender.to_string(), "open_id"),
            _ => (channel.to_string(), "chat_id"),
        };

        let target = db.get_feishu_push_target(bot_id).await.ok().flatten();
        let current_level = target
            .as_ref()
            .map(|t| t.push_level.as_str())
            .unwrap_or("disabled");
        let new_level = match current_level {
            "disabled" => "result_only",
            "result_only" => "all",
            "all" => "disabled",
            _ => "disabled",
        };

        if let Err(e) = db.update_feishu_push_level(bot_id, new_level).await {
            tracing::error!("[feishu:{}] /feishupush update failed: {e}", bot_id);
            let msg = "⚠️ 操作失败，请稍后重试";
            FeishuApiClient::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                msg,
            )
            .await;
        } else {
            let (status_text, status_emoji) = match new_level {
                "disabled" => ("关闭", "ℹ️"),
                "result_only" => ("已切换为仅结论", "✅"),
                "all" => ("已切换为全部", "✅"),
                _ => ("未知", "⚠️"),
            };
            let msg = format!("{} 推送{}。", status_emoji, status_text);
            FeishuApiClient::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                &msg,
            )
            .await;
            tracing::info!(
                "[feishu:{}] /feishupush: push level changed to {} for bot_id={}",
                bot_id,
                new_level,
                bot_id
            );
        }

        if let Some(rid) = reaction_id {
            FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /list — list all registered project directories.
    pub(crate) async fn handle_list(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            bot_id,
            chat_type,
            sender,
            channel,
            message_id,
            reaction_id,
            ..
        } = context;
        let (receive_id, receive_id_type) = match chat_type {
            "p2p" => (sender.to_string(), "open_id"),
            _ => (channel.to_string(), "chat_id"),
        };

        let directories = db.get_project_directories().await.unwrap_or_default();
        if directories.is_empty() {
            FeishuApiClient::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                "📂 暂无已注册的项目目录。\n\n请在 Web 设置页「项目目录」中添加，或使用 /bind <名称> 绑定一个项目（首次使用会自动创建）。",
            )
            .await;
        } else {
            let mut lines: Vec<String> = directories
                .iter()
                .map(|d| {
                    let name = d.name.as_deref().unwrap_or("(未命名)");
                    format!("• {}  →  {}", name, d.path)
                })
                .collect();
            lines.insert(0, format!("📂 已注册的项目目录（共 {} 个）：", directories.len()));
            lines.push(String::new());
            lines.push("💡 使用 /bind <名称> 绑定到本项目聊天".to_string());
            FeishuApiClient::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                &lines.join("\n"),
            )
            .await;
        }

        if let Some(rid) = reaction_id {
            FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /new — start a fresh session without resuming the previous one.
    /// 全局内置斜杠命令，用于清空当前会话的 session，开启全新对话。
    ///
    /// 支持两种场景：
    /// 1. 项目绑定场景：清除绑定的 todo/loop 会话
    /// 2. 私聊默认响应执行器场景：清除默认执行器的会话
    pub(crate) async fn handle_new(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            bot_id,
            chat_type,
            sender,
            channel,
            message_id,
            reaction_id,
            ..
        } = context;
        let (receive_id, receive_id_type) = match chat_type {
            "p2p" => (sender.to_string(), "open_id"),
            _ => (channel.to_string(), "chat_id"),
        };

        // 先尝试项目绑定场景
        match db.get_feishu_project_binding(bot_id, channel).await {
            Ok(Some(binding)) => {
                // 清除 session_id 和 latest_record_id，使下一条消息无法 resume
                // should_resume 的判断依赖 latest_record.session_id.is_some()，
                // 清除后 latest_record_id=None → latest_record=None → should_resume=false
                if let Err(e) = db.clear_feishu_binding_session(binding.id).await {
                    tracing::error!("[feishu:{}] /new clear session failed: {e}", bot_id);
                    FeishuApiClient::send_text(
                        credentials,
                        token_manager,
                        bot_id,
                        &receive_id,
                        receive_id_type,
                        "⚠️ 清除会话失败，请稍后重试。",
                    )
                    .await;
                    if let Some(rid) = reaction_id {
                        FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                    }
                    return;
                }

                tracing::info!(
                    "[feishu:{}] /new command: cleared session for binding {}, next message will start fresh",
                    bot_id,
                    binding.id
                );
                FeishuApiClient::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "🆕 已开启新会话。\n\n发送你的任务，我将使用全新 session 执行，不再resume之前的对话。",
                )
                .await;

                if let Some(rid) = reaction_id {
                    FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
            Ok(None) => {
                // 没有绑定项目，尝试私聊默认响应执行器场景
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /new query binding failed: {e}", bot_id);
                FeishuApiClient::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 查询绑定失败，请稍后重试。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
        }

        // 私聊默认响应执行器场景：获取 workspace 和默认执行器配置
        let workspace_id = match db.get_agent_bot_workspace_id(bot_id).await {
            Ok(Some(wid)) => wid,
            Ok(None) => {
                tracing::warn!("[feishu:{}] /new: bot has no workspace", bot_id);
                FeishuApiClient::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 未找到工作空间，无法使用 /new。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /new query workspace failed: {e}", bot_id);
                FeishuApiClient::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 查询工作空间失败，请稍后重试。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
        };

        // 获取 workspace 设置，判断默认响应类型
        let settings = match crate::db::workspace_setting::get_workspace_settings(db, workspace_id).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                FeishuApiClient::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "📭 当前未配置默认响应，无法使用 /new。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /new query workspace settings failed: {e}", bot_id);
                FeishuApiClient::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 查询工作空间设置失败，请稍后重试。",
                )
                .await;
                if let Some(rid) = reaction_id {
                    FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
                }
                return;
            }
        };

        // 只处理 executor 类型的默认响应
        if settings.default_response_type != "executor" {
            FeishuApiClient::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                "📭 当前默认响应类型不是执行器，无法使用 /new 清空会话。",
            )
            .await;
            if let Some(rid) = reaction_id {
                FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
            }
            return;
        }

        let executor_name = settings.default_response_executor
            .unwrap_or_else(|| "claudecode".to_string());

        // 清空执行器 session：设置为 None
        if let Err(e) = db.set_executor_session(workspace_id, &executor_name, None).await {
            tracing::error!("[feishu:{}] /new clear executor session failed: {e}", bot_id);
            FeishuApiClient::send_text(
                credentials,
                token_manager,
                bot_id,
                &receive_id,
                receive_id_type,
                "⚠️ 清除执行器会话失败，请稍后重试。",
            )
            .await;
            if let Some(rid) = reaction_id {
                FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
            }
            return;
        }

        tracing::info!(
            "[feishu:{}] /new command: cleared executor session for {}, workspace={}",
            bot_id,
            executor_name,
            workspace_id
        );
        FeishuApiClient::send_text(
            credentials,
            token_manager,
            bot_id,
            &receive_id,
            receive_id_type,
            &format!("🆕 已开启新会话。\n\n下次对话将使用全新的 {} 会话，不再接续之前的对话。", executor_name),
        )
        .await;

        if let Some(rid) = reaction_id {
            FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /stop — stop the currently running execution for this binding.
    /// 与前端「停止」按钮逻辑相同：通过 task_manager 取消任务。
    pub(crate) async fn handle_stop(
        task_manager: &Arc<TaskManager>,
        context: FeishuCommandContext<'_>,
    ) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            bot_id,
            chat_type,
            sender,
            channel,
            message_id,
            reaction_id,
            ..
        } = context;
        let (receive_id, receive_id_type) = match chat_type {
            "p2p" => (sender.to_string(), "open_id"),
            _ => (channel.to_string(), "chat_id"),
        };

        match db.get_feishu_project_binding(bot_id, channel).await {
            Ok(Some(binding)) => {
                // 获取当前 binding 的最新执行记录
                if let Some(record_id) = binding.latest_record_id {
                    match db.get_execution_record(record_id).await {
                        Ok(Some(record)) => {
                            if record.status == crate::models::ExecutionStatus::Running {
                                // 任务正在运行，尝试停止
                                if let Some(ref task_id) = record.task_id {
                                    let cancelled = task_manager.cancel(task_id).await;
                                    if cancelled {
                                        tracing::info!(
                                            "[feishu:{}] /stop: cancelled task {} for record {}",
                                            bot_id,
                                            task_id,
                                            record_id
                                        );
                                        FeishuApiClient::send_text(
                                            credentials,
                                            token_manager,
                                            bot_id,
                                            &receive_id,
                                            receive_id_type,
                                            "⏹️ 已发送停止信号，任务即将终止。",
                                        )
                                        .await;
                                    } else {
                                        // 任务不在 task_manager 中（可能已崩溃），强制更新 DB
                                        tracing::warn!(
                                            "[feishu:{}] /stop: task {} not in task_manager, forcing DB update",
                                            bot_id,
                                            task_id
                                        );
                                        let _ = db.force_fail_execution_record(record_id).await;
                                        FeishuApiClient::send_text(
                                            credentials,
                                            token_manager,
                                            bot_id,
                                            &receive_id,
                                            receive_id_type,
                                            "⚠️ 任务已不在运行中（可能已异常退出），已更新状态。",
                                        )
                                        .await;
                                    }
                                } else {
                                    FeishuApiClient::send_text(
                                        credentials,
                                        token_manager,
                                        bot_id,
                                        &receive_id,
                                        receive_id_type,
                                        "⚠️ 该执行记录没有 task_id，无法停止。",
                                    )
                                    .await;
                                }
                            } else {
                                FeishuApiClient::send_text(
                                    credentials,
                                    token_manager,
                                    bot_id,
                                    &receive_id,
                                    receive_id_type,
                                    "ℹ️ 当前没有正在执行的任务。",
                                )
                                .await;
                            }
                        }
                        Ok(None) => {
                            FeishuApiClient::send_text(
                                credentials,
                                token_manager,
                                bot_id,
                                &receive_id,
                                receive_id_type,
                                "⚠️ 执行记录不存在。",
                            )
                            .await;
                        }
                        Err(e) => {
                            tracing::error!("[feishu:{}] /stop query record failed: {e}", bot_id);
                            FeishuApiClient::send_text(
                                credentials,
                                token_manager,
                                bot_id,
                                &receive_id,
                                receive_id_type,
                                "⚠️ 查询执行记录失败，请稍后重试。",
                            )
                            .await;
                        }
                    }
                } else {
                    FeishuApiClient::send_text(
                        credentials,
                        token_manager,
                        bot_id,
                        &receive_id,
                        receive_id_type,
                        "ℹ️ 当前没有执行记录可停止。",
                    )
                    .await;
                }
            }
            Ok(None) => {
                FeishuApiClient::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "📭 当前聊天未绑定任何项目，无可停止的任务。",
                )
                .await;
            }
            Err(e) => {
                tracing::error!("[feishu:{}] /stop query binding failed: {e}", bot_id);
                FeishuApiClient::send_text(
                    credentials,
                    token_manager,
                    bot_id,
                    &receive_id,
                    receive_id_type,
                    "⚠️ 查询绑定失败，请稍后重试。",
                )
                .await;
            }
        }

        if let Some(rid) = reaction_id {
            FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// Handle /help — show interactive help card with tabbed navigation.
    /// 点击 Tab 按钮会触发 card.action.trigger 事件，由飞书平台回调处理。
    pub(crate) async fn handle_help(context: FeishuCommandContext<'_>) {
        let FeishuCommandContext {
            db,
            credentials,
            token_manager,
            ctx,
            bot_id,
            chat_type: _,
            sender,
            message_id,
            content,
            reaction_id,
            ..
        } = context;

        // 解析当前分组，默认 "status"（状态页是控制台首页）
        let parsed = content.strip_prefix("/help ").unwrap_or("").trim().to_lowercase();
        let group = if parsed.is_empty() { "status".to_string() } else { parsed };

        // 状态感知控制台：查当前绑定/运行状态/推送级别/最近任务后渲染
        let state = SlashCommandHandler::assemble_help_card_state(ctx, db, bot_id, &group, 1).await;
        let card = build_help_console_card(&state);
        let card_json = render_card(&card, &format!("feishu:{}", sender));

        // 发送卡片（reply API），失败降级纯文本
        if let Err(e) = FeishuApiClient::reply_card(credentials, token_manager, bot_id, message_id, &card_json).await {
            tracing::error!("[feishu:{}] /help send card failed: {}", bot_id, e);
            FeishuApiClient::send_text(credentials, token_manager, bot_id, sender, "open_id", "📋 NTD 控制台\n\n发送 /help 打开任务控制台。").await;
        }

        if let Some(rid) = reaction_id {
            FeishuApiClient::delete_reaction(credentials, token_manager, bot_id, message_id, rid).await;
        }
    }

    /// 组装 /help 卡片状态：按 agent_bot.workspace_id 查该 workspace 的摘要/事项/环路/最近任务 + 所有工作空间。
    /// handle_help、nav 切页、act 执行后刷新都复用它（只读 db，运行状态取最近记录里的 running）。
    /// ctx 用于查询已注册的执行器列表（工作空间页渲染按钮排用）。
    pub(crate) async fn assemble_help_card_state(
        ctx: &ServiceContext,
        db: &Database,
        bot_id: i64,
        current_group: &str,
        page: usize,
    ) -> HelpCardState {
        let wid = db.get_agent_bot_workspace_id(bot_id).await.ok().flatten();
        let workspace = SlashCommandHandler::load_workspace_summary(db, wid).await;
        let push_level = SlashCommandHandler::load_push_level(db, bot_id).await;
        // 最近任务 + 运行状态都来自该 workspace 的最近执行记录
        let (recent_records, is_running) = SlashCommandHandler::recent_records_and_running(db, wid).await;
        // 104：四个列表 Tab（事项/环路/任务/工艺）统一抽成独立加载函数——
        // assemble 曾因逐 Tab 内联查询膨胀到 112 行，抽完后主干只剩「取数 + 组装结构体」，
        // 每个 loader 查询+降级+截断各自内聚，失败口径（error 日志 + 空列表降级）一致。
        let todos = SlashCommandHandler::load_todo_items(db, wid, bot_id).await;
        let loops = SlashCommandHandler::load_loop_items(db, wid).await;
        let tasks = SlashCommandHandler::load_task_items(db, wid, bot_id).await;
        let processes = SlashCommandHandler::load_process_items(db, wid, bot_id).await;
        let workspaces = SlashCommandHandler::load_workspace_items(db, wid).await;
        let available_executors =
            SlashCommandHandler::load_executor_options(ctx, workspace.as_ref().map(|w| w.executor.as_str())).await;
        HelpCardState {
            current_group: current_group.to_string(),
            workspace,
            is_running,
            push_level,
            recent_records,
            todos,
            loops,
            tasks,
            processes,
            workspaces,
            page,
            available_executors,
        }
    }

    /// 加载当前 workspace 摘要（104 从 assemble_help_card_state 抽出）：未设置 workspace → None。
    pub(crate) async fn load_workspace_summary(db: &Database, wid: Option<i64>) -> Option<WorkspaceSummary> {
        let id = wid?;
        SlashCommandHandler::build_workspace_summary(db, id).await
    }

    /// 加载推送级别（104 从 assemble_help_card_state 抽出）：无推送配置时默认 result_only，
    /// 与既有行为一致（unwrap_or 回退，DB 失败静默）。
    pub(crate) async fn load_push_level(db: &Database, bot_id: i64) -> String {
        db.get_feishu_push_target(bot_id)
            .await
            .ok()
            .flatten()
            .map(|t| t.push_level)
            .unwrap_or_else(|| "result_only".to_string())
    }

    /// 加载工作空间页列表（104 从 assemble_help_card_state 抽出）：
    /// 全量目录 + 标记当前 workspace；DB 失败静默降级空列表（既有口径，不动）。
    pub(crate) async fn load_workspace_items(db: &Database, wid: Option<i64>) -> Vec<WorkspaceItem> {
        db.get_project_directories()
            .await
            .ok()
            .unwrap_or_default()
            .into_iter()
            .map(|d| WorkspaceItem {
                name: d.name.clone().unwrap_or_else(|| d.path.clone()),
                id: d.id,
                is_current: wid == Some(d.id),
            })
            .collect()
    }

    /// 加载执行器选项排（104 从 assemble_help_card_state 抽出）：
    /// 已注册执行器 + 标记当前 workspace 配的默认执行器，供工作空间页渲染按钮排。
    /// current_executor 传空串表示无 workspace 摘要，此时全部不标记。
    pub(crate) async fn load_executor_options(
        ctx: &ServiceContext,
        current_executor: Option<&str>,
    ) -> Vec<ExecutorOption> {
        let current = current_executor.unwrap_or("");
        ctx.executor_registry
            .list_executors()
            .await
            .into_iter()
            .map(|t| {
                let name = t.as_str().to_string();
                let is_current = name == current;
                ExecutorOption { name, is_current }
            })
            .collect()
    }

    /// 当前 workspace 摘要（名 + 默认执行器）。
    pub(crate) async fn build_workspace_summary(db: &Database, workspace_id: i64) -> Option<WorkspaceSummary> {
        let name = db.get_workspace_name_by_id(workspace_id).await.ok().flatten()?;
        let executor = crate::db::workspace_setting::get_workspace_settings(db, workspace_id)
            .await
            .ok()
            .flatten()
            .and_then(|s| s.default_response_executor)
            .unwrap_or_else(|| "claudecode".to_string());
        Some(WorkspaceSummary { id: workspace_id, name, executor })
    }

    /// 该 workspace 最近 5 条执行记录 → RecentTaskItem；顺带判断是否有 running。
    pub(crate) async fn recent_records_and_running(db: &Database, wid: Option<i64>) -> (Vec<RecentTaskItem>, bool) {
        let Some(id) = wid else {
            return (vec![], false);
        };
        let Ok((records, _)) = db.get_execution_records_by_workspace(id, 5, 0).await else {
            return (vec![], false);
        };
        let is_running = records.iter().any(|r| r.status == crate::models::ExecutionStatus::Running);
        let items = records.into_iter().map(|r| SlashCommandHandler::record_to_recent_item(&r)).collect();
        (items, is_running)
    }

    /// Todo → 事项页列表项。
    /// TodoBrief → 卡片「事项列表」项（056：摘要字段足够拼状态图标，无需整行 Todo）。
    pub(crate) fn brief_to_item(t: crate::models::TodoBrief) -> TodoItem {
        use crate::models::TodoStatus;
        let status_icon = match t.status {
            TodoStatus::Completed => "✅",
            TodoStatus::Running | TodoStatus::InProgress => "▶️",
            _ => "⏸️",
        };
        TodoItem { id: t.id, title: t.title, status_icon: status_icon.to_string() }
    }

    /// LoopListRow → 环路页列表项。
    pub(crate) fn loop_to_item(l: crate::db::loop_::LoopListRow) -> LoopItem {
        LoopItem { id: l.loop_.id, name: l.loop_.name, status: l.loop_.status }
    }

    /// 加载事项页条目（104 从 assemble_help_card_state 抽出，与 tasks/processes 同口径）。
    /// 056：卡片只展示最近 20 条摘要（id+标题+状态图标），用 brief 接口替代整行全量拉取；
    /// take(20) 截断保持卡片体积。DB 失败时记 error（含 bot/ws 上下文）后降级为空列表——
    /// 卡片缺区块可接受，但静默吞错会让故障无法定位（CodeRabbit#4）。
    pub(crate) async fn load_todo_items(db: &Database, wid: Option<i64>, bot_id: i64) -> Vec<TodoItem> {
        let Some(id) = wid else {
            return vec![];
        };
        db.get_todo_briefs(Some(id), None, None)
            .await
            .map(|ts| ts.into_iter().take(20).map(SlashCommandHandler::brief_to_item).collect())
            .unwrap_or_else(|e| {
                tracing::error!("飞书卡片加载 todo 摘要失败（bot={bot_id}, ws={id}）: {e}");
                Vec::new()
            })
    }

    /// 加载环路页条目（104 从 assemble_help_card_state 抽出，与 tasks/processes 同口径）。
    /// 既有行为原样搬迁：DB 失败静默降级空列表（ok() 口径，历史如此，不动）。
    pub(crate) async fn load_loop_items(db: &Database, wid: Option<i64>) -> Vec<LoopItem> {
        let Some(id) = wid else {
            return vec![];
        };
        db.list_loops_with_counts(Some(id))
            .await
            .ok()
            .map(|ls| ls.into_iter().map(SlashCommandHandler::loop_to_item).collect())
            .unwrap_or_default()
    }

    /// 加载任务页条目（104 新增）：按 workspace 查最近 20 条（id DESC）。
    /// take(20) 截断与事项页同口径控制卡片体积；DB 失败降级空列表并记 error（对齐 todos 口径）。
    pub(crate) async fn load_task_items(db: &Database, wid: Option<i64>, bot_id: i64) -> Vec<TaskItem> {
        let Some(id) = wid else {
            return vec![];
        };
        db.list_tasks(id, None)
            .await
            .map(|ts| ts.into_iter().take(20).map(SlashCommandHandler::task_to_item).collect())
            .unwrap_or_else(|e| {
                tracing::error!("飞书卡片加载 task 列表失败（bot={bot_id}, ws={id}）: {e}");
                Vec::new()
            })
    }

    /// 加载工艺页条目（104 新增）：系统内置（workspace_id IS NULL）+ 当前 workspace 用户模板，
    /// 最多 20 条（name ASC 由 DB 排序）；安装/实例化不在卡片上提供。
    pub(crate) async fn load_process_items(db: &Database, wid: Option<i64>, bot_id: i64) -> Vec<ProcessItem> {
        db.list_process_templates(None)
            .await
            .map(|ts| {
                ts.into_iter()
                    .filter(|t| t.workspace_id.is_none() || t.workspace_id == wid)
                    .take(20)
                    .map(SlashCommandHandler::process_to_item)
                    .collect()
            })
            .unwrap_or_else(|e| {
                tracing::error!("飞书卡片加载工艺模板列表失败（bot={bot_id}）: {e}");
                Vec::new()
            })
    }

    /// tasks::Model → 任务页列表项（104 新增）。
    /// 状态图标与前端 STATUS_COLOR 四态对齐；runnable 只认「环路模式 + 有 loop_id」，
    /// 委派任务无环路可触，卡片上不提供执行按钮。
    pub(crate) fn task_to_item(t: crate::db::entity::tasks::Model) -> TaskItem {
        let status_icon = match t.status.as_str() {
            "pending" => "⏸️",
            "running" => "⏳",
            "success" => "✅",
            "failed" => "❌",
            _ => "⏸️", // 未知状态回退「未开始」图标，卡片展示不出错优先于猜错
        };
        // 方式标签：委派模式把处理人拼进标签，用户一眼能看到任务交给谁在跑
        let mode_label = match t.execution_mode.as_str() {
            "loop" => "环路".to_string(),
            "delegate" => format!("委派@{}", t.assignee_name.as_deref().unwrap_or("未指定")),
            other => other.to_string(),
        };
        TaskItem {
            id: t.id,
            title: t.title,
            status_icon: status_icon.to_string(),
            mode_label,
            runnable: t.execution_mode == "loop" && t.loop_id.is_some(),
        }
    }

    /// process_templates::Model → 工艺页列表项（104 新增）。
    /// meta 拼「复杂度 · v版本 · 系统/用户」，复杂度中文标签与前端 COMPLEXITY_LABEL 对齐。
    pub(crate) fn process_to_item(t: crate::db::entity::process_templates::Model) -> ProcessItem {
        let complexity = match t.complexity.as_str() {
            "light" => "轻量",
            "standard" => "标准",
            "complex" => "复杂",
            other => other, // 未知复杂度展示原值，比截断更真实
        };
        // 系统内置（workspace_id IS NULL）与用户模板用不同标识，方便用户区分可编辑性
        let source = if t.is_system { "系统" } else { "用户" };
        ProcessItem {
            display_name: t.display_name,
            meta: format!("{complexity} · v{} · {source}", t.version),
        }
    }

    /// ExecutionRecord → 卡片「最近任务」项（状态 emoji + 标题 + 时间）。
    pub(crate) fn record_to_recent_item(r: &crate::models::ExecutionRecord) -> RecentTaskItem {
        use crate::models::ExecutionStatus;
        let status_icon = match r.status {
            ExecutionStatus::Success => "✅",
            ExecutionStatus::Running => "⏳",
            ExecutionStatus::Failed => "❌",
        };
        // 标题优先用触发源标题，其次结果文本，最后命令
        let title = r.source_todo_title.clone().or(r.result.clone()).unwrap_or_else(|| r.command.clone());
        RecentTaskItem {
            status_icon: status_icon.to_string(),
            title,
            time_desc: CardActionHandler::format_record_time(&r.started_at),
        }
    }
}

#[cfg(test)]
// 注：本模块测试体不使用 unwrap/expect（用断言宏），无需 lint 豁免——
// 曾带冗余 allow 被评审指出，删除（13-禁止清单：allow 仅在确有需要时使用）。
mod tests {
    use super::SlashCommandHandler;
    use crate::db::entity::process_templates;
    use crate::db::entity::tasks;

    /// 造一个字段齐全的 tasks::Model，测试里改关键字段即可覆盖各分支。
    fn task_fixture() -> tasks::Model {
        tasks::Model {
            id: 1,
            title: "测试任务".to_string(),
            description: "desc".to_string(),
            status: "pending".to_string(),
            workspace_id: Some(1),
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

    /// 环路模式 + 有 loop_id：runnable=true，标签「环路」，状态图标按 status 映射。
    #[test]
    fn test_task_to_item_loop_mode_runnable() {
        let item = SlashCommandHandler::task_to_item(task_fixture());
        assert_eq!(item.id, 1);
        assert_eq!(item.mode_label, "环路");
        assert!(item.runnable, "环路模式且有 loop_id 应可执行");
        assert_eq!(item.status_icon, "⏸️", "pending 应映射 ⏸️");
    }

    /// 环路模式但缺 loop_id：不可执行（无环路可触）。
    #[test]
    fn test_task_to_item_loop_mode_without_loop_id_not_runnable() {
        let mut t = task_fixture();
        t.loop_id = None;
        let item = SlashCommandHandler::task_to_item(t);
        assert!(!item.runnable, "缺 loop_id 的环路任务不可执行");
    }

    /// 委派模式：不可执行，标签带处理人「委派@张三」。
    #[test]
    fn test_task_to_item_delegate_mode_label() {
        let mut t = task_fixture();
        t.execution_mode = "delegate".to_string();
        t.assignee_name = Some("张三".to_string());
        let item = SlashCommandHandler::task_to_item(t);
        assert!(!item.runnable, "委派任务卡片上不可执行");
        assert_eq!(item.mode_label, "委派@张三");
    }

    /// 委派模式缺处理人：标签兜底「委派@未指定」，不 panic。
    #[test]
    fn test_task_to_item_delegate_missing_assignee_fallback() {
        let mut t = task_fixture();
        t.execution_mode = "delegate".to_string();
        t.assignee_name = None;
        let item = SlashCommandHandler::task_to_item(t);
        assert_eq!(item.mode_label, "委派@未指定");
    }

    /// 状态图标四态映射 + 未知状态回退 ⏸️。
    #[test]
    fn test_task_to_item_status_icons() {
        for (status, icon) in [("running", "⏳"), ("success", "✅"), ("failed", "❌"), ("weird", "⏸️")] {
            let mut t = task_fixture();
            t.status = status.to_string();
            let item = SlashCommandHandler::task_to_item(t);
            assert_eq!(item.status_icon, icon, "status={status} 图标应映射 {icon}");
        }
    }

    /// 工艺项：系统模板 meta 含「标准 · v1.0.0 · 系统」。
    #[test]
    fn test_process_to_item_system_template() {
        let t = process_templates::Model {
            id: 1,
            guid: "g1".to_string(),
            name: "delivery".to_string(),
            display_name: "标准需求交付工艺".to_string(),
            description: "".to_string(),
            category: "software".to_string(),
            complexity: "standard".to_string(),
            version: "1.0.0".to_string(),
            source_path: None,
            workspace_id: None,
            is_system: true,
            previous_version_id: None,
            created_at: None,
            updated_at: None,
        };
        let item = SlashCommandHandler::process_to_item(t);
        assert_eq!(item.display_name, "标准需求交付工艺");
        assert_eq!(item.meta, "标准 · v1.0.0 · 系统");
    }

    /// 工艺项：用户模板标识「用户」，未知复杂度展示原值不截断。
    #[test]
    fn test_process_to_item_user_template_unknown_complexity() {
        let t = process_templates::Model {
            id: 2,
            guid: "g2".to_string(),
            name: "custom".to_string(),
            display_name: "自定义流程".to_string(),
            description: "".to_string(),
            category: "migration".to_string(),
            complexity: "weird".to_string(),
            version: "0.2".to_string(),
            source_path: None,
            workspace_id: Some(1),
            is_system: false,
            previous_version_id: None,
            created_at: None,
            updated_at: None,
        };
        let item = SlashCommandHandler::process_to_item(t);
        assert_eq!(item.meta, "weird · v0.2 · 用户");
    }
}
